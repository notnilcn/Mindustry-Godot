// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BuildPayload` operations (`world/blocks/payloads/BuildPayload.java`) —
//! plan 08 M6 remainder/M7.
//!
//! The carried entity already exists (a `Building` with `TilePos::EMPTY`); this
//! module owns placement ([`dump_build_payload`]) and the requirement/build-time
//! reads used by the deconstructor and constructor.

pub use super::payload_block::{PayloadPlaceHook, PayloadPlacement, dump_build_payload};
pub use super::{payload_build_time, payload_destroyed, payload_requirements};

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::world::behavior::PayloadRef;

/// `BuildPayload.contentEquals(other)`.
pub fn content_equals(world: &World, a: PayloadRef, b: PayloadRef) -> bool {
    match (
        super::build_payload_block(world, a),
        super::build_payload_block(world, b),
    ) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// `BuildPayload.set(x, y, rotation)` — build payloads ignore rotation.
pub fn set_position(world: &mut World, payload: PayloadRef, x: f32, y: f32) {
    if let Some(entity) = payload.entity
        && let Some(mut pos) = world.get_mut::<crate::entities::comp::Pos>(entity)
    {
        pos.x = x;
        pos.y = y;
    }
}

/// `BuildPayload.block()`.
pub fn block_of(world: &World, payload: PayloadRef) -> Option<crate::content::BlockId> {
    super::build_payload_block(world, payload)
}

/// Raw entity handle (identity helper for callers).
pub fn build_entity(payload: PayloadRef) -> Option<Entity> {
    payload.entity
}
