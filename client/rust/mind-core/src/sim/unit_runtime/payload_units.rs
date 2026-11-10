// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Carried-unit payloads: spawn a carried unit, the `dump()` gates and the
//! tracked-unit re-insert (`UnitPayload`).
//! Upstream: core/src/mindustry/entities/units/UnitPayload.java

use super::*;

// ---------------------------------------------------------------------------
// Unit payloads (`world/blocks/payloads/UnitPayload.java`)
// ---------------------------------------------------------------------------

/// Spawns a carried unit payload (`new UnitPayload(type.create(team))`) at
/// `(x, y)` for a payload block (`PayloadSource`/`UnitFactory`).
///
/// The entity is created through the plan-11 lifecycle but stays outside
/// `Groups.unit` (no [`crate::entities::comp::Unit`] marker, not in
/// [`UnitRuntime::units`]) until [`dump_carried_unit`] adds it, matching Java's
/// `UnitType.create` + `Payload.dump` split. Returns the payload handle with a
/// [`UnitPayloadSize`] for `Payload.size()`.
pub fn create_unit_payload(
    world: &mut World,
    type_id: crate::content::UnitTypeId,
    team: u8,
    x: f32,
    y: f32,
    rotation: f32,
) -> Option<crate::world::behavior::PayloadRef> {
    let def = world
        .get_non_send::<UnitRuntime>()?
        .content
        .unit(type_id)?
        .clone();
    Some(spawn_unit_payload(world, &def, team, x, y, rotation))
}

/// Content-resolved half of [`create_unit_payload`]: spawns the carried unit
/// entity and returns its payload handle.
///
/// Split out because a block-runtime-only harness (no [`UnitRuntime`]) has no
/// content registry in the world, while a block behavior already holds the
/// resolved [`UnitTypeDef`](crate::content::registries::units::UnitTypeDef).
/// Production unit blocks always have the runtime installed; the dump gates do
/// need it.
pub fn spawn_unit_payload(
    world: &mut World,
    def: &crate::content::registries::units::UnitTypeDef,
    team: u8,
    x: f32,
    y: f32,
    rotation: f32,
) -> crate::world::behavior::PayloadRef {
    let seq = crate::world::blocks::units::behavior::next_unit_seq(world);
    let entity = crate::entities::comp::unit::lifecycle::spawn_carried_unit_def(
        world, seq, def, team, x, y, rotation,
    );
    world
        .entity_mut(entity)
        .insert(crate::world::blocks::payloads::UnitPayloadSize(
            def.hit_size,
        ));
    world
        .entity_mut(entity)
        .insert(crate::world::blocks::payloads::CarriedUnit);
    crate::world::blocks::payloads::payload_ref(
        crate::world::blocks::payloads::PayloadKind::Unit,
        entity,
        def.id.raw(),
    )
}

/// `UnitPayload.dump` gates (`UnitPayload.java:104-127`) for a carried unit:
/// `Units.canCreate`, the `unit.solidity()` tile test and the grounded-overlap
/// count (`physicSize * 1.05` with the same `allowLegStep` layer).
pub fn unit_payload_can_dump(world: &World, entity: Entity) -> bool {
    let Some(runtime) = world.get_non_send::<UnitRuntime>() else {
        return false;
    };
    let Some(def) = world
        .get::<UnitTypeComp>(entity)
        .and_then(|comp| runtime.content.unit(comp.type_id))
    else {
        return false;
    };
    let team = world.get::<TeamComp>(entity).map(|t| t.team).unwrap_or(0);
    // `if(!Units.canCreate(unit.team, unit.type)) return false;`
    if !crate::entities::comp::unit::queries::can_create_unit(world, team, def) {
        return false;
    }
    let Some(pos) = world.get::<Pos>(entity).copied() else {
        return false;
    };
    let rotation = world
        .get::<UnitCore>(entity)
        .map(|core| core.rotation)
        .unwrap_or(0.0);
    // `SolidPred solid = unit.solidity(); if(solid != null)` — flying units
    // return null (`ElevationMoveComp.solidity`), everything else uses
    // `EntityCollisions.solid` on the tile one unit ahead along the rotation.
    if !crate::combat::bullet::unit_is_flying(world, entity) {
        let rad = rotation.to_radians();
        let tx = WorldGrid::to_tile(pos.x + rad.cos());
        let ty = WorldGrid::to_tile(pos.y + rad.sin());
        let solid = runtime
            .grid
            .tiles
            .getn(tx, ty)
            .is_none_or(|tile| tile.solid_full(&runtime.content));
        if solid {
            return false;
        }
    }
    // `if(!unit.type.flying && Units.count(unit.x, unit.y,
    // unit.physicSize() * 1.05f, o -> o.isGrounded() && (o.type.allowLegStep
    // == unit.type.allowLegStep)) > 0) return false;`
    if !def.flying {
        let physic = def.hit_size * 0.7 * 1.05;
        let physic2 = physic * physic;
        let leg_step = def.allow_leg_step;
        let overlap = runtime.units.iter().copied().any(|other| {
            if other == entity || world.get_entity(other).is_err() {
                return false;
            }
            let Some(other_pos) = world.get::<Pos>(other) else {
                return false;
            };
            let (dx, dy) = (other_pos.x - pos.x, other_pos.y - pos.y);
            if dx * dx + dy * dy > physic2 {
                return false;
            }
            if !crate::combat::bullet::unit_is_grounded(world, other) {
                return false;
            }
            world
                .get::<UnitTypeComp>(other)
                .and_then(|comp| runtime.content.unit(comp.type_id))
                .is_some_and(|other_def| other_def.allow_leg_step == leg_step)
        });
        if overlap {
            return false;
        }
    }
    true
}

/// `unit.add()` half of `UnitPayload.dump` (`UnitPayload.java:130-143`):
/// re-inserts the `Groups.unit` marker, assigns a fresh entity id
/// (`EntityGroup.nextId`), applies the random velocity nudge and joins the live
/// runtime. The caller has already passed [`unit_payload_can_dump`].
pub fn dump_carried_unit(world: &mut World, entity: Entity) -> bool {
    if world.get_entity(entity).is_err() || !unit_payload_can_dump(world, entity) {
        return false;
    }
    world
        .entity_mut(entity)
        .remove::<crate::world::blocks::payloads::CarriedUnit>();
    world
        .entity_mut(entity)
        .remove::<crate::world::blocks::payloads::PayloadCarried>();
    world.entity_mut(entity).insert(crate::entities::comp::Unit);
    // `unit.vel.add(Mathf.range(0.5f), Mathf.range(0.5f))` (`UnitPayload.java:139`).
    let Some(mut runtime) = world.remove_non_send::<UnitRuntime>() else {
        return false;
    };
    let (jx, jy) = (
        runtime
            .rng
            .range(crate::determinism::RngStream::Sim, -0.5, 0.5),
        runtime
            .rng
            .range(crate::determinism::RngStream::Sim, -0.5, 0.5),
    );
    if let Some(mut vel) = world.get_mut::<Vel>(entity) {
        vel.x += jx;
        vel.y += jy;
    }
    // `unit.add()`: `EntityGroup.nextId()` then join `Groups.unit`.
    runtime.seq = runtime.seq.wrapping_add(1);
    let seq = runtime.seq;
    if let Some(mut stored) = world.get_mut::<EntitySeq>(entity) {
        stored.0 = seq;
    } else {
        world.entity_mut(entity).insert(EntitySeq(seq));
    }
    runtime.units.push(entity);
    runtime.units_created += 1;
    world.insert_non_send(runtime);
    true
}
