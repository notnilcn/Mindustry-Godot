// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Deterministic unit harness (plan 11 §7b).
//!
//! Wraps plan 07's [`BuildHarness`] (grid + buildings + content) with a unit
//! list, the [`Pathfinder`] and the AI tick. This is the headless integration
//! point for `mind-headless units ...`; the P0 `Sim` schedule is untouched so
//! plan 05's goldens stay frozen. Entities are iterated in `EntitySeq` order.

use bevy_ecs::entity::Entity;

use crate::content::ContentRegistry;
use crate::determinism::{Checksum, Checksummer, SimRng};
use crate::ecs::EntitySeq;
use crate::entities::comp::unit::comp::{PhysicsComp, UnitCore};
use crate::entities::comp::unit::lifecycle::{set_move_target, spawn_unit};
use crate::entities::comp::unit::queries::{UnitSnapshot, snapshot};
use crate::entities::comp::{Health, Pos, TeamComp};
use crate::world::{BuildHarness, TilePos, WorldGrid};

use super::controller::ControllerSlot;
use super::pathfinder::{Cost, Pathfinder};
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

    /// Advances one AI + movement tick.
    pub fn tick(&mut self) {
        let entities = self.units.clone();
        let UnitHarness {
            build,
            pathfinder,
            rng,
            path_team,
            ..
        } = self;
        let _ = &rng;
        let team = *path_team;
        for entity in entities {
            let Some(target) = build
                .world
                .get::<ControllerSlot>(entity)
                .and_then(|slot| slot.target)
            else {
                continue;
            };
            let arrived = update_ground(
                &mut build.world,
                &build.grid,
                pathfinder,
                team,
                entity,
                target,
            );
            if arrived && let Some(mut slot) = build.world.get_mut::<ControllerSlot>(entity) {
                slot.target = None;
            }
        }
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
}
