// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Payload system core (`world/blocks/payloads/Payload.java`,
//! `PayloadBlock.java`) — plan 08 M6.
//!
//! This module is the frozen payload API consumed by plan 10
//! (`PayloadAmmoTurret`) and plan 11 (`CargoAI`, carrying units). A carried
//! payload is a real ECS entity (deviation L2): a `Building` with
//! `TilePos::EMPTY` (BuildPayload) or a unit entity (UnitPayload), both marked
//! with [`PayloadCarried`]. Holders store a [`PayloadRef`] plus the `pay_vector`
//! / `pay_rotation` movement state.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockId, ItemId};
use crate::world::block::TILE_SIZE;
use crate::world::modules::ModuleDims;

pub use crate::world::behavior::PayloadRef;

pub mod block_producer;
pub mod build_payload;
pub mod constructor;
pub mod payload_block;
pub mod payload_conveyor;
pub mod payload_deconstructor;
pub mod payload_loader;
pub mod payload_mass_driver;
pub mod payload_router;
pub mod payload_source;
pub mod payload_unloader;
pub mod payload_void;

use std::sync::Arc;

use crate::content::ContentRegistry;
use crate::world::behavior::BehaviorRegistry;

pub use payload_block::{PayloadPlaceHook, PayloadPlacement};
pub use payload_conveyor::{PayloadConveyorBehavior, PayloadConveyorBuild};
pub use payload_conveyor::{dispatch_accept_payload, dispatch_handle_payload};

/// Registers the payload-family behaviors available at this milestone.
pub fn register(registry: &mut BehaviorRegistry, _content: &ContentRegistry) {
    registry.register_named(
        "payload-conveyor",
        Arc::new(PayloadConveyorBehavior::VANILLA),
    );
    registry.register_named(
        "reinforced-payload-conveyor",
        Arc::new(PayloadConveyorBehavior::REINFORCED),
    );
    registry.register_named(
        "payload-router",
        Arc::new(payload_router::PayloadRouterBehavior::VANILLA),
    );
    registry.register_named(
        "reinforced-payload-router",
        Arc::new(payload_router::PayloadRouterBehavior::REINFORCED),
    );
    registry.register_named(
        "payload-mass-driver",
        Arc::new(payload_mass_driver::PayloadMassDriverBehavior::VANILLA),
    );
    registry.register_named(
        "large-payload-mass-driver",
        Arc::new(payload_mass_driver::PayloadMassDriverBehavior::LARGE),
    );
    registry.register_named(
        "small-deconstructor",
        Arc::new(payload_deconstructor::PayloadDeconstructorBehavior::SMALL),
    );
    registry.register_named(
        "deconstructor",
        Arc::new(payload_deconstructor::PayloadDeconstructorBehavior::LARGE),
    );
    registry.register_named(
        "constructor",
        Arc::new(constructor::ConstructorBehavior::VANILLA),
    );
    registry.register_named(
        "large-constructor",
        Arc::new(constructor::ConstructorBehavior::LARGE),
    );
    registry.register_named(
        "payload-loader",
        Arc::new(payload_loader::PayloadLoaderBehavior::VANILLA),
    );
    registry.register_named(
        "payload-unloader",
        Arc::new(payload_unloader::PayloadUnloaderBehavior::VANILLA),
    );
    registry.register_named(
        "payload-source",
        Arc::new(payload_source::PayloadSourceBehavior::VANILLA),
    );
    registry.register_named(
        "payload-void",
        Arc::new(payload_void::PayloadVoidBehavior::VANILLA),
    );
}

/// `Payload` type tags (`payloadUnit = 0`, `payloadBlock = 1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PayloadKind {
    /// A unit payload (`payloadUnit = 0`).
    #[default]
    Unit,
    /// A building payload (`payloadBlock = 1`).
    Build,
}

impl PayloadKind {
    /// Java type tag.
    pub const fn tag(self) -> u8 {
        match self {
            PayloadKind::Unit => 0,
            PayloadKind::Build => 1,
        }
    }

    /// Decodes a Java type tag.
    pub const fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(PayloadKind::Unit),
            1 => Some(PayloadKind::Build),
            _ => None,
        }
    }
}

/// `PayloadUnit` marker on a carried unit entity (`PayloadCarried` counterpart).
#[derive(Debug, Clone, Copy, Component, Default)]
pub struct CarriedUnit;

/// Marker on a carried build entity (its `Building.tile` is `TilePos::EMPTY`).
#[derive(Debug, Clone, Copy, Component, Default)]
pub struct CarriedBuild;

/// `PayloadCarried` marker on the carried entity (`Payload` holder backref).
#[derive(Debug, Clone, Copy, Component)]
pub struct PayloadCarried {
    /// Holder entity (`Unit` or `Building` carrying this payload).
    pub holder: Entity,
    /// Payload kind.
    pub kind: PayloadKind,
}

/// `PayloadBlock.PayloadBlockBuild` holder state.
#[derive(Debug, Clone, Copy, Component)]
pub struct PayloadHolder {
    /// Held payload (`payload`).
    pub payload: Option<PayloadRef>,
    /// Local offset from the holder center (`payVector`).
    pub pay_vector: (f32, f32),
    /// Payload rotation in degrees (`payRotation`).
    pub pay_rotation: f32,
    /// Picked-up flag (`carried`; view hint).
    pub carried: bool,
}

impl Default for PayloadHolder {
    fn default() -> Self {
        Self {
            payload: None,
            pay_vector: (0.0, 0.0),
            pay_rotation: 0.0,
            carried: false,
        }
    }
}

/// Builds a payload handle.
pub fn payload_ref(kind: PayloadKind, entity: Entity, content: u16) -> PayloadRef {
    PayloadRef {
        entity: Some(entity),
        content,
        is_block: kind == PayloadKind::Build,
    }
}

/// Kind of a payload handle.
pub fn kind_of(payload: PayloadRef) -> PayloadKind {
    if payload.is_block {
        PayloadKind::Build
    } else {
        PayloadKind::Unit
    }
}

/// `Payload.size()` — `block.size * tilesize` for builds; unit hit size for
/// units (`0.0` until plan 11 supplies unit metadata).
pub fn payload_size(world: &World, payload: PayloadRef) -> f32 {
    let Some(entity) = payload.entity else {
        return 0.0;
    };
    if payload.is_block {
        world
            .get::<crate::entities::comp::Building>(entity)
            .zip(world.get_resource::<crate::world::block::BlockTable>())
            .and_then(|(building, table)| table.get(building.block).map(|inst| inst.def.size))
            .map(|size| size as f32 * TILE_SIZE)
            .unwrap_or(0.0)
    } else {
        world
            .get::<UnitPayloadSize>(entity)
            .map(|s| s.0)
            .unwrap_or(0.0)
    }
}

/// Unit payload hit size in world units (plan 11 sets this from `UnitType`).
#[derive(Debug, Clone, Copy, Component)]
pub struct UnitPayloadSize(pub f32);

/// `Payload.fits(s)` (`size()/tilesize <= s`).
pub fn payload_fits(world: &World, payload: PayloadRef, limit: f32) -> bool {
    payload_size(world, payload) / TILE_SIZE <= limit
}

/// Block content of a build payload.
pub fn build_payload_block(world: &World, payload: PayloadRef) -> Option<BlockId> {
    if !payload.is_block {
        return None;
    }
    payload
        .entity
        .and_then(|e| world.get::<crate::entities::comp::Building>(e))
        .map(|b| b.block)
}

/// Spawns a carried build payload entity for `block` (`new BuildPayload(block)`).
///
/// The building is created with an empty tile (deviation L2); the caller attaches
/// it to a holder through [`handle_payload`].
pub fn create_build_payload(world: &mut World, block: BlockId, team: u8) -> Option<Entity> {
    let (items, liquids) = world
        .get_resource::<ModuleDims>()
        .map(|d| (d.items, d.liquids))
        .unwrap_or((0, 0));
    let inst = world
        .get_resource::<crate::world::block::BlockTable>()?
        .instance(block)?;
    let entity = inst.spawn(
        world,
        0,
        crate::world::TilePos::EMPTY,
        team,
        0,
        items,
        liquids,
    );
    world.entity_mut(entity).insert(CarriedBuild);
    Some(entity)
}

/// `Payload.write` (M6 shape: `bool` presence + `kind:u8` + block/unit tag).
pub fn payload_write(
    world: &World,
    payload: Option<PayloadRef>,
    w: &mut crate::world::BuildingWriter,
) {
    let Some(payload) = payload else {
        w.bool(false);
        return;
    };
    w.bool(true);
    w.b(payload.kind_tag() as i8);
    if payload.is_block {
        w.s(build_payload_block(world, payload)
            .map(|b| b.raw() as i16)
            .unwrap_or(-1));
        w.b(0);
    } else {
        w.b(payload.content as i8);
    }
}

impl PayloadRef {
    /// Java payload type tag.
    pub fn kind_tag(self) -> u8 {
        if self.is_block { 1 } else { 0 }
    }
}

/// Item requirements of a payload (`BuildPayload.requirements()` = the carried
/// block's `requirements`; unit requirements are plan 11's `UnitType`).
pub fn payload_requirements(world: &World, payload: PayloadRef) -> Vec<(ItemId, i32)> {
    if !payload.is_block {
        return Vec::new();
    }
    let Some(block) = payload
        .entity
        .and_then(|e| world.get::<crate::entities::comp::Building>(e))
        .map(|b| b.block)
    else {
        return Vec::new();
    };
    world
        .get_resource::<crate::world::block::BlockTable>()
        .and_then(|table| table.get(block).map(|inst| inst.def.requirements.clone()))
        .map(|reqs| reqs.into_iter().map(|s| (s.item, s.amount)).collect())
        .unwrap_or_default()
}

/// `Payload.buildTime()` (`BuildPayload` = carried block `buildTime`).
pub fn payload_build_time(world: &World, payload: PayloadRef) -> f32 {
    if !payload.is_block {
        return 0.0;
    }
    payload
        .entity
        .and_then(|e| world.get::<crate::entities::comp::Building>(e))
        .and_then(|b| {
            world
                .get_resource::<crate::world::block::BlockTable>()
                .and_then(|table| table.get(b.block).map(|inst| inst.def.build_time))
        })
        .unwrap_or(0.0)
}

/// `Payload.destroyed()` (`BuildPayload`: mark dead + run destroy hooks).
pub fn payload_destroyed(world: &mut World, payload: PayloadRef) {
    let Some(entity) = payload.entity else {
        return;
    };
    if let Some(block) = world
        .get::<crate::entities::comp::Building>(entity)
        .map(|b| b.block)
    {
        if let Some(inst) = world
            .get_resource::<crate::world::block::BlockTable>()
            .and_then(|table| table.instance(block))
        {
            inst.behavior.on_destroyed(world, entity);
            inst.behavior.after_destroyed(world, entity);
        }
    }
    world.entity_mut(entity).despawn();
}

/// `Payload.fits` limit helper: `size / tilesize`.
pub fn payload_blocks(world: &World, payload: PayloadRef) -> f32 {
    payload_size(world, payload) / TILE_SIZE
}

/// Attaches `payload` to `holder` (`PayloadBlockBuild.handlePayload`).
pub fn handle_payload(world: &mut World, holder: Entity, source: Entity, payload: PayloadRef) {
    let kind = kind_of(payload);
    if let Some(entity) = payload.entity {
        world
            .entity_mut(entity)
            .insert(PayloadCarried { holder, kind });
    }
    let source_pos = world
        .get::<crate::entities::comp::Pos>(source)
        .map(|p| (p.x, p.y));
    let holder_pos = world
        .get::<crate::entities::comp::Pos>(holder)
        .map(|p| (p.x, p.y));
    if let Some(mut holder_state) = world.get_mut::<PayloadHolder>(holder) {
        holder_state.payload = Some(payload);
        if let (Some(source_pos), Some(holder_pos)) = (source_pos, holder_pos) {
            holder_state.pay_vector = (source_pos.0 - holder_pos.0, source_pos.1 - holder_pos.1);
        }
    }
}

/// Detaches and returns the payload held by `holder` (`takePayload`).
pub fn take_payload(world: &mut World, holder: Entity) -> Option<PayloadRef> {
    let payload = world.get::<PayloadHolder>(holder).and_then(|h| h.payload)?;
    if let Some(entity) = payload.entity {
        world.entity_mut(entity).remove::<PayloadCarried>();
    }
    if let Some(mut holder_state) = world.get_mut::<PayloadHolder>(holder) {
        holder_state.payload = None;
    }
    Some(payload)
}

/// `PayloadBlock.updatePayload`: syncs the payload entity transform from the
/// holder's `pay_vector`/`pay_rotation`.
pub fn update_payload(world: &mut World, holder: Entity) {
    let Some((payload, pay_vector)) = world
        .get::<PayloadHolder>(holder)
        .map(|state| (state.payload, state.pay_vector))
    else {
        return;
    };
    let Some(payload) = payload else {
        return;
    };
    let Some(entity) = payload.entity else {
        return;
    };
    let Some(pos) = world
        .get::<crate::entities::comp::Pos>(holder)
        .map(|p| (p.x, p.y))
    else {
        return;
    };
    if let Some(mut payload_pos) = world.get_mut::<crate::entities::comp::Pos>(entity) {
        payload_pos.x = pos.0 + pay_vector.0;
        payload_pos.y = pos.1 + pay_vector.1;
    }
}

/// `PayloadBlock.pushOutput`: pushes nearby grounded units when a payload is
/// ejected (`progress >= 0.55`). Unit physics is plan 11; this is the seam.
pub fn push_output(_world: &mut World, _payload: PayloadRef, _progress: f32) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;

    #[test]
    fn payload_kind_tags_match_java() {
        assert_eq!(PayloadKind::Unit.tag(), 0);
        assert_eq!(PayloadKind::Build.tag(), 1);
        assert_eq!(PayloadKind::from_tag(1), Some(PayloadKind::Build));
        assert_eq!(PayloadKind::from_tag(9), None);
    }

    #[test]
    fn carried_build_has_empty_tile_and_marker() {
        let mut harness = BuildHarness::new(16, 16, 7);
        let container = harness.content().block_id("container").expect("container");
        let entity = create_build_payload(&mut harness.world, container, 0).expect("payload");
        assert!(harness.world.get::<CarriedBuild>(entity).is_some());
        let building = harness.world.get::<crate::entities::comp::Building>(entity);
        assert!(building.is_some_and(|b| b.tile == crate::world::TilePos::EMPTY));
        let size = payload_size(
            &harness.world,
            payload_ref(PayloadKind::Build, entity, container.raw()),
        );
        assert_eq!(size, 2.0 * TILE_SIZE);
    }
}
