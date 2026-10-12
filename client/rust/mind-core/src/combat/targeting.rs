// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Target selection queries (plan 10 §3.7; plan 11 `BlockIndexer` integration).
//!
//! Plan 10's turrets/weapons consume this instead of scanning every live
//! entity. Buildings come from plan 11's [`BlockIndexer`] (per-team flag
//! buckets + flat stable list); units are snapshotted through a `Unit`-filtered
//! ECS query so pooled bullets, fires and puddles never enter the target loop.
//! Selection order matches the previous harness fallback exactly — nearest by
//! `dst2`, ties broken by entity index — so combat goldens are unchanged.

use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_ecs::world::World;

use crate::ai::BlockIndexer;
use crate::content::ContentRegistry;
use crate::entities::comp::{Health, Pos, TeamComp, Unit};

/// Deterministic target index shared by a combat tick (plan 10 `TargetQueries`).
///
/// Built once per turret pass from a `&mut World`, then queried immutably.
#[derive(Debug, Default, Clone)]
pub struct TargetQueries {
    /// Plan 11 building index (per-team flag buckets + stable `all` list).
    pub indexer: BlockIndexer,
    /// Live unit entities in stable (entity-index) order (plan 11 units).
    pub units: Vec<Entity>,
}

impl TargetQueries {
    /// Builds the index from the current world (`BlockIndexer::rebuild` + unit query).
    pub fn build(world: &mut World, content: &ContentRegistry) -> Self {
        let mut indexer = BlockIndexer::new();
        indexer.rebuild(world, content);

        let mut units: Vec<Entity> = {
            let mut query = world.query_filtered::<Entity, With<Unit>>();
            query.iter(world).collect()
        };
        units.sort_unstable_by_key(|entity| entity.index());
        units.dedup();

        Self { indexer, units }
    }

    /// `Units.bestTarget`-equivalent: nearest enemy unit in range, else the
    /// nearest enemy building (`Units.bestTarget` checks units first).
    ///
    /// Units pass the `Turret.findTarget` gate
    /// `(e.isGrounded() || targetAir) && (!e.isGrounded() || targetGround)`;
    /// buildings need `target_ground` + `target_blocks` (plan 10 §3.8).
    /// `allow_under_blocks` mirrors `AIController.target`'s
    /// `targetUnderBlocks || !t.block.underBullets` building predicate
    /// (AIController.java:282).
    #[allow(clippy::too_many_arguments)] // mirrors `Units.bestTarget`
    pub fn closest_target(
        &self,
        world: &World,
        team: u8,
        x: f32,
        y: f32,
        range: f32,
        target_air: bool,
        target_ground: bool,
        target_blocks: bool,
        allow_under_blocks: bool,
    ) -> Option<Entity> {
        if let Some(unit) =
            self.closest_unit_filtered(world, team, x, y, range, target_air, target_ground)
        {
            return Some(unit);
        }
        if target_ground && target_blocks {
            self.indexer
                .find_enemy_building(world, team, x, y, range, allow_under_blocks)
        } else {
            None
        }
    }

    /// Nearest enemy unit in range (`Units.closestTarget` unit half).
    pub fn closest_unit(
        &self,
        world: &World,
        team: u8,
        x: f32,
        y: f32,
        range: f32,
    ) -> Option<Entity> {
        self.closest_unit_filtered(world, team, x, y, range, true, true)
    }

    /// `Units.closestEnemy` with the air/ground gate: units within
    /// `range + hitSize / 2`, dead units skipped.
    #[allow(clippy::too_many_arguments)]
    pub fn closest_unit_filtered(
        &self,
        world: &World,
        team: u8,
        x: f32,
        y: f32,
        range: f32,
        target_air: bool,
        target_ground: bool,
    ) -> Option<Entity> {
        let mut best: Option<(f32, Entity)> = None;
        for &entity in &self.units {
            if world.get::<TeamComp>(entity).map(|t| t.team) == Some(team) {
                continue;
            }
            if !world.get::<Health>(entity).is_some_and(|h| !h.dead) {
                continue;
            }
            let flying = crate::combat::bullet::unit_is_flying(world, entity);
            if !((flying && target_air) || (!flying && target_ground)) {
                continue;
            }
            let Some(pos) = world.get::<Pos>(entity) else {
                continue;
            };
            let half = world
                .get::<crate::entities::comp::unit::comp::HitboxComp>(entity)
                .map_or(0.0, |h| h.hit_size / 2.0);
            let dx = pos.x - x;
            let dy = pos.y - y;
            let dist2 = dx * dx + dy * dy;
            if dist2 > (range + half) * (range + half) {
                continue;
            }
            if is_closer(best, dist2, entity) {
                best = Some((dist2, entity));
            }
        }
        best.map(|(_, entity)| entity)
    }

    /// Nearest damaged ally building (`Units.findAllyTile` for healing turrets).
    pub fn find_ally_tile(
        &self,
        world: &World,
        team: u8,
        x: f32,
        y: f32,
        range: f32,
    ) -> Option<Entity> {
        let range2 = range * range;
        let mut best: Option<(f32, Entity)> = None;
        for &entity in self.indexer.all() {
            if world.get::<TeamComp>(entity).map(|t| t.team) != Some(team) {
                continue;
            }
            if !world.get::<Health>(entity).is_some_and(Health::damaged) {
                continue;
            }
            let Some(pos) = world.get::<Pos>(entity) else {
                continue;
            };
            let dx = pos.x - x;
            let dy = pos.y - y;
            let dist2 = dx * dx + dy * dy;
            if dist2 > range2 {
                continue;
            }
            if is_closer(best, dist2, entity) {
                best = Some((dist2, entity));
            }
        }
        best.map(|(_, entity)| entity)
    }
}

/// Whether `(dist2, entity)` should replace the current best (stable tie-break).
fn is_closer(best: Option<(f32, Entity)>, dist2: f32, entity: Entity) -> bool {
    best.is_none_or(|(current, current_entity)| {
        dist2 < current || (dist2 == current && entity.index() < current_entity.index())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::harness::CombatHarness;

    #[test]
    fn closest_target_prefers_nearest_enemy() {
        let mut harness = CombatHarness::new(64, 64, 5);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(30, 30, wall, 0, true));
        assert!(harness.place(10, 30, wall, 0, true));
        let near = harness.build_at(10, 30).expect("near wall");
        let (x, y) = CombatHarness::tile_center(4, 30);
        let targets = TargetQueries::build(&mut harness.build.world, &harness.build.content);
        let best = targets
            .closest_target(&harness.build.world, 1, x, y, 400.0, true, true, true, true)
            .expect("target");
        assert_eq!(best, near);
    }

    #[test]
    fn closest_target_respects_ground_gate() {
        let mut harness = CombatHarness::new(64, 64, 5);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(30, 30, wall, 0, true));
        let (x, y) = CombatHarness::tile_center(4, 30);
        let targets = TargetQueries::build(&mut harness.build.world, &harness.build.content);
        assert!(
            targets
                .closest_target(
                    &harness.build.world,
                    1,
                    x,
                    y,
                    400.0,
                    true,
                    false,
                    true,
                    true
                )
                .is_none(),
            "target_ground=false excludes buildings"
        );
    }
}
