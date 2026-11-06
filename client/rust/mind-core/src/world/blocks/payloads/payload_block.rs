// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Shared payload-holder movement (`world/blocks/payloads/PayloadBlock.java`) —
//! plan 08 M6 remainder/M7.
//!
//! Ports `PayloadBlockBuild`'s `moveInPayload`/`moveOutPayload`/`dumpPayload`/
//! `pushOutput`/`hasArrived` helpers. A carried payload is a real ECS entity
//! (plan 08 L2); the holder stores a `PayloadRef` plus `pay_vector`/
//! `pay_rotation`. Drawing is plan 16.

use std::sync::Arc;

use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;

use crate::content::BlockId;
use crate::entities::comp::{Building, Pos};
use crate::world::behavior::PayloadRef;
use crate::world::block::{BlockTable, TILE_SIZE};
use crate::world::blocks::distribution::transfer;
use crate::world::tile_index::TileBuilds;
use crate::world::update::delta;

use super::{
    PayloadHolder, PayloadKind, dispatch_accept_payload, dispatch_handle_payload, kind_of,
    payload_size, update_payload,
};

/// Grid placement seam for `BuildPayload.place` (`PayloadPlacement`).
///
/// Java writes through `tile.setBlock(...)`; Rust behaviors only hold the ECS
/// `World`, so the authoritative grid mutation is provided by the host
/// (`BuildHarness`/`Sim`). When no hook is installed a best-effort ECS-only
/// placement updates the building's tile and the `TileBuilds` mirror.
pub trait PayloadPlacement: Send + Sync {
    /// Places `entity` (a carried `BuildPayload`) at the tile implied by its
    /// current position. Returns whether it was placed.
    fn place_payload(&self, world: &mut World, entity: Entity, block: BlockId) -> bool;
}

/// Installed particle grid-placement hook.
#[derive(Resource, Default, Clone)]
pub struct PayloadPlaceHook(pub Option<Arc<dyn PayloadPlacement>>);

/// `Block.rotate`.
pub fn block_rotates(world: &World, block: BlockId) -> bool {
    world
        .get_resource::<BlockTable>()
        .and_then(|table| table.get(block))
        .is_some_and(|inst| inst.rotate)
}

/// `Block.size`.
pub fn block_size(world: &World, block: BlockId) -> i32 {
    world
        .get_resource::<BlockTable>()
        .and_then(|table| table.get(block).map(|inst| inst.def.size))
        .unwrap_or(1)
        .max(1)
}

/// `Building.rotdeg()`.
pub fn rot_deg(world: &World, e: Entity) -> f32 {
    world
        .get::<Building>(e)
        .map(|b| b.rotation as f32 * 90.0)
        .unwrap_or(0.0)
}

fn trnsx(deg: f32) -> f32 {
    deg.to_radians().cos()
}

fn trnsy(deg: f32) -> f32 {
    deg.to_radians().sin()
}

/// `Angles.moveToward(from, to, speed)`.
pub fn move_toward_angle(from: f32, to: f32, speed: f32) -> f32 {
    let mut delta = (to - from).rem_euclid(360.0);
    if delta > 180.0 {
        delta -= 360.0;
    }
    if delta.abs() <= speed || speed == 0.0 {
        to.rem_euclid(360.0)
    } else {
        (from + delta.signum() * speed).rem_euclid(360.0)
    }
}

/// `Vec2.approach(target, speed)` per component.
fn approach(value: f32, target: f32, speed: f32) -> f32 {
    let diff = target - value;
    if diff.abs() <= speed {
        target
    } else {
        value + diff.signum() * speed
    }
}

/// `PayloadBlockBuild.hasArrived()`.
pub fn has_arrived(world: &World, e: Entity) -> bool {
    world
        .get::<PayloadHolder>(e)
        .is_some_and(|h| h.pay_vector.0.abs() < 0.01 && h.pay_vector.1.abs() < 0.01)
}

/// The held payload's size in tiles (`payload.size()/tilesize`); holder block
/// size when there is no payload.
pub fn held_size_tiles(world: &World, e: Entity) -> f32 {
    let Some(holder) = world.get::<PayloadHolder>(e) else {
        return 1.0;
    };
    match holder.payload {
        Some(payload) => payload_size(world, payload) / TILE_SIZE,
        None => world
            .get::<Building>(e)
            .map(|b| block_size(world, b.block) as f32)
            .unwrap_or(1.0),
    }
}

/// `PayloadBlockBuild.updatePayload()` extended to honor `pay_rotation` for
/// carried units; build payloads ignore rotation exactly like upstream.
pub fn sync_payload_transform(world: &mut World, holder: Entity) {
    update_payload(world, holder);
}

/// `PayloadBlockBuild.moveInPayload(rotate)`.
pub fn move_in_payload(world: &mut World, e: Entity, rotate: bool) -> bool {
    let Some(holder) = world.get::<PayloadHolder>(e).copied() else {
        return false;
    };
    if holder.payload.is_none() {
        return false;
    }
    sync_payload_transform(world, e);

    let payload_speed = 0.7f32;
    let rotate_speed = 3.0f32;
    let dt = delta(world, e);
    if rotate {
        let target = if block_rotates(
            world,
            world
                .get::<Building>(e)
                .map(|b| b.block)
                .unwrap_or_default(),
        ) {
            rot_deg(world, e)
        } else {
            90.0
        };
        let (mut vector, mut rotation) = (holder.pay_vector, holder.pay_rotation);
        rotation = move_toward_angle(rotation, target, rotate_speed * dt);
        let step = payload_speed * dt;
        vector.0 = approach(vector.0, 0.0, step);
        vector.1 = approach(vector.1, 0.0, step);
        if let Some(mut state) = world.get_mut::<PayloadHolder>(e) {
            state.pay_vector = vector;
            state.pay_rotation = rotation;
        }
    } else {
        let step = payload_speed * dt;
        if let Some(mut state) = world.get_mut::<PayloadHolder>(e) {
            let (vx, vy) = state.pay_vector;
            state.pay_vector = (approach(vx, 0.0, step), approach(vy, 0.0, step));
        }
    }
    sync_payload_transform(world, e);
    has_arrived(world, e)
}

/// `PayloadBlockBuild.moveOutPayload()`.
pub fn move_out_payload(world: &mut World, e: Entity) {
    let Some(holder) = world.get::<PayloadHolder>(e).copied() else {
        return;
    };
    let Some(payload) = holder.payload else {
        return;
    };
    sync_payload_transform(world, e);

    let size = block_size(
        world,
        world
            .get::<Building>(e)
            .map(|b| b.block)
            .unwrap_or_default(),
    ) as f32;
    let rot = rot_deg(world, e);
    let dest = (
        trnsx(rot) * size * TILE_SIZE / 2.0,
        trnsy(rot) * size * TILE_SIZE / 2.0,
    );
    let dt = delta(world, e);
    let step = 0.7f32 * dt;

    let (mut vector, mut rotation) = (holder.pay_vector, holder.pay_rotation);
    rotation = move_toward_angle(rotation, rot, 3.0 * dt);
    vector.0 = approach(vector.0, dest.0, step);
    vector.1 = approach(vector.1, dest.1, step);
    if let Some(mut state) = world.get_mut::<PayloadHolder>(e) {
        state.pay_vector = vector;
        state.pay_rotation = rotation;
    }
    sync_payload_transform(world, e);

    let front = transfer::front(world, e);
    let front_block = front.and_then(|f| world.get::<Building>(f).map(|b| b.block));
    let can_dump = front_block.is_none_or(|b| {
        !world
            .get_resource::<BlockTable>()
            .and_then(|t| t.get(b).map(|i| i.def.solid))
            .unwrap_or(false)
    });
    let can_move = front_block.is_some_and(|b| {
        let table = world.get_resource::<BlockTable>();
        table.is_some_and(|t| {
            t.get(b).is_some_and(|i| {
                kind_outputs_payload(i.def.kind) || kind_accepts_payload(i.def.kind)
            })
        })
    });

    if can_dump && !can_move {
        let dist = ((dest.0 - vector.0).powi(2) + (dest.1 - vector.1).powi(2)).sqrt();
        let denom = (size * TILE_SIZE / 2.0).max(f32::EPSILON);
        super::push_output(world, payload, 1.0 - dist / denom);
    }

    let arrived = (dest.0 - vector.0).abs() < 0.001 && (dest.1 - vector.1).abs() < 0.001;
    if arrived {
        if let Some(mut state) = world.get_mut::<PayloadHolder>(e) {
            let half = size * TILE_SIZE / 2.0;
            state.pay_vector = (
                state.pay_vector.0.clamp(-half, half),
                state.pay_vector.1.clamp(-half, half),
            );
        }
        if can_move {
            let Some(front) = front else { return };
            if dispatch_accept_payload(world, front, e, payload) {
                dispatch_handle_payload(world, front, e, payload);
                if let Some(mut state) = world.get_mut::<PayloadHolder>(e) {
                    state.payload = None;
                }
            }
        } else if can_dump {
            dump_payload(world, e);
        }
    }
}

/// `PayloadBlockBuild.dumpPayload()`.
pub fn dump_payload(world: &mut World, e: Entity) {
    let Some(payload) = world.get::<PayloadHolder>(e).and_then(|h| h.payload) else {
        return;
    };
    let Some(entity) = payload.entity else {
        if let Some(mut state) = world.get_mut::<PayloadHolder>(e) {
            state.payload = None;
        }
        return;
    };
    let rotation = world
        .get::<PayloadHolder>(e)
        .map(|h| h.pay_rotation)
        .unwrap_or(0.0);
    // Translate forward slightly (`payload.set(x + tx, y + ty, ...)`).
    if let Some(mut pos) = world.get_mut::<Pos>(entity) {
        pos.x += trnsx(rotation) * 0.1;
        pos.y += trnsy(rotation) * 0.1;
    }
    let dumped = if kind_of(payload) == PayloadKind::Build {
        dump_build_payload(world, payload)
    } else {
        // `UnitPayload.dump` (`UnitPayload.java:100-144`): canCreate +
        // solidity + overlap gates, then `unit.add()`.
        dump_unit_payload(world, entity)
    };
    if dumped {
        if let Some(mut state) = world.get_mut::<PayloadHolder>(e) {
            state.payload = None;
        }
    } else if let Some(mut pos) = world.get_mut::<Pos>(entity) {
        pos.x -= trnsx(rotation) * 0.1;
        pos.y -= trnsy(rotation) * 0.1;
    }
}

/// `UnitPayload.dump` (`UnitPayload.java:100-144`): drops the carried markers
/// and adds the unit to `Groups.unit` through the live runtime. The gates
/// (`Units.canCreate`, `unit.solidity()`, the grounded-overlap count) live in
/// [`crate::sim::unit_runtime::unit_payload_can_dump`].
pub fn dump_unit_payload(world: &mut World, entity: Entity) -> bool {
    crate::sim::unit_runtime::dump_carried_unit(world, entity)
}

/// `PayloadBlockBuild.onRemoved` (`PayloadBlock.java:170-174`): a block that is
/// removed (and not itself carried) dumps its payload instead of dropping it.
///
/// A refused dump (solid tile, overlap, unit cap) drops the payload with the
/// block, exactly like the Java reference going out of scope; the detached
/// payload entity is despawned so it cannot leak as an orphan.
pub fn on_removed_dump(world: &mut World, e: Entity) {
    let Some(holder) = world.get::<PayloadHolder>(e).copied() else {
        return;
    };
    if holder.payload.is_none() || holder.carried {
        return;
    }
    dump_payload(world, e);
    if let Some(payload) = world
        .get::<PayloadHolder>(e)
        .and_then(|holder| holder.payload)
    {
        if let Some(entity) = payload.entity {
            world.entity_mut(entity).despawn();
        }
        if let Some(mut holder) = world.get_mut::<PayloadHolder>(e) {
            holder.payload = None;
        }
    }
}

/// Native unit-payload body marker. Java writes a one-byte generated unit class
/// id here (all vanilla ids are < 64), so `0xFF` cannot collide with an upstream
/// body; it makes the two layouts distinguishable on read.
pub const NATIVE_UNIT_MARKER: u8 = 0xFF;

/// `UnitPayload.write` (`UnitPayload.java:65-69`) native half: the caller has
/// already written the `payloadUnit` type tag. Writes the marker, the unit
/// record revision, the content type id and the shared unit record body
/// (`Payload.write` itself is the outer `bool exists`/tag prefix).
pub fn write_unit_payload(
    world: &World,
    payload: PayloadRef,
    w: &mut crate::world::BuildingWriter,
) {
    w.ub(NATIVE_UNIT_MARKER);
    let Some(entity) = payload.entity else {
        // No entity: revision 0 reads back as "no payload".
        w.us(0);
        return;
    };
    w.us(crate::io::sim_io::SIM_UNIT_REVISION);
    w.us(payload.content);
    if let Err(error) = crate::io::sim_io::write_unit_fields(world, entity, w) {
        log::warn!("payload_write: unit payload body failed: {error}");
    }
}

/// `Payload.read` unit branch (`Payload.java:100-116`): upstream writes a
/// generated `unit.classId()` byte and the entity IO body; native saves carry
/// [`NATIVE_UNIT_MARKER`] + the shared unit record instead. An upstream body
/// still stops the caller (`Unsupported`) exactly as before.
fn read_unit_payload(world: &mut World, r: &mut crate::world::BuildingReader) -> PayloadRead {
    let Ok(marker) = r.ub() else {
        return PayloadRead::Unsupported;
    };
    if marker != NATIVE_UNIT_MARKER {
        log::warn!(
            "payload_read: upstream unit payload (class id {marker}) not decoded (native format only)"
        );
        return PayloadRead::Unsupported;
    }
    let Ok(revision) = r.us() else {
        return PayloadRead::Unsupported;
    };
    if revision == 0 {
        return PayloadRead::None;
    }
    if revision > crate::io::sim_io::SIM_UNIT_REVISION {
        log::warn!("payload_read: unsupported unit-payload revision {revision}");
        return PayloadRead::Unsupported;
    }
    let Ok(type_raw) = r.us() else {
        return PayloadRead::Unsupported;
    };
    let type_id = crate::content::UnitTypeId::new(type_raw);
    let known = world
        .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
        .is_some_and(|runtime| runtime.content.unit(type_id).is_some());
    if !known {
        log::warn!("payload_read: unit payload type {type_raw} is unknown");
        return PayloadRead::Unsupported;
    }
    let Ok(decoded) = crate::io::sim_io::read_unit_fields(r, revision) else {
        return PayloadRead::Unsupported;
    };
    let Some(payload) = crate::sim::unit_runtime::create_unit_payload(
        world,
        type_id,
        decoded.team,
        decoded.x,
        decoded.y,
        decoded.rotation,
    ) else {
        return PayloadRead::Unsupported;
    };
    if let Some(entity) = payload.entity {
        crate::io::sim_io::apply_unit_record(world, entity, &decoded);
    }
    payload
        .entity
        .map(|_| PayloadRead::Unit(payload))
        .unwrap_or(PayloadRead::None)
}

/// `BuildPayload.place(tile)` — placement seam. Returns whether the payload
/// became a placed building.
pub fn dump_build_payload(world: &mut World, payload: PayloadRef) -> bool {
    let Some(entity) = payload.entity else {
        return false;
    };
    let Some(block) = world.get::<Building>(entity).map(|b| b.block) else {
        return false;
    };
    let hook = world
        .get_resource::<PayloadPlaceHook>()
        .and_then(|h| h.0.clone());
    let placed = match hook {
        Some(hook) => hook.place_payload(world, entity, block),
        None => place_on_tile_index(world, entity),
    };
    if placed {
        world.entity_mut(entity).remove::<super::CarriedBuild>();
        world.entity_mut(entity).remove::<super::PayloadCarried>();
    }
    placed
}

/// Best-effort ECS-only placement: sets the tile from the payload's position and
/// updates the `TileBuilds` mirror. The host may override via
/// [`PayloadPlaceHook`] to also mutate the authoritative `WorldGrid`.
fn place_on_tile_index(world: &mut World, entity: Entity) -> bool {
    let Some(pos) = world.get::<Pos>(entity).map(|p| (p.x, p.y)) else {
        return false;
    };
    let Some(block) = world.get::<Building>(entity).map(|b| b.block) else {
        return false;
    };
    let offset = world
        .get_resource::<BlockTable>()
        .and_then(|table| table.get(block))
        .map(|instance| instance.def.offset)
        .unwrap_or(0.0);
    // Java `PayloadComp.dropBlock` (`PayloadComp.java:211`): the drop tile is
    // `World.toTile(x - block.offset)`; `Tile.setBlock` then re-centres the
    // building at `tile*8 + block.offset` (`Tile.drawx`, `Tile.java:175-181`).
    let tile_x = crate::world::WorldGrid::to_tile(pos.0 - offset);
    let tile_y = crate::world::WorldGrid::to_tile(pos.1 - offset);
    let size = block_size(world, block);
    if let Some(mut building) = world.get_mut::<Building>(entity) {
        building.tile = crate::world::TilePos::new(tile_x as i16, tile_y as i16);
        building.rotation = 0;
    }
    if let Some(mut pos) = world.get_mut::<Pos>(entity) {
        pos.x = tile_x as f32 * TILE_SIZE + offset;
        pos.y = tile_y as f32 * TILE_SIZE + offset;
    }
    if let Some(mut index) = world.get_resource_mut::<TileBuilds>() {
        let width = index.width;
        let height = index.height;
        for dx in 0..size {
            for dy in 0..size {
                let x = tile_x + dx;
                let y = tile_y + dy;
                if x >= 0 && y >= 0 && x < width && y < height {
                    index.cells[(x + y * width) as usize] = Some(entity);
                }
            }
        }
    }
    true
}

/// `Block.outputsPayload` derived from the block kind (plan 02 has no explicit
/// flag field; the kind is the parity ABI).
pub fn kind_outputs_payload(kind: crate::content::BlockKind) -> bool {
    use crate::content::BlockKind as K;
    matches!(
        kind,
        K::PayloadConveyor
            | K::PayloadRouter
            | K::PayloadMassDriver
            | K::Constructor
            | K::PayloadLoader
            | K::PayloadSource
    )
}

/// `Block.acceptsPayload` derived from the block kind.
pub fn kind_accepts_payload(kind: crate::content::BlockKind) -> bool {
    use crate::content::BlockKind as K;
    matches!(
        kind,
        K::PayloadConveyor
            | K::PayloadRouter
            | K::PayloadMassDriver
            | K::PayloadDeconstructor
            | K::PayloadLoader
            | K::PayloadUnloader
            | K::PayloadVoid
    )
}

/// `Block.acceptsUnitPayloads` (base `PayloadBlock` default `true`).
pub fn kind_accepts_unit_payloads(kind: crate::content::BlockKind) -> bool {
    use crate::content::BlockKind as K;
    matches!(
        kind,
        K::PayloadConveyor
            | K::PayloadRouter
            | K::PayloadDeconstructor
            | K::PayloadLoader
            | K::PayloadUnloader
            | K::PayloadMassDriver
            | K::PayloadVoid
    )
}

/// Behaviour-level `updatePayload` for a `PayloadBlockBuild`: updates the
/// carried payload (and drops it when dead).
pub fn payload_block_update(world: &mut World, e: Entity) {
    let payload = world.get::<PayloadHolder>(e).and_then(|h| h.payload);
    if payload.is_some() {
        sync_payload_transform(world, e);
    }
}

/// Base `PayloadBlockBuild.acceptPayload` (`this.payload == null`).
pub fn accept_payload_base(world: &World, e: Entity) -> bool {
    world
        .get::<PayloadHolder>(e)
        .is_none_or(|h| h.payload.is_none())
}

/// Base `PayloadBlockBuild.write` (`payVector`, `payRotation`, `Payload`).
pub fn write_base(world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
    let holder = world.get::<PayloadHolder>(e).copied().unwrap_or_default();
    w.f(holder.pay_vector.0);
    w.f(holder.pay_vector.1);
    w.f(holder.pay_rotation);
    super::payload_write(world, holder.payload, w);
}

/// `Payload.read` outcome (`Payload.java:87-118`): `bool exists`; a block
/// payload carries `s block`, `b revision` and the nested `readAll` chunk, a
/// unit payload the native [`NATIVE_UNIT_MARKER`] record (upstream class-id
/// bodies still stop the read as `Unsupported`).
pub enum PayloadRead {
    /// No payload present.
    None,
    /// A decoded block payload.
    Block(super::PayloadRef),
    /// A decoded unit payload (native body; the carried unit entity exists).
    Unit(super::PayloadRef),
    /// A unit payload was present but cannot be decoded yet: the caller must
    /// stop reading its own body (the tile chunk is length-bounded, so the
    /// next building stays aligned).
    Unsupported,
}

pub fn payload_read(world: &mut World, r: &mut crate::world::BuildingReader) -> PayloadRead {
    if !r.bool().unwrap_or(false) {
        return PayloadRead::None;
    }
    let kind = r.b().unwrap_or(0);
    if kind == 0 {
        // `Payload.read`: `payloadUnit` split.
        return read_unit_payload(world, r);
    }
    let block_raw = r.s().unwrap_or(-1);
    let block = crate::content::BlockId::new(block_raw.max(0) as u16);
    let revision = r.b().unwrap_or(0) as u8;
    let Some(entity) = super::create_build_payload(world, block, 0) else {
        return PayloadRead::None;
    };
    let mut decoded = crate::world::building_io::DecodedBase::default();
    if crate::world::building_io::read_base_legacy(&mut decoded, r).is_err() {
        return PayloadRead::None;
    }
    decoded.kind_revision = Some(revision);
    let max_health = world
        .get_resource::<crate::world::block::BlockTable>()
        .and_then(|table| table.instance(block))
        .map(|inst| inst.def.health.max(0) as f32)
        .unwrap_or(f32::MAX);
    crate::world::building_io::apply_base(world, entity, &decoded, max_health);
    if let Some(inst) = world
        .get_resource::<crate::world::block::BlockTable>()
        .and_then(|table| table.instance(block))
    {
        // Nested `build.read(read, revision)`: no body buffer, the behavior
        // consumes exactly its bytes from the parent stream.
        inst.behavior.read(world, entity, r, revision);
    }
    PayloadRead::Block(super::PayloadRef {
        entity: Some(entity),
        content: block.raw(),
        is_block: true,
    })
}

/// Base `PayloadBlockBuild.read`; `false` = an undecodable payload bailed the
/// read (the caller stops before its own fields).
pub fn read_base(world: &mut World, e: Entity, r: &mut crate::world::BuildingReader) -> bool {
    let mut holder = world.get::<PayloadHolder>(e).copied().unwrap_or_default();
    holder.pay_vector = (r.f().unwrap_or(0.0), r.f().unwrap_or(0.0));
    holder.pay_rotation = r.f().unwrap_or(0.0);
    let result = payload_read(world, r);
    if let PayloadRead::Block(payload) | PayloadRead::Unit(payload) = result {
        holder.payload = Some(payload);
    }
    world.entity_mut(e).insert(holder);
    !matches!(result, PayloadRead::Unsupported)
}

/// Removes a swallowed payload entity (void/incineration).
pub fn consume_payload_entity(world: &mut World, payload: PayloadRef) {
    if let Some(entity) = payload.entity {
        world.entity_mut(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;

    #[test]
    fn dump_build_payload_places_on_tile_index() {
        let mut harness = BuildHarness::new(16, 16, 7);
        let container = harness.content().block_id("container").expect("container");
        harness
            .world
            .resource_mut::<crate::world::TileBuilds>()
            .rebuild(&harness.grid);
        let entity =
            super::super::create_build_payload(&mut harness.world, container, 0).expect("payload");
        let (cx, cy) = BuildHarness::tile_center(5, 5);
        if let Some(mut pos) = harness.world.get_mut::<Pos>(entity) {
            pos.x = cx;
            pos.y = cy;
        }
        let payload = PayloadRef {
            entity: Some(entity),
            content: container.raw(),
            is_block: true,
        };
        assert!(dump_build_payload(&mut harness.world, payload));
        let tile = harness
            .world
            .get::<Building>(entity)
            .map(|b| b.tile)
            .expect("building");
        assert_eq!(tile.x(), 5);
        assert_eq!(tile.y(), 5);
        assert!(
            harness
                .world
                .get::<super::super::CarriedBuild>(entity)
                .is_none()
        );
        let indexed = harness
            .world
            .get_resource::<crate::world::TileBuilds>()
            .and_then(|index| index.get(5, 5));
        assert_eq!(indexed, Some(entity));
    }

    #[test]
    fn move_out_moves_payload_to_front_conveyor() {
        let mut harness = BuildHarness::new(32, 32, 7);
        let container = harness.content().block_id("container").expect("container");
        let conveyor = harness
            .content()
            .block_id("payload-conveyor")
            .expect("payload-conveyor");
        let loader = harness
            .content()
            .block_id("payload-loader")
            .expect("payload-loader");
        // Downstream conveyor occupies the loader's facing edge (8,6).
        assert!(harness.place(9, 6, conveyor, 0, true));
        assert!(harness.place(6, 6, loader, 0, true));
        let loader_e = harness.build_at(6, 6).expect("loader");
        let front = harness.build_at(9, 6).expect("front");
        let entity =
            super::super::create_build_payload(&mut harness.world, container, 0).expect("payload");
        let payload = PayloadRef {
            entity: Some(entity),
            content: container.raw(),
            is_block: true,
        };
        super::super::handle_payload(&mut harness.world, loader_e, loader_e, payload);
        for _ in 0..400 {
            move_out_payload(&mut harness.world, loader_e);
        }
        assert!(
            harness
                .world
                .get::<PayloadHolder>(loader_e)
                .is_some_and(|h| h.payload.is_none()),
            "payload should have been moved out"
        );
        assert!(
            harness
                .world
                .get::<PayloadHolder>(front)
                .is_some_and(|h| h.payload.is_some()),
            "payload should have arrived on the front conveyor"
        );
    }
}
