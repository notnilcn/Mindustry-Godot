// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `StorageBlock` build behavior (`world/blocks/storage/StorageBlock.java`).
//!
//! M0 ports the deposit half against the block's `ItemModule`. M4 adds the
//! `linkedCore` forwarding (`acceptItem`/`handleItem`/`canUnload`/
//! `getMaximumAccepted`/`itemTaken`/`removeStack`), the incineration effect hook
//! and the `overwrote` merge. The `linked_core` field is assigned by `CoreBlock`
//! proximity unification (M5).
//!
//! **R2 item-pool deviation (PAR-0484 / CG-ECON-01).** Java aliases the module
//! pointer of a core-owned storage (`CoreBlock.java:764-770`: `t.items = items`),
//! so the storage *is* the core pool: reads, `removeStack`, `canUnload` and the
//! unloader's provider checks all see the core's items. ECS components cannot be
//! aliased, so the observable contract is reconstructed by resolving item
//! accesses through [`StorageBuild::linked_core`] instead (helpers
//! [`item_holder`]/[`has_items`]/[`item_count`], used by [`super::unloader`]);
//! the storage's own `ItemModule` stays empty for a linked build. Unlinked
//! storages keep their own module exactly as before. This is the same
//! no-component-aliasing deviation documented in
//! [`super::core_block`]'s R2 note; the `TeamInventory` resource remains the
//! long-term follow-up.

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

/// Entity whose `ItemModule` backs `e`'s item storage.
///
/// Java aliases a core-owned storage's `items` pointer to the core's module
/// (`CoreBlock.java:764-770`), so every read/take on the storage observes the
/// core pool. Because ECS components cannot alias (see the R2 note above),
/// reads resolve through [`StorageBuild::linked_core`] instead: a linked
/// storage delegates to its core, everything else keeps its own module. A
/// stale link (core destroyed before the storage re-proximities) falls back to
/// the storage itself so reads stay defined.
pub fn item_holder(world: &World, e: Entity) -> Entity {
    match linked_core(world, e) {
        Some(core) if world.get::<ItemModule>(core).is_some() => core,
        _ => e,
    }
}

/// `StorageBuild.items != null` probe against the resolved holder
/// ([`item_holder`]): a linked storage reports the core's module.
pub fn has_items(world: &World, e: Entity) -> bool {
    world.get::<ItemModule>(item_holder(world, e)).is_some()
}

/// `items.get(item)` against the resolved holder ([`item_holder`]): a linked
/// storage reports the core's count (`Unloader.isPossibleItem` /
/// `unloadAccumulate` read `other.items`).
pub fn item_count(world: &World, e: Entity, item: ItemId) -> i32 {
    world
        .get::<ItemModule>(item_holder(world, e))
        .map(|module| module.get(item))
        .unwrap_or(0)
}

impl BuildingBehavior for StorageBehavior {
    fn logic_sense(
        &self,
        world: &World,
        e: Entity,
        a: crate::logic::LAccess,
    ) -> Option<crate::logic::access::Sensed> {
        use crate::logic::LAccess;
        use crate::logic::access::Sensed;
        Some(match a {
            // `StorageBlock.sense(itemCapacity)` (`StorageBlock.java:113-116`):
            // a linked storage reports its core's capacity.
            LAccess::ItemCapacity => {
                let core = world
                    .get::<StorageBuild>(e)
                    .and_then(|state| state.linked_core)?;
                let capacity = world
                    .get::<crate::world::blocks::storage::core_block::CoreBuild>(core)?
                    .storage_capacity;
                Sensed::Num(f64::from(capacity))
            }
            _ => return None,
        })
    }
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
        match linked_core(world, e) {
            // Java's linked storage *is* the core module, so `removeStack`
            // (`StorageBlock.java:84-92`: `super.removeStack` then the sector
            // count) takes from the core pool. Route through the core behavior
            // so `handleCoreItem` bookkeeping stays on one path.
            Some(core) => transfer::dispatch_remove_stack(world, core, item, amount),
            None => transfer::default_remove_stack(world, e, item, amount),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;

    /// PAR-0484 / CG-ECON-01: Java aliases a core-owned storage's `items`
    /// pointer to the core module (`CoreBlock.java:764-770`), so reads and
    /// takes on the storage observe the shared pool. Without ECS aliasing the
    /// same contract is reconstructed through `linked_core`: the storage's own
    /// module stays empty while [`item_count`]/[`has_items`] and `removeStack`
    /// resolve through the core.
    #[test]
    fn linked_storage_reads_and_takes_through_the_core() {
        let mut harness = BuildHarness::new(24, 24, 7);
        let core = harness
            .content()
            .block_id("core-shard")
            .expect("core-shard");
        let container = harness.content().block_id("container").expect("container");
        let copper = harness.content().item_id("copper").expect("copper");
        // core-shard 3x3 centered at (6,6) -> footprint (5,5)..(7,7); the 2x2
        // container at (8,6) touches it at (7,6)/(7,7).
        assert!(harness.place(6, 6, core, 0, true));
        assert!(harness.place(8, 6, container, 0, true));
        let core_e = harness.build_at(6, 6).expect("core");
        let storage_e = harness.build_at(8, 6).expect("container");
        assert_eq!(
            harness
                .world
                .get::<StorageBuild>(storage_e)
                .and_then(|state| state.linked_core),
            Some(core_e),
            "container must be owned by the touching core"
        );
        // The linked storage's own module is never filled; the pool is the core.
        assert_eq!(
            harness
                .world
                .get::<ItemModule>(storage_e)
                .map(|m| m.get(copper)),
            Some(0)
        );
        harness
            .world
            .get_mut::<ItemModule>(core_e)
            .expect("core items")
            .add(copper, 40, 4300);
        assert!(has_items(&harness.world, storage_e));
        assert_eq!(item_holder(&harness.world, storage_e), core_e);
        assert_eq!(item_count(&harness.world, storage_e, copper), 40);
        // `StorageBlock.removeStack` (`StorageBlock.java:84-92`) takes from the
        // shared module.
        let taken = transfer::dispatch_remove_stack(&mut harness.world, storage_e, copper, 7);
        assert_eq!(taken, 7);
        assert_eq!(item_count(&harness.world, storage_e, copper), 33);
        assert_eq!(
            harness
                .world
                .get::<ItemModule>(core_e)
                .map(|m| m.get(copper)),
            Some(33),
            "the core module is the one pool"
        );
        assert_eq!(
            harness.world.get::<ItemModule>(storage_e).map(|m| m.total),
            Some(0),
            "no shadow copy in the storage module"
        );
    }

    /// The unlinked storage path is unchanged: its own module is the pool.
    #[test]
    fn unlinked_storage_keeps_its_own_module() {
        let mut harness = BuildHarness::new(16, 16, 7);
        let container = harness.content().block_id("container").expect("container");
        let copper = harness.content().item_id("copper").expect("copper");
        assert!(harness.place(4, 4, container, 0, true));
        let storage_e = harness.build_at(4, 4).expect("container");
        assert_eq!(
            harness
                .world
                .get::<StorageBuild>(storage_e)
                .and_then(|state| state.linked_core),
            None
        );
        harness
            .world
            .get_mut::<ItemModule>(storage_e)
            .expect("storage items")
            .add(copper, 40, 300);
        assert_eq!(item_holder(&harness.world, storage_e), storage_e);
        assert_eq!(item_count(&harness.world, storage_e, copper), 40);
        let taken = transfer::dispatch_remove_stack(&mut harness.world, storage_e, copper, 7);
        assert_eq!(taken, 7);
        assert_eq!(
            harness
                .world
                .get::<ItemModule>(storage_e)
                .map(|m| m.get(copper)),
            Some(33)
        );
    }
}
