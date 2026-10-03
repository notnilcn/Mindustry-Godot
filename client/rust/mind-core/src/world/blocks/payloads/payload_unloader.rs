// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PayloadUnloader` build behavior
//! (`world/blocks/payloads/PayloadUnloader.java`) — plan 08 M7.
//!
//! Drains a carried build payload's items, liquids and power into the block
//! inventory, dumps items/liquids to neighbours and publishes the unloaded
//! power as `PowerProduction` (R12, ported against plan 09's module ABI).

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::entities::comp::Building;
use crate::world::behavior::{BuildingBehavior, PayloadRef};
use crate::world::block::BlockTable;
use crate::world::blocks::distribution::transfer;
use crate::world::blocks::liquid::current_liquid;
use crate::world::blocks::liquid::movement::dump_liquid_proximity;
use crate::world::blocks::power::{PowerProduction, read_power_info};
use crate::world::modules::{ItemModule, LiquidModule, PowerModule};
use crate::world::update::{edelta, run_timer};

use super::PayloadHolder;
use super::payload_block::{move_in_payload, move_out_payload, payload_block_update};
use super::payload_loader::{
    PayloadLoaderBehavior, block_def, ensure_liquid_node, liquid_capacity,
};

/// `PayloadUnloader.PayloadUnloaderBuild` extra state.
#[derive(Debug, Clone, Copy, Component, Default)]
pub struct PayloadUnloaderBuild {
    /// Power produced this tick (`lastOutputPower`).
    pub last_output_power: f32,
}

/// `PayloadUnloader` behavior (`payload-unloader`).
#[derive(Debug, Clone, Copy)]
pub struct PayloadUnloaderBehavior {
    /// Loader half.
    pub base: PayloadLoaderBehavior,
    /// `offloadSpeed` (item dump attempts per tick).
    pub offload_speed: i32,
    /// `maxPowerUnload` (per tick).
    pub max_power_unload: f32,
}

impl PayloadUnloaderBehavior {
    /// Vanilla `payload-unloader`.
    pub const VANILLA: PayloadUnloaderBehavior = PayloadUnloaderBehavior {
        base: PayloadLoaderBehavior::VANILLA,
        offload_speed: 4,
        max_power_unload: 80.0,
    };
}

/// `PayloadUnloaderBuild.full()`.
pub fn unloader_full(world: &World, e: Entity) -> bool {
    let cap = transfer::item_capacity(world, e);
    world.get::<ItemModule>(e).is_some_and(|m| m.total() >= cap)
}

/// `PayloadUnloaderBuild.shouldExport()`: the payload has been fully drained
/// (no items, no liquids, no stored power).
pub fn unloader_should_export(world: &World, e: Entity) -> bool {
    let Some(payload) = world.get::<PayloadHolder>(e).and_then(|h| h.payload) else {
        return false;
    };
    let Some(payload_entity) = payload.entity else {
        return true;
    };
    let Some(block) = world.get::<Building>(payload_entity).map(|b| b.block) else {
        return true;
    };
    let def = world
        .get_resource::<BlockTable>()
        .and_then(|table| table.get(block).map(|inst| inst.def.clone()));
    let Some(def) = def else {
        return true;
    };
    let items_empty = !def.has_items
        || world
            .get::<ItemModule>(payload_entity)
            .is_none_or(|m| m.total() == 0);
    let liquids_empty = !def.has_liquids
        || world
            .get::<LiquidModule>(payload_entity)
            .is_none_or(|m| m.current_amount <= 0.011);
    let battery_empty = !read_power_info(world, payload_entity).is_some_and(|p| p.buffered)
        || world
            .get::<PowerModule>(payload_entity)
            .is_none_or(|m| m.status <= 0.000_000_1);
    items_empty && liquids_empty && battery_empty
}

impl BuildingBehavior for PayloadUnloaderBehavior {
    fn update_batch(
        &self,
        world: &mut bevy_ecs::world::World,
        inst: &crate::world::block::BlockInstance,
        entities: &[bevy_ecs::entity::Entity],
    ) {
        // Plan 08 §7.4: allocate-free batched dispatch (empty-consumer fast path).
        for &e in entities {
            crate::world::update::building_update_no_consumers(world, e, inst);
        }
    }

    fn create_state(&self, world: &mut World, e: Entity) {
        self.base.create_state(world, e);
        if world.get::<PayloadUnloaderBuild>(e).is_none() {
            world.entity_mut(e).insert(PayloadUnloaderBuild::default());
        }
        ensure_liquid_node(world, e);
        if world.get::<PowerProduction>(e).is_none() {
            world.entity_mut(e).insert(PowerProduction(0.0));
        }
    }

    fn accept_payload(
        &self,
        world: &World,
        e: Entity,
        source: Entity,
        payload: PayloadRef,
    ) -> bool {
        self.base.accept_payload(world, e, source, payload)
    }

    fn handle_payload(&self, world: &mut World, e: Entity, source: Entity, payload: PayloadRef) {
        self.base.handle_payload(world, e, source, payload);
    }

    fn get_payload(&self, world: &World, e: Entity) -> Option<PayloadRef> {
        self.base.get_payload(world, e)
    }

    fn take_payload(&self, world: &mut World, e: Entity) -> Option<PayloadRef> {
        super::take_payload(world, e)
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        payload_block_update(world, e);
        if let Some(mut state) = world.get_mut::<PayloadUnloaderBuild>(e) {
            state.last_output_power = 0.0;
        }

        if unloader_should_export(world, e) {
            move_out_payload(world, e);
        } else if move_in_payload(world, e, true) {
            let efficiency = world
                .get::<Building>(e)
                .map(|b| b.efficiency)
                .unwrap_or(1.0);
            let payload_entity = world
                .get::<PayloadHolder>(e)
                .and_then(|h| h.payload)
                .and_then(|p| p.entity);
            if let Some(payload_entity) = payload_entity {
                let def = world
                    .get::<Building>(payload_entity)
                    .map(|b| b.block)
                    .and_then(|block| block_def(world, block));

                // Unload items (`PayloadUnloader.java:73-85`).
                if def.as_ref().is_some_and(|def| def.has_items)
                    && !unloader_full(world, e)
                    && efficiency > 0.01
                    && run_timer(
                        world,
                        e,
                        0,
                        self.base.load_time / efficiency.max(f32::EPSILON),
                    )
                {
                    let capacity = transfer::item_capacity(world, e).max(0);
                    let mut batch = 0;
                    while batch < self.base.items_loaded && !unloader_full(world, e) {
                        batch += 1;
                        let item_count = world
                            .get::<ItemModule>(payload_entity)
                            .map(|m| m.items.len())
                            .unwrap_or(0);
                        let mut moved = false;
                        for index in 0..item_count {
                            let amount = world
                                .get::<ItemModule>(payload_entity)
                                .and_then(|m| m.items.get(index).copied())
                                .unwrap_or(0);
                            if amount <= 0 {
                                continue;
                            }
                            let item = crate::content::ItemId::new(index as u16);
                            if let Some(mut payload_items) =
                                world.get_mut::<ItemModule>(payload_entity)
                            {
                                payload_items.remove(item, 1);
                            }
                            if let Some(mut items) = world.get_mut::<ItemModule>(e) {
                                items.add(item, 1, capacity);
                            }
                            moved = true;
                            break;
                        }
                        if !moved {
                            break;
                        }
                    }
                }

                // Unload liquids (`PayloadUnloader.java:86-96`).
                if def.as_ref().is_some_and(|def| def.has_liquids) {
                    let payload_amount = world
                        .get::<LiquidModule>(payload_entity)
                        .map(|m| m.current_amount)
                        .unwrap_or(0.0);
                    if payload_amount >= 0.01 {
                        let loader_current = world.get::<LiquidModule>(e).and_then(current_liquid);
                        let loader_amount = world
                            .get::<LiquidModule>(e)
                            .map(|m| m.current_amount)
                            .unwrap_or(0.0);
                        if let Some(liquid) = world
                            .get::<LiquidModule>(payload_entity)
                            .and_then(current_liquid)
                            && (loader_current == Some(liquid) || loader_amount <= 0.2)
                        {
                            let capacity = liquid_capacity(world, e);
                            let remaining = capacity - loader_amount;
                            let flow = (self.base.liquids_loaded * edelta(world, e))
                                .min(remaining)
                                .min(payload_amount)
                                .max(0.0);
                            if flow > 0.0 {
                                if let Some(mut module) = world.get_mut::<LiquidModule>(e) {
                                    module.add(liquid, flow, capacity);
                                }
                                if let Some(mut module) =
                                    world.get_mut::<LiquidModule>(payload_entity)
                                {
                                    module.remove(liquid, flow);
                                }
                            }
                        }
                    }
                }

                // Unload power (`PayloadUnloader.java:98-104`).
                if let Some(params) = read_power_info(world, payload_entity)
                    && params.buffered
                    && params.capacity > 0.0
                {
                    let capacity = params.capacity;
                    let total = world
                        .get::<PowerModule>(payload_entity)
                        .map(|m| m.status * capacity)
                        .unwrap_or(0.0);
                    let unloaded = (self.max_power_unload * edelta(world, e)).min(total);
                    if let Some(mut state) = world.get_mut::<PayloadUnloaderBuild>(e) {
                        state.last_output_power = unloaded;
                    }
                    if let Some(mut module) = world.get_mut::<PowerModule>(payload_entity) {
                        module.status -= unloaded / capacity;
                    }
                }
            }
        }

        // `dumpLiquid(liquids.current())` then the item offload attempts.
        if let Some(liquid) = world.get::<LiquidModule>(e).and_then(current_liquid) {
            dump_liquid_proximity(world, e, liquid, 2.0, -1);
        }
        for _ in 0..self.offload_speed {
            let _ = transfer::dump_accumulate(world, e, None);
        }
        // Publish `getPowerProduction()`.
        let output = world
            .get::<PayloadUnloaderBuild>(e)
            .map(|state| state.last_output_power)
            .unwrap_or(0.0);
        if let Some(mut production) = world.get_mut::<PowerProduction>(e) {
            production.0 = output;
        }
    }

    fn accept_item(
        &self,
        _world: &World,
        _e: Entity,
        _src: Entity,
        _item: crate::content::ItemId,
    ) -> bool {
        false
    }

    fn accept_liquid(
        &self,
        _world: &World,
        _e: Entity,
        _src: Entity,
        _liquid: crate::content::LiquidId,
    ) -> bool {
        // `PayloadUnloaderBuild.acceptLiquid` rejects all input.
        false
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        1
    }

    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        self.base.write(world, e, w);
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        revision: u8,
    ) {
        self.base.read(world, e, r, revision);
        if world.get::<PayloadUnloaderBuild>(e).is_none() {
            world.entity_mut(e).insert(PayloadUnloaderBuild::default());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::LiquidId;
    use crate::world::BuildHarness;

    #[test]
    fn unloader_drains_liquid_to_neighbour() {
        let mut harness = BuildHarness::new(24, 24, 7);
        if let Some(mut rules) = harness
            .world
            .get_resource_mut::<crate::world::limits::BuildRules>()
        {
            rules.cheat = true;
        }
        let unloader = harness
            .content()
            .block_id("payload-unloader")
            .expect("payload-unloader");
        let container = harness
            .content()
            .block_id("liquid-container")
            .expect("liquid-container");
        let tank = harness.content().block_id("liquid-tank").expect("tank");
        assert!(harness.place(6, 6, unloader, 0, true));
        assert!(harness.place(9, 6, tank, 0, true));
        let unloader_e = harness.build_at(6, 6).expect("unloader");
        let neighbour_e = harness.build_at(9, 6).expect("tank");

        let payload_entity =
            super::super::create_build_payload(&mut harness.world, container, 0).expect("payload");
        if let Some(mut module) = harness.world.get_mut::<LiquidModule>(payload_entity) {
            module.add(LiquidId::WATER, 50.0, 700.0);
        }
        let payload = PayloadRef {
            entity: Some(payload_entity),
            content: container.raw(),
            is_block: true,
        };
        super::super::handle_payload(&mut harness.world, unloader_e, unloader_e, payload);
        for _ in 0..60 {
            harness.tick();
        }
        let neighbour_water = harness
            .world
            .get::<LiquidModule>(neighbour_e)
            .map(|m| m.current_amount)
            .unwrap_or(0.0);
        assert!(
            neighbour_water > 0.0,
            "neighbour should receive dumped liquid: {neighbour_water}"
        );
        assert!(
            harness
                .world
                .get::<LiquidModule>(payload_entity)
                .is_some_and(|m| m.current_amount <= 0.011)
        );
    }

    #[test]
    fn unloader_publishes_unloaded_power() {
        let mut harness = BuildHarness::new(24, 24, 7);
        if let Some(mut rules) = harness
            .world
            .get_resource_mut::<crate::world::limits::BuildRules>()
        {
            rules.cheat = true;
        }
        let unloader = harness
            .content()
            .block_id("payload-unloader")
            .expect("payload-unloader");
        let battery = harness.content().block_id("battery").expect("battery");
        assert!(harness.place(6, 6, unloader, 0, true));
        let unloader_e = harness.build_at(6, 6).expect("unloader");
        let payload_entity =
            super::super::create_build_payload(&mut harness.world, battery, 0).expect("payload");
        if let Some(mut module) = harness.world.get_mut::<PowerModule>(payload_entity) {
            module.status = 0.5;
        }
        let payload = PayloadRef {
            entity: Some(payload_entity),
            content: battery.raw(),
            is_block: true,
        };
        super::super::handle_payload(&mut harness.world, unloader_e, unloader_e, payload);
        harness.tick();
        let output = harness
            .world
            .get::<PayloadUnloaderBuild>(unloader_e)
            .map(|s| s.last_output_power)
            .unwrap_or(0.0);
        assert!(output > 0.0, "unloader should output power: {output}");
        assert_eq!(
            harness
                .world
                .get::<PowerProduction>(unloader_e)
                .map(|p| p.0),
            Some(output)
        );
    }
}
