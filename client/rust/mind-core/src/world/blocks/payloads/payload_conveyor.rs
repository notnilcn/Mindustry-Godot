// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PayloadConveyor` build behavior
//! (`world/blocks/payloads/PayloadConveyor.java`) — plan 08 M6.
//!
//! Time-paced payload transport (`moveTime = 45`), the `curStep`/`step` rollover
//! hand-off and the `next` alignment rules. Drawing/interp state is exposed for
//! plan 16.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::entities::comp::Building;
use crate::world::TileBuilds;
use crate::world::behavior::{BuildingBehavior, PayloadRef};
use crate::world::block::BlockTable;
use crate::world::update::build_time;

use super::{PayloadHolder, handle_payload, payload_fits, update_payload};

/// `PayloadConveyor.PayloadConveyorBuild` state.
#[derive(Debug, Clone, Copy, Component, Default)]
pub struct PayloadConveyorBuild {
    /// Progress within the move step (`progress`).
    pub progress: f32,
    /// Payload rotation (`itemRotation`).
    pub item_rotation: f32,
    /// Interp animation (`animation`).
    pub animation: f32,
    /// Current interpolation (`curInterp`).
    pub cur_interp: f32,
    /// Previous interpolation (`lastInterp`).
    pub last_interp: f32,
    /// Next aligned conveyor (`next`).
    pub next: Option<Entity>,
    /// Forward blocked (`blocked`).
    pub blocked: bool,
    /// Last step (`step`).
    pub step: i32,
    /// Step the payload was accepted on (`stepAccepted`).
    pub step_accepted: i32,
}

/// `PayloadConveyor` behavior (`payload-conveyor`, `reinforced-payload-conveyor`).
#[derive(Debug, Clone, Copy)]
pub struct PayloadConveyorBehavior {
    /// `PayloadConveyor.moveTime`.
    pub move_time: f32,
    /// `PayloadConveyor.payloadLimit`.
    pub payload_limit: f32,
}

impl PayloadConveyorBehavior {
    /// Vanilla `payload-conveyor`.
    pub const VANILLA: PayloadConveyorBehavior = PayloadConveyorBehavior {
        move_time: 45.0,
        payload_limit: 3.0,
    };
    /// `reinforced-payload-conveyor` (`moveTime = 35`).
    pub const REINFORCED: PayloadConveyorBehavior = PayloadConveyorBehavior {
        move_time: 35.0,
        payload_limit: 3.0,
    };
}

pub(crate) fn tile_entity(world: &World, x: i32, y: i32) -> Option<Entity> {
    world.get_resource::<TileBuilds>()?.get(x, y)
}

pub(crate) fn ntrns(size: i32) -> i32 {
    size / 2 + 1
}

/// Resolves the aligned `next` payload conveyor (`PayloadConveyor.onProximityUpdate`).
pub(crate) fn resolve_next(world: &World, e: Entity) -> Option<Entity> {
    let building = world.get::<Building>(e)?;
    let table = world.get_resource::<BlockTable>()?;
    let size = table.get(building.block)?.def.size.max(1);
    let (dx, dy) = crate::world::blocks::autotiler::d4(building.rotation);
    let dist = ntrns(size);
    let accept = tile_entity(
        world,
        building.tile.x() as i32 + dx * dist,
        building.tile.y() as i32 + dy * dist,
    )?;
    let accept_building = world.get::<Building>(accept)?;
    let accept_size = table.get(accept_building.block)?.def.size.max(1);
    if accept_size == size {
        let expected_x = building.tile.x() as i32 + dx * size;
        let expected_y = building.tile.y() as i32 + dy * size;
        if expected_x == accept_building.tile.x() as i32
            && expected_y == accept_building.tile.y() as i32
        {
            return Some(accept);
        }
    }
    None
}

impl BuildingBehavior for PayloadConveyorBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<PayloadConveyorBuild>(e).is_none() {
            world.entity_mut(e).insert(PayloadConveyorBuild {
                step: -1,
                step_accepted: -1,
                ..PayloadConveyorBuild::default()
            });
        }
        if world.get::<PayloadHolder>(e).is_none() {
            world.entity_mut(e).insert(PayloadHolder::default());
        }
    }

    fn on_proximity_update(&self, world: &mut World, e: Entity) {
        let next = resolve_next(world, e);
        let blocked = {
            let Some(building) = world.get::<Building>(e) else {
                return;
            };
            let table = world.get_resource::<BlockTable>();
            let (dx, dy) = crate::world::blocks::autotiler::d4(building.rotation);
            let size = table
                .and_then(|t| t.get(building.block))
                .map(|i| i.def.size.max(1))
                .unwrap_or(1);
            let dist = 1 + size / 2;
            let solid = tile_entity(
                world,
                building.tile.x() as i32 + dx * dist,
                building.tile.y() as i32 + dy * dist,
            )
            .and_then(|t| world.get::<Building>(t).map(|b| b.block))
            .and_then(|block| table.and_then(|t| t.get(block)))
            .is_some();
            solid && next.is_none()
        };
        if let Some(mut conveyor) = world.get_mut::<PayloadConveyorBuild>(e) {
            conveyor.next = next;
            conveyor.blocked = blocked;
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(mut conveyor) = world.get::<PayloadConveyorBuild>(e).cloned() else {
            return;
        };
        let enabled = world.get::<Building>(e).is_some_and(|b| b.enabled);
        let time = build_time(world);
        if world
            .get::<PayloadHolder>(e)
            .is_some_and(|h| h.payload.is_some())
        {
            update_payload(world, e);
        }

        if enabled {
            conveyor.last_interp = conveyor.cur_interp;
            conveyor.cur_interp = conveyor.progress / self.move_time;
            if conveyor.last_interp > conveyor.cur_interp {
                conveyor.last_interp = 0.0;
            }
            conveyor.progress = time % self.move_time;
        }

        update_payload(world, e);

        let item = world.get::<PayloadHolder>(e).and_then(|h| h.payload);
        if item.is_some()
            && conveyor.next.is_none()
            && let Some(payload) = item
        {
            let progress = conveyor.progress / self.move_time;
            super::push_output(world, payload, progress);
        }

        if enabled {
            let cur_step = (time / self.move_time) as i32;
            if cur_step > conveyor.step {
                let valid = conveyor.step != -1;
                conveyor.step = cur_step;
                if valid && conveyor.step_accepted != cur_step && item.is_some() {
                    let payload = item.unwrap_or_default();
                    if let Some(next) = conveyor.next {
                        let accepted = dispatch_accept_payload(world, next, e, payload);
                        if accepted {
                            dispatch_handle_payload(world, next, e, payload);
                            take_from_holder(world, e);
                        }
                    }
                }
            }
        }
        world.entity_mut(e).insert(conveyor);
    }

    fn accept_payload(
        &self,
        world: &World,
        e: Entity,
        source: Entity,
        payload: PayloadRef,
    ) -> bool {
        let empty = world
            .get::<PayloadHolder>(e)
            .is_none_or(|h| h.payload.is_none());
        let progress = world
            .get::<PayloadConveyorBuild>(e)
            .map(|c| c.progress)
            .unwrap_or(0.0);
        let enabled = world.get::<Building>(e).is_some_and(|b| b.enabled);
        empty
            && payload_fits(world, payload, self.payload_limit)
            && (source == e || (enabled && progress <= 5.0))
    }

    fn handle_payload(&self, world: &mut World, e: Entity, source: Entity, payload: PayloadRef) {
        handle_payload(world, e, source, payload);
        let rotation = world
            .get::<crate::entities::comp::Pos>(source)
            .map(|_| 0.0)
            .unwrap_or(0.0);
        let step = world
            .get::<PayloadConveyorBuild>(e)
            .map(|c| c.step)
            .unwrap_or(0);
        let _ = rotation;
        if let Some(mut conveyor) = world.get_mut::<PayloadConveyorBuild>(e) {
            conveyor.step_accepted = step;
            conveyor.item_rotation = 0.0;
            conveyor.animation = 0.0;
        }
    }

    fn get_payload(&self, world: &World, e: Entity) -> Option<PayloadRef> {
        world.get::<PayloadHolder>(e).and_then(|h| h.payload)
    }

    fn take_payload(&self, world: &mut World, e: Entity) -> Option<PayloadRef> {
        super::take_payload(world, e)
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        0
    }

    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        let conveyor = world
            .get::<PayloadConveyorBuild>(e)
            .copied()
            .unwrap_or_default();
        w.f(conveyor.progress);
        w.f(conveyor.item_rotation);
        let payload = world.get::<PayloadHolder>(e).and_then(|h| h.payload);
        super::payload_write(world, payload, w);
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        _revision: u8,
    ) {
        let mut conveyor = world
            .get::<PayloadConveyorBuild>(e)
            .copied()
            .unwrap_or_default();
        if let Ok(progress) = r.f() {
            conveyor.progress = progress;
        }
        if let Ok(rotation) = r.f() {
            conveyor.item_rotation = rotation;
        }
        world.entity_mut(e).insert(conveyor);
        if world.get::<PayloadHolder>(e).is_none() {
            world.entity_mut(e).insert(PayloadHolder::default());
        }
    }
}

fn take_from_holder(world: &mut World, e: Entity) {
    if let Some(mut holder) = world.get_mut::<PayloadHolder>(e) {
        holder.payload = None;
    }
}

/// Dispatches `acceptPayload` to `target`'s behavior.
pub fn dispatch_accept_payload(
    world: &World,
    target: Entity,
    source: Entity,
    payload: PayloadRef,
) -> bool {
    let Some(block) = world.get::<Building>(target).map(|b| b.block) else {
        return false;
    };
    match world
        .get_resource::<BlockTable>()
        .and_then(|table| table.instance(block))
    {
        Some(inst) => inst.behavior.accept_payload(world, target, source, payload),
        None => false,
    }
}

/// Dispatches `handlePayload` to `target`'s behavior.
pub fn dispatch_handle_payload(
    world: &mut World,
    target: Entity,
    source: Entity,
    payload: PayloadRef,
) {
    let Some(block) = world.get::<Building>(target).map(|b| b.block) else {
        return;
    };
    let Some(inst) = world
        .get_resource::<BlockTable>()
        .and_then(|table| table.instance(block))
    else {
        return;
    };
    inst.behavior.handle_payload(world, target, source, payload);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;
    use crate::world::TilePos;

    #[test]
    fn payload_conveyor_hands_off_to_next() {
        let mut harness = BuildHarness::new(32, 32, 7);
        let container = harness.content().block_id("container").expect("container");
        let conveyor = harness
            .content()
            .block_id("payload-conveyor")
            .expect("payload-conveyor");
        // size-3 conveyors: center (6,6) footprint (5,5)..(7,7); (9,6) -> (8,5)..(10,7).
        // Place the downstream conveyor first so the upstream one resolves `next`.
        assert!(harness.place(9, 6, conveyor, 0, true));
        assert!(harness.place(6, 6, conveyor, 0, true));
        let first = harness.build_at(6, 6).expect("first");
        let entity =
            super::super::create_build_payload(&mut harness.world, container, 0).expect("payload");
        // Attach the payload to the first conveyor.
        let payload = PayloadRef {
            entity: Some(entity),
            content: container.raw(),
            is_block: true,
        };
        handle_payload(&mut harness.world, first, first, payload);
        for _ in 0..200 {
            harness.tick();
        }
        let second = harness.build_at(9, 6).expect("second");
        let on_second = harness
            .world
            .get::<PayloadHolder>(second)
            .and_then(|h| h.payload);
        assert!(on_second.is_some(), "payload should have been handed off");
        let on_first = harness
            .world
            .get::<PayloadHolder>(first)
            .and_then(|h| h.payload);
        assert!(
            on_first.is_none(),
            "payload should have left the first conveyor"
        );
        let _ = TilePos::EMPTY;
    }
}
