// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DuctJunction` build behavior
//! (`world/blocks/distribution/DuctJunction.java`) — plan 08 M2.
//!
//! Four independent item slots, each advancing toward its up/right/down/left
//! neighbor with an independent `times` cursor. Upstream defines the class but
//! does not register a vanilla block; the behavior is registered by name for
//! mods/forward compatibility.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ItemId;
use crate::world::behavior::BuildingBehavior;
use crate::world::edelta;
use crate::world::modules::ItemModule;

use super::transfer;

/// `DuctJunction.DuctJunctionBuild` state.
#[derive(Debug, Clone, Component)]
pub struct DuctJunctionBuild {
    /// Per-direction item slot (`itemdata`).
    pub itemdata: [Option<ItemId>; 4],
    /// Per-direction progress cursor (`times`).
    pub times: [f32; 4],
}

impl Default for DuctJunctionBuild {
    fn default() -> Self {
        Self {
            itemdata: [None; 4],
            times: [0.0; 4],
        }
    }
}

/// `DuctJunction` behavior (`speed = 5`).
#[derive(Debug, Clone, Copy)]
pub struct DuctJunctionBehavior {
    /// `DuctJunction.speed`.
    pub speed: f32,
}

impl DuctJunctionBehavior {
    /// Vanilla `speed = 5`.
    pub const VANILLA: DuctJunctionBehavior = DuctJunctionBehavior { speed: 5.0 };
}

impl BuildingBehavior for DuctJunctionBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<DuctJunctionBuild>(e).is_none() {
            world.entity_mut(e).insert(DuctJunctionBuild::default());
        }
    }

    fn always_update_when_disabled(&self) -> bool {
        true
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(mut build) = world.get::<DuctJunctionBuild>(e).cloned() else {
            return;
        };
        let bound = 1.0 - 1.0 / self.speed;
        let inc = edelta(world, e) / self.speed * 2.0;
        let team = world
            .get::<crate::entities::comp::TeamComp>(e)
            .map(|t| t.team);
        for dir in 0..4usize {
            let Some(item) = build.itemdata[dir] else {
                build.times[dir] = 0.0;
                continue;
            };
            build.times[dir] += inc;
            if build.times[dir] < bound {
                continue;
            }
            let Some(next) = transfer::nearby(world, e, dir as u8) else {
                continue;
            };
            if world
                .get::<crate::entities::comp::TeamComp>(next)
                .map(|t| t.team)
                != team
                || !transfer::dispatch_accept_item(world, next, e, item)
            {
                continue;
            }
            transfer::dispatch_handle_item(world, next, e, item);
            build.itemdata[dir] = None;
            if let Some(mut items) = world.get_mut::<ItemModule>(e) {
                items.remove(item, 1);
            }
            build.times[dir] %= bound;
        }
        world.entity_mut(e).insert(build);
    }

    fn accept_item(&self, world: &World, e: Entity, source: Entity, _item: ItemId) -> bool {
        let Some(build) = world.get::<DuctJunctionBuild>(e) else {
            return false;
        };
        let relative = transfer::relative_dir(world, source, e);
        if relative < 0 || build.itemdata[relative as usize].is_some() {
            return false;
        }
        let Some(to) = transfer::nearby(world, e, relative as u8) else {
            return false;
        };
        world
            .get::<crate::entities::comp::TeamComp>(to)
            .map(|t| t.team)
            == world
                .get::<crate::entities::comp::TeamComp>(e)
                .map(|t| t.team)
    }

    fn handle_item(&self, world: &mut World, e: Entity, source: Entity, item: ItemId) {
        let relative = transfer::relative_dir(world, source, e);
        if relative < 0 {
            return;
        }
        if let Some(mut items) = world.get_mut::<ItemModule>(e) {
            items.add(item, 1, i32::MAX / 2);
        }
        if let Some(mut build) = world.get_mut::<DuctJunctionBuild>(e) {
            build.itemdata[relative as usize] = Some(item);
            build.times[relative as usize] = -1.0;
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

    fn remove_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) -> i32 {
        let Some(mut build) = world.get::<DuctJunctionBuild>(e).cloned() else {
            return 0;
        };
        let mut removed = 0;
        let mut remaining = amount;
        for dir in 0..4 {
            if remaining <= 0 {
                break;
            }
            if build.itemdata[dir] == Some(item) {
                remaining -= 1;
                removed += 1;
                build.itemdata[dir] = None;
                if let Some(mut items) = world.get_mut::<ItemModule>(e) {
                    items.remove(item, 1);
                }
            }
        }
        world.entity_mut(e).insert(build);
        removed
    }

    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        if let Some(build) = world.get::<DuctJunctionBuild>(e) {
            for dir in 0..4 {
                w.f(build.times[dir]);
                w.s(build.itemdata[dir].map(|i| i.raw() as i16).unwrap_or(-1));
            }
        }
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        _revision: u8,
    ) {
        let mut build = world
            .get::<DuctJunctionBuild>(e)
            .cloned()
            .unwrap_or_default();
        for dir in 0..4 {
            if let Ok(time) = r.f() {
                build.times[dir] = time;
            }
            if let Ok(id) = r.s() {
                build.itemdata[dir] = (id >= 0).then(|| ItemId::new(id as u16));
            }
        }
        world.entity_mut(e).insert(build);
    }
}
