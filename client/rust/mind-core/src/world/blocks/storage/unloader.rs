// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Unloader` build behavior (`world/blocks/storage/Unloader.java`) — plan 08
//! M4. Full port of the `ContainerStat` cache, the comparator, `isPossibleItem`
//! and the `unloadAccumulate` trade loop.

use std::cmp::Ordering;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockKind, ItemId};
use crate::world::behavior::BuildingBehavior;
use crate::world::block::BlockTable;
use crate::world::config::ConfigValue;
use crate::world::modules::ItemModule;
use crate::world::update::delta;

use super::super::distribution::transfer;
use super::super::storage::storage_block::StorageBuild;

/// `Unloader.ContainerStat`.
#[derive(Debug, Clone, Default)]
pub struct ContainerStat {
    /// Neighbor building (`building`).
    pub building: Option<Entity>,
    /// `items.get(item) / getMaximumAccepted(item)` (`loadFactor`).
    pub load_factor: f32,
    /// Whether the neighbor can receive the current item (`canLoad`).
    pub can_load: bool,
    /// Whether the neighbor can give the current item (`canUnload`).
    pub can_unload: bool,
    /// Cached `!(other is CoreBuild || StorageBuild)` (`notStorage`).
    pub not_storage: bool,
    /// Priority cursor (`lastUsed`).
    pub last_used: u32,
}

/// `Unloader.UnloaderBuild` state.
#[derive(Debug, Clone, Default, Component)]
pub struct UnloaderBuild {
    /// Unload timer (`unloadTimer`).
    pub unload_timer: f32,
    /// Round-robin item cursor (`rotations`).
    pub rotations: u32,
    /// Configured item (`sortItem`).
    pub sort_item: Option<ItemId>,
    /// Cached neighbors (`possibleBlocks`).
    pub possible: Vec<ContainerStat>,
}

fn is_storage_kind(world: &World, e: Entity) -> bool {
    world
        .get::<crate::entities::comp::Building>(e)
        .zip(world.get_resource::<BlockTable>())
        .is_some_and(|(building, table)| {
            table.get(building.block).is_some_and(|inst| {
                matches!(
                    inst.def.kind,
                    BlockKind::CoreBlock | BlockKind::StorageBlock
                )
            })
        })
}

fn same_team(world: &World, a: Entity, b: Entity) -> bool {
    world
        .get::<crate::entities::comp::TeamComp>(a)
        .zip(world.get::<crate::entities::comp::TeamComp>(b))
        .is_some_and(|(x, y)| x.team == y.team)
}

fn has_items(world: &World, e: Entity) -> bool {
    world.get::<ItemModule>(e).is_some()
}

fn item_count(world: &World, e: Entity, item: ItemId) -> i32 {
    world.get::<ItemModule>(e).map(|m| m.get(item)).unwrap_or(0)
}

/// `UnloaderBuild.comparator`. Ascending order: unload-from is the last element.
pub fn compare(a: &ContainerStat, b: &ContainerStat) -> Ordering {
    let unload_core = (!a.not_storage).cmp(&!b.not_storage);
    if unload_core != Ordering::Equal {
        return unload_core;
    }
    let unload_priority = (a.can_unload && !a.can_load).cmp(&(b.can_unload && !b.can_load));
    if unload_priority != Ordering::Equal {
        return unload_priority;
    }
    let load_priority = (a.can_unload || !a.can_load).cmp(&(b.can_unload || !b.can_load));
    if load_priority != Ordering::Equal {
        return load_priority;
    }
    let load_factor = a
        .load_factor
        .partial_cmp(&b.load_factor)
        .unwrap_or(Ordering::Equal);
    if load_factor != Ordering::Equal {
        return load_factor;
    }
    b.last_used.cmp(&a.last_used)
}

/// `Unloader` behavior (`unloader`).
#[derive(Debug, Clone, Copy)]
pub struct UnloaderBehavior {
    /// `Unloader.speed`.
    pub speed: f32,
    /// `Unloader.allowCoreUnload`.
    pub allow_core_unload: bool,
}

impl UnloaderBehavior {
    /// Vanilla `unloader`.
    pub const VANILLA: UnloaderBehavior = UnloaderBehavior {
        speed: 1.0,
        allow_core_unload: true,
    };
}

impl BuildingBehavior for UnloaderBehavior {
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
        if world.get::<UnloaderBuild>(e).is_none() {
            world.entity_mut(e).insert(UnloaderBuild::default());
        }
    }

    fn always_update_when_disabled(&self) -> bool {
        true
    }

    fn on_proximity_update(&self, world: &mut World, e: Entity) {
        let Some(building) = world.get::<crate::entities::comp::Building>(e) else {
            return;
        };
        let proximity: Vec<Entity> = building.proximity.iter().copied().collect();
        let mut possible: Vec<ContainerStat> = Vec::with_capacity(proximity.len());
        for other in proximity {
            if !same_team(world, e, other) {
                continue;
            }
            let not_storage = !is_storage_kind(world, other);
            let storage_unlinked = world
                .get::<StorageBuild>(other)
                .is_some_and(|b| b.linked_core.is_none());
            let can_unload = transfer::dispatch_can_unload(world, other)
                && (self.allow_core_unload || not_storage || storage_unlinked)
                && has_items(world, other);
            if not_storage || can_unload {
                possible.push(ContainerStat {
                    building: Some(other),
                    not_storage,
                    ..ContainerStat::default()
                });
            }
        }
        if let Some(mut unloader) = world.get_mut::<UnloaderBuild>(e) {
            unloader.possible = possible;
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(mut build) = world.get::<UnloaderBuild>(e).cloned() else {
            return;
        };
        build.unload_timer += delta(world, e);
        if build.unload_timer < self.speed || build.possible.len() < 2 {
            world.entity_mut(e).insert(build);
            return;
        }
        let item_types = world
            .get::<ItemModule>(e)
            .map(|m| m.items.len())
            .unwrap_or(0);

        let mut item: Option<ItemId> = None;
        if let Some(sort) = build.sort_item {
            if self.is_possible_item(world, e, sort, &mut build) {
                item = Some(sort);
            }
        } else if item_types > 0 {
            for i in 0..item_types {
                let id = ((build.rotations as usize + i + 1) % item_types) as u16;
                let possible_item = ItemId::new(id);
                if self.is_possible_item(world, e, possible_item, &mut build) {
                    item = Some(possible_item);
                    break;
                }
            }
        }

        match item {
            Some(item) => {
                build.rotations = item.raw() as u32;
                for pb in &mut build.possible {
                    if let Some(other) = pb.building {
                        let max = transfer::dispatch_get_maximum_accepted(world, other, item);
                        pb.load_factor = if max == 0 {
                            0.0
                        } else {
                            item_count(world, other, item) as f32 / max as f32
                        };
                        pb.last_used = (pb.last_used + 1) % (i32::MAX as u32);
                    }
                }
                build.possible.sort_by(compare);
                self.unload_accumulate(world, e, item, &mut build);
            }
            None => {
                build.unload_timer = build.unload_timer.min(self.speed);
            }
        }
        world.entity_mut(e).insert(build);
    }

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        match world.get::<UnloaderBuild>(e).and_then(|b| b.sort_item) {
            Some(item) => ConfigValue::Item(item),
            None => ConfigValue::None,
        }
    }

    fn configured(
        &self,
        world: &mut World,
        e: Entity,
        _player: Option<Entity>,
        value: ConfigValue,
    ) {
        if let Some(mut b) = world.get_mut::<UnloaderBuild>(e) {
            b.sort_item = match value {
                ConfigValue::Item(item) => Some(item),
                _ => None,
            };
        }
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        1
    }

    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        let id = world
            .get::<UnloaderBuild>(e)
            .and_then(|b| b.sort_item)
            .map(|item| item.raw() as i16)
            .unwrap_or(-1);
        w.s(id);
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        revision: u8,
    ) {
        let mut build = world.get::<UnloaderBuild>(e).cloned().unwrap_or_default();
        let id = if revision == 1 {
            r.s().unwrap_or(-1)
        } else {
            r.b().map(|v| v as i16).unwrap_or(-1)
        };
        build.sort_item = (id >= 0).then(|| ItemId::new(id as u16));
        world.entity_mut(e).insert(build);
    }
}

impl UnloaderBehavior {
    fn is_possible_item(
        &self,
        world: &World,
        e: Entity,
        item: ItemId,
        build: &mut UnloaderBuild,
    ) -> bool {
        let mut has_provider = false;
        let mut has_receiver = false;
        let mut is_distinct = false;
        for pb in &mut build.possible {
            let Some(other) = pb.building else {
                continue;
            };
            pb.can_load = pb.not_storage && transfer::dispatch_accept_item(world, other, e, item);
            pb.can_unload = transfer::dispatch_can_unload(world, other)
                && has_items(world, other)
                && item_count(world, other, item) > 0;
            is_distinct |= (has_provider && pb.can_load) || (has_receiver && pb.can_unload);
            has_provider |= pb.can_unload;
            has_receiver |= pb.can_load;
        }
        is_distinct
    }

    fn unload_accumulate(
        &self,
        world: &mut World,
        e: Entity,
        item: ItemId,
        build: &mut UnloaderBuild,
    ) {
        let mut any = false;
        while build.unload_timer >= self.speed {
            let pbs = build.possible.len();
            if pbs == 0 {
                break;
            }
            let mut to_index: Option<usize> = None;
            for i in 0..pbs {
                let pb = &build.possible[i];
                if pb.can_load
                    && let Some(other) = pb.building
                    && transfer::dispatch_accept_item(world, other, e, item)
                {
                    to_index = Some(i);
                    break;
                }
            }
            let mut from_index: Option<usize> = None;
            for i in (0..pbs).rev() {
                let pb = &build.possible[i];
                if pb.can_unload
                    && let Some(other) = pb.building
                    && transfer::dispatch_can_unload(world, other)
                    && item_count(world, other, item) > 0
                {
                    from_index = Some(i);
                    break;
                }
            }
            let (Some(to_index), Some(from_index)) = (to_index, from_index) else {
                break;
            };
            let to = build.possible[to_index].building.unwrap_or(e);
            let from = build.possible[from_index].building.unwrap_or(e);
            let from_max = transfer::dispatch_get_maximum_accepted(world, from, item);
            let to_max = transfer::dispatch_get_maximum_accepted(world, to, item);
            let from_factor = if from_max == 0 {
                0.0
            } else {
                item_count(world, from, item) as f32 / from_max as f32
            };
            let to_factor = if to_max == 0 {
                0.0
            } else {
                item_count(world, to, item) as f32 / to_max as f32
            };
            build.possible[from_index].load_factor = from_factor;
            build.possible[to_index].load_factor = to_factor;
            if from_factor != to_factor || !build.possible[from_index].can_load {
                transfer::dispatch_handle_item(world, to, e, item);
                transfer::dispatch_remove_stack(world, from, item, 1);
                build.possible[to_index].last_used = 0;
                build.possible[from_index].last_used = 0;
                build.unload_timer -= self.speed;
                any = true;
            } else {
                break;
            }
        }
        if !any {
            build.unload_timer = build.unload_timer.min(self.speed);
        }
    }
}

/// `Unloader.allItems` accessor (the unloader's own `ItemModule` slot count).
#[allow(dead_code)]
fn all_items(world: &World, e: Entity) -> usize {
    world
        .get::<ItemModule>(e)
        .map(|m| m.items.len())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;
    use crate::world::config::ConfigValue;

    #[test]
    fn unloader_drains_storage_into_a_lane() {
        let mut harness = BuildHarness::new(32, 32, 7);
        let copper = harness.content().item_id("copper").expect("copper");
        let container = harness.content().block_id("container").expect("container");
        let unloader = harness.content().block_id("unloader").expect("unloader");
        let conveyor = harness.content().block_id("conveyor").expect("conveyor");
        let vault = harness.content().block_id("vault").expect("vault");
        // 2x2 container at (4,4), unloader at (6,4) is adjacent to (5,4).
        assert!(harness.place(4, 4, container, 0, true));
        assert!(harness.place(6, 4, unloader, 0, true));
        // `vault` is size 3; center (11,4) covers (10,4)..(12,6). Place it before
        // the belts so each belt resolves its `next` when it is placed.
        assert!(harness.place(11, 4, vault, 0, true));
        for x in 7..=9 {
            assert!(harness.place(x, 4, conveyor, 0, true));
        }
        let source = harness.build_at(4, 4).expect("source");
        harness
            .world
            .get_mut::<ItemModule>(source)
            .expect("items")
            .add(copper, 100, 300);
        for _ in 0..600 {
            harness.tick();
        }
        let drained = harness
            .build_at(11, 4)
            .and_then(|e| harness.world.get::<ItemModule>(e))
            .map(|m| m.total)
            .unwrap_or(0);
        assert!(drained > 0, "vault total={drained}");
    }

    #[test]
    fn comparator_order_matches_java() {
        let core = ContainerStat {
            not_storage: false,
            ..ContainerStat::default()
        };
        let plain = ContainerStat {
            not_storage: true,
            ..ContainerStat::default()
        };
        // Non-core sorts before core (core is the unload-from end).
        assert_eq!(compare(&plain, &core), Ordering::Less);
        let unload_only = ContainerStat {
            not_storage: true,
            can_unload: true,
            can_load: false,
            ..ContainerStat::default()
        };
        let load_only = ContainerStat {
            not_storage: true,
            can_unload: false,
            can_load: true,
            ..ContainerStat::default()
        };
        assert_eq!(compare(&unload_only, &load_only), Ordering::Greater);
    }

    #[test]
    fn unloader_config_roundtrip() {
        let mut harness = BuildHarness::new(16, 16, 7);
        let copper = harness.content().item_id("copper").expect("copper");
        let unloader = harness.content().block_id("unloader").expect("unloader");
        assert!(harness.place(5, 4, unloader, 0, true));
        assert!(harness.configure(5, 4, ConfigValue::Item(copper)));
        let read = harness
            .build_at(5, 4)
            .map(|e| crate::world::config::read_config(&harness.world, e));
        assert_eq!(read, Some(ConfigValue::Item(copper)));
    }
}
