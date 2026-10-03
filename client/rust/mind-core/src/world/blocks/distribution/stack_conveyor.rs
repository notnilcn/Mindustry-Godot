// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `StackConveyor` build behavior (`world/blocks/distribution/StackConveyor.java`)
//! — plan 08 M1.
//!
//! The Move/Load/Unload state machine (recomputed in `onProximityUpdate` from
//! `buildBlending` and neighbor states, including the mutual `proxUpdating`
//! recursion guard), the `cooldown` reel, whole-stack transfer to a front
//! `StackConveyorBuild` and batch unload while enabled.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockId, BlockKind, ItemId};
use crate::entities::comp::Building;
use crate::world::behavior::BuildingBehavior;
use crate::world::block::BlockTable;
use crate::world::modules::ItemModule;
use crate::world::{TilePos, delta, edelta, no_sleep};

use super::super::autotiler::{
    BlendNeighbor, BlendWorld, blends_armored, build_blending, relative_to,
};
use super::transfer;

/// `stateMove`.
pub const STATE_MOVE: u8 = 0;
/// `stateLoad`.
pub const STATE_LOAD: u8 = 1;
/// `stateUnload`.
pub const STATE_UNLOAD: u8 = 2;

/// `StackConveyor.StackConveyorBuild` state.
#[derive(Debug, Clone, Component)]
pub struct StackConveyorBuild {
    /// Current state (`state`).
    pub state: u8,
    /// Blend bitmask (view).
    pub blendprox: u8,
    /// Packed source tile pos or `-1` (`link`).
    pub link: i32,
    /// Reel cooldown (`cooldown`).
    pub cooldown: f32,
    /// Current item (`lastItem`).
    pub last_item: Option<ItemId>,
    /// Recursion guard (`proxUpdating`).
    pub prox_updating: bool,
}

impl Default for StackConveyorBuild {
    fn default() -> Self {
        Self {
            state: STATE_MOVE,
            blendprox: 0,
            link: -1,
            cooldown: 0.0,
            last_item: None,
            prox_updating: false,
        }
    }
}

fn is_stack_conveyor(block: BlockId, table: &BlockTable) -> bool {
    table
        .get(block)
        .is_some_and(|inst| inst.def.kind == BlockKind::StackConveyor)
}

struct StackBlendWorld<'a> {
    table: &'a BlockTable,
    dirs: [Option<BlendNeighbor>; 4],
}

impl BlendWorld for StackBlendWorld<'_> {
    fn blends_block(
        &self,
        _tile: TilePos,
        rotation: u8,
        other_x: i32,
        other_y: i32,
        other_rot: u8,
        other_block: BlockId,
    ) -> bool {
        let has_items = self
            .table
            .get(other_block)
            .is_some_and(|inst| inst.def.has_items);
        has_items
            && is_stack_conveyor(other_block, self.table)
            && blends_armored(
                self,
                _tile,
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

/// `StackConveyor` behavior (`plastanium-conveyor`, `surge-conveyor`).
#[derive(Debug, Clone, Copy)]
pub struct StackConveyorBehavior {
    /// Speed (`StackConveyor.speed`).
    pub speed: f32,
    /// Base efficiency (`surge-conveyor` = 1).
    pub base_efficiency: f32,
    /// Whether unloading uses routers (`StackConveyor.outputRouter`).
    pub output_router: bool,
    /// Recharge (`StackConveyor.recharge`).
    pub recharge: f32,
}

impl StackConveyorBehavior {
    /// `plastanium-conveyor`.
    pub const PLASTANIUM: StackConveyorBehavior = StackConveyorBehavior {
        speed: 4.0 / 60.0,
        base_efficiency: 0.0,
        output_router: true,
        recharge: 2.0,
    };
    /// `surge-conveyor`.
    pub const SURGE: StackConveyorBehavior = StackConveyorBehavior {
        speed: 5.0 / 60.0,
        base_efficiency: 1.0,
        output_router: false,
        recharge: 2.0,
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

    fn blend_bits(world: &World, e: Entity) -> [i32; 5] {
        let Some(building) = world.get::<Building>(e) else {
            return [0; 5];
        };
        let dirs = Self::neighbor_dirs(world, e);
        let Some(table) = world.get_resource::<BlockTable>().cloned() else {
            return [0; 5];
        };
        let adapter = StackBlendWorld {
            table: &table,
            dirs,
        };
        build_blending(&adapter, building.tile, building.rotation, &[None; 4], true)
    }
}

impl BuildingBehavior for StackConveyorBehavior {
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
        if world.get::<StackConveyorBuild>(e).is_none() {
            world.entity_mut(e).insert(StackConveyorBuild::default());
        }
    }

    fn on_proximity_update(&self, world: &mut World, e: Entity) {
        let last_state = world
            .get::<StackConveyorBuild>(e)
            .map(|b| b.state)
            .unwrap_or(STATE_MOVE);
        let front = transfer::front(world, e);
        let back = transfer::back(world, e);

        let bits = Self::blend_bits(world, e);
        let case0 = bits[0] == 0;
        let front_blend = bits[3] & 1 != 0;
        let back_blend = bits[3] & 4 != 0;
        let back_is_unload = back
            .and_then(|b| world.get::<StackConveyorBuild>(b))
            .is_some_and(|b| b.state == STATE_UNLOAD);

        let mut state = STATE_MOVE;
        if case0 && front_blend && (!back_blend || back_is_unload) {
            state = STATE_LOAD;
        }
        if self.output_router && case0 && !front_blend && back_blend {
            state = STATE_UNLOAD;
        }
        let front_is_stack = front
            .and_then(|f| world.get::<StackConveyorBuild>(f))
            .is_some();
        if !self.output_router && !front_is_stack {
            state = STATE_UNLOAD;
        }

        // Cannot load when facing.
        if state == STATE_LOAD {
            let near: Vec<Entity> = world
                .get::<Building>(e)
                .map(|b| b.proximity.iter().copied().collect())
                .unwrap_or_default();
            for other in near {
                if world.get::<StackConveyorBuild>(other).is_some()
                    && transfer::front(world, other) == Some(e)
                {
                    state = STATE_MOVE;
                    break;
                }
            }
        }

        if let Some(mut build) = world.get_mut::<StackConveyorBuild>(e) {
            build.state = state;
            build.blendprox = bits[4] as u8;
        }

        if state != last_state {
            if let Some(mut build) = world.get_mut::<StackConveyorBuild>(e) {
                build.prox_updating = true;
            }
            let neighbors: Vec<Entity> = world
                .get::<Building>(e)
                .map(|b| b.proximity.iter().copied().collect())
                .unwrap_or_default();
            for near in neighbors {
                let skip = world
                    .get::<StackConveyorBuild>(near)
                    .is_some_and(|b| b.prox_updating && b.state != STATE_UNLOAD);
                if skip {
                    continue;
                }
                self.dispatch_proximity_update(world, near);
            }
            if let Some(mut build) = world.get_mut::<StackConveyorBuild>(e) {
                build.prox_updating = false;
            }
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let enabled = world.get::<Building>(e).map(|b| b.enabled).unwrap_or(false);
        let efficiency = world
            .get::<Building>(e)
            .map(|b| b.efficiency)
            .unwrap_or(0.0);
        let eff = if enabled {
            efficiency + self.base_efficiency
        } else {
            1.0
        };

        let Some(build) = world.get::<StackConveyorBuild>(e).cloned() else {
            return;
        };
        let mut cooldown = build.cooldown;
        if cooldown > 0.0 {
            cooldown = (cooldown - self.speed * eff * delta(world, e)).clamp(0.0, self.recharge);
            if let Some(mut b) = world.get_mut::<StackConveyorBuild>(e) {
                b.cooldown = cooldown;
            }
        }
        if build.link == -1 || cooldown > 0.0 {
            return;
        }

        // Refresh last item.
        let has_last = build
            .last_item
            .is_some_and(|item| transfer::item_count(world, e, item) > 0);
        let last_item = if has_last {
            build.last_item
        } else {
            transfer::first_item(world, e)
        };
        if let Some(mut b) = world.get_mut::<StackConveyorBuild>(e) {
            b.last_item = last_item;
        }
        if !enabled {
            return;
        }
        let Some(last_item) = last_item else {
            return;
        };

        if build.state == STATE_UNLOAD {
            loop {
                let moved = if !self.output_router {
                    transfer::move_forward(world, e, last_item)
                } else {
                    transfer::dump(world, e, Some(last_item))
                };
                if !moved {
                    break;
                }
                if !self.output_router
                    && let Some(mut items) = world.get_mut::<ItemModule>(e)
                {
                    items.remove(last_item, 1);
                }
                if transfer::item_count(world, e, last_item) <= 0 {
                    self.poof_out(world, e);
                    if let Some(mut b) = world.get_mut::<StackConveyorBuild>(e) {
                        b.last_item = None;
                    }
                    break;
                }
            }
        } else {
            let max = transfer::get_maximum_accepted(world, e, last_item);
            if (build.state != STATE_LOAD || transfer::item_total(world, e) >= max)
                && let Some(front) = transfer::front(world, e)
                && world.get::<StackConveyorBuild>(front).is_some()
                && world
                    .get::<StackConveyorBuild>(front)
                    .is_some_and(|f| f.link == -1)
                && same_team(world, e, front)
            {
                self.poof_out(world, e);
                if let Some(mut b) = world.get_mut::<StackConveyorBuild>(e) {
                    b.link = -1;
                }
                let pos = world
                    .get::<Building>(e)
                    .map(|b| b.tile.pack())
                    .unwrap_or(-1);
                self.transfer_module(world, e, front);
                if let Some(mut fb) = world.get_mut::<StackConveyorBuild>(front) {
                    fb.last_item = Some(last_item);
                    fb.link = pos;
                    fb.cooldown = 1.0;
                }
                if let Some(mut b) = world.get_mut::<StackConveyorBuild>(e) {
                    b.cooldown = self.recharge;
                }
                no_sleep(world, front);
            }
        }
    }

    fn accept_item(&self, world: &World, e: Entity, source: Entity, item: ItemId) -> bool {
        if e == source {
            return transfer::item_total(world, e) < transfer::item_capacity(world, e)
                && (transfer::item_total(world, e) == 0
                    || transfer::item_count(world, e, item) > 0);
        }
        let Some(build) = world.get::<StackConveyorBuild>(e) else {
            return false;
        };
        if build.cooldown > self.recharge - 1.0 {
            return false;
        }
        let front = transfer::front(world, e);
        !((build.state != STATE_LOAD)
            || (transfer::item_total(world, e) > 0 && transfer::item_count(world, e, item) == 0)
            || transfer::item_total(world, e) >= transfer::get_maximum_accepted(world, e, item)
            || front == Some(source))
    }

    fn accept_stack(
        &self,
        world: &World,
        e: Entity,
        item: ItemId,
        amount: i32,
        source: Option<Entity>,
    ) -> i32 {
        if transfer::item_total(world, e) > 0 && transfer::item_count(world, e, item) == 0 {
            return 0;
        }
        transfer::default_accept_stack(world, e, item, amount, source)
    }

    fn handle_item(&self, world: &mut World, e: Entity, _source: Entity, item: ItemId) {
        if transfer::item_total(world, e) == 0 {
            self.poof_in(world, e);
        }
        transfer::default_handle_item(world, e, e, item);
        if let Some(mut b) = world.get_mut::<StackConveyorBuild>(e) {
            b.last_item = Some(item);
        }
        no_sleep(world, e);
    }

    fn handle_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) {
        if amount <= 0 {
            return;
        }
        if transfer::item_total(world, e) == 0 {
            self.poof_in(world, e);
        }
        transfer::default_handle_stack(world, e, item, amount);
        if let Some(mut b) = world.get_mut::<StackConveyorBuild>(e) {
            b.last_item = Some(item);
        }
    }

    fn remove_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) -> i32 {
        let removed = transfer::default_remove_stack(world, e, item, amount);
        if transfer::item_total(world, e) == 0 {
            self.poof_out(world, e);
        }
        removed
    }

    fn item_taken(&self, world: &mut World, e: Entity, _item: ItemId) {
        if transfer::item_total(world, e) == 0 {
            self.poof_out(world, e);
        }
    }
}

impl StackConveyorBehavior {
    fn dispatch_proximity_update(&self, world: &mut World, e: Entity) {
        let Some(block) = world.get::<Building>(e).map(|b| b.block) else {
            return;
        };
        let Some(inst) = world
            .get_resource::<BlockTable>()
            .and_then(|table| table.instance(block))
        else {
            return;
        };
        inst.behavior.on_proximity_update(world, e);
    }

    fn poof_in(&self, world: &mut World, e: Entity) {
        let pos = world
            .get::<Building>(e)
            .map(|b| b.tile.pack())
            .unwrap_or(-1);
        if let Some(mut b) = world.get_mut::<StackConveyorBuild>(e) {
            b.link = pos;
        }
    }

    fn poof_out(&self, world: &mut World, e: Entity) {
        if let Some(mut b) = world.get_mut::<StackConveyorBuild>(e) {
            b.link = -1;
        }
    }

    fn transfer_module(&self, world: &mut World, from: Entity, to: Entity) {
        let Some(stacks) = world.get::<ItemModule>(from).map(|module| {
            module
                .stacks()
                .collect::<smallvec::SmallVec<[(ItemId, i32); 8]>>()
        }) else {
            return;
        };
        let cap = transfer::item_capacity(world, to).max(0);
        if let Some(mut items) = world.get_mut::<ItemModule>(to) {
            for (item, amount) in stacks {
                items.add(item, amount, if cap == 0 { i32::MAX / 2 } else { cap });
            }
        }
        if let Some(mut items) = world.get_mut::<ItemModule>(from) {
            items.items.iter_mut().for_each(|amount| *amount = 0);
            items.total = 0;
        }
    }
}

fn same_team(world: &World, a: Entity, b: Entity) -> bool {
    world
        .get::<crate::entities::comp::TeamComp>(a)
        .zip(world.get::<crate::entities::comp::TeamComp>(b))
        .is_some_and(|(x, y)| x.team == y.team)
}

/// `edelta` helper kept for symmetry with the upstream update.
#[allow(dead_code)]
fn _edelta(world: &World, e: Entity) -> f32 {
    edelta(world, e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;

    #[test]
    fn stack_conveyor_batches_to_front() {
        let mut harness = BuildHarness::new(16, 16, 7);
        let copper = harness.content().item_id("copper").expect("copper");
        let stack = harness
            .content()
            .block_id("plastanium-conveyor")
            .expect("plastanium-conveyor");
        assert!(harness.place(5, 4, stack, 0, true));
        assert!(harness.place(6, 4, stack, 0, true));
        let first = harness.build_at(5, 4).expect("first");
        let second = harness.build_at(6, 4).expect("second");
        let first_pos = harness
            .world
            .get::<Building>(first)
            .map(|b| b.tile.pack())
            .unwrap();
        if let Some(mut state) = harness.world.get_mut::<StackConveyorBuild>(first) {
            state.link = first_pos;
            state.state = STATE_MOVE;
        }
        if let Some(mut items) = harness.world.get_mut::<ItemModule>(first) {
            items.add(copper, 10, 10);
        }
        crate::world::update::building_update(&mut harness.world, first);
        let moved = harness
            .world
            .get::<ItemModule>(second)
            .map(|items| items.total)
            .unwrap_or(0);
        assert_eq!(moved, 10, "stack should transfer as a batch");
        let cleared = harness
            .world
            .get::<ItemModule>(first)
            .map(|items| items.total)
            .unwrap_or(-1);
        assert_eq!(cleared, 0);
        // Receiver links back to the sender tile.
        assert_eq!(
            harness
                .world
                .get::<StackConveyorBuild>(second)
                .map(|b| b.link),
            Some(first_pos)
        );
    }

    #[test]
    fn stack_conveyor_state_is_load_when_facing_a_stack() {
        let mut harness = BuildHarness::new(16, 16, 7);
        let stack = harness
            .content()
            .block_id("plastanium-conveyor")
            .expect("plastanium-conveyor");
        assert!(harness.place(5, 4, stack, 0, true));
        assert!(harness.place(6, 4, stack, 0, true));
        let first = harness.build_at(5, 4).expect("first");
        let state = harness
            .world
            .get::<StackConveyorBuild>(first)
            .map(|b| b.state)
            .unwrap_or(9);
        assert_eq!(state, STATE_LOAD);
    }
}
