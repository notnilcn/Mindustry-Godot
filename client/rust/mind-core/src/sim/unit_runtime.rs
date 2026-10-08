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

use crate::ai::controller::{AiKind, ControllerSlot};
use crate::ai::pathfinder::Pathfinder;
use crate::ai::types::flying::update_flying;
use crate::ai::types::ground::update_ground;
use crate::ai::wave_spawner::WaveSpawner;
use crate::audio::AudioSinkRes;
use crate::combat::bullet::{self, CombatCtx};
use crate::combat::targeting::TargetQueries;
use crate::combat::view::{FxHandle, noop_fx};
use crate::content::{BlockKind, ContentRegistry, UnitTypeId};
use crate::determinism::{Checksum, Checksummer, SimRng};
use crate::ecs::{BuildingComp, EntitySeq};
use crate::entities::comp::unit::comp::{PhysicsComp, UnitCore, UnitTypeComp};
use crate::entities::comp::unit::lifecycle::{spawn_unit, spawn_unit_def, sync_weapon_state};
use crate::entities::comp::unit::movement;
use crate::entities::comp::{Building, Health, Pos, TeamComp};
use crate::game::runtime::CampaignRuntime;
use crate::game::spawn_group::SpawnGroup;
use crate::game::waves::Waves;
use crate::weapons::UnitWeapons;
use crate::world::{TilePos, WorldGrid};

use super::Sim;
use super::runtime::{RuntimeError, load_vanilla_content};

/// First entity sequence allocated to live units/bullets.
///
/// The live building runtime allocates from `0`; keeping units well above that
/// range keeps `EntitySeq` unique across the mixed world (ordering only needs
/// uniqueness, but distinct ranges keep debug dumps readable).
pub const UNIT_SEQ_BASE: u64 = 1_000_000_000;

/// World pixels per tile (`config::TILESIZE`).
const TILE_SIZE: f32 = crate::config::TILESIZE as f32;

/// Live unit/combat runtime state attached to a [`Sim`]'s ECS world.
///
/// Owns the vanilla content snapshot the unit components need, the terrain
/// snapshot the movement/kinematics passes read, the per-team flowfield
/// pathfinder and the live unit/bullet entity lists. Stored as a non-send
/// resource because [`ContentRegistry`] is `!Send`.
pub struct UnitRuntime {
    /// Content registry (unit/bullet/status/weapon defs).
    pub content: ContentRegistry,
    /// Terrain snapshot (`Sim::grid`); refreshed after tile mutations.
    pub grid: WorldGrid,
    /// Per-team flowfield pathfinder (`Vars.pathfinder`).
    pub pathfinder: Pathfinder,
    /// Team the packed path tiles were built for.
    pub path_team: u8,
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
}

impl UnitRuntime {
    /// Builds a runtime over `grid` with `content` and a fresh pathfinder.
    pub fn new(content: ContentRegistry, grid: &WorldGrid, wave_team: u8) -> Self {
        let mut pathfinder = Pathfinder::new(grid.width(), grid.height());
        pathfinder.rebuild(grid, &content, wave_team);
        Self {
            content,
            grid: grid.clone(),
            pathfinder,
            path_team: wave_team,
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
        }
    }

    /// Re-snapshots the terrain and rebuilds the packed path tiles.
    pub fn refresh_grid(&mut self, grid: &WorldGrid) {
        self.grid = grid.clone();
        self.pathfinder
            .rebuild(&self.grid, &self.content, self.path_team);
    }

    /// Clears transient match state after `Logic.reset` (`Groups.clear`).
    pub fn reset(&mut self, grid: &WorldGrid) {
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
        self.refresh_grid(grid);
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
    pub fn unit_runtime_mut(
        &mut self,
    ) -> Option<bevy_ecs::change_detection::Mut<'_, UnitRuntime>> {
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
        let wave_team = crate::game::rules::Rules::default().wave_team;
        let runtime = UnitRuntime::new(content, &self.grid, wave_team);
        self.ecs.0.insert_non_send(runtime);
    }

    /// Sets the live wave/`WaveSpawner` team (`session.waveTeam`) and rebuilds
    /// the packed path tiles when it changes.
    pub fn set_unit_wave_team(&mut self, team: u8) {
        let grid = self.grid.clone();
        let Some(mut runtime) = self.ecs.0.get_non_send_mut::<UnitRuntime>() else {
            return;
        };
        runtime.wave_team = team;
        if runtime.path_team != team {
            runtime.path_team = team;
            runtime.refresh_grid(&grid);
        }
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

    /// Re-snapshots the live grid into the unit runtime (map load/tile change).
    pub fn refresh_unit_runtime(&mut self) {
        let grid = self.grid.clone();
        let Some(mut runtime) = self.ecs.0.get_non_send_mut::<UnitRuntime>() else {
            return;
        };
        runtime.refresh_grid(&grid);
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
}

/// `EntitySet::UpdateUnits` system: `GroundAI`/`FlyingAI` target + move,
/// kind kinematics and the weapon pass (`Groups.unit.update()`).
///
/// No-op unless the unit runtime is installed, so the P0 path is untouched.
pub fn unit_update_system(world: &mut World) {
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
    let entities = runtime.units.clone();
    for entity in entities {
        if world.get_entity(entity).is_err() {
            continue;
        }
        let before = world.get::<Pos>(entity).map(|pos| (pos.x, pos.y));
        let Some(slot) = world.get::<ControllerSlot>(entity).copied() else {
            continue;
        };
        let team = world
            .get::<TeamComp>(entity)
            .map(|team| team.team)
            .unwrap_or(0);

        // `GroundAI.updateMovement`/`FlyingAI`: pick a core/building anchor and
        // an in-range mount target. Spawned wave units have no command, so the
        // anchor supplies their move target.
        let (anchor_tile, mount_target) = acquire_target(world, &runtime, &queries, entity, team);
        set_mount_targets(world, entity, mount_target);

        let move_target = slot.target.or(anchor_tile);
        if let Some(target) = move_target {
            let flying = slot.kind == AiKind::Flying
                || world
                    .get::<PhysicsComp>(entity)
                    .is_some_and(|physics| physics.flying);
            let arrived = if flying {
                update_flying(world, entity, target)
            } else {
                let UnitRuntime {
                    grid, pathfinder, ..
                } = &mut runtime;
                update_ground(world, grid, pathfinder, team, entity, target)
            };
            if arrived
                && let Some(mut stored) = world.get_mut::<ControllerSlot>(entity)
            {
                stored.target = None;
            }
        }

        // Kind kinematics (`Unit.update` tail: legs/mech/tank/crawl/water).
        if world.get_entity(entity).is_ok() {
            let after = world.get::<Pos>(entity).map(|pos| (pos.x, pos.y));
            let delta = match (before, after) {
                (Some(before), Some(after)) => (after.0 - before.0, after.1 - before.1),
                _ => (0.0, 0.0),
            };
            let _ = movement::update_kinematics(
                world,
                &runtime.grid,
                &runtime.content,
                entity,
                delta,
            );
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
        crate::weapons::update_weapons(&mut ctx);
    }
    world.insert_resource(audio);
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
        for &entity in &list {
            if ctx.world.get_entity(entity).is_ok() && bullet::bullet_alive(ctx.world, entity) {
                let _ = bullet::collide_bullet(&mut ctx, entity);
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
    runtime.bullets_created += spawned.len() as u64;
    runtime.bullets.extend(spawned);
    world.insert_non_send(runtime);
}

/// `TickSet::RunWave` system: advances the spawn window and emits the pending
/// wave's groups through `WaveSpawner` (`Logic.runWave` + `WaveSpawner.update`).
pub fn run_wave_system(world: &mut World) {
    let Some(mut runtime) = world.remove_non_send::<UnitRuntime>() else {
        return;
    };
    runtime.wave_spawner.update();
    let Some(wave) = runtime.pending_wave.take() else {
        world.insert_non_send(runtime);
        return;
    };

    // `SpawnGroup.getSpawned(state.wave - 1)`: the pending index is already the
    // pre-increment wave (`MindCampaign.run_wave`/the campaign timer store it).
    let (groups, wave_team) = match world.get_resource::<CampaignRuntime>() {
        Some(campaign) if !campaign.session.rules.spawns.is_empty() => (
            groups_from_rules(&campaign.session.rules),
            campaign.session.wave_team(),
        ),
        Some(campaign) => (
            runtime.waves.get().to_vec(),
            campaign.session.wave_team(),
        ),
        None => (runtime.waves.get().to_vec(), runtime.wave_team),
    };
    runtime.wave_team = wave_team;
    runtime.path_team = wave_team;
    {
        let UnitRuntime {
            grid,
            content,
            pathfinder,
            ..
        } = &mut runtime;
        pathfinder.rebuild(grid, content, wave_team);
    }

    // `WaveSpawner` spawn tiles are the `spawn` overlay tiles of the live grid
    // (`TileOverlayChangeEvent` rebuild covers map loads here).
    let spawns: Vec<TilePos> = runtime
        .grid
        .tiles
        .array()
        .iter()
        .filter_map(|tile| {
            runtime
                .content
                .block(tile.overlay)
                .is_some_and(|def| def.kind == BlockKind::SpawnBlock)
                .then(|| TilePos::new(tile.x, tile.y))
        })
        .collect();
    runtime.wave_spawner.set_spawns(spawns);

    let UnitRuntime {
        wave_spawner,
        content,
        seq,
        units,
        units_created,
        ..
    } = &mut runtime;
    let mut spawned: Vec<Entity> = Vec::new();
    let emitted = wave_spawner.spawn_enemies(&groups, wave, |group, x, y, rotation| {
        let team = group.team.unwrap_or(wave_team);
        let entity = group.create_unit(world, content, *seq, team, x, y, rotation, wave)?;
        let payloads = group.payloads.as_ref().map_or(0, |payloads| payloads.len() as u64);
        *seq = seq.wrapping_add(1 + payloads);
        spawned.push(entity);
        Some(entity)
    });
    units.extend(spawned);
    *units_created += u64::from(emitted);
    world.insert_non_send(runtime);
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

/// Picks the unit's movement anchor (nearest enemy building) and its in-range
/// mount target (nearest enemy unit/building within the unit's range).
fn acquire_target(
    world: &World,
    runtime: &UnitRuntime,
    queries: &TargetQueries,
    entity: Entity,
    team: u8,
) -> (Option<TilePos>, Option<Entity>) {
    let Some(pos) = world.get::<Pos>(entity).copied() else {
        return (None, None);
    };
    let (range, target_air, target_ground) = world
        .get::<UnitTypeComp>(entity)
        .and_then(|comp| runtime.content.unit(comp.type_id))
        .map(|def| {
            (
                def.range.max(def.max_range).max(TILE_SIZE),
                def.target_air,
                def.target_ground,
            )
        })
        .unwrap_or((TILE_SIZE, true, true));

    let anchor = nearest_enemy_building(world, team, pos.x, pos.y);
    let in_range = queries.closest_target(
        world,
        team,
        pos.x,
        pos.y,
        range,
        target_air,
        target_ground,
        true,
    );
    let mount_target = in_range.or_else(|| {
        anchor.filter(|target| {
            building_center(world, *target).is_some_and(|(x, y)| {
                let dx = x - pos.x;
                let dy = y - pos.y;
                dx * dx + dy * dy <= (range + TILE_SIZE * 2.0).powi(2)
            })
        })
    });
    let anchor_tile = anchor.and_then(|target| building_tile(world, target));
    (anchor_tile, mount_target)
}

/// Nearest hostile building by center distance (ties break by entity index).
fn nearest_enemy_building(world: &World, team: u8, x: f32, y: f32) -> Option<Entity> {
    let mut best: Option<(f32, Entity)> = None;
    for entity in world.iter_entities() {
        let Some(comp) = entity.get::<BuildingComp>() else {
            continue;
        };
        if comp.team.0 == team {
            continue;
        }
        let Some((bx, by)) = building_center(world, entity.id()) else {
            continue;
        };
        let dx = bx - x;
        let dy = by - y;
        let dist2 = dx * dx + dy * dy;
        let replace = match best {
            None => true,
            Some((best_dist, best_entity)) => {
                dist2 < best_dist || (dist2 == best_dist && entity.id().index() < best_entity.index())
            }
        };
        if replace {
            best = Some((dist2, entity.id()));
        }
    }
    best.map(|(_, entity)| entity)
}

/// World-pixel center of a live building (`Pos`, else the center tile).
fn building_center(world: &World, entity: Entity) -> Option<(f32, f32)> {
    if let Some(pos) = world.get::<Pos>(entity) {
        return Some((pos.x, pos.y));
    }
    world
        .get::<BuildingComp>(entity)
        .map(|comp| tile_center(comp.pos))
        .or_else(|| world.get::<Building>(entity).map(|building| tile_center(building.tile)))
}

/// Center tile of a live building.
fn building_tile(world: &World, entity: Entity) -> Option<TilePos> {
    world
        .get::<BuildingComp>(entity)
        .map(|comp| comp.pos)
        .or_else(|| world.get::<Building>(entity).map(|building| building.tile))
}

/// World-pixel center of a tile.
fn tile_center(tile: TilePos) -> (f32, f32) {
    (
        (tile.x() as f32 + 0.5) * TILE_SIZE,
        (tile.y() as f32 + 0.5) * TILE_SIZE,
    )
}

/// `AIController.target`: stores the mount target/aim and gates firing.
fn set_mount_targets(world: &mut World, entity: Entity, target: Option<Entity>) {
    let aim = target.and_then(|target| world.get::<Pos>(target).map(|pos| (pos.x, pos.y)));
    let Some(mut weapons) = world.get_mut::<UnitWeapons>(entity) else {
        return;
    };
    for mount in &mut weapons.mounts {
        mount.target = target;
        mount.shoot = target.is_some();
        if let Some((x, y)) = aim {
            mount.aim_x = x;
            mount.aim_y = y;
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlockId;
    use crate::determinism::SimCommand;

    fn install(sim: &mut Sim) {
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
    }

    fn block_id(sim: &Sim, name: &str) -> u16 {
        sim.unit_runtime()
            .expect("runtime")
            .content
            .block_id(name)
            .expect("block exists")
            .get()
    }

    /// The evaluator repro at the core level: request a wave on a live world
    /// with a player core and a `spawn` overlay; units must spawn, move and
    /// fire bullets at the core.
    #[test]
    fn run_wave_spawns_moving_firing_units() {
        let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
        install(&mut sim);
        let core = block_id(&sim, "core-shard");
        let spawn = block_id(&sim, "spawn");
        assert!(
            sim.command(SimCommand::Place {
                x: 52,
                y: 52,
                block: core,
                rotation: 0,
                team: 1,
                player: None,
            })
            .is_ok(),
            "player core placed"
        );
        let index = sim.grid.tiles.index(6, 6);
        sim.grid.tiles.geti_mut(index).overlay = BlockId::new(spawn);

        sim.set_unit_wave_team(2);
        sim.request_wave_spawn(0);
        // One tick spawns the wave; the same tick's `UpdateUnits` already runs.
        sim.tick().expect("tick");
        assert!(
            sim.group_counts().get("unit").copied().unwrap_or(0) >= 1,
            "wave spawned a unit: {:?}",
            sim.group_counts()
        );

        let start = sim
            .unit_runtime()
            .expect("runtime")
            .units
            .first()
            .and_then(|entity| sim.ecs.0.get::<Pos>(*entity).copied())
            .map(|pos| (pos.x, pos.y));
        for _ in 0..900 {
            sim.tick().expect("tick");
        }
        let runtime = sim.unit_runtime().expect("runtime");
        assert!(runtime.bullets_created > 0, "wave unit fired bullets");
        assert!(runtime.units_removed + runtime.bullets_removed > 0, "combat resolved");
        let moved = runtime
            .units
            .iter()
            .filter_map(|entity| sim.ecs.0.get::<Pos>(*entity))
            .any(|pos| {
                start.is_some_and(|(sx, sy)| {
                    (pos.x - sx).powi(2) + (pos.y - sy).powi(2) > (TILE_SIZE * 2.0).powi(2)
                })
            });
        assert!(moved, "spawned unit left its spawn tile");
    }

    /// Uninstalled runtime: the wave request and systems are inert (P0 spine).
    #[test]
    fn no_runtime_is_a_noop() {
        let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
        sim.request_wave_spawn(0);
        sim.tick().expect("tick");
        assert_eq!(sim.group_counts().get("unit"), Some(&0));
        assert_eq!(sim.group_counts().get("bullet"), Some(&0));
        assert!(!sim.has_unit_runtime());
    }

    /// Two identical live worlds stay bit-identical (position/health/checksum).
    #[test]
    fn live_units_are_deterministic() {
        fn run() -> String {
            let mut sim = Sim::new(7, 64, 64, BlockId::AIR, BlockId::AIR);
            install(&mut sim);
            let core = block_id(&sim, "core-shard");
            let spawn = block_id(&sim, "spawn");
            assert!(
                sim.command(SimCommand::Place {
                    x: 50,
                    y: 50,
                    block: core,
                    rotation: 0,
                    team: 1,
                    player: None,
                })
                .is_ok()
            );
            let index = sim.grid.tiles.index(8, 8);
            sim.grid.tiles.geti_mut(index).overlay = BlockId::new(spawn);
            sim.set_unit_wave_team(2);
            sim.request_wave_spawn(0);
            for _ in 0..300 {
                sim.tick().expect("tick");
            }
            sim.unit_runtime_checksum().to_hex()
        }
        assert_eq!(run(), run());
    }

    /// `SimCommand::SpawnUnit` spawns through the live runtime when installed.
    #[test]
    fn spawn_unit_command_uses_the_live_runtime() {
        let mut sim = Sim::new(1, 32, 32, BlockId::AIR, BlockId::AIR);
        install(&mut sim);
        let dagger = sim
            .unit_runtime()
            .expect("runtime")
            .content
            .unit_by_name("dagger")
            .expect("dagger")
            .id
            .get();
        assert!(
            sim.command(SimCommand::SpawnUnit {
                unit: dagger,
                x: 16.0,
                y: 16.0,
                team: 2,
            })
            .is_ok()
        );
        assert_eq!(sim.group_counts().get("unit"), Some(&1));
        assert!(sim.unit_runtime().expect("runtime").units_created == 1);
    }
}
