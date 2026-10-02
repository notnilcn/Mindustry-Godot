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
        // Unit dumping is plan 11 (`UnitPayload.dump` solidity/overlap gates).
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

/// `UnitPayload.dump` seam (plan 11 owns solidity/overlap/`add()`).
fn dump_unit_payload(_world: &mut World, _entity: Entity) -> bool {
    false
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
    let tile_x = (pos.0 / TILE_SIZE).floor() as i32;
    let tile_y = (pos.1 / TILE_SIZE).floor() as i32;
    let Some(block) = world.get::<Building>(entity).map(|b| b.block) else {
        return false;
    };
    let size = block_size(world, block);
    if let Some(mut building) = world.get_mut::<Building>(entity) {
        building.tile = crate::world::TilePos::new(tile_x as i16, tile_y as i16);
        building.rotation = 0;
    }
    if let Some(mut pos) = world.get_mut::<Pos>(entity) {
        pos.x = (tile_x as f32 + 0.5) * TILE_SIZE;
        pos.y = (tile_y as f32 + 0.5) * TILE_SIZE;
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

/// Base `PayloadBlockBuild.read`.
pub fn read_base(world: &mut World, e: Entity, r: &mut crate::world::BuildingReader) {
    let mut holder = world.get::<PayloadHolder>(e).copied().unwrap_or_default();
    holder.pay_vector = (r.f().unwrap_or(0.0), r.f().unwrap_or(0.0));
    holder.pay_rotation = r.f().unwrap_or(0.0);
    world.entity_mut(e).insert(holder);
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
