// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DuctRouter` build behavior (`world/blocks/distribution/DuctRouter.java`) —
//! plan 08 M2. Duct-style `progress`/`current` movement with the `sortItem`
//! filter and the `cdump` target scan. `StackRouter` (M2) reuses
//! [`target_scan`] and [`facing_input`].

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ItemId;
use crate::entities::comp::Building;
use crate::world::behavior::BuildingBehavior;
use crate::world::config::ConfigValue;
use crate::world::modules::ItemModule;
use crate::world::{edelta, no_sleep};

use super::transfer;

/// `DuctRouter.DuctRouterBuild` state.
#[derive(Debug, Clone, Default, Component)]
pub struct DuctRouterBuild {
    /// Configured sort item (`sortItem`); `None` = round-robin.
    pub sort_item: Option<ItemId>,
    /// Movement progress (`progress`).
    pub progress: f32,
    /// Item currently in transit (`current`).
    pub current: Option<ItemId>,
}

/// `DuctRouter` behavior (`duct-router`).
#[derive(Debug, Clone, Copy)]
pub struct DuctRouterBehavior {
    /// `DuctRouter.speed`.
    pub speed: f32,
}

impl DuctRouterBehavior {
    /// Vanilla `duct-router` (`speed = 5`).
    pub const VANILLA: DuctRouterBehavior = DuctRouterBehavior { speed: 5.0 };
}

/// The `DuctRouterBuild.target()` proximity scan shared by `DuctRouter` and
/// `StackRouter`. Returns the accepting target and advances `cdump` exactly like
/// the Java loop (one increment per candidate).
pub fn target_scan(
    world: &mut World,
    e: Entity,
    current: ItemId,
    sort_item: Option<ItemId>,
) -> Option<Entity> {
    let proximity: Vec<Entity> = world
        .get::<Building>(e)
        .map(|b| b.proximity.iter().copied().collect())
        .unwrap_or_default();
    if proximity.is_empty() {
        return None;
    }
    let rotation = world.get::<Building>(e).map(|b| b.rotation).unwrap_or(0);
    let team = world
        .get::<crate::entities::comp::TeamComp>(e)
        .map(|t| t.team);
    let dump = world.get::<Building>(e).map(|b| b.cdump).unwrap_or(0) as usize;
    for i in 0..proximity.len() {
        let other = proximity[(i + dump) % proximity.len()];
        let rel = transfer::relative_dir(world, e, other);
        let sort_mismatch =
            sort_item.is_some_and(|sort| (current == sort) != (rel == rotation as i8));
        if !sort_mismatch
            && rel != ((rotation as i8 + 2) % 4)
            && world
                .get::<crate::entities::comp::TeamComp>(other)
                .map(|t| t.team)
                == team
            && transfer::dispatch_accept_item(world, other, e, current)
        {
            transfer::increment_dump(world, e, proximity.len());
            return Some(other);
        }
        transfer::increment_dump(world, e, proximity.len());
    }
    None
}

/// `DuctRouterBuild.acceptItem`: the source must feed the facing edge and the
/// router must be empty.
pub fn facing_input(world: &World, e: Entity, source: Entity, current: bool, total: i32) -> bool {
    let rotation = world.get::<Building>(e).map(|b| b.rotation).unwrap_or(0);
    !current && total == 0 && transfer::relative_to_edge(world, e, source) == rotation as i8
}

impl BuildingBehavior for DuctRouterBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<DuctRouterBuild>(e).is_none() {
            world.entity_mut(e).insert(DuctRouterBuild::default());
        }
    }

    fn always_update_when_disabled(&self) -> bool {
        true
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(build) = world.get::<DuctRouterBuild>(e).cloned() else {
            return;
        };
        let bound = 1.0 - 1.0 / self.speed;
        let mut progress = build.progress + edelta(world, e) / self.speed * 2.0;
        let mut current = build.current;
        if let Some(item) = current {
            if progress >= bound
                && let Some(target) = target_scan(world, e, item, build.sort_item)
            {
                transfer::dispatch_handle_item(world, target, e, item);
                if let Some(mut items) = world.get_mut::<ItemModule>(e) {
                    items.remove(item, 1);
                }
                current = None;
                progress %= bound;
            }
        } else {
            progress = 0.0;
        }
        if current.is_none() && transfer::item_total(world, e) > 0 {
            current = transfer::first_item(world, e);
        }
        if let Some(mut b) = world.get_mut::<DuctRouterBuild>(e) {
            b.progress = progress;
            b.current = current;
        }
    }

    fn accept_item(&self, world: &World, e: Entity, source: Entity, _item: ItemId) -> bool {
        let (current, total) = world
            .get::<DuctRouterBuild>(e)
            .map(|b| (b.current.is_some(), transfer::item_total(world, e)))
            .unwrap_or((false, 0));
        facing_input(world, e, source, current, total)
    }

    fn handle_item(&self, world: &mut World, e: Entity, _source: Entity, item: ItemId) {
        if let Some(mut items) = world.get_mut::<ItemModule>(e) {
            items.add(item, 1, i32::MAX / 2);
        }
        no_sleep(world, e);
        if let Some(mut b) = world.get_mut::<DuctRouterBuild>(e) {
            b.current = Some(item);
            b.progress = -1.0;
        }
    }

    fn handle_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) {
        transfer::default_handle_stack(world, e, item, amount);
        if let Some(mut b) = world.get_mut::<DuctRouterBuild>(e) {
            b.current = Some(item);
        }
    }

    fn remove_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) -> i32 {
        let removed = transfer::default_remove_stack(world, e, item, amount);
        if removed > 0
            && let Some(mut b) = world.get_mut::<DuctRouterBuild>(e)
            && b.current == Some(item)
        {
            b.current = None;
        }
        removed
    }

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        match world.get::<DuctRouterBuild>(e).and_then(|b| b.sort_item) {
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
        if let Some(mut b) = world.get_mut::<DuctRouterBuild>(e) {
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
            .get::<DuctRouterBuild>(e)
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
        let mut build = world.get::<DuctRouterBuild>(e).cloned().unwrap_or_default();
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
    fn duct_router_moves_item_to_front() {
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
        let router = harness.content().block_id("duct-router").expect("router");
        let container = harness.content().block_id("container").expect("container");
        assert!(harness.place(4, 4, source, 0, true));
        assert!(harness.place(5, 4, duct, 0, true));
        assert!(harness.place(6, 4, router, 0, true));
        assert!(harness.place(7, 4, container, 0, true));
        for _ in 0..300 {
            harness.tick();
        }
        let received = harness
            .build_at(7, 4)
            .and_then(|e| harness.world.get::<ItemModule>(e))
            .map(|m| m.total)
            .unwrap_or(0);
        assert!(received > 0, "container total={received}");
    }

    #[test]
    fn duct_router_config_roundtrip() {
        let mut harness = BuildHarness::new(16, 16, 7);
        let copper = harness.content().item_id("copper").expect("copper");
        let router = harness.content().block_id("duct-router").expect("router");
        assert!(harness.place(5, 4, router, 0, true));
        assert!(harness.configure(5, 4, ConfigValue::Item(copper)));
        let read = harness
            .build_at(5, 4)
            .map(|e| crate::world::config::read_config(&harness.world, e));
        assert_eq!(read, Some(ConfigValue::Item(copper)));
    }
}
