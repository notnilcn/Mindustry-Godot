// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `StorageBlock` build behavior (`world/blocks/storage/StorageBlock.java`).
//!
//! M0 ports the deposit half: `acceptItem`/`handleItem`/`acceptStack`/
//! `handleStack`/`removeStack` against the block's `ItemModule`. The
//! `linkedCore` forwarding, incineration override and `sector.info` hooks land
//! in M4/M5.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ItemId;
use crate::world::behavior::BuildingBehavior;

use super::super::distribution::transfer;

/// `StorageBlock` behavior (`container`/`vault` and reinforced variants).
#[derive(Debug, Default, Clone, Copy)]
pub struct StorageBehavior;

impl BuildingBehavior for StorageBehavior {
    fn accept_item(&self, world: &World, e: Entity, src: Entity, item: ItemId) -> bool {
        transfer::default_accept_item(world, e, src, item)
    }

    fn handle_item(&self, world: &mut World, e: Entity, src: Entity, item: ItemId) {
        transfer::default_handle_item(world, e, src, item);
    }

    fn accept_stack(
        &self,
        world: &World,
        e: Entity,
        item: ItemId,
        amount: i32,
        source: Option<Entity>,
    ) -> i32 {
        transfer::default_accept_stack(world, e, item, amount, source)
    }

    fn handle_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) {
        transfer::default_handle_stack(world, e, item, amount);
    }

    fn remove_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) -> i32 {
        transfer::default_remove_stack(world, e, item, amount)
    }
}
