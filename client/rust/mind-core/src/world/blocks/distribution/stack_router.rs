// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `StackRouter` build behavior (`world/blocks/distribution/StackRouter.java`)
//! — plan 08 M2. `DuctRouter` with a batch-unload state machine and a
//! `baseEfficiency` bonus (`surge-router`).

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ItemId;
use crate::entities::comp::Building;
use crate::world::behavior::BuildingBehavior;
use crate::world::config::ConfigValue;
use crate::world::modules::ItemModule;
use crate::world::no_sleep;

use super::duct_router::target_scan;
use super::transfer;

/// `StackRouter.StackRouterBuild` state.
#[derive(Debug, Clone, Default, Component)]
pub struct StackRouterBuild {
    /// Configured sort item (`sortItem`); `None` = round-robin.
    pub sort_item: Option<ItemId>,
    /// Fill/unload progress (`progress`).
    pub progress: f32,
    /// Item currently being routed (`current`).
    pub current: Option<ItemId>,
    /// Whether the router is batch-unloading (`unloading`).
    pub unloading: bool,
}

/// `StackRouter` behavior (`surge-router`).
#[derive(Debug, Clone, Copy)]
pub struct StackRouterBehavior {
    /// `StackRouter.speed`.
    pub speed: f32,
    /// `StackRouter.baseEfficiency` (`surge-router` = 1).
    pub base_efficiency: f32,
}

impl StackRouterBehavior {
    /// Vanilla `surge-router`.
    pub const SURGE: StackRouterBehavior = StackRouterBehavior {
        speed: 6.0,
        base_efficiency: 1.0,
    };
}

impl BuildingBehavior for StackRouterBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<StackRouterBuild>(e).is_none() {
            world.entity_mut(e).insert(StackRouterBuild::default());
        }
    }

    fn always_update_when_disabled(&self) -> bool {
        true
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(build) = world.get::<StackRouterBuild>(e).cloned() else {
            return;
        };
        let cap = self.speed;
        let item_capacity = transfer::item_capacity(world, e);
        let total = transfer::item_total(world, e);
        let (enabled, efficiency) = world
            .get::<Building>(e)
            .map(|b| (b.enabled, b.efficiency))
            .unwrap_or((true, 0.0));
        let eff = if enabled {
            efficiency + self.base_efficiency
        } else {
            0.0
        };

        let mut progress = build.progress;
        let mut current = build.current;
        let mut unloading = build.unloading;

        if !unloading && current.is_some() && total >= item_capacity && progress < cap {
            progress += eff;
        }
        if !unloading && current.is_some() && total >= item_capacity && progress >= cap {
            unloading = true;
            progress %= cap;
        }

        if unloading && let Some(item) = current {
            loop {
                if transfer::item_count(world, e, item) <= 0 {
                    break;
                }
                let Some(target) = target_scan(world, e, item, build.sort_item) else {
                    break;
                };
                transfer::dispatch_handle_item(world, target, e, item);
                if let Some(mut items) = world.get_mut::<ItemModule>(e) {
                    items.remove(item, 1);
                }
            }
            if transfer::item_count(world, e, item) == 0 {
                current = None;
                unloading = false;
            }
        }

        let any = transfer::item_total(world, e);
        if (current.is_none() || current.is_some_and(|c| transfer::item_count(world, e, c) == 0))
            && any > 0
        {
            current = transfer::first_item(world, e);
        }
        if any == 0 {
            unloading = false;
            current = None;
        }

        if let Some(mut b) = world.get_mut::<StackRouterBuild>(e) {
            b.progress = progress;
            b.current = current;
            b.unloading = unloading;
        }
    }

    fn accept_item(&self, world: &World, e: Entity, source: Entity, item: ItemId) -> bool {
        let Some(build) = world.get::<StackRouterBuild>(e) else {
            return false;
        };
        let capacity = transfer::item_capacity(world, e);
        let rotation = world.get::<Building>(e).map(|b| b.rotation).unwrap_or(0);
        !build.unloading
            && (build.current.is_none() || build.current == Some(item))
            && transfer::item_total(world, e) < capacity
            && transfer::relative_to_edge(world, e, source) == rotation as i8
    }

    fn handle_item(&self, world: &mut World, e: Entity, _source: Entity, item: ItemId) {
        if let Some(mut items) = world.get_mut::<ItemModule>(e) {
            items.add(item, 1, i32::MAX / 2);
        }
        no_sleep(world, e);
        if let Some(mut b) = world.get_mut::<StackRouterBuild>(e) {
            b.current = Some(item);
            b.progress = -1.0;
        }
    }

    fn handle_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) {
        transfer::default_handle_stack(world, e, item, amount);
        if let Some(mut b) = world.get_mut::<StackRouterBuild>(e) {
            b.current = Some(item);
        }
    }

    fn remove_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) -> i32 {
        let removed = transfer::default_remove_stack(world, e, item, amount);
        if removed > 0
            && let Some(mut b) = world.get_mut::<StackRouterBuild>(e)
            && b.current == Some(item)
        {
            b.current = None;
        }
        removed
    }

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        match world.get::<StackRouterBuild>(e).and_then(|b| b.sort_item) {
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
        if let Some(mut b) = world.get_mut::<StackRouterBuild>(e) {
            b.sort_item = match value {
                ConfigValue::Item(item) => Some(item),
                _ => None,
            };
        }
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        1
    }

    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        let id = world
            .get::<StackRouterBuild>(e)
            .and_then(|b| b.sort_item)
            .map(|item| item.raw() as i16)
            .unwrap_or(-1);
        w.s(id);
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        revision: u8,
    ) {
        let mut build = world
            .get::<StackRouterBuild>(e)
            .cloned()
            .unwrap_or_default();
        if revision >= 1
            && let Ok(id) = r.s()
        {
            build.sort_item = (id >= 0).then(|| ItemId::new(id as u16));
        }
        world.entity_mut(e).insert(build);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;

    #[test]
    fn stack_router_unloads_batch_to_container() {
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
        let router = harness
            .content()
            .block_id("surge-router")
            .expect("surge-router");
        let container = harness.content().block_id("container").expect("container");
        assert!(harness.place(4, 4, source, 0, true));
        assert!(harness.place(5, 4, duct, 0, true));
        assert!(harness.place(6, 4, router, 0, true));
        assert!(harness.place(7, 4, container, 0, true));
        // Power is not simulated here; the harness leaves efficiency at 1.
        if let Some(e) = harness.build_at(6, 4) {
            harness.world.get_mut::<Building>(e).unwrap().efficiency = 1.0;
        }
        for _ in 0..400 {
            harness.tick();
        }
        let received = harness
            .build_at(7, 4)
            .and_then(|e| harness.world.get::<ItemModule>(e))
            .map(|m| m.total)
            .unwrap_or(0);
        assert!(received > 0, "container total={received}");
    }
}
