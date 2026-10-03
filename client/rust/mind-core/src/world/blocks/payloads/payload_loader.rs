// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PayloadLoader` build behavior
//! (`world/blocks/payloads/PayloadLoader.java`) — plan 08 M7.
//!
//! Loads items, liquids and power into a carried build payload. The liquid/power
//! paths are ported against plan 09's module ABI (`LiquidModule` capacity,
//! `read_power_info().capacity`/`buffered`) — R12.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::entities::comp::Building;
use crate::world::behavior::{BuildingBehavior, PayloadRef};
use crate::world::block::BlockTable;
use crate::world::blocks::distribution::transfer;
use crate::world::blocks::liquid::{LiquidNode, accept_liquid, current_liquid};
use crate::world::blocks::power::read_power_info;
use crate::world::modules::{ItemModule, LiquidModule, PowerModule};
use crate::world::update::{edelta, run_timer};

use super::payload_block::{
    accept_payload_base, move_in_payload, move_out_payload, payload_block_update,
};
use super::{PayloadHolder, PayloadKind, kind_of, payload_fits};

/// `PayloadLoader.PayloadLoaderBuild` extra state.
#[derive(Debug, Clone, Copy, Component, Default)]
pub struct PayloadLoaderBuild {
    /// Whether the payload is ready to be expelled (`exporting`).
    pub exporting: bool,
}

/// `PayloadLoader` behavior (`payload-loader`).
#[derive(Debug, Clone, Copy)]
pub struct PayloadLoaderBehavior {
    /// `loadTime` (ticks).
    pub load_time: f32,
    /// `itemsLoaded` batch size.
    pub items_loaded: i32,
    /// `liquidsLoaded`.
    pub liquids_loaded: f32,
    /// `maxBlockSize`.
    pub max_block_size: f32,
    /// `maxPowerConsumption`.
    pub max_power_consumption: f32,
    /// `basePowerUse`.
    pub base_power_use: f32,
}

impl PayloadLoaderBehavior {
    /// Vanilla `payload-loader`.
    pub const VANILLA: PayloadLoaderBehavior = PayloadLoaderBehavior {
        load_time: 2.0,
        items_loaded: 8,
        liquids_loaded: 40.0,
        max_block_size: 3.0,
        max_power_consumption: 40.0,
        // `PayloadLoader.init`: `basePowerUse = consPower.usage` (the loader
        // consumes 2 power/tick), `loadPowerDynamic = true`.
        base_power_use: 2.0,
    };
}

/// Borrowed copy of the payload's block metadata (`Block` instance of a carried
/// build). Returns `None` for unknown blocks.
pub(crate) fn block_def(
    world: &World,
    block: crate::content::BlockId,
) -> Option<std::sync::Arc<crate::content::BlockDef>> {
    world
        .get_resource::<BlockTable>()
        .and_then(|table| table.get(block).map(|inst| inst.def.clone()))
}

/// `Block.liquidCapacity` for a building (0 when unknown).
pub(crate) fn liquid_capacity(world: &World, e: Entity) -> f32 {
    let Some(building) = world.get::<Building>(e) else {
        return 0.0;
    };
    world
        .get_resource::<BlockTable>()
        .and_then(|table| table.get(building.block))
        .map(|inst| inst.def.liquid_capacity)
        .unwrap_or(0.0)
}

/// `PayloadLoaderBuild.hasBattery()`: the payload block has a buffered power
/// consumer (`consPower.buffered`).
pub fn has_battery(world: &World, payload_entity: Entity) -> bool {
    read_power_info(world, payload_entity).is_some_and(|params| params.buffered)
}

/// Ensures the loader/unloader owns a plan-09 [`LiquidNode`] so external
/// liquid networks (and the unloader's `dumpLiquid`) can route through it.
pub(crate) fn ensure_liquid_node(world: &mut World, e: Entity) {
    if world.get::<LiquidNode>(e).is_some() {
        return;
    }
    let capacity = liquid_capacity(world, e);
    world.entity_mut(e).insert(LiquidNode {
        capacity,
        accepts: true,
        ..LiquidNode::default()
    });
}

/// Whether a build payload is loadable (`PayloadLoaderBuild.acceptPayload`).
pub fn loadable_payload(world: &World, payload: PayloadRef, max_block_size: f32) -> bool {
    if kind_of(payload) != PayloadKind::Build || !payload_fits(world, payload, max_block_size) {
        return false;
    }
    let Some(block) = payload
        .entity
        .and_then(|e| world.get::<Building>(e))
        .map(|b| b.block)
    else {
        return false;
    };
    world
        .get_resource::<BlockTable>()
        .and_then(|table| table.get(block))
        .is_some_and(|inst| {
            (inst.def.has_items && inst.def.item_capacity >= 10)
                || (inst.def.has_liquids && inst.def.liquid_capacity >= 10.0)
                || inst.def.has_power
        })
}

/// `PayloadLoaderBuild.shouldExport()`.
///
/// `exporting || (payload full of liquid) || (payload full of an item the loader
/// also holds; `separateItemCapacity` proxy) || (battery charged)`.
pub fn loader_should_export(world: &World, e: Entity) -> bool {
    let holder = world.get::<PayloadHolder>(e).copied().unwrap_or_default();
    let Some(payload) = holder.payload else {
        return false;
    };
    let exporting = world
        .get::<PayloadLoaderBuild>(e)
        .is_some_and(|s| s.exporting);
    if exporting {
        return true;
    }
    let Some(payload_entity) = payload.entity else {
        return false;
    };
    let Some(payload_block) = world.get::<Building>(payload_entity).map(|b| b.block) else {
        return false;
    };
    let Some(def) = block_def(world, payload_block) else {
        return false;
    };

    // Liquid container full.
    if def.has_liquids {
        let loader_amount = world
            .get::<LiquidModule>(e)
            .map(|m| m.current_amount)
            .unwrap_or(0.0);
        let payload_amount = world
            .get::<LiquidModule>(payload_entity)
            .map(|m| m.current_amount)
            .unwrap_or(0.0);
        if loader_amount >= 0.1 && payload_amount >= def.liquid_capacity - 0.001 {
            return true;
        }
    }

    // Item container full (`separateItemCapacity` proxy).
    let capacity = def.item_capacity;
    let loader_total = world.get::<ItemModule>(e).map(|m| m.total()).unwrap_or(0);
    if capacity > 0 && loader_total > 0 {
        let loader = world.get::<ItemModule>(e);
        let any_full = world
            .get::<ItemModule>(payload_entity)
            .is_some_and(|payload_items| {
                payload_items
                    .stacks()
                    .any(|(item, amount)| amount >= capacity && loader.is_some_and(|l| l.has(item)))
            });
        if any_full {
            return true;
        }
    }

    // Battery charged.
    if has_battery(world, payload_entity)
        && world
            .get::<PowerModule>(payload_entity)
            .is_some_and(|m| m.status >= 0.999_999_999)
    {
        return true;
    }
    false
}

/// `PayloadLoaderBuild.updateTile` core (shared with the unloader's base call).
pub fn loader_update(world: &mut World, e: Entity, behavior: &PayloadLoaderBehavior) {
    payload_block_update(world, e);
    if loader_should_export(world, e) {
        move_out_payload(world, e);
        return;
    }
    if !move_in_payload(world, e, true) {
        return;
    }
    let efficiency = world
        .get::<Building>(e)
        .map(|b| b.efficiency)
        .unwrap_or(1.0);
    let holder = world.get::<PayloadHolder>(e).copied().unwrap_or_default();
    let Some(payload) = holder.payload else {
        return;
    };
    let Some(payload_entity) = payload.entity else {
        return;
    };
    let Some(payload_block) = world.get::<Building>(payload_entity).map(|b| b.block) else {
        return;
    };
    let Some(def) = block_def(world, payload_block) else {
        return;
    };

    // Load up items.
    if def.has_items && world.get::<ItemModule>(e).is_some_and(|m| m.any()) {
        let mut accepted_any = false;
        if efficiency > 0.01
            && run_timer(
                world,
                e,
                0,
                behavior.load_time / efficiency.max(f32::EPSILON),
            )
        {
            accepted_any = false;
            'outer: for _ in 0..behavior.items_loaded {
                if !world.get::<ItemModule>(e).is_some_and(|m| m.any()) {
                    break;
                }
                let item_count = world
                    .get::<ItemModule>(e)
                    .map(|m| m.items.len())
                    .unwrap_or(0);
                for index in 0..item_count {
                    let amount = world
                        .get::<ItemModule>(e)
                        .and_then(|m| m.items.get(index).copied())
                        .unwrap_or(0);
                    if amount <= 0 {
                        continue;
                    }
                    let item = crate::content::ItemId::new(index as u16);
                    if transfer::dispatch_accept_item(world, payload_entity, e, item) {
                        transfer::dispatch_handle_item(world, payload_entity, e, item);
                        if let Some(mut items) = world.get_mut::<ItemModule>(e) {
                            items.remove(item, 1);
                        }
                        accepted_any = true;
                        break;
                    } else {
                        // Payload refuses this item: flag export on the next tick.
                        if let Some(mut state) = world.get_mut::<PayloadLoaderBuild>(e) {
                            state.exporting = true;
                        }
                        break 'outer;
                    }
                }
            }
        }
        if !accepted_any
            && !world.get::<ItemModule>(e).is_some_and(|m| m.any())
            && let Some(mut state) = world.get_mut::<PayloadLoaderBuild>(e)
        {
            state.exporting = true;
        }
    }

    // Load up liquids (`PayloadLoader.java:185-200`).
    if def.has_liquids {
        let loader_amount = world
            .get::<LiquidModule>(e)
            .map(|m| m.current_amount)
            .unwrap_or(0.0);
        if loader_amount >= 0.001
            && let Some(liquid) = world.get::<LiquidModule>(e).and_then(current_liquid)
        {
            let payload_stored = world
                .get::<LiquidModule>(payload_entity)
                .map(|m| m.get(liquid))
                .unwrap_or(0.0);
            let flow = (behavior.liquids_loaded * edelta(world, e))
                .min(def.liquid_capacity - payload_stored)
                .min(loader_amount)
                .max(0.0);
            if world.get::<LiquidModule>(payload_entity).is_some()
                && accept_liquid(world, payload_entity, e, liquid)
            {
                if let Some(mut module) = world.get_mut::<LiquidModule>(payload_entity) {
                    module.add(liquid, flow, def.liquid_capacity);
                }
                if let Some(mut module) = world.get_mut::<LiquidModule>(e) {
                    module.remove(liquid, flow);
                }
            } else if let Some(mut state) = world.get_mut::<PayloadLoaderBuild>(e) {
                // Payload refuses this liquid: flag export.
                state.exporting = true;
            }
        }
    }

    // Load up power (`PayloadLoader.java:199-215`).
    if let Some(params) = read_power_info(world, payload_entity)
        && params.buffered
        && params.capacity > 0.0
    {
        let loader_status = world.get::<PowerModule>(e).map(|m| m.status).unwrap_or(0.0);
        // `powerInput` in raw units; subtract the base usage to get what is
        // actually usable for charging the payload.
        let power_input =
            loader_status * (behavior.base_power_use + behavior.max_power_consumption);
        let available_input = (power_input - behavior.base_power_use).max(0.0);
        let charge = available_input / params.capacity * edelta(world, e);
        if let Some(mut module) = world.get_mut::<PowerModule>(payload_entity) {
            module.status += charge;
            if module.status >= 1.0 {
                module.status = module.status.clamp(0.0, 1.0);
                if let Some(mut state) = world.get_mut::<PayloadLoaderBuild>(e) {
                    state.exporting = true;
                }
            }
        }
    }
}

impl BuildingBehavior for PayloadLoaderBehavior {
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
        if world.get::<PayloadLoaderBuild>(e).is_none() {
            world.entity_mut(e).insert(PayloadLoaderBuild::default());
        }
        if world.get::<PayloadHolder>(e).is_none() {
            world.entity_mut(e).insert(PayloadHolder::default());
        }
        ensure_liquid_node(world, e);
    }

    fn accept_payload(
        &self,
        world: &World,
        e: Entity,
        _source: Entity,
        payload: PayloadRef,
    ) -> bool {
        accept_payload_base(world, e) && loadable_payload(world, payload, self.max_block_size)
    }

    fn handle_payload(&self, world: &mut World, e: Entity, source: Entity, payload: PayloadRef) {
        super::handle_payload(world, e, source, payload);
        if let Some(mut state) = world.get_mut::<PayloadLoaderBuild>(e) {
            state.exporting = false;
        }
    }

    fn get_payload(&self, world: &World, e: Entity) -> Option<PayloadRef> {
        world.get::<PayloadHolder>(e).and_then(|h| h.payload)
    }

    fn take_payload(&self, world: &mut World, e: Entity) -> Option<PayloadRef> {
        super::take_payload(world, e)
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        loader_update(world, e, self);
    }

    fn accept_item(
        &self,
        world: &World,
        e: Entity,
        src: Entity,
        _item: crate::content::ItemId,
    ) -> bool {
        let cap = transfer::item_capacity(world, e);
        world.get::<ItemModule>(e).is_some_and(|m| m.total() < cap)
            && world
                .get::<super::payload_unloader::PayloadUnloaderBuild>(src)
                .is_none()
    }

    fn accept_liquid(
        &self,
        world: &World,
        e: Entity,
        src: Entity,
        liquid: crate::content::LiquidId,
    ) -> bool {
        // `PayloadLoaderBuild.acceptLiquid`.
        let module_ok = world
            .get::<LiquidModule>(e)
            .is_some_and(|m| m.current_amount < 0.2 || current_liquid(m) == Some(liquid));
        module_ok
            && world
                .get::<super::payload_unloader::PayloadUnloaderBuild>(src)
                .is_none()
    }

    fn can_unload(&self, world: &World, e: Entity) -> bool {
        world.get::<ItemModule>(e).is_some_and(|m| m.total() > 0)
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        1
    }

    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        super::payload_block::write_base(world, e, w);
        let exporting = world
            .get::<PayloadLoaderBuild>(e)
            .is_some_and(|s| s.exporting);
        w.bool(exporting);
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        revision: u8,
    ) {
        super::payload_block::read_base(world, e, r);
        if revision >= 1 {
            let exporting = r.bool().unwrap_or(false);
            if let Some(mut state) = world.get_mut::<PayloadLoaderBuild>(e) {
                state.exporting = exporting;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::LiquidId;
    use crate::world::BuildHarness;

    fn harness() -> BuildHarness {
        let mut harness = BuildHarness::new(24, 24, 7);
        if let Some(mut rules) = harness
            .world
            .get_resource_mut::<crate::world::limits::BuildRules>()
        {
            rules.cheat = true;
        }
        harness
    }

    #[test]
    fn loader_moves_liquid_into_payload() {
        let mut harness = harness();
        let loader = harness
            .content()
            .block_id("payload-loader")
            .expect("payload-loader");
        let container = harness
            .content()
            .block_id("liquid-container")
            .expect("liquid-container");
        assert!(harness.place(6, 6, loader, 0, true));
        let loader_e = harness.build_at(6, 6).expect("loader");
        let payload_entity =
            super::super::create_build_payload(&mut harness.world, container, 0).expect("payload");
        if let Some(mut module) = harness.world.get_mut::<LiquidModule>(loader_e) {
            module.add(LiquidId::WATER, 50.0, 100.0);
        }
        let payload = PayloadRef {
            entity: Some(payload_entity),
            content: container.raw(),
            is_block: true,
        };
        super::super::handle_payload(&mut harness.world, loader_e, loader_e, payload);
        for _ in 0..30 {
            harness.tick();
        }
        assert_eq!(
            harness
                .world
                .get::<LiquidModule>(payload_entity)
                .map(|m| m.current_amount),
            Some(50.0)
        );
        assert_eq!(
            harness
                .world
                .get::<LiquidModule>(loader_e)
                .map(|m| m.current_amount),
            Some(0.0)
        );
    }

    #[test]
    fn loader_charges_battery_payload() {
        let mut harness = harness();
        let loader = harness
            .content()
            .block_id("payload-loader")
            .expect("payload-loader");
        let battery = harness.content().block_id("battery").expect("battery");
        assert!(harness.place(6, 6, loader, 0, true));
        let loader_e = harness.build_at(6, 6).expect("loader");
        let payload_entity =
            super::super::create_build_payload(&mut harness.world, battery, 0).expect("payload");
        if let Some(mut module) = harness.world.get_mut::<PowerModule>(loader_e) {
            module.status = 1.0;
        }
        let payload = PayloadRef {
            entity: Some(payload_entity),
            content: battery.raw(),
            is_block: true,
        };
        super::super::handle_payload(&mut harness.world, loader_e, loader_e, payload);
        for _ in 0..30 {
            harness.tick();
        }
        let status = harness
            .world
            .get::<PowerModule>(payload_entity)
            .map(|m| m.status)
            .unwrap_or(0.0);
        assert!(status > 0.2, "payload battery should be charged: {status}");
    }
}
