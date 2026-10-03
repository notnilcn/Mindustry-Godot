// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Generator/reactor explosion triggers (`PowerGenerator.onDestroyed`/
//! `shouldExplode`/`createExplosion`/`onExplosion`).
//!
//! Plan 09 owns the **pure trigger**: the death/overheat conditions, the exact
//! `shouldExplode()` gate, the `Rules.reactorExplosions` gate and firing exactly
//! once. Plan 10 (`Damage.dynamicExplosion`) and plan 17 (`Fx`/`Effect.shake`/
//! `scorch`) own the bodies and are reached through the [`ExplosionSink`] seam.
//! The default sink is inert, so headless goldens are unchanged; hosts install the
//! real one. `explosionIgnitionChance`/`explosionFireballs`/`explosionPuddles`
//! ride along as parameters so plan 10/17 can reproduce `onExplosion` verbatim.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;

use crate::entities::comp::{Building, TeamComp};
use crate::world::block::TILE_SIZE;

/// One building's explosion parameters (`PowerGenerator` fields).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReactorExplosion {
    /// World-pixel center x.
    pub x: f32,
    /// World-pixel center y.
    pub y: f32,
    /// Owning team (`None` when the entity has no `TeamComp`).
    pub team: Option<u8>,
    /// `PowerGenerator.explosionRadius` in tiles.
    pub radius: f32,
    /// `PowerGenerator.explosionDamage`.
    pub damage: f32,
    /// Whether the explosion can ignite tiles (plan 10 `Fires`).
    pub fire: bool,
}

impl ReactorExplosion {
    /// Builds the parameter bundle for `entity` from its tile/team and knobs.
    pub fn for_building(
        world: &World,
        entity: Entity,
        radius: f32,
        damage: f32,
        fire: bool,
    ) -> Option<Self> {
        let building = world.get::<Building>(entity)?;
        let x = (building.tile.x() as f32 + 0.5) * TILE_SIZE;
        let y = (building.tile.y() as f32 + 0.5) * TILE_SIZE;
        let team = world.get::<TeamComp>(entity).map(|team| team.team);
        Some(Self {
            x,
            y,
            team,
            radius,
            damage,
            fire,
        })
    }
}

/// Plan-10 `Damage.dynamicExplosion` + plan-17 effect receiver.
pub trait ExplosionSink: Send + Sync {
    /// Applies the explosion (`PowerGenerator.onExplosion` body).
    fn explode(&self, world: &mut World, explosion: ReactorExplosion);
}

/// Host-installed [`ExplosionSink`]. `None` is the inert headless default.
#[derive(Resource, Default)]
pub struct ExplosionSinkRes(pub Option<Box<dyn ExplosionSink>>);

/// Marker inserted the first time a building fires its reactor explosion, so a
/// self-destruct (`kill()`) followed by `onDestroyed()` cannot double-fire.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Component)]
pub struct ExplosionFired;

/// `GeneratorBuild.createExplosion`: fire `explosion` when `should_explode` and
/// only once per building (the `Rules.reactorExplosions` outer gate is applied by
/// the caller, matching `onDestroyed`). Returns whether the trigger fired.
pub fn fire_explosion(
    world: &mut World,
    entity: Entity,
    explosion: ReactorExplosion,
    should_explode: bool,
) -> bool {
    if !should_explode || world.get::<ExplosionFired>(entity).is_some() {
        return false;
    }
    world.entity_mut(entity).insert(ExplosionFired);
    if let Some(mut sink) = world.remove_resource::<ExplosionSinkRes>() {
        if let Some(implementation) = sink.0.as_mut() {
            implementation.explode(world, explosion);
        }
        world.insert_resource(sink);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::world::TilePos;
    use crate::world::block::BlockTable;
    use crate::world::limits::BuildRules;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingSink {
        calls: Arc<AtomicUsize>,
    }

    impl ExplosionSink for CountingSink {
        fn explode(&self, _world: &mut World, _explosion: ReactorExplosion) {
            self.calls.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn building() -> (World, Entity) {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table
            .get_named("combustion-generator")
            .expect("gen")
            .clone();
        let mut world = World::new();
        world.insert_resource(BuildRules::default());
        let entity = inst.spawn(
            &mut world,
            0,
            TilePos::new(4, 4),
            1,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        world.insert_resource(table);
        (world, entity)
    }

    #[test]
    fn trigger_fires_exactly_once() {
        let (mut world, entity) = building();
        let calls = Arc::new(AtomicUsize::new(0));
        world.insert_resource(ExplosionSinkRes(Some(Box::new(CountingSink {
            calls: calls.clone(),
        }))));
        let explosion =
            ReactorExplosion::for_building(&world, entity, 12.0, 0.0, false).expect("params");
        assert!(fire_explosion(&mut world, entity, explosion, true));
        assert!(!fire_explosion(&mut world, entity, explosion, true));
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert!(world.get::<ExplosionFired>(entity).is_some());
    }

    #[test]
    fn gate_and_default_sink_are_inert() {
        let (mut world, entity) = building();
        let explosion =
            ReactorExplosion::for_building(&world, entity, 12.0, 0.0, false).expect("params");
        // `shouldExplode()` false -> no marker, no fire.
        assert!(!fire_explosion(&mut world, entity, explosion, false));
        assert!(world.get::<ExplosionFired>(entity).is_none());
        // Default sink (no resource) still marks exactly once.
        assert!(fire_explosion(&mut world, entity, explosion, true));
        assert!(!fire_explosion(&mut world, entity, explosion, true));
    }

    #[test]
    fn params_capture_tile_center_and_team() {
        let (world, entity) = building();
        let explosion =
            ReactorExplosion::for_building(&world, entity, 19.0, 5000.0, true).expect("params");
        assert_eq!(explosion.team, Some(1));
        assert_eq!(explosion.radius, 19.0);
        assert_eq!(explosion.damage, 5000.0);
        assert_eq!(explosion.x, 4.5 * TILE_SIZE);
        assert_eq!(explosion.y, 4.5 * TILE_SIZE);
    }
}
