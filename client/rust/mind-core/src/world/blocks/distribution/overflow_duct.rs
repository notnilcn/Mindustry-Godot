// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `OverflowDuct`/`UnderflowDuct` behavior
//! (`world/blocks/distribution/OverflowDuct.java`) — plan 08 M2.
//!
//! Duct-style `progress`/`current` with a `target()` side-choice scan and the
//! `cdump` alternation.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ItemId;
use crate::entities::comp::Building;
use crate::world::behavior::BuildingBehavior;
use crate::world::modules::ItemModule;
use crate::world::{edelta, no_sleep};

use super::transfer;

/// `OverflowDuct.OverflowDuctBuild` state.
#[derive(Debug, Clone, Default, Component)]
pub struct OverflowDuctBuild {
    /// Movement progress (`progress`).
    pub progress: f32,
    /// Item currently on the duct (`current`).
    pub current: Option<ItemId>,
}

/// `OverflowDuct` behavior (`overflow-duct`, `underflow-duct`).
#[derive(Debug, Clone, Copy)]
pub struct OverflowDuctBehavior {
    /// `OverflowDuct.speed`.
    pub speed: f32,
    /// `OverflowDuct.invert` (`underflow-duct`).
    pub invert: bool,
}

impl OverflowDuctBehavior {
    /// `overflow-duct`.
    pub const NORMAL: OverflowDuctBehavior = OverflowDuctBehavior {
        speed: 4.0,
        invert: false,
    };
    /// `underflow-duct`.
    pub const UNDERFLOW: OverflowDuctBehavior = OverflowDuctBehavior {
        speed: 4.0,
        invert: true,
    };
}

impl BuildingBehavior for OverflowDuctBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<OverflowDuctBuild>(e).is_none() {
            world.entity_mut(e).insert(OverflowDuctBuild::default());
        }
    }

    fn always_update_when_disabled(&self) -> bool {
        true
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(build) = world.get::<OverflowDuctBuild>(e).cloned() else {
            return;
        };
        let bound = 1.0 - 1.0 / self.speed;
        let mut progress = build.progress + edelta(world, e) / self.speed * 2.0;
        if let Some(current) = build.current {
            if progress >= bound
                && let Some(target) = self.target(world, e, current)
            {
                transfer::dispatch_handle_item(world, target, e, current);
                if let Some(mut building) = world.get_mut::<Building>(e) {
                    building.cdump = if building.cdump == 0 { 2 } else { 0 };
                }
                if let Some(mut items) = world.get_mut::<ItemModule>(e) {
                    items.remove(current, 1);
                }
                if let Some(mut b) = world.get_mut::<OverflowDuctBuild>(e) {
                    b.current = None;
                }
                progress %= bound;
            }
        } else {
            progress = 0.0;
        }
        let first = if transfer::item_total(world, e) > 0 {
            transfer::first_item(world, e)
        } else {
            None
        };
        if let Some(mut b) = world.get_mut::<OverflowDuctBuild>(e) {
            b.progress = progress;
            if b.current.is_none() {
                b.current = first;
            }
        }
    }

    fn accept_item(&self, world: &World, e: Entity, source: Entity, _item: ItemId) -> bool {
        let Some(build) = world.get::<OverflowDuctBuild>(e) else {
            return false;
        };
        if build.current.is_some() || transfer::item_total(world, e) != 0 {
            return false;
        }
        let rotation = world.get::<Building>(e).map(|b| b.rotation).unwrap_or(0);
        transfer::relative_to_edge(world, e, source) == rotation as i8
    }

    fn handle_item(&self, world: &mut World, e: Entity, _source: Entity, item: ItemId) {
        if let Some(mut items) = world.get_mut::<ItemModule>(e) {
            items.add(item, 1, i32::MAX / 2);
        }
        no_sleep(world, e);
        if let Some(mut b) = world.get_mut::<OverflowDuctBuild>(e) {
            b.current = Some(item);
            b.progress = -1.0;
        }
    }

    fn handle_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) {
        transfer::default_handle_stack(world, e, item, amount);
        if let Some(mut b) = world.get_mut::<OverflowDuctBuild>(e) {
            b.current = Some(item);
        }
    }

    fn remove_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) -> i32 {
        let removed = transfer::default_remove_stack(world, e, item, amount);
        if removed > 0
            && let Some(mut b) = world.get_mut::<OverflowDuctBuild>(e)
            && b.current == Some(item)
        {
            b.current = None;
        }
        removed
    }
}

impl OverflowDuctBehavior {
    fn target(&self, world: &World, e: Entity, current: ItemId) -> Option<Entity> {
        let team = world
            .get::<crate::entities::comp::TeamComp>(e)
            .map(|t| t.team);
        let rotation = world.get::<Building>(e).map(|b| b.rotation).unwrap_or(0);
        let cdump = world.get::<Building>(e).map(|b| b.cdump).unwrap_or(0);
        let accepts = |candidate: Entity| {
            same_team(world, candidate, team)
                && transfer::dispatch_accept_item(world, candidate, e, current)
        };
        if self.invert {
            let left = transfer::nearby(world, e, (rotation + 1) % 4);
            let right = transfer::nearby(world, e, (rotation + 3) % 4);
            let lc = left.is_some_and(accepts);
            let rc = right.is_some_and(accepts);
            if lc && !rc {
                return left;
            } else if rc && !lc {
                return right;
            } else if lc && rc {
                return if cdump == 0 { left } else { right };
            }
        }
        let front = transfer::nearby(world, e, rotation);
        if front.is_some_and(accepts) {
            return front;
        }
        if self.invert {
            return None;
        }
        for i in -1..=1i32 {
            let dir = ((rotation as i32 + (((i + cdump as i32 + 1) % 3 + 3) % 3).wrapping_sub(1))
                .rem_euclid(4)) as u8;
            if dir == rotation {
                continue;
            }
            if let Some(other) = transfer::nearby(world, e, dir)
                && accepts(other)
            {
                return Some(other);
            }
        }
        None
    }
}

fn same_team(world: &World, entity: Entity, team: Option<u8>) -> bool {
    world
        .get::<crate::entities::comp::TeamComp>(entity)
        .map(|t| t.team)
        == team
}
