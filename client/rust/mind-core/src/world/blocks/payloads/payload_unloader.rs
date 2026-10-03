// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PayloadUnloader` build behavior
//! (`world/blocks/payloads/PayloadUnloader.java`) — plan 08 M7.
//!
//! Drains a carried build payload's items into the block inventory and dumps
//! them to neighbours. Power unload is stubbed until plan 09's module ABI (R12).

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::entities::comp::Building;
use crate::world::behavior::{BuildingBehavior, PayloadRef};
use crate::world::block::BlockTable;
use crate::world::blocks::distribution::transfer;
use crate::world::modules::ItemModule;
use crate::world::update::run_timer;

use super::PayloadHolder;
use super::payload_block::{move_in_payload, move_out_payload, payload_block_update};
use super::payload_loader::PayloadLoaderBehavior;

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

/// `PayloadUnloaderBuild.shouldExport()`.
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
    let liquids_empty = !def.has_liquids;
    items_empty && liquids_empty
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
            if let Some(payload_entity) = payload_entity
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
                        if let Some(mut payload_items) = world.get_mut::<ItemModule>(payload_entity)
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
        }

        for _ in 0..self.offload_speed {
            let _ = transfer::dump_accumulate(world, e, None);
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
