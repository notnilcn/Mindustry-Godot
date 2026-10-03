// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `OverflowGate`/`UnderflowGate` behavior
//! (`world/blocks/distribution/OverflowGate.java`) — plan 08 M2.
//!
//! Instant transfer only: `acceptItem`/`handleItem` route through
//! `getTileTarget`, preferring the forward direction and falling back to a side;
//! the persistent `rotation` byte is the side-choice bitfield.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockId, BlockKind, ItemId};
use crate::entities::comp::Building;
use crate::world::behavior::BuildingBehavior;
use crate::world::block::BlockTable;

use super::transfer;

fn instant_transfer(block: BlockId, table: &BlockTable) -> bool {
    table
        .get(block)
        .is_some_and(|inst| matches!(inst.def.kind, BlockKind::Sorter | BlockKind::OverflowGate))
}

/// `OverflowGate` behavior (`overflow-gate`, `underflow-gate`).
#[derive(Debug, Default, Clone, Copy)]
pub struct OverflowGateBehavior {
    /// `OverflowGate.invert` (`underflow-gate`).
    pub invert: bool,
}

impl OverflowGateBehavior {
    /// `overflow-gate`.
    pub const NORMAL: OverflowGateBehavior = OverflowGateBehavior { invert: false };
    /// `underflow-gate`.
    pub const UNDERFLOW: OverflowGateBehavior = OverflowGateBehavior { invert: true };
}

impl BuildingBehavior for OverflowGateBehavior {
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

    fn accept_item(&self, world: &World, e: Entity, source: Entity, item: ItemId) -> bool {
        let (target, _flip) = self.get_tile_target(world, e, item, source);
        let Some(target) = target else {
            return false;
        };
        let same_team = world
            .get::<crate::entities::comp::TeamComp>(e)
            .zip(world.get::<crate::entities::comp::TeamComp>(target))
            .is_some_and(|(a, b)| a.team == b.team);
        same_team && transfer::dispatch_accept_item(world, target, e, item)
    }

    fn handle_item(&self, world: &mut World, e: Entity, source: Entity, item: ItemId) {
        let (target, flip) = self.get_tile_target(world, e, item, source);
        if let Some(dir) = flip
            && let Some(mut building) = world.get_mut::<Building>(e)
        {
            building.rotation ^= 1 << dir;
        }
        if let Some(target) = target {
            transfer::dispatch_handle_item(world, target, e, item);
        }
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        4
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        revision: u8,
    ) {
        if revision == 1 {
            let mut legacy = crate::world::item_buffer::DirectionalItemBuffer::new(25);
            let _ = legacy.read(r);
        } else if revision == 3 {
            let _ = r.i();
        }
        if let Some(mut items) = world.get_mut::<crate::world::modules::ItemModule>(e) {
            items.items.iter_mut().for_each(|amount| *amount = 0);
            items.total = 0;
        }
    }
}

impl OverflowGateBehavior {
    /// `OverflowGateBuild.getTileTarget`; returns the target and, when the
    /// side-choice branch was taken, the direction whose bit should flip.
    fn get_tile_target(
        &self,
        world: &World,
        e: Entity,
        item: ItemId,
        src: Entity,
    ) -> (Option<Entity>, Option<u8>) {
        let from = transfer::self_to_source(world, e, src);
        if from < 0 {
            return (None, None);
        }
        let from = from as u8;
        let forward_dir = (from + 2) % 4;
        let to = transfer::nearby(world, e, forward_dir);
        let team = world
            .get::<crate::entities::comp::TeamComp>(e)
            .map(|t| t.team);
        let enabled = world.get::<Building>(e).map(|b| b.enabled).unwrap_or(false);
        let from_inst = instant_of(world, src);
        let can_forward = to.is_some_and(|candidate| {
            same_team(world, candidate, team)
                && !(from_inst && instant_of(world, candidate))
                && transfer::dispatch_accept_item(world, candidate, e, item)
        });
        let inv = self.invert == enabled;

        if !can_forward || inv {
            let a = transfer::nearby(world, e, (from + 3) % 4);
            let b = transfer::nearby(world, e, (from + 1) % 4);
            let ac = a.is_some_and(|candidate| {
                same_team(world, candidate, team)
                    && !(from_inst && instant_of(world, candidate))
                    && transfer::dispatch_accept_item(world, candidate, e, item)
            });
            let bc = b.is_some_and(|candidate| {
                same_team(world, candidate, team)
                    && !(from_inst && instant_of(world, candidate))
                    && transfer::dispatch_accept_item(world, candidate, e, item)
            });
            if !ac && !bc {
                return (if inv && can_forward { to } else { None }, None);
            }
            return if ac && !bc {
                (a, None)
            } else if bc && !ac {
                (b, None)
            } else {
                let rotation = world.get::<Building>(e).map(|b| b.rotation).unwrap_or(0);
                let pick = if (rotation & (1 << from)) == 0 { a } else { b };
                (pick, Some(from))
            };
        }
        (to, None)
    }
}

fn same_team(world: &World, entity: Entity, team: Option<u8>) -> bool {
    world
        .get::<crate::entities::comp::TeamComp>(entity)
        .map(|t| t.team)
        == team
}

fn instant_of(world: &World, entity: Entity) -> bool {
    world
        .get::<Building>(entity)
        .zip(world.get_resource::<BlockTable>())
        .is_some_and(|(building, table)| instant_transfer(building.block, table))
}
