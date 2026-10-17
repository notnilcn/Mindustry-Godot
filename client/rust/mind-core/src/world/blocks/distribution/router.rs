// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Router` build behavior (`world/blocks/distribution/Router.java`) — plan 08
//! M2. Per-item rotation cursors, `lastItem`/`lastInput` and the `overflowGate`
//! source skip. `RouterBuild implements ControlBlock` (`Router.java:30-53`):
//! a size-1 router has a lazily created `block` proxy unit the player can
//! possess; while possessed it routes only toward the aimed side, and only
//! while shooting (`Router.java:102-117`; L06 T2).

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
    /// `ControlBlock.unit()` proxy (`unit`, `Router.java:35-44`): created on
    /// first use by `Sim::control_block_unit_id`, never added to the unit pass.
    pub block_unit: Option<Entity>,
}

impl Default for RouterBuild {
    fn default() -> Self {
        Self {
            cycles: Vec::new(),
            last_item: None,
            last_input: None,
            time: 0.0,
            block_unit: None,
        }
    }
}

/// `isControlled()` (`ControlBlock.java:9-11`): the proxy holds the player.
pub fn router_controlled_unit(world: &World, e: Entity) -> Option<Entity> {
    world
        .get::<RouterBuild>(e)
        .and_then(|router| router.block_unit)
        .filter(|unit| world.get::<crate::entities::comp::Player>(*unit).is_some())
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

    /// The proxy is never in `Groups.unit`; it goes with the building.
    fn on_removed(&self, world: &mut World, e: Entity) {
        if let Some(unit) = world.get::<RouterBuild>(e).and_then(|r| r.block_unit)
            && world.get_entity(unit).is_ok()
        {
            let _ = world.despawn(unit);
        }
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
        let controlled = router_controlled_unit(world, e).is_some();
        if let Some(item) = last_item {
            time += 1.0 / self.speed * delta(world, e);
            let target = self.get_tile_target(world, e, item, last_input, false, controlled);
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
                self.get_tile_target(world, e, item, last_input, true, controlled);
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
    /// `getTileTarget`'s possessed branch (`Router.java:102-117`): sync the
    /// proxy (`health`, `ammo`, `team`, position), take the aimed quadrant
    /// `mod((angleTo(aim) + 45) / 90, 4)` and, only while shooting, the
    /// accepting `nearby(angle)` building; otherwise nothing.
    fn controlled_target(&self, world: &mut World, e: Entity, item: ItemId) -> Option<Entity> {
        let unit = router_controlled_unit(world, e)?;
        let pos = world.get::<crate::entities::comp::Pos>(e).copied()?;
        let team = world
            .get::<crate::entities::comp::TeamComp>(e)
            .map(|t| t.team);
        let health = world
            .get::<crate::entities::comp::Health>(e)
            .map(|h| h.health);
        if let Some(mut unit_pos) = world.get_mut::<crate::entities::comp::Pos>(unit) {
            unit_pos.x = pos.x;
            unit_pos.y = pos.y;
        }
        if let (Some(team), Some(mut unit_team)) =
            (team, world.get_mut::<crate::entities::comp::TeamComp>(unit))
        {
            unit_team.team = team;
        }
        if let (Some(health), Some(mut unit_health)) =
            (health, world.get_mut::<crate::entities::comp::Health>(unit))
        {
            unit_health.health = health;
        }
        let (aim_x, aim_y, shooting) = world
            .get_resource::<crate::ai::types::player::PlayerInput>()
            .map(|input| (input.aim_x, input.aim_y, input.shooting))
            .unwrap_or((pos.x, pos.y, false));
        if !shooting {
            return None;
        }
        // `Angles.angle(x, y, aimX, aimY)` in [0, 360).
        let mut angle = (aim_y - pos.y).atan2(aim_x - pos.x).to_degrees();
        if angle < 0.0 {
            angle += 360.0;
        }
        let quadrant = (((angle + 45.0) / 90.0) as i32).rem_euclid(4) as u8;
        let tile = world.get::<Building>(e)?.tile;
        let (dx, dy) = crate::world::blocks::autotiler::d4(quadrant);
        let other = world
            .get_resource::<crate::world::TileBuilds>()?
            .get(tile.x() as i32 + dx, tile.y() as i32 + dy)?;
        transfer::dispatch_accept_item(world, other, e, item).then_some(other)
    }

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
            return self.controlled_target(world, e, item);
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
