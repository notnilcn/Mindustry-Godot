// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Deterministic combat harness (plan 10 §3.2/§7b).
//!
//! Wraps plan 07's [`BuildHarness`] (grid + `BlockTable` + buildings) with
//! insertion-ordered bullet entity management and the combat tick. This is the
//! headless integration point for `mind-headless combat ...`; the P0 `Sim`
//! schedule is untouched (plan 10 never edits plan-05 core files).
//!
//! Bullet order is the spawn vector order (stable, deterministic); buildings are
//! visited in `EntitySeq` order. No `HashMap` iteration is used in sim paths.

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy_ecs::entity::Entity;

use crate::content::registries::bullets::BulletDef;
use crate::content::{BulletId, BulletKind, ContentRegistry};
use crate::determinism::{Checksum, Checksummer, SimRng};
use crate::world::BuildHarness;

use super::bullet::{self, BulletSpawn};
use super::damage::area::{DamageOptions, damage_area};
use super::view::{FxHandle, noop_fx};

/// Deterministic combat harness.
pub struct CombatHarness {
    /// Plan-07 build world (grid + buildings + content).
    pub build: BuildHarness,
    /// Live bullet entities in spawn order.
    pub bullets: Vec<Entity>,
    /// Monotonic bullet entity sequence.
    pub seq: u64,
    /// Deterministic RNG (combat stream).
    pub rng: SimRng,
    /// FX sink seam (plan 17).
    pub fx: FxHandle,
    /// Total bullets created.
    pub bullets_created: u64,
    /// Total bullets removed.
    pub bullets_removed: u64,
    /// Accumulated applied damage.
    total_damage: f32,
    /// Fixture bullet names → ids.
    names: BTreeMap<String, BulletId>,
    /// Reusable spawn scratch used by `combat_ctx`/`lightning`.
    scratch_spawned: Vec<Entity>,
}

impl CombatHarness {
    /// Creates a flat `width x height` combat world with full vanilla content
    /// plus the plan-10 fixture bullets.
    pub fn new(width: i32, height: i32, seed: u64) -> Self {
        let mut build = BuildHarness::new(width, height, seed);
        let names = register_fixture_bullets(&mut build.content);
        // M5 logistics seam: register a `TurretBehavior` for every ported
        // vanilla turret so a *placed* turret owns `TurretState` and accepts
        // items/liquids through plan 08's transfer path (plan 10 §3.2). Plan
        // 07's `register_behavior` keeps only the last named override, so the
        // table is rebuilt once here with all turret overrides via the public
        // `BlockTable::build` API.
        {
            use crate::world::block::BlockTable;
            use crate::world::blocks::default_registry;
            use crate::world::blocks::defense::turrets::{behavior::TurretBehavior, config_for};
            let mut registry = default_registry(&build.content);
            for name in [
                "duo",
                "scatter",
                "scorch",
                "hail",
                "salvo",
                "swarmer",
                "fuse",
                "ripple",
                "cyclone",
                "foreshadow",
                "spectre",
                "breach",
                "diffuse",
                "wave",
                "tsunami",
                "lancer",
                "arc",
                "meltdown",
                "parallax",
                "segment",
                "titan",
                "disperse",
                "afflict",
                "lustre",
                "smite",
                "malign",
                "sublimate",
                "scathe",
                "build-tower",
            ] {
                if let Some(config) = config_for(&build.content, name, &names) {
                    registry.register_named(name, Arc::new(TurretBehavior::new(config)));
                }
            }
            if let Ok(table) = BlockTable::build(&build.content, &registry) {
                build.world.insert_resource(table);
            }
        }
        Self {
            build,
            bullets: Vec::new(),
            seq: 1_000_000,
            rng: SimRng::new(seed),
            fx: noop_fx(),
            bullets_created: 0,
            bullets_removed: 0,
            total_damage: 0.0,
            names,
            scratch_spawned: Vec::new(),
        }
    }

    /// Content registry.
    pub fn content(&self) -> &ContentRegistry {
        &self.build.content
    }

    /// Resolves a fixture bullet name (`fuse`, `rail`, `laser`, ...).
    pub fn bullet_id(&self, name: &str) -> Option<BulletId> {
        self.names.get(name).copied()
    }

    /// The fixture bullet-name map (`fuse`, `scatter_scrap`, ...; turret tests).
    pub fn names_map(&self) -> &BTreeMap<String, BulletId> {
        &self.names
    }

    /// Places a block (delegates to plan 07).
    pub fn place(
        &mut self,
        x: i32,
        y: i32,
        block: crate::content::BlockId,
        rot: u8,
        instant: bool,
    ) -> bool {
        self.build.place(x, y, block, rot, instant)
    }

    /// Building entity at a tile.
    pub fn build_at(&self, x: i32, y: i32) -> Option<Entity> {
        self.build.build_at(x, y)
    }

    /// Building health at a tile (`0` when empty).
    pub fn building_health_at(&self, x: i32, y: i32) -> f32 {
        self.build
            .build_at(x, y)
            .and_then(|e| self.build.world.get::<crate::entities::comp::Health>(e))
            .map(|h| h.health)
            .unwrap_or(0.0)
    }

    /// Advances one combat tick: buildings, units, bullets, then fires/puddles.
    pub fn tick(&mut self) {
        self.build.tick();
        self.update_weapons_only();
        self.update_turrets_only();
        self.update_defense_only();
        self.step_bullets_only();
        self.fire_tick(0.0);
        self.puddle_tick();
    }

    /// Updates the fire system one tick with a water attribute multiplier.
    pub fn fire_tick(&mut self, water_attr: f32) {
        super::fires::update_fires(
            &mut self.build.world,
            &mut self.build.grid.tiles,
            &self.build.content,
            &mut self.rng,
            self.fx.as_ref(),
            water_attr,
        );
    }

    /// Updates the puddle system one tick.
    pub fn puddle_tick(&mut self) {
        super::puddles::update_puddles(
            &mut self.build.world,
            &mut self.build.grid.tiles,
            &self.build.content,
            &mut self.rng,
            self.fx.as_ref(),
        );
    }

    /// Advances only the bullet systems (motion + collision + cull).
    pub fn step_bullets_only(&mut self) {
        let list = std::mem::take(&mut self.bullets);
        let mut spawned: Vec<Entity> = Vec::new();
        {
            let mut ctx = bullet::CombatCtx {
                world: &mut self.build.world,
                content: &self.build.content,
                grid: &self.build.grid,
                rng: &mut self.rng,
                fx: self.fx.as_ref(),
                audio: &self.build.audio,
                seq: &mut self.seq,
                spawned: &mut spawned,
            };
            for &entity in &list {
                if ctx.world.get_entity(entity).is_ok() {
                    let _ = bullet::update_bullet(&mut ctx, entity);
                }
            }
            for &entity in &list {
                if bullet::bullet_alive(ctx.world, entity) {
                    bullet::collide_bullet(&mut ctx, entity);
                }
            }
            let mut alive: Vec<Entity> = Vec::with_capacity(list.len());
            for entity in list {
                if ctx.world.get_entity(entity).is_err() {
                    self.bullets_removed += 1;
                } else if bullet::bullet_alive(ctx.world, entity) {
                    alive.push(entity);
                } else {
                    bullet::finish_bullet(&mut ctx, entity);
                    self.bullets_removed += 1;
                }
            }
            self.bullets = alive;
        }
        self.bullets_created += spawned.len() as u64;
        self.bullets.extend(spawned);
    }

    /// Spawns a fixture bullet by name.
    pub fn spawn_bullet(
        &mut self,
        name: &str,
        x: f32,
        y: f32,
        angle: f32,
        team: u8,
    ) -> Option<Entity> {
        let def = self.bullet_id(name)?;
        self.spawn_def(def, x, y, angle, team)
    }

    /// Spawns a bullet by resolved def id.
    pub fn spawn_def(
        &mut self,
        def: BulletId,
        x: f32,
        y: f32,
        angle: f32,
        team: u8,
    ) -> Option<Entity> {
        let spawn = BulletSpawn {
            def,
            x,
            y,
            angle,
            team,
            ..BulletSpawn::default()
        };
        let mut spawned: Vec<Entity> = Vec::new();
        let entity = {
            let mut ctx = bullet::CombatCtx {
                world: &mut self.build.world,
                content: &self.build.content,
                grid: &self.build.grid,
                rng: &mut self.rng,
                fx: self.fx.as_ref(),
                audio: &self.build.audio,
                seq: &mut self.seq,
                spawned: &mut spawned,
            };
            ctx.spawn(&spawn)
        };
        self.bullets_created += spawned.len() as u64;
        self.bullets.extend(spawned);
        entity
    }

    /// Spawns a plan-11 stand-in unit carrying `weapons` (M4 weapon fixture).
    ///
    /// Plan 11 replaces this with `UnitType`/`Unit` construction; until then the
    /// harness owns a minimal unit with position/velocity/team/health and the
    /// [`crate::weapons::UnitWeapons`] mount list.
    pub fn spawn_test_unit(
        &mut self,
        x: f32,
        y: f32,
        team: u8,
        weapons: Vec<crate::content::registries::units::weapon::WeaponDef>,
    ) -> Entity {
        use crate::entities::comp::{BaseEntity, Health, Pos, TeamComp, Unit, Vel};
        use crate::weapons::{UnitState, UnitWeapons};

        let unit_weapons = UnitWeapons::from_defs(&weapons);
        let seq = self.seq;
        self.seq = self.seq.wrapping_add(1);
        self.build
            .world
            .spawn((
                crate::ecs::EntitySeq(seq),
                BaseEntity::new(),
                Unit,
                Pos { x, y },
                Vel { x: 0.0, y: 0.0 },
                TeamComp { team },
                Health::new(1_000.0),
                UnitState::default(),
                unit_weapons,
            ))
            .id()
    }

    /// A test unit's weapon/mount state.
    pub fn unit_weapons(&self, entity: Entity) -> Option<&crate::weapons::UnitWeapons> {
        self.build.world.get::<crate::weapons::UnitWeapons>(entity)
    }

    /// Mutable test unit weapon/mount state.
    pub fn unit_weapons_mut(
        &mut self,
        entity: Entity,
    ) -> Option<bevy_ecs::change_detection::Mut<'_, crate::weapons::UnitWeapons>> {
        self.build
            .world
            .get_mut::<crate::weapons::UnitWeapons>(entity)
    }

    /// Sets a mount's world aim point.
    pub fn set_unit_aim(&mut self, entity: Entity, index: usize, aim: (f32, f32)) {
        if let Some(mut weapons) = self.unit_weapons_mut(entity)
            && let Some(mount) = weapons.mounts.get_mut(index)
        {
            mount.aim_x = aim.0;
            mount.aim_y = aim.1;
        }
    }

    /// Sets a mount's `shoot` flag.
    pub fn set_unit_shoot(&mut self, entity: Entity, index: usize, shoot: bool) {
        if let Some(mut weapons) = self.unit_weapons_mut(entity)
            && let Some(mount) = weapons.mounts.get_mut(index)
        {
            mount.shoot = shoot;
        }
    }

    /// Sets a test unit's rotation.
    pub fn set_unit_rotation(&mut self, entity: Entity, rotation: f32) {
        if let Some(mut state) = self
            .build
            .world
            .get_mut::<crate::weapons::UnitState>(entity)
        {
            state.rotation = rotation;
        }
    }

    /// Sets a test unit's movement delta length (shoot-velocity gate).
    pub fn set_unit_delta_len(&mut self, entity: Entity, delta_len: f32) {
        if let Some(mut state) = self
            .build
            .world
            .get_mut::<crate::weapons::UnitState>(entity)
        {
            state.delta_len = delta_len;
        }
    }

    /// Runs only the weapon update pass (unit fixtures).
    pub fn update_weapons_only(&mut self) {
        let mut spawned: Vec<Entity> = Vec::new();
        {
            let mut ctx = bullet::CombatCtx {
                world: &mut self.build.world,
                content: &self.build.content,
                grid: &self.build.grid,
                rng: &mut self.rng,
                fx: self.fx.as_ref(),
                audio: &self.build.audio,
                seq: &mut self.seq,
                spawned: &mut spawned,
            };
            crate::weapons::update_weapons(&mut ctx);
        }
        self.bullets_created += spawned.len() as u64;
        self.bullets.extend(spawned);
    }

    /// Spawns a plan-10 turret fixture for a supported config name (`duo`,
    /// `test-item`, `test-liquid`, `test-power`).
    ///
    /// The fixture carries `Pos`/`TeamComp`/`TurretState` plus the liquid and
    /// power modules so turret behavior can be driven by
    /// [`Self::update_turrets_only`] from the harness (plan 10 §3.2 fallback;
    /// plan 07's `BuildingBehavior::update_tile` has no content handle yet).
    pub fn spawn_test_turret(&mut self, name: &str, x: f32, y: f32, team: u8) -> Option<Entity> {
        use crate::entities::comp::{Pos, TeamComp};
        use crate::world::blocks::defense::turrets::{self, TurretState};
        use crate::world::modules::{LiquidModule, PowerModule};

        let config = turrets::config_for(&self.build.content, name, &self.names)?;
        let seq = self.seq;
        self.seq = self.seq.wrapping_add(1);
        let liquid_count = self.build.content.liquids().len();
        let entity = self
            .build
            .world
            .spawn((
                crate::ecs::EntitySeq(seq),
                Pos { x, y },
                TeamComp { team },
                TurretState::new(std::sync::Arc::new(config)),
                LiquidModule::with_liquids(liquid_count),
                PowerModule::new(),
            ))
            .id();
        // Fixture turrets are treated as powered unless a test un-powers them.
        if let Some(mut power) = self.build.world.get_mut::<PowerModule>(entity) {
            power.status = 1.0;
        }
        Some(entity)
    }

    /// A test turret's state.
    pub fn turret_state(
        &self,
        entity: Entity,
    ) -> Option<&crate::world::blocks::defense::turrets::TurretState> {
        self.build.world.get(entity)
    }

    /// Spawns a plan-10 `ForceProjector` fixture (M7).
    pub fn spawn_test_force_projector(
        &mut self,
        tile_x: i32,
        tile_y: i32,
        team: u8,
        radius: f32,
    ) -> Entity {
        use crate::entities::comp::{Pos, TeamComp};
        use crate::world::blocks::defense::shields::ForceProjectorState;

        let (x, y) = Self::tile_center(tile_x, tile_y);
        let seq = self.seq;
        self.seq = self.seq.wrapping_add(1);
        let state = ForceProjectorState {
            radius,
            ..Default::default()
        };
        self.build
            .world
            .spawn((
                crate::ecs::EntitySeq(seq),
                Pos { x, y },
                TeamComp { team },
                state,
            ))
            .id()
    }

    /// Spawns a plan-10 `MendProjector` fixture (M7).
    pub fn spawn_test_mend_projector(&mut self, tile_x: i32, tile_y: i32, team: u8) -> Entity {
        use crate::entities::comp::{Pos, TeamComp};
        use crate::world::blocks::defense::shields::MendProjectorState;

        let (x, y) = Self::tile_center(tile_x, tile_y);
        let seq = self.seq;
        self.seq = self.seq.wrapping_add(1);
        self.build
            .world
            .spawn((
                crate::ecs::EntitySeq(seq),
                Pos { x, y },
                TeamComp { team },
                MendProjectorState::default(),
            ))
            .id()
    }

    /// Spawns a plan-10 `ShockMine` fixture (M7).
    pub fn spawn_test_shock_mine(&mut self, tile_x: i32, tile_y: i32, team: u8) -> Entity {
        use crate::entities::comp::{Pos, TeamComp};
        use crate::world::blocks::defense::shields::ShockMineState;

        let (x, y) = Self::tile_center(tile_x, tile_y);
        let seq = self.seq;
        self.seq = self.seq.wrapping_add(1);
        self.build
            .world
            .spawn((
                crate::ecs::EntitySeq(seq),
                Pos { x, y },
                TeamComp { team },
                ShockMineState::default(),
            ))
            .id()
    }

    /// Spawns a plan-10 `TargetDummy` fixture (M7).
    pub fn spawn_test_target_dummy(
        &mut self,
        tile_x: i32,
        tile_y: i32,
        team: u8,
        health: f32,
    ) -> Entity {
        use crate::entities::comp::{Health, Pos, TeamComp};
        use crate::world::blocks::defense::shields::TargetDummyState;

        let (x, y) = Self::tile_center(tile_x, tile_y);
        let seq = self.seq;
        self.seq = self.seq.wrapping_add(1);
        self.build
            .world
            .spawn((
                crate::ecs::EntitySeq(seq),
                Pos { x, y },
                TeamComp { team },
                Health::new(health),
                TargetDummyState::default(),
            ))
            .id()
    }

    /// Runs the M7 defense pass (projectors/mend/shield-wall/dummy) before the
    /// bullet pass, mirroring `Groups.build.update` before `Groups.bullet.update`.
    pub fn update_defense_only(&mut self) {
        use crate::world::blocks::defense::shields::{
            ForceProjectorState, MendProjectorState, ShieldWallState, TargetDummyState,
            update_force_projector, update_mend_projector, update_shield_wall, update_target_dummy,
        };
        let projectors: Vec<Entity> = self
            .build
            .world
            .iter_entities()
            .filter(|entity_ref| entity_ref.contains::<ForceProjectorState>())
            .map(|entity_ref| entity_ref.id())
            .collect();
        let mends: Vec<Entity> = self
            .build
            .world
            .iter_entities()
            .filter(|entity_ref| entity_ref.contains::<MendProjectorState>())
            .map(|entity_ref| entity_ref.id())
            .collect();
        let walls: Vec<Entity> = self
            .build
            .world
            .iter_entities()
            .filter(|entity_ref| entity_ref.contains::<ShieldWallState>())
            .map(|entity_ref| entity_ref.id())
            .collect();
        let dummies: Vec<Entity> = self
            .build
            .world
            .iter_entities()
            .filter(|entity_ref| entity_ref.contains::<TargetDummyState>())
            .map(|entity_ref| entity_ref.id())
            .collect();

        let mut spawned: Vec<Entity> = Vec::new();
        {
            let mut ctx = bullet::CombatCtx {
                world: &mut self.build.world,
                content: &self.build.content,
                grid: &self.build.grid,
                rng: &mut self.rng,
                fx: self.fx.as_ref(),
                audio: &self.build.audio,
                seq: &mut self.seq,
                spawned: &mut spawned,
            };
            for entity in projectors {
                let team = ctx
                    .world
                    .get::<crate::entities::comp::TeamComp>(entity)
                    .map(|t| t.team)
                    .unwrap_or(0);
                if let Some(mut state) = ctx.world.entity_mut(entity).take::<ForceProjectorState>()
                {
                    update_force_projector(&mut ctx, entity, &mut state, team, 1.0, false, 0.0);
                    ctx.world.entity_mut(entity).insert(state);
                }
            }
            for entity in mends {
                if let Some(mut state) = ctx.world.entity_mut(entity).take::<MendProjectorState>() {
                    update_mend_projector(&mut ctx, entity, &mut state, 1.0, true, false);
                    ctx.world.entity_mut(entity).insert(state);
                }
            }
            for entity in walls {
                if let Some(mut state) = ctx.world.entity_mut(entity).take::<ShieldWallState>() {
                    update_shield_wall(&mut state, 1.0, true);
                    ctx.world.entity_mut(entity).insert(state);
                }
            }
            for entity in dummies {
                if let Some(mut state) = ctx.world.entity_mut(entity).take::<TargetDummyState>() {
                    update_target_dummy(&mut state);
                    ctx.world.entity_mut(entity).insert(state);
                }
            }
        }
        self.bullets_created += spawned.len() as u64;
        self.bullets.extend(spawned);
    }

    /// Runs only the turret update pass (M5 fixtures).
    pub fn update_turrets_only(&mut self) {
        let mut spawned: Vec<Entity> = Vec::new();
        {
            let mut ctx = bullet::CombatCtx {
                world: &mut self.build.world,
                content: &self.build.content,
                grid: &self.build.grid,
                rng: &mut self.rng,
                fx: self.fx.as_ref(),
                audio: &self.build.audio,
                seq: &mut self.seq,
                spawned: &mut spawned,
            };
            crate::world::blocks::defense::turrets::update_turrets(&mut ctx);
        }
        self.bullets_created += spawned.len() as u64;
        self.bullets.extend(spawned);
    }

    /// Applies area damage to buildings at `(x, y)` (return applied total).
    pub fn damage_buildings(&mut self, x: f32, y: f32, radius: f32, damage: f32) -> f32 {
        let applied = damage_area(
            &mut self.build.world,
            &self.build.content,
            None,
            x,
            y,
            radius,
            damage,
            DamageOptions::default(),
        );
        self.total_damage += applied;
        applied
    }

    /// Accumulated applied damage.
    pub fn damage_dealt(&self) -> f32 {
        self.total_damage
    }

    /// Live bullet count.
    pub fn bullets_live(&self) -> usize {
        self.bullets.len()
    }

    /// Center pixel of a tile.
    pub fn tile_center(x: i32, y: i32) -> (f32, f32) {
        BuildHarness::tile_center(x, y)
    }

    /// Canonical FNV-1a-64 checksum over the live bullets in spawn order.
    pub fn checksum_value(&self) -> Checksum {
        let mut c = Checksummer::new();
        for &entity in &self.bullets {
            let Some(b) = self.build.world.get::<bullet::Bullet>(entity) else {
                continue;
            };
            c.part(&b.def.raw());
            if let Some(p) = self.build.world.get::<crate::entities::comp::Pos>(entity) {
                c.part(&p.x);
                c.part(&p.y);
            }
            if let Some(v) = self.build.world.get::<crate::entities::comp::Vel>(entity) {
                c.part(&v.x);
                c.part(&v.y);
            }
            c.part(&b.time);
            c.part(&b.lifetime);
            c.part(&b.damage);
            c.part(&b.flags);
        }
        // Buildings contribute current health, in deterministic sequence order, so
        // the golden reflects combat damage rather than only live bullets.
        let mut buildings: Vec<(u64, f32)> = self
            .build
            .world
            .iter_entities()
            .filter_map(|entity_ref| {
                let health = entity_ref.get::<crate::entities::comp::Health>()?;
                entity_ref.get::<crate::entities::comp::Building>()?;
                let seq = entity_ref
                    .get::<crate::ecs::EntitySeq>()
                    .map(|s| s.0)
                    .unwrap_or(u64::MAX);
                Some((seq, health.health))
            })
            .collect();
        buildings.sort_by_key(|entry| entry.0);
        for (seq, health) in buildings {
            c.part(&seq);
            c.part(&health);
        }
        // Fires/puddles contribute by tile order (plan 10 §6.4).
        let mut fires: Vec<((i16, i16), f32, f32)> = self
            .build
            .world
            .iter_entities()
            .filter_map(|entity_ref| {
                let fire = entity_ref.get::<super::fires::FireState>()?;
                Some((fire.tile, fire.time, fire.lifetime))
            })
            .collect();
        fires.sort_by_key(|entry| entry.0);
        for (tile, time, lifetime) in fires {
            c.part(&tile.0);
            c.part(&tile.1);
            c.part(&time);
            c.part(&lifetime);
        }
        let mut puddles: Vec<((i16, i16), u16, f32)> = self
            .build
            .world
            .iter_entities()
            .filter_map(|entity_ref| {
                let puddle = entity_ref.get::<super::puddles::PuddleState>()?;
                Some((puddle.tile, puddle.liquid.raw(), puddle.amount))
            })
            .collect();
        puddles.sort_by_key(|entry| entry.0);
        for (tile, liquid, amount) in puddles {
            c.part(&tile.0);
            c.part(&tile.1);
            c.part(&liquid);
            c.part(&amount);
        }
        c.finish()
    }

    /// Checksum as 16 hex digits.
    pub fn checksum_hex(&self) -> String {
        self.checksum_value().to_hex()
    }

    /// Installs a recording FX sink (tests/scenarios).
    pub fn set_fx(&mut self, fx: FxHandle) {
        self.fx = fx;
    }

    /// Builds a borrow bundle for direct kind/system calls (tests).
    pub fn combat_ctx(&mut self) -> bullet::CombatCtx<'_> {
        bullet::CombatCtx {
            world: &mut self.build.world,
            content: &self.build.content,
            grid: &self.build.grid,
            rng: &mut self.rng,
            fx: self.fx.as_ref(),
            audio: &self.build.audio,
            seq: &mut self.seq,
            spawned: &mut self.scratch_spawned,
        }
    }

    /// Drains bullets spawned through [`Self::combat_ctx`] into the live list.
    pub fn claim_scratch_spawned(&mut self) {
        let spawned = std::mem::take(&mut self.scratch_spawned);
        self.bullets_created += spawned.len() as u64;
        self.bullets.extend(spawned);
    }

    /// Creates a lightning branch via the combat context (M3 tests/scenarios).
    #[allow(clippy::too_many_arguments)]
    pub fn lightning(
        &mut self,
        team: u8,
        color: crate::content::Rgba,
        damage: f32,
        x: f32,
        y: f32,
        rotation: f32,
        length: i32,
    ) -> super::lightning::LightningResult {
        let result = {
            let mut ctx = self.combat_ctx();
            super::lightning::create(&mut ctx, team, color, damage, x, y, rotation, length)
        };
        self.claim_scratch_spawned();
        result
    }

    /// A cloneable handle to the default no-op sink.
    pub fn noop_fx() -> FxHandle {
        Arc::new(super::view::NoopFx)
    }
}

/// Registers the plan-10 fixture bullets (`fuse`, `rail`, `laser`, ...).
///
/// The vanilla bullet space has no name lookup (upstream bullets are anonymous
/// or weapon-inline), so the headless combat scenarios use these named fixtures.
fn register_fixture_bullets(content: &mut ContentRegistry) -> BTreeMap<String, BulletId> {
    let mut names = BTreeMap::new();
    let mut add = |name: &str, kind: BulletKind, configure: &dyn Fn(&mut BulletDef)| {
        let mut def = BulletDef::new(kind);
        configure(&mut def);
        if let Ok(id) = content.add_bullet(def) {
            names.insert(name.to_owned(), id);
        }
    };

    add("fuse", BulletKind::Basic, &|def| {
        def.speed = 4.0;
        def.lifetime = 100.0;
        def.damage = 40.0;
        def.hit_size = 4.0;
        def.building_damage_multiplier = 1.0;
        def.drag = 0.0;
        // Plan-18 call-site fixture: exercise `hit`/`despawned` sounds.
        def.hit_sound = crate::content::registries::sound_meta::SoundId::EXPLOSION_ARTILLERY;
        def.despawn_sound = crate::content::registries::sound_meta::SoundId::EXPLOSION;
    });
    add("fuse_slow", BulletKind::Basic, &|def| {
        def.speed = 1.0;
        def.lifetime = 200.0;
        def.damage = 10.0;
        def.hit_size = 4.0;
        def.drag = 0.0;
    });
    add("fuse_frag", BulletKind::Basic, &|def| {
        def.speed = 3.0;
        def.lifetime = 40.0;
        def.damage = 5.0;
        def.hit_size = 3.0;
        def.drag = 0.0;
    });
    add("fuse_scale", BulletKind::Basic, &|def| {
        def.speed = 4.0;
        def.lifetime = 100.0;
        def.damage = 10.0;
        def.hit_size = 4.0;
        def.scale_life = true;
        def.drag = 0.0;
    });
    add("rail", BulletKind::Rail, &|def| {
        def.speed = 12.0;
        def.lifetime = 40.0;
        def.damage = 60.0;
        def.hit_size = 2.0;
        def.collides = true;
        def.pierce = true;
        def.pierce_building = true;
        def.pierce_cap = 3;
        def.pierce_damage_factor = 0.0;
        def.remove_after_pierce = true;
        def.drag = 0.0;
    });
    add("laser", BulletKind::Laser, &|def| {
        def.speed = 8.0;
        def.lifetime = 20.0;
        def.damage = 30.0;
        def.hit_size = 2.0;
        def.collides = false;
        def.length = 200.0;
        def.drag = 0.0;
    });
    add("fire_bullet", BulletKind::Fire, &|def| {
        def.speed = 2.0;
        def.lifetime = 40.0;
        def.damage = 0.0;
        def.hit_size = 3.0;
        def.drag = 0.0;
    });
    add("liquid_bullet", BulletKind::Liquid, &|def| {
        def.speed = 5.0;
        def.lifetime = 30.0;
        def.damage = 0.0;
        def.hit_size = 3.0;
        def.drag = 0.0;
    });
    add("artillery", BulletKind::Artillery, &|def| {
        def.speed = 1.2;
        def.lifetime = 180.0;
        def.damage = 5.0;
        def.splash_damage = 40.0;
        def.splash_damage_radius = 24.0;
        def.hit_size = 4.0;
        def.drag = 0.0;
    });
    add("explosion_marker", BulletKind::Explosion, &|def| {
        def.speed = 0.001;
        def.lifetime = 1.0;
        def.damage = 0.0;
        def.splash_damage = 100.0;
        def.splash_damage_radius = 32.0;
        def.hit_size = 1.0;
    });
    add("frag", BulletKind::Basic, &|def| {
        def.speed = 4.0;
        def.lifetime = 60.0;
        def.damage = 20.0;
        def.hit_size = 4.0;
        def.frag_bullets = 3;
        def.frag_on_hit = true;
        def.frag_on_despawn = true;
        def.frag_random_spread = 60.0;
        def.frag_spread = 25.0;
        def.frag_velocity_min = 2.0;
        def.frag_velocity_max = 4.0;
        def.frag_life_min = 0.5;
        def.frag_life_max = 1.0;
        def.drag = 0.0;
    });
    add("sticky", BulletKind::Basic, &|def| {
        def.speed = 3.0;
        def.lifetime = 200.0;
        def.damage = 5.0;
        def.hit_size = 5.0;
        def.drag = 0.0;
    });
    add("terrain", BulletKind::Basic, &|def| {
        def.speed = 6.0;
        def.lifetime = 60.0;
        def.damage = 5.0;
        def.hit_size = 3.0;
        def.collide_terrain = true;
        def.drag = 0.0;
    });
    add("interval", BulletKind::Basic, &|def| {
        def.speed = 4.0;
        def.lifetime = 80.0;
        def.damage = 5.0;
        def.hit_size = 3.0;
        def.bullet_interval = 10.0;
        def.interval_bullets = 1;
        def.interval_random_spread = 20.0;
        def.drag = 0.0;
    });
    add("splash", BulletKind::Basic, &|def| {
        def.speed = 4.0;
        def.lifetime = 80.0;
        def.damage = 5.0;
        def.hit_size = 4.0;
        def.splash_damage = 30.0;
        def.splash_damage_radius = 24.0;
        def.drag = 0.0;
    });
    add("point", BulletKind::Point, &|def| {
        def.speed = 4.0;
        def.lifetime = 20.0;
        def.damage = 40.0;
        def.hit_size = 2.0;
        def.collides = true;
        def.drag = 0.0;
    });
    add("multi", BulletKind::Multi, &|def| {
        def.lifetime = 1.0;
        def.multi_repeat = 2;
        def.damage = 10.0;
        def.drag = 0.0;
    });
    add("emp", BulletKind::Emp, &|def| {
        def.speed = 5.0;
        def.lifetime = 30.0;
        def.damage = 20.0;
        def.emp_radius = 24.0;
        def.unit_damage_scl = 0.7;
        def.power_damage_scl = 2.0;
        def.drag = 0.0;
    });
    add("flak", BulletKind::Flak, &|def| {
        def.speed = 4.0;
        def.lifetime = 60.0;
        def.damage = 5.0;
        def.splash_damage = 30.0;
        def.splash_damage_radius = 24.0;
        def.explode_range = 20.0;
        def.explode_delay = 3.0;
        def.flak_delay = 0.0;
        def.drag = 0.0;
    });
    add("sap", BulletKind::Sap, &|def| {
        def.speed = 0.0;
        def.lifetime = 30.0;
        def.damage = 20.0;
        def.length = 60.0;
        def.sap_strength = 0.5;
    });
    add("shrapnel", BulletKind::Shrapnel, &|def| {
        def.speed = 0.0;
        def.lifetime = 10.0;
        def.damage = 25.0;
        def.length = 60.0;
    });
    add("interceptor", BulletKind::Interceptor, &|def| {
        def.speed = 6.0;
        def.lifetime = 40.0;
        def.damage = 15.0;
        def.hit_size = 4.0;
        def.drag = 0.0;
    });
    add("mass_driver", BulletKind::MassDriver, &|def| {
        def.speed = 8.0;
        def.lifetime = 60.0;
        def.damage = 75.0;
        def.hit_size = 4.0;
        def.drag = 0.0;
    });
    add("continuous", BulletKind::ContinuousLaser, &|def| {
        def.speed = 0.0;
        def.lifetime = 60.0;
        def.damage = 10.0;
        def.damage_interval = 5.0;
        def.length = 80.0;
    });
    add("empty", BulletKind::Empty, &|def| {
        def.lifetime = 40.0;
        def.damage = 5.0;
    });
    // Plan 10 owns turret ammo (`Blocks.java` `ammoTypes`); register the M5
    // `duo` ammo set (the closure above must be dropped first).
    crate::world::blocks::defense::turrets::register_bullets(content, &mut names);
    // Cross-references (child defs must exist first).
    if let (Some(parent), Some(child)) = (names.get("frag"), names.get("fuse_frag"))
        && let Some(def) = content.bullet_mut(*parent)
    {
        def.frag_bullet = Some(*child);
    }
    if let (Some(parent), Some(child)) = (names.get("interval"), names.get("fuse_frag"))
        && let Some(def) = content.bullet_mut(*parent)
    {
        def.interval_bullet = Some(*child);
    }
    if let (Some(parent), Some(child)) = (names.get("multi"), names.get("fuse_frag"))
        && let Some(def) = content.bullet_mut(*parent)
    {
        def.spawn_bullets.push(*child);
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_bullets_registered() {
        let harness = CombatHarness::new(8, 8, 1);
        for name in [
            "fuse",
            "fuse_slow",
            "fuse_frag",
            "rail",
            "laser",
            "fire_bullet",
            "liquid_bullet",
            "artillery",
            "explosion_marker",
            "frag",
            "sticky",
            "terrain",
            "interval",
            "splash",
        ] {
            assert!(harness.bullet_id(name).is_some(), "missing {name}");
        }
    }

    #[test]
    fn spawn_and_checksum_is_deterministic() {
        let mut a = CombatHarness::new(16, 16, 7);
        let mut b = CombatHarness::new(16, 16, 7);
        for harness in [&mut a, &mut b] {
            let (x, y) = CombatHarness::tile_center(2, 2);
            let _ = harness.spawn_bullet("fuse", x, y, 0.0, 0);
            for _ in 0..10 {
                harness.step_bullets_only();
            }
        }
        assert_eq!(a.checksum_hex(), b.checksum_hex());
    }
}
