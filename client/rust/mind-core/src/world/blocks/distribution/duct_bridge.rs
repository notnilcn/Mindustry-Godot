// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DuctBridge` build behavior (`world/blocks/distribution/DuctBridge.java`) —
//! plan 08 M3. Pulled transfer paced by `progress` over a `DirectionBridge`
//! auto-link, with the forward fallback when unlinked.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ItemId;
use crate::entities::comp::Building;
use crate::world::behavior::BuildingBehavior;
use crate::world::modules::ItemModule;
use crate::world::{edelta, no_sleep};

use super::direction_bridge::{DirectionBridgeBuild, find_link, is_valid, relative_to_edge};
use super::transfer;

/// `DuctBridge.DuctBridgeBuild` state (`progress`).
#[derive(Debug, Clone, Default, Component)]
pub struct DuctBridgeBuild {
    /// Transfer progress (`progress`).
    pub progress: f32,
}

/// `DuctBridge` behavior (`duct-bridge`).
#[derive(Debug, Clone, Copy)]
pub struct DuctBridgeBehavior {
    /// `DuctBridge.speed`.
    pub speed: f32,
    /// `DirectionBridge.range`.
    pub range: i32,
}

impl DuctBridgeBehavior {
    /// Vanilla `duct-bridge` (`speed = 4`, `range = 4`).
    pub const VANILLA: DuctBridgeBehavior = DuctBridgeBehavior {
        speed: 4.0,
        range: 4,
    };
}

impl BuildingBehavior for DuctBridgeBehavior {
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
        if world.get::<DirectionBridgeBuild>(e).is_none() {
            world.entity_mut(e).insert(DirectionBridgeBuild::default());
        }
        if world.get::<DuctBridgeBuild>(e).is_none() {
            world.entity_mut(e).insert(DuctBridgeBuild::default());
        }
    }

    fn always_update_when_disabled(&self) -> bool {
        true
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let link = find_link(world, e, self.range);
        if let Some(mut bridge) = world.get_mut::<DirectionBridgeBuild>(e) {
            bridge.last_link = link;
        }
        if let Some(link) = link {
            let rotation = world.get::<Building>(e).map(|b| b.rotation).unwrap_or(0);
            if let Some(mut target) = world.get_mut::<DirectionBridgeBuild>(link) {
                target.occupied[(rotation % 4) as usize] = Some(e);
            }
            let link_cap = transfer::item_capacity(world, link);
            if transfer::item_total(world, e) > 0 && transfer::item_total(world, link) < link_cap {
                let mut progress = world
                    .get::<DuctBridgeBuild>(e)
                    .map(|b| b.progress)
                    .unwrap_or(0.0)
                    + edelta(world, e);
                while progress > self.speed {
                    let item = world
                        .get_mut::<ItemModule>(e)
                        .and_then(|mut items| items.take());
                    if let Some(item) = item
                        && transfer::item_total(world, link) < link_cap
                    {
                        transfer::dispatch_handle_item(world, link, e, item);
                    }
                    progress -= self.speed;
                }
                if let Some(mut bridge) = world.get_mut::<DuctBridgeBuild>(e) {
                    bridge.progress = progress;
                }
            }
        }

        if link.is_none()
            && transfer::item_total(world, e) > 0
            && let Some(item) = transfer::first_item(world, e)
            && transfer::move_forward(world, e, item)
            && let Some(mut items) = world.get_mut::<ItemModule>(e)
        {
            items.remove(item, 1);
        }

        // Clear stale occupancy entries.
        let rotation = world.get::<Building>(e).map(|b| b.rotation).unwrap_or(0);
        if let Some(bridge) = world.get::<DirectionBridgeBuild>(e).cloned() {
            for i in 0..4 {
                let stale = match bridge.occupied[i] {
                    None => true,
                    Some(other) => {
                        let other_rot = world
                            .get::<Building>(other)
                            .map(|b| b.rotation)
                            .unwrap_or(i as u8);
                        other_rot != i as u8
                            || !is_valid(world, other)
                            || world
                                .get::<DirectionBridgeBuild>(other)
                                .map(|b| b.last_link)
                                .unwrap_or(None)
                                != Some(e)
                    }
                };
                if stale && let Some(mut b) = world.get_mut::<DirectionBridgeBuild>(e) {
                    b.occupied[i] = None;
                }
            }
        }
        let _ = rotation;
    }

    fn accept_item(&self, world: &World, e: Entity, source: Entity, _item: ItemId) -> bool {
        if find_link(world, e, self.range).is_none() {
            return false;
        }
        let Some(bridge) = world.get::<DirectionBridgeBuild>(e) else {
            return false;
        };
        let rotation = world.get::<Building>(e).map(|b| b.rotation).unwrap_or(0);
        let rel = relative_to_edge(world, e, source);
        if rel < 0 {
            return false;
        }
        transfer::item_total(world, e) < transfer::item_capacity(world, e)
            && rel as u8 != rotation
            && bridge.occupied[((rel as u8 + 2) % 4) as usize].is_none()
    }

    fn handle_item(&self, world: &mut World, e: Entity, _source: Entity, item: ItemId) {
        transfer::default_handle_item(world, e, e, item);
        no_sleep(world, e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;

    #[test]
    fn duct_bridge_auto_links_and_delivers() {
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
            .block_id("duct-bridge")
            .expect("duct-bridge");
        let belt = harness.content().block_id("conveyor").expect("conveyor");
        let container = harness.content().block_id("container").expect("container");
        assert!(harness.place(4, 4, source, 0, true));
        assert!(harness.place(5, 4, bridge, 0, true));
        assert!(harness.place(8, 4, bridge, 0, true));
        assert!(harness.place(9, 4, belt, 0, true));
        assert!(harness.place(10, 4, container, 0, true));
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
