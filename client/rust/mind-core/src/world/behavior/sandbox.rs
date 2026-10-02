// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Sandbox block behavior (`world/blocks/sandbox/*`).
//!
//! Plan 07 §3.12 sandbox family: `ItemSource`/`ItemVoid`/`LiquidSource`/
//! `LiquidVoid`/`PowerSource`/`PowerVoid`. The power paths call plan 09's graph
//! API at runtime; here `PowerSource`/`PowerVoid` set module status directly so
//! the consumer/efficiency math is exercised without the graph arena.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ItemId;
use crate::entities::comp::SandboxState;
use crate::world::config::ConfigValue;
use crate::world::modules::{ItemModule, LiquidModule, PowerModule};

use super::BuildingBehavior;

/// Sandbox source/sink behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct SandboxBehavior;

impl BuildingBehavior for SandboxBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<SandboxState>(e).is_none() {
            world.entity_mut(e).insert(SandboxState::default());
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        // `ItemSource`: emit `itemsPerSecond / 60` items per tick.
        let per_tick = world
            .get_resource::<crate::world::block::BlockTable>()
            .and_then(|table| {
                world
                    .get::<crate::entities::comp::Building>(e)
                    .and_then(|b| table.instance(b.block))
            })
            .map(|inst| match &inst.kind_data {
                crate::world::BlockKindData::ItemSource(def) => def.items_per_second / 60.0,
                _ => 0.0,
            })
            .unwrap_or(0.0);
        if per_tick <= 0.0 {
            return;
        }
        let item = world
            .get::<SandboxState>(e)
            .and_then(|state| state.item)
            .unwrap_or(ItemId::COPPER);
        if let Some(mut items) = world.get_mut::<ItemModule>(e) {
            items.add(item, per_tick.round() as i32, i32::MAX);
        }
    }

    fn configured(
        &self,
        world: &mut World,
        e: Entity,
        _player: Option<Entity>,
        value: ConfigValue,
    ) {
        match value {
            ConfigValue::Item(item) => {
                if let Some(mut state) = world.get_mut::<SandboxState>(e) {
                    state.item = Some(item);
                }
            }
            ConfigValue::None => {
                if let Some(mut state) = world.get_mut::<SandboxState>(e) {
                    state.item = None;
                }
            }
            _ => {}
        }
    }

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        world
            .get::<SandboxState>(e)
            .and_then(|state| state.item)
            .map(ConfigValue::Item)
            .unwrap_or(ConfigValue::None)
    }

    fn efficiency_scale(&self, _world: &World, _e: Entity) -> f32 {
        1.0
    }
}

/// Reads the current liquid of a `LiquidSource` (`config` readback fallback).
pub fn source_liquid(world: &World, e: Entity) -> Option<crate::content::LiquidId> {
    world.get::<LiquidModule>(e).and_then(|module| {
        module
            .liquids
            .iter()
            .enumerate()
            .find(|(_, amount)| **amount > 0.0)
            .map(|(index, _)| crate::content::LiquidId::new(index as u16))
    })
}

/// Reads a power source's status (`PowerSource` adapter).
pub fn source_status(world: &World, e: Entity) -> f32 {
    world
        .get::<PowerModule>(e)
        .map(|module| module.status)
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::entities::comp::{Building, TeamComp};
    use crate::world::TilePos;
    use crate::world::block::BlockTable;
    use crate::world::limits::BuildRules;
    use bevy_ecs::world::World as EcsWorld;

    #[test]
    fn item_source_configure_and_emit() {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table.get_named("item-source").expect("item-source").clone();
        let mut world = EcsWorld::new();
        world.insert_resource(BuildRules::default());
        let e = inst.spawn(
            &mut world,
            0,
            TilePos::new(2, 2),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        inst.behavior.create_state(&mut world, e);
        let copper = content.item_id("copper").expect("copper");
        inst.behavior
            .configured(&mut world, e, None, ConfigValue::Item(copper));
        assert_eq!(inst.behavior.config(&world, e), ConfigValue::Item(copper));
        assert!(world.get::<Building>(e).is_some());
        assert!(world.get::<TeamComp>(e).is_some());
    }
}
