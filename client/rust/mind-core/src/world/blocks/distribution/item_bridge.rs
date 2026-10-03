// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `ItemBridge`/`BufferedItemBridge` build behavior
//! (`world/blocks/distribution/ItemBridge.java`,
//! `BufferedItemBridge.java`) — plan 08 M3.
//!
//! Configured link (packed tile pos / relative `Point2`), `incoming` supplier
//! pruning, the transport timer and the buffered `ItemBuffer` path. The exact
//! upstream `checkAccept` compatibility bug is preserved.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use smallvec::SmallVec;

use crate::content::{BlockId, BlockKind, ItemId};
use crate::entities::comp::Building;
use crate::world::behavior::BuildingBehavior;
use crate::world::block::BlockTable;
use crate::world::config::ConfigValue;
use crate::world::item_buffer::ItemBuffer;
use crate::world::modules::ItemModule;
use crate::world::update::{build_time, delta, edelta, run_timer};
use crate::world::{TileBuilds, TilePos};

use super::transfer;

/// `ItemBridge.ItemBridgeBuild` state.
#[derive(Debug, Clone, Component)]
pub struct ItemBridgeBuild {
    /// Packed destination tile pos or `-1` (`link`).
    pub link: i32,
    /// Suppliers pointing at this bridge (`incoming`).
    pub incoming: SmallVec<[i32; 4]>,
    /// Warmup (`warmup`).
    pub warmup: f32,
    /// Animation time (`time`).
    pub time: f32,
    /// Animation speed (`timeSpeed`).
    pub time_speed: f32,
    /// Moved in the previous 30-tick window (`wasMoved`).
    pub was_moved: bool,
    /// Moved this tick (`moved`).
    pub moved: bool,
    /// Whether the link was valid last tick (`hadValidLink`).
    pub had_valid_link: bool,
    /// Transport accumulator (`transportCounter`).
    pub transport_counter: f32,
    /// Buffered queue (`BufferedItemBridgeBuild.buffer`).
    pub buffer: Option<ItemBuffer>,
}

impl Default for ItemBridgeBuild {
    fn default() -> Self {
        Self {
            link: -1,
            incoming: SmallVec::new(),
            warmup: 0.0,
            time: -8.0,
            time_speed: 0.0,
            was_moved: false,
            moved: false,
            had_valid_link: false,
            transport_counter: 0.0,
            buffer: None,
        }
    }
}

fn is_item_bridge(block: BlockId, table: &BlockTable) -> bool {
    table.get(block).is_some_and(|inst| {
        matches!(
            inst.def.kind,
            BlockKind::ItemBridge | BlockKind::BufferedItemBridge
        )
    })
}

fn tile_entity(world: &World, packed: i32) -> Option<Entity> {
    let (x, y) = crate::world::pos::unpack(packed);
    world.get_resource::<TileBuilds>()?.get(x as i32, y as i32)
}

/// `ItemBridge.positionsValid(x1, y1, x2, y2, baseRange)`.
pub fn positions_valid(x1: i32, y1: i32, x2: i32, y2: i32, base_range: i32) -> bool {
    if x1 == x2 {
        (y1 - y2).abs() <= base_range
    } else if y1 == y2 {
        (x1 - x2).abs() <= base_range
    } else {
        false
    }
}

/// `ItemBridge` behavior (`phase-conveyor`, `bridge-conveyor`).
#[derive(Debug, Clone, Copy)]
pub struct ItemBridgeBehavior {
    /// `ItemBridge.range`.
    pub range: i32,
    /// `ItemBridge.transportTime` (non-buffered).
    pub transport_time: f32,
    /// `ItemBridge.linkSameType`.
    pub link_same_type: bool,
    /// Whether this is a `BufferedItemBridge` (`buffer` path).
    pub buffered: bool,
    /// `BufferedItemBridge.bufferCapacity`.
    pub buffer_capacity: usize,
    /// `BufferedItemBridge.speed`.
    pub speed: f32,
    /// Whether accepting when disabled is suppressed.
    pub no_accept_disabled: bool,
}

impl ItemBridgeBehavior {
    /// `phase-conveyor`.
    pub const PHASE_CONVEYOR: ItemBridgeBehavior = ItemBridgeBehavior {
        range: 12,
        transport_time: 2.0,
        link_same_type: true,
        buffered: false,
        buffer_capacity: 0,
        speed: 0.0,
        no_accept_disabled: false,
    };
    /// `bridge-conveyor`.
    pub const BRIDGE_CONVEYOR: ItemBridgeBehavior = ItemBridgeBehavior {
        range: 4,
        transport_time: 0.0,
        link_same_type: true,
        buffered: true,
        buffer_capacity: 14,
        speed: 74.0,
        no_accept_disabled: false,
    };

    /// `ItemBridge.linkValid(tile, other, checkDouble)`.
    fn link_valid(&self, world: &World, e: Entity, other: Entity, check_double: bool) -> bool {
        let (Some(a), Some(b)) = (world.get::<Building>(e), world.get::<Building>(other)) else {
            return false;
        };
        if !positions_valid(
            a.tile.x() as i32,
            a.tile.y() as i32,
            b.tile.x() as i32,
            b.tile.y() as i32,
            self.range,
        ) {
            return false;
        }
        let same_block = a.block == b.block;
        let Some(table) = world.get_resource::<BlockTable>() else {
            return false;
        };
        let both_bridges = is_item_bridge(a.block, table) && is_item_bridge(b.block, table);
        let type_ok = if self.link_same_type {
            same_block
        } else {
            both_bridges
        };
        if !type_ok {
            return false;
        }
        let same_team = world
            .get::<crate::entities::comp::TeamComp>(e)
            .zip(world.get::<crate::entities::comp::TeamComp>(other))
            .is_some_and(|(x, y)| x.team == y.team);
        if !same_team {
            return false;
        }
        if check_double
            && world
                .get::<ItemBridgeBuild>(other)
                .is_some_and(|b| b.link == a.tile.pack())
        {
            return false;
        }
        true
    }

    fn linked(&self, world: &World, e: Entity, source: Entity) -> bool {
        if !self.link_valid(world, source, e, true) {
            return false;
        }
        let tile = match world.get::<Building>(e) {
            Some(b) => b.tile.pack(),
            None => return false,
        };
        world
            .get::<ItemBridgeBuild>(source)
            .is_some_and(|b| b.link == tile)
    }

    fn check_accept(&self, world: &World, e: Entity, source: Entity, link_tile: i32) -> bool {
        let Some(a) = world.get::<Building>(e) else {
            return false;
        };
        if self.linked(world, e, source) {
            return true;
        }
        let Some(other) = tile_entity(world, link_tile) else {
            return false;
        };
        if !self.link_valid(world, e, other, false) {
            return false;
        }
        let other_tile = world
            .get::<Building>(other)
            .map(|b| b.tile)
            .unwrap_or_default();
        let rel = relative_to(a.tile, other_tile);
        let size = world
            .get_resource::<BlockTable>()
            .and_then(|table| world.get::<Building>(source).map(|b| (table, b.block)))
            .and_then(|(table, block)| table.get(block))
            .map(|inst| inst.def.size)
            .unwrap_or(1)
            .max(1);
        let facing = crate::world::edges::facing_edge(
            size,
            world
                .get::<Building>(source)
                .map(|b| b.tile.x() as i32)
                .unwrap_or(0),
            world
                .get::<Building>(source)
                .map(|b| b.tile.y() as i32)
                .unwrap_or(0),
            a.tile,
        );
        let rel2 = relative_to(a.tile, facing);
        // `incoming` bug kept commented upstream (issue #9257).
        rel != rel2
    }

    fn check_dump(&self, world: &World, e: Entity, to: Entity) -> bool {
        let Some(a) = world.get::<Building>(e) else {
            return false;
        };
        let link_tile = world
            .get::<ItemBridgeBuild>(e)
            .map(|b| b.link)
            .unwrap_or(-1);
        if let Some(other) = tile_entity(world, link_tile)
            && self.link_valid(world, e, other, false)
        {
            let other_tile = world
                .get::<Building>(other)
                .map(|b| b.tile)
                .unwrap_or_default();
            let rel = relative_to(a.tile, other_tile);
            let rel2 = relative_to(
                a.tile,
                world
                    .get::<Building>(to)
                    .map(|b| b.tile)
                    .unwrap_or_default(),
            );
            return rel != rel2;
        }
        let Some(to_building) = world.get::<Building>(to) else {
            return true;
        };
        let edge = crate::world::edges::facing_edge(
            1,
            to_building.tile.x() as i32,
            to_building.tile.y() as i32,
            a.tile,
        );
        let i = relative_to(a.tile, edge);
        let incoming = world
            .get::<ItemBridgeBuild>(e)
            .map(|b| b.incoming.clone())
            .unwrap_or_default();
        !incoming
            .iter()
            .any(|v| relative_to(a.tile, TilePos::from_pack(*v)) == i)
    }
}

fn relative_to(origin: TilePos, other: TilePos) -> i8 {
    let x = origin.x() as i32;
    let y = origin.y() as i32;
    let cx = other.x() as i32;
    let cy = other.y() as i32;
    if x == cx && y == cy - 1 {
        1
    } else if x == cx && y == cy + 1 {
        3
    } else if x == cx - 1 && y == cy {
        0
    } else if x == cx + 1 && y == cy {
        2
    } else {
        -1
    }
}

impl BuildingBehavior for ItemBridgeBehavior {
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
        if world.get::<ItemBridgeBuild>(e).is_none() {
            let buffer = self.buffered.then(|| ItemBuffer::new(self.buffer_capacity));
            world.entity_mut(e).insert(ItemBridgeBuild {
                buffer,
                ..ItemBridgeBuild::default()
            });
        }
    }

    fn always_update_when_disabled(&self) -> bool {
        true
    }

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        let (link, tile) = match (world.get::<ItemBridgeBuild>(e), world.get::<Building>(e)) {
            (Some(bridge), Some(building)) => (bridge.link, building.tile),
            _ => return ConfigValue::None,
        };
        let (lx, ly) = crate::world::pos::unpack(link);
        ConfigValue::Point2(lx as i32 - tile.x() as i32, ly as i32 - tile.y() as i32)
    }

    fn configured(
        &self,
        world: &mut World,
        e: Entity,
        _player: Option<Entity>,
        value: ConfigValue,
    ) {
        let tile = world.get::<Building>(e).map(|b| b.tile);
        let link = match (value, tile) {
            (ConfigValue::Point2(x, y), Some(tile)) => {
                TilePos::new((tile.x() as i32 + x) as i16, (tile.y() as i32 + y) as i16).pack()
            }
            (ConfigValue::Number(n), _) => n as i32,
            _ => -1,
        };
        if let Some(mut bridge) = world.get_mut::<ItemBridgeBuild>(e) {
            bridge.link = link;
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        // 30-tick moved pulse.
        if run_timer(world, e, 0, 30.0)
            && let Some(mut bridge) = world.get_mut::<ItemBridgeBuild>(e)
        {
            bridge.was_moved = bridge.moved;
            bridge.moved = false;
        }
        let d = delta(world, e);
        if let Some(mut bridge) = world.get_mut::<ItemBridgeBuild>(e) {
            bridge.time_speed = approach(
                bridge.time_speed,
                if bridge.was_moved { 1.0 } else { 0.0 },
                1.0 / 60.0,
            );
            bridge.time += bridge.time_speed * d;
        }

        self.check_incoming(world, e);
        let link_tile = world
            .get::<ItemBridgeBuild>(e)
            .map(|b| b.link)
            .unwrap_or(-1);
        let other = tile_entity(world, link_tile);
        let valid = other.is_some_and(|other| self.link_valid(world, e, other, true));
        if let Some(mut bridge) = world.get_mut::<ItemBridgeBuild>(e) {
            bridge.had_valid_link = valid;
        }
        if !valid {
            self.do_dump(world, e);
            if let Some(mut bridge) = world.get_mut::<ItemBridgeBuild>(e) {
                bridge.warmup = 0.0;
            }
            return;
        }
        let Some(other) = other else {
            return;
        };
        let pos = world
            .get::<Building>(e)
            .map(|b| b.tile.pack())
            .unwrap_or(-1);
        if let Some(mut target) = world.get_mut::<ItemBridgeBuild>(other)
            && !target.incoming.contains(&pos)
        {
            target.incoming.push(pos);
        }
        let efficiency = world
            .get::<Building>(e)
            .map(|b| b.efficiency)
            .unwrap_or(0.0);
        if let Some(mut bridge) = world.get_mut::<ItemBridgeBuild>(e) {
            bridge.warmup = approach(bridge.warmup, efficiency, 1.0 / 30.0);
        }
        self.update_transport(world, e, other);
    }

    fn accept_item(&self, world: &World, e: Entity, source: Entity, _item: ItemId) -> bool {
        let table = world.get_resource::<BlockTable>();
        let has_items = world
            .get::<Building>(e)
            .and_then(|b| table.and_then(|t| t.get(b.block)))
            .is_some_and(|inst| inst.def.has_items);
        if !has_items {
            return false;
        }
        let same_team = world
            .get::<crate::entities::comp::TeamComp>(e)
            .zip(world.get::<crate::entities::comp::TeamComp>(source))
            .is_some_and(|(a, b)| a.team == b.team);
        if !same_team || transfer::item_total(world, e) >= transfer::item_capacity(world, e) {
            return false;
        }
        let enabled = world.get::<Building>(e).map(|b| b.enabled).unwrap_or(false);
        if self.no_accept_disabled && !enabled {
            return false;
        }
        let link_tile = world
            .get::<ItemBridgeBuild>(e)
            .map(|b| b.link)
            .unwrap_or(-1);
        self.check_accept(world, e, source, link_tile)
    }

    fn handle_item(&self, world: &mut World, e: Entity, _source: Entity, item: ItemId) {
        transfer::default_handle_item(world, e, e, item);
    }

    fn can_dump(&self, world: &World, e: Entity, to: Entity, _item: ItemId) -> bool {
        self.check_dump(world, e, to)
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        1
    }

    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        let bridge = world.get::<ItemBridgeBuild>(e).cloned().unwrap_or_default();
        w.i(bridge.link);
        w.f(bridge.warmup);
        w.b(bridge.incoming.len() as i8);
        for value in &bridge.incoming {
            w.i(*value);
        }
        w.bool(bridge.was_moved || bridge.moved);
        if let Some(buffer) = &bridge.buffer {
            buffer.write(w);
        }
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        revision: u8,
    ) {
        let mut bridge = world.get::<ItemBridgeBuild>(e).cloned().unwrap_or_default();
        let Ok(link) = r.i() else {
            return;
        };
        bridge.link = link;
        if let Ok(warmup) = r.f() {
            bridge.warmup = warmup;
        }
        if let Ok(count) = r.b() {
            bridge.incoming.clear();
            for _ in 0..count.max(0) {
                if let Ok(value) = r.i() {
                    bridge.incoming.push(value);
                }
            }
        }
        if revision >= 1
            && let Ok(moved) = r.bool()
        {
            bridge.was_moved = moved;
            bridge.moved = moved;
        }
        if let Some(buffer) = bridge.buffer.as_mut() {
            let _ = buffer.read(r);
        }
        world.entity_mut(e).insert(bridge);
    }
}

impl ItemBridgeBehavior {
    fn check_incoming(&self, world: &mut World, e: Entity) {
        let pos = world
            .get::<Building>(e)
            .map(|b| b.tile.pack())
            .unwrap_or(-1);
        let incoming = world
            .get::<ItemBridgeBuild>(e)
            .map(|b| b.incoming.clone())
            .unwrap_or_default();
        let mut kept: SmallVec<[i32; 4]> = SmallVec::new();
        for value in incoming {
            let valid = tile_entity(world, value).is_some_and(|other| {
                self.link_valid(world, e, other, false)
                    && world
                        .get::<ItemBridgeBuild>(other)
                        .is_some_and(|b| b.link == pos)
            });
            if valid {
                kept.push(value);
            }
        }
        if let Some(mut bridge) = world.get_mut::<ItemBridgeBuild>(e) {
            bridge.incoming = kept;
        }
    }

    fn do_dump(&self, world: &mut World, e: Entity) {
        if self.buffered {
            transfer::dump(world, e, None);
        } else {
            transfer::dump_accumulate(world, e, None);
        }
    }

    fn update_transport(&self, world: &mut World, e: Entity, other: Entity) {
        if self.buffered {
            self.update_transport_buffered(world, e, other);
            return;
        }
        let mut counter = world
            .get::<ItemBridgeBuild>(e)
            .map(|b| b.transport_counter)
            .unwrap_or(0.0)
            + edelta(world, e);
        while counter >= self.transport_time {
            let item = world
                .get_mut::<ItemModule>(e)
                .and_then(|mut items| items.take());
            if let Some(item) = item {
                if transfer::dispatch_accept_item(world, other, e, item) {
                    transfer::dispatch_handle_item(world, other, e, item);
                    if let Some(mut bridge) = world.get_mut::<ItemBridgeBuild>(e) {
                        bridge.moved = true;
                    }
                } else if let Some(mut items) = world.get_mut::<ItemModule>(e) {
                    items.add(item, 1, i32::MAX / 2);
                }
            }
            counter -= self.transport_time;
        }
        if let Some(mut bridge) = world.get_mut::<ItemBridgeBuild>(e) {
            bridge.transport_counter = counter;
        }
    }

    fn update_transport_buffered(&self, world: &mut World, e: Entity, other: Entity) {
        let now = build_time(world);
        let time_scale = world
            .get::<Building>(e)
            .map(|b| b.time_scale)
            .unwrap_or(1.0);
        // Fill the buffer.
        let accepts = world
            .get::<ItemBridgeBuild>(e)
            .and_then(|b| b.buffer.as_ref())
            .is_some_and(ItemBuffer::accepts);
        if accepts && transfer::item_total(world, e) > 0 {
            let item = world
                .get_mut::<ItemModule>(e)
                .and_then(|mut items| items.take());
            if let Some(item) = item
                && let Some(mut bridge) = world.get_mut::<ItemBridgeBuild>(e)
                && let Some(buffer) = bridge.buffer.as_mut()
            {
                buffer.accept(item, -1, now);
            }
        }
        let item = world
            .get::<ItemBridgeBuild>(e)
            .and_then(|b| b.buffer.as_ref())
            .and_then(|buffer| buffer.poll(self.speed / time_scale, now));
        if let Some(item) = item
            && run_timer(world, e, 1, 4.0 / time_scale)
            && transfer::dispatch_accept_item(world, other, e, item)
        {
            transfer::dispatch_handle_item(world, other, e, item);
            if let Some(mut bridge) = world.get_mut::<ItemBridgeBuild>(e) {
                bridge.moved = true;
                if let Some(buffer) = bridge.buffer.as_mut() {
                    buffer.remove();
                }
            }
        }
    }
}

fn approach(from: f32, to: f32, speed: f32) -> f32 {
    from + (to - from).clamp(-speed, speed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;

    #[test]
    fn positions_valid_matrix() {
        assert!(positions_valid(1, 1, 1, 5, 4));
        assert!(!positions_valid(1, 1, 1, 6, 4));
        assert!(positions_valid(1, 1, 5, 1, 4));
        assert!(!positions_valid(1, 1, 5, 2, 4));
    }

    #[test]
    fn phase_conveyor_config_roundtrip() {
        let mut harness = BuildHarness::new(32, 32, 7);
        let bridge = harness
            .content()
            .block_id("phase-conveyor")
            .expect("phase-conveyor");
        assert!(harness.place(5, 4, bridge, 0, true));
        assert!(harness.configure(5, 4, ConfigValue::Point2(3, 0)));
        let read = harness
            .build_at(5, 4)
            .map(|e| crate::world::config::read_config(&harness.world, e));
        assert_eq!(read, Some(ConfigValue::Point2(3, 0)));
    }

    #[test]
    fn buffered_bridge_delivers_across_a_gap() {
        let mut harness = BuildHarness::new(32, 32, 7);
        let copper = harness.content().item_id("copper").expect("copper");
        harness.register_behavior(
            "item-source",
            std::sync::Arc::new(crate::world::fixtures::logistics::SourceBehavior {
                item: copper,
                per_tick: 1,
            }),
        );
        let source = harness.content().block_id("item-source").expect("source");
        let bridge = harness
            .content()
            .block_id("bridge-conveyor")
            .expect("bridge");
        let belt = harness.content().block_id("conveyor").expect("conveyor");
        let container = harness.content().block_id("container").expect("container");
        assert!(harness.place(4, 4, source, 0, true));
        assert!(harness.place(5, 4, bridge, 0, true));
        assert!(harness.place(8, 4, bridge, 0, true));
        assert!(harness.place(9, 4, belt, 0, true));
        assert!(harness.place(10, 4, container, 0, true));
        // Only the source bridge is linked; the sink dumps.
        assert!(harness.configure(5, 4, ConfigValue::Point2(3, 0)));
        for _ in 0..900 {
            harness.tick();
        }
        let received = harness
            .build_at(10, 4)
            .and_then(|e| harness.world.get::<ItemModule>(e))
            .map(|items| items.total)
            .unwrap_or(0);
        assert!(received > 0, "container total={received}");
    }
}
