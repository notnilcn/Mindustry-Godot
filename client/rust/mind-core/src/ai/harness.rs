// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Deterministic unit harness (plan 11 §7b).
//!
//! Wraps plan 07's [`BuildHarness`] (grid + buildings + content) with a unit
//! list, the [`Pathfinder`] and the AI tick. This is the headless integration
//! point for `mind-headless units ...`; the P0 `Sim` schedule is untouched so
//! plan 05's goldens stay frozen. Entities are iterated in `EntitySeq` order.

use bevy_ecs::entity::Entity;

use crate::combat::bullet::{self, CombatCtx};
use crate::combat::view::{FxHandle, noop_fx};
use crate::content::ContentRegistry;
use crate::determinism::{Checksum, Checksummer, SimRng};
use crate::ecs::EntitySeq;
use crate::entities::comp::unit::comp::{PhysicsComp, UnitCore};
use crate::entities::comp::unit::lifecycle::{
    kill_unit, remove_unit, set_move_target, spawn_unit, sync_weapon_state,
};
use crate::entities::comp::unit::movement;
use crate::entities::comp::unit::queries::{UnitSnapshot, snapshot};
use crate::entities::comp::{Health, Pos, TeamComp};
use crate::world::{BuildHarness, TilePos, WorldGrid};

use super::controller::{AiKind, ControllerSlot};
use super::pathfinder::{Cost, Pathfinder};
use super::types::flying::update_flying;
use super::types::ground::update_ground;

/// Deterministic unit/AI test harness.
pub struct UnitHarness {
    /// Plan-07 build world (grid + buildings + content + ECS).
    pub build: BuildHarness,
    /// Pathfinder resource.
    pub pathfinder: Pathfinder,
    /// Live units in spawn order.
    pub units: Vec<Entity>,
    /// Monotonic entity-sequence allocator for units.
    pub seq: u64,
    /// Deterministic RNG (unit/AI stream).
    pub rng: SimRng,
    /// Total units created.
    pub units_created: u64,
    /// Total units removed.
    pub units_removed: u64,
    /// Team whose path tiles are built (`Pathfinder` is per-team upstream).
    pub path_team: u8,
    /// Live bullet entities in spawn order (plan-10 weapon fire).
    pub bullets: Vec<Entity>,
    /// FX sink seam (plan 17; no-op in headless).
    pub fx: FxHandle,
    /// Total bullets created by unit weapons.
    pub bullets_created: u64,
    /// Total bullets removed by unit weapons.
    pub bullets_removed: u64,
}

impl UnitHarness {
    /// Creates a flat `width x height` unit world.
    pub fn new(width: i32, height: i32, seed: u64) -> Self {
        let build = BuildHarness::new(width, height, seed);
        let mut pathfinder = Pathfinder::new(width, height);
        pathfinder.rebuild(&build.grid, &build.content, 0);
        Self {
            build,
            pathfinder,
            units: Vec::new(),
            seq: 1_000_000,
            rng: SimRng::new(seed),
            units_created: 0,
            units_removed: 0,
            path_team: 0,
            bullets: Vec::new(),
            fx: noop_fx(),
            bullets_created: 0,
            bullets_removed: 0,
        }
    }

    /// Content registry.
    pub fn content(&self) -> &ContentRegistry {
        &self.build.content
    }

    /// Tile grid.
    pub fn grid(&self) -> &WorldGrid {
        &self.build.grid
    }

    /// Spawns a unit by content name at world pixels `(x, y)`.
    pub fn spawn(&mut self, name: &str, team: u8, x: f32, y: f32, rotation: f32) -> Option<Entity> {
        let seq = self.seq;
        let entity = spawn_unit(
            &mut self.build.world,
            &self.build.content,
            seq,
            name,
            team,
            x,
            y,
            rotation,
        )?;
        self.seq = self.seq.wrapping_add(1);
        self.units_created += 1;
        self.units.push(entity);
        Some(entity)
    }

    /// Issues a move command (tile target) to a unit (`CommandAI.commandPosition`).
    pub fn command_move(&mut self, entity: Entity, tx: i32, ty: i32) -> bool {
        set_move_target(
            &mut self.build.world,
            entity,
            TilePos::new(tx as i16, ty as i16),
        )
    }

    /// Despawns a unit without the death path (`UnitComp.remove`).
    pub fn remove(&mut self, entity: Entity) -> bool {
        if remove_unit(&mut self.build.world, entity) {
            self.units_removed += 1;
            true
        } else {
            false
        }
    }

    /// Kills a unit through the death path (`UnitComp.kill`/`destroy`).
    pub fn kill(&mut self, entity: Entity) -> bool {
        if kill_unit(&mut self.build.world, entity) {
            self.units_removed += 1;
            true
        } else {
            false
        }
    }

    /// Whether `entity` is still alive in the world.
    pub fn is_alive(&self, entity: Entity) -> bool {
        self.build.world.get_entity(entity).is_ok()
    }

    /// Number of live units.
    pub fn unit_count(&self) -> usize {
        self.units.iter().filter(|e| self.is_alive(**e)).count()
    }

    /// A read-only snapshot of `entity`.
    pub fn snapshot(&self, entity: Entity) -> Option<UnitSnapshot> {
        snapshot(&self.build.world, entity)
    }

    /// Advances one AI + movement + weapon + bullet tick.
    pub fn tick(&mut self) {
        let entities = self.units.clone();
        let team = self.path_team;
        for entity in entities {
            let before = self.build.world.get::<Pos>(entity).map(|p| (p.x, p.y));
            let slot = self.build.world.get::<ControllerSlot>(entity).copied();
            if let Some(target) = slot.and_then(|slot| slot.target) {
                let arrived = if slot.is_some_and(|slot| slot.kind == AiKind::Flying) {
                    update_flying(&mut self.build.world, entity, target)
                } else {
                    update_ground(
                        &mut self.build.world,
                        &self.build.grid,
                        &mut self.pathfinder,
                        team,
                        entity,
                        target,
                    )
                };
                if arrived
                    && let Some(mut slot) = self.build.world.get_mut::<ControllerSlot>(entity)
                {
                    slot.target = None;
                }
            }
            if self.build.world.get_entity(entity).is_ok() {
                let after = self.build.world.get::<Pos>(entity).map(|p| (p.x, p.y));
                let delta = match (before, after) {
                    (Some(before), Some(after)) => (after.0 - before.0, after.1 - before.1),
                    _ => (0.0, 0.0),
                };
                let breaks = movement::update_kinematics(
                    &mut self.build.world,
                    &self.build.grid,
                    &self.build.content,
                    entity,
                    delta,
                );
                // `ConstructBlock.deconstructFinish`: tanks/legs overrun props
                // (`unitMoveBreakable`); the grid is mutated through plan-06 ops.
                for (tx, ty) in breaks {
                    self.build.with_ctx(|ctx| ctx.remove_block(tx, ty));
                }
            }
        }
        self.sync_weapon_states();
        self.update_weapons_only();
        self.step_bullets_only();
    }

    /// Re-syncs every unit's plan-10 weapon state from its core/velocity.
    pub fn sync_weapon_states(&mut self) {
        let entities = self.units.clone();
        for entity in entities {
            if self.is_alive(entity) {
                sync_weapon_state(&mut self.build.world, entity);
            }
        }
    }

    /// The plan-10 weapon/mount storage of `entity`.
    pub fn unit_weapons(&self, entity: Entity) -> Option<&crate::weapons::UnitWeapons> {
        self.build.world.get::<crate::weapons::UnitWeapons>(entity)
    }

    /// Mutable weapon/mount storage of `entity`.
    pub fn unit_weapons_mut(
        &mut self,
        entity: Entity,
    ) -> Option<bevy_ecs::change_detection::Mut<'_, crate::weapons::UnitWeapons>> {
        self.build
            .world
            .get_mut::<crate::weapons::UnitWeapons>(entity)
    }

    /// Sets a mount's target entity (`mount.target`).
    pub fn set_weapon_target(&mut self, entity: Entity, index: usize, target: Option<Entity>) {
        if let Some(mut weapons) = self.unit_weapons_mut(entity)
            && let Some(mount) = weapons.mounts.get_mut(index)
        {
            mount.target = target;
        }
    }

    /// Sets a mount's `shoot` flag.
    pub fn set_weapon_shoot(&mut self, entity: Entity, index: usize, shoot: bool) {
        if let Some(mut weapons) = self.unit_weapons_mut(entity)
            && let Some(mount) = weapons.mounts.get_mut(index)
        {
            mount.shoot = shoot;
        }
    }

    /// Sets a mount's world aim point.
    pub fn set_weapon_aim(&mut self, entity: Entity, index: usize, aim: (f32, f32)) {
        if let Some(mut weapons) = self.unit_weapons_mut(entity)
            && let Some(mount) = weapons.mounts.get_mut(index)
        {
            mount.aim_x = aim.0;
            mount.aim_y = aim.1;
        }
    }

    /// Building health at a tile (`0` when empty).
    pub fn building_health_at(&self, x: i32, y: i32) -> f32 {
        self.build
            .build_at(x, y)
            .and_then(|entity| self.build.world.get::<Health>(entity))
            .map(|health| health.health)
            .unwrap_or(0.0)
    }

    /// Runs the plan-10 weapon update pass over every unit with mounts.
    pub fn update_weapons_only(&mut self) {
        let mut spawned: Vec<Entity> = Vec::new();
        {
            let mut ctx = CombatCtx {
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

    /// Advances only the bullet systems (motion + collision + cull).
    pub fn step_bullets_only(&mut self) {
        let list = std::mem::take(&mut self.bullets);
        let mut spawned: Vec<Entity> = Vec::new();
        {
            let mut ctx = CombatCtx {
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

    /// Updates the pathfinding tile layer (call after placing/breaking blocks).
    pub fn refresh_path_tiles(&mut self) {
        self.pathfinder
            .rebuild(&self.build.grid, &self.build.content, self.path_team);
    }

    /// Deterministic checksum over live units in `EntitySeq` order.
    pub fn checksum_value(&self) -> Checksum {
        let mut c = Checksummer::new();
        c.part(&self.build.grid.width());
        c.part(&self.build.grid.height());
        let mut units: Vec<(u64, Entity)> = self
            .units
            .iter()
            .copied()
            .filter(|entity| self.is_alive(*entity))
            .map(|entity| {
                let seq = self
                    .build
                    .world
                    .get::<EntitySeq>(entity)
                    .map(|seq| seq.0)
                    .unwrap_or(u64::MAX);
                (seq, entity)
            })
            .collect();
        units.sort_by_key(|(seq, entity)| (*seq, entity.index()));
        for (seq, entity) in units {
            c.part(&seq);
            if let Some(pos) = self.build.world.get::<Pos>(entity) {
                c.part(&pos.x);
                c.part(&pos.y);
            }
            if let Some(core) = self.build.world.get::<UnitCore>(entity) {
                c.part(&core.rotation);
                c.part(&core.dead);
            }
            if let Some(health) = self.build.world.get::<Health>(entity) {
                c.part(&health.health);
            }
            if let Some(team) = self.build.world.get::<TeamComp>(entity) {
                c.part(&team.team);
            }
            if let Some(physics) = self.build.world.get::<PhysicsComp>(entity) {
                c.part(&physics.speed);
            }
            if let Some(slot) = self.build.world.get::<ControllerSlot>(entity) {
                c.part(&(slot.kind as u8));
                match slot.target {
                    Some(target) => {
                        c.part(&1u8);
                        c.part(&target.pack());
                    }
                    None => {
                        c.part(&0u8);
                        c.part(&0i32);
                    }
                }
            }
        }
        c.part(&self.pathfinder.updates);
        c.finish()
    }

    /// Checksum as 16 hex digits.
    pub fn checksum_hex(&self) -> String {
        self.checksum_value().to_hex()
    }

    /// Ensures the flat-ground pathfinder is valid for the harness grid.
    pub fn ensure_pathfinder(&mut self) {
        if self.pathfinder.tiles.is_empty() {
            self.refresh_path_tiles();
        }
    }

    /// Cost used by the ground harness.
    pub const fn ground_cost() -> Cost {
        Cost::Ground
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_path_and_arrive_is_deterministic() {
        let mut first = UnitHarness::new(64, 64, 7);
        let entity = first.spawn("dagger", 0, 40.0, 40.0, 0.0).expect("dagger");
        assert!(first.command_move(entity, 60, 60));
        for _ in 0..1500 {
            first.tick();
        }
        let snap = first.snapshot(entity).expect("alive");
        let (tx, ty) = ground_tile_center(60, 60);
        let dist = ((snap.x - tx).powi(2) + (snap.y - ty).powi(2)).sqrt();
        assert!(dist <= snap.hit_size.max(4.0), "arrived within hitSize");
        assert_eq!(first.unit_count(), 1);
        let checksum = first.checksum_hex();

        let mut second = UnitHarness::new(64, 64, 7);
        let entity2 = second.spawn("dagger", 0, 40.0, 40.0, 0.0).expect("dagger");
        assert!(second.command_move(entity2, 60, 60));
        for _ in 0..1500 {
            second.tick();
        }
        assert_eq!(second.checksum_hex(), checksum);
    }

    fn ground_tile_center(x: i32, y: i32) -> (f32, f32) {
        super::super::types::ground::tile_center(x, y)
    }

    #[test]
    fn unit_weapon_fires_at_building_target() {
        let mut harness = UnitHarness::new(32, 16, 7);
        let wall = harness
            .content()
            .block_id("copper-wall")
            .expect("copper-wall");
        harness.build.rules.default_team = 1;
        assert!(harness.build.place(12, 8, wall, 0, true));
        assert!(harness.build.build_at(12, 8).is_some(), "wall placed");
        let (ux, uy) = ground_tile_center(4, 8);
        let unit = harness.spawn("dagger", 0, ux, uy, 0.0).expect("dagger");
        let mount_count = harness
            .unit_weapons(unit)
            .map(|w| w.mounts.len())
            .unwrap_or(0);
        assert!(mount_count > 0, "dagger has weapon mounts");
        let aim = ground_tile_center(12, 8);
        // Dagger carries an alternating mirrored weapon pair: every mount must
        // be told to fire so the pair alternates (plan-10 `Weapon.alternate`).
        for index in 0..mount_count {
            harness.set_weapon_aim(unit, index, aim);
            harness.set_weapon_shoot(unit, index, true);
        }
        let before = harness.building_health_at(12, 8);
        for _ in 0..240 {
            harness.tick();
        }
        let after = harness.building_health_at(12, 8);
        assert!(harness.bullets_created > 0, "a bullet was created");
        assert!(
            after < before,
            "unit weapon fired and damaged the target (before={before} after={after})"
        );
    }

    /// Plan 11 M8 lifecycle hardening (`cargo test units::fuzz_*`).
    mod units {
        use super::*;

        /// Deterministic add/remove fuzz: repeated spawn/despawn must stay
        /// consistent and reproducible.
        fn fuzz_add_remove_run(seed: u64) -> (usize, String) {
            let mut harness = UnitHarness::new(64, 64, seed);
            let mut live: Vec<Entity> = Vec::new();
            for step in 0..400u64 {
                let roll = (step.wrapping_mul(2_654_435_761)) % 100;
                if roll < 55 || live.is_empty() {
                    let x = 8.0 + ((step * 7) % 48) as f32;
                    let y = 8.0 + ((step * 13) % 48) as f32;
                    if let Some(entity) = harness.spawn("dagger", 0, x, y, 0.0) {
                        live.push(entity);
                    }
                } else {
                    let index = (step as usize * 31) % live.len();
                    let entity = live.swap_remove(index);
                    assert!(harness.remove(entity), "tracked unit must exist");
                }
                harness.tick();
                assert_eq!(
                    harness.unit_count(),
                    live.len(),
                    "live count tracks removal"
                );
            }
            (harness.unit_count(), harness.checksum_hex())
        }

        #[test]
        fn fuzz_add_remove() {
            let first = fuzz_add_remove_run(11);
            let second = fuzz_add_remove_run(11);
            assert_eq!(first, second, "add/remove fuzz is deterministic");
            assert!(first.0 <= 400);
        }

        fn fuzz_kill_during_update_run(seed: u64) -> (usize, String) {
            let mut harness = UnitHarness::new(64, 64, seed);
            let mut live: Vec<Entity> = Vec::new();
            for i in 0..80i32 {
                if let Some(entity) = harness.spawn(
                    "dagger",
                    0,
                    10.0 + (i % 20) as f32 * 2.0,
                    10.0 + (i / 20) as f32 * 2.0,
                    0.0,
                ) {
                    harness.command_move(entity, 60, 60);
                    live.push(entity);
                }
            }
            for tick in 0..300u64 {
                if tick % 5 == 0 {
                    let snapshot = live.clone();
                    for (index, entity) in snapshot.iter().enumerate() {
                        if index % 7 == tick as usize % 7 && harness.kill(*entity) {
                            live.retain(|candidate| candidate != entity);
                        }
                    }
                }
                // `tick` iterates a clone that still contains just-killed
                // entities; it must skip them without panicking.
                harness.tick();
                assert_eq!(harness.unit_count(), live.len());
            }
            (harness.unit_count(), harness.checksum_hex())
        }

        #[test]
        fn fuzz_kill_during_update() {
            let first = fuzz_kill_during_update_run(5);
            let second = fuzz_kill_during_update_run(5);
            assert_eq!(first, second, "kill-during-update is deterministic");
        }

        #[test]
        fn determinism_workers_1_vs_4() {
            use crate::async_work::{AsyncCore, PhysicsProcess, PhysicsSnapshot};

            let mut harness = UnitHarness::new(64, 64, 7);
            let mut slots: Vec<u32> = Vec::new();
            for i in 0..120i32 {
                if let Some(entity) = harness.spawn(
                    "dagger",
                    0,
                    12.0 + (i % 24) as f32 * 2.0,
                    12.0 + (i / 24) as f32 * 2.0,
                    0.0,
                ) {
                    let seq = harness
                        .build
                        .world
                        .get::<EntitySeq>(entity)
                        .map(|seq| seq.0)
                        .unwrap_or(u64::MAX);
                    slots.push(seq as u32);
                }
            }
            let snapshot = PhysicsSnapshot::new(slots);

            let mut world_one = bevy_ecs::world::World::new();
            let mut one = AsyncCore::new(4);
            one.add(Box::new(PhysicsProcess::new(4)));
            one.step(&mut world_one, &snapshot, 1);

            let mut world_four = bevy_ecs::world::World::new();
            let mut four = AsyncCore::new(4);
            four.add(Box::new(PhysicsProcess::new(4)));
            four.step(&mut world_four, &snapshot, 4);

            assert_eq!(one.last_workers(), 1);
            assert_eq!(four.last_workers(), 4);
            assert_eq!(
                one.state_sum(),
                four.state_sum(),
                "unit physics join is worker-count independent"
            );
        }

        /// Steady-state alloc audit (plan 11 §7d): after warmup, equal tick
        /// windows must allocate identically (no per-tick growth).
        #[cfg(feature = "alloc-audit")]
        #[test]
        fn steady_state_alloc_audit() {
            use crate::util::alloc::{alloc_count, enabled};

            if !enabled() {
                return;
            }
            let mut harness = UnitHarness::new(128, 128, 7);
            for i in 0..200i32 {
                if let Some(entity) = harness.spawn(
                    "dagger",
                    0,
                    20.0 + (i % 40) as f32 * 2.0,
                    20.0 + (i / 40) as f32 * 2.0,
                    0.0,
                ) {
                    harness.command_move(entity, 100, 100);
                }
            }
            for _ in 0..120 {
                harness.tick();
            }
            let before_first = alloc_count();
            for _ in 0..60 {
                harness.tick();
            }
            let first_window = alloc_count().saturating_sub(before_first);
            let before_second = alloc_count();
            for _ in 0..60 {
                harness.tick();
            }
            let second_window = alloc_count().saturating_sub(before_second);
            assert!(
                first_window.abs_diff(second_window) <= 8,
                "unit-heavy steady state must not grow allocations (windows {first_window} vs {second_window})"
            );
        }
    }
}
