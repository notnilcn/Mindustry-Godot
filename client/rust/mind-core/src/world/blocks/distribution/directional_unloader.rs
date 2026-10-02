// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DirectionalUnloader` build behavior
//! (`world/blocks/distribution/DirectionalUnloader.java`) — plan 08 M4.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockKind, ItemId};
use crate::world::behavior::BuildingBehavior;
use crate::world::block::BlockTable;
use crate::world::config::ConfigValue;
use crate::world::edelta;
use crate::world::modules::ItemModule;

use super::super::storage::storage_block::StorageBuild;
use super::transfer;

/// `DirectionalUnloader.DirectionalUnloaderBuild` state.
#[derive(Debug, Clone, Default, Component)]
pub struct DirectionalUnloaderBuild {
    /// Unload timer (`unloadTimer`).
    pub unload_timer: f32,
    /// Configured item (`unloadItem`).
    pub unload_item: Option<ItemId>,
    /// Round-robin item cursor (`offset`).
    pub offset: i32,
}

fn kind_of(world: &World, e: Entity) -> Option<BlockKind> {
    let building = world.get::<crate::entities::comp::Building>(e)?;
    let table = world.get_resource::<BlockTable>()?;
    table.get(building.block).map(|inst| inst.def.kind)
}

fn same_team(world: &World, a: Entity, b: Entity) -> bool {
    world
        .get::<crate::entities::comp::TeamComp>(a)
        .zip(world.get::<crate::entities::comp::TeamComp>(b))
        .is_some_and(|(x, y)| x.team == y.team)
}

/// `DirectionalUnloader` behavior (`duct-unloader`).
#[derive(Debug, Clone, Copy)]
pub struct DirectionalUnloaderBehavior {
    /// `DirectionalUnloader.speed`.
    pub speed: f32,
    /// `DirectionalUnloader.allowCoreUnload`.
    pub allow_core_unload: bool,
}

impl DirectionalUnloaderBehavior {
    /// Vanilla `duct-unloader`.
    pub const VANILLA: DirectionalUnloaderBehavior = DirectionalUnloaderBehavior {
        speed: 1.0,
        allow_core_unload: false,
    };
}

impl BuildingBehavior for DirectionalUnloaderBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<DirectionalUnloaderBuild>(e).is_none() {
            world
                .entity_mut(e)
                .insert(DirectionalUnloaderBuild::default());
        }
    }

    fn always_update_when_disabled(&self) -> bool {
        true
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(mut build) = world.get::<DirectionalUnloaderBuild>(e).cloned() else {
            return;
        };
        build.unload_timer += edelta(world, e);
        if build.unload_timer >= self.speed {
            if let (Some(front), Some(back)) = (transfer::front(world, e), transfer::back(world, e))
            {
                let back_kind = kind_of(world, back);
                let back_is_core_or_linked = back_kind == Some(BlockKind::CoreBlock)
                    || (back_kind == Some(BlockKind::StorageBlock)
                        && world
                            .get::<StorageBuild>(back)
                            .is_some_and(|b| b.linked_core.is_some()));
                let can = same_team(world, front, back)
                    && world.get::<ItemModule>(back).is_some()
                    && transfer::dispatch_can_unload(world, back)
                    && (self.allow_core_unload || !back_is_core_or_linked);
                if can {
                    match build.unload_item {
                        None => {
                            let items = world
                                .get::<ItemModule>(e)
                                .map(|m| m.items.len())
                                .unwrap_or(0);
                            if items > 0 {
                                for i in 0..items {
                                    let id = ((i as i32 + build.offset).rem_euclid(items as i32))
                                        as usize;
                                    let item = ItemId::new(id as u16);
                                    if transfer::item_count(world, back, item) > 0
                                        && transfer::dispatch_accept_item(world, front, e, item)
                                    {
                                        transfer::dispatch_handle_item(world, front, e, item);
                                        if let Some(mut module) = world.get_mut::<ItemModule>(back)
                                        {
                                            module.remove(item, 1);
                                        }
                                        transfer::dispatch_item_taken(world, back, item);
                                        build.offset = item.raw() as i32 + 1;
                                        break;
                                    }
                                }
                            }
                        }
                        Some(item) => {
                            if transfer::item_count(world, back, item) > 0
                                && transfer::dispatch_accept_item(world, front, e, item)
                            {
                                transfer::dispatch_handle_item(world, front, e, item);
                                if let Some(mut module) = world.get_mut::<ItemModule>(back) {
                                    module.remove(item, 1);
                                }
                                transfer::dispatch_item_taken(world, back, item);
                            }
                        }
                    }
                }
            }
            build.unload_timer %= self.speed;
        }
        world.entity_mut(e).insert(build);
    }

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        match world
            .get::<DirectionalUnloaderBuild>(e)
            .and_then(|b| b.unload_item)
        {
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
        if let Some(mut b) = world.get_mut::<DirectionalUnloaderBuild>(e) {
            b.unload_item = match value {
                ConfigValue::Item(item) => Some(item),
                _ => None,
            };
        }
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        0
    }

    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        let build = world
            .get::<DirectionalUnloaderBuild>(e)
            .cloned()
            .unwrap_or_default();
        w.s(build.unload_item.map(|i| i.raw() as i16).unwrap_or(-1));
        w.s(build.offset as i16);
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        _revision: u8,
    ) {
        let mut build = world
            .get::<DirectionalUnloaderBuild>(e)
            .cloned()
            .unwrap_or_default();
        if let Ok(id) = r.s() {
            build.unload_item = (id >= 0).then(|| ItemId::new(id as u16));
        }
        if let Ok(offset) = r.s() {
            build.offset = offset as i32;
        }
        world.entity_mut(e).insert(build);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;

    #[test]
    fn directional_unloader_drains_back_into_front() {
        let mut harness = BuildHarness::new(16, 16, 7);
        let copper = harness.content().item_id("copper").expect("copper");
        let duct = harness.content().block_id("duct").expect("duct");
        let unloader = harness
            .content()
            .block_id("duct-unloader")
            .expect("duct-unloader");
        let container = harness.content().block_id("container").expect("container");
        // Back (west) container -> unloader (rot 0 = east) -> duct -> container.
        assert!(harness.place(3, 4, container, 0, true));
        assert!(harness.place(5, 4, unloader, 0, true));
        assert!(harness.place(6, 4, duct, 0, true));
        assert!(harness.place(7, 4, container, 0, true));
        let back = harness.build_at(3, 4).expect("back");
        harness
            .world
            .get_mut::<ItemModule>(back)
            .expect("items")
            .add(copper, 50, 300);
        for _ in 0..600 {
            harness.tick();
        }
        let drained = harness
            .build_at(7, 4)
            .and_then(|e| harness.world.get::<ItemModule>(e))
            .map(|m| m.total)
            .unwrap_or(0);
        assert!(drained > 0, "front container total={drained}");
    }
}
