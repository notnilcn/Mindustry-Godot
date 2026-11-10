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
mod command_queue;
mod deaths;
#[cfg(all(test, feature = "msav-import"))]
mod markers_region_tests;
#[cfg(all(test, feature = "msav-import"))]
mod msav_tests;
mod payload_units;
mod physics;
mod targeting;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod unit_payload_tests;

use ai::defender::update_defender_ai;
use ai::flying::update_flying_ai;
use ai::follow::update_flying_follow_ai;
use ai::ground::{GroundAiRules, update_ground_ai};
use ai::hug::update_hug_ai;
use ai::{builder_falls_back, resolve_command_kind};
use ai_weapons::{
    AiWeaponCtx, AiWeaponsState, command_ai_weapon_pass, logic_ai_weapon_pass, update_ai_weapons,
};
use command_queue::pop_live_command_queue;
use deaths::has_core_of_team;
use physics::rebuild_avoidance;
use targeting::{building_center, entity_pos_within, face_target, unit_control_cost};

pub use payload_units::{
    create_unit_payload, dump_carried_unit, spawn_unit_payload, unit_payload_can_dump,
};
// Only `sim::combat_tests` names this by the old path, so gate the
// re-export to test builds (RULES 3.3) and keep the lib build warning-free.
pub(crate) use ai_weapons::{control_weapons_for, update_ai_weapons_for};
#[cfg(test)]
pub(crate) use deaths::building_explosion_terms;
pub use physics::unit_physics_system;
pub(crate) use targeting::{
    closest_core_of_team, closest_enemy_core, invalid_target, invalidate_target_entity,
    target_hit_size_of,
};

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
        let mut units: Vec<(u64, Entity)> = self
            .units
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
            })
            .collect();
        units.sort_by_key(|(seq, entity)| (*seq, entity.index()));
        for (seq, entity) in units {
            c.part(&seq);
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
            if let Some(team) = world.get::<TeamComp>(entity) {
                c.part(&team.team);
            }
        }
        let mut bullets: Vec<(u64, Entity)> = self
            .bullets
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
            })
            .collect();
        bullets.sort_by_key(|(seq, entity)| (*seq, entity.index()));
        for (seq, entity) in bullets {
            c.part(&seq);
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

impl Sim {
    /// Whether the opt-in live unit runtime is installed.
    pub fn has_unit_runtime(&self) -> bool {
        self.ecs.0.contains_non_send::<UnitRuntime>()
    }

    /// Read-only live unit runtime, if installed.
    pub fn unit_runtime(&self) -> Option<&UnitRuntime> {
        self.ecs.0.get_non_send::<UnitRuntime>()
    }

    /// Mutable live unit runtime, if installed.
    pub fn unit_runtime_mut(&mut self) -> Option<bevy_ecs::change_detection::Mut<'_, UnitRuntime>> {
        self.ecs.0.get_non_send_mut::<UnitRuntime>()
    }

    /// Installs the live unit runtime with vanilla content (idempotent).
    pub fn install_unit_runtime(&mut self) -> Result<(), RuntimeError> {
        if self.has_unit_runtime() {
            return Ok(());
        }
        let content = load_vanilla_content()?;
        self.install_unit_runtime_from(content);
        Ok(())
    }

    /// Installs the live unit runtime over a prebuilt content registry.
    pub fn install_unit_runtime_from(&mut self, content: ContentRegistry) {
        // A loaded world carries door `open` state; mirror `checkSolid` into the
        // grid before the pathfinder packs it.
        crate::world::sync_dynamic_solid(&mut self.grid, &self.ecs.0);
        // The content-backed status applier must exist before anything can
        // apply a status (`StatusApplier` resource; NEW-1): install it while
        // the registry is still owned here, then move it into the runtime.
        crate::combat::damage::status::install_status_applier(&mut self.ecs.0, &content);
        // `BuildDamageEvent` log (G01 T15): drained by the minimap seam.
        self.ecs
            .0
            .insert_resource(crate::world::events::BuildDamageLog::default());
        // `flashHit` wall-collision log (I02 T4): drained by the block pass.
        self.ecs
            .0
            .insert_resource(crate::world::events::WallFlashLog::default());
        let wave_team = crate::game::rules::Rules::default().wave_team;
        let core_capture =
            crate::game::runtime::rules_of(&self.ecs.0).is_some_and(|rules| rules.core_capture);
        let runtime = UnitRuntime::new(content, &self.grid, &self.ecs.0, wave_team, core_capture);
        self.ecs.0.insert_non_send(runtime);
    }

    /// Sets the live wave/`WaveSpawner` team (`session.waveTeam`) and rebuilds
    /// the packed path tiles when it changes.
    pub fn set_unit_wave_team(&mut self, team: u8) {
        let grid = self.grid.clone();
        let core_capture =
            crate::game::runtime::rules_of(&self.ecs.0).is_some_and(|rules| rules.core_capture);
        let Some(mut runtime) = self.ecs.0.remove_non_send::<UnitRuntime>() else {
            return;
        };
        runtime.wave_team = team;
        if runtime.path_team != team {
            runtime.path_team = team;
            runtime.refresh_grid(&grid, &self.ecs.0, core_capture);
        }
        self.ecs.0.insert_non_send(runtime);
    }

    /// Requests a wave spawn for `wave` (0-based; `Logic.runWave` calls
    /// `spawnEnemies` before incrementing `state.wave`). Re-snapshots the live
    /// grid first so spawn-overlay/terrain changes are visible to the spawner.
    pub fn request_wave_spawn(&mut self, wave: i32) {
        self.refresh_unit_runtime();
        let Some(mut runtime) = self.ecs.0.get_non_send_mut::<UnitRuntime>() else {
            return;
        };
        runtime.pending_wave = Some(wave);
    }

    /// `Logic.skipWave()` -> `runWave()` (`Logic.java:315-325`; `SpawnWaveI`
    /// with `natural = true`, `J/logic/LExecutor.java:2173-2175`) on the live
    /// session: advances the wave and queues `spawnEnemies()` for the
    /// pre-increment wave. Without a `CampaignRuntime` (sandbox/scenario)
    /// Java still runs the wave: `LogicWorldState.wave` increments and the
    /// unit runtime spawns `getSpawned(wave - 1)`.
    pub fn logic_skip_wave(&mut self) {
        self.refresh_unit_runtime();
        if let Some(mut runtime) = self
            .ecs
            .0
            .remove_resource::<crate::game::runtime::CampaignRuntime>()
        {
            {
                let mut unit_runtime = self.ecs.0.get_non_send_mut::<UnitRuntime>();
                crate::game::runtime::run_wave_now(&mut runtime, unit_runtime.as_deref_mut());
            }
            self.ecs.0.insert_resource(runtime);
            return;
        }
        let spawn_index = {
            self.ecs
                .0
                .get_resource_mut::<crate::logic::world::LogicWorldState>()
                .map(|mut state| {
                    let index = state.wave - 1;
                    state.wave += 1;
                    index
                })
        };
        if let Some(spawn_index) = spawn_index {
            self.request_wave_spawn(spawn_index);
        }
    }

    /// `SpawnWaveI` positional path (`J/logic/LExecutor.java:2177-2198`):
    /// every spawn group whose `spawn` is `-1` or the packed `(x, y)` emits
    /// `group.getSpawned(state.wave - 1)` units within `tilesize * 2` px of
    /// `World.unconv(x, y)`, facing the map center; the wave does not advance.
    pub fn logic_spawn_wave_at(&mut self, x: i32, y: i32) {
        let Some((groups, wave, wave_team)) = self
            .ecs
            .0
            .get_resource::<crate::game::runtime::CampaignRuntime>()
            .map(|campaign| {
                (
                    groups_from_rules(&campaign.session.rules),
                    campaign.session.wave,
                    campaign.session.wave_team(),
                )
            })
        else {
            return;
        };
        let packed = TilePos::new(x as i16, y as i16).pack();
        let width_px = self.grid.width() as f32 * TILE_SIZE;
        let height_px = self.grid.height() as f32 * TILE_SIZE;
        // Java `Tmp.v1.rnd(spread)` with `spread = tilesize * 2`: a random
        // direction of length 16 (same draw as `spawn_enemies`' ground branch).
        let (wx, wy) = (x as f32 * 8.0, y as f32 * 8.0);
        let Some(mut runtime) = self.ecs.0.remove_non_send::<UnitRuntime>() else {
            return;
        };
        let UnitRuntime {
            content,
            seq,
            units,
            units_created,
            rng,
            ..
        } = &mut runtime;
        let unmoving = content.status_id("unmoving");
        let invincible = content.status_id("invincible");
        let mut emitted = 0u64;
        for group in &groups {
            // `if(group.type == null) continue` (unresolvable unit type).
            if content
                .unit_id(&group.unit)
                .and_then(|id| content.unit(id))
                .is_none()
            {
                continue;
            }
            if group.spawn != -1 && group.spawn != packed {
                continue;
            }
            let count = group.get_spawned(wave - 1);
            for _ in 0..count {
                let theta = rng
                    .range(crate::determinism::RngStream::Sim, 0.0, 360.0)
                    .to_radians();
                let sx = wx + theta.cos() * 16.0;
                let sy = wy + theta.sin() * 16.0;
                let rotation = (height_px / 2.0 - sy)
                    .atan2(width_px / 2.0 - sx)
                    .to_degrees();
                let team = group.team.unwrap_or(wave_team);
                if let Some(entity) = spawn_group_unit(
                    &mut self.ecs.0,
                    content,
                    seq,
                    group,
                    team,
                    sx,
                    sy,
                    rotation,
                    wave - 1,
                    unmoving,
                    invincible,
                ) {
                    units.push(entity);
                    emitted += 1;
                }
            }
        }
        *units_created += emitted;
        self.ecs.0.insert_non_send(runtime);
    }

    /// Re-snapshots the live grid into the unit runtime (map load/tile change).
    pub fn refresh_unit_runtime(&mut self) {
        // `Tile.solid()`'s `checkSolid` term is mirrored into the grid before
        // anything packs/collides against it (`world::sync_dynamic_solid`).
        crate::world::sync_dynamic_solid(&mut self.grid, &self.ecs.0);
        let grid = self.grid.clone();
        let core_capture =
            crate::game::runtime::rules_of(&self.ecs.0).is_some_and(|rules| rules.core_capture);
        let Some(mut runtime) = self.ecs.0.remove_non_send::<UnitRuntime>() else {
            return;
        };
        runtime.refresh_grid(&grid, &self.ecs.0, core_capture);
        self.ecs.0.insert_non_send(runtime);
    }

    /// `Groups.fires`/`Groups.puddles` update half of `Logic.updateEntities`:
    /// advances every live fire and puddle one tick against the authoritative
    /// tile grid (`Sim.grid`; the runtime snapshot is not on this path).
    ///
    /// `water_attr` is the active weathers' water attribute
    /// (`weather_water_attr`; `0.0` with no weather).
    pub fn update_fires_and_puddles(&mut self) {
        let Some(mut runtime) = self.ecs.0.remove_non_send::<UnitRuntime>() else {
            return;
        };
        let fx = runtime.fx.clone();
        // `state.envAttrs.get(Attribute.water)` from the active weathers
        // (PAR-0127); `0.0` when no weather is running.
        let water_attr = crate::game::runtime::weather_water_attr(&self.ecs.0, &runtime.content);
        let fireballs = crate::combat::fires::update_fires(
            &mut self.ecs.0,
            &mut self.grid.tiles,
            &runtime.content,
            &mut runtime.rng,
            fx.as_ref(),
            water_attr,
        );
        crate::combat::puddles::update_puddles(
            &mut self.ecs.0,
            &mut self.grid.tiles,
            &runtime.content,
            &mut runtime.rng,
            fx.as_ref(),
        );
        self.ecs.0.insert_non_send(runtime);
        // `FireComp.update` fireballs (`Bullets.fireball.createNet`, team
        // derelict, random angle, type damage) plus the puddle-reaction
        // fireballs collected by `Puddles.reactPuddle`.
        let pending_puddle_fireballs = self
            .ecs
            .0
            .get_resource_mut::<crate::combat::puddles::PendingFireballs>()
            .map(|mut pending| std::mem::take(&mut pending.0))
            .unwrap_or_default();
        if !fireballs.is_empty() || !pending_puddle_fireballs.is_empty() {
            self.with_combat_ctx(|ctx| {
                for (x, y) in fireballs.into_iter().chain(pending_puddle_fireballs) {
                    let angle = ctx
                        .rng
                        .range(crate::determinism::RngStream::Sim, 0.0, 360.0);
                    let _ = ctx.spawn(&crate::combat::bullet::BulletSpawn {
                        def: crate::content::registries::bullets::FIREBALL,
                        team: 0,
                        x,
                        y,
                        angle,
                        damage: -1.0,
                        ..crate::combat::bullet::BulletSpawn::default()
                    });
                }
            });
        }
        // H09 T7: due `Damage.dynamicExplosion` delayed actions (lightning,
        // fireballs, staggered waves).
        self.drain_delayed_explosions();
    }

    /// Runs `f` with a live [`crate::combat::bullet::CombatCtx`] (unit runtime
    /// + audio sink).
    ///
    /// Bullets spawned through the context join `UnitRuntime.bullets`; `None`
    /// without a unit runtime.
    pub(crate) fn with_combat_ctx<R>(
        &mut self,
        f: impl FnOnce(&mut crate::combat::bullet::CombatCtx<'_>) -> R,
    ) -> Option<R> {
        let mut runtime = self.ecs.0.remove_non_send::<UnitRuntime>()?;
        let audio = self
            .ecs
            .0
            .remove_resource::<crate::audio::AudioSinkRes>()
            .unwrap_or_else(crate::audio::AudioSinkRes::noop);
        let mut spawned: Vec<Entity> = Vec::new();
        let result = {
            let UnitRuntime {
                content,
                grid,
                rng,
                fx,
                seq,
                ..
            } = &mut runtime;
            let mut ctx = crate::combat::bullet::CombatCtx {
                world: &mut self.ecs.0,
                content,
                grid,
                rng,
                fx: fx.as_ref(),
                audio: &audio,
                seq,
                spawned: &mut spawned,
            };
            f(&mut ctx)
        };
        self.ecs.0.insert_resource(audio);
        runtime.bullets_created += spawned.len() as u64;
        runtime.bullets.extend(spawned);
        self.ecs.0.insert_non_send(runtime);
        Some(result)
    }

    /// Drains due `DelayedExplosionActions` (the `Time.run` delays of
    /// `Damage.dynamicExplosion`; H09 T7). Entries count down one per tick and
    /// run in insertion order.
    fn drain_delayed_explosions(&mut self) {
        use crate::combat::damage::explosion::{DelayedAction, DelayedExplosionActions};
        let due = {
            let Some(mut actions) = self.ecs.0.get_resource_mut::<DelayedExplosionActions>() else {
                return;
            };
            let mut due: Vec<DelayedAction> = Vec::new();
            let mut index = 0;
            while index < actions.0.len() {
                actions.0[index].0 -= 1.0;
                if actions.0[index].0 <= 0.0 {
                    due.push(actions.0.remove(index).1);
                } else {
                    index += 1;
                }
            }
            due
        };
        for action in due {
            self.with_combat_ctx(|ctx| {
                crate::combat::damage::explosion::run_delayed_action(ctx, &action);
            });
        }
    }

    /// Deterministic digest of the live unit runtime; empty when not installed.
    pub fn unit_runtime_checksum(&self) -> Checksum {
        match self.ecs.0.get_non_send::<UnitRuntime>() {
            Some(runtime) => runtime.checksum(&self.ecs.0),
            None => Checksummer::new().finish(),
        }
    }

    /// Spawns a live unit by content name and tracks it; `None` without the
    /// runtime or for an unknown unit type.
    pub fn spawn_live_unit(
        &mut self,
        name: &str,
        team: u8,
        x: f32,
        y: f32,
        rotation: f32,
    ) -> Option<Entity> {
        let ecs = &mut self.ecs.0;
        let mut runtime = ecs.remove_non_send::<UnitRuntime>()?;
        let entity = runtime.spawn(ecs, name, team, x, y, rotation);
        ecs.insert_non_send(runtime);
        entity
    }

    /// `PayloadBlockBuild.getPayload()` accessor: the payload held by a
    /// payload-block building (`PayloadHolder.payload`), independent of the
    /// block family.
    pub fn held_payload(&self, holder: Entity) -> Option<crate::world::behavior::PayloadRef> {
        self.ecs
            .0
            .get::<crate::world::blocks::payloads::PayloadHolder>(holder)
            .and_then(|state| state.payload)
    }

    /// `Payload.size()` of the payload held by `holder`, in world units
    /// (block size for build payloads, `UnitType.hitSize` for unit payloads).
    pub fn held_payload_size(&self, holder: Entity) -> Option<f32> {
        let payload = self.held_payload(holder)?;
        Some(crate::world::blocks::payloads::payload_size(
            &self.ecs.0,
            payload,
        ))
    }

    /// Uploads the player movement axis (`Core.input.axis(Binding.moveX/Y)`)
    /// for the ticks that follow; read by
    /// [`crate::ai::types::player::update_player`].
    pub fn set_player_input(&mut self, axis_x: f32, axis_y: f32) {
        let mut input = self
            .ecs
            .0
            .get_resource::<PlayerInput>()
            .copied()
            .unwrap_or_default();
        input.axis_x = axis_x;
        input.axis_y = axis_y;
        self.ecs.0.insert_resource(input);
    }

    /// Uploads the full desktop control state (`DesktopInput.updateMovement`):
    /// movement axis, `player.shooting`, the cursor aim point, the boost key
    /// and `mouseMove`. Read by [`crate::ai::types::player::update_player`].
    #[allow(clippy::too_many_arguments)] // one atomic `Player`-state upload
    pub fn set_player_control(
        &mut self,
        axis_x: f32,
        axis_y: f32,
        shooting: bool,
        aim_x: f32,
        aim_y: f32,
        boosting: bool,
        mouse_move: bool,
    ) {
        let mut input = self
            .ecs
            .0
            .get_resource::<PlayerInput>()
            .copied()
            .unwrap_or_default();
        input.axis_x = axis_x;
        input.axis_y = axis_y;
        input.shooting = shooting;
        input.aim_x = aim_x;
        input.aim_y = aim_y;
        input.boosting = boosting;
        input.mouse_move = mouse_move;
        self.ecs.0.insert_resource(input);
    }

    /// Sets or clears the player unit's mine order
    /// (`InputHandler.tryBeginMine`/`tryStopMine` set `player.unit().mineTile`).
    pub fn set_player_mine_tile(&mut self, tile: Option<TilePos>) {
        let mut input = self
            .ecs
            .0
            .get_resource::<PlayerInput>()
            .copied()
            .unwrap_or_default();
        input.mine_tile = tile;
        input.mine_timer = 0.0;
        self.ecs.0.insert_resource(input);
    }

    /// The player unit's ordered mine tile, if any.
    pub fn player_mine_tile(&self) -> Option<TilePos> {
        self.ecs
            .0
            .get_resource::<PlayerInput>()
            .and_then(|input| input.mine_tile)
    }

    /// Total ore produced by the player unit (tutorial counter evidence).
    pub fn player_mined_count(&self) -> i64 {
        self.ecs
            .0
            .get_resource::<PlayerInput>()
            .map(|input| input.mined)
            .unwrap_or(0)
    }

    /// `InputHandler.canMine` for the player unit; `false` without one.
    pub fn player_can_mine(&self, x: i16, y: i16) -> bool {
        let Some(entity) = self.player_unit else {
            return false;
        };
        let Some(runtime) = self.unit_runtime() else {
            return false;
        };
        crate::ai::types::player::can_mine_tile(
            &self.ecs.0,
            &runtime.grid,
            &runtime.content,
            entity,
            TilePos::new(x, y),
        )
    }

    /// Spawns the controlled player unit (`CoreBlock.playerSpawn` with the core
    /// type's unit) and rebinds its controller to [`AiKind::Player`]. The
    /// `comp::Player` marker puts it in the `player` group; `PlayerInput` drives
    /// it from the next tick.
    pub fn spawn_player_unit(
        &mut self,
        name: &str,
        team: u8,
        x: f32,
        y: f32,
        rotation: f32,
    ) -> Option<Entity> {
        let entity = self.spawn_live_unit(name, team, x, y, rotation)?;
        if let Some(mut slot) = self.ecs.0.get_mut::<ControllerSlot>(entity) {
            slot.kind = AiKind::Player;
            slot.target = None;
        }
        self.ecs
            .0
            .entity_mut(entity)
            .insert(crate::entities::comp::Player);
        self.player_unit = Some(entity);
        self.player_team = Some(team);
        self.player_death_timer = 0.0;
        Some(entity)
    }

    /// `PlayerComp.team` (`PlayerComp.java:44-48`): the player always has a
    /// team, so `update_player_respawn` retries the spawn every 60 ticks even
    /// when the first `spawn_player_at_best_core` failed (no supported core).
    pub fn set_player_team(&mut self, team: u8) {
        self.player_team = Some(team);
        self.player_death_timer = 0.0;
    }

    /// `PlayerComp.bestCore()` (`PlayerComp.java:121-126`): the largest core,
    /// then the closest to the last player position; supported-env cores are
    /// preferred unless there is only one core.
    pub fn best_core_for_player(&self, team: u8) -> Option<Entity> {
        use crate::world::blocks::storage::core_block::CoreBuild;
        let runtime = self.ecs.0.get_non_send::<UnitRuntime>()?;
        let content = &runtime.content;
        let env = crate::game::runtime::rules_of(&self.ecs.0).map_or(1, |rules| rules.env);
        let pos = self.player_last_pos.or_else(|| self.player_unit_position());
        let mut cores: Vec<(Entity, i32, bool, f32)> = Vec::new();
        for entity_ref in self.ecs.0.iter_entities() {
            if entity_ref.get::<CoreBuild>().is_none() {
                continue;
            }
            if entity_ref.get::<TeamComp>().map(|t| t.team) != Some(team) {
                continue;
            }
            let Some(build) = entity_ref.get::<Building>() else {
                continue;
            };
            let size = content.block(build.block).map_or(1, |def| def.size);
            let supported = content
                .block(build.block)
                .and_then(|def| def.core_unit)
                .and_then(|name| content.unit_by_name(name))
                .is_none_or(|unit| unit.supports_env(env));
            let dst2 = entity_ref.get::<Pos>().map_or(0.0, |p| {
                pos.map_or(0.0, |(px, py)| (p.x - px).powi(2) + (p.y - py).powi(2))
            });
            cores.push((entity_ref.id(), size, supported, dst2));
        }
        let single = cores.len() <= 1;
        cores
            .iter()
            .filter(|(_, _, supported, _)| single || *supported)
            .min_by(|a, b| {
                b.1.cmp(&a.1)
                    .then(a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal))
            })
            .map(|(entity, _, _, _)| *entity)
    }

    /// `CoreBlock.playerSpawn` (`CoreBlock.java:95-125`) at the best core
    /// (`PlayerComp.bestCore`). See [`Self::spawn_player_at_core`].
    pub fn spawn_player_at_best_core(&mut self, team: u8) -> Option<Entity> {
        let core = self.best_core_for_player(team)?;
        self.spawn_player_at_core(team, core)
    }

    /// `CoreBlock.requestSpawn(player)` (`CoreBlock.java:639-643`): spawn the
    /// core's `unitType` at the core with reloaded mounts, `spawnedByCore` set
    /// and the upward impulse; `None` when the core unit type does not support
    /// the sector env. Used by the best-core respawn and by
    /// `CoreBlock.onControlSelect` (I01 T12).
    pub fn spawn_player_at_core(&mut self, team: u8, core: Entity) -> Option<Entity> {
        let (x, y) = self.ecs.0.get::<Pos>(core).map(|pos| (pos.x, pos.y))?;
        let (name, env): (String, i32) = {
            let runtime = self.ecs.0.get_non_send::<UnitRuntime>()?;
            let content = &runtime.content;
            let block = self.ecs.0.get::<Building>(core).map(|b| b.block)?;
            let name = content
                .block(block)
                .and_then(|def| def.core_unit)
                .unwrap_or("alpha")
                .to_string();
            let env = crate::game::runtime::rules_of(&self.ecs.0).map_or(1, |rules| rules.env);
            (name, env)
        };
        // `CoreBlock.requestSpawn`: bail when the unit type does not support
        // the sector env (the caller retries on the next death timer window).
        let supports = {
            let runtime = self.ecs.0.get_non_send::<UnitRuntime>()?;
            runtime
                .content
                .unit_by_name(&name)
                .is_some_and(|def| def.supports_env(env))
        };
        if !supports {
            return None;
        }
        let entity = self.spawn_player_unit(&name, team, x, y, 90.0)?;
        // `for(mount : unit.mounts) mount.reload = mount.weapon.reload;`
        let reloads: Vec<f32> = {
            let runtime = self.ecs.0.get_non_send::<UnitRuntime>()?;
            self.ecs
                .0
                .get::<UnitTypeComp>(entity)
                .and_then(|comp| runtime.content.unit(comp.type_id))
                .map(|def| def.weapons.iter().map(|weapon| weapon.reload).collect())
                .unwrap_or_default()
        };
        if let Some(mut weapons) = self.ecs.0.get_mut::<UnitWeapons>(entity) {
            for (mount, reload) in weapons.mounts.iter_mut().zip(reloads) {
                mount.reload = reload;
            }
        }
        if let Some(mut core_comp) = self.ecs.0.get_mut::<UnitCore>(entity) {
            core_comp.spawned_by_core = true;
        }
        // `unit.impulse(0, 3)` (`CoreBlock.playerSpawn`): `vel += v / mass`.
        crate::entities::comp::unit::velocity::impulse(&mut self.ecs.0, entity, 0.0, 3.0);
        // `if(state.isCampaign() && player == Vars.player){ spawnType.unlock(); }`
        // (CoreBlock.java:121-123): queue the spawned unit type for the campaign
        // unlock pipeline (H04 T2's `UnlockRef`).
        let unit_type = self
            .ecs
            .0
            .get::<UnitTypeComp>(entity)
            .map(|comp| comp.type_id);
        let is_campaign = self
            .ecs
            .0
            .get_resource::<crate::game::runtime::CampaignRuntime>()
            .is_some_and(|campaign| campaign.session.is_campaign());
        if is_campaign
            && let Some(id) = unit_type
            && let Some(queues) = self
                .ecs
                .0
                .get_resource::<crate::game::runtime::CampaignCoreQueues>()
            && let Ok(mut queues) = queues.0.lock()
        {
            let unlock = crate::game::runtime::UnlockRef::Unit(id);
            if !queues.unlocks.contains(&unlock) {
                queues.unlocks.push(unlock);
            }
        }
        Some(entity)
    }

    /// `PlayerComp.update` (`PlayerComp.java:214-243`): clear a dead player
    /// unit, then respawn at the best core after `deathDelay` (60 ticks) while
    /// the team still has a core.
    pub fn update_player_respawn(&mut self) {
        if let Some(entity) = self.player_unit {
            if let Some(pos) = self.ecs.0.get::<Pos>(entity) {
                self.player_last_pos = Some((pos.x, pos.y));
            }
            let gone = self.ecs.0.get_entity(entity).is_err()
                || self
                    .ecs
                    .0
                    .get::<Health>(entity)
                    .is_none_or(|health| health.dead);
            let falling = self.ecs.0.get::<Falling>(entity).is_some();
            if gone && !falling {
                self.player_unit = None;
                self.player_death_timer = 0.0;
                if self.ecs.0.get_entity(entity).is_ok() {
                    let _ = self.ecs.0.despawn(entity);
                }
            } else if gone {
                // A dead `Falling` player unit is a wreck: Java's `dead()` is
                // true (the 60-tick respawn delay runs, `PlayerComp.update`)
                // but the wreck stays in the world until it crashes and
                // `resolve_deaths` destroys it (T6). Clear the handle so the
                // respawn timer starts, do not despawn.
                self.player_unit = None;
                self.player_death_timer = 0.0;
            }
        }
        if self.player_unit.is_some() {
            self.player_death_timer = 0.0;
            return;
        }
        let Some(team) = self.player_team else {
            return;
        };
        if self.best_core_for_player(team).is_none() {
            return;
        }
        self.player_death_timer += 1.0;
        if self.player_death_timer >= 60.0 {
            self.player_death_timer = 0.0;
            let _ = self.spawn_player_at_best_core(team);
        }
    }

    /// `InputHandler.unitControl` (`InputHandler.java:775-822`): possess an own
    /// AI unit, moving the `Player` controller to it; a previous core-spawned
    /// unit is despawned without death effects and its type docked.
    pub fn possess_unit(&mut self, seq: i32) -> bool {
        use crate::entities::comp::Player;
        use crate::entities::comp::unit::comp::{DockedType, UnitCore};
        let Some(team) = self.player_team else {
            return false;
        };
        let target = self
            .ecs
            .0
            .iter_entities()
            .find(|entity| {
                entity
                    .get::<crate::ecs::EntitySeq>()
                    .is_some_and(|s| s.0 == seq as u64)
            })
            .map(|entity| entity.id());
        let Some(target) = target else {
            return false;
        };
        if self.ecs.0.get::<TeamComp>(target).map(|t| t.team) != Some(team) {
            return false;
        }
        if self
            .ecs
            .0
            .get::<Health>(target)
            .is_none_or(|health| health.dead)
        {
            return false;
        }
        let def = {
            let Some(runtime) = self.ecs.0.get_non_send::<UnitRuntime>() else {
                return false;
            };
            self.ecs
                .0
                .get::<UnitTypeComp>(target)
                .and_then(|comp| runtime.content.unit(comp.type_id))
                .cloned()
        };
        let Some(def) = def else {
            return false;
        };
        if !def.player_controllable {
            return false;
        }
        if self
            .ecs
            .0
            .get::<ControllerSlot>(target)
            .is_some_and(|slot| slot.kind == AiKind::Player)
        {
            return false;
        }
        let before = self.player_unit.filter(|entity| *entity != target);
        if let Some(mut slot) = self.ecs.0.get_mut::<ControllerSlot>(target) {
            slot.kind = AiKind::Player;
            slot.target = None;
        }
        self.ecs.0.entity_mut(target).insert(Player);
        self.player_unit = Some(target);
        self.player_death_timer = 0.0;
        if let Some(before) = before {
            let core_spawned = self
                .ecs
                .0
                .get::<UnitCore>(before)
                .is_some_and(|core| core.spawned_by_core);
            // `unit.dockedType = before.type` for a core spawn; otherwise carry
            // a dockable type from the previous unit.
            let docked = if core_spawned {
                self.ecs
                    .0
                    .get::<UnitTypeComp>(before)
                    .map(|comp| comp.type_id)
            } else {
                let carried = self.ecs.0.get::<DockedType>(before).map(|docked| docked.0);
                carried.or_else(|| {
                    self.ecs
                        .0
                        .get::<UnitTypeComp>(before)
                        .map(|comp| comp.type_id)
                        .filter(|id| {
                            self.ecs
                                .0
                                .get_non_send::<UnitRuntime>()
                                .and_then(|runtime| runtime.content.unit(*id))
                                .is_some_and(|def| def.core_unit_dock)
                        })
                })
            };
            if let Some(id) = docked {
                self.ecs.0.entity_mut(target).insert(DockedType(id));
            }
            if core_spawned {
                // `Call.unitDespawn(before)`: removal without death effects.
                let _ = self.ecs.0.despawn(before);
                if let Some(mut runtime) = self.ecs.0.get_non_send_mut::<UnitRuntime>() {
                    runtime.units.retain(|entity| *entity != before);
                }
            } else {
                let before_def = {
                    let Some(runtime) = self.ecs.0.get_non_send::<UnitRuntime>() else {
                        return false;
                    };
                    self.ecs
                        .0
                        .get::<UnitTypeComp>(before)
                        .and_then(|comp| runtime.content.unit(comp.type_id))
                        .cloned()
                };
                let _ = before_def;
                self.release_unit_to_ai(before, team);
            }
        }
        true
    }

    /// Releases a possessed unit back to its team AI
    /// (`InputHandler.unitClear` standard branch / `Unit.resetController`):
    /// drop the `Player` marker and hand the unit to the team controller.
    /// Also the controller-switch half of `UnitComp.afterRead` (H04 T13).
    pub(crate) fn release_unit_to_ai(&mut self, entity: Entity, team: u8) {
        use crate::entities::comp::Player;
        let def = {
            let Some(runtime) = self.ecs.0.get_non_send::<UnitRuntime>() else {
                return;
            };
            self.ecs
                .0
                .get::<UnitTypeComp>(entity)
                .and_then(|comp| runtime.content.unit(comp.type_id))
                .cloned()
        };
        let kind = def.map_or(AiKind::Ground, |def| {
            crate::entities::comp::unit::lifecycle::select_team_controller(&self.ecs.0, &def, team)
        });
        self.ecs.0.entity_mut(entity).remove::<Player>();
        if let Some(mut slot) = self.ecs.0.get_mut::<ControllerSlot>(entity) {
            slot.kind = kind;
            slot.target = None;
        }
    }

    /// `InputHandler.unitClear` (`InputHandler.java:824-875`): dock into the
    /// carried core unit type when possible, otherwise release the unit and
    /// respawn at the best core immediately.
    pub fn clear_unit(&mut self) -> bool {
        use crate::entities::comp::unit::comp::{DockedType, HitboxComp, UnitCore};
        let Some(current) = self.player_unit else {
            return false;
        };
        let Some(team) = self.player_team else {
            return false;
        };
        let core_spawned = self
            .ecs
            .0
            .get::<UnitCore>(current)
            .is_some_and(|core| core.spawned_by_core);
        if !core_spawned {
            // `docked == null` -> the best core's unit type is the fallback.
            let docked = self
                .ecs
                .0
                .get::<DockedType>(current)
                .map(|docked| docked.0)
                .or_else(|| {
                    let core = self.best_core_for_player(team)?;
                    let block = self.ecs.0.get::<Building>(core).map(|build| build.block)?;
                    let runtime = self.ecs.0.get_non_send::<UnitRuntime>()?;
                    let name = runtime.content.block(block).and_then(|def| def.core_unit)?;
                    runtime.content.unit_by_name(name).map(|def| def.id)
                });
            let dock_info = docked.and_then(|id| {
                let runtime = self.ecs.0.get_non_send::<UnitRuntime>()?;
                let def = runtime.content.unit(id)?;
                def.core_unit_dock
                    .then(|| (def.name().to_string(), def.flying, def.hit_size))
            });

            if let Some((name, dock_flying, dock_hit)) = dock_info {
                let (px, py) = self
                    .ecs
                    .0
                    .get::<Pos>(current)
                    .map_or((0.0, 0.0), |pos| (pos.x, pos.y));
                let rotation = self
                    .ecs
                    .0
                    .get::<UnitCore>(current)
                    .map_or(0.0, |core| core.rotation);
                let (current_flying, current_hit) = {
                    let Some(runtime) = self.ecs.0.get_non_send::<UnitRuntime>() else {
                        return false;
                    };
                    let flying = self
                        .ecs
                        .0
                        .get::<UnitTypeComp>(current)
                        .and_then(|comp| runtime.content.unit(comp.type_id))
                        .is_some_and(|def| def.flying);
                    let hit = self
                        .ecs
                        .0
                        .get::<HitboxComp>(current)
                        .map_or(8.0, |hitbox| hitbox.hit_size);
                    (flying, hit)
                };
                // Translate backwards so the docked unit is not stuck inside
                // the possessed one (`InputHandler.java:851-854`).
                let (sx, sy) = if current_flying && dock_flying {
                    let rad = (rotation + 180.0).to_radians();
                    let len = current_hit / 2.0 + dock_hit / 2.0;
                    (px + rad.cos() * len, py + rad.sin() * len)
                } else {
                    (px, py)
                };
                // The possessed unit must go back to the AI before the docked
                // unit takes the `Player` controller (`unit.controller(player)`
                // calls `old.resetController()`, InputHandler.java:865-872);
                // otherwise two entities keep the player controller.
                self.release_unit_to_ai(current, team);
                let Some(entity) = self.spawn_player_unit(&name, team, sx, sy, rotation) else {
                    return false;
                };
                if let Some(mut core_comp) = self.ecs.0.get_mut::<UnitCore>(entity) {
                    core_comp.spawned_by_core = true;
                }
                return true;
            }
        }
        // Standard respawn: release the unit to AI, then `player.checkSpawn()`.
        self.release_unit_to_ai(current, team);
        self.player_unit = None;
        let _ = self.spawn_player_at_best_core(team);
        true
    }

    /// `CoreBlock.canControlSelect(player)`/`onControlSelect(player)`
    /// (`CoreBlock.java:619-637`): Ctrl-click on an own core clears the player's
    /// unit and respawns it at that core (`player.clearUnit();
    /// player.deathTimer = Player.deathDelay + 1; requestSpawn(player)` ->
    /// `Call.playerSpawn(tile, player)`). I01 T12.
    pub fn control_select_core(&mut self, core: Entity) -> bool {
        let Some(team) = self.player_team else {
            return false;
        };
        // `canControlSelect(unit)` = `unit.isPlayer()`: the player must hold a
        // unit to control-select with.
        if self.player_unit.is_none() {
            return false;
        }
        if let Some(current) = self.player_unit {
            self.release_unit_to_ai(current, team);
            self.player_unit = None;
        }
        // `player.deathTimer = Player.deathDelay + 1` (60 + 1) guards the retry
        // window while `requestSpawn` spawns immediately.
        self.player_death_timer = 61.0;
        if self.spawn_player_at_core(team, core).is_some() {
            self.player_death_timer = 0.0;
            true
        } else {
            false
        }
    }

    /// `InputHandler.selectedUnit()`'s ControlBlock branch
    /// (`InputHandler.java:2159-2161`): `build instanceof ControlBlock cont &&
    /// cont.canControl() && build.team == player.team() && cont.unit() !=
    /// player.unit()` -> the detached `block` proxy unit. Turrets
    /// (`TurretBuild.unit`; I01 T12) need `playerControllable`
    /// (`Turret.java:382-384`); size-1 routers (`Router.java:46-49`) get their
    /// proxy created on first use like Java's lazy `unit()` (`:37-44`; L06
    /// T2). `None` when the tile holds no controllable block, the team
    /// differs, or the proxy is the player's own unit already.
    pub fn control_block_unit_id(&mut self, pos: TilePos) -> Option<i32> {
        let entity = self.grid.entity_at(pos)?;
        let team = self.player_team?;
        if self.ecs.0.get::<TeamComp>(entity).map(|t| t.team) != Some(team) {
            return None;
        }
        let def = self
            .ecs
            .0
            .get::<crate::entities::comp::Building>(entity)
            .and_then(|building| {
                self.ecs
                    .0
                    .get_resource::<crate::world::block::BlockTable>()
                    .and_then(|table| table.get(building.block))
                    .map(|instance| {
                        (
                            instance.def.kind,
                            instance.def.size,
                            instance.def.player_controllable,
                        )
                    })
            });
        let unit = match def {
            // `RouterBuild.canControl()` = `size == 1` (the distributor is not).
            Some((crate::content::BlockKind::Router, size, _)) => {
                if size != 1 {
                    return None;
                }
                self.ensure_router_block_unit(entity)?
            }
            Some((_, _, player_controllable)) => {
                // `TurretBuild.canControl()` = `playerControllable`.
                if !player_controllable {
                    return None;
                }
                self.ecs
                    .0
                    .get::<crate::world::blocks::defense::turrets::TurretState>(entity)?
                    .block_unit?
            }
            None => return None,
        };
        if self.player_unit == Some(unit) || self.ecs.0.get_entity(unit).is_err() {
            return None;
        }
        self.ecs.0.get::<EntitySeq>(unit).map(|seq| seq.0 as i32)
    }

    /// `RouterBuild.unit()` (`Router.java:37-44`): `UnitTypes.block.create(team)`
    /// tied to the router on first use; kept on `RouterBuild.block_unit`,
    /// never added to the unit pass.
    fn ensure_router_block_unit(&mut self, router: Entity) -> Option<Entity> {
        use crate::world::blocks::distribution::router::RouterBuild;
        if let Some(unit) = self.ecs.0.get::<RouterBuild>(router)?.block_unit
            && self.ecs.0.get_entity(unit).is_ok()
        {
            return Some(unit);
        }
        let def = self
            .unit_runtime()?
            .content
            .unit_by_name("block")
            .cloned()?;
        let team = self.ecs.0.get::<TeamComp>(router).map_or(0, |t| t.team);
        let (x, y) = self
            .ecs
            .0
            .get::<crate::entities::comp::Pos>(router)
            .map(|pos| (pos.x, pos.y))?;
        let tile = self
            .ecs
            .0
            .get::<crate::entities::comp::Building>(router)
            .map_or(i32::MIN, |building| building.tile.pack());
        let world = &mut self.ecs.0;
        let seq = crate::world::blocks::units::behavior::next_unit_seq(world);
        let unit = crate::entities::comp::unit::lifecycle::spawn_unit_def(
            world, seq, &def, team, x, y, 0.0,
        );
        world
            .entity_mut(unit)
            .insert(crate::entities::comp::unit::comp::BlockUnitComp { tile });
        if let Some(mut state) = world.get_mut::<RouterBuild>(router) {
            state.block_unit = Some(unit);
        }
        Some(unit)
    }

    /// The controlled player unit entity, if any.
    pub fn player_unit(&self) -> Option<Entity> {
        self.player_unit
    }

    /// Debug seam (H01 T3): kill the unit with `EntitySeq == seq` by zeroing
    /// its health (`UnitComp.killed`; `environment::kill`); returns whether a
    /// live unit was found.
    pub fn debug_kill_unit(&mut self, seq: i64) -> bool {
        let target = self
            .ecs
            .0
            .iter_entities()
            .find(|entity| {
                entity
                    .get::<crate::ecs::EntitySeq>()
                    .is_some_and(|s| s.0 == seq as u64)
            })
            .map(|entity| entity.id());
        let Some(target) = target else {
            return false;
        };
        let Some(mut health) = self.ecs.0.get_mut::<Health>(target) else {
            return false;
        };
        health.health = 0.0;
        health.dead = true;
        true
    }

    /// Debug seam (G01 T19): apply a status by name to a unit selected by
    /// `EntitySeq`; returns false for unknown id/status/entity.
    pub fn debug_apply_status(&mut self, seq: i64, status: &str, ticks: f32) -> bool {
        let Some(status_id) = self
            .unit_runtime()
            .and_then(|runtime| runtime.content.status_id(status))
        else {
            return false;
        };
        let target = self
            .ecs
            .0
            .iter_entities()
            .find(|entity| {
                entity
                    .get::<crate::ecs::EntitySeq>()
                    .is_some_and(|s| s.0 == seq as u64)
            })
            .map(|entity| entity.id());
        let Some(target) = target else {
            return false;
        };
        crate::combat::damage::status::apply_status(&mut self.ecs.0, target, status_id, ticks);
        true
    }

    /// `boss_json` seam (G01 T19): `Teams.bosses`/`GameState.boss()` for the
    /// HUD boss bar (G03 T9) and the `guardian` hint (G05).
    pub fn boss_json(&self) -> String {
        use crate::content::ctype::Mappable;
        use crate::entities::comp::unit::comp::{StatusComp, UnitTypeComp};
        let bosses = self.bosses();
        let runtime = self.unit_runtime();
        let mut names: Vec<String> = Vec::new();
        let mut health_sum = 0.0f32;
        let mut max_sum = 0.0f32;
        let mut first: Option<serde_json::Value> = None;
        for (index, &entity) in bosses.iter().enumerate() {
            let def = runtime.and_then(|runtime| {
                self.ecs
                    .0
                    .get::<UnitTypeComp>(entity)
                    .and_then(|comp| runtime.content.unit(comp.type_id))
            });
            let name = def.map(|def| def.name().to_string()).unwrap_or_default();
            if index < 6 {
                names.push(name.clone());
            }
            let health = self.ecs.0.get::<Health>(entity).copied();
            health_sum += health.map_or(0.0, |h| h.health.max(0.0));
            max_sum += health.map_or(0.0, |h| h.max_health);
            if index == 0 {
                let armor_override = self
                    .ecs
                    .0
                    .get::<StatusComp>(entity)
                    .map(|status| status.armor_override);
                let armor = armor_override
                    .filter(|value| *value >= 0.0)
                    .unwrap_or_else(|| def.map_or(0.0, |def| def.armor));
                first = Some(serde_json::json!({
                    "id": self.ecs.0.get::<crate::ecs::EntitySeq>(entity).map_or(0, |seq| seq.0),
                    "type": name,
                    "armor": armor,
                    "flying": crate::combat::bullet::unit_is_flying(&self.ecs.0, entity),
                    "health": health.map_or(0.0, |h| h.health),
                    "max_health": health.map_or(0.0, |h| h.max_health),
                }));
            }
        }
        let value = if max_sum > 0.0 {
            health_sum / max_sum
        } else {
            0.0
        };
        serde_json::json!({
            "count": bosses.len(),
            "names": names,
            "value": value,
            "first": first,
        })
        .to_string()
    }

    /// `Teams.bosses` (`Teams.java:213-223`): live wave-team units with the
    /// `boss` status, in `EntitySeq` order.
    pub fn bosses(&self) -> Vec<Entity> {
        let Some(runtime) = self.unit_runtime() else {
            return Vec::new();
        };
        let Some(boss) = runtime.content.status_id("boss") else {
            return Vec::new();
        };
        let wave_team = crate::game::runtime::rules_of(&self.ecs.0)
            .map_or(runtime.wave_team, |rules| rules.wave_team);
        let mut bosses: Vec<(u64, Entity)> = Vec::new();
        for &entity in &runtime.units {
            if self.ecs.0.get_entity(entity).is_err() {
                continue;
            }
            if self.ecs.0.get::<TeamComp>(entity).map(|t| t.team) != Some(wave_team) {
                continue;
            }
            if self
                .ecs
                .0
                .get::<Health>(entity)
                .is_none_or(|health| health.dead)
            {
                continue;
            }
            let has_boss = self
                .ecs
                .0
                .get::<crate::entities::comp::unit::comp::StatusComp>(entity)
                .is_some_and(|statuses| statuses.statuses.iter().any(|entry| entry.effect == boss));
            if !has_boss {
                continue;
            }
            let seq = self
                .ecs
                .0
                .get::<crate::ecs::EntitySeq>(entity)
                .map_or(u64::MAX, |seq| seq.0);
            bosses.push((seq, entity));
        }
        bosses.sort_by_key(|(seq, entity)| (*seq, entity.index()));
        bosses.into_iter().map(|(_, entity)| entity).collect()
    }

    /// World-pixel position of the player unit (`Player.x/y` mirror).
    pub fn player_unit_position(&self) -> Option<(f32, f32)> {
        let entity = self.player_unit?;
        self.ecs.0.get::<Pos>(entity).map(|pos| (pos.x, pos.y))
    }

    /// Whether a non-zero movement axis has reached the player unit since the
    /// last world load (`HintsFragment.desktopMove` trigger).
    pub fn player_unit_moved(&self) -> bool {
        self.ecs
            .0
            .get_resource::<PlayerInput>()
            .is_some_and(|input| input.moved)
    }
}

pub fn unit_update_system(world: &mut World) {
    // `CombatEnv` sync (G01 T14): `Rules.fire` and `Env.oxygen` gate
    // `Fires.create`; `state.envAttrs` heat (0.0 until a live source lands).
    if let Some(campaign) = world.get_resource::<CampaignRuntime>() {
        let rules = &campaign.session.rules;
        world.insert_resource(crate::combat::fires::CombatEnv {
            fire: rules.fire,
            oxygen: rules.env & crate::game::planet::env_bit(crate::content::EnvFlag::Oxygen) != 0,
            heat: 0.0,
        });
    }
    let Some(mut runtime) = world.remove_non_send::<UnitRuntime>() else {
        return;
    };
    runtime.units.retain(|entity| {
        world.get_entity(*entity).is_ok() && world.get::<UnitTypeComp>(*entity).is_some()
    });
    if runtime.units.is_empty() {
        world.insert_non_send(runtime);
        return;
    }

    // One target index per tick (buildings by team + live units), reused by
    // every unit's acquisition pass. Bullets/fires/puddles never enter it.
    let queries = TargetQueries::build(world, &runtime.content);
    // `AvoidanceProcess`: while active, rebuild the occupancy grid from the
    // current positions before any unit moves this tick (queries read the
    // previous frame's buffer; the tick-start grid is that data).
    if runtime.avoidance_active {
        rebuild_avoidance(world, &mut runtime);
    }
    // `state.rules` fields `GroundAI` reads, snapshotted once per tick so the
    // unit loop can mutate the world freely.
    let ground_rules = world
        .get_resource::<CampaignRuntime>()
        .map(|campaign| GroundAiRules {
            waves: campaign.session.rules.waves,
            default_team: campaign.session.rules.default_team,
            drop_zone_radius: campaign.session.rules.drop_zone_radius,
            wave_team: campaign.session.rules.wave_team,
            pvp: campaign.session.rules.pvp,
        })
        .unwrap_or_default();
    // `Pathfinder.EnemyCoreField.getPositions` target sets, one per team per
    // tick (plan G01 §6.9: stable targets keep the `(team, cost)` field cache).
    let mut core_target_cache: BTreeMap<u8, Vec<TilePos>> = BTreeMap::new();
    // `GameState.hasSpawns()` (`GameState.java:68-70`): `rules.waves &&
    // ((rules.waveTeam has a core && rules.attackMode) || !rules.spawns.isEmpty())`.
    // Snapshotted once per tick for the per-unit drop-zone push gate (T5).
    let has_spawns = world
        .get_resource::<CampaignRuntime>()
        .is_some_and(|campaign| {
            let rules = &campaign.session.rules;
            rules.waves
                && ((rules.attack_mode
                    && has_core_of_team(world, &runtime.content, rules.wave_team))
                    || !rules.spawns.is_empty())
        });
    let entities = runtime.units.clone();
    for entity in entities {
        if world.get_entity(entity).is_err() {
            continue;
        }
        let before = world.get::<Pos>(entity).map(|pos| (pos.x, pos.y));
        // `VelComp.update` is `@MethodPriority(-1)`, so the velocity model
        // integrates `vel` (tile collisions + drag) before the controller runs.
        // `before` is read first so the kind kinematics `delta` includes this
        // tick's movement.
        crate::entities::comp::unit::velocity::vel_update(
            world,
            &runtime.grid,
            &runtime.content,
            entity,
        );
        let Some(slot) = world.get::<ControllerSlot>(entity).copied() else {
            continue;
        };
        let team = world
            .get::<TeamComp>(entity)
            .map(|team| team.team)
            .unwrap_or(0);

        // `UnitComp.update` (`UnitComp.java:869-871`): remove units spawned by
        // the core that are not player-controlled.
        if world
            .get::<crate::entities::comp::unit::comp::UnitCore>(entity)
            .is_some_and(|core| core.spawned_by_core)
            && slot.kind != AiKind::Player
            && !world
                .get::<crate::entities::comp::Health>(entity)
                .is_none_or(|health| health.dead)
        {
            let _ = world.despawn(entity);
            continue;
        }

        // `UnitComp.update` (`UnitComp.java:859-861`): `if(!net.client() &&
        // !dead && shouldUpdateController()) controller.updateUnit();` — a dead
        // unit never runs its controller/targeting/weapons. A `Falling` wreck
        // keeps integrating its velocity with `drag = 0.01` and sinks at
        // `type.fallSpeed` (UnitComp.java:803-830); any other dead unit is
        // destroyed by `resolve_deaths` at the end of the tick.
        if world
            .get::<crate::entities::comp::Health>(entity)
            .is_some_and(|health| health.dead)
        {
            if world.get::<Falling>(entity).is_some() {
                crate::entities::comp::unit::environment::update_falling_wreck(
                    world,
                    &runtime.grid,
                    &runtime.content,
                    entity,
                );
            }
            continue;
        }

        if slot.kind == AiKind::Player {
            // `Player` possession: the host-uploaded axis drives the unit
            // (`DesktopInput.updateMovement`); no target acquisition or pathing.
            crate::ai::types::player::update_player(world, &runtime.grid, &runtime.content, entity);
        } else if slot.kind == AiKind::Command {
            // `CommandAI.updateUnit` -> `defaultBehavior`/`commandController`
            // (T10a/T10b: MinerAI/RepairAI).
            let command_kind = resolve_command_kind(world, &runtime.content, entity, &slot);
            if command_kind == Some(crate::content::registries::commands::ControllerKind::Repair) {
                // `RepairAI.updateTargeting` replaces the enemy targeting: the
                // closest damaged friendly building, else `super` targeting.
                let mut repair = world
                    .get::<crate::ai::types::repair::RepairAiState>(entity)
                    .copied()
                    .unwrap_or_default();
                {
                    let UnitRuntime {
                        content,
                        grid,
                        pathfinder,
                        ..
                    } = &mut runtime;
                    let mut ctx = crate::ai::ai_controller::AiCtx {
                        world,
                        grid,
                        content,
                        pathfinder,
                        team,
                    };
                    crate::ai::types::repair::update_repair_ai(&mut ctx, entity, &mut repair);
                }
                if let Some(mut stored) =
                    world.get_mut::<crate::ai::types::repair::RepairAiState>(entity)
                {
                    *stored = repair;
                } else if world.get_entity(entity).is_ok() {
                    world.entity_mut(entity).insert(repair);
                }
            } else if matches!(
                command_kind,
                Some(crate::content::registries::commands::ControllerKind::Builder)
                    | Some(crate::content::registries::commands::ControllerKind::BuilderAssist)
            ) {
                // `UnitCommand.loadAll`: rebuild -> `BuilderAI()` (alwaysFlee
                // false, fleeRange 370); assist -> `BuilderAI{onlyAssist=true}`
                // (UnitCommand.java:73-116).
                let only_assist = command_kind
                    == Some(crate::content::registries::commands::ControllerKind::BuilderAssist);
                let mut builder = world
                    .get::<crate::ai::types::builder::BuilderAiState>(entity)
                    .copied()
                    .unwrap_or_else(|| {
                        let mut state =
                            crate::ai::types::builder::BuilderAiState::with_flee(false, 370.0);
                        state.only_assist = only_assist;
                        state
                    });
                {
                    let UnitRuntime {
                        content,
                        grid,
                        pathfinder,
                        ..
                    } = &mut runtime;
                    let mut ctx = crate::ai::ai_controller::AiCtx {
                        world,
                        grid,
                        content,
                        pathfinder,
                        team,
                    };
                    crate::ai::types::builder::update_builder_ai(&mut ctx, entity, &mut builder);
                }
                if let Some(mut stored) =
                    world.get_mut::<crate::ai::types::builder::BuilderAiState>(entity)
                {
                    *stored = builder;
                } else if world.get_entity(entity).is_ok() {
                    world.entity_mut(entity).insert(builder);
                }
                // `BuilderAI.shouldShoot()`: `!unit.isBuilding() && canAttack`
                // (BuilderAI.java:262-264); `isBuilding` is
                // `buildSpeedMultiplier > 0 && activelyBuilding` (UnitComp.java).
                let is_building = world
                    .get::<crate::sim::builder::BuilderPlans>(entity)
                    .is_some_and(|plans| plans.update_building && !plans.plans.is_empty());
                let can_attack = world
                    .get::<UnitTypeComp>(entity)
                    .and_then(|comp| runtime.content.unit(comp.type_id))
                    .is_some_and(|def| def.can_attack);
                let _ = command_ai_weapon_pass(
                    world,
                    &runtime.content,
                    &queries,
                    entity,
                    team,
                    can_attack && !is_building,
                );
            } else {
                // Targeting runs for the weapons (`updateTargeting`, autoTarget);
                // movement only follows an explicit order (`targetPos`); with none
                // the unit idles in place instead of walking to the core anchor.
                let mount_target =
                    command_ai_weapon_pass(world, &runtime.content, &queries, entity, team, true);
                // `CommandAI.updateUnit`'s stance handling (T11): boost and
                // pursueTarget (edit `slot` before the movement match).
                let mut slot = slot;
                let stances = world
                    .get::<crate::ai::unit_stance_runtime::UnitStancesComp>(entity)
                    .copied()
                    .unwrap_or_default();
                let def_range = world
                    .get::<UnitTypeComp>(entity)
                    .and_then(|comp| runtime.content.unit(comp.type_id))
                    .map(|def| def.max_range)
                    .unwrap_or(0.0);
                if stances.has_name(&runtime.content, "boost") {
                    // `shouldBoost()`: `updateBoosting(false)` while an attack
                    // target is in range, else true (CommandAI.java:150-163).
                    let in_range = mount_target
                        .is_some_and(|target| entity_pos_within(world, entity, target, def_range));
                    if let Some(mut core) = world.get_mut::<UnitCore>(entity) {
                        core.boosting = !in_range;
                    }
                }
                if slot.target.is_none()
                    && stances.has_name(&runtime.content, "pursuetarget")
                    && let Some(target) = mount_target
                    && let Some(pos) = world.get::<Pos>(target).copied()
                {
                    // `pursueTarget(target, true)`: `commandTarget(target, false)`.
                    slot.target = Some(TilePos::new(
                        crate::world::WorldGrid::to_tile(pos.x) as i16,
                        crate::world::WorldGrid::to_tile(pos.y) as i16,
                    ));
                }
                // `CommandAI.defaultBehavior` attack tracking (G01 T16):
                // `targetPos.set(attackTarget)` every tick, dead targets are
                // cleared, and the unit stops within `range - 10`.
                let mut move_target = slot.target;
                if let Some(target) = world
                    .get::<crate::ai::types::command::AttackTargetComp>(entity)
                    .and_then(|comp| comp.0)
                {
                    let alive = world.get_entity(target).is_ok()
                        && world
                            .get::<Health>(target)
                            .is_none_or(|health| !health.dead);
                    if !alive {
                        world
                            .entity_mut(entity)
                            .insert(crate::ai::types::command::AttackTargetComp(None));
                        move_target = None;
                        if let Some(mut stored) = world.get_mut::<ControllerSlot>(entity) {
                            stored.target = None;
                        }
                    } else if let Some(target_pos) = world.get::<Pos>(target).copied() {
                        let within = world.get::<Pos>(entity).copied().is_some_and(|pos| {
                            let dx = target_pos.x - pos.x;
                            let dy = target_pos.y - pos.y;
                            dx * dx + dy * dy <= (def_range - 10.0).max(0.0).powi(2)
                        });
                        move_target = if within {
                            None
                        } else {
                            Some(TilePos::new(
                                crate::world::WorldGrid::to_tile(target_pos.x) as i16,
                                crate::world::WorldGrid::to_tile(target_pos.y) as i16,
                            ))
                        };
                    }
                }
                if command_kind == Some(crate::content::registries::commands::ControllerKind::Miner)
                {
                    // `CommandAI.updateUnit`: the mine command auto-adds
                    // `mineAuto` when no item stance is set (T11).
                    let mut stances = world
                        .get::<crate::ai::unit_stance_runtime::UnitStancesComp>(entity)
                        .copied()
                        .unwrap_or_default();
                    if !stances.has_name(&runtime.content, "mineauto")
                        && !stances.any_item_stance(&runtime.content)
                        && let Some(def) = runtime.content.unit_stance_by_name("mineauto")
                    {
                        crate::ai::unit_stance_runtime::apply_stance(&mut stances.0, def);
                        world.entity_mut(entity).insert(stances);
                    }
                    let mut miner = world
                        .get::<crate::ai::types::miner::MinerAiState>(entity)
                        .copied()
                        .unwrap_or_default();
                    {
                        let UnitRuntime {
                            content,
                            grid,
                            pathfinder,
                            ..
                        } = &mut runtime;
                        let mut ctx = crate::ai::ai_controller::AiCtx {
                            world,
                            grid,
                            content,
                            pathfinder,
                            team,
                        };
                        crate::ai::types::miner::update_miner_ai(&mut ctx, entity, &mut miner);
                    }
                    if let Some(mut stored) =
                        world.get_mut::<crate::ai::types::miner::MinerAiState>(entity)
                    {
                        *stored = miner;
                    } else if world.get_entity(entity).is_ok() {
                        world.entity_mut(entity).insert(miner);
                    }
                } else {
                    if world
                        .get::<crate::entities::comp::unit::comp::MinerComp>(entity)
                        .is_some_and(|comp| comp.mine_tile.is_some())
                    {
                        // `CommandAI.command`: a non-mine command clears `mineTile`.
                        crate::entities::comp::unit::miner::clear_mine_tile(world, entity);
                    }
                    // The default `defaultBehavior` movement runs for commands
                    // without a controller (`None`: move/enterPayload/...) and for
                    // the not-yet-dispatched Repair/Builder controllers.
                    match move_target {
                        Some(target) => {
                            let flying = world
                                .get::<PhysicsComp>(entity)
                                .is_some_and(|physics| physics.flying);
                            // Refresh the lazy control path before borrowing
                            // fields (`unit.type.pathCostId`).
                            let cost = unit_control_cost(world, &runtime.content, entity);
                            runtime.control_path(cost);
                            let UnitRuntime {
                                grid,
                                content,
                                control_paths,
                                ..
                            } = &mut runtime;
                            let arrived = crate::ai::types::command::update_command_move(
                                world,
                                grid,
                                content,
                                &mut control_paths[usize::from(cost.id())],
                                entity,
                                target,
                                flying,
                            );
                            if arrived {
                                // `finishPath`: `targetPos = null`.
                                if let Some(mut stored) = world.get_mut::<ControllerSlot>(entity) {
                                    stored.target = None;
                                }
                                // Then the next queued waypoint becomes the
                                // target (`CommandAI.java:476-490`, I01 T13).
                                pop_live_command_queue(world, &runtime.grid, entity);
                            }
                        }
                        None => {
                            // Java `CommandAI.defaultBehavior` idles without
                            // touching `vel`; drag brings the unit to a stop.
                        }
                    }
                }
                // `CommandAI` -> `faceTarget()` (CommandAI.java:400/420).
                face_target(world, &runtime.content, entity, mount_target);
            }
        } else if slot.kind == AiKind::Missile {
            // `MissileAI.updateMovement` (live dispatch, PAR-0088): steer at
            // the shooter's aim, always move forward, die on enemy-building
            // contact or at the `TimedKill` lifetime.
            let shooter = world
                .get::<MissileShooter>(entity)
                .map(|shooter| shooter.0)
                .unwrap_or(None);
            // `AIController.target` snapshot from the shot's aimed entity
            // (`BulletType.create` `target`; `MissileAI.java:33-36`).
            let target = world
                .get::<crate::ai::types::missile::MissileTarget>(entity)
                .map(|target| target.0)
                .unwrap_or(None);
            let mut state = MissileAi {
                unit: Some(entity),
                target,
                shooter,
            };
            {
                let UnitRuntime {
                    content,
                    grid,
                    pathfinder,
                    ..
                } = &mut runtime;
                let mut ctx = crate::ai::ai_controller::AiCtx {
                    world,
                    grid,
                    content,
                    pathfinder,
                    team,
                };
                let _ = update_missile(&mut ctx, entity, &mut state);
            }
            if let Some(mut stored) = world.get_mut::<MissileShooter>(entity) {
                stored.0 = state.shooter;
            }
            // `AIController.updateTargeting` -> `updateWeapons`
            // (AIController.java:114-116, 172-238): a controllable weapon finds
            // its own target within `weapon.range()`. For the scathe missile the
            // shoot-on-death explosion is a proximity fuse against ground units
            // (`findTarget(mountX, mountY, wrange, bullet.collidesAir,
            // bullet.collidesGround)`).
            let mount_target = {
                let def = world
                    .get::<UnitTypeComp>(entity)
                    .and_then(|comp| runtime.content.unit(comp.type_id));
                def.and_then(|def| {
                    let weapon = def.weapons.first()?;
                    let range = weapon.range();
                    let (air, ground) = runtime
                        .content
                        .bullet(weapon.bullet.id)
                        .map(|bullet| (bullet.collides_air, bullet.collides_ground))
                        .unwrap_or((false, true));
                    let (x, y) = world.get::<Pos>(entity).map(|pos| (pos.x, pos.y))?;
                    queries.closest_target(
                        world,
                        team,
                        x,
                        y,
                        range,
                        air,
                        ground,
                        true,
                        def.target_under_blocks,
                    )
                })
            };
            update_ai_weapons(
                world,
                &runtime.content,
                &queries,
                entity,
                team,
                AiWeaponCtx {
                    main_target: mount_target,
                    retarget: true,
                    flying_find: false,
                    core_override: None,
                    should_shoot: true,
                    ..Default::default()
                },
            );
        } else if slot.kind == AiKind::Suicide {
            // `SuicideAI.updateUnit` (`ai/types/suicide.rs`): ram the nearest
            // hostile in a straight line (terrain ignored) and fire the
            // close-range death weapon (`shootCone 180`).
            let mount_target = {
                let pos = world.get::<Pos>(entity).copied();
                let def = world
                    .get::<UnitTypeComp>(entity)
                    .and_then(|comp| runtime.content.unit(comp.type_id));
                match (pos, def) {
                    (Some(pos), Some(def)) => queries.closest_target(
                        world,
                        team,
                        pos.x,
                        pos.y,
                        def.max_range.max(def.range),
                        def.target_air,
                        def.target_ground,
                        true,
                        def.target_under_blocks,
                    ),
                    _ => None,
                }
            };
            if let Some(mut state) = world.get_mut::<AiWeaponsState>(entity) {
                state.main_target = mount_target;
                state.retarget = 0.0;
            } else if world.get_entity(entity).is_ok() {
                world.entity_mut(entity).insert(AiWeaponsState {
                    main_target: mount_target,
                    retarget: 0.0,
                    ..AiWeaponsState::default()
                });
            }
            update_ai_weapons(
                world,
                &runtime.content,
                &queries,
                entity,
                team,
                AiWeaponCtx {
                    main_target: mount_target,
                    retarget: false,
                    flying_find: false,
                    core_override: None,
                    should_shoot: true,
                    ..Default::default()
                },
            );
            let mut state = crate::ai::types::suicide::SuicideAi {
                unit: Some(entity),
                target: mount_target,
                suicidal: false,
            };
            {
                let UnitRuntime {
                    content,
                    grid,
                    pathfinder,
                    ..
                } = &mut runtime;
                let mut ctx = crate::ai::ai_controller::AiCtx {
                    world,
                    grid,
                    content,
                    pathfinder,
                    team,
                };
                let _ = crate::ai::types::suicide::update_suicide(&mut ctx, entity, &mut state);
            }
        } else if slot.kind == AiKind::Builder && !builder_falls_back(world, team) {
            // `BuilderAI` (T10c): pull ghost plans from the team queue and
            // move toward the active plan; `Sim::update_builders` executes it.
            let mut state = world
                .get::<crate::ai::types::builder::BuilderAiState>(entity)
                .copied()
                .unwrap_or_else(|| {
                    // AI-team core builders construct with
                    // `BuilderAI(true, coreFleeRange)` (`UnitTypes.java:2495`,
                    // 4352; the def carries the range).
                    let core_flee_range = world
                        .get::<UnitTypeComp>(entity)
                        .and_then(|comp| runtime.content.unit(comp.type_id))
                        .and_then(|def| match def.controller {
                            crate::content::registries::units::ControllerKind::Builder {
                                core_flee_range,
                            } => Some(core_flee_range),
                            _ => None,
                        })
                        .unwrap_or(400.0);
                    crate::ai::types::builder::BuilderAiState::with_flee(true, core_flee_range)
                });
            {
                let UnitRuntime {
                    content,
                    grid,
                    pathfinder,
                    ..
                } = &mut runtime;
                let mut ctx = crate::ai::ai_controller::AiCtx {
                    world,
                    grid,
                    content,
                    pathfinder,
                    team,
                };
                crate::ai::types::builder::update_builder_ai(&mut ctx, entity, &mut state);
            }
            if let Some(mut stored) =
                world.get_mut::<crate::ai::types::builder::BuilderAiState>(entity)
            {
                *stored = state;
            } else if world.get_entity(entity).is_ok() {
                world.entity_mut(entity).insert(state);
            }
            let _ = command_ai_weapon_pass(world, &runtime.content, &queries, entity, team, true);
        } else if slot.kind == AiKind::Boost {
            // `BoostAI.defaultBehavior` (`ai/types/boost.rs`): enable boost
            // without issuing movement; the lift-off runs in `update_kinematics`
            // (`UnitComp.updateBoosting`).
            let mut state = crate::ai::types::boost::BoostAi {
                unit: Some(entity),
                boosting: true,
            };
            {
                let UnitRuntime {
                    content,
                    grid,
                    pathfinder,
                    ..
                } = &mut runtime;
                let mut ctx = crate::ai::ai_controller::AiCtx {
                    world,
                    grid,
                    content,
                    pathfinder,
                    team,
                };
                crate::ai::types::boost::update_boost(&mut ctx, entity, &mut state);
            }
        } else if slot.kind == AiKind::Defender {
            // `DefenderAI` (`DefenderAI.java:12-45`): escort the healthiest
            // player-controllable ally; no weapons pass of its own.
            update_defender_ai(world, &mut runtime, entity, team, ground_rules);
        } else if slot.kind == AiKind::FlyingFollow {
            // `FlyingFollowAI` (`FlyingFollowAI.java:11-60`): follow an allied
            // leader on wave teams, else the `FlyingAI` fallback.
            update_flying_follow_ai(
                world,
                &mut runtime,
                &queries,
                entity,
                team,
                slot,
                ground_rules,
            );
        } else if slot.kind == AiKind::Hug {
            // `HugAI` (`HugAI.java:16-61`): ray-checked approach, contact
            // circle and the enemy-core override.
            update_hug_ai(
                world,
                &mut runtime,
                &queries,
                entity,
                team,
                ground_rules,
                &mut core_target_cache,
            );
        } else if slot.kind == AiKind::Cargo {
            // `CargoAI` (`CargoAI.java:26-97`, H01 T13): the tether unit
            // shuttles items between its loader and a configured unload point.
            let mut state = world
                .get::<crate::ai::types::cargo::CargoAi>(entity)
                .copied()
                .unwrap_or_default();
            {
                let UnitRuntime {
                    content,
                    grid,
                    pathfinder,
                    ..
                } = &mut runtime;
                let mut ctx = crate::ai::ai_controller::AiCtx {
                    world,
                    grid,
                    content,
                    pathfinder,
                    team,
                };
                let _ = crate::ai::types::cargo::update_cargo(&mut ctx, entity, &mut state);
            }
            if let Some(mut stored) = world.get_mut::<crate::ai::types::cargo::CargoAi>(entity) {
                *stored = state;
            } else if world.get_entity(entity).is_ok() {
                world.entity_mut(entity).insert(state);
            }
        } else if slot.kind == AiKind::Assembler {
            // `AssemblerAI` (`AssemblerAI.java:11-24`, H01 T13): hold at the
            // arranged spot and face the locked angle.
            let state = world
                .get::<crate::ai::types::assembler::AssemblerAi>(entity)
                .copied()
                .unwrap_or_default();
            {
                let UnitRuntime {
                    content,
                    grid,
                    pathfinder,
                    ..
                } = &mut runtime;
                let mut ctx = crate::ai::ai_controller::AiCtx {
                    world,
                    grid,
                    content,
                    pathfinder,
                    team,
                };
                crate::ai::types::assembler::update_assembler(&mut ctx, entity, &state);
            }
            if let Some(mut stored) =
                world.get_mut::<crate::ai::types::assembler::AssemblerAi>(entity)
            {
                *stored = state;
            } else if world.get_entity(entity).is_ok() {
                world.entity_mut(entity).insert(state);
            }
        } else if slot.kind == AiKind::Logic {
            // `LogicAI` (T1): `AIController.updateUnit` runs
            // `updateTargeting()` (weapons) before `updateMovement()`.
            //
            // A bare `Logic` slot without the component (only reachable through
            // a decoded fixture that sets the kind directly; Java cannot reach
            // this state) is left inert.
            if let Some(mut state) = world
                .get::<crate::ai::types::logic::LogicAi>(entity)
                .cloned()
            {
                let can_boost = world
                    .get::<UnitTypeComp>(entity)
                    .and_then(|comp| runtime.content.unit(comp.type_id))
                    .is_some_and(|def| def.can_boost);
                let should_shoot = state.shoot && !(can_boost && state.boost);
                let explicit = matches!(
                    state.aim_control,
                    crate::logic::enums::LUnitControl::Target
                        | crate::logic::enums::LUnitControl::Targetp
                );
                let pos_target = (state.aim_control == crate::logic::enums::LUnitControl::Target)
                    .then_some(state.pos_target);
                let main_target = if state.aim_control == crate::logic::enums::LUnitControl::Targetp
                {
                    state.main_target
                } else {
                    None
                };
                logic_ai_weapon_pass(
                    world,
                    &runtime.content,
                    &queries,
                    entity,
                    team,
                    main_target,
                    pos_target,
                    should_shoot,
                    explicit,
                );
                let alive = {
                    // `controlPath.getPathPosition(unit, ...)` reads
                    // `unit.type.pathCostId` (ControlPathfinder.java:1137-1138).
                    let cost = unit_control_cost(world, &runtime.content, entity);
                    runtime.control_path(cost);
                    let UnitRuntime {
                        content,
                        grid,
                        pathfinder,
                        control_paths,
                        ..
                    } = &mut runtime;
                    let control_path = &mut control_paths[usize::from(cost.id())];
                    let mut ctx = crate::ai::ai_controller::AiCtx {
                        world,
                        grid,
                        content,
                        pathfinder,
                        team,
                    };
                    crate::ai::types::logic::update_logic(
                        &mut ctx,
                        entity,
                        &mut state,
                        control_path,
                    )
                };
                if alive {
                    if let Some(mut stored) =
                        world.get_mut::<crate::ai::types::logic::LogicAi>(entity)
                    {
                        *stored = state;
                    } else if world.get_entity(entity).is_ok() {
                        world.entity_mut(entity).insert(state);
                    }
                } else {
                    // `unit.resetController()` (the unit loop holds the
                    // resolved def; `reset_controller`'s `with_content` cannot
                    // see the `UnitRuntime` removed by this system).
                    let def = world
                        .get::<UnitTypeComp>(entity)
                        .and_then(|comp| runtime.content.unit(comp.type_id));
                    crate::logic::executor::unit_control::reset_controller_def(
                        world, entity, def, team,
                    );
                }
            }
        } else if slot.kind == AiKind::NoAi {
            // `NoAI.updateUnit(){}`: the dummy never runs a controller (the
            // kinematics/environment tail still runs, as in Java `Unit.update`).
        } else {
            let flying = slot.kind == AiKind::Flying
                || world
                    .get::<PhysicsComp>(entity)
                    .is_some_and(|physics| physics.flying);
            if flying {
                // `FlyingAI` (FlyingAI.java:16-72): main target via
                // `findMainTarget` (core in range, else `targetFlags`, else the
                // core), circle/close on it, shuttle to the spawner when idle
                // on the default team.
                update_flying_ai(
                    world,
                    &mut runtime,
                    &queries,
                    entity,
                    team,
                    slot,
                    ground_rules,
                );
            } else {
                // `GroundAI.updateMovement` (GroundAI.java:21-85): field walk to
                // the enemy-core set with the Java targeting cadence.
                update_ground_ai(
                    world,
                    &mut runtime,
                    &queries,
                    entity,
                    team,
                    slot,
                    ground_rules,
                    &mut core_target_cache,
                );
            }
        }

        // Kind kinematics (`Unit.update` tail: legs/mech/tank/crawl/water).
        if world.get_entity(entity).is_ok() {
            let after = world.get::<Pos>(entity).map(|pos| (pos.x, pos.y));
            let delta = match (before, after) {
                (Some(before), Some(after)) => (after.0 - before.0, after.1 - before.1),
                _ => (0.0, 0.0),
            };
            // Snapshot the legs before the kind pass so the splash pass can
            // tell which feet actually stepped this tick
            // (`LegsComp.java:196-202`).
            let legs_before = world
                .get::<crate::entities::comp::unit::LegsComp>(entity)
                .cloned();
            let _ =
                movement::update_kinematics(world, &runtime.grid, &runtime.content, entity, delta);
            // GAPS #10: leg splash + crawler crush (the damage half of the
            // kind updates, which needs `rules.unitDamage` and the content
            // registry).
            apply_movement_damage(
                world,
                &runtime.grid,
                &runtime.content,
                entity,
                delta,
                legs_before.as_ref(),
            );

            // H07 T3: `MechComp.update` step + `UnitComp.update` walk sounds
            // (view-only; no-op without an audio sink).
            crate::audio::steps::update_unit_step_audio(
                world,
                &runtime.grid,
                &runtime.content,
                &mut runtime.rng,
                entity,
                delta,
            );

            // `MinerComp.update` runs after the controller and the kind
            // kinematics for every unit (G01 T10a).
            crate::entities::comp::unit::miner::update_miner_comp(
                world,
                &runtime.grid,
                &runtime.content,
                entity,
            );
            // `UnitComp.update` environment half (G01 T13): bounds/drowning/
            // floor status/env death/drop-zone push/solid kill.
            crate::entities::comp::unit::environment::update_unit_environment(
                world,
                &runtime.grid,
                &runtime.content,
                runtime.wave_spawner.get_spawns(),
                has_spawns,
                entity,
            );

            // `TimedKillComp.update`: `time = min(time + delta, lifetime)`;
            // at the lifetime the unit is marked dead and `resolve_deaths`
            // runs the normal kill path (including `shootOnDeath`).
            if let Some(mut timed) = world.get_mut::<TimedKillComp>(entity) {
                timed.time = (timed.time + 1.0).min(timed.lifetime);
                if timed.lifetime > 0.0
                    && timed.time >= timed.lifetime
                    && let Some(mut health) = world.get_mut::<Health>(entity)
                {
                    health.health = 0.0;
                    health.dead = true;
                }
            }
        }
    }

    // Weapon state sync + firing pass (`UnitWeapons.update`).
    let units = runtime.units.clone();
    for entity in units {
        if world.get_entity(entity).is_ok() {
            sync_weapon_state(world, entity);
        }
    }
    let audio = world
        .remove_resource::<AudioSinkRes>()
        .unwrap_or_else(AudioSinkRes::noop);
    // H02 T11: shock mines queued by `unitOn` fire their tendrils in push
    // order (Java creates them inside the unit pass).
    let pending = world
        .remove_resource::<crate::world::behavior::PendingShockMineTriggers>()
        .unwrap_or_default();
    let mut spawned: Vec<Entity> = Vec::new();
    {
        let UnitRuntime {
            content,
            grid,
            rng,
            fx,
            seq,
            ..
        } = &mut runtime;
        let mut ctx = CombatCtx {
            world,
            content,
            grid,
            rng,
            fx: fx.as_ref(),
            audio: &audio,
            seq,
            spawned: &mut spawned,
        };
        for (mine, team) in pending.0 {
            let state = ctx
                .world
                .get::<crate::world::blocks::defense::shields::ShockMineState>(mine)
                .cloned();
            if let Some(state) = state {
                crate::world::blocks::defense::shields::shock_mine_trigger(
                    &mut ctx, mine, &state, team,
                );
            }
        }
        crate::weapons::update_weapons(&mut ctx);
    }
    world.insert_resource(audio);
    // Units spawned by `spawnUnit` bullets bypass `spawned` (no bullet entity).
    adopt_spawned_units(world, &mut runtime);
    runtime.bullets_created += spawned.len() as u64;
    runtime.bullets.extend(spawned);
    world.insert_non_send(runtime);
}

/// Leg splash + crawler crush damage — the damage half of the kind movement
/// bodies, run from the live unit loop after [`movement::update_kinematics`].
///
/// * `LegsComp.java:196-202`: on a step (`l.group` change) a legged unit deals
///   `legSplashDamage * rules.unitDamage(team)` in `legSplashRange` at the
///   foot's base, units and buildings included (`false, true` = no armor
///   pierce, air hits).
/// * `CrawlComp.java:56-93`: while moving, every tile in the crawl circle whose
///   building belongs to another team takes `crushDamage * Time.delta *
///   rules.unitDamage(team)`. `t.build.damage` is `BuildingComp.damage(float)`
///   (`BuildingComp.java:2064-2088`: `rules.blockHealth`, the status/shield
///   guards and death), i.e. [`crate::combat::damage::area::apply_health`].
///
/// The kind modules keep only geometry/scan state (`entities/comp/unit/{legs,
/// crawl}.rs`); the damage pass needs the content registry and the live rules,
/// so it lives here (GAPS #10).
fn apply_movement_damage(
    world: &mut World,
    grid: &WorldGrid,
    content: &ContentRegistry,
    entity: Entity,
    delta: (f32, f32),
    legs_before: Option<&crate::entities::comp::unit::LegsComp>,
) {
    if world
        .get::<crate::entities::comp::unit::comp::StatusComp>(entity)
        .is_some_and(|status| status.disarmed)
    {
        return;
    }
    let Some(def) = world
        .get::<crate::entities::comp::unit::comp::UnitTypeComp>(entity)
        .and_then(|comp| content.unit(comp.type_id))
    else {
        return;
    };
    let team = world
        .get::<TeamComp>(entity)
        .map(|team| team.team)
        .unwrap_or(0);
    let unit_damage = crate::game::runtime::rules_of(world)
        .map(|rules| rules.unit_damage(team))
        .unwrap_or(1.0);

    if def.leg_splash_damage > 0.0
        && let Some(before) = legs_before
    {
        // A leg stepped this tick when its foot moved (`update_legs` snaps a
        // foot to its destination on a re-step).
        let stepped: Vec<(f32, f32)> = world
            .get::<crate::entities::comp::unit::LegsComp>(entity)
            .map(|after| {
                after
                    .legs
                    .iter()
                    .enumerate()
                    .filter(|(index, leg)| {
                        before.legs.get(*index).is_none_or(|old| {
                            (old.foot_x - leg.foot_x).abs() > 0.0001
                                || (old.foot_y - leg.foot_y).abs() > 0.0001
                        })
                    })
                    .map(|(_, leg)| (leg.base_x, leg.base_y))
                    .collect()
            })
            .unwrap_or_default();
        for (x, y) in stepped {
            crate::combat::damage::area::damage_area(
                world,
                content,
                Some(team),
                x,
                y,
                def.leg_splash_range,
                def.leg_splash_damage * unit_damage,
                crate::combat::damage::area::DamageOptions::default(),
            );
        }
    }

    if def.crush_damage > 0.0
        && world
            .get::<crate::entities::comp::unit::CrawlComp>(entity)
            .is_some()
    {
        if (delta.0 * delta.0 + delta.1 * delta.1).sqrt() <= 0.001 {
            return;
        }
        let Some(pos) = world.get::<Pos>(entity).copied() else {
            return;
        };
        let hit_size = world
            .get::<crate::entities::comp::unit::comp::HitboxComp>(entity)
            .map(|hitbox| hitbox.hit_size)
            .unwrap_or(0.0);
        let tile = crate::config::TILESIZE as f32;
        // `int radius = (int)Math.max(0, hitSize / tilesize * 2f)`.
        let radius = (hit_size / tile * 2.0).max(0.0) as i32;
        let mut targets: Vec<Entity> = Vec::new();
        for cx in -radius..=radius {
            for cy in -radius..=radius {
                if cx * cx + cy * cy > radius {
                    continue;
                }
                let tx = WorldGrid::to_tile(pos.x + cx as f32 * tile);
                let ty = WorldGrid::to_tile(pos.y + cy as f32 * tile);
                let Some(build) = grid.tiles.getn(tx, ty).and_then(|t| t.build) else {
                    continue;
                };
                if world.get::<TeamComp>(build).map(|t| t.team) != Some(team) {
                    targets.push(build);
                }
            }
        }
        // Overlapping tile samples can name the same building; damage it once.
        targets.sort_by_key(|entity| entity.index());
        targets.dedup();
        for build in targets {
            crate::combat::damage::area::apply_health(world, build, def.crush_damage * unit_damage);
        }
    }
}

/// Adopts units spawned by `spawnUnit` bullets (`BulletType.create` returns no
/// bullet for them, so the bullet drain cannot see them) into the live unit
/// list. Call after any pass that can create bullets; ordering is
/// `(EntitySeq, entity.index())`.
fn adopt_spawned_units(world: &mut World, runtime: &mut UnitRuntime) {
    // T9: query only the `SpawnedByBullet` archetype instead of scanning every
    // entity in the world. Ordering is restored by the sort below.
    use bevy_ecs::query::With;
    let mut query = world
        .query_filtered::<(Entity, Option<&EntitySeq>), With<bullet::spawn::SpawnedByBullet>>();
    let mut found: Vec<(u64, Entity)> = query
        .iter(world)
        .map(|(entity, seq)| (seq.map_or(u64::MAX, |seq| seq.0), entity))
        .collect();
    found.sort_unstable_by_key(|(seq, entity)| (*seq, entity.index()));
    for (_, entity) in found {
        // `Logic`'s `UnitCreateEvent` (`Logic.java:228-235`) for payload/
        // ability spawns (H09 T10); the runtime is out of the world here, so
        // the type name comes from the local one.
        let name = world
            .get::<crate::entities::comp::unit::comp::UnitTypeComp>(entity)
            .and_then(|comp| runtime.content.unit(comp.type_id))
            .map(|def| def.name.clone());
        crate::game::stats::notify_unit_created(world, entity, name.as_deref());
        world
            .entity_mut(entity)
            .remove::<crate::combat::bullet::spawn::SpawnedByBullet>();
        runtime.units.push(entity);
        runtime.units_created += 1;
    }
}

/// `EntitySet::UpdateBuildings` turret half: `Turret.TurretBuild.updateTile`
/// for every placed turret (`Groups.build.update`). Runs after the generic
/// building pass; bullets fired here join the live bullet list (PAR-0086).
pub fn turret_update_system(world: &mut World) {
    let Some(mut runtime) = world.remove_non_send::<UnitRuntime>() else {
        return;
    };
    let audio = world
        .remove_resource::<AudioSinkRes>()
        .unwrap_or_else(AudioSinkRes::noop);
    let mut spawned: Vec<Entity> = Vec::new();
    {
        let UnitRuntime {
            content,
            grid,
            rng,
            fx,
            seq,
            ..
        } = &mut runtime;
        let mut ctx = CombatCtx {
            world,
            content,
            grid,
            rng,
            fx: fx.as_ref(),
            audio: &audio,
            seq,
            spawned: &mut spawned,
        };
        crate::world::blocks::defense::turrets::update_turrets(&mut ctx);
    }
    world.insert_resource(audio);
    // Units spawned by `spawnUnit` bullets bypass `spawned` (no bullet entity).
    adopt_spawned_units(world, &mut runtime);
    runtime.bullets_created += spawned.len() as u64;
    runtime.bullets.extend(spawned);
    world.insert_non_send(runtime);
}

/// `EntitySet::UpdateBullets` system: `Groups.bullet.update()` motion half.
pub fn bullet_update_system(world: &mut World) {
    let Some(mut runtime) = world.remove_non_send::<UnitRuntime>() else {
        return;
    };
    runtime.bullets.retain(|entity| {
        world.get_entity(*entity).is_ok() && world.get::<bullet::Bullet>(*entity).is_some()
    });
    if runtime.bullets.is_empty() {
        world.insert_non_send(runtime);
        return;
    }
    let list = runtime.bullets.clone();
    let audio = world
        .remove_resource::<AudioSinkRes>()
        .unwrap_or_else(AudioSinkRes::noop);
    let mut spawned: Vec<Entity> = Vec::new();
    {
        let UnitRuntime {
            content,
            grid,
            rng,
            fx,
            seq,
            bullets,
            bullets_removed,
            ..
        } = &mut runtime;
        let mut ctx = CombatCtx {
            world,
            content,
            grid,
            rng,
            fx: fx.as_ref(),
            audio: &audio,
            seq,
            spawned: &mut spawned,
        };
        let mut alive: Vec<Entity> = Vec::with_capacity(list.len());
        for &entity in &list {
            if ctx.world.get_entity(entity).is_err() {
                *bullets_removed += 1;
                continue;
            }
            let _ = bullet::update_bullet(&mut ctx, entity);
            if bullet::bullet_alive(ctx.world, entity) {
                alive.push(entity);
            } else {
                bullet::finish_bullet(&mut ctx, entity);
                *bullets_removed += 1;
            }
        }
        *bullets = alive;
    }
    world.insert_resource(audio);
    // Units spawned by `spawnUnit` bullets bypass `spawned` (no bullet entity).
    adopt_spawned_units(world, &mut runtime);
    runtime.bullets_created += spawned.len() as u64;
    runtime.bullets.extend(spawned);
    world.insert_non_send(runtime);
}

/// `EntitySet::CollideBullets` system: `Groups.bullet.collide()`.
pub fn bullet_collide_system(world: &mut World) {
    let Some(mut runtime) = world.remove_non_send::<UnitRuntime>() else {
        return;
    };
    if runtime.bullets.is_empty() {
        world.insert_non_send(runtime);
        return;
    }
    let list = runtime.bullets.clone();
    let audio = world
        .remove_resource::<AudioSinkRes>()
        .unwrap_or_else(AudioSinkRes::noop);
    let mut spawned: Vec<Entity> = Vec::new();
    {
        let UnitRuntime {
            content,
            grid,
            rng,
            fx,
            seq,
            bullets,
            bullets_removed,
            units,
            ..
        } = &mut runtime;
        let units: &[Entity] = units;
        let mut ctx = CombatCtx {
            world,
            content,
            grid,
            rng,
            fx: fx.as_ref(),
            audio: &audio,
            seq,
            spawned: &mut spawned,
        };
        for &entity in &list {
            if ctx.world.get_entity(entity).is_ok() && bullet::bullet_alive(ctx.world, entity) {
                let _ = bullet::collide_bullet(&mut ctx, entity);
            }
            // `Groups.bullet.collide()` unit half (PAR-0141): direct hits on
            // enemy units (`BulletComp.collision` -> `hitEntity`).
            if ctx.world.get_entity(entity).is_ok() && bullet::bullet_alive(ctx.world, entity) {
                let _ = bullet::collide_units(&mut ctx, entity, units);
            }
        }
        let mut alive: Vec<Entity> = Vec::with_capacity(list.len());
        for &entity in &list {
            if ctx.world.get_entity(entity).is_err() {
                *bullets_removed += 1;
            } else if bullet::bullet_alive(ctx.world, entity) {
                alive.push(entity);
            } else {
                bullet::finish_bullet(&mut ctx, entity);
                *bullets_removed += 1;
            }
        }
        *bullets = alive;
    }
    world.insert_resource(audio);
    // Units spawned by `spawnUnit` bullets bypass `spawned` (no bullet entity).
    adopt_spawned_units(world, &mut runtime);
    runtime.bullets_created += spawned.len() as u64;
    runtime.bullets.extend(spawned);
    world.insert_non_send(runtime);
}

/// `TickSet::RunWave` system: advances the spawn window and emits the pending
/// wave's groups through `WaveSpawner` (`Logic.runWave` + `WaveSpawner.update`).
/// One `SpawnGroup.createUnit` + `WaveSpawner.spawnEffect` emission
/// (`WaveSpawner.java:224-247`): the extracted per-unit body of
/// `run_wave_system`'s closure, shared with the positional `spawnwave` path
/// (`LExecutor.java:2190-2195`, H06 T2).
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_group_unit(
    world: &mut World,
    content: &ContentRegistry,
    seq: &mut u64,
    group: &SpawnGroup,
    team: u8,
    x: f32,
    y: f32,
    rotation: f32,
    wave: i32,
    unmoving: Option<crate::content::StatusId>,
    invincible: Option<crate::content::StatusId>,
) -> Option<Entity> {
    let entity = group.create_unit(world, content, *seq, team, x, y, rotation, wave)?;
    if let Some(status) = unmoving {
        crate::combat::damage::status::apply_status(
            world,
            entity,
            status,
            SPAWN_EFFECT_UNMOVING_TICKS,
        );
    }
    if let Some(status) = invincible {
        crate::combat::damage::status::apply_status(
            world,
            entity,
            status,
            SPAWN_EFFECT_INVINCIBLE_TICKS,
        );
    }
    let payloads = group
        .payloads
        .as_ref()
        .map_or(0, |payloads| payloads.len() as u64);
    *seq = seq.wrapping_add(1 + payloads);
    Some(entity)
}

pub fn run_wave_system(world: &mut World) {
    let Some(mut runtime) = world.remove_non_send::<UnitRuntime>() else {
        return;
    };
    runtime.wave_spawner.update();
    let Some(wave) = runtime.pending_wave.take() else {
        world.insert_non_send(runtime);
        return;
    };

    // `SpawnGroup.getSpawned(state.wave - 1)`: the pending value is the
    // pre-increment wave minus one (`WaveSpawner.java:74`;
    // `MindCampaign.run_wave`/the campaign timer return it from
    // `run_wave_campaign`).
    let (groups, wave_team) = match world.get_resource::<CampaignRuntime>() {
        Some(campaign) if !campaign.session.rules.spawns.is_empty() => (
            groups_from_rules(&campaign.session.rules),
            campaign.session.wave_team(),
        ),
        Some(campaign) => (runtime.waves.get().to_vec(), campaign.session.wave_team()),
        None => (runtime.waves.get().to_vec(), runtime.wave_team),
    };
    runtime.wave_team = wave_team;
    runtime.path_team = wave_team;
    let core_capture = world
        .get_resource::<CampaignRuntime>()
        .is_some_and(|campaign| campaign.session.rules.core_capture);
    {
        let UnitRuntime {
            grid,
            content,
            pathfinder,
            ..
        } = &mut runtime;
        pathfinder.rebuild(grid, content, world, core_capture);
    }

    // `WaveSpawner` spawn tiles are the `spawn` overlay tiles of the live grid
    // (`TileOverlayChangeEvent` rebuild covers map loads here).
    let mut spawns: Vec<TilePos> = runtime
        .grid
        .tiles
        .array()
        .iter()
        .filter(|tile| {
            runtime
                .content
                .block(tile.overlay)
                .is_some_and(|def| def.kind == BlockKind::SpawnBlock)
        })
        .map(|tile| TilePos::new(tile.x, tile.y))
        .collect();

    // `WaveSpawner.eachGroundSpawn`/`eachFlyerSpawn` context
    // (WaveSpawner.java:122-200). Overlay ground spawns pass `worldx() = x*8`
    // (corner) and take the drop-zone shockwave; attack-mode core spawns keep
    // `shockwave = false`. Flyers spawn at the clamped map edge unless
    // `rules.airUseSpawns`, plus the wave-team cores in attack mode.
    //
    // The core branch (`WaveSpawner.java:134-173`): with `rules.wavesSpawnAtCores`
    // in attack mode every wave-team core is also a spawn position, stepped
    // toward the first player core until the 3x3 tile area is clear (PAR-0139
    // prerequisite: attack sectors author no spawn-overlay tiles).
    let width = runtime.grid.width();
    let height = runtime.grid.height();
    let air_use_spawns = world
        .get_resource::<CampaignRuntime>()
        .is_some_and(|campaign| campaign.session.rules.air_use_spawns);
    let ts = TILE_SIZE;
    let mut ctx = SpawnContext {
        width_px: width as f32 * ts,
        height_px: height as f32 * ts,
        ground: Vec::new(),
        flyer: Vec::new(),
        campaign_mult: world
            .get_resource::<CampaignRuntime>()
            .and_then(|campaign| {
                campaign
                    .session
                    .is_campaign()
                    .then_some(campaign.session.difficulty_enemy_spawn_multiplier)
            }),
    };
    for tile in &spawns {
        ctx.ground.push((
            tile.x() as f32 * ts,
            tile.y() as f32 * ts,
            tile.pack(),
            true,
        ));
        if air_use_spawns {
            ctx.flyer
                .push((tile.x() as f32 * ts, tile.y() as f32 * ts, tile.pack()));
        } else {
            let (fx, fy) = flyer_edge_position(width, height, *tile);
            ctx.flyer.push((fx, fy, tile.pack()));
        }
    }
    if let Some(campaign) = world.get_resource::<CampaignRuntime>() {
        let rules = campaign.session.rules.clone();
        if rules.waves_spawn_at_cores && rules.attack_mode {
            let player_core = campaign
                .session
                .teams
                .get_or_null(crate::ecs::TeamId(rules.default_team))
                .and_then(|data| data.cores.first().copied())
                .and_then(|core| world.get::<Pos>(core).map(|pos| (pos.x, pos.y)));
            if let Some((px, py)) = player_core {
                let wave_cores: Vec<Entity> = campaign
                    .session
                    .teams
                    .get_or_null(crate::ecs::TeamId(wave_team))
                    .map(|data| data.cores.clone())
                    .unwrap_or_default();
                for core in wave_cores {
                    let Some(pos) = world.get::<Pos>(core).copied() else {
                        continue;
                    };
                    let packed = world
                        .get::<BuildingComp>(core)
                        .map(|comp| comp.pos.pack())
                        .unwrap_or(-1);
                    // `cons.accept(core.x, core.y, false)` (flyer branch) uses
                    // the raw core position; the ground branch steps toward the
                    // player core (`core_spawn_position`).
                    ctx.flyer.push((pos.x, pos.y, packed));
                    if let Some(tile) =
                        core_spawn_position(&runtime.grid, &runtime.content, pos.x, pos.y, px, py)
                    {
                        ctx.ground.push((
                            tile.x() as f32 * ts,
                            tile.y() as f32 * ts,
                            packed,
                            false,
                        ));
                        // Keep the public spawn-tile list complete (T6a's
                        // `closest_spawn_tile`, `playerNear`).
                        spawns.push(tile);
                    }
                }
            }
        }
    }

    // `doShockwave` (`WaveSpawner.java:104-106`): 99,999,999 damage over the
    // drop-zone radius on every overlay spawn, before any unit is emitted.
    let drop_zone_radius = world
        .get_resource::<CampaignRuntime>()
        .map(|campaign| campaign.session.rules.drop_zone_radius)
        .unwrap_or(300.0);
    let shock_targets: Vec<(f32, f32)> = ctx
        .ground
        .iter()
        .filter_map(|&(x, y, _, shockwave)| shockwave.then_some((x, y)))
        .collect();
    for (x, y) in shock_targets {
        crate::combat::damage::area::damage_area(
            world,
            &runtime.content,
            Some(wave_team),
            x,
            y,
            drop_zone_radius,
            99_999_999.0,
            crate::combat::damage::area::DamageOptions::default(),
        );
    }
    runtime.wave_spawner.set_spawns(spawns);
    log::info!(
        "wave spawn: wave {wave} with {} group(s), {} ground / {} flyer position(s)",
        groups.len(),
        ctx.ground.len(),
        ctx.flyer.len(),
    );

    let UnitRuntime {
        wave_spawner,
        content,
        seq,
        units,
        units_created,
        rng,
        ..
    } = &mut runtime;
    // `WaveSpawner.spawnEffect` (WaveSpawner.java:228-235): unmoving 30,
    // invincible 60 on every spawned unit.
    let unmoving = content.status_id("unmoving");
    let invincible = content.status_id("invincible");
    let mut spawned: Vec<Entity> = Vec::new();
    let emitted = wave_spawner.spawn_enemies(
        &groups,
        wave,
        &ctx,
        content,
        rng,
        |group, x, y, rotation| {
            let team = group.team.unwrap_or(wave_team);
            let entity = spawn_group_unit(
                world, content, seq, group, team, x, y, rotation, wave, unmoving, invincible,
            )?;
            spawned.push(entity);
            Some(entity)
        },
    );
    units.extend(spawned);
    *units_created += u64::from(emitted);
    log::info!("wave spawn: emitted {emitted} unit(s) for wave {wave}");
    world.insert_non_send(runtime);
}

impl Sim {
    /// Live selectable-unit snapshot for the RTS input layer
    /// (`[{id,type,x,y,team,commandable}]`; `id` is the unit's `EntitySeq`).
    ///
    /// Published to `MindInput.set_selectable_units_json` every frame so
    /// drag/tap selection and command mode have a list (PAR-0140). Upstream's
    /// selectable list is `Units`-backed, so both teams are listed (the input
    /// filters by team/`commandable`); `commandable` is `Unit.isCommandable()`
    /// (`controller instanceof CommandAI`) on the live default team, so the
    /// player's own unit (`Player` controller) is never selectable.
    pub fn selectable_units_json(&self) -> String {
        let Some(runtime) = self.ecs.0.get_non_send::<UnitRuntime>() else {
            return String::from("[]");
        };
        let default_team = self.live_default_team();
        let mut rows: Vec<serde_json::Value> = Vec::new();
        for &entity in &runtime.units {
            let Ok(entity_ref) = self.ecs.0.get_entity(entity) else {
                continue;
            };
            if entity_ref.get::<UnitCore>().is_some_and(|core| core.dead) {
                continue;
            }
            let (Some(seq), Some(pos), Some(team), Some(type_comp)) = (
                entity_ref.get::<EntitySeq>(),
                entity_ref.get::<Pos>(),
                entity_ref.get::<TeamComp>(),
                entity_ref.get::<UnitTypeComp>(),
            ) else {
                continue;
            };
            let commandable = team.team == default_team
                && entity_ref
                    .get::<ControllerSlot>()
                    .is_some_and(|slot| slot.kind == AiKind::Command);
            let hit_size = runtime
                .content
                .unit(type_comp.type_id)
                .map_or(0.0, |def| def.hit_size);
            // `instanceof Payloadc` (transport select) and
            // `UnitType.controlSelectGlobal` (H08 T6a consumers).
            let payload = entity_ref
                .get::<crate::entities::comp::unit::comp::UnitPayloads>()
                .is_some();
            let select_global = runtime
                .content
                .unit(type_comp.type_id)
                .is_none_or(|def| def.control_select_global);
            rows.push(serde_json::json!({
                "id": seq.0 as i32,
                "type": type_comp.type_id.get() as i32,
                "x": pos.x,
                "y": pos.y,
                "team": team.team,
                "commandable": commandable,
                "hit_size": hit_size,
                "payload": payload,
                "select_global": select_global,
            }));
        }
        serde_json::to_string(&rows).unwrap_or_else(|_| String::from("[]"))
    }

    /// `MapObjectives.CommandModeObjective.update` (`MapObjectives.java:671-675`):
    /// any live unit whose `EntitySeq` is in `ids`, on `live_default_team()`,
    /// has a `Command` controller with a live order target
    /// (`CommandAI.hasCommand()` = `targetPos != null`).
    pub fn any_unit_has_command(&self, ids: &[i32]) -> bool {
        if ids.is_empty() {
            return false;
        }
        let team = self.live_default_team();
        self.ecs.0.iter_entities().any(|entity_ref| {
            let Some(seq) = entity_ref.get::<EntitySeq>() else {
                return false;
            };
            if !ids.contains(&(seq.0 as i32)) {
                return false;
            }
            if entity_ref.get::<TeamComp>().map(|team_comp| team_comp.team) != Some(team) {
                return false;
            }
            entity_ref
                .get::<ControllerSlot>()
                .is_some_and(|slot| slot.kind == AiKind::Command && slot.target.is_some())
        })
    }

    /// Debug-only live unit dump (`id`, `type`, `team`, `kind`, position,
    /// rotation, health, `can_shoot` and per-mount aim/fire state); MCP probes
    /// only, never saved or synced.
    pub fn unit_debug_json(&self) -> String {
        let Some(runtime) = self.ecs.0.get_non_send::<UnitRuntime>() else {
            return String::from("[]");
        };
        let mut rows: Vec<serde_json::Value> = Vec::new();
        for &entity in &runtime.units {
            let Ok(entity_ref) = self.ecs.0.get_entity(entity) else {
                continue;
            };
            let Some(seq) = entity_ref.get::<EntitySeq>() else {
                continue;
            };
            let type_name = entity_ref
                .get::<UnitTypeComp>()
                .and_then(|comp| runtime.content.unit(comp.type_id))
                .map(|def| def.name.clone())
                .unwrap_or_default();
            let mounts: Vec<serde_json::Value> = entity_ref
                .get::<crate::weapons::UnitWeapons>()
                .map(|weapons| {
                    weapons
                        .mounts
                        .iter()
                        .map(|mount| {
                            serde_json::json!({
                                "shoot": mount.shoot,
                                "aim": [mount.aim_x, mount.aim_y],
                                "rotation": mount.rotation,
                                "target_rotation": mount.target_rotation,
                                "reload": mount.reload,
                                "warmup": mount.warmup,
                                "total_shots": mount.total_shots,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            // Active status names (G07 T9 readback; append-only field).
            let statuses: Vec<String> = entity_ref
                .get::<crate::entities::comp::unit::comp::StatusComp>()
                .map(|comp| {
                    comp.statuses
                        .iter()
                        .map(|status| {
                            runtime
                                .content
                                .status(status.effect)
                                .map(|def| def.name.clone())
                                .unwrap_or_default()
                        })
                        .collect()
                })
                .unwrap_or_default();
            rows.push(serde_json::json!({
                "id": seq.0 as i32,
                "type": type_name,
                "team": entity_ref.get::<TeamComp>().map(|team| team.team).unwrap_or(0),
                "kind": entity_ref
                    .get::<ControllerSlot>()
                    .map(|slot| slot.kind.name())
                    .unwrap_or("?"),
                "pos": entity_ref.get::<Pos>().map(|pos| [pos.x, pos.y]),
                "rotation": entity_ref.get::<UnitCore>().map(|core| core.rotation),
                // `UnitCore.elevation` (H08 T7 / PAR-0137 evidence).
                "elevation": entity_ref.get::<UnitCore>().map(|core| core.elevation),
                "health": entity_ref.get::<Health>().map(|health| health.health),
                "can_shoot": entity_ref.get::<crate::weapons::UnitState>().map(|state| state.can_shoot),
                "vel": entity_ref.get::<crate::entities::comp::Vel>().map(|vel| [vel.x, vel.y]),
                "statuses": statuses,
                "mounts": mounts,
            }));
        }
        serde_json::to_string(&rows).unwrap_or_else(|_| String::from("[]"))
    }
}

impl Sim {
    /// `Payloadc` unit carrier command (`InputHandler.requestUnitPayload` /
    /// `requestBuildPayload` / `requestDropPayload`; PAR-0138).
    ///
    /// `kind`: `0` pick up a unit (`target` = its `EntitySeq`), `1` pick up the
    /// building at world `(x, y)`, `2` drop the last payload at `(x, y)`.
    /// Returns whether the carrier's payload stack changed.
    pub fn apply_payload_command(&mut self, kind: u8, x: f32, y: f32, target: Option<i32>) -> bool {
        let Some(carrier) = self.player_unit() else {
            return false;
        };
        // The drop sound needs the payload about to leave the stack
        // (`PayloadComp.playPayloadDropSound`), read before the drop.
        let dropped = if kind == 2 {
            self.ecs
                .0
                .get::<crate::entities::comp::unit::comp::UnitPayloads>(carrier)
                .and_then(|payloads| payloads.stack.last().copied())
        } else {
            None
        };
        let carrier_pos = self.ecs.0.get::<Pos>(carrier).map(|pos| (pos.x, pos.y));
        let changed = match kind {
            0 => self.payload_pickup_unit(carrier, x, y, target),
            1 => self.payload_pickup_build(carrier, x, y),
            2 => self.payload_drop(carrier, x, y),
            _ => false,
        };
        // G07 T14: `PayloadComp.pickup`/`playPayloadDropSound` sounds.
        if changed && let Some(audio) = self.ecs.0.get_resource::<AudioSinkRes>().cloned() {
            let (cx, cy) = carrier_pos.unwrap_or((x, y));
            let fx = self
                .rng_streams
                .next_float(crate::determinism::RngStream::Fx);
            let at = |sound: crate::audio::SoundId, pitch: f32| {
                audio.emit(crate::audio::AudioEvent::at(
                    sound, cx, cy, pitch, 1.0, true,
                ));
            };
            match kind {
                // `PayloadComp.pickup(Unit)` -> `payloadPickup.at(self, random(0.9, 1.1))`.
                0 => at(crate::audio::SoundId::PAYLOAD_PICKUP, 0.9 + fx * 0.2),
                // `PayloadComp.pickup(Building)` -> pitch 1 (`PayloadComp.java:117`).
                1 => at(crate::audio::SoundId::PAYLOAD_PICKUP, 1.0),
                2 => {
                    // Size: unit `hitSize`, build `block.size * tilesize`
                    // (`Payload.size()`). Deviation: the port cannot tell a
                    // ground placement from an acceptor hand-off, so the
                    // drop-size rule covers both.
                    let size = match dropped {
                        Some(crate::entities::comp::unit::comp::CarriedPayload::Unit {
                            type_id,
                            ..
                        }) => self
                            .unit_runtime()
                            .and_then(|runtime| runtime.content.unit(type_id))
                            .map(|def| def.hit_size)
                            .unwrap_or(0.0),
                        Some(crate::entities::comp::unit::comp::CarriedPayload::Build {
                            block,
                            ..
                        }) => self
                            .unit_runtime()
                            .and_then(|runtime| runtime.content.block(block))
                            .map(|def| def.size as f32 * 8.0)
                            .unwrap_or(0.0),
                        None => 0.0,
                    };
                    let sound = if size <= 12.0 {
                        crate::audio::SoundId::PAYLOAD_DROP1
                    } else if size <= 20.0 {
                        crate::audio::SoundId::PAYLOAD_DROP2
                    } else {
                        crate::audio::SoundId::PAYLOAD_DROP3
                    };
                    at(sound, 0.9 + fx * 0.2);
                }
                _ => {}
            }
        }
        changed
    }

    /// `InputHandler.requestUnitPayload` + `PayloadComp.canPickup(Unit)`.
    ///
    /// With no explicit `target`, resolves the candidate like
    /// `InputHandler.tryPickupPayload`: the closest unit to the cursor point
    /// within the carrier's reach (`carrier.hitSize * 2`).
    pub(crate) fn payload_pickup_unit(
        &mut self,
        carrier: Entity,
        x: f32,
        y: f32,
        target: Option<i32>,
    ) -> bool {
        let ecs = &mut self.ecs.0;
        let Some(mut runtime) = ecs.remove_non_send::<UnitRuntime>() else {
            return false;
        };
        let mut changed = false;
        let candidates: Vec<Entity> = if let Some(target_seq) = target {
            runtime
                .units
                .iter()
                .copied()
                .filter(|entity| {
                    ecs.get::<EntitySeq>(*entity)
                        .is_some_and(|seq| seq.0 as i32 == target_seq)
                })
                .collect()
        } else {
            let carrier_hit = ecs
                .get::<crate::entities::comp::unit::comp::HitboxComp>(carrier)
                .map(|hitbox| hitbox.hit_size)
                .unwrap_or(0.0);
            // `Units.closest(team, pay.x(), pay.y(), hitSize * 2f, ...)`: the
            // search is centered on the carrier, not the cursor
            // (`InputHandler.tryPickupPayload`, InputHandler.java:1043).
            let (cx, cy) = ecs
                .get::<Pos>(carrier)
                .map(|pos| (pos.x, pos.y))
                .unwrap_or((x, y));
            let mut near: Vec<(f32, Entity)> = runtime
                .units
                .iter()
                .copied()
                .filter(|entity| *entity != carrier)
                .filter_map(|entity| {
                    let pos = ecs.get::<Pos>(entity)?;
                    let (dx, dy) = (pos.x - cx, pos.y - cy);
                    let dist2 = dx * dx + dy * dy;
                    (dist2 <= (carrier_hit * 2.0).powi(2)).then_some((dist2, entity))
                })
                .collect();
            near.sort_by_key(|(dist2, entity)| (dist2.to_bits(), entity.index()));
            near.into_iter().map(|(_, entity)| entity).collect()
        };
        for target in candidates {
            if target == carrier {
                continue;
            }
            let team_of = |entity: Entity| ecs.get::<TeamComp>(entity).map(|t| t.team);
            let carrier_team = team_of(carrier).unwrap_or(0);
            let target_team = team_of(target).unwrap_or(0);
            let target_ai = ecs
                .get::<ControllerSlot>(target)
                .is_some_and(|slot| slot.kind != AiKind::Player);
            let target_grounded = ecs
                .get::<UnitCore>(target)
                .is_some_and(|core| core.elevation < 0.001);
            let (target_id, target_hit, target_allowed) = ecs
                .get::<UnitTypeComp>(target)
                .and_then(|comp| runtime.content.unit(comp.type_id))
                .map(|def| (def.id, def.hit_size, def.allowed_in_payloads))
                .unwrap_or((crate::content::UnitTypeId::new(0), f32::MAX, false));
            let carrier_hit = ecs
                .get::<crate::entities::comp::unit::comp::HitboxComp>(carrier)
                .map(|hitbox| hitbox.hit_size)
                .unwrap_or(0.0);
            let capacity = ecs
                .get::<crate::entities::comp::unit::comp::PayloadComp>(carrier)
                .map(|payload| payload.capacity)
                .unwrap_or(0.0);
            let used = ecs
                .get::<crate::entities::comp::unit::comp::UnitPayloads>(carrier)
                .map(|payloads| payloads.payload_used(&runtime.content))
                .unwrap_or(0.0);
            let within = match (
                ecs.get::<Pos>(carrier).copied(),
                ecs.get::<Pos>(target).copied(),
            ) {
                (Some(carrier_pos), Some(target_pos)) => {
                    let (dx, dy) = (target_pos.x - carrier_pos.x, target_pos.y - carrier_pos.y);
                    // `u.within(unit, u.hitSize + unit.hitSize)`
                    // (InputHandler.java:1043).
                    (dx * dx + dy * dy).sqrt() <= carrier_hit + target_hit
                }
                _ => false,
            };
            // `PayloadComp.canPickup(Unit)`: `type.pickupUnits` gate.
            let pickup_units = ecs
                .get::<UnitTypeComp>(carrier)
                .and_then(|comp| runtime.content.unit(comp.type_id))
                .is_some_and(|def| def.pickup_units);
            if !pickup_units
                || carrier_team != target_team
                || !target_ai
                || !target_grounded
                || !target_allowed
                || !within
                || used + target_hit * target_hit > capacity + 0.001
            {
                continue;
            }
            let health = ecs.get::<Health>(target).map(|h| h.health).unwrap_or(0.0);
            let rotation = ecs
                .get::<UnitCore>(target)
                .map(|c| c.rotation)
                .unwrap_or(0.0);
            runtime.units.retain(|entity| *entity != target);
            runtime.units_removed += 1;
            let _ = ecs.despawn(target);
            if let Some(mut payloads) =
                ecs.get_mut::<crate::entities::comp::unit::comp::UnitPayloads>(carrier)
            {
                payloads
                    .stack
                    .push(crate::entities::comp::unit::comp::CarriedPayload::Unit {
                        type_id: target_id,
                        team: target_team,
                        health,
                        rotation,
                    });
                changed = true;
            }
            break;
        }
        ecs.insert_non_send(runtime);
        if changed {
            return true;
        }
        // `else { Building build = world.buildWorld(pay.x(), pay.y()); ... }`
        // (`InputHandler.java:1045-1052`): fall back to the building under the
        // carrier.
        let (cx, cy) = ecs
            .get::<Pos>(carrier)
            .map(|pos| (pos.x, pos.y))
            .unwrap_or((x, y));
        self.payload_pickup_build(carrier, cx, cy)
    }

    /// `InputHandler.requestBuildPayload` + `PayloadComp.canPickup(Building)`.
    pub(crate) fn payload_pickup_build(&mut self, carrier: Entity, x: f32, y: f32) -> bool {
        let tx = WorldGrid::to_tile(x) as i16;
        let ty = WorldGrid::to_tile(y) as i16;
        let Some(build) = self
            .grid
            .tiles
            .getn(tx as i32, ty as i32)
            .and_then(|tile| tile.build)
        else {
            return false;
        };
        let Some(building) = self.ecs.0.get::<crate::entities::comp::Building>(build) else {
            return false;
        };
        let (block, rotation) = (building.block, building.rotation);
        let build_team = self
            .ecs
            .0
            .get::<TeamComp>(build)
            .map(|team| team.team)
            .unwrap_or(0);
        let health = self
            .ecs
            .0
            .get::<Health>(build)
            .map(|h| h.health)
            .unwrap_or(0.0);

        let ecs = &mut self.ecs.0;
        let Some(runtime) = ecs.remove_non_send::<UnitRuntime>() else {
            return false;
        };
        let carrier_team = ecs.get::<TeamComp>(carrier).map(|t| t.team).unwrap_or(0);
        let carrier_pos = ecs.get::<Pos>(carrier).copied();
        let carrier_hit = ecs
            .get::<crate::entities::comp::unit::comp::HitboxComp>(carrier)
            .map(|hitbox| hitbox.hit_size)
            .unwrap_or(0.0);
        let capacity = ecs
            .get::<crate::entities::comp::unit::comp::PayloadComp>(carrier)
            .map(|payload| payload.capacity)
            .unwrap_or(0.0);
        let used = ecs
            .get::<crate::entities::comp::unit::comp::UnitPayloads>(carrier)
            .map(|payloads| payloads.payload_used(&runtime.content))
            .unwrap_or(0.0);
        let (size, pickable) = runtime
            .content
            .block(block)
            .map(|def| {
                (
                    def.size as f32 * crate::config::TILESIZE as f32,
                    crate::world::build::block_can_pickup(def),
                )
            })
            .unwrap_or((0.0, false));
        let tile_size = crate::config::TILESIZE as f32;
        let within = match (carrier_pos, building_center(ecs, build)) {
            (Some(carrier_pos), Some((bx, by))) => {
                let (dx, dy) = (bx - carrier_pos.x, by - carrier_pos.y);
                (dx * dx + dy * dy).sqrt() <= size * 1.2 + tile_size * 5.0
            }
            _ => false,
        };
        let mut changed = false;
        if build_team == carrier_team
            && pickable
            && within
            && used + size * size <= capacity + 0.001
            && let Some(rt) = self.block_runtime.as_mut()
            && rt
                .pickup_building(&mut self.ecs.0, &mut self.grid, build)
                .is_some()
        {
            if let Some(mut payloads) = self
                .ecs
                .0
                .get_mut::<crate::entities::comp::unit::comp::UnitPayloads>(carrier)
            {
                payloads
                    .stack
                    .push(crate::entities::comp::unit::comp::CarriedPayload::Build {
                        block,
                        team: carrier_team,
                        rotation,
                        health,
                    });
                changed = true;
            }
            let _ = carrier_hit;
        }
        self.ecs.0.insert_non_send(runtime);
        if changed {
            // The grid snapshot dropped the building.
            self.refresh_unit_runtime();
        }
        changed
    }

    /// `InputHandler.requestDropPayload` + `PayloadComp.dropLastPayload`.
    pub(crate) fn payload_drop(&mut self, carrier: Entity, x: f32, y: f32) -> bool {
        // `InputHandler.tryDropPayload` -> `requestDropPayload(player,
        // player.x, player.y)`: the payload lands at the carrier, never the
        // cursor (`InputHandler.java:1058-1063`).
        let _ = (x, y);
        let Some(carrier_pos) = self.ecs.0.get::<Pos>(carrier).copied() else {
            return false;
        };
        let (cx, cy) = (carrier_pos.x, carrier_pos.y);
        let Some(payload) = self
            .ecs
            .0
            .get::<crate::entities::comp::unit::comp::UnitPayloads>(carrier)
            .and_then(|payloads| payloads.stack.last().copied())
        else {
            return false;
        };
        match payload {
            crate::entities::comp::unit::comp::CarriedPayload::Unit {
                type_id,
                team,
                health,
                rotation,
            } => {
                let ecs = &mut self.ecs.0;
                let Some(mut runtime) = ecs.remove_non_send::<UnitRuntime>() else {
                    return false;
                };
                let Some(def) = runtime.content.unit(type_id) else {
                    ecs.insert_non_send(runtime);
                    return false;
                };
                let seq = runtime.seq;
                runtime.seq = seq.wrapping_add(1);
                let entity = crate::entities::comp::unit::lifecycle::spawn_unit_def(
                    ecs, seq, def, team, cx, cy, rotation,
                );
                if let Some(mut stored) = ecs.get_mut::<Health>(entity) {
                    stored.health = health;
                }
                runtime.units.push(entity);
                runtime.units_created += 1;
                if let Some(mut payloads) =
                    ecs.get_mut::<crate::entities::comp::unit::comp::UnitPayloads>(carrier)
                {
                    let _ = payloads.stack.pop();
                }
                ecs.insert_non_send(runtime);
                true
            }
            crate::entities::comp::unit::comp::CarriedPayload::Build {
                block,
                team,
                rotation,
                health,
            } => {
                let size = self
                    .block_runtime
                    .as_ref()
                    .and_then(|runtime| runtime.content().block(block))
                    .map(|def| def.size)
                    .unwrap_or(1) as i16;
                let offset = (size - 1) / 2;
                let tx = WorldGrid::to_tile(cx) as i16 - offset;
                let ty = WorldGrid::to_tile(cy) as i16 - offset;
                // `tryDropPayload` -> `dropBlock`: keep carrying it when the
                // placement is invalid (Java returns false and the stack is
                // not popped).
                if !self.runtime_place(
                    tx,
                    ty,
                    block,
                    rotation,
                    team,
                    crate::world::build::PlaceFlags::PLAYER,
                ) {
                    return false;
                }
                if let Some(build) = self
                    .grid
                    .tiles
                    .getn(tx as i32 + offset as i32, ty as i32 + offset as i32)
                    .and_then(|tile| tile.build)
                    && let Some(mut stored) = self.ecs.0.get_mut::<Health>(build)
                {
                    stored.health = health;
                }
                if let Some(mut payloads) =
                    self.ecs
                        .0
                        .get_mut::<crate::entities::comp::unit::comp::UnitPayloads>(carrier)
                {
                    let _ = payloads.stack.pop();
                }
                true
            }
        }
    }
}

/// `WaveSpawner.eachGroundSpawn` core branch: a spawn tile near `(cx, cy)`
/// stepped toward `(px, py)` until no solid tile lies within radius 3
/// (`coreMargin = tilesize * 2`, `maxSteps = 30`, core size 3).
fn core_spawn_position(
    grid: &WorldGrid,
    content: &ContentRegistry,
    cx: f32,
    cy: f32,
    px: f32,
    py: f32,
) -> Option<TilePos> {
    let tile = crate::config::TILESIZE as f32;
    let (dx, dy) = (px - cx, py - cy);
    let len = (dx * dx + dy * dy).sqrt();
    if len <= f32::EPSILON {
        return None;
    }
    let limit = (tile * 2.0 + tile * 1.5 * std::f32::consts::SQRT_2).min(len);
    let mut offset = limit;
    for _ in 0..30 {
        let tx = ((cx + dx / len * offset) / tile).round() as i32;
        let ty = ((cy + dy / len * offset) / tile).round() as i32;
        let mut blocked = false;
        for ox in -3..=3i32 {
            for oy in -3..=3i32 {
                if ox * ox + oy * oy > 9 {
                    continue;
                }
                if grid.tiles.getc(tx + ox, ty + oy).solid_full(content) {
                    blocked = true;
                    break;
                }
            }
            if blocked {
                break;
            }
        }
        if !blocked {
            return Some(TilePos::new(tx as i16, ty as i16));
        }
        offset += tile * 1.1;
    }
    None
}

/// `TickSet::AfterGameUpdate` system: mirrors the live enemy count and spawn
/// window into the campaign session (`state.enemies`/`spawner.isSpawning`).
pub fn enemy_count_system(world: &mut World) {
    let Some(mut campaign) = world.remove_resource::<CampaignRuntime>() else {
        return;
    };
    if campaign.session.phase != crate::game::State::Playing || campaign.is_client {
        world.insert_resource(campaign);
        return;
    }
    let wave_team = campaign.session.wave_team();
    let (enemies, spawning) = match world.get_non_send::<UnitRuntime>() {
        Some(runtime) => {
            let enemies = runtime
                .units
                .iter()
                .filter(|entity| {
                    world
                        .get::<TeamComp>(**entity)
                        .is_some_and(|team| team.team == wave_team)
                        && world
                            .get::<UnitCore>(**entity)
                            .is_some_and(|core| !core.dead)
                })
                .count() as i32;
            (enemies, runtime.wave_spawner.spawning)
        }
        None => (0, false),
    };
    campaign.session.enemies = enemies;
    campaign.session.is_spawning = spawning;
    world.insert_resource(campaign);
}

/// Converts the persisted `Rules.spawns` shape into runtime spawn groups.
fn groups_from_rules(rules: &crate::game::rules::Rules) -> Vec<SpawnGroup> {
    rules
        .spawns
        .iter()
        .filter_map(|group| {
            serde_json::to_value(group)
                .ok()
                .and_then(|value| SpawnGroup::from_json(&value).ok())
        })
        .collect()
}
