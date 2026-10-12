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
use crate::ai::types::missile::{MissileAi, MissileShooter, update_missile};
use crate::ai::types::player::PlayerInput;
use crate::ai::wave_spawner::WaveSpawner;
use crate::audio::AudioSinkRes;
use crate::combat::bullet::{self, CombatCtx};
use crate::combat::targeting::TargetQueries;
use crate::combat::view::{FxHandle, noop_fx};
use crate::content::{BlockKind, ContentRegistry, UnitTypeId};
use crate::determinism::{Checksum, Checksummer, SimRng};
use crate::ecs::{BuildingComp, EntitySeq};
use crate::entities::comp::unit::comp::{PhysicsComp, TimedKillComp, UnitCore, UnitTypeComp};
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
    /// `Vars.controlPath`: per-goal ground fields for `CommandAI` move
    /// orders. Passability is derived lazily from [`Self::pathfinder`]'s
    /// packed tiles (the `solid` bit is team-independent).
    pub control_path: crate::ai::ControlPathfinder,
    /// Whether [`Self::control_path`] must re-read the path tiles.
    pub control_path_dirty: bool,
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
            control_path: crate::ai::ControlPathfinder::new(grid.width(), grid.height()),
            control_path_dirty: true,
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
        self.control_path_dirty = true;
    }

    /// The control pathfinder with passability current to the last
    /// [`Self::refresh_grid`] (`ControlPathfinder` world-update hook).
    pub fn control_path(&mut self) -> &mut crate::ai::ControlPathfinder {
        if self.control_path_dirty {
            self.control_path.rebuild_passability(
                self.pathfinder.width,
                self.pathfinder.height,
                &self.pathfinder.tiles,
            );
            self.control_path_dirty = false;
        }
        &mut self.control_path
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

/// `UnitComp.destroy()` shoot-on-death half (`UnitComp.java:910-919`): every
/// mount whose weapon has `shootOnDeath` and is not gated by
/// `killShooter && totalShots > 0` fires once before the unit is removed.
fn run_shoot_on_death(world: &mut World, runtime: &mut UnitRuntime, entity: Entity) {
    sync_weapon_state(world, entity);
    let state = world
        .get::<crate::weapons::UnitState>(entity)
        .copied()
        .unwrap_or_default();
    let Some(mut weapons) = world.entity_mut(entity).take::<UnitWeapons>() else {
        return;
    };
    let UnitRuntime {
        content,
        grid,
        rng,
        fx,
        seq,
        ..
    } = runtime;
    let audio = world
        .remove_resource::<AudioSinkRes>()
        .unwrap_or_else(AudioSinkRes::noop);
    let mut spawned: Vec<Entity> = Vec::new();
    {
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
        for i in 0..weapons.weapons.len() {
            if i >= weapons.mounts.len() {
                break;
            }
            if !weapons.weapons[i].shoot_on_death {
                continue;
            }
            if weapons.weapons[i].bullet.kill_shooter && weapons.mounts[i].total_shots > 0 {
                continue;
            }
            weapons.mounts[i].reload = 0.0;
            weapons.mounts[i].shoot = true;
            crate::weapons::update_weapon(
                &mut ctx,
                entity,
                &state,
                &weapons.weapons,
                &mut weapons.mounts,
                &mut weapons.pending,
                i,
            );
        }
    }
    world.insert_resource(audio);
    runtime.bullets_created += spawned.len() as u64;
    runtime.bullets.extend(spawned);
    world.entity_mut(entity).insert(weapons);
}

impl Sim {
    /// Death resolution after the entity pass (`HealthComp.damage` ->
    /// `kill()`): live units at zero health are killed through their death
    /// abilities (`UnitComp.killed`/`destroy`), and buildings at zero health
    /// are destroyed through the tile ops (`Building.kill` ->
    /// `Call.buildDestroyed`). Returns `(units, buildings)` removed.
    ///
    /// Without this pass damage only drained `Health` — nothing ever died in
    /// the live sim (PAR-0141 / PAR-0086).
    pub fn resolve_deaths(&mut self) -> (usize, usize) {
        let mut killed_units = 0usize;
        if let Some(mut runtime) = self.ecs.0.remove_non_send::<UnitRuntime>() {
            let world = &mut self.ecs.0;
            let dead: Vec<Entity> = runtime
                .units
                .iter()
                .copied()
                .filter(|entity| {
                    world
                        .get::<Health>(*entity)
                        .is_some_and(|h| h.dead || (h.max_health > 0.0 && h.health <= 0.0))
                })
                .collect();
            for entity in dead {
                // `UnitComp.destroy()` fires `shootOnDeath` mounts first
                // (PAR-0088); spawned bullets join the live list.
                run_shoot_on_death(world, &mut runtime, entity);
                let UnitRuntime {
                    content,
                    grid,
                    rng,
                    seq,
                    ..
                } = &mut runtime;
                let before = *seq;
                if crate::entities::comp::unit::lifecycle::kill_unit_with_abilities(
                    world,
                    content,
                    &grid.tiles,
                    rng,
                    seq,
                    entity,
                ) {
                    killed_units += 1;
                    runtime.units_removed += 1;
                }
                // Death-ability spawns (`SpawnDeathAbility`) join the live list.
                if runtime.seq != before {
                    let spawned: Vec<Entity> = world
                        .iter_entities()
                        .filter(|e| {
                            e.get::<EntitySeq>()
                                .is_some_and(|s| s.0 >= before && s.0 < runtime.seq)
                                && e.get::<UnitTypeComp>().is_some()
                        })
                        .map(|e| e.id())
                        .collect();
                    runtime.units_created += spawned.len() as u64;
                    runtime.units.extend(spawned);
                }
            }
            runtime
                .units
                .retain(|entity| world.get_entity(*entity).is_ok());
            world.insert_non_send(runtime);
        }

        let mut destroyed = 0usize;
        if self.block_runtime.is_some() {
            let mut dead: Vec<(u64, Entity)> = self
                .ecs
                .0
                .iter_entities()
                .filter(|e| {
                    e.get::<Building>().is_some()
                        && e.get::<Health>()
                            .is_some_and(|h| h.dead || (h.max_health > 0.0 && h.health <= 0.0))
                })
                .map(|e| (e.get::<EntitySeq>().map_or(u64::MAX, |s| s.0), e.id()))
                .collect();
            dead.sort_unstable_by_key(|(seq, entity)| (*seq, entity.index()));
            for (_, entity) in dead {
                // `BuildingComp.onDestroyed` -> `splashLiquid`: a liquid
                // container spills `(amount/4).clamp(0,10)` per deposit, up to
                // `(amount/5).clamp(0,30)` deposits scattered over the block
                // footprint (`rules.damageExplosions` gate).
                let damage_explosions = self
                    .ecs
                    .0
                    .get_resource::<CampaignRuntime>()
                    .map(|campaign| campaign.session.rules.damage_explosions)
                    .unwrap_or(true);
                let spill: Vec<(crate::content::LiquidId, f32)> = if damage_explosions {
                    self.ecs
                        .0
                        .get::<crate::world::modules::LiquidModule>(entity)
                        .map(|module| {
                            module
                                .liquids
                                .iter()
                                .enumerate()
                                .filter(|(_, amount)| **amount > 0.0)
                                .map(|(id, amount)| {
                                    (crate::content::LiquidId::new(id as u16), *amount)
                                })
                                .collect()
                        })
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                let (center, size) = self
                    .ecs
                    .0
                    .get::<crate::entities::comp::Building>(entity)
                    .map(|building| {
                        let size = self
                            .ecs
                            .0
                            .get_resource::<crate::world::BlockTable>()
                            .and_then(|table| table.instance(building.block))
                            .map(|instance| instance.def.size)
                            .unwrap_or(1);
                        (
                            crate::world::BuildHarness::tile_center(
                                building.tile.x() as i32,
                                building.tile.y() as i32,
                            ),
                            size,
                        )
                    })
                    .unwrap_or(((0.0, 0.0), 1));
                let Some(runtime) = self.block_runtime.as_mut() else {
                    break;
                };
                if let Some((x, y, block)) =
                    runtime.destroy_building(&mut self.ecs.0, &mut self.grid, entity)
                {
                    destroyed += 1;
                    log::debug!("building destroyed at ({x}, {y}) block {}", block.raw());
                    self.push_event(crate::event::SimEvent::BlockBrokenEvent(
                        crate::event::BlockBrokenEvent { x, y, block },
                    ));
                    if !spill.is_empty()
                        && let Some(mut unit_runtime) = self.ecs.0.remove_non_send::<UnitRuntime>()
                    {
                        let spread = size as f32 * 4.0;
                        for (liquid, amount) in &spill {
                            let splash = (amount / 4.0).clamp(0.0, 10.0);
                            // Java loops `for(int i = 0; i < clamp(amount / 5, 0, 30); i++)`
                            // over a float bound, i.e. `ceil` deposits.
                            let count = (amount / 5.0).clamp(0.0, 30.0).ceil() as i32;
                            for _ in 0..count {
                                let px = center.0
                                    + unit_runtime.rng.range(
                                        crate::determinism::RngStream::Sim,
                                        -spread,
                                        spread,
                                    );
                                let py = center.1
                                    + unit_runtime.rng.range(
                                        crate::determinism::RngStream::Sim,
                                        -spread,
                                        spread,
                                    );
                                let _ = crate::combat::puddles::deposit(
                                    &mut self.ecs.0,
                                    &self.grid.tiles,
                                    &unit_runtime.content,
                                    crate::world::WorldGrid::to_tile(px) as i16,
                                    crate::world::WorldGrid::to_tile(py) as i16,
                                    *liquid,
                                    splash,
                                    &mut unit_runtime.rng,
                                );
                            }
                        }
                        self.ecs.0.insert_non_send(unit_runtime);
                    }
                }
            }
            if destroyed > 0 {
                self.refresh_unit_runtime();
            }
        }
        (killed_units, destroyed)
    }

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
        // The content-backed status applier must exist before anything can
        // apply a status (`StatusApplier` resource; NEW-1): install it while
        // the registry is still owned here, then move it into the runtime.
        crate::combat::damage::status::install_status_applier(&mut self.ecs.0, &content);
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
        crate::combat::fires::update_fires(
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
        Some(entity)
    }

    /// The controlled player unit entity, if any.
    pub fn player_unit(&self) -> Option<Entity> {
        self.player_unit
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

        if slot.kind == AiKind::Player {
            // `Player` possession: the host-uploaded axis drives the unit
            // (`DesktopInput.updateMovement`); no target acquisition or pathing.
            crate::ai::types::player::update_player(world, &runtime.grid, &runtime.content, entity);
        } else if slot.kind == AiKind::Command {
            // `CommandAI.updateUnit` -> `defaultBehavior`: targeting runs for
            // the weapons (`updateTargeting`, autoTarget), but movement only
            // follows an explicit order (`targetPos`); with none the unit
            // idles in place instead of walking to the core anchor.
            let (_, mount_target) = acquire_target(world, &runtime, &queries, entity, team);
            set_mount_targets(world, entity, mount_target);
            match slot.target {
                Some(target) => {
                    let flying = world
                        .get::<PhysicsComp>(entity)
                        .is_some_and(|physics| physics.flying);
                    // Refresh the lazy control path before borrowing fields.
                    runtime.control_path();
                    let UnitRuntime {
                        grid,
                        content,
                        control_path,
                        ..
                    } = &mut runtime;
                    let arrived = crate::ai::types::command::update_command_move(
                        world,
                        grid,
                        content,
                        control_path,
                        entity,
                        target,
                        flying,
                    );
                    if arrived && let Some(mut stored) = world.get_mut::<ControllerSlot>(entity) {
                        // `finishPath`: `targetPos = null`.
                        stored.target = None;
                    }
                }
                None => {
                    if let Some(mut vel) = world.get_mut::<crate::entities::comp::Vel>(entity) {
                        vel.x = 0.0;
                        vel.y = 0.0;
                    }
                }
            }
            // `CommandAI` -> `faceTarget()` (CommandAI.java:400/420).
            face_target(world, &runtime.content, entity, mount_target);
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
            set_mount_targets(world, entity, mount_target);
        } else {
            // `GroundAI.updateMovement`/`FlyingAI`: pick a core/building anchor
            // and an in-range mount target. Spawned wave units have no command,
            // so the anchor supplies their move target.
            let (anchor_tile, mount_target) =
                acquire_target(world, &runtime, &queries, entity, team);
            set_mount_targets(world, entity, mount_target);

            let move_target = slot.target.or(anchor_tile);
            if let Some(target) = move_target {
                let flying = slot.kind == AiKind::Flying
                    || world
                        .get::<PhysicsComp>(entity)
                        .is_some_and(|physics| physics.flying);
                let arrived = if flying {
                    update_flying(world, &runtime.grid, &runtime.content, entity, target)
                } else {
                    let UnitRuntime {
                        grid,
                        content,
                        pathfinder,
                        ..
                    } = &mut runtime;
                    update_ground(world, grid, content, pathfinder, team, entity, target)
                };
                if arrived && let Some(mut stored) = world.get_mut::<ControllerSlot>(entity) {
                    stored.target = None;
                }
            }
            // `GroundAI.updateMovement` ends with `faceTarget()` (GroundAI.java:61).
            let flying = world
                .get::<PhysicsComp>(entity)
                .is_some_and(|physics| physics.flying);
            if !flying {
                face_target(world, &runtime.content, entity, mount_target);
            }
        }

        // Kind kinematics (`Unit.update` tail: legs/mech/tank/crawl/water).
        if world.get_entity(entity).is_ok() {
            let after = world.get::<Pos>(entity).map(|pos| (pos.x, pos.y));
            let delta = match (before, after) {
                (Some(before), Some(after)) => (after.0 - before.0, after.1 - before.1),
                _ => (0.0, 0.0),
            };
            let _ =
                movement::update_kinematics(world, &runtime.grid, &runtime.content, entity, delta);

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
    // Units spawned by `spawnUnit` bullets bypass `spawned` (no bullet entity).
    adopt_spawned_units(world, &mut runtime);
    runtime.bullets_created += spawned.len() as u64;
    runtime.bullets.extend(spawned);
    world.insert_non_send(runtime);
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
        Some(campaign) => (runtime.waves.get().to_vec(), campaign.session.wave_team()),
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

    // `WaveSpawner.eachGroundSpawn` core branch (`WaveSpawner.java:134-173`):
    // with `rules.wavesSpawnAtCores` in attack mode every wave-team core is
    // also a spawn position, stepped toward the first player core until the
    // 3x3 tile area is clear (PAR-0139 prerequisite: attack sectors author no
    // spawn-overlay tiles, so enemy units only enter this way).
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
                    if let Some(pos) = world.get::<Pos>(core)
                        && let Some(tile) = core_spawn_position(
                            &runtime.grid,
                            &runtime.content,
                            pos.x,
                            pos.y,
                            px,
                            py,
                        )
                    {
                        spawns.push(tile);
                    }
                }
            }
        }
    }
    log::info!(
        "wave spawn: wave {wave} with {} group(s), {} spawn position(s), attack_mode {}, spawn_at_cores {}",
        groups.len(),
        spawns.len(),
        world
            .get_resource::<CampaignRuntime>()
            .is_some_and(|campaign| campaign.session.rules.attack_mode),
        world
            .get_resource::<CampaignRuntime>()
            .is_some_and(|campaign| campaign.session.rules.waves_spawn_at_cores),
    );
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
        let payloads = group
            .payloads
            .as_ref()
            .map_or(0, |payloads| payloads.len() as u64);
        *seq = seq.wrapping_add(1 + payloads);
        spawned.push(entity);
        Some(entity)
    });
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
            rows.push(serde_json::json!({
                "id": seq.0 as i32,
                "type": type_comp.type_id.get() as i32,
                "x": pos.x,
                "y": pos.y,
                "team": team.team,
                "commandable": commandable,
            }));
        }
        serde_json::to_string(&rows).unwrap_or_else(|_| String::from("[]"))
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
                "health": entity_ref.get::<Health>().map(|health| health.health),
                "can_shoot": entity_ref.get::<crate::weapons::UnitState>().map(|state| state.can_shoot),
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
        match kind {
            0 => self.payload_pickup_unit(carrier, x, y, target),
            1 => self.payload_pickup_build(carrier, x, y),
            2 => self.payload_drop(carrier, x, y),
            _ => false,
        }
    }

    /// `InputHandler.requestUnitPayload` + `PayloadComp.canPickup(Unit)`.
    ///
    /// With no explicit `target`, resolves the candidate like
    /// `InputHandler.tryPickupPayload`: the closest unit to the cursor point
    /// within the carrier's reach (`carrier.hitSize * 2`).
    fn payload_pickup_unit(
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
            let mut near: Vec<(f32, Entity)> = runtime
                .units
                .iter()
                .copied()
                .filter(|entity| *entity != carrier)
                .filter_map(|entity| {
                    let pos = ecs.get::<Pos>(entity)?;
                    let (dx, dy) = (pos.x - x, pos.y - y);
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
                    (dx * dx + dy * dy).sqrt() <= carrier_hit * 2.0 + target_hit * 2.0
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
        changed
    }

    /// `InputHandler.requestBuildPayload` + `PayloadComp.canPickup(Building)`.
    fn payload_pickup_build(&mut self, carrier: Entity, x: f32, y: f32) -> bool {
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
                    def.kind != crate::content::BlockKind::CoreBlock,
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
    fn payload_drop(&mut self, carrier: Entity, x: f32, y: f32) -> bool {
        // `requestDropPayload`: clamp the drop point to `tilesize * 4` around
        // the carrier.
        let Some(carrier_pos) = self.ecs.0.get::<Pos>(carrier).copied() else {
            return false;
        };
        let (dx, dy) = (x - carrier_pos.x, y - carrier_pos.y);
        let len = (dx * dx + dy * dy).sqrt();
        let limit = crate::config::TILESIZE as f32 * 4.0;
        let (cx, cy) = if len > limit {
            (
                carrier_pos.x + dx / len * limit,
                carrier_pos.y + dy / len * limit,
            )
        } else {
            (x, y)
        };
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
                if !self.runtime_place(tx, ty, block, rotation, team) {
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
                if grid.tiles.getc(tx + ox, ty + oy).solid(content) {
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
    let (range, target_air, target_ground, target_under_blocks) = world
        .get::<UnitTypeComp>(entity)
        .and_then(|comp| runtime.content.unit(comp.type_id))
        .map(|def| {
            (
                def.range.max(def.max_range).max(TILE_SIZE),
                def.target_air,
                def.target_ground,
                def.target_under_blocks,
            )
        })
        .unwrap_or((TILE_SIZE, true, true, true));

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
        target_under_blocks,
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
        // Derelict (team 0) ruins are never an attack anchor
        // (`BlockIndexer.findEnemyTile`, BlockIndexer.java:466).
        if comp.team.0 == team || crate::ai::block_indexer::is_derelict(world, comp.team.0) {
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
                dist2 < best_dist
                    || (dist2 == best_dist && entity.id().index() < best_entity.index())
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
        .or_else(|| {
            world
                .get::<Building>(entity)
                .map(|building| tile_center(building.tile))
        })
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

/// `AIController.faceTarget` (AIController.java:121-128): omni-movement
/// units and mechs turn toward a valid in-range target when `type.faceTarget`
/// and the type has weapons (`unit.lookAt`: `moveToward(rotation, angle,
/// rotateSpeed * speedMultiplier)`, UnitComp.java:491). Moving units keep
/// facing their velocity (already applied by the movement pass). The
/// `Predict.intercept` lead is not ported (aims at the target position).
fn face_target(
    world: &mut World,
    content: &ContentRegistry,
    entity: Entity,
    target: Option<Entity>,
) {
    let Some(target) = target else {
        return;
    };
    let Some(def) = world
        .get::<UnitTypeComp>(entity)
        .and_then(|comp| content.unit(comp.type_id))
    else {
        return;
    };
    let mech = world
        .get::<crate::entities::comp::unit::comp::MechComp>(entity)
        .is_some();
    if !(def.omni_movement || mech) || !def.face_target || def.weapons.is_empty() {
        return;
    }
    let (Some(pos), Some(tpos)) = (
        world.get::<Pos>(entity).copied(),
        world.get::<Pos>(target).copied(),
    ) else {
        return;
    };
    let speed_mult = world
        .get::<crate::entities::comp::unit::comp::StatusComp>(entity)
        .map_or(1.0, |status| status.speed_multiplier);
    let angle = (tpos.y - pos.y).atan2(tpos.x - pos.x).to_degrees();
    if let Some(mut core) = world.get_mut::<UnitCore>(entity) {
        core.rotation = crate::ai::types::missile::move_toward_angle(
            core.rotation,
            angle,
            def.rotate_speed * speed_mult,
        );
    }
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
        assert!(
            runtime.units_removed + runtime.bullets_removed > 0,
            "combat resolved"
        );
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

    /// PAR-0140: a wire `UnitCommand` (`id` = `EntitySeq`) orders a live
    /// default-team unit; other teams and the relay path stay untouched.
    #[test]
    fn live_unit_command_sets_a_move_target() {
        let mut sim = Sim::new(1, 32, 32, BlockId::AIR, BlockId::AIR);
        install(&mut sim);
        if let Some(mut rules) = sim
            .ecs
            .0
            .get_resource_mut::<crate::world::limits::BuildRules>()
        {
            rules.default_team = 1;
        }
        let dagger = sim
            .unit_runtime()
            .expect("runtime")
            .content
            .unit_by_name("dagger")
            .expect("dagger")
            .id
            .get();
        sim.command(SimCommand::SpawnUnit {
            unit: dagger,
            x: 32.0,
            y: 32.0,
            team: 1,
        })
        .expect("spawn friendly");
        sim.command(SimCommand::SpawnUnit {
            unit: dagger,
            x: 64.0,
            y: 64.0,
            team: 2,
        })
        .expect("spawn enemy");
        let friendly = sim.unit_runtime().expect("runtime").units[0];
        let enemy = sim.unit_runtime().expect("runtime").units[1];
        let friendly_id = sim
            .ecs
            .0
            .get::<crate::ecs::EntitySeq>(friendly)
            .expect("seq")
            .0 as i32;
        let enemy_id = sim
            .ecs
            .0
            .get::<crate::ecs::EntitySeq>(enemy)
            .expect("seq")
            .0 as i32;

        // Move order on both ids: only the default-team unit accepts.
        sim.command(SimCommand::UnitCommand {
            units: smallvec::smallvec![friendly_id, enemy_id],
            command: 0,
            x: 200.0,
            y: 200.0,
        })
        .expect("command");
        assert_eq!(
            sim.ecs
                .0
                .get::<crate::ai::controller::ControllerSlot>(friendly)
                .expect("slot")
                .target,
            Some(TilePos::new(25, 25)),
            "the friendly unit is ordered at the wire target tile"
        );
        assert_eq!(
            sim.ecs
                .0
                .get::<crate::ai::controller::ControllerSlot>(enemy)
                .expect("slot")
                .target,
            None,
            "an enemy id is not commandable"
        );

        // The order drives movement.
        let before = sim.ecs.0.get::<Pos>(friendly).expect("pos").x;
        for _ in 0..30 {
            sim.tick().expect("tick");
        }
        let after = sim.ecs.0.get::<Pos>(friendly).expect("pos").x;
        assert!(
            after > before,
            "the ordered unit moves toward the target ({before} -> {after})"
        );
    }

    /// PAR-0140: the selectable snapshot lists live units by `EntitySeq` with
    /// the player-team `commandable` flag.
    #[test]
    fn selectable_units_json_lists_live_units() {
        let mut sim = Sim::new(1, 32, 32, BlockId::AIR, BlockId::AIR);
        install(&mut sim);
        if let Some(mut rules) = sim
            .ecs
            .0
            .get_resource_mut::<crate::world::limits::BuildRules>()
        {
            rules.default_team = 1;
        }
        let dagger = sim
            .unit_runtime()
            .expect("runtime")
            .content
            .unit_by_name("dagger")
            .expect("dagger")
            .id
            .get();
        sim.command(SimCommand::SpawnUnit {
            unit: dagger,
            x: 32.0,
            y: 32.0,
            team: 1,
        })
        .expect("spawn friendly");
        sim.command(SimCommand::SpawnUnit {
            unit: dagger,
            x: 64.0,
            y: 64.0,
            team: 2,
        })
        .expect("spawn enemy");
        let friendly = sim.unit_runtime().expect("runtime").units[0];
        let friendly_id = sim
            .ecs
            .0
            .get::<crate::ecs::EntitySeq>(friendly)
            .expect("seq")
            .0 as i32;

        let rows: serde_json::Value =
            serde_json::from_str(&sim.selectable_units_json()).expect("json");
        let rows = rows.as_array().expect("array");
        assert_eq!(rows.len(), 2, "both live units are listed");
        let friendly_row = rows
            .iter()
            .find(|row| row["id"].as_i64() == Some(i64::from(friendly_id)))
            .expect("friendly row");
        assert_eq!(friendly_row["team"].as_u64(), Some(1));
        assert_eq!(friendly_row["commandable"].as_bool(), Some(true));
        assert!(
            rows.iter()
                .any(|row| row["commandable"].as_bool() == Some(false)),
            "the enemy unit is listed but not commandable"
        );
    }

    /// Live world with a block runtime and `defaultTeam = 1`; returns the
    /// dagger content id.
    fn player_team_world(size: i32) -> (Sim, u16) {
        let mut sim = Sim::new(1, size, size, BlockId::AIR, BlockId::AIR);
        install(&mut sim);
        if let Some(mut rules) = sim
            .ecs
            .0
            .get_resource_mut::<crate::world::limits::BuildRules>()
        {
            rules.default_team = 1;
        }
        let dagger = sim
            .unit_runtime()
            .expect("runtime")
            .content
            .unit_by_name("dagger")
            .expect("dagger")
            .id
            .get();
        (sim, dagger)
    }

    fn spawn_dagger(sim: &mut Sim, dagger: u16, x: f32, y: f32, team: u8) -> (Entity, i32) {
        sim.command(SimCommand::SpawnUnit {
            unit: dagger,
            x,
            y,
            team,
        })
        .expect("spawn");
        let entity = *sim
            .unit_runtime()
            .expect("runtime")
            .units
            .last()
            .expect("spawned");
        let id = sim.ecs.0.get::<EntitySeq>(entity).expect("seq").0 as i32;
        (entity, id)
    }

    fn move_order(sim: &mut Sim, id: i32, x: f32, y: f32) {
        sim.command(SimCommand::UnitCommand {
            units: smallvec::smallvec![id],
            command: 0,
            x,
            y,
        })
        .expect("command");
    }

    /// PAR-0140 (UnitType.java:283): a player-team dagger gets `CommandAI`
    /// and idles without an order; a wave-team dagger keeps `GroundAI`.
    #[test]
    fn player_team_units_get_command_ai_and_idle() {
        let (mut sim, dagger) = player_team_world(48);
        let (friendly, _) = spawn_dagger(&mut sim, dagger, 160.0, 160.0, 1);
        let (enemy, _) = spawn_dagger(&mut sim, dagger, 320.0, 320.0, 2);
        let kind = |sim: &Sim, entity| {
            sim.ecs
                .0
                .get::<crate::ai::controller::ControllerSlot>(entity)
                .expect("slot")
                .kind
        };
        assert_eq!(kind(&sim, friendly), AiKind::Command);
        assert_eq!(kind(&sim, enemy), AiKind::Ground, "AI team keeps GroundAI");

        let before = *sim.ecs.0.get::<Pos>(friendly).expect("pos");
        for _ in 0..120 {
            sim.tick().expect("tick");
        }
        let after = *sim.ecs.0.get::<Pos>(friendly).expect("pos");
        assert_eq!(
            (before.x, before.y),
            (after.x, after.y),
            "an unordered CommandAI unit does not walk to the core anchor"
        );
    }

    /// PAR-0140: a MOVE order ~10 tiles away on open ground is reached
    /// within the `CommandAI` arrival radius and the order clears.
    #[test]
    fn command_ai_move_order_arrives_and_clears() {
        let (mut sim, dagger) = player_team_world(48);
        let (unit, id) = spawn_dagger(&mut sim, dagger, 80.0, 80.0, 1);
        // Tile (20, 16): world (160, 128) (`World.unconv`).
        move_order(&mut sim, id, 160.0, 128.0);
        let slot = |sim: &Sim| {
            *sim.ecs
                .0
                .get::<crate::ai::controller::ControllerSlot>(unit)
                .expect("slot")
        };
        assert_eq!(slot(&sim).target, Some(TilePos::new(20, 16)));
        // Dagger speed 0.5 px/tick over ~89 px: well under 400 ticks.
        let mut arrived_at = None;
        for tick in 0..400 {
            sim.tick().expect("tick");
            if slot(&sim).target.is_none() {
                arrived_at = Some(tick);
                break;
            }
        }
        assert!(arrived_at.is_some(), "the order finished");
        let pos = *sim.ecs.0.get::<Pos>(unit).expect("pos");
        let dist = ((pos.x - 160.0).powi(2) + (pos.y - 128.0).powi(2)).sqrt();
        assert!(
            dist <= crate::ai::types::command::command_arrival_radius(8.0, false) + 1e-3,
            "arrived within the arrival radius ({dist})"
        );
        for _ in 0..60 {
            sim.tick().expect("tick");
        }
        let rest = *sim.ecs.0.get::<Pos>(unit).expect("pos");
        assert_eq!((pos.x, pos.y), (rest.x, rest.y), "idles after arrival");
    }

    /// PAR-0140: ground orders route around a wall through the control
    /// pathfinder instead of stalling against it.
    #[test]
    fn command_ai_ground_order_routes_around_walls() {
        let (mut sim, dagger) = player_team_world(48);
        let wall = block_id(&sim, "copper-wall");
        // A vertical wall at x = 20 spanning y = 4..=28, open above.
        for y in 4..=28 {
            assert!(
                sim.command(SimCommand::Place {
                    x: 20,
                    y,
                    block: wall,
                    rotation: 0,
                    team: 1,
                    player: None,
                })
                .is_ok(),
                "wall placed"
            );
        }
        let (unit, id) = spawn_dagger(&mut sim, dagger, 12.0 * 8.0, 16.0 * 8.0, 1);
        move_order(&mut sim, id, 28.0 * 8.0, 16.0 * 8.0);
        let mut done = false;
        for _ in 0..3000 {
            sim.tick().expect("tick");
            let slot = sim
                .ecs
                .0
                .get::<crate::ai::controller::ControllerSlot>(unit)
                .expect("slot");
            if slot.target.is_none() {
                done = true;
                break;
            }
        }
        let pos = *sim.ecs.0.get::<Pos>(unit).expect("pos");
        assert!(done, "the detour order finished (at {}, {})", pos.x, pos.y);
        assert!(pos.x > 21.0 * 8.0, "crossed to the far side of the wall");
    }

    /// PAR-0140: the player's own unit (`Player` controller) is listed but
    /// not commandable, and a wire order naming it is ignored.
    #[test]
    fn player_unit_is_not_commandable() {
        let (mut sim, dagger) = player_team_world(32);
        let player = sim
            .spawn_player_unit("alpha", 1, 64.0, 64.0, 0.0)
            .expect("player unit");
        let player_id = sim.ecs.0.get::<EntitySeq>(player).expect("seq").0 as i32;
        let (friendly, friendly_id) = spawn_dagger(&mut sim, dagger, 96.0, 96.0, 1);

        let rows: serde_json::Value =
            serde_json::from_str(&sim.selectable_units_json()).expect("json");
        let row = |id: i32| {
            rows.as_array()
                .expect("array")
                .iter()
                .find(|row| row["id"].as_i64() == Some(i64::from(id)))
                .cloned()
                .expect("row")
        };
        assert_eq!(row(player_id)["commandable"].as_bool(), Some(false));
        assert_eq!(row(friendly_id)["commandable"].as_bool(), Some(true));

        sim.command(SimCommand::UnitCommand {
            units: smallvec::smallvec![player_id, friendly_id],
            command: 0,
            x: 200.0,
            y: 200.0,
        })
        .expect("command");
        let target = |entity| {
            sim.ecs
                .0
                .get::<crate::ai::controller::ControllerSlot>(entity)
                .expect("slot")
                .target
        };
        assert_eq!(target(player), None, "the player unit ignores orders");
        assert_eq!(target(friendly), Some(TilePos::new(25, 25)));
    }

    /// The live campaign timer (`TickSet::Campaign` -> `runWave`) drives the
    /// same spawn path and updates `state.enemies`/`spawner.isSpawning`.
    #[test]
    fn campaign_wave_timer_spawns_live_units() {
        use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
        use crate::game::State;
        use crate::game::planet::EmptyNeighborhood;
        use crate::game::play::PlaySession;
        use crate::game::rules::Rules;
        use crate::game::runtime::CampaignRuntime;
        use crate::game::universe::Campaign;

        let registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
            .expect("content");
        let campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
        let mut session = PlaySession::new(Rules {
            waves: true,
            wave_timer: true,
            wave_spacing: 0.05,
            default_team: 1,
            wave_team: 2,
            ..Rules::default()
        });
        session.phase = State::Playing;
        session.spawn_count = 1;
        session.wavetime = 0.001;
        // Stable player core so `checkGameState` does not end the match.
        let core_entity = Entity::from_raw_u32(1).expect("entity");
        session
            .teams
            .register_core(core_entity, crate::ecs::TeamId(1), &session.rules);

        let mut sim = Sim::new(1, 48, 48, BlockId::AIR, BlockId::AIR);
        install(&mut sim);
        let spawn = block_id(&sim, "spawn");
        let index = sim.grid.tiles.index(5, 5);
        sim.grid.tiles.geti_mut(index).overlay = BlockId::new(spawn);
        sim.set_unit_wave_team(2);
        sim.ecs
            .0
            .insert_resource(CampaignRuntime::new(session, campaign));
        sim.refresh_unit_runtime();

        for _ in 0..4 {
            sim.tick().expect("tick");
        }
        let runtime = sim
            .ecs
            .0
            .remove_resource::<CampaignRuntime>()
            .expect("campaign runtime");
        assert!(runtime.session.wave >= 1, "wave timer fired");
        assert!(
            runtime.session.enemies >= 1,
            "live enemy count mirrored into the session"
        );
        assert!(
            sim.group_counts().get("unit").copied().unwrap_or(0) >= 1,
            "campaign timer spawned a live unit"
        );
        sim.ecs.0.insert_resource(runtime);
    }

    /// Attack-mode waves spawn near the wave team's cores (`WaveSpawner`
    /// `wavesSpawnAtCores`; PAR-0139 prerequisite: attack sectors author no
    /// spawn-overlay tiles, so enemy units only enter this way).
    #[test]
    fn attack_mode_waves_spawn_near_enemy_cores() {
        use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
        use crate::ecs::BuildingComp;
        use crate::ecs::TeamId;
        use crate::game::State;
        use crate::game::planet::EmptyNeighborhood;
        use crate::game::play::PlaySession;
        use crate::game::rules::Rules;
        use crate::game::runtime::CampaignRuntime;
        use crate::game::universe::Campaign;
        use crate::world::block::BlockTable as Table;

        let registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
            .expect("content");
        let campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
        let mut session = PlaySession::new(Rules {
            waves: false,
            attack_mode: true,
            waves_spawn_at_cores: true,
            default_team: 1,
            wave_team: 2,
            ..Rules::default()
        });
        session.phase = State::Playing;

        let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");

        // Materialize a player core and the wave team's core (the spawn anchor).
        let table: Table = sim.ecs.0.get_resource::<Table>().expect("table").clone();
        let core_inst = table.get_named("core-shard").expect("core-shard").clone();
        let core_block = registry.block_id("core-shard").expect("core-shard id");
        let spawn_core = |sim: &mut Sim, seq: u64, tile: TilePos, team: u8| -> Entity {
            let entity = core_inst.spawn(
                &mut sim.ecs.0,
                seq,
                tile,
                team,
                0,
                registry.items().len(),
                registry.liquids().len(),
            );
            sim.ecs.0.entity_mut(entity).insert(BuildingComp {
                pos: tile,
                block: core_block,
                team: TeamId(team),
                rot: 0,
            });
            entity
        };
        let player_core = spawn_core(&mut sim, 10, TilePos::new(12, 32), 1);
        let enemy_core = spawn_core(&mut sim, 11, TilePos::new(48, 32), 2);
        session
            .teams
            .register_core(player_core, TeamId(1), &session.rules);
        session
            .teams
            .register_core(enemy_core, TeamId(2), &session.rules);

        sim.ecs
            .0
            .insert_resource(CampaignRuntime::new(session, campaign));
        sim.set_unit_wave_team(2);
        sim.refresh_unit_runtime();
        sim.request_wave_spawn(0);
        for _ in 0..10 {
            sim.tick().expect("tick");
        }

        let ts = crate::config::TILESIZE as f32;
        let runtime = sim.unit_runtime().expect("runtime");
        let mut spawned_near_core = 0;
        for &entity in &runtime.units {
            if let Some(pos) = sim.ecs.0.get::<Pos>(entity)
                && let Some(team) = sim.ecs.0.get::<TeamComp>(entity)
                && team.team == 2
                && pos.x > 32.0 * ts
            {
                spawned_near_core += 1;
            }
        }
        assert!(
            spawned_near_core >= 1,
            "wave units spawned near the enemy core (units: {}, near: {})",
            runtime.units.len(),
            spawned_near_core
        );
    }
}

#[cfg(all(test, feature = "msav-import"))]
mod msav_tests {
    use super::*;
    use crate::content::{BlockId, MemoryBundle, MemoryUnlockStore, create_base_content};
    use crate::editor::context::EditorContext;
    use crate::io::save::{SaveIo, SaveReadState};
    use crate::world::building_io::DecodedBase;
    use std::path::Path;

    /// Evaluator repro over the vendored Ground Zero preset: a real sector with
    /// its core materialized spawns wave units at the map's `spawn` overlay.
    #[test]
    fn ground_zero_wave_spawns_live_units() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../assets/maps/serpulo/groundZero.msav");
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        let mut content =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
                .expect("content");
        let mut grid = WorldGrid::new(0, 0);
        grid.begin_map_load();
        let pending: Vec<(usize, DecodedBase)> = {
            let mut context = EditorContext::new(&mut grid, &content);
            let mut state = SaveReadState {
                context: Some(&mut context),
                content: Some(&mut content),
                ..SaveReadState::default()
            };
            SaveIo::load_bytes(&bytes, &mut state).expect("map imports");
            std::mem::take(&mut context.pending_buildings)
        };
        grid.end_map_load(&content);
        let spawn = content.block_id("spawn").expect("spawn");
        let spawns = grid
            .tiles
            .array()
            .iter()
            .filter(|tile| tile.overlay == spawn)
            .count();
        assert!(spawns > 0, "groundZero carries spawn overlay tiles");

        let (width, height) = (grid.tiles.width, grid.tiles.height);
        let air = BlockId::AIR;
        let mut sim = Sim::new(1, width, height, air, air);
        sim.grid = grid;
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        assert!(sim.materialize_map_buildings(&pending) > 0, "core linked");
        sim.set_unit_wave_team(2);
        sim.request_wave_spawn(0);
        for _ in 0..120 {
            sim.tick().expect("tick");
        }
        assert!(
            sim.group_counts().get("unit").copied().unwrap_or(0) >= 1,
            "groundZero wave spawned a live unit: {:?}",
            sim.group_counts()
        );
    }

    /// PAR-0086/PAR-0141 end-to-end on Ground Zero: a mechanical drill on
    /// copper ore feeds an adjacent duo, which shoots down wave daggers.
    #[test]
    fn ground_zero_drill_fed_duo_kills_wave_daggers() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../assets/maps/serpulo/groundZero.msav");
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        let mut content =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
                .expect("content");
        let mut grid = WorldGrid::new(0, 0);
        grid.begin_map_load();
        let pending: Vec<(usize, DecodedBase)> = {
            let mut context = EditorContext::new(&mut grid, &content);
            let mut state = SaveReadState {
                context: Some(&mut context),
                content: Some(&mut content),
                ..SaveReadState::default()
            };
            SaveIo::load_bytes(&bytes, &mut state).expect("map imports");
            std::mem::take(&mut context.pending_buildings)
        };
        grid.end_map_load(&content);
        let ore = content.block_id("ore-copper").expect("ore");
        let (width, height) = (grid.tiles.width, grid.tiles.height);
        let air = BlockId::AIR;
        let mut sim = Sim::new(1, width, height, air, air);
        sim.grid = grid;
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        assert!(sim.materialize_map_buildings(&pending) > 0, "core linked");
        let core_block = content.block_id("core-shard").expect("core");
        let core = sim
            .grid
            .tiles
            .array()
            .iter()
            .find(|t| t.block == core_block)
            .map(|t| (i32::from(t.x), i32::from(t.y)))
            .expect("core tile");
        // A free 2x2 copper patch (drill) with a free tile to its right (duo).
        let free = |sim: &Sim, x: i32, y: i32| {
            sim.grid
                .tiles
                .getn(x, y)
                .is_some_and(|t| t.block == air && t.build.is_none())
        };
        let is_ore =
            |sim: &Sim, x: i32, y: i32| sim.grid.tiles.getn(x, y).is_some_and(|t| t.overlay == ore);
        let mut site = None;
        'search: for r in 2..40 {
            for dy in -r..=r {
                for dx in -r..=r {
                    let (x, y) = (core.0 + dx, core.1 + dy);
                    let cells = [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)];
                    if cells
                        .iter()
                        .all(|&(cx, cy)| free(&sim, cx, cy) && is_ore(&sim, cx, cy))
                        && free(&sim, x + 2, y)
                    {
                        site = Some((x, y));
                        break 'search;
                    }
                }
            }
        }
        let (dx, dy) = site.expect("copper patch near the core");
        eprintln!(
            "ground zero core {core:?} drill site ({dx}, {dy}) duo ({}, {dy})",
            dx + 2
        );
        let drill = content.block_id("mechanical-drill").expect("drill").get();
        let duo = content.block_id("duo").expect("duo").get();
        for (x, y, block) in [(dx, dy, drill), (dx + 2, dy, duo)] {
            sim.command(crate::determinism::SimCommand::Place {
                x: x as i16,
                y: y as i16,
                block,
                rotation: 0,
                team: 1,
                player: None,
            })
            .expect("place");
        }
        let duo_entity = sim.grid.tiles.get(dx + 2, dy).build.expect("duo built");
        // Let the drill stock the duo (mechanical drill ~ one copper / 9 s).
        for _ in 0..1800 {
            sim.tick().expect("tick");
        }
        let ammo = sim
            .ecs
            .0
            .get::<crate::world::blocks::defense::turrets::TurretState>(duo_entity)
            .map(|s| s.total_ammo)
            .unwrap_or(0);
        assert!(ammo > 0, "the drill fed copper ammo into the duo");
        let (px, py) = (
            (dx + 2) as f32 * TILE_SIZE + 4.0,
            dy as f32 * TILE_SIZE + 4.0,
        );
        let dagger = sim
            .spawn_live_unit("dagger", 2, px + 70.0, py, 180.0)
            .expect("dagger");
        for _ in 0..1200 {
            sim.tick().expect("tick");
            if sim.ecs.0.get_entity(dagger).is_err() {
                break;
            }
        }
        assert!(
            sim.ecs.0.get_entity(dagger).is_err(),
            "the drill-fed duo killed the dagger (ammo {ammo}, health {:?})",
            sim.ecs.0.get::<Health>(dagger).map(|h| h.health)
        );
    }

    /// §7 (PAR-0139-adjacent): an undefended Ground Zero core with low health is
    /// destroyed by wave daggers and the campaign reports game over.
    #[test]
    fn ground_zero_wave_destroys_undefended_core() {
        use crate::ecs::TeamId;
        use crate::game::State;
        use crate::game::planet::EmptyNeighborhood;
        use crate::game::play::PlaySession;
        use crate::game::rules::Rules;
        use crate::game::runtime::CampaignRuntime;
        use crate::game::universe::Campaign;

        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../assets/maps/serpulo/groundZero.msav");
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        let mut content =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
                .expect("content");
        let mut grid = WorldGrid::new(0, 0);
        grid.begin_map_load();
        let pending: Vec<(usize, DecodedBase)> = {
            let mut context = EditorContext::new(&mut grid, &content);
            let mut state = SaveReadState {
                context: Some(&mut context),
                content: Some(&mut content),
                ..SaveReadState::default()
            };
            SaveIo::load_bytes(&bytes, &mut state).expect("map imports");
            std::mem::take(&mut context.pending_buildings)
        };
        grid.end_map_load(&content);
        let (width, height) = (grid.tiles.width, grid.tiles.height);
        let air = BlockId::AIR;
        let mut sim = Sim::new(1, width, height, air, air);
        sim.grid = grid;
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        assert!(sim.materialize_map_buildings(&pending) > 0, "core linked");
        // Bullet-vs-building collision reads the runtime grid snapshot; refresh
        // it after materializing the map's buildings (bullets otherwise fly
        // through them).
        sim.refresh_unit_runtime();
        let core_block = content.block_id("core-shard").expect("core");
        let (core_x, core_y, core) = sim
            .grid
            .tiles
            .array()
            .iter()
            .find(|t| t.block == core_block)
            .and_then(|t| t.build.map(|build| (i32::from(t.x), i32::from(t.y), build)))
            .expect("core tile + entity");
        // Keep the fight short (`Health.health = 50`); the check needs the core
        // to die, not the whole base to fall.
        sim.ecs
            .0
            .get_mut::<Health>(core)
            .expect("core health")
            .health = 50.0;

        let campaign = Campaign::from_registry(&content, &EmptyNeighborhood);
        let mut session = PlaySession::new(Rules {
            waves: false,
            default_team: 1,
            wave_team: 2,
            ..Rules::default()
        });
        session.phase = State::Playing;
        // A sector makes `is_campaign()` true: wave-team units get their AI
        // (`GroundAI`) instead of idling `CommandAI`, and `checkGameState`
        // reports the sector loss when the core dies.
        session.sector = content.planet_id("serpulo").map(|planet| (planet, 170));
        session.teams.register_core(core, TeamId(1), &session.rules);
        sim.ecs
            .0
            .insert_resource(CampaignRuntime::new(session, campaign));

        // Three wave daggers ~10 tiles from the core walk in and shoot it.
        // Find an open 3-tile column on passable terrain (the crater walls are
        // solid rock; the tile-center deadlock at a boundary must be avoided).
        let free_tile = |sim: &Sim, x: i32, y: i32| {
            sim.grid
                .tiles
                .getn(x, y)
                .is_some_and(|t| !t.solid(&content) && t.build.is_none())
        };
        let mut spawn_column = None;
        'search: for r in 4..20 {
            for (dx, dy) in [
                (r, 0),
                (-r, 0),
                (0, r),
                (0, -r),
                (r, r),
                (r, -r),
                (-r, r),
                (-r, -r),
            ] {
                let (x, y) = (core_x + dx, core_y + dy);
                if (-1..=1).all(|offset| free_tile(&sim, x, y + offset)) {
                    spawn_column = Some((x, y));
                    break 'search;
                }
            }
        }
        let (sx, sy) = spawn_column.expect("open column near the core");
        eprintln!("wave spawn column ({sx}, {sy}) core ({core_x}, {core_y})");
        for offset in -1..=1 {
            let _ = sim
                .spawn_live_unit(
                    "dagger",
                    2,
                    sx as f32 * TILE_SIZE + 4.0,
                    (sy + offset) as f32 * TILE_SIZE + 4.5,
                    180.0,
                )
                .expect("dagger");
        }
        let mut core_destroyed_at = None;
        for tick in 0..3000 {
            sim.tick().expect("tick");
            if sim.grid.tiles.get(core_x, core_y).build.is_none() {
                core_destroyed_at = Some(tick);
                break;
            }
        }
        // The campaign liveness sweep + `checkGameState` run on the tick after
        // the core is removed.
        for _ in 0..10 {
            sim.tick().expect("tick");
        }
        let game_over = sim
            .ecs
            .0
            .get_resource::<CampaignRuntime>()
            .is_some_and(|runtime| runtime.session.game_over);
        eprintln!(
            "ground zero core destroyed at {core_destroyed_at:?}, game over {game_over}, units {:?}",
            sim.group_counts()
        );
        assert!(
            core_destroyed_at.is_some(),
            "the wave destroyed the undefended core"
        );
        assert!(game_over, "the campaign reported game over (sector lost)");
    }

    /// The authored attack map pins its spawn groups to the map's spawn
    /// overlay tile with Arc `Point2.pack` order; the inverted `TilePos::pack`
    /// silently dropped every pinned group (PAR-0139).
    #[test]
    fn hidden_225_pinned_spawn_matches_the_arc_pack_order() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../assets/maps/serpulo/hidden/225.msav");
        let bytes = std::fs::read(&path).expect("map");
        let mut content =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
                .expect("content");
        let mut grid = WorldGrid::new(0, 0);
        grid.begin_map_load();
        let rules = {
            let mut context = EditorContext::new(&mut grid, &content);
            let mut state = SaveReadState {
                context: Some(&mut context),
                content: Some(&mut content),
                ..SaveReadState::default()
            };
            SaveIo::load_bytes(&bytes, &mut state).expect("import");
            state.rules
        };
        let rules = rules.expect("rules");
        let spawn_id = content.block_id("spawn").expect("spawn");
        let spawn_tile = grid
            .tiles
            .array()
            .iter()
            .find(|tile| tile.overlay == spawn_id)
            .expect("authored spawn overlay");
        assert_eq!(
            (spawn_tile.x, spawn_tile.y),
            (95, 318),
            "the map's spawn overlay tile"
        );
        let game = SpawnGroup::from_json(&serde_json::to_value(&rules.spawns[0]).expect("json"))
            .expect("group");
        assert_eq!(
            game.spawn,
            TilePos::new(95, 318).pack(),
            "the authored pin matches the Arc pack order"
        );
    }
}
