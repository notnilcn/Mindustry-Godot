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

/// `LaunchPad` behavior: accumulate `launchCounter`, launch a full pad
/// (`LaunchPad.java:138-161`), item acceptance (`:138-140`) and IO (`:206-222`).
#[derive(Debug, Default, Clone, Copy)]
pub struct LaunchPadBehavior;

impl BuildingBehavior for LaunchPadBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<CampaignState>(e).is_none() {
            world.entity_mut(e).insert(CampaignState::default());
        }
    }

    /// `LaunchPad.shouldConsume` (`LaunchPad.java:92-95`).
    fn should_consume(&self, world: &World, e: Entity) -> bool {
        let launch_time = instance_launch_time(world, e);
        world
            .get::<CampaignState>(e)
            .is_some_and(|state| state.launch_time < launch_time)
    }

    /// `LaunchPad.acceptItem` (`LaunchPad.java:138-140`): a single-type pad
    /// only takes the item it already holds.
    fn accept_item(&self, world: &World, e: Entity, _src: Entity, item: ItemId) -> bool {
        let Some(items) = world.get::<crate::world::modules::ItemModule>(e) else {
            return false;
        };
        items.total() < launch_item_capacity(world, e)
            && (instance_accept_multiple(world, e)
                || items.total() == 0
                || items.first() == Some(item))
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        // `LaunchPad.updateTile` (`LaunchPad.java:143-161`): `launchCounter +=
        // edelta()` (efficiency is already in `edelta`), and a full pad fires
        // at `launchTime`.
        let edelta = edelta(world, e);
        let launch_time = instance_launch_time(world, e);
        if let Some(mut state) = world.get_mut::<CampaignState>(e) {
            state.launch_time += edelta;
            if state.launch_time < launch_time {
                return;
            }
        }
        let capacity = launch_item_capacity(world, e);
        if capacity <= 0
            || world
                .get::<crate::world::modules::ItemModule>(e)
                .map(|items| items.total())
                .unwrap_or(0)
                < capacity
        {
            return;
        }
        // `consume(); LaunchPayload.create(); items.each(...); entity.add();
        // items.clear(); launchCounter = 0`.
        crate::world::update::trigger_consumers(world, e);
        let stacks: Vec<(ItemId, i32)> = world
            .get::<crate::world::modules::ItemModule>(e)
            .map(|items| {
                items
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, amount)| **amount > 0)
                    .map(|(index, amount)| (ItemId::new(index as u16), *amount))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(mut items) = world.get_mut::<crate::world::modules::ItemModule>(e) {
            items.clear();
        }
        let (x, y) = world
            .get::<crate::entities::comp::Pos>(e)
            .map(|pos| (pos.x, pos.y))
            .unwrap_or((0.0, 0.0));
        let team = world
            .get::<crate::entities::comp::TeamComp>(e)
            .map(|team| team.team)
            .unwrap_or(0);
        let block = world
            .get::<Building>(e)
            .map(|building| building.block)
            .unwrap_or(crate::content::BlockId::AIR);
        let id = crate::game::launch_payload::next_payload_id(world);
        world.spawn((
            crate::entities::comp::Pos { x, y },
            crate::entities::comp::TeamComp { team },
            crate::game::launch_payload::LaunchPayloadState {
                id,
                stacks,
                time: 0.0,
                lifetime: 120.0,
                block,
            },
        ));
        if let Some(mut state) = world.get_mut::<CampaignState>(e) {
            state.launch_time = 0.0;
        }
    }

    /// `LaunchPad` IO version 1 (`LaunchPad.java:206-222`).
    fn version(&self, _world: &World, _e: Entity) -> u8 {
        1
    }

    fn write(&self, world: &World, e: Entity, w: &mut BuildingWriter) {
        if let Some(state) = world.get::<CampaignState>(e) {
            w.f(state.launch_time);
        }
    }

    fn read(&self, world: &mut World, e: Entity, r: &mut BuildingReader, revision: u8) {
        let launch_time = r.f().unwrap_or(0.0);
        if revision >= 1
            && let Some(mut state) = world.get_mut::<CampaignState>(e)
        {
            state.launch_time = launch_time;
        }
    }
}

/// `LaunchPadDef` knobs for a placed pad.
fn launch_pad_def(world: &World, e: Entity) -> (f32, bool) {
    let Some(building) = world.get::<Building>(e) else {
        return (0.0, false);
    };
    world
        .get_resource::<crate::world::block::BlockTable>()
        .and_then(|table| table.instance(building.block))
        .map(|inst| match &inst.kind_data {
            BlockKindData::LaunchPad(def) => (def.launch_time, def.accept_multiple_items),
            _ => (0.0, false),
        })
        .unwrap_or((0.0, false))
}

fn instance_launch_time(world: &World, e: Entity) -> f32 {
    launch_pad_def(world, e).0
}

fn instance_accept_multiple(world: &World, e: Entity) -> bool {
    launch_pad_def(world, e).1
}

fn launch_item_capacity(world: &World, e: Entity) -> i32 {
    crate::world::blocks::distribution::transfer::item_capacity(world, e)
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

    /// A placed pad with its behavior state (`H04 T6` tests).
    fn pad(
        name: &str,
    ) -> (
        EcsWorld,
        std::sync::Arc<crate::world::block::BlockInstance>,
        Entity,
        crate::content::ContentRegistry,
    ) {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table.get_named(name).expect("pad").clone();
        let mut world = EcsWorld::new();
        world.insert_resource(BuildRules::default());
        let e = inst.spawn(
            &mut world,
            0,
            TilePos::new(4, 4),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        inst.behavior.create_state(&mut world, e);
        world.insert_resource(table);
        (world, inst, e, content)
    }

    #[test]
    fn launch_pad_accepts_single_item_type() {
        // `advanced-launch-pad`: `acceptMultipleItems = false`
        // (`LaunchPad.java:138-140`).
        let (mut world, inst, e, content) = pad("advanced-launch-pad");
        let copper = content.item_id("copper").expect("copper");
        let lead = content.item_id("lead").expect("lead");
        assert!(inst.behavior.accept_item(&world, e, e, copper));
        if let Some(mut items) = world.get_mut::<crate::world::modules::ItemModule>(e) {
            items.add(copper, 100, 100);
        }
        assert!(
            !inst.behavior.accept_item(&world, e, e, lead),
            "a full single-type pad rejects lead"
        );
        if let Some(mut items) = world.get_mut::<crate::world::modules::ItemModule>(e) {
            items.remove(copper, 90);
        }
        assert!(
            !inst.behavior.accept_item(&world, e, e, lead),
            "a non-empty single-type pad rejects another item type"
        );
        // `launch-pad`: both types are accepted.
        let (mut world, inst, e, content) = pad("launch-pad");
        let copper = content.item_id("copper").expect("copper");
        let lead = content.item_id("lead").expect("lead");
        if let Some(mut items) = world.get_mut::<crate::world::modules::ItemModule>(e) {
            items.add(copper, 50, 100);
        }
        assert!(inst.behavior.accept_item(&world, e, e, lead));
    }

    #[test]
    fn launch_pad_launches_full_stack() {
        let (mut world, inst, e, content) = pad("advanced-launch-pad");
        let copper = content.item_id("copper").expect("copper");
        if let Some(mut items) = world.get_mut::<crate::world::modules::ItemModule>(e) {
            items.add(copper, 100, 100);
        }
        assert!(
            inst.behavior.should_consume(&world, e),
            "a pad below launchTime still consumes"
        );
        if let Some(mut building) = world.get_mut::<Building>(e) {
            building.efficiency = 1.0;
        }
        if let Some(mut state) = world.get_mut::<CampaignState>(e) {
            state.launch_time = 1799.0;
        }
        inst.behavior.update_tile(&mut world, e);
        let module = world
            .get::<crate::world::modules::ItemModule>(e)
            .expect("items");
        assert_eq!(module.total(), 0, "the pad empties into the payload");
        let state = world.get::<CampaignState>(e).expect("state");
        assert_eq!(state.launch_time, 0.0);
        let mut query = world.query::<&crate::game::launch_payload::LaunchPayloadState>();
        let payloads: Vec<_> = query.iter(&world).collect();
        assert_eq!(payloads.len(), 1, "one LaunchPayload entity");
        assert_eq!(payloads[0].stacks, vec![(copper, 100)]);
        assert_eq!(payloads[0].lifetime, 120.0);
    }

    #[test]
    fn launch_pad_io_v1_roundtrip() {
        let (mut world, inst, e, _content) = pad("advanced-launch-pad");
        if let Some(mut state) = world.get_mut::<CampaignState>(e) {
            state.launch_time = 777.5;
        }
        assert_eq!(inst.behavior.version(&world, e), 1);
        let mut buf = Vec::new();
        {
            let mut w: BuildingWriter = crate::io::wire::WireWriter::new(&mut buf);
            inst.behavior.write(&world, e, &mut w);
        }
        if let Some(mut state) = world.get_mut::<CampaignState>(e) {
            state.launch_time = 0.0;
        }
        {
            let mut r: BuildingReader = crate::io::wire::WireReader::new(&buf);
            inst.behavior.read(&mut world, e, &mut r, 1);
            assert_eq!(r.remaining(), 0);
        }
        assert_eq!(
            world.get::<CampaignState>(e).expect("state").launch_time,
            777.5
        );
    }
}
