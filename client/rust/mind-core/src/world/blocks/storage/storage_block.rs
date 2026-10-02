// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `StorageBlock` build behavior (`world/blocks/storage/StorageBlock.java`).
//!
//! M0 ports the deposit half against the block's `ItemModule`. M4 adds the
//! `linkedCore` forwarding (`acceptItem`/`handleItem`/`canUnload`/
//! `getMaximumAccepted`/`itemTaken`/`removeStack`), the incineration effect hook
//! and the `overwrote` merge. The `linked_core` field is assigned by `CoreBlock`
//! proximity unification (M5).

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ItemId;
use crate::world::behavior::BuildingBehavior;
use crate::world::modules::ItemModule;

use super::super::distribution::transfer;

/// `StorageBlock.StorageBuild` state.
#[derive(Debug, Clone, Default, Component)]
pub struct StorageBuild {
    /// Owning core (`linkedCore`), assigned by `CoreBlock` proximity (M5).
    pub linked_core: Option<Entity>,
}

/// `StorageBlock.incinerateEffect` (view-only hook; plan 16 draws it).
pub fn incinerate_effect(_world: &World, _self_e: Entity, _source: Entity) {}

/// `StorageBlock` behavior (`container`/`vault` and reinforced variants).
#[derive(Debug, Default, Clone, Copy)]
pub struct StorageBehavior;

fn linked_core(world: &World, e: Entity) -> Option<Entity> {
    world.get::<StorageBuild>(e).and_then(|b| b.linked_core)
}

impl BuildingBehavior for StorageBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<StorageBuild>(e).is_none() {
            world.entity_mut(e).insert(StorageBuild::default());
        }
    }

    fn accept_item(&self, world: &World, e: Entity, src: Entity, item: ItemId) -> bool {
        match linked_core(world, e) {
            Some(core) => transfer::dispatch_accept_item(world, core, src, item),
            None => transfer::default_accept_item(world, e, src, item),
        }
    }

    fn handle_item(&self, world: &mut World, e: Entity, src: Entity, item: ItemId) {
        match linked_core(world, e) {
            Some(core) => {
                incinerate_effect(world, e, src);
                transfer::dispatch_handle_item(world, core, src, item);
            }
            None => transfer::default_handle_item(world, e, src, item),
        }
    }

    fn get_maximum_accepted(&self, world: &World, e: Entity, item: ItemId) -> i32 {
        match linked_core(world, e) {
            Some(core) => transfer::dispatch_get_maximum_accepted(world, core, item),
            None => transfer::item_capacity(world, e),
        }
    }

    fn can_unload(&self, world: &World, e: Entity) -> bool {
        match linked_core(world, e) {
            Some(core) => transfer::dispatch_can_unload(world, core),
            None => true,
        }
    }

    fn item_taken(&self, world: &mut World, e: Entity, item: ItemId) {
        if let Some(core) = linked_core(world, e) {
            transfer::dispatch_item_taken(world, core, item);
        }
    }

    fn remove_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) -> i32 {
        transfer::default_remove_stack(world, e, item, amount)
    }

    fn accept_stack(
        &self,
        world: &World,
        e: Entity,
        item: ItemId,
        amount: i32,
        source: Option<Entity>,
    ) -> i32 {
        match linked_core(world, e) {
            Some(core) => transfer::dispatch_get_maximum_accepted(world, core, item)
                .saturating_sub(transfer::item_count(world, core, item))
                .min(amount),
            None => transfer::default_accept_stack(world, e, item, amount, source),
        }
    }

    fn handle_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) {
        match linked_core(world, e) {
            Some(core) => {
                let cap = transfer::dispatch_get_maximum_accepted(world, core, item);
                let accepted = cap
                    .saturating_sub(transfer::item_count(world, core, item))
                    .min(amount);
                if let Some(mut items) = world.get_mut::<ItemModule>(core) {
                    items.add(item, accepted, cap.max(0));
                }
            }
            None => transfer::default_handle_stack(world, e, item, amount),
        }
    }

    fn overwrote(&self, world: &mut World, e: Entity, previous: &[Entity]) {
        // Only add previous items when not linked to a core.
        if world
            .get::<StorageBuild>(e)
            .is_some_and(|b| b.linked_core.is_some())
        {
            return;
        }
        let capacity = transfer::item_capacity(world, e);
        let mut merged: Vec<(ItemId, i32)> = Vec::new();
        for other in previous {
            if let Some(module) = world.get::<ItemModule>(*other) {
                for (item, amount) in module.stacks() {
                    merged.push((item, amount));
                }
            }
        }
        if let Some(mut items) = world.get_mut::<ItemModule>(e) {
            for (item, amount) in merged {
                items.add(item, amount, capacity.max(0));
            }
        }
    }
}
