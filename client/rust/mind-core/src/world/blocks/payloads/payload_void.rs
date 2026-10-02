// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PayloadVoid` build behavior (`world/blocks/payloads/PayloadVoid.java`) —
//! plan 08 M7. Swallows any build/unit payload that reaches its center.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::entities::comp::Building;
use crate::world::behavior::{BuildingBehavior, PayloadRef};

use super::PayloadHolder;
use super::payload_block::{consume_payload_entity, move_in_payload, payload_block_update};

/// `PayloadVoid` behavior (`payload-void`).
#[derive(Debug, Clone, Copy, Default)]
pub struct PayloadVoidBehavior {
    /// `PayloadVoid.maxPayloadSize` (unbounded in vanilla; sized to the block).
    pub max_payload_size: f32,
}

impl PayloadVoidBehavior {
    /// Sandbox `payload-void`.
    pub const VANILLA: PayloadVoidBehavior = PayloadVoidBehavior {
        max_payload_size: 5.0,
    };
}

impl BuildingBehavior for PayloadVoidBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<PayloadHolder>(e).is_none() {
            world.entity_mut(e).insert(PayloadHolder::default());
        }
    }

    fn accept_payload(
        &self,
        _world: &World,
        e: Entity,
        _source: Entity,
        payload: PayloadRef,
    ) -> bool {
        let _ = e;
        let _ = payload;
        true
    }

    fn handle_payload(&self, world: &mut World, e: Entity, source: Entity, payload: PayloadRef) {
        super::handle_payload(world, e, source, payload);
    }

    fn get_payload(&self, world: &World, e: Entity) -> Option<PayloadRef> {
        world.get::<PayloadHolder>(e).and_then(|h| h.payload)
    }

    fn take_payload(&self, world: &mut World, e: Entity) -> Option<PayloadRef> {
        super::take_payload(world, e)
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        payload_block_update(world, e);
        let efficiency = world
            .get::<Building>(e)
            .map(|b| b.efficiency)
            .unwrap_or(0.0);
        if !move_in_payload(world, e, false) || efficiency <= 0.0 {
            return;
        }
        if let Some(payload) = world.get::<PayloadHolder>(e).and_then(|h| h.payload) {
            consume_payload_entity(world, payload);
            if let Some(mut holder) = world.get_mut::<PayloadHolder>(e) {
                holder.payload = None;
            }
        }
    }

    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        super::payload_block::write_base(world, e, w);
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        _revision: u8,
    ) {
        super::payload_block::read_base(world, e, r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;

    #[test]
    fn void_swallows_payload() {
        let mut harness = BuildHarness::new(16, 16, 7);
        let container = harness.content().block_id("container").expect("container");
        let void_block = harness
            .content()
            .block_id("payload-void")
            .expect("payload-void");
        assert!(harness.place(6, 6, void_block, 0, true));
        let e = harness.build_at(6, 6).expect("void");
        let payload_entity =
            super::super::create_build_payload(&mut harness.world, container, 0).expect("payload");
        let payload = PayloadRef {
            entity: Some(payload_entity),
            content: container.raw(),
            is_block: true,
        };
        super::super::handle_payload(&mut harness.world, e, e, payload);
        harness.tick();
        assert!(
            harness
                .world
                .get::<PayloadHolder>(e)
                .is_some_and(|h| h.payload.is_none())
        );
        assert!(harness.world.get_entity(payload_entity).is_err());
    }
}
