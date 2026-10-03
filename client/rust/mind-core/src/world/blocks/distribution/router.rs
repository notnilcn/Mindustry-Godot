// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Router` build behavior (`world/blocks/distribution/Router.java`) — plan 08
//! M2. Per-item rotation cursors, `lastItem`/`lastInput` and the `overflowGate`
//! source skip. The player-controlled block-unit branch is plan 11 (L8).

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockId, BlockKind, ItemId};
use crate::entities::comp::Building;
use crate::world::behavior::BuildingBehavior;
use crate::world::block::BlockTable;
use crate::world::modules::ItemModule;
use crate::world::{TilePos, delta};

use super::transfer;

/// `Router.RouterBuild` state.
#[derive(Debug, Clone, Component)]
pub struct RouterBuild {
    /// Per-item round-robin cursors (`cycles`), sized to the item count.
    pub cycles: Vec<u8>,
    /// Item being routed (`lastItem`).
    pub last_item: Option<ItemId>,
    /// Last source building (`lastInput`; a tile upstream).
    pub last_input: Option<Entity>,
    /// Accumulated routing time (`time`).
    pub time: f32,
}

impl Default for RouterBuild {
    fn default() -> Self {
        Self {
            cycles: Vec::new(),
            last_item: None,
            last_input: None,
            time: 0.0,
        }
    }
}

fn is_overflow_gate(block: BlockId, table: &BlockTable) -> bool {
    table
        .get(block)
        .is_some_and(|inst| inst.def.kind == BlockKind::OverflowGate)
}

/// `Router` behavior (`router`, `distributor`).
#[derive(Debug, Clone, Copy)]
pub struct RouterBehavior {
    /// `Router.speed`.
    pub speed: f32,
}

impl RouterBehavior {
    /// Vanilla `router`/`distributor` (`speed = 8`).
    pub const VANILLA: RouterBehavior = RouterBehavior { speed: 8.0 };
}

impl BuildingBehavior for RouterBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<RouterBuild>(e).is_none() {
            let len = world
                .get::<ItemModule>(e)
                .map(|module| module.items.len())
                .unwrap_or(0);
            world.entity_mut(e).insert(RouterBuild {
                cycles: vec![0; len],
                ..RouterBuild::default()
            });
        }
    }

    fn always_update_when_disabled(&self) -> bool {
        true
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let (mut last_item, last_input, mut time) = {
            let Some(router) = world.get::<RouterBuild>(e) else {
                return;
            };
            (router.last_item, router.last_input, router.time)
        };
        if last_item.is_none() && transfer::item_total(world, e) > 0 {
            last_item = transfer::first_item(world, e);
        }
        if let Some(item) = last_item {
            time += 1.0 / self.speed * delta(world, e);
            let target = self.get_tile_target(world, e, item, last_input, false, false);
            // Java: `time >= 1 || !(target.block instanceof Router || target.block.instantTransfer)`.
            let deferred = target
                .and_then(|t| world.get::<Building>(t))
                .and_then(|b| {
                    world
                        .get_resource::<BlockTable>()
                        .and_then(|table| table.get(b.block))
                        .map(|inst| {
                            matches!(
                                inst.def.kind,
                                BlockKind::Router | BlockKind::Sorter | BlockKind::OverflowGate
                            )
                        })
                })
                .unwrap_or(false);
            if let Some(target) = target
                && (time >= 1.0 || !deferred)
            {
                self.get_tile_target(world, e, item, last_input, true, false);
                transfer::dispatch_handle_item(world, target, e, item);
                if let Some(mut items) = world.get_mut::<ItemModule>(e) {
                    items.remove(item, 1);
                }
                last_item = None;
            }
        }
        if let Some(mut router) = world.get_mut::<RouterBuild>(e) {
            router.last_item = last_item;
            router.time = time;
        }
    }

    fn update_batch(
        &self,
        world: &mut World,
        inst: &crate::world::block::BlockInstance,
        entities: &[Entity],
    ) {
        // Plan 08 §7.4: allocate-free batched dispatch (empty-consumer fast path).
        for &e in entities {
            crate::world::update::building_update_no_consumers(world, e, inst);
        }
    }

    fn accept_stack(
        &self,
        _world: &World,
        _e: Entity,
        _item: ItemId,
        _amount: i32,
        _source: Option<Entity>,
    ) -> i32 {
        0
    }

    fn accept_item(&self, world: &World, e: Entity, source: Entity, _item: ItemId) -> bool {
        let same_team = world
            .get::<crate::entities::comp::TeamComp>(e)
            .zip(world.get::<crate::entities::comp::TeamComp>(source))
            .is_some_and(|(a, b)| a.team == b.team);
        let router = world.get::<RouterBuild>(e);
        same_team
            && router.is_some_and(|r| r.last_item.is_none())
            && transfer::item_total(world, e) == 0
    }

    fn handle_item(&self, world: &mut World, e: Entity, source: Entity, item: ItemId) {
        if let Some(mut items) = world.get_mut::<ItemModule>(e) {
            items.add(item, 1, 1);
        }
        if let Some(mut router) = world.get_mut::<RouterBuild>(e) {
            router.last_item = Some(item);
            router.time = 0.0;
            router.last_input = Some(source);
        }
    }

    fn remove_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) -> i32 {
        let result = transfer::default_remove_stack(world, e, item, amount);
        if result != 0
            && let Some(mut router) = world.get_mut::<RouterBuild>(e)
            && router.last_item == Some(item)
        {
            router.last_item = None;
        }
        result
    }
}

impl RouterBehavior {
    /// `RouterBuild.getTileTarget`. `set` advances the per-item cursor.
    fn get_tile_target(
        &self,
        world: &mut World,
        e: Entity,
        item: ItemId,
        from: Option<Entity>,
        set: bool,
        controlled: bool,
    ) -> Option<Entity> {
        if controlled {
            return None; // L8: block units are plan 11.
        }
        let len = world
            .get::<Building>(e)
            .map(|b| b.proximity.len())
            .unwrap_or(0);
        if len == 0 {
            return None;
        }
        let counter = world
            .get::<RouterBuild>(e)
            .map(|r| *r.cycles.get(item.index()).unwrap_or(&0) as usize)
            .unwrap_or(0);
        let from_is_overflow = from
            .and_then(|f| world.get::<Building>(f))
            .and_then(|b| {
                world
                    .get_resource::<BlockTable>()
                    .map(|table| is_overflow_gate(b.block, table))
            })
            .unwrap_or(false);
        for i in 0..len {
            // Index the proximity list in place instead of collecting it into a
            // `Vec` per call (two allocations per router per tick before; plan
            // 08 §7.4 alloc-audit). `proximity` is short, so re-reading it is
            // cheaper than the heap traffic.
            let Some(other) = world
                .get::<Building>(e)
                .and_then(|b| b.proximity.get((i + counter) % len).copied())
            else {
                continue;
            };
            if set
                && let Some(mut router) = world.get_mut::<RouterBuild>(e)
                && item.index() < router.cycles.len()
            {
                router.cycles[item.index()] =
                    ((router.cycles[item.index()] as usize + 1) % len) as u8;
            }
            if from_is_overflow && from == Some(other) {
                continue;
            }
            if transfer::dispatch_accept_item(world, other, e, item) {
                return Some(other);
            }
        }
        None
    }
}

/// `TilePos` re-export used by the router tests.
#[allow(dead_code)]
fn _tile_pos() -> TilePos {
    TilePos::new(0, 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;

    #[test]
    fn router_distributes_to_all_outputs() {
        let mut harness = BuildHarness::new(16, 16, 7);
        let coal = harness.content().item_id("coal").expect("coal");
        harness.register_behavior(
            "item-source",
            std::sync::Arc::new(crate::world::fixtures::logistics::SourceBehavior {
                item: coal,
                per_tick: 1,
            }),
        );
        let source = harness.content().block_id("item-source").expect("source");
        let belt = harness.content().block_id("conveyor").expect("conveyor");
        let router = harness.content().block_id("router").expect("router");
        let container = harness.content().block_id("container").expect("container");
        assert!(harness.place(5, 2, source, 1, true));
        assert!(harness.place(5, 3, belt, 1, true));
        assert!(harness.place(5, 4, router, 0, true));
        // 2x2 containers whose footprints touch the router's left/right/up tiles.
        assert!(harness.place(3, 3, container, 0, true));
        assert!(harness.place(6, 3, container, 0, true));
        assert!(harness.place(5, 5, container, 0, true));
        for _ in 0..300 {
            harness.tick();
        }
        let counts: Vec<i32> = [(3, 3), (6, 3), (5, 5)]
            .iter()
            .map(|(x, y)| {
                harness
                    .build_at(*x, *y)
                    .and_then(|e| harness.world.get::<ItemModule>(e))
                    .map(|items| items.total)
                    .unwrap_or(0)
            })
            .collect();
        let max = counts.iter().copied().max().unwrap_or(0);
        let min = counts.iter().copied().min().unwrap_or(0);
        assert!(
            counts.iter().all(|count| *count > 0) && max - min <= 2,
            "router outputs: {counts:?}"
        );
    }
}
