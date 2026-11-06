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

use crate::config::TILESIZE;
use crate::content::registries::fx_meta::EffectRef;
use crate::content::{EffectId, ItemId, Rgba};
use crate::determinism::RngStream;
use crate::entities::comp::Building;
use crate::sim::unit_runtime::UnitRuntime;
use crate::world::behavior::BuildingBehavior;
use crate::world::block::BlockTable;
use crate::world::modules::ItemModule;
use crate::world::pos::TilePos;

use super::super::distribution::transfer;
use super::core_block::CoreBuild;

/// `StorageBlock.StorageBuild` state.
#[derive(Debug, Clone, Default, Component)]
pub struct StorageBuild {
    /// Owning core (`linkedCore`), assigned by `CoreBlock` proximity (M5).
    pub linked_core: Option<Entity>,
}

/// `Mathf.chance(0.3)` in `StorageBlock.incinerateEffect`
/// (`StorageBlock.java:40-49`).
pub const INCINERATE_CHANCE: f64 = 0.3;

/// Emit predicate of `StorageBlock.incinerateEffect`: `Mathf.chance(0.3)` and
/// the `self.wasVisible` gate. `roll` is the uniform `[0, 1)` draw already
/// taken from the view-only `RngStream::Fx` stream (never checksummed).
pub fn should_emit_incinerate(was_visible: bool, roll: f32) -> bool {
    was_visible && f64::from(roll) < INCINERATE_CHANCE
}

/// `Fx.coreBurn.at(x, y)` through the live unit runtime's FX sink (view-only).
pub(crate) fn emit_core_burn_at(world: &World, x: f32, y: f32) {
    let Some(fx) = world
        .get_non_send::<UnitRuntime>()
        .map(|runtime| runtime.fx.clone())
    else {
        return;
    };
    fx.effect(
        &EffectRef::Named(EffectId::CORE_BURN),
        x,
        y,
        0.0,
        Rgba::WHITE,
    );
}

/// Block footprint size in tiles (`Block.size`); 1 when the table has no row.
fn block_size(world: &World, e: Entity) -> i32 {
    let Some(block) = world.get::<Building>(e).map(|building| building.block) else {
        return 1;
    };
    world
        .get_resource::<BlockTable>()
        .and_then(|table| table.get(block))
        .map(|instance| instance.def.size)
        .unwrap_or(1)
        .max(1)
}

/// `Edges.getFacingEdge(block, tilex, tiley, other)` (`Edges.java:62-67`): a
/// non-multiblock returns its own tile, a multiblock clamps toward `other`.
fn facing_edge(size: i32, tile: TilePos, other: TilePos) -> (i32, i32) {
    let (x, y) = (i32::from(tile.x()), i32::from(tile.y()));
    if size <= 1 {
        return (x, y);
    }
    let lo = -((size - 1) / 2);
    let hi = size / 2;
    (
        x + (i32::from(other.x()) - x).clamp(lo, hi),
        y + (i32::from(other.y()) - y).clamp(lo, hi),
    )
}

/// Midpoint of two edge tile centres in world pixels. Arc `Tile.worldx()` is
/// `x * tilesize + tilesize / 2`, so the midpoint of `a` and `b` is
/// `(a + b + 1) / 2 * tilesize`.
fn edge_midpoint_px(a: (i32, i32), b: (i32, i32)) -> (f32, f32) {
    let ts = TILESIZE as f32;
    (
        (a.0 + b.0 + 1) as f32 * 0.5 * ts,
        (a.1 + b.1 + 1) as f32 * 0.5 * ts,
    )
}

/// `StorageBlock.incinerateEffect(self, source)` (`StorageBlock.java:40-49`):
/// on a 30% roll, when `self` was visible, burn at the midpoint of the two
/// facing edges (`Fx.coreBurn.at((edge.worldx() + edge2.worldx()) / 2, ...)`).
///
/// `&mut World` because the roll comes from the unit runtime's view-only
/// `RngStream::Fx` stream; the stream is excluded from the simulation
/// checksum, matching Java's client-side `Mathf.chance` draw.
pub fn incinerate_effect(world: &mut World, self_e: Entity, source: Entity) {
    let Some(roll) = world
        .get_non_send_mut::<UnitRuntime>()
        .map(|mut runtime| runtime.rng.next_float(RngStream::Fx))
    else {
        return;
    };
    let Some(self_tile) = world.get::<Building>(self_e).map(|building| building.tile) else {
        return;
    };
    let Some(source_tile) = world.get::<Building>(source).map(|building| building.tile) else {
        return;
    };
    let visible = world
        .get::<Building>(self_e)
        .is_some_and(|building| building.was_visible);
    if !should_emit_incinerate(visible, roll) {
        return;
    }
    // `Edges.getFacingEdge(source, self)` and `(self, source)`.
    let source_edge = facing_edge(block_size(world, source), source_tile, self_tile);
    let self_edge = facing_edge(block_size(world, self_e), self_tile, source_tile);
    let (x, y) = edge_midpoint_px(source_edge, self_edge);
    emit_core_burn_at(world, x, y);
}

/// `StorageBlock` behavior (`container`/`vault` and reinforced variants).
#[derive(Debug, Default, Clone, Copy)]
pub struct StorageBehavior;

fn linked_core(world: &World, e: Entity) -> Option<Entity> {
    world.get::<StorageBuild>(e).and_then(|b| b.linked_core)
}

/// `linkedCore.items.get(item) >= ((CoreBuild)linkedCore).storageCapacity`
/// (`StorageBlock.java:63`): the linked-core fullness gate of the burn effect.
fn linked_core_is_full(world: &World, core: Entity, item: ItemId) -> bool {
    let cap = world
        .get::<CoreBuild>(core)
        .map(|core_build| core_build.storage_capacity)
        .unwrap_or(0);
    world
        .get::<ItemModule>(core)
        .map(|module| module.get(item))
        .unwrap_or(0)
        >= cap
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
                // `StorageBlock.StorageBuild.handleItem` (`StorageBlock.java:61-71`):
                // a full linked core burns at the storage's facing edge; the
                // `noEffect` latch then suppresses the core's own burn for this
                // arrival (the storage-effect path doubles as the effect owner).
                if linked_core_is_full(world, core, item) {
                    incinerate_effect(world, e, src);
                }
                if let Some(mut core_build) = world.get_mut::<CoreBuild>(core) {
                    core_build.no_effect = true;
                }
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

    /// CG-ECON-06: Java's `StorageBlock.incinerateEffect`
    /// (`StorageBlock.java:40-49`) emits only on `Mathf.chance(0.3)` **and**
    /// `self.wasVisible`. Pin both bounds of the predicate.
    #[test]
    fn incinerate_effect_predicate_is_chance_and_visibility() {
        assert!(should_emit_incinerate(true, 0.0));
        assert!(should_emit_incinerate(true, 0.299_99));
        assert!(!should_emit_incinerate(true, 0.3));
        assert!(!should_emit_incinerate(true, 0.999_99));
        assert!(!should_emit_incinerate(false, 0.0));
        assert!(!should_emit_incinerate(false, 0.299_99));
    }

    /// The burn sits at the midpoint of the two facing edges
    /// (`(edge.worldx() + edge2.worldx()) / 2`, `StorageBlock.java:44-48`).
    #[test]
    fn incinerate_effect_uses_the_facing_edge_midpoint() {
        let ts = TILESIZE as f32;
        // 1x1 conveyor at (10,10) feeds a 3x3 core centred at (12,10).
        let source_edge = facing_edge(1, TilePos::new(10, 10), TilePos::new(12, 10));
        let core_edge = facing_edge(3, TilePos::new(12, 10), TilePos::new(10, 10));
        assert_eq!(source_edge, (10, 10), "non-multiblocks return their tile");
        assert_eq!(core_edge, (11, 10), "the core edge faces the conveyor");
        let (x, y) = edge_midpoint_px(source_edge, core_edge);
        assert!((x - 11.0 * ts).abs() < 1e-4, "x {x}");
        assert!((y - 10.5 * ts).abs() < 1e-4, "y {y}");
        // A 3x3 core fed from the north clamps its edge toward the source.
        assert_eq!(
            facing_edge(3, TilePos::new(12, 10), TilePos::new(12, 8)),
            (12, 9)
        );
    }

    /// The linked-storage fullness gate (`StorageBlock.java:63`): only a full
    /// core burns; an empty/partial pool does not.
    #[test]
    fn linked_storage_burns_only_when_the_core_item_is_full() {
        let mut harness = BuildHarness::new(24, 24, 7);
        let core = harness
            .content()
            .block_id("core-shard")
            .expect("core-shard");
        let container = harness.content().block_id("container").expect("container");
        let copper = harness.content().item_id("copper").expect("copper");
        assert!(harness.place(6, 6, core, 0, true));
        assert!(harness.place(8, 6, container, 0, true));
        let core_e = harness.build_at(6, 6).expect("core");
        assert!(!linked_core_is_full(&harness.world, core_e, copper));
        let cap = harness
            .world
            .get::<CoreBuild>(core_e)
            .expect("core build")
            .storage_capacity;
        harness
            .world
            .get_mut::<ItemModule>(core_e)
            .expect("core items")
            .add(copper, cap, cap);
        assert!(linked_core_is_full(&harness.world, core_e, copper));
    }
}
