// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Sandbox power/liquid sources and sinks
//! (`world/blocks/sandbox/{PowerSource,PowerVoid,LiquidSource,LiquidVoid}.java`).

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::LiquidId;
use crate::world::modules::{LiquidModule, PowerModule};

use super::PowerProduction;

/// Marker inserted when a generator self-destructs (e.g. `explodeOnFull`).
///
/// Plan 07/10 own the actual despawn/damage; plan 09 only raises the trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component)]
pub struct Destroyed;

/// `Building.kill()` stand-in: flags the building for destruction.
pub fn kill_building(world: &mut World, entity: Entity) {
    world.entity_mut(entity).insert(Destroyed);
    if let Some(mut module) = world.get_mut::<PowerModule>(entity) {
        module.status = 0.0;
    }
}

/// `PowerSource.updateTile`: always produce `power_production`.
pub fn update_power_source(world: &mut World, entity: Entity, power_production: f32) {
    world
        .entity_mut(entity)
        .insert(PowerProduction(power_production));
    if let Some(mut module) = world.get_mut::<PowerModule>(entity) {
        module.status = 1.0;
    }
}

/// `PowerVoid.updateTile`: consume an unbounded amount, always satisfied.
pub fn update_power_void(world: &mut World, entity: Entity) {
    if let Some(mut module) = world.get_mut::<PowerModule>(entity) {
        module.status = 1.0;
    }
}

/// `LiquidSource.updateTile`: fill to capacity with the configured liquid.
pub fn update_liquid_source(world: &mut World, entity: Entity, liquid: LiquidId, capacity: f32) {
    let Some(mut module) = world.get_mut::<LiquidModule>(entity) else {
        return;
    };
    if liquid.index() < module.liquids.len() {
        module.liquids[liquid.index()] = capacity;
        module.current_amount = module.liquids.iter().copied().sum();
    }
}

/// `LiquidVoid.updateTile`: empty the module.
pub fn update_liquid_void(world: &mut World, entity: Entity) {
    let Some(mut module) = world.get_mut::<LiquidModule>(entity) else {
        return;
    };
    module.liquids.fill(0.0);
    module.current_amount = 0.0;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::modules::LiquidModule;
    use bevy_ecs::world::World;

    #[test]
    fn liquid_source_fills_to_capacity() {
        let mut world = World::new();
        let entity = world.spawn(LiquidModule::with_liquids(2)).id();
        update_liquid_source(&mut world, entity, LiquidId::WATER, 20.0);
        let module = world.get::<LiquidModule>(entity).expect("module");
        assert_eq!(module.get(LiquidId::WATER), 20.0);
        update_liquid_void(&mut world, entity);
        assert_eq!(
            world.get::<LiquidModule>(entity).expect("module").current(),
            0.0
        );
    }
}
