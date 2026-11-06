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

use crate::content::{BlockId, ItemId, UnitTypeId};
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

/// `Payload.size()` — `block.size * tilesize` for builds; the carried unit's
/// `UnitType.hitSize` for units ([`UnitPayloadSize`], set on carry/read).
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

/// Unit payload hit size in world units (`UnitPayload.size()` = `unit.hitSize`).
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

/// `Payload.write` (`Payload.java:87-118`).
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
        // `b build.version(); build.writeAll(write)` — the nested building
        // chunk (base with the embedded version + per-kind body).
        let Some(entity) = payload.entity else {
            w.b(0);
            return;
        };
        w.b(crate::world::building_io::behavior_version(world, entity) as i8);
        let _ = crate::world::building_io::write_all_body(world, entity, w, false);
    } else {
        // `UnitPayload.write` (`UnitPayload.java:65-69`): the payload tag, a
        // unit class id and the generated `unit.write` body. The port has no
        // generated class ids, so native saves store the content id behind
        // `NATIVE_UNIT_MARKER` followed by the shared unit record
        // (`io::sim_io::write_unit_fields`; the same field set as the live
        // `SIM_UNIT_CLASS_ID` chunk).
        payload_block::write_unit_payload(world, payload, w);
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
        && let Some(inst) = world
            .get_resource::<crate::world::block::BlockTable>()
            .and_then(|table| table.instance(block))
    {
        inst.behavior.on_destroyed(world, entity);
        inst.behavior.after_destroyed(world, entity);
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
        if kind == PayloadKind::Unit {
            // `UnitPayload`s stay out of `Groups.unit` until `dump()` calls
            // `unit.add()`; the `Unit` marker is that membership key, so a
            // carried unit must not carry it (otherwise every live-unit query,
            // turret target pass and save unit row would see it).
            let hit_size = world
                .get::<UnitPayloadSize>(entity)
                .map(|size| size.0)
                .or_else(|| {
                    world
                        .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
                        .and_then(|runtime| runtime.content.unit(UnitTypeId::new(payload.content)))
                        .map(|def| def.hit_size)
                });
            world
                .entity_mut(entity)
                .remove::<crate::entities::comp::Unit>();
            world.entity_mut(entity).insert(CarriedUnit);
            if let Some(hit_size) = hit_size {
                world.entity_mut(entity).insert(UnitPayloadSize(hit_size));
            }
        }
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
    let Some((payload, pay_vector, pay_rotation)) = world
        .get::<PayloadHolder>(holder)
        .map(|state| (state.payload, state.pay_vector, state.pay_rotation))
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
    // `Payload.set(x, y, rotation)` (`PayloadBlock.java:196`): a unit payload
    // also takes the holder rotation (`UnitPayload.set` sets `unit.rotation`).
    if !payload.is_block
        && let Some(mut core) = world.get_mut::<crate::entities::comp::unit::comp::UnitCore>(entity)
    {
        core.rotation = pay_rotation;
    }
}

/// `PayloadBlock.pushOutput` (`PayloadBlock.java:80-96`): pushes nearby
/// grounded units out of the way while a payload is being ejected
/// (`progress >= 0.55`), matching the payload's leg-step layer.
pub fn push_output(world: &mut World, payload: PayloadRef, progress: f32) {
    const THRESH: f32 = 0.55;
    if progress < THRESH {
        return;
    }
    let Some(entity) = payload.entity else {
        return;
    };
    let Some(payload_pos) = world
        .get::<crate::entities::comp::Pos>(entity)
        .map(|pos| (pos.x, pos.y))
    else {
        return;
    };
    let size = payload_size(world, payload);
    let radius = size / 2.0;
    let scl = (((progress - THRESH) / (1.0 - THRESH)) * 1.1).clamp(0.0, 1.0);
    let leg_step = if payload.is_block {
        false
    } else {
        world
            .get::<crate::entities::comp::unit::comp::UnitTypeComp>(entity)
            .and_then(|comp| {
                world
                    .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
                    .and_then(|runtime| runtime.content.unit(comp.type_id))
            })
            .is_some_and(|def| def.allow_leg_step)
    };
    // Collect the pushes under the (immutable) runtime borrow, then apply them.
    let mut pushes: Vec<(Entity, f32, f32)> = Vec::new();
    {
        let Some(runtime) = world.get_non_send::<crate::sim::unit_runtime::UnitRuntime>() else {
            return;
        };
        for other in runtime.units.iter().copied() {
            if other == entity || world.get_entity(other).is_err() {
                continue;
            }
            let Some(pos) = world.get::<crate::entities::comp::Pos>(other) else {
                continue;
            };
            let (dx, dy) = (pos.x - payload_pos.0, pos.y - payload_pos.1);
            let dst = (dx * dx + dy * dy).sqrt();
            let rs = radius
                + world
                    .get::<crate::entities::comp::unit::comp::HitboxComp>(other)
                    .map_or(0.0, |hitbox| hitbox.hit_size / 2.0);
            if dst >= rs || !crate::combat::bullet::unit_is_grounded(world, other) {
                continue;
            }
            let other_leg = world
                .get::<crate::entities::comp::unit::comp::UnitTypeComp>(other)
                .and_then(|comp| runtime.content.unit(comp.type_id))
                .is_some_and(|def| def.allow_leg_step);
            if other_leg != leg_step {
                continue;
            }
            // `Tmp.v1.set(dx, dy).setLength(min(rs - dst, 1f)).scl(scl)`.
            let len = (rs - dst).min(1.0) * scl;
            let (nx, ny) = if dst > 1.0e-6 {
                (dx / dst * len, dy / dst * len)
            } else {
                (0.0, 0.0)
            };
            pushes.push((other, nx, ny));
        }
    }
    for (other, nx, ny) in pushes {
        if let Some(mut vel) = world.get_mut::<crate::entities::comp::Vel>(other) {
            vel.x += nx;
            vel.y += ny;
        }
    }
}

/// The carried unit entity of `holder`, if its payload is a unit
/// (`PayloadBlockBuild.drawPayload` render seam: the entity's `Pos`/`UnitCore`
/// rotation are kept in sync by [`update_payload`]).
pub fn carried_unit(world: &World, holder: Entity) -> Option<Entity> {
    let payload = world.get::<PayloadHolder>(holder)?.payload?;
    (!payload.is_block)
        .then_some(payload.entity?)
        .filter(|entity| world.get_entity(*entity).is_ok())
}

/// `Payload.size()` of `holder`'s payload in world units (the carried-unit
/// draw/hitbox extent).
pub fn held_payload_size(world: &World, holder: Entity) -> Option<f32> {
    let payload = world.get::<PayloadHolder>(holder)?.payload?;
    Some(payload_size(world, payload))
}

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
