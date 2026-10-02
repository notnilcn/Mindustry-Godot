// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Junction` build behavior (`world/blocks/distribution/Junction.java`) —
//! plan 08 M2. Per-direction `DirectionalItemBuffer`, straight-through routing
//! and the rev 0/1 IO shape.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ItemId;
use crate::world::behavior::BuildingBehavior;
use crate::world::item_buffer::{DirectionalItemBuffer, packed};
use crate::world::update::build_time;

use super::transfer;

/// `Junction.capacity`.
pub const CAPACITY: usize = 6;

/// `Junction.JunctionBuild` state.
#[derive(Debug, Clone, Component)]
pub struct JunctionBuild {
    /// Per-direction time-delayed queue.
    pub buffer: DirectionalItemBuffer,
}

impl Default for JunctionBuild {
    fn default() -> Self {
        Self {
            buffer: DirectionalItemBuffer::new(CAPACITY),
        }
    }
}

/// `Junction` behavior.
#[derive(Debug, Clone, Copy)]
pub struct JunctionBehavior {
    /// Frames taken to pass through (`Junction.speed`).
    pub speed: f32,
}

impl JunctionBehavior {
    /// Vanilla `junction` (`speed = 26`).
    pub const VANILLA: JunctionBehavior = JunctionBehavior { speed: 26.0 };
}

impl BuildingBehavior for JunctionBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<JunctionBuild>(e).is_none() {
            world.entity_mut(e).insert(JunctionBuild::default());
        }
    }

    fn always_update_when_disabled(&self) -> bool {
        true
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let now = build_time(world);
        let time_scale = world
            .get::<crate::entities::comp::Building>(e)
            .map(|b| b.time_scale)
            .unwrap_or(1.0);
        let team = world
            .get::<crate::entities::comp::TeamComp>(e)
            .map(|t| t.team);
        for dir in 0..4u8 {
            let item = world.get::<JunctionBuild>(e).and_then(|junction| {
                junction
                    .buffer
                    .poll(dir as usize, self.speed / time_scale, now)
            });
            let Some(item) = item else {
                continue;
            };
            let Some(dest) = transfer::nearby(world, e, dir) else {
                continue;
            };
            let dest_team = world
                .get::<crate::entities::comp::TeamComp>(dest)
                .map(|t| t.team);
            if dest_team != team || !transfer::dispatch_accept_item(world, dest, e, item) {
                continue;
            }
            transfer::dispatch_handle_item(world, dest, e, item);
            if let Some(mut junction) = world.get_mut::<JunctionBuild>(e) {
                junction.buffer.remove(dir as usize);
            }
        }
    }

    fn handle_item(&self, world: &mut World, e: Entity, source: Entity, item: ItemId) {
        let relative = transfer::relative_dir(world, source, e);
        if relative < 0 {
            return;
        }
        let now = build_time(world);
        if let Some(mut junction) = world.get_mut::<JunctionBuild>(e) {
            junction.buffer.accept(relative as usize, item, now);
        }
    }

    fn accept_item(&self, world: &World, e: Entity, source: Entity, item: ItemId) -> bool {
        let relative = transfer::relative_dir(world, source, e);
        if relative < 0 {
            return false;
        }
        let Some(junction) = world.get::<JunctionBuild>(e) else {
            return false;
        };
        if !junction.buffer.accepts(relative as usize) {
            return false;
        }
        let source_team = world
            .get::<crate::entities::comp::TeamComp>(source)
            .map(|t| t.team);
        let Some(to) = transfer::nearby(world, e, relative as u8) else {
            return false;
        };
        let _ = item;
        world
            .get::<crate::entities::comp::TeamComp>(to)
            .map(|t| t.team)
            == source_team
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

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        1
    }

    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        if let Some(junction) = world.get::<JunctionBuild>(e) {
            junction.buffer.write(w);
        }
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        revision: u8,
    ) {
        let mut junction = world.get::<JunctionBuild>(e).cloned().unwrap_or_default();
        let _ = if revision == 0 {
            junction.buffer.read_legacy(r)
        } else {
            junction.buffer.read(r)
        };
        world.entity_mut(e).insert(junction);
    }
}

/// Accessor for tests (time encoding side of the buffer).
pub fn buffer_item_time(value: u64) -> f32 {
    packed::buffer_item_time(value)
}

#[cfg(test)]
mod tests {
    use crate::world::BuildHarness;

    #[test]
    fn junction_passes_items_straight_through() {
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
        let belt = harness.content().block_id("conveyor").expect("conveyor");
        let junction = harness.content().block_id("junction").expect("junction");
        let container = harness.content().block_id("container").expect("container");
        assert!(harness.place(4, 4, source, 0, true));
        assert!(harness.place(5, 4, junction, 0, true));
        assert!(harness.place(6, 4, belt, 0, true));
        assert!(harness.place(7, 4, container, 0, true));
        for _ in 0..400 {
            harness.tick();
        }
        let received = harness
            .build_at(7, 4)
            .and_then(|e| harness.world.get::<crate::world::modules::ItemModule>(e))
            .map(|items| items.total)
            .unwrap_or(0);
        assert!(received > 0, "vault total={received}");
    }
}
