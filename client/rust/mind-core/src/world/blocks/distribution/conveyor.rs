// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Conveyor`/`ArmoredConveyor` build behavior
//! (`world/blocks/distribution/Conveyor.java`) — plan 08 M0/M1.
//!
//! Fixed `[3]` belt arrays, the Java-exact motion loop (`updateTile`),
//! `acceptItem`/`handleItem`/`acceptStack`/`handleStack`/`removeStack`/`pass`,
//! `overwrote` and the rev 0/1 IO shapes.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockId, ItemId};
use crate::entities::comp::Building;
use crate::world::behavior::BuildingBehavior;
use crate::world::block::BlockTable;
use crate::world::modules::ItemModule;
use crate::world::{TilePos, no_sleep, sleep};

use super::super::autotiler::{
    BlendNeighbor, BlendWorld, blends_armored, build_blending, looking_at, looking_at_either,
    relative_to,
};
use super::transfer;

/// Belt capacity (`Conveyor.capacity`).
pub const CAPACITY: usize = 3;
/// Belt item spacing in tiles (`Conveyor.itemSpace`).
pub const ITEM_SPACE: f32 = 0.4;

/// `Conveyor.ConveyorBuild` parallel-array state.
#[derive(Debug, Clone, Component)]
pub struct ConveyorBuild {
    /// Item ids per slot.
    pub ids: [ItemId; CAPACITY],
    /// Lateral offset per slot.
    pub xs: [f32; CAPACITY],
    /// Longitudinal position per slot (`0..1`).
    pub ys: [f32; CAPACITY],
    /// Number of items (`len`).
    pub len: u8,
    /// Cached front building.
    pub next: Option<Entity>,
    /// Cached front building when it is a same-team conveyor.
    pub nextc: Option<Entity>,
    /// Whether `nextc.rotation == rotation`.
    pub aligned: bool,
    /// Last insertion slot.
    pub last_inserted: u8,
    /// Middle insertion slot.
    pub mid: u8,
    /// Minimum belt position this tick.
    pub minitem: f32,
    /// Clog heat (`0..1`).
    pub clog_heat: f32,
    /// Autotile blend case (`blendbits`).
    pub blend_bits: u8,
    /// Autotile non-square blend mask (`blending`).
    pub blending: u8,
    /// Autotile x scale.
    pub blend_sclx: i8,
    /// Autotile y scale.
    pub blend_scly: i8,
}

impl Default for ConveyorBuild {
    fn default() -> Self {
        Self {
            ids: [ItemId::new(0); CAPACITY],
            xs: [0.0; CAPACITY],
            ys: [0.0; CAPACITY],
            len: 0,
            next: None,
            nextc: None,
            aligned: false,
            last_inserted: 0,
            mid: 0,
            minitem: 1.0,
            clog_heat: 0.0,
            blend_bits: 0,
            blending: 0,
            blend_sclx: 1,
            blend_scly: 1,
        }
    }
}

impl ConveyorBuild {
    /// `ConveyorBuild.add(o)` — shifts slots up and grows `len`.
    pub fn add(&mut self, o: usize) {
        let start = (o + 1).max(self.len as usize);
        let mut i = start.min(CAPACITY - 1);
        while i > o {
            self.ids[i] = self.ids[i - 1];
            self.xs[i] = self.xs[i - 1];
            self.ys[i] = self.ys[i - 1];
            i -= 1;
        }
        self.len = (self.len + 1).min(CAPACITY as u8);
    }

    /// `ConveyorBuild.remove(o)` — shifts slots down and shrinks `len`.
    pub fn remove(&mut self, o: usize) {
        let len = self.len as usize;
        let mut i = o;
        while i + 1 < len && i + 1 < CAPACITY {
            self.ids[i] = self.ids[i + 1];
            self.xs[i] = self.xs[i + 1];
            self.ys[i] = self.ys[i + 1];
            i += 1;
        }
        if self.len > 0 {
            self.len -= 1;
        }
    }
}

/// Autotiler host reading block metadata.
struct ConveyorBlendWorld<'a> {
    table: &'a BlockTable,
    armored: bool,
    dirs: [Option<BlendNeighbor>; 4],
}

impl BlendWorld for ConveyorBlendWorld<'_> {
    fn blends_block(
        &self,
        source: TilePos,
        rotation: u8,
        other_x: i32,
        other_y: i32,
        other_rot: u8,
        other_block: BlockId,
    ) -> bool {
        let Some(has_items) = self.table.get(other_block).map(|inst| inst.def.has_items) else {
            return false;
        };
        if self.armored {
            return blends_armored(
                self,
                source,
                rotation,
                other_x,
                other_y,
                other_rot,
                other_block,
            );
        }
        (has_items
            || (looking_at(self, source, rotation, other_x, other_y, other_block) && has_items))
            && looking_at_either(
                self,
                source,
                rotation,
                other_x,
                other_y,
                other_rot,
                other_block,
            )
    }

    fn near_build(&self, direction: u8, _source: TilePos) -> Option<BlendNeighbor> {
        self.dirs.get(direction as usize).copied().flatten()
    }

    fn square_sprite(&self, block: BlockId) -> bool {
        self.table
            .get(block)
            .is_some_and(|inst| inst.def.square_sprite)
    }

    fn rotated_output(&self, block: BlockId) -> bool {
        self.table.get(block).is_some_and(|inst| inst.rotate)
    }

    fn block_size(&self, block: BlockId) -> i32 {
        self.table.get(block).map(|inst| inst.def.size).unwrap_or(1)
    }
}

/// `Conveyor`/`ArmoredConveyor` behavior (one instance per speed class).
#[derive(Debug, Clone, Copy)]
pub struct ConveyorBehavior {
    /// Belt speed in tiles/tick (`Conveyor.speed`).
    pub speed: f32,
    /// Displayed speed (`Conveyor.displayedSpeed`).
    pub displayed_speed: f32,
    /// Whether this is the armored variant (`ArmoredConveyor`).
    pub armored: bool,
}

impl ConveyorBehavior {
    /// Base `conveyor` behavior.
    pub const BASE: ConveyorBehavior = ConveyorBehavior {
        speed: 0.035,
        displayed_speed: 5.0,
        armored: false,
    };
    /// `titanium-conveyor`.
    pub const TITANIUM: ConveyorBehavior = ConveyorBehavior {
        speed: 0.0801,
        displayed_speed: 10.0,
        armored: false,
    };
    /// `armored-conveyor`.
    pub const ARMORED: ConveyorBehavior = ConveyorBehavior {
        speed: 0.08,
        displayed_speed: 10.0,
        armored: true,
    };

    fn neighbor_dirs(world: &World, e: Entity) -> [Option<BlendNeighbor>; 4] {
        let mut dirs = [None; 4];
        let Some(building) = world.get::<Building>(e) else {
            return dirs;
        };
        let source = building.tile;
        for other in &building.proximity {
            let Some(other_building) = world.get::<Building>(*other) else {
                continue;
            };
            let real = relative_to(
                source.x() as i32,
                source.y() as i32,
                other_building.tile.x() as i32,
                other_building.tile.y() as i32,
            );
            if (0..4).contains(&real) {
                dirs[real as usize] = Some(BlendNeighbor {
                    x: other_building.tile.x() as i32,
                    y: other_building.tile.y() as i32,
                    rotation: other_building.rotation,
                    block: other_building.block,
                });
            }
        }
        dirs
    }
}

impl BuildingBehavior for ConveyorBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<ConveyorBuild>(e).is_none() {
            world.entity_mut(e).insert(ConveyorBuild::default());
        }
    }

    fn on_proximity_update(&self, world: &mut World, e: Entity) {
        let Some(building) = world.get::<Building>(e) else {
            return;
        };
        let tile = building.tile;
        let rotation = building.rotation;
        let dirs = Self::neighbor_dirs(world, e);
        let table = world.get_resource::<BlockTable>().cloned();
        let (blend_bits, blending, sclx, scly) = match &table {
            Some(table) => {
                let adapter = ConveyorBlendWorld {
                    table,
                    armored: self.armored,
                    dirs,
                };
                let bits = build_blending(&adapter, tile, rotation, &[None; 4], true);
                (bits[3] as u8, bits[4] as u8, bits[1] as i8, bits[2] as i8)
            }
            None => (0, 0, 1, 1),
        };
        let next = transfer::front(world, e);
        let nextc = next.filter(|next| {
            world.get::<ConveyorBuild>(*next).is_some()
                && world
                    .get::<crate::entities::comp::TeamComp>(*next)
                    .zip(world.get::<crate::entities::comp::TeamComp>(e))
                    .is_some_and(|(a, b)| a.team == b.team)
        });
        let aligned = nextc
            .and_then(|next| world.get::<Building>(next))
            .is_some_and(|next_building| next_building.rotation == rotation);
        if let Some(mut belt) = world.get_mut::<ConveyorBuild>(e) {
            belt.next = next;
            belt.nextc = nextc;
            belt.aligned = aligned;
            belt.blend_bits = blend_bits;
            belt.blending = blending;
            belt.blend_sclx = sclx;
            belt.blend_scly = scly;
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        // Copy the belt state into stack locals, run the whole motion loop, and
        // write it back once: the Java loop touches the belt 5-6 times per tick;
        // one ECS access instead of one-per-item is the §7.4 hot-path win. The
        // observable state transition is identical (`pass` never mutates `e`).
        let Some(mut belt) = world.get::<ConveyorBuild>(e).cloned() else {
            return;
        };
        let next = belt.next;
        let nextc = belt.nextc;
        let aligned = belt.aligned;
        let (next_minitem, next_insert) =
            match nextc.and_then(|next| world.get::<ConveyorBuild>(next)) {
                Some(next_belt) => (next_belt.minitem, next_belt.last_inserted),
                None => (1.0, 0),
            };

        let (time_scale, efficiency) = world
            .get::<Building>(e)
            .map(|b| (b.time_scale, b.efficiency))
            .unwrap_or((1.0, 0.0));
        if belt.len == 0 && (time_scale - 1.0).abs() <= 1e-6 {
            belt.clog_heat = 0.0;
            if let Some(mut current) = world.get_mut::<ConveyorBuild>(e) {
                *current = belt;
            }
            sleep(world, e);
            return;
        }

        let next_max = if aligned {
            1.0 - (ITEM_SPACE - next_minitem).max(0.0)
        } else {
            1.0
        };
        let moved = self.speed * efficiency * time_scale;
        let original_len = belt.len as usize;
        let mut minitem = 1.0f32;
        let mut mid = 0u8;
        let mut current_len = belt.len as i32;

        for i in (0..original_len).rev() {
            let previous = if i == original_len - 1 {
                100.0
            } else {
                belt.ys[i + 1]
            };
            let nextpos = previous - ITEM_SPACE;
            let maxmove = (nextpos - belt.ys[i]).clamp(0.0, moved);
            belt.ys[i] += maxmove;
            if belt.ys[i] > next_max {
                belt.ys[i] = next_max;
            }
            if belt.ys[i] > 0.5 && i > 0 {
                mid = (i - 1) as u8;
            }
            belt.xs[i] = approach(belt.xs[i], 0.0, moved * 2.0);
            let item = belt.ids[i];

            if belt.ys[i] >= 1.0 && pass(world, e, next, item) {
                if aligned
                    && let Some(next) = nextc
                    && let Some(mut next_belt) = world.get_mut::<ConveyorBuild>(next)
                {
                    next_belt.xs[next_insert as usize] = belt.xs[i];
                }
                if let Some(mut items) = world.get_mut::<ItemModule>(e) {
                    items.remove(item, (current_len - i as i32).max(0));
                }
                belt.remove(i);
                belt.len = (i as u8).min(belt.len);
                current_len = belt.len as i32;
            } else if belt.ys[i] < minitem {
                minitem = belt.ys[i];
            }
        }

        belt.mid = mid;
        belt.minitem = minitem;
        let threshold = ITEM_SPACE + if belt.blend_bits == 1 { 0.3 } else { 0.0 };
        belt.clog_heat = if minitem < threshold {
            approach(belt.clog_heat, 1.0, 1.0 / 60.0)
        } else {
            0.0
        };
        if let Some(mut current) = world.get_mut::<ConveyorBuild>(e) {
            *current = belt;
        }

        no_sleep(world, e);
    }

    fn accept_item(&self, world: &World, e: Entity, source: Entity, _item: ItemId) -> bool {
        let (len, minitem, rotation, next) = {
            let Some(belt) = world.get::<ConveyorBuild>(e) else {
                return false;
            };
            let Some(building) = world.get::<Building>(e) else {
                return false;
            };
            (belt.len, belt.minitem, building.rotation, belt.next)
        };
        if len as usize >= CAPACITY {
            return false;
        }
        let (Some(source_tile), Some(tile)) = (
            world.get::<Building>(source).map(|b| b.tile),
            world.get::<Building>(e).map(|b| b.tile),
        ) else {
            return false;
        };
        let rel = relative_to(
            source_tile.x() as i32,
            source_tile.y() as i32,
            tile.x() as i32,
            tile.y() as i32,
        );
        if rel < 0 {
            return false;
        }
        let direction = (rel - rotation as i8).abs();
        let facing_ok =
            (direction == 0 && minitem >= ITEM_SPACE) || (direction % 2 == 1 && minitem > 0.7);
        let source_rotates = world
            .get::<Building>(source)
            .and_then(|b| {
                world
                    .get_resource::<BlockTable>()
                    .and_then(|t| t.get(b.block))
            })
            .is_some_and(|inst| inst.rotate);
        facing_ok && !(source_rotates && next == Some(source))
    }

    fn handle_item(&self, world: &mut World, e: Entity, source: Entity, item: ItemId) {
        let (len, mid, rotation) = {
            let Some(belt) = world.get::<ConveyorBuild>(e) else {
                return;
            };
            let Some(building) = world.get::<Building>(e) else {
                return;
            };
            (belt.len, belt.mid as usize, building.rotation)
        };
        if len as usize >= CAPACITY {
            return;
        }
        let (Some(source_tile), Some(tile)) = (
            world.get::<Building>(source).map(|b| b.tile),
            world.get::<Building>(e).map(|b| b.tile),
        ) else {
            return;
        };
        let rel = relative_to(
            source_tile.x() as i32,
            source_tile.y() as i32,
            tile.x() as i32,
            tile.y() as i32,
        );
        let ang = rel - rotation as i8;
        let x = if ang == -1 || ang == 3 {
            1.0
        } else if ang == 1 || ang == -3 {
            -1.0
        } else {
            0.0
        };
        no_sleep(world, e);
        if let Some(mut items) = world.get_mut::<ItemModule>(e) {
            items.add(item, 1, CAPACITY as i32);
        }
        if let Some(mut belt) = world.get_mut::<ConveyorBuild>(e) {
            if ang == 0 {
                belt.add(0);
                belt.xs[0] = x;
                belt.ys[0] = 0.0;
                belt.ids[0] = item;
            } else {
                let slot = mid.min(CAPACITY - 1);
                belt.add(slot);
                belt.xs[slot] = x;
                belt.ys[slot] = 0.5;
                belt.ids[slot] = item;
            }
        }
    }

    fn accept_stack(
        &self,
        world: &World,
        e: Entity,
        _item: ItemId,
        amount: i32,
        _source: Option<Entity>,
    ) -> i32 {
        let minitem = world
            .get::<ConveyorBuild>(e)
            .map(|belt| belt.minitem)
            .unwrap_or(0.0);
        ((minitem / ITEM_SPACE) as i32).min(amount)
    }

    fn handle_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) {
        let len = world
            .get::<ConveyorBuild>(e)
            .map(|belt| belt.len as i32)
            .unwrap_or(0);
        let amount = amount.min(CAPACITY as i32 - len);
        if amount <= 0 {
            return;
        }
        let mut i = amount - 1;
        while i >= 0 {
            {
                let Some(mut belt) = world.get_mut::<ConveyorBuild>(e) else {
                    break;
                };
                belt.add(0);
                belt.xs[0] = 0.0;
                belt.ys[0] = i as f32 * ITEM_SPACE;
                belt.ids[0] = item;
            }
            if let Some(mut items) = world.get_mut::<ItemModule>(e) {
                items.add(item, 1, i32::MAX / 2);
            }
            i -= 1;
        }
        no_sleep(world, e);
    }

    fn remove_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) -> i32 {
        no_sleep(world, e);
        let mut removed = 0;
        for _ in 0..amount {
            let Some(index) = world
                .get::<ConveyorBuild>(e)
                .and_then(|belt| (0..belt.len as usize).find(|i| belt.ids[*i] == item))
            else {
                break;
            };
            if let Some(mut belt) = world.get_mut::<ConveyorBuild>(e) {
                belt.remove(index);
            }
            removed += 1;
        }
        if let Some(mut items) = world.get_mut::<ItemModule>(e) {
            items.remove(item, removed);
        }
        removed
    }

    fn overwrote(&self, world: &mut World, e: Entity, previous: &[Entity]) {
        let Some(prev) = previous.first().copied() else {
            return;
        };
        let Some(prev_belt) = world.get::<ConveyorBuild>(prev).cloned() else {
            return;
        };
        if let Some(mut belt) = world.get_mut::<ConveyorBuild>(e) {
            belt.ids = prev_belt.ids;
            belt.xs = prev_belt.xs;
            belt.ys = prev_belt.ys;
            belt.len = prev_belt.len;
            belt.clog_heat = prev_belt.clog_heat;
            belt.last_inserted = prev_belt.last_inserted;
            belt.mid = prev_belt.mid;
            belt.minitem = prev_belt.minitem;
        }
        let prev_items: Vec<(ItemId, i32)> = world
            .get::<ItemModule>(prev)
            .map(|items| items.stacks().collect())
            .unwrap_or_default();
        if let Some(mut items) = world.get_mut::<ItemModule>(e) {
            for (item, amount) in prev_items {
                items.add(item, amount, i32::MAX / 2);
            }
        }
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        1
    }

    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        let Some(belt) = world.get::<ConveyorBuild>(e) else {
            w.i(0);
            return;
        };
        w.i(belt.len as i32);
        for i in 0..belt.len as usize {
            w.s(belt.ids[i].raw() as i16);
            w.b((belt.xs[i] * 127.0) as i8);
            w.b((belt.ys[i] * 255.0 - 128.0) as i8);
        }
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        revision: u8,
    ) {
        let Ok(amount) = r.i() else {
            return;
        };
        let mut belt = world.get::<ConveyorBuild>(e).cloned().unwrap_or_default();
        belt.len = 0;
        for i in 0..amount.max(0) {
            let (id, x, y) = if revision == 0 {
                let Ok(val) = r.i() else {
                    break;
                };
                let id = ((val >> 24) as u8) as u16;
                let x = ((val >> 16) as i8) as f32 / 127.0;
                let y = (((val >> 8) as i8) as f32 + 128.0) / 255.0;
                (id, x, y)
            } else {
                let (Ok(id), Ok(x), Ok(y)) = (r.s(), r.b(), r.b()) else {
                    break;
                };
                (id as u16, x as f32 / 127.0, (y as f32 + 128.0) / 255.0)
            };
            if (i as usize) < CAPACITY {
                belt.ids[i as usize] = ItemId::new(id);
                belt.xs[i as usize] = x;
                belt.ys[i as usize] = y;
                belt.len += 1;
            }
        }
        world.entity_mut(e).insert(belt);
    }
}

/// `ConveyorBuild.pass(item)`.
fn pass(world: &mut World, e: Entity, next: Option<Entity>, item: ItemId) -> bool {
    let Some(next) = next else {
        return false;
    };
    let same_team = world
        .get::<crate::entities::comp::TeamComp>(e)
        .zip(world.get::<crate::entities::comp::TeamComp>(next))
        .is_some_and(|(a, b)| a.team == b.team);
    if !same_team {
        return false;
    }
    if transfer::dispatch_accept_item(world, next, e, item) {
        transfer::dispatch_handle_item(world, next, e, item);
        true
    } else {
        false
    }
}

/// Linear approach (`Mathf.approach`).
pub(crate) fn approach(from: f32, to: f32, speed: f32) -> f32 {
    from + (to - from).clamp(-speed, speed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::world::block::BlockTable;
    use crate::world::limits::BuildRules;
    use bevy_ecs::world::World as EcsWorld;

    fn spawn_belt(
        world: &mut EcsWorld,
        table: &BlockTable,
        content: &crate::content::ContentRegistry,
        name: &str,
        x: i16,
        y: i16,
        seq: u64,
    ) -> Entity {
        let inst = table.get_named(name).expect(name).clone();
        inst.spawn(
            world,
            seq,
            TilePos::new(x, y),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        )
    }

    #[test]
    fn belt_moves_item_at_exact_speed() {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let mut world = EcsWorld::new();
        world.insert_resource(BuildRules::default());
        world.insert_resource(table.clone());
        let belt = spawn_belt(&mut world, &table, &content, "conveyor", 4, 4, 0);
        let copper = ItemId::new(0);
        {
            let mut belt_c = world.get_mut::<ConveyorBuild>(belt).expect("belt");
            belt_c.add(0);
            belt_c.ids[0] = copper;
            belt_c.xs[0] = 0.0;
            belt_c.ys[0] = 0.0;
        }
        world
            .get_mut::<ItemModule>(belt)
            .expect("items")
            .add(copper, 1, 10);
        let inst = table.get_named("conveyor").expect("inst").clone();
        inst.behavior.update_tile(&mut world, belt);
        let after = world.get::<ConveyorBuild>(belt).expect("belt").ys[0];
        assert!((after - 0.035).abs() < 1e-6, "moved {after}");
    }

    #[test]
    fn adjacent_belts_pass_isolated() {
        let mut harness = crate::world::BuildHarness::new(16, 16, 7);
        let copper = harness.content().item_id("copper").expect("copper");
        let conveyor = harness.content().block_id("conveyor").expect("conveyor");
        assert!(harness.place(5, 4, conveyor, 0, true));
        assert!(harness.place(6, 4, conveyor, 0, true));
        let first = harness.build_at(5, 4).expect("first");
        let second = harness.build_at(6, 4).expect("second");
        {
            let mut b = harness.world.get_mut::<ConveyorBuild>(first).expect("belt");
            b.add(0);
            b.ids[0] = copper;
            b.ys[0] = 1.0;
        }
        harness
            .world
            .get_mut::<ItemModule>(first)
            .expect("items")
            .add(copper, 1, 10);
        let next = harness
            .world
            .get::<ConveyorBuild>(first)
            .and_then(|b| b.next);
        let nextc = harness
            .world
            .get::<ConveyorBuild>(first)
            .and_then(|b| b.nextc);
        let aligned = harness
            .world
            .get::<ConveyorBuild>(first)
            .map(|b| b.aligned)
            .unwrap_or(false);
        let accepts = next
            .map(|n| transfer::dispatch_accept_item(&harness.world, n, first, copper))
            .unwrap_or(false);
        crate::world::update::building_update(&mut harness.world, first);
        let second_len = harness
            .world
            .get::<ConveyorBuild>(second)
            .map(|b| b.len)
            .unwrap_or(9);
        println!(
            "next={next:?} nextc={nextc:?} aligned={aligned} accepts={accepts} second_len={second_len}"
        );
        assert_eq!(second_len, 1);
    }

    #[test]
    fn terminal_passes_to_container_isolated() {
        let mut harness = crate::world::BuildHarness::new(16, 16, 7);
        let copper = harness.content().item_id("copper").expect("copper");
        let conveyor = harness.content().block_id("conveyor").expect("conveyor");
        let container = harness.content().block_id("container").expect("container");
        assert!(harness.place(5, 4, conveyor, 0, true));
        assert!(harness.place(6, 4, container, 0, true));
        let belt = harness.build_at(5, 4).expect("belt");
        let sink = harness.build_at(6, 4).expect("sink");
        {
            let mut b = harness.world.get_mut::<ConveyorBuild>(belt).expect("belt");
            b.add(0);
            b.ids[0] = copper;
            b.ys[0] = 1.0;
        }
        harness
            .world
            .get_mut::<ItemModule>(belt)
            .expect("items")
            .add(copper, 1, 10);
        let next = harness
            .world
            .get::<ConveyorBuild>(belt)
            .and_then(|b| b.next);
        let accepts = next
            .map(|n| transfer::dispatch_accept_item(&harness.world, n, belt, copper))
            .unwrap_or(false);
        crate::world::update::building_update(&mut harness.world, belt);
        let sink_total = harness
            .world
            .get::<ItemModule>(sink)
            .map(|m| m.total)
            .unwrap_or(-1);
        let belt_len = harness
            .world
            .get::<ConveyorBuild>(belt)
            .map(|b| b.len)
            .unwrap_or(9);
        println!("next={next:?} accepts={accepts} sink_total={sink_total} belt_len={belt_len}");
        assert_eq!(sink_total, 1);
    }

    #[test]
    fn harness_chain_delivers_to_container() {
        let mut harness = crate::world::BuildHarness::new(16, 16, 7);
        let copper = harness.content().item_id("copper").expect("copper");
        harness.register_behavior(
            "item-source",
            std::sync::Arc::new(crate::world::fixtures::logistics::SourceBehavior {
                item: copper,
                per_tick: 1,
            }),
        );
        let source = harness.content().block_id("item-source").expect("source");
        let conveyor = harness.content().block_id("conveyor").expect("conveyor");
        let container = harness.content().block_id("container").expect("container");
        assert!(harness.place(4, 4, source, 0, true));
        for x in 5..=9 {
            assert!(harness.place(x, 4, conveyor, 0, true), "belt {x}");
        }
        assert!(harness.place(10, 4, container, 0, true));
        // ~6 tiles at 0.035 tiles/tick needs ~172 ticks; allow margin.
        for _ in 0..400 {
            harness.tick();
        }
        let sink_total = harness
            .build_at(10, 4)
            .and_then(|e| harness.world.get::<ItemModule>(e))
            .map(|items| items.total)
            .unwrap_or(0);
        assert!(sink_total > 0, "sink_total={sink_total}");
    }

    #[test]
    fn titanium_belt_speed() {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let mut world = EcsWorld::new();
        world.insert_resource(BuildRules::default());
        world.insert_resource(table.clone());
        let belt = spawn_belt(&mut world, &table, &content, "titanium-conveyor", 4, 4, 0);
        {
            let mut belt_c = world.get_mut::<ConveyorBuild>(belt).expect("belt");
            belt_c.add(0);
            belt_c.ids[0] = ItemId::new(0);
            belt_c.ys[0] = 0.0;
        }
        crate::world::update::building_update(&mut world, belt);
        assert!((world.get::<ConveyorBuild>(belt).expect("belt").ys[0] - 0.0801).abs() < 1e-5);
    }
}
