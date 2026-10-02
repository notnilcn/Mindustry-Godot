// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PayloadLoader` build behavior
//! (`world/blocks/payloads/PayloadLoader.java`) — plan 08 M7.
//!
//! Loads items (and, via plan 09's power module, power) into a carried build
//! payload. Liquid/power transfer math is stubbed until plan 09's module ABI
//! lands (R12); item transfer is exact.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::entities::comp::Building;
use crate::world::behavior::{BuildingBehavior, PayloadRef};
use crate::world::block::BlockTable;
use crate::world::blocks::distribution::transfer;
use crate::world::modules::ItemModule;
use crate::world::update::run_timer;

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
        base_power_use: 0.0,
    };
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
    let Some(payload_block) = payload
        .entity
        .and_then(|p| world.get::<Building>(p))
        .map(|b| b.block)
    else {
        return false;
    };
    let capacity = world
        .get_resource::<BlockTable>()
        .and_then(|table| table.get(payload_block).map(|inst| inst.def.item_capacity))
        .unwrap_or(0);
    let loader_total = world.get::<ItemModule>(e).map(|m| m.total()).unwrap_or(0);
    if capacity > 0 && loader_total > 0 {
        let loader = world.get::<ItemModule>(e);
        let any_full = world
            .get::<ItemModule>(payload.entity.unwrap_or(e))
            .is_some_and(|payload_items| {
                payload_items
                    .stacks()
                    .any(|(item, amount)| amount >= capacity && loader.is_some_and(|l| l.has(item)))
            });
        if any_full {
            return true;
        }
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
    let has_items = world
        .get_resource::<BlockTable>()
        .and_then(|table| table.get(payload_block).map(|inst| inst.def.has_items))
        .unwrap_or(false);
    if !has_items {
        return;
    }
    let loader_any = world.get::<ItemModule>(e).is_some_and(|m| m.any());
    if !loader_any {
        return;
    }

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

impl BuildingBehavior for PayloadLoaderBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<PayloadLoaderBuild>(e).is_none() {
            world.entity_mut(e).insert(PayloadLoaderBuild::default());
        }
        if world.get::<PayloadHolder>(e).is_none() {
            world.entity_mut(e).insert(PayloadHolder::default());
        }
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
