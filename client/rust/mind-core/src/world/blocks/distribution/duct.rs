// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Duct`/`ArmoredConveyor`-style duct behavior
//! (`world/blocks/distribution/Duct.java`) — plan 08 M1.
//!
//! Ports `updateTile` (progress reel + `moveForward`), `acceptItem`
//! (standard vs. armored acceptance), `handleItem`/`handleStack`/`removeStack`,
//! the `buildBlending` state and the rev 0/1 IO shape.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockId, BlockKind, ItemId};
use crate::entities::comp::Building;
use crate::world::behavior::BuildingBehavior;
use crate::world::block::BlockTable;
use crate::world::modules::ItemModule;
use crate::world::{TilePos, edelta, no_sleep};

use super::super::autotiler::{
    BlendNeighbor, BlendWorld, build_blending, looking_at, looking_at_either, point_equals,
    relative_to,
};
use super::transfer;
use crate::world::edges::facing_edge;

/// `Duct.DuctBuild` state.
#[derive(Debug, Clone, Component)]
pub struct DuctBuild {
    /// Movement progress (`progress`).
    pub progress: f32,
    /// Item currently on the duct (`current`).
    pub current: Option<ItemId>,
    /// Direction the current item came from (`recDir`).
    pub rec_dir: u8,
    /// Autotile blend case.
    pub blend_bits: u8,
    /// Autotile x scale.
    pub xscl: i8,
    /// Autotile y scale.
    pub yscl: i8,
    /// Autotile non-square blend mask.
    pub blending: u8,
    /// Cached front building.
    pub next: Option<Entity>,
    /// Cached back building.
    pub prev: Option<Entity>,
    /// Cached front duct.
    pub nextc: Option<Entity>,
    /// Front is missing/does not carry items (`capped`).
    pub capped: bool,
    /// No blend and back is missing/no items (`backCapped`).
    pub back_capped: bool,
}

impl Default for DuctBuild {
    fn default() -> Self {
        Self {
            progress: 0.0,
            current: None,
            rec_dir: 0,
            blend_bits: 0,
            xscl: 1,
            yscl: 1,
            blending: 0,
            next: None,
            prev: None,
            nextc: None,
            capped: false,
            back_capped: false,
        }
    }
}

fn is_duct_kind(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::Duct | BlockKind::DuctRouter | BlockKind::OverflowDuct | BlockKind::DuctBridge
    )
}

struct DuctBlendWorld<'a> {
    table: &'a BlockTable,
    armored: bool,
    dirs: [Option<BlendNeighbor>; 4],
}

impl DuctBlendWorld<'_> {
    fn has_items(&self, block: BlockId) -> bool {
        self.table.get(block).is_some_and(|inst| inst.def.has_items)
    }

    fn is_duct(&self, block: BlockId) -> bool {
        self.table
            .get(block)
            .is_some_and(|inst| is_duct_kind(inst.def.kind))
    }

    fn duct_blends_armored(
        &self,
        tile: TilePos,
        rotation: u8,
        other_x: i32,
        other_y: i32,
        other_rot: u8,
        other_block: BlockId,
    ) -> bool {
        let tile_x = tile.x() as i32;
        let tile_y = tile.y() as i32;
        let (dx, dy) = super::super::autotiler::d4(rotation);
        if point_equals(tile_x + dx, tile_y + dy, other_x, other_y) {
            return true;
        }
        let rotated = self.table.get(other_block).is_some_and(|inst| inst.rotate);
        if !rotated {
            let size = self
                .table
                .get(other_block)
                .map(|inst| inst.def.size)
                .unwrap_or(1)
                .max(1);
            let edge = facing_edge(size, other_x, other_y, tile);
            relative_to(edge.x() as i32, edge.y() as i32, tile_x, tile_y) == rotation as i8
        } else {
            let (ox, oy) = super::super::autotiler::d4(other_rot);
            point_equals(other_x + ox, other_y + oy, tile_x, tile_y) && self.is_duct(other_block)
        }
    }
}

impl BlendWorld for DuctBlendWorld<'_> {
    fn blends_block(
        &self,
        tile: TilePos,
        rotation: u8,
        other_x: i32,
        other_y: i32,
        other_rot: u8,
        other_block: BlockId,
    ) -> bool {
        let has_items = self.has_items(other_block);
        if !self.armored {
            return (has_items
                || (looking_at(self, tile, rotation, other_x, other_y, other_block) && has_items))
                && looking_at_either(
                    self,
                    tile,
                    rotation,
                    other_x,
                    other_y,
                    other_rot,
                    other_block,
                );
        }
        has_items
            && (self.duct_blends_armored(tile, rotation, other_x, other_y, other_rot, other_block)
                || looking_at(self, tile, rotation, other_x, other_y, other_block))
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

/// `Duct` behavior (one instance per speed/armor class).
#[derive(Debug, Clone, Copy)]
pub struct DuctBehavior {
    /// Duct speed (`Duct.speed`).
    pub speed: f32,
    /// Whether this is the armored variant.
    pub armored: bool,
}

impl DuctBehavior {
    /// `duct`.
    pub const NORMAL: DuctBehavior = DuctBehavior {
        speed: 4.0,
        armored: false,
    };
    /// `armored-duct`.
    pub const ARMORED: DuctBehavior = DuctBehavior {
        speed: 4.0,
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

impl BuildingBehavior for DuctBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<DuctBuild>(e).is_none() {
            world.entity_mut(e).insert(DuctBuild::default());
        }
    }

    fn always_update_when_disabled(&self) -> bool {
        true
    }

    fn on_proximity_update(&self, world: &mut World, e: Entity) {
        let Some(building) = world.get::<Building>(e) else {
            return;
        };
        let tile = building.tile;
        let rotation = building.rotation;
        let dirs = Self::neighbor_dirs(world, e);
        let table = world.get_resource::<BlockTable>().cloned();
        let (blend_bits, xscl, yscl, blending) = match &table {
            Some(table) => {
                let adapter = DuctBlendWorld {
                    table,
                    armored: self.armored,
                    dirs,
                };
                let bits = build_blending(&adapter, tile, rotation, &[None; 4], true);
                (bits[0] as u8, bits[1] as i8, bits[2] as i8, bits[4] as u8)
            }
            None => (0, 1, 1, 0),
        };
        let next = transfer::front(world, e);
        let nextc = next.filter(|n| world.get::<DuctBuild>(*n).is_some());
        let prev = transfer::back(world, e);
        let next_has_items = next
            .and_then(|n| world.get::<Building>(n))
            .and_then(|b| {
                table
                    .as_ref()
                    .and_then(|t| t.get(b.block))
                    .map(|inst| inst.def.has_items)
            })
            .unwrap_or(false);
        let prev_has_items = prev
            .and_then(|n| world.get::<Building>(n))
            .and_then(|b| {
                table
                    .as_ref()
                    .and_then(|t| t.get(b.block))
                    .map(|inst| inst.def.has_items)
            })
            .unwrap_or(false);
        let capped = next.is_none() || !next_has_items;
        let back_capped = blend_bits == 0 && (prev.is_none() || !prev_has_items);
        if let Some(mut duct) = world.get_mut::<DuctBuild>(e) {
            duct.blend_bits = blend_bits;
            duct.xscl = xscl;
            duct.yscl = yscl;
            duct.blending = blending;
            duct.next = next;
            duct.nextc = nextc;
            duct.prev = prev;
            duct.capped = capped;
            duct.back_capped = back_capped;
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let (has_current, has_next) = {
            let Some(duct) = world.get::<DuctBuild>(e) else {
                return;
            };
            (duct.current.is_some(), duct.next.is_some())
        };
        let progress = world.get::<DuctBuild>(e).map(|d| d.progress).unwrap_or(0.0);
        let bound = 1.0 - 1.0 / self.speed;
        let mut new_progress = progress + edelta(world, e) / self.speed * 2.0;

        if has_current && has_next {
            if new_progress >= bound {
                let item = world.get::<DuctBuild>(e).and_then(|d| d.current);
                if let Some(item) = item
                    && transfer::move_forward(world, e, item)
                {
                    if let Some(mut items) = world.get_mut::<ItemModule>(e) {
                        items.remove(item, 1);
                    }
                    if let Some(mut duct) = world.get_mut::<DuctBuild>(e) {
                        duct.current = None;
                    }
                    new_progress %= bound;
                }
            }
        } else {
            new_progress = 0.0;
        }

        let total = transfer::item_total(world, e);
        let first = if total > 0 {
            transfer::first_item(world, e)
        } else {
            None
        };
        if let Some(mut duct) = world.get_mut::<DuctBuild>(e) {
            duct.progress = new_progress;
            if duct.current.is_none() {
                duct.current = first;
            }
        }
    }

    fn accept_item(&self, world: &World, e: Entity, source: Entity, _item: ItemId) -> bool {
        let Some(duct) = world.get::<DuctBuild>(e) else {
            return false;
        };
        if duct.current.is_some() || transfer::item_total(world, e) != 0 {
            return false;
        }
        let rotation = match world.get::<Building>(e) {
            Some(building) => building.rotation,
            None => return false,
        };
        let table = world.get_resource::<BlockTable>();
        let is_duct = table
            .and_then(|t| world.get::<Building>(source).map(|b| (t, b.block)))
            .is_some_and(|(t, block)| t.get(block).is_some_and(|inst| is_duct_kind(inst.def.kind)));
        let has_items = table
            .and_then(|t| world.get::<Building>(source).map(|b| (t, b.block)))
            .is_some_and(|(t, block)| t.get(block).is_some_and(|inst| inst.def.has_items));
        let source_rotates = table
            .and_then(|t| world.get::<Building>(source).map(|b| (t, b.block)))
            .is_some_and(|(t, block)| t.get(block).is_some_and(|inst| inst.rotate));
        if self.armored {
            let source_front = transfer::front(world, source) == Some(e);
            let edge = transfer::relative_to_edge(world, e, source);
            (source_rotates && source_front && has_items && is_duct) || edge == rotation as i8
        } else {
            let not_front = !(source_rotates && duct.next == Some(source));
            let edge = transfer::relative_to_edge(world, e, source);
            not_front && edge >= 0 && (edge - rotation as i8).abs() != 2
        }
    }

    fn handle_item(&self, world: &mut World, e: Entity, source: Entity, item: ItemId) {
        let rec_dir = transfer::relative_to_edge(world, e, source).max(0) as u8;
        if let Some(mut items) = world.get_mut::<ItemModule>(e) {
            items.add(item, 1, i32::MAX / 2);
        }
        no_sleep(world, e);
        if let Some(mut duct) = world.get_mut::<DuctBuild>(e) {
            duct.current = Some(item);
            duct.progress = -1.0;
            duct.rec_dir = rec_dir;
        }
    }

    fn handle_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) {
        transfer::default_handle_stack(world, e, item, amount);
        if let Some(mut duct) = world.get_mut::<DuctBuild>(e) {
            duct.current = Some(item);
        }
    }

    fn remove_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) -> i32 {
        let removed = transfer::default_remove_stack(world, e, item, amount);
        if removed > 0
            && let Some(mut duct) = world.get_mut::<DuctBuild>(e)
            && duct.current == Some(item)
        {
            duct.current = None;
        }
        removed
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        1
    }

    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        let rec_dir = world
            .get::<DuctBuild>(e)
            .map(|duct| duct.rec_dir)
            .unwrap_or(0);
        w.b(rec_dir as i8);
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        revision: u8,
    ) {
        let mut duct = world.get::<DuctBuild>(e).cloned().unwrap_or_default();
        if revision >= 1
            && let Ok(rec_dir) = r.b()
        {
            duct.rec_dir = rec_dir as u8;
        }
        duct.current = transfer::first_item(world, e);
        world.entity_mut(e).insert(duct);
    }
}

/// `Edges`/`point_equals` helper re-export used by the blend tests.
#[allow(dead_code)]
fn _assert_helpers() {
    let _ = point_equals(0, 0, 0, 0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;

    #[test]
    fn duct_lane_delivers_to_container() {
        let mut harness = BuildHarness::new(16, 16, 7);
        let copper = harness.content().item_id("copper").expect("copper");
        harness.register_behavior(
            "item-source",
            std::sync::Arc::new(crate::world::fixtures::logistics::SourceBehavior {
                item: copper,
                per_tick: 1,
            }),
        );
        let source = harness.content().block_id("item-source").expect("source");
        let duct = harness.content().block_id("duct").expect("duct");
        let container = harness.content().block_id("container").expect("container");
        assert!(harness.place(4, 4, source, 0, true));
        for x in 5..=8 {
            assert!(harness.place(x, 4, duct, 0, true), "duct {x}");
        }
        assert!(harness.place(9, 4, container, 0, true));
        for _ in 0..600 {
            harness.tick();
        }
        let total = harness
            .build_at(9, 4)
            .and_then(|e| harness.world.get::<ItemModule>(e))
            .map(|items| items.total)
            .unwrap_or(0);
        assert!(total > 0, "container total={total}");
    }

    #[test]
    fn armored_duct_accepts_from_a_duct_source() {
        let mut harness = BuildHarness::new(16, 16, 7);
        let copper = harness.content().item_id("copper").expect("copper");
        let duct_id = harness.content().block_id("duct").expect("duct");
        let armored = harness
            .content()
            .block_id("armored-duct")
            .expect("armored-duct");
        assert!(harness.place(4, 4, duct_id, 0, true));
        assert!(harness.place(5, 4, armored, 0, true));
        let source_duct = harness.build_at(4, 4).expect("source duct");
        let armored_build = harness.build_at(5, 4).expect("armored duct");
        assert!(
            transfer::dispatch_accept_item(&harness.world, armored_build, source_duct, copper),
            "armored duct should accept from a duct source"
        );
    }
}
