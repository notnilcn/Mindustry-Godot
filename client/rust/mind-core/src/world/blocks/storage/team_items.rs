// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Team-shared core inventory (`CoreBlock.java:764-780`, `Team.core()`).
//!
//! Java's `CoreBlock.CoreBuild.onProximityUpdate` aliases one `ItemModule`
//! pointer across every core of a team and every storage those cores own:
//!
//! ```java
//! for(Building other : state.teams.cores(team)) if(other.tile != tile) this.items = other.items;
//! state.teams.registerCore(this);
//! proximity.each(this::owns, t -> { t.items = items; ((StorageBuild)t).linkedCore = this; });
//! ```
//!
//! ECS components cannot alias, so the port carries the same pointer shape
//! explicitly: [`super::core_block::CoreBuild::items_holder`] is the
//! **canonical core** of a team, and every item access on a core resolves
//! through [`core_holder`] (a linked storage resolves through its owning core
//! first, see [`super::storage_block::item_holder`]). The canonical core is the
//! team's lowest [`EntitySeq`] core, which is exactly the order Java's
//! `Team.cores().first()` and the existing port readers (`sim/builder.rs`
//! `team_core`, `sim_host.rs` `core_items_json`/`write_record_items_to_core`,
//! `debug_add_core_items`) already use.
//!
//! Observable contract (what Java's single module produces):
//!
//! - every core of a team reads and writes the same pool (a belt into a
//!   second core lands in the team inventory; a `removeStack` on either core
//!   takes from it);
//! - aggregations ([`team_item_count`]/[`team_item_stacks`], used by
//!   `SectorInfo.prepare` and `ItemObjective`) count each aliased core once;
//! - `CoreBlock.owns`' linked storages still resolve through the core pool
//!   (`PAR-0484`), now through the canonical core.
//!
//! Single-core teams keep the pre-alias behaviour byte for byte: the holder is
//! the core itself ([`core_holder`] returns `core` when no link exists), so
//! every helper hits that core's own `ItemModule`.
//!
//! **ECS deviation (documented R2).** A non-canonical core must still carry an
//! `ItemModule` component (it is the core's `hasItems`/save shape), but that
//! module is *inert*: the pool lives only on the canonical core. Code that
//! reads a core's `ItemModule` directly instead of the helpers (logic
//! `sensor`/`setitem` in `mc/logic/access.rs`, the debug building-items panel
//! for a secondary core) therefore sees the empty mirror rather than the pool.
//! Every behavior path (`transfer`, `StorageBlock`, `Unloader`, `CoreBlock`),
//! the HUD (`core_items_json` reads the first team core) and the sector
//! record resolve through the holder and are exact. Non-canonical modules are
//! never summed, so a save that stores a copy per core (upstream does, because
//! of the alias) cannot double count.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ItemId;
use crate::ecs::EntitySeq;
use crate::entities::comp::{Building, TeamComp};
use crate::world::block::BlockTable;
use crate::world::modules::ItemModule;

use super::core_block::CoreBuild;
use super::storage_block::StorageBuild;

/// The core whose `ItemModule` backs `core`'s items (Java's aliased `items`
/// pointer).
///
/// A stale link (the canonical core was despawned before the survivors were
/// relinked) or a cross-team link (impossible through the link helpers, kept
/// defensive for hand-built worlds) falls back to `core` itself so reads stay
/// defined.
pub fn core_holder(world: &World, core: Entity) -> Entity {
    let Some(link) = world
        .get::<CoreBuild>(core)
        .and_then(|build| build.items_holder)
    else {
        return core;
    };
    if link == core
        || world.get::<CoreBuild>(link).is_none()
        || world.get::<ItemModule>(link).is_none()
    {
        return core;
    }
    let same_team = world
        .get::<TeamComp>(core)
        .zip(world.get::<TeamComp>(link))
        .is_some_and(|(a, b)| a.team == b.team);
    if same_team { link } else { core }
}

/// Live cores of `team` in `Team.cores()` order: lowest [`EntitySeq`] first
/// (the `Team.core()` canonical order; a missing seq sorts last and ties break
/// on the entity index, keeping the choice deterministic).
pub fn team_cores(world: &World, team: u8) -> Vec<Entity> {
    let mut cores: Vec<(u64, Entity)> = world
        .iter_entities()
        .filter_map(|entity_ref| {
            let entity = entity_ref.id();
            if entity_ref.get::<CoreBuild>().is_none()
                || entity_ref.get::<TeamComp>().map(|comp| comp.team) != Some(team)
            {
                return None;
            }
            let seq = entity_ref
                .get::<EntitySeq>()
                .map(|seq| seq.0)
                .unwrap_or(u64::MAX);
            Some((seq, entity))
        })
        .collect();
    cores.sort_unstable_by_key(|(seq, entity)| (*seq, entity.index()));
    cores.into_iter().map(|(_, entity)| entity).collect()
}

/// Canonical core of `team` (`Team.core()`: `cores.first()`).
pub fn canonical_core(world: &World, team: u8) -> Option<Entity> {
    team_cores(world, team).into_iter().next()
}

/// Rewrites every core's `items_holder` link for `team` (Java
/// `this.items = other.items` for all team cores) and returns the canonical
/// holder.
///
/// Called from the core's proximity pass (`CoreBlock.onProximityUpdate`) and
/// the destroy path; both cover every team-wide alias change.
pub fn link_team_cores(world: &mut World, team: u8) -> Option<Entity> {
    let cores = team_cores(world, team);
    let canonical = *cores.first()?;
    for core in &cores {
        if let Some(mut build) = world.get_mut::<CoreBuild>(*core) {
            build.items_holder = (*core != canonical).then_some(canonical);
        }
    }
    Some(canonical)
}

/// `items` module backing `core` (through [`core_holder`]).
pub fn core_items(world: &World, core: Entity) -> Option<&ItemModule> {
    world.get::<ItemModule>(core_holder(world, core))
}

/// `items.get(item)` of the team pool.
pub fn core_item_count(world: &World, core: Entity, item: ItemId) -> i32 {
    core_items(world, core)
        .map(|module| module.get(item))
        .unwrap_or(0)
}

/// `items.total` of the team pool.
pub fn core_item_total(world: &World, core: Entity) -> i32 {
    core_items(world, core)
        .map(|module| module.total)
        .unwrap_or(0)
}

/// `super.handleItem`/`items.add` on the team pool; returns the amount stored.
pub fn add_to_core(
    world: &mut World,
    core: Entity,
    item: ItemId,
    amount: i32,
    capacity: i32,
) -> i32 {
    let holder = core_holder(world, core);
    world
        .get_mut::<ItemModule>(holder)
        .map(|mut module| module.add(item, amount, capacity))
        .unwrap_or(0)
}

/// `super.removeStack` on the team pool; returns the amount taken.
pub fn remove_from_core(world: &mut World, core: Entity, item: ItemId, amount: i32) -> i32 {
    let holder = core_holder(world, core);
    world
        .get_mut::<ItemModule>(holder)
        .map(|mut module| module.remove(item, amount))
        .unwrap_or(0)
}

/// Team item total (`Team.items().get(item)`): the canonical core of each
/// alias group counts its pool, an unaliased core counts itself. Counting the
/// holders (and not the inert mirrors) keeps every team core's copy from
/// double counting.
pub fn team_item_count(world: &World, team: u8, item: ItemId) -> i32 {
    let mut total = 0i32;
    for core in team_cores(world, team) {
        if core_holder(world, core) != core {
            continue;
        }
        if let Some(module) = world.get::<ItemModule>(core) {
            total = total.saturating_add(module.get(item));
        }
    }
    total
}

/// All non-zero team stacks in item-id order (`Team.items()` snapshot for
/// `SectorInfo.prepare`).
pub fn team_item_stacks(world: &World, team: u8) -> Vec<(ItemId, i32)> {
    let mut totals: Vec<i32> = Vec::new();
    for core in team_cores(world, team) {
        if core_holder(world, core) != core {
            continue;
        }
        let Some(module) = world.get::<ItemModule>(core) else {
            continue;
        };
        if totals.len() < module.items.len() {
            totals.resize(module.items.len(), 0);
        }
        for (index, amount) in module.items.iter().enumerate() {
            totals[index] = totals[index].saturating_add(*amount);
        }
    }
    totals
        .into_iter()
        .enumerate()
        .filter(|(_, amount)| *amount > 0)
        .map(|(index, amount)| (ItemId::new(index as u16), amount))
        .collect()
}

/// `CoreBuild.onRemoved` (`CoreBlock.java:861-880`) for the shared pool.
///
/// Java keeps the pool alive in the survivors because every core references
/// the same module object; the port migrates the canonical module to the next
/// core before the dying entity is despawned. The dying core's owned storages
/// are re-linked to the surviving canonical core (Java instead unlinks them
/// and copies a proportional share — a share its own re-link discards — which
/// would duplicate pool items). When the last core of a team dies there is no
/// pool to keep, so the Java proportional share is written into the owned
/// storages ([`release_owned_storages`]).
///
/// The dying core's `CoreBuild` is removed before the survivors recompute
/// `storageCapacity`, mirroring Java's `unregisterCore` before
/// `other.onProximityUpdate()`; otherwise the dead core's capacity would stay
/// counted.
pub fn on_core_removed(world: &mut World, core: Entity) {
    let Some(team) = world.get::<TeamComp>(core).map(|comp| comp.team) else {
        return;
    };
    let was_canonical = core_holder(world, core) == core;
    let survivors: Vec<Entity> = team_cores(world, team)
        .into_iter()
        .filter(|candidate| *candidate != core)
        .collect();
    let new_canonical = survivors.first().copied();

    release_owned_storages(world, core, new_canonical);

    if was_canonical
        && let Some(target) = new_canonical
        && let Some(source) = world.get::<ItemModule>(core).cloned()
        && let Some(mut destination) = world.get_mut::<ItemModule>(target)
    {
        *destination = source;
    }

    // Exclude the dying core from the survivors' scans (`Team.unregisterCore`
    // runs before the surviving cores' `onProximityUpdate` in Java).
    world.entity_mut(core).remove::<CoreBuild>();

    link_team_cores(world, team);
    for survivor in survivors {
        dispatch_proximity_update(world, survivor);
    }
}

/// `CoreBlock.CoreBuild.onRemoved`'s storage half.
fn release_owned_storages(world: &mut World, core: Entity, new_canonical: Option<Entity>) {
    let storages: Vec<Entity> = world
        .iter_entities()
        .filter_map(|entity_ref| {
            let entity = entity_ref.id();
            entity_ref
                .get::<StorageBuild>()
                .is_some_and(|state| state.linked_core == Some(core))
                .then_some(entity)
        })
        .collect();
    if storages.is_empty() {
        return;
    }
    if let Some(target) = new_canonical {
        // Keep the storages in the team pool instead of duplicating a share.
        for storage in &storages {
            if let Some(mut state) = world.get_mut::<StorageBuild>(*storage) {
                state.linked_core = Some(target);
            }
        }
        return;
    }

    // Last core of the team: `CoreBlock.onRemoved` splits the pool across the
    // owned storages by `itemCapacity` (`(int)Math.min(cap, amount * cap /
    // totalCapacity)`), so the items survive the core.
    let source = world.get::<ItemModule>(core).cloned();
    let total_capacity: i32 = storages
        .iter()
        .map(|storage| item_capacity(world, *storage))
        .sum();
    for storage in &storages {
        let capacity = item_capacity(world, *storage).max(0);
        if let (Some(source), true) = (&source, total_capacity > 0)
            && let Some(mut module) = world.get_mut::<ItemModule>(*storage)
        {
            module.clear();
            for (index, amount) in source.items.iter().enumerate() {
                if *amount <= 0 {
                    continue;
                }
                let share = ((*amount as f32 * capacity as f32 / total_capacity as f32) as i32)
                    .min(capacity);
                if share > 0 {
                    module.set(ItemId::new(index as u16), share);
                }
            }
        }
        if let Some(mut state) = world.get_mut::<StorageBuild>(*storage) {
            state.linked_core = None;
        }
    }
}

/// `Block.itemCapacity` of a building (0 when the table has no row).
fn item_capacity(world: &World, entity: Entity) -> i32 {
    let Some(block) = world.get::<Building>(entity).map(|building| building.block) else {
        return 0;
    };
    world
        .get_resource::<BlockTable>()
        .and_then(|table| table.get(block))
        .map(|instance| instance.def.item_capacity)
        .unwrap_or(0)
}

/// Runs a survivor's `onProximityUpdate` (Java `other.onProximityUpdate()`)
/// after the pool migrated, so its `storageCapacity` drops the dead core.
fn dispatch_proximity_update(world: &mut World, entity: Entity) {
    let Some(behavior) = world.get_resource::<BlockTable>().and_then(|table| {
        world
            .get::<Building>(entity)
            .and_then(|building| table.instance(building.block))
            .map(|instance| instance.behavior.clone())
    }) else {
        return;
    };
    behavior.on_proximity_update(world, entity);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;
    use crate::world::blocks::distribution::transfer;

    /// Places a core for `team` at `(x, y)` and returns the building entity.
    fn place_core(harness: &mut BuildHarness, x: i32, y: i32, team: u8) -> Entity {
        harness.rules.default_team = team;
        let core = harness
            .content()
            .block_id("core-shard")
            .expect("core-shard");
        assert!(harness.place(x, y, core, 0, true), "place at ({x},{y})");
        harness.build_at(x, y).expect("core entity")
    }

    fn copper(harness: &BuildHarness) -> ItemId {
        harness.content().item_id("copper").expect("copper")
    }

    fn team_of(harness: &BuildHarness, entity: Entity) -> u8 {
        harness
            .world
            .get::<TeamComp>(entity)
            .map(|comp| comp.team)
            .expect("team")
    }

    fn module_count(harness: &BuildHarness, entity: Entity, item: ItemId) -> i32 {
        harness
            .world
            .get::<ItemModule>(entity)
            .map(|module| module.get(item))
            .unwrap_or(0)
    }

    /// Two cores of one team are one pool: the pool lives on the canonical
    /// (lowest-seq) core, the second core resolves through it, and the pool is
    /// not duplicated by the aggregate.
    #[test]
    fn two_cores_of_a_team_share_one_pool() {
        let mut harness = BuildHarness::new(40, 40, 7);
        let first = place_core(&mut harness, 6, 6, 1);
        let second = place_core(&mut harness, 26, 26, 1);
        let item = copper(&harness);
        let team = team_of(&harness, first);
        assert_eq!(team_of(&harness, second), team);

        // The first core owns the pool (lowest `EntitySeq`).
        assert_eq!(core_holder(&harness.world, first), first);
        assert_eq!(core_holder(&harness.world, second), first);
        assert_eq!(
            harness
                .world
                .get::<CoreBuild>(first)
                .and_then(|build| build.items_holder),
            None,
            "the canonical core holds its own module"
        );

        // A deposit into the canonical core is visible on both cores.
        add_to_core(&mut harness.world, first, item, 40, 4000);
        assert_eq!(core_item_count(&harness.world, second, item), 40);
        assert_eq!(team_item_count(&harness.world, team, item), 40);
        assert_eq!(
            module_count(&harness, second, item),
            0,
            "the mirror module stays inert"
        );

        // A belt/dump into the *second* core lands in the shared pool.
        let stored = add_to_core(&mut harness.world, second, item, 5, 4000);
        assert_eq!(stored, 5);
        assert_eq!(module_count(&harness, first, item), 45);
        assert_eq!(module_count(&harness, second, item), 0);

        // A take on the second core drains the pool.
        assert_eq!(remove_from_core(&mut harness.world, second, item, 20), 20);
        assert_eq!(module_count(&harness, first, item), 25);
        assert_eq!(team_item_count(&harness.world, team, item), 25);
    }

    /// Placing a second core after the pool was filled keeps every item (Java
    /// adopts the existing module; the canonical link does the same).
    #[test]
    fn placing_a_second_core_keeps_the_existing_pool() {
        let mut harness = BuildHarness::new(40, 40, 7);
        let first = place_core(&mut harness, 6, 6, 1);
        let item = copper(&harness);
        let team = team_of(&harness, first);
        add_to_core(&mut harness.world, first, item, 120, 4000);
        assert_eq!(team_item_count(&harness.world, team, item), 120);

        let second = place_core(&mut harness, 26, 26, 1);
        assert_eq!(core_holder(&harness.world, second), first);
        assert_eq!(
            team_item_count(&harness.world, team, item),
            120,
            "no item is lost or duplicated when the second core joins"
        );
        assert_eq!(
            harness
                .world
                .get::<CoreBuild>(first)
                .map(|build| build.storage_capacity),
            harness
                .world
                .get::<CoreBuild>(second)
                .map(|build| build.storage_capacity),
            "both cores report the unified capacity"
        );
    }

    /// A second core placed with items before the alias exists (e.g. a map
    /// generator / a save with per-core copies) does not corrupt the total:
    /// the aggregate counts the canonical holder only.
    #[test]
    fn non_canonical_module_is_inert_and_not_summed() {
        let mut harness = BuildHarness::new(40, 40, 7);
        let first = place_core(&mut harness, 6, 6, 1);
        let second = place_core(&mut harness, 26, 26, 1);
        let item = copper(&harness);
        let team = team_of(&harness, first);
        add_to_core(&mut harness.world, first, item, 50, 4000);
        // Simulate a save written by upstream (one module copy per core) after
        // the alias: write a duplicate into the inert mirror.
        harness
            .world
            .get_mut::<ItemModule>(second)
            .expect("second module")
            .add(item, 50, 4000);
        assert_eq!(
            team_item_count(&harness.world, team, item),
            50,
            "the alias collapses the duplicate copy"
        );
        let stacks: Vec<_> = team_item_stacks(&harness.world, team);
        assert_eq!(stacks, vec![(item, 50)]);
    }

    /// Enemy/derelict cores are untouched: the alias is team-keyed.
    #[test]
    fn enemy_and_derelict_cores_are_not_pooled() {
        let mut harness = BuildHarness::new(48, 48, 7);
        let first = place_core(&mut harness, 6, 6, 1);
        let second = place_core(&mut harness, 26, 26, 1);
        let enemy = place_core(&mut harness, 6, 40, 2);
        let derelict = place_core(&mut harness, 40, 6, 0);
        let item = copper(&harness);
        let team = team_of(&harness, first);
        assert_ne!(team_of(&harness, enemy), team);
        assert_ne!(team_of(&harness, derelict), team);
        assert_eq!(core_holder(&harness.world, enemy), enemy);
        assert_eq!(core_holder(&harness.world, derelict), derelict);

        add_to_core(&mut harness.world, first, item, 70, 4000);
        assert_eq!(team_item_count(&harness.world, team, item), 70);
        assert_eq!(team_item_count(&harness.world, 2, item), 0);
        assert_eq!(team_item_count(&harness.world, 0, item), 0);
        assert_eq!(core_item_count(&harness.world, second, item), 70);
        assert_eq!(core_item_count(&harness.world, enemy, item), 0);
        assert_eq!(core_item_count(&harness.world, derelict, item), 0);

        // An enemy deposit does not leak into the player's pool either.
        add_to_core(&mut harness.world, enemy, item, 9, 4000);
        assert_eq!(team_item_count(&harness.world, 2, item), 9);
        assert_eq!(team_item_count(&harness.world, team, item), 70);
    }

    /// `CoreBlock.onRemoved`: destroying the canonical core migrates the pool
    /// to the survivor (Java's remaining cores keep the same module object).
    #[test]
    fn destroying_the_canonical_core_keeps_the_pool() {
        let mut harness = BuildHarness::new(40, 40, 7);
        let first = place_core(&mut harness, 6, 6, 1);
        let second = place_core(&mut harness, 26, 26, 1);
        let item = copper(&harness);
        let team = team_of(&harness, first);
        add_to_core(&mut harness.world, first, item, 80, 4000);

        // `remove_from_proximity` then the `WorldHooks::on_removed` dispatch
        // (`BuildingComp.removeFromProximity` -> `onRemoved`) then despawn.
        let _ = crate::world::proximity::remove_from_proximity(
            &mut harness.world,
            first,
            &harness.content,
        );
        on_core_removed(&mut harness.world, first);
        harness.world.despawn(first);

        assert_eq!(canonical_core(&harness.world, team), Some(second));
        assert_eq!(core_holder(&harness.world, second), second);
        assert_eq!(
            module_count(&harness, second, item),
            80,
            "the pool migrated to the surviving core"
        );
        assert_eq!(team_item_count(&harness.world, team, item), 80);
        let capacity = harness
            .world
            .get::<CoreBuild>(second)
            .map(|build| build.storage_capacity)
            .unwrap_or(0);
        assert_eq!(
            capacity, 4000,
            "the dead core's capacity must not stay counted"
        );
    }

    /// Destroying the **last** core writes the Java proportional share into the
    /// linked storages (`CoreBlock.onRemoved`), so the items are not lost.
    #[test]
    fn last_core_shares_its_pool_with_linked_storages() {
        let mut harness = BuildHarness::new(40, 40, 7);
        let core = place_core(&mut harness, 6, 6, 1);
        let container = harness.content().block_id("container").expect("container");
        assert!(harness.place(8, 6, container, 0, true));
        let storage = harness.build_at(8, 6).expect("container entity");
        let item = copper(&harness);
        assert_eq!(
            harness
                .world
                .get::<StorageBuild>(storage)
                .and_then(|state| state.linked_core),
            Some(core)
        );
        add_to_core(&mut harness.world, core, item, 300, 4300);
        // core-shard 4000 + container 300: the container's capacity is 300.
        let _ = crate::world::proximity::remove_from_proximity(
            &mut harness.world,
            core,
            &harness.content,
        );
        on_core_removed(&mut harness.world, core);
        harness.world.despawn(core);

        assert_eq!(
            harness
                .world
                .get::<StorageBuild>(storage)
                .and_then(|state| state.linked_core),
            None,
            "the storage is released from the dead core"
        );
        // 300 items * 300 capacity / 300 total capacity = the full 300.
        assert_eq!(module_count(&harness, storage, item), 300);
    }

    /// The PAR-0484 linked-storage route still resolves through the pool, even
    /// when the storage is owned by a non-canonical core.
    #[test]
    fn linked_storage_of_a_second_core_reads_the_team_pool() {
        let mut harness = BuildHarness::new(40, 40, 7);
        let first = place_core(&mut harness, 6, 6, 1);
        let second = place_core(&mut harness, 20, 20, 1);
        let container = harness.content().block_id("container").expect("container");
        // 2x2 container at (22,20) touches the 3x3 core at (20,20).
        assert!(harness.place(22, 20, container, 0, true));
        let storage = harness.build_at(22, 20).expect("container entity");
        assert_eq!(
            harness
                .world
                .get::<StorageBuild>(storage)
                .and_then(|state| state.linked_core),
            Some(second),
            "the container is owned by the adjacent second core"
        );
        let item = copper(&harness);
        add_to_core(&mut harness.world, first, item, 33, 4000);

        let holder = super::super::storage_block::item_holder(&harness.world, storage);
        assert_eq!(holder, first, "storage -> second core -> canonical core");
        assert_eq!(
            super::super::storage_block::item_count(&harness.world, storage, item),
            33
        );
        let taken = transfer::dispatch_remove_stack(&mut harness.world, storage, item, 8);
        assert_eq!(taken, 8);
        assert_eq!(module_count(&harness, first, item), 25);
        assert_eq!(module_count(&harness, second, item), 0);
    }

    /// The helper keeps the single-core path: holder == core and its own module
    /// is the pool.
    #[test]
    fn single_core_path_is_unchanged() {
        let mut harness = BuildHarness::new(24, 24, 7);
        let core = place_core(&mut harness, 6, 6, 1);
        let item = copper(&harness);
        assert_eq!(core_holder(&harness.world, core), core);
        add_to_core(&mut harness.world, core, item, 11, 4000);
        assert_eq!(module_count(&harness, core, item), 11);
        assert_eq!(team_item_count(&harness.world, 1, item), 11);
        assert_eq!(team_item_stacks(&harness.world, 1), vec![(item, 11)]);
        // A non-core building is never aliased.
        let container = harness.content().block_id("container").expect("container");
        assert!(harness.place(10, 6, container, 0, true));
        let storage = harness.build_at(10, 6).expect("container");
        assert_eq!(core_holder(&harness.world, storage), storage);
        assert!(
            harness
                .world
                .get::<StorageBuild>(storage)
                .is_some_and(|state| state.linked_core.is_none()),
            "unlinked storage keeps its own module"
        );
    }
}
