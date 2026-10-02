// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Campaign block building halves (`world/blocks/campaign/*`).
//!
//! Plan 07 §3.12: `Accelerator`, `LandingPad`, `LaunchPad`. Launch targets,
//! resource transfer and `Universe` effects are plan 12 via a
//! `LaunchTargetProvider`; this module owns the per-building state machine and
//! config so the blocks round-trip and update deterministically.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ItemId;
use crate::entities::comp::{Building, CampaignState};
use crate::world::BlockKindData;
use crate::world::update::edelta;

use super::{BuildingBehavior, BuildingReader, BuildingWriter};

/// `Accelerator` behavior (heat charge state machine).
#[derive(Debug, Default, Clone, Copy)]
pub struct AcceleratorBehavior;

impl BuildingBehavior for AcceleratorBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<CampaignState>(e).is_none() {
            world.entity_mut(e).insert(CampaignState::default());
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let heat_time = instance_heat_time(world, e);
        let efficiency = world
            .get::<Building>(e)
            .map(|building| building.efficiency)
            .unwrap_or(0.0);
        let edelta = edelta(world, e);
        if let Some(mut state) = world.get_mut::<CampaignState>(e) {
            if efficiency > 0.0 {
                state.heat = (state.heat + edelta / heat_time.max(0.0001)).min(1.0);
            } else {
                state.heat = (state.heat - edelta / heat_time.max(0.0001)).max(0.0);
            }
        }
    }

    fn write(&self, world: &World, e: Entity, w: &mut BuildingWriter) {
        if let Some(state) = world.get::<CampaignState>(e) {
            w.f(state.heat);
        }
    }

    fn read(&self, world: &mut World, e: Entity, r: &mut BuildingReader, _revision: u8) {
        let heat = r.f().unwrap_or(0.0);
        if let Some(mut state) = world.get_mut::<CampaignState>(e) {
            state.heat = heat;
        }
    }
}

fn instance_heat_time(world: &World, e: Entity) -> f32 {
    let Some(building) = world.get::<Building>(e) else {
        return 1.0;
    };
    world
        .get_resource::<crate::world::block::BlockTable>()
        .and_then(|table| table.instance(building.block))
        .map(|inst| match &inst.kind_data {
            BlockKindData::Accelerator(def) => def.heat_time,
            _ => 1.0,
        })
        .unwrap_or(1.0)
}

/// `LandingPad` behavior (item config + landing cooldown).
#[derive(Debug, Default, Clone, Copy)]
pub struct LandingPadBehavior;

impl BuildingBehavior for LandingPadBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<CampaignState>(e).is_none() {
            world.entity_mut(e).insert(CampaignState::default());
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let edelta = edelta(world, e);
        if let Some(mut state) = world.get_mut::<CampaignState>(e)
            && state.cooldown > 0.0
        {
            state.cooldown = (state.cooldown - edelta).max(0.0);
        }
    }

    fn config(&self, world: &World, e: Entity) -> super::super::config::ConfigValue {
        world
            .get::<CampaignState>(e)
            .and_then(|state| state.item)
            .map(super::super::config::ConfigValue::Item)
            .unwrap_or(super::super::config::ConfigValue::None)
    }

    fn configured(
        &self,
        world: &mut World,
        e: Entity,
        _player: Option<Entity>,
        value: super::super::config::ConfigValue,
    ) {
        if let Some(mut state) = world.get_mut::<CampaignState>(e) {
            match value {
                super::super::config::ConfigValue::Item(item) => state.item = Some(item),
                super::super::config::ConfigValue::None => state.item = None,
                _ => {}
            }
        }
    }
}

/// `LaunchPad` behavior (launch timer + config).
#[derive(Debug, Default, Clone, Copy)]
pub struct LaunchPadBehavior;

impl BuildingBehavior for LaunchPadBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<CampaignState>(e).is_none() {
            world.entity_mut(e).insert(CampaignState::default());
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let efficiency = world
            .get::<Building>(e)
            .map(|building| building.efficiency)
            .unwrap_or(0.0);
        let edelta = edelta(world, e);
        if let Some(mut state) = world.get_mut::<CampaignState>(e)
            && efficiency > 0.0
        {
            state.launch_time += edelta;
        }
    }

    fn write(&self, world: &World, e: Entity, w: &mut BuildingWriter) {
        if let Some(state) = world.get::<CampaignState>(e) {
            w.f(state.launch_time);
        }
    }

    fn read(&self, world: &mut World, e: Entity, r: &mut BuildingReader, _revision: u8) {
        let launch_time = r.f().unwrap_or(0.0);
        if let Some(mut state) = world.get_mut::<CampaignState>(e) {
            state.launch_time = launch_time;
        }
    }
}

/// Campaign launch target (plan 12 supplies the real destination/resource logic).
pub trait LaunchTargetProvider: Send + Sync {
    /// Whether the launch can proceed.
    fn can_launch(&self, world: &World, entity: Entity) -> bool {
        let _ = (world, entity);
        true
    }
}

/// Item configured on a landing pad (read helper for plan 12).
pub fn landing_item(world: &World, e: Entity) -> Option<ItemId> {
    world.get::<CampaignState>(e).and_then(|state| state.item)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::world::TilePos;
    use crate::world::block::BlockTable;
    use crate::world::limits::BuildRules;
    use bevy_ecs::world::World as EcsWorld;

    #[test]
    fn landing_pad_config_roundtrips() {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table.get_named("landing-pad").expect("landing-pad").clone();
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
        world.insert_resource(table);
        let copper = content.item_id("copper").expect("copper");
        inst.behavior.configured(
            &mut world,
            e,
            None,
            crate::world::config::ConfigValue::Item(copper),
        );
        assert_eq!(
            inst.behavior.config(&world, e),
            crate::world::config::ConfigValue::Item(copper)
        );
        assert_eq!(landing_item(&world, e), Some(copper));
    }
}
