// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Opt-in live unit/combat runtime: the [`UnitRuntime`] resource plus the
//! scheduled `Groups.unit`/`Groups.bullet` update systems and the
//! `WaveSpawner` driver.
//!
//! Ported from `core/src/mindustry/core/Logic.java` (`updateEntities` and the
//! wave-timer/`runWave` half of `update`), `ai/WaveSpawner.java`,
//! `ai/types/GroundAI.java` (the core-approach/target subset our controller
//! framework exposes) and `entities/comp/UnitComp.java`. The live building
//! runtime (`sim::runtime`) owns placed buildings; this module owns the units,
//! bullets and the wave spawner that operate on the same ECS world.
//!
//! The runtime is opt-in: a `Sim` that never calls [`Sim::install_unit_runtime`]
//! has no unit runtime installed, so every system below is a no-op and the P0
//! schedule/checksum/golden path stays byte-identical. The schedule slots are
//! the reserved plan-05 sets (`EntitySet::UpdateUnits`/`UpdateBullets`/
//! `CollideBullets`, `TickSet::RunWave`), never reordered.
//!
//! `ContentRegistry` is `!Send` (its mod-error sink is a plain trait object), so
//! the runtime is stored as a bevy *non-send* resource (`World::insert_non_send`)
//! and the systems pull it out with plain `&mut World` signatures. The schedule's
//! single-threaded executor keeps every access on the owning thread.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use std::collections::BTreeMap;

use crate::ai::controller::{AiKind, ControllerSlot};
use crate::ai::pathfinder::{Cost, Pathfinder};
use crate::ai::types::flying::{
    FlyingAiState, circle_attack, move_to, move_to_point, update_flying,
};
use crate::ai::types::ground::{GroundAiState, pathfind_field, update_ground};
use crate::ai::types::missile::{MissileAi, MissileShooter, update_missile};
use crate::ai::types::player::PlayerInput;
use crate::ai::wave_spawner::{
    SPAWN_EFFECT_INVINCIBLE_TICKS, SPAWN_EFFECT_UNMOVING_TICKS, SpawnContext, WaveSpawner,
    flyer_edge_position,
};
use crate::audio::AudioSinkRes;
use crate::combat::bullet::{self, CombatCtx};
use crate::combat::targeting::TargetQueries;
use crate::combat::view::{FxHandle, noop_fx};
use crate::content::ctype::Mappable;
use crate::content::{BlockKind, ContentRegistry, UnitTypeId};
use crate::determinism::{Checksum, Checksummer, SimRng};
use crate::ecs::{BuildingComp, EntitySeq};
use crate::entities::comp::unit::comp::{
    HitboxComp, PhysicsComp, TimedKillComp, UnitCore, UnitTypeComp,
};
use crate::entities::comp::unit::lifecycle::{spawn_unit, spawn_unit_def, sync_weapon_state};
use crate::entities::comp::unit::movement;
use crate::entities::comp::{Building, Health, Pos, TeamComp, Vel};
use crate::game::runtime::CampaignRuntime;
use crate::game::spawn_group::SpawnGroup;
use crate::game::waves::Waves;
use crate::weapons::UnitWeapons;
use crate::world::blocks::storage::core_block::CoreBuild;
use crate::world::{TilePos, WorldGrid};

use super::Sim;
use super::query::{sorted_by_seq, team_of, unit_def};
use super::runtime::{RuntimeError, load_vanilla_content};

/// First entity sequence allocated to live units/bullets.
///
/// The live building runtime allocates from `0`; keeping units well above that
/// range keeps `EntitySeq` unique across the mixed world (ordering only needs
/// uniqueness, but distinct ranges keep debug dumps readable).
pub const UNIT_SEQ_BASE: u64 = 1_000_000_000;

/// Finds the live unit whose [`EntitySeq`] equals `id` (`Groups.unit.getByID`;
/// `EntityGroup.java:42-44`). Saved unit chunks carry this id (I01 T10), so the
/// cargo loader can re-tether its unit after a load.
///
/// Units only: entities without [`UnitTypeComp`] (buildings, bullets, weather)
/// never match. Duplicate ids (possible only in legacy saves) resolve to the
/// lowest ECS entity index for determinism.
pub fn unit_by_id(world: &World, id: i32) -> Option<Entity> {
    if id < 0 {
        return None;
    }
    let target = id as u32 as u64;
    world
        .iter_entities()
        .filter(|entity| entity.get::<UnitTypeComp>().is_some())
        .filter(|entity| entity.get::<EntitySeq>().is_some_and(|seq| seq.0 == target))
        .map(|entity| entity.id())
        .min_by_key(|entity| entity.index())
}

mod ai;
mod ai_weapons;
mod combat_glue;
mod command_queue;
mod deaths;
mod debug;
mod install;
#[cfg(all(test, feature = "msav-import"))]
mod markers_region_tests;
#[cfg(all(test, feature = "msav-import"))]
mod msav_tests;
mod payload_cmds;
mod payload_units;
mod physics;
mod player;
mod systems;
mod targeting;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod unit_payload_tests;
mod update;
mod waves;

pub use payload_units::{
    create_unit_payload, dump_carried_unit, spawn_unit_payload, unit_payload_can_dump,
};
// Only `sim::combat_tests` names this by the old path, so gate the
// re-export to test builds (RULES 3.3) and keep the lib build warning-free.
pub(crate) use ai_weapons::{control_weapons_for, update_ai_weapons_for};
#[cfg(test)]
pub(crate) use deaths::building_explosion_terms;
pub use physics::unit_physics_system;
pub use systems::{
    bullet_collide_system, bullet_update_system, enemy_count_system, turret_update_system,
};
pub(crate) use targeting::{
    closest_core_of_team, closest_enemy_core, invalid_target, invalidate_target_entity,
    target_hit_size_of,
};
pub use update::unit_update_system;
pub use waves::run_wave_system;

/// World pixels per tile (`config::TILESIZE`).
const TILE_SIZE: f32 = crate::config::TILESIZE as f32;

/// Dead flying unit in its wreck fall (`UnitComp.killed`; UnitComp.java:980-991):
/// the unit keeps existing with `Health.dead` until it crashes (grounded or
/// overkilled), then `resolve_deaths` runs the normal destroy path
/// (`UnitComp.update` dead branch, UnitComp.java:803-830).
#[derive(Debug, Clone, Copy, PartialEq, bevy_ecs::component::Component)]
pub struct Falling;

/// Live unit/combat runtime state attached to a [`Sim`]'s ECS world.
///
/// Owns the vanilla content snapshot the unit components need, the terrain
/// snapshot the movement/kinematics passes read, the per-team flowfield
/// pathfinder and the live unit/bullet entity lists. Stored as a non-send
/// resource because [`ContentRegistry`] is `!Send`.
/// Pre-kill snapshot for the `UnitComp` wreck decals (J03 T5).
struct WreckDecalInput {
    /// Unit type name (`<name>-wreck<i>` region prefix).
    name: String,
    /// `UnitType.hitSize`.
    hit_size: f32,
    /// `UnitType.createScorch`.
    create_scorch: bool,
    /// Unit center x.
    x: f32,
    /// Unit center y.
    y: f32,
    /// Unit rotation in degrees.
    rotation: f32,
    /// `EntitySeq.0` (deterministic view-RNG seed).
    seed: u64,
}

pub struct UnitRuntime {
    /// Content registry (unit/bullet/status/weapon defs).
    pub content: ContentRegistry,
    /// Terrain snapshot (`Sim::grid`); refreshed after tile mutations.
    pub grid: WorldGrid,
    /// Per-team flowfield pathfinder (`Vars.pathfinder`).
    pub pathfinder: Pathfinder,
    /// Team the packed path tiles were built for.
    pub path_team: u8,
    /// `Vars.controlPath`: per-goal fields for `CommandAI`/`LogicAI` move
    /// orders, one instance per `ControlPathfinder.costTypes` entry indexed by
    /// [`crate::ai::control_pathfinder::ControlCost::id`] (Java keys its
    /// clusters/fields by `pathCost`). Passability is derived lazily from
    /// [`Self::pathfinder`]'s packed tiles.
    pub control_paths:
        [crate::ai::ControlPathfinder; crate::ai::control_pathfinder::CONTROL_COST_COUNT],
    /// Whether each [`Self::control_paths`] entry must re-read the path tiles.
    pub control_path_dirty: [bool; crate::ai::control_pathfinder::CONTROL_COST_COUNT],
    /// Wave team (`rules.waveTeam`) new spawns default to.
    pub wave_team: u8,
    /// Live units in spawn order.
    pub units: Vec<Entity>,
    /// Live bullets in spawn order.
    pub bullets: Vec<Entity>,
    /// Deterministic unit/combat RNG stream.
    pub rng: SimRng,
    /// Monotonic entity-sequence allocator for units/bullets.
    pub seq: u64,
    /// FX sink seam (plan 17; no-op in headless core).
    pub fx: FxHandle,
    /// `WaveSpawner` state (`spawns`, window, last wave).
    pub wave_spawner: WaveSpawner,
    /// Fallback vanilla wave table (`Waves.get()`; `SaveVersion` applies it
    /// when `Rules.spawns` is empty).
    pub waves: Waves,
    /// Wave index requested by `runWave`/the campaign timer, spawned by the
    /// `TickSet::RunWave` system.
    pub pending_wave: Option<i32>,
    /// Total units created.
    pub units_created: u64,
    /// Total units removed.
    pub units_removed: u64,
    /// Total bullets created.
    pub bullets_created: u64,
    /// Total bullets removed.
    pub bullets_removed: u64,
    /// `AvoidanceProcess` occupancy grid (`getAvoidance()`), one `i32` per
    /// tile: `max(i32::MAX - unit.id)` over every ground-layer AI unit
    /// (`AvoidanceProcess.java:52-106`). Rebuilt at the start of every tick
    /// while active; queried via `get_target_tile`'s avoidance term.
    pub avoidance: Vec<i32>,
    /// `AvoidanceProcess.active`: lazily set by the first
    /// `getTargetTile(.., avoidanceId)` request (`AvoidanceProcess.java:40-48`).
    pub avoidance_active: bool,
}

impl UnitRuntime {
    /// Builds a runtime over `grid` with `content` and a fresh pathfinder.
    ///
    /// `world` supplies the per-tile building terms of the packed path tiles
    /// (`Pathfinder.packTile`); `core_capture` mirrors `Rules.coreCapture`.
    pub fn new(
        content: ContentRegistry,
        grid: &WorldGrid,
        world: &World,
        wave_team: u8,
        core_capture: bool,
    ) -> Self {
        let mut pathfinder = Pathfinder::new(grid.width(), grid.height());
        pathfinder.rebuild(grid, &content, world, core_capture);
        Self {
            content,
            grid: grid.clone(),
            pathfinder,
            path_team: wave_team,
            control_paths: crate::ai::control_pathfinder::ControlCost::ALL.map(|cost| {
                crate::ai::ControlPathfinder::with_cost(grid.width(), grid.height(), cost)
            }),
            control_path_dirty: [true; crate::ai::control_pathfinder::CONTROL_COST_COUNT],
            wave_team,
            units: Vec::new(),
            bullets: Vec::new(),
            // Fixed seed: the runtime stream only feeds spawn/weapon jitter, so
            // the sim's own `rng_streams` seed does not need to be mirrored.
            rng: SimRng::new(1),
            seq: UNIT_SEQ_BASE,
            fx: noop_fx(),
            wave_spawner: WaveSpawner::new(),
            waves: Waves::new(),
            pending_wave: None,
            units_created: 0,
            units_removed: 0,
            bullets_created: 0,
            bullets_removed: 0,
            avoidance: Vec::new(),
            avoidance_active: false,
        }
    }

    /// Re-snapshots the terrain and rebuilds the packed path tiles.
    pub fn refresh_grid(&mut self, grid: &WorldGrid, world: &World, core_capture: bool) {
        self.grid = grid.clone();
        self.pathfinder
            .rebuild(&self.grid, &self.content, world, core_capture);
        self.control_path_dirty = [true; crate::ai::control_pathfinder::CONTROL_COST_COUNT];
    }

    /// The control pathfinder for `cost` with passability current to the last
    /// [`Self::refresh_grid`] (`ControlPathfinder` world-update hook).
    pub fn control_path(
        &mut self,
        cost: crate::ai::control_pathfinder::ControlCost,
    ) -> &mut crate::ai::ControlPathfinder {
        let index = usize::from(cost.id());
        if self.control_path_dirty[index] {
            let team = self.path_team;
            self.control_paths[index].cost = cost;
            self.control_paths[index].rebuild_passability(
                self.pathfinder.width,
                self.pathfinder.height,
                &self.pathfinder.tiles,
                team,
            );
            self.control_path_dirty[index] = false;
        }
        &mut self.control_paths[index]
    }

    /// Clears transient match state after `Logic.reset` (`Groups.clear`).
    pub fn reset(&mut self, grid: &WorldGrid, world: &World, core_capture: bool) {
        self.units.clear();
        self.bullets.clear();
        self.pending_wave = None;
        self.wave_spawner = WaveSpawner::new();
        self.rng = SimRng::new(1);
        self.seq = UNIT_SEQ_BASE;
        self.units_created = 0;
        self.units_removed = 0;
        self.bullets_created = 0;
        self.bullets_removed = 0;
        self.refresh_grid(grid, world, core_capture);
    }

    /// Live unit count (entity-validated).
    pub fn unit_count(&self, world: &World) -> usize {
        self.units
            .iter()
            .filter(|entity| world.get_entity(**entity).is_ok())
            .count()
    }

    /// Live bullet count (entity-validated).
    pub fn bullet_count(&self, world: &World) -> usize {
        self.bullets
            .iter()
            .filter(|entity| world.get_entity(**entity).is_ok())
            .count()
    }

    /// Spawns a unit by content name (`UnitType.spawn`) and tracks it.
    pub fn spawn(
        &mut self,
        world: &mut World,
        name: &str,
        team: u8,
        x: f32,
        y: f32,
        rotation: f32,
    ) -> Option<Entity> {
        let seq = self.seq;
        let entity = spawn_unit(world, &self.content, seq, name, team, x, y, rotation)?;
        self.seq = self.seq.wrapping_add(1);
        self.units_created += 1;
        self.units.push(entity);
        Some(entity)
    }

    /// Spawns a unit by resolved content id (the `SimCommand::SpawnUnit` path).
    pub fn spawn_by_id(
        &mut self,
        world: &mut World,
        id: UnitTypeId,
        team: u8,
        x: f32,
        y: f32,
        rotation: f32,
    ) -> Option<Entity> {
        let def = self.content.unit(id)?;
        let seq = self.seq;
        let entity = spawn_unit_def(world, seq, def, team, x, y, rotation);
        self.seq = self.seq.wrapping_add(1);
        self.units_created += 1;
        self.units.push(entity);
        Some(entity)
    }

    /// Deterministic digest of the live unit/bullet state (ascending seq).
    ///
    /// Kept separate from [`Sim::checksum`]: the P0 stream stays byte-identical
    /// and this is the runtime's own determinism/desync gate.
    pub fn checksum(&self, world: &World) -> Checksum {
        let mut c = Checksummer::new();
        c.part(&self.grid.width());
        c.part(&self.grid.height());
        let units = sorted_by_seq(
            self.units
                .iter()
                .copied()
                .filter(|entity| world.get_entity(*entity).is_ok())
                .map(|entity| {
                    (
                        world
                            .get::<EntitySeq>(entity)
                            .map(|seq| seq.0)
                            .unwrap_or(u64::MAX),
                        entity,
                    )
                }),
        );
        for entity in units {
            c.part(
                &world
                    .get::<EntitySeq>(entity)
                    .map(|seq| seq.0)
                    .unwrap_or(u64::MAX),
            );
            if let Some(pos) = world.get::<Pos>(entity) {
                c.part(&pos.x);
                c.part(&pos.y);
            }
            if let Some(core) = world.get::<UnitCore>(entity) {
                c.part(&core.rotation);
                c.part(&u8::from(core.dead));
            }
            if let Some(health) = world.get::<Health>(entity) {
                c.part(&health.health);
            }
            if let Some(team) = team_of(world, entity) {
                c.part(&team);
            }
        }
        let bullets = sorted_by_seq(
            self.bullets
                .iter()
                .copied()
                .filter(|entity| world.get_entity(*entity).is_ok())
                .map(|entity| {
                    (
                        world
                            .get::<EntitySeq>(entity)
                            .map(|seq| seq.0)
                            .unwrap_or(u64::MAX),
                        entity,
                    )
                }),
        );
        for entity in bullets {
            c.part(
                &world
                    .get::<EntitySeq>(entity)
                    .map(|seq| seq.0)
                    .unwrap_or(u64::MAX),
            );
            if let Some(pos) = world.get::<Pos>(entity) {
                c.part(&pos.x);
                c.part(&pos.y);
            }
            if let Some(state) = world.get::<bullet::Bullet>(entity) {
                c.part(&state.time.to_bits());
                c.part(&state.def.raw());
            }
        }
        c.part(&self.pathfinder.updates);
        c.finish()
    }
}
