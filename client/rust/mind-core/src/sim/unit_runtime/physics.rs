// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit physics/avoidance: body collection, collision layers, the avoidance
//! occupancy rebuild and the relaxation pass.
//! Upstream: core/src/mindustry/async/PhysicsProcess.java
//! core/src/mindustry/async/AvoidanceProcess.java

use super::*;

/// One unit body for [`unit_physics_system`] (`PhysicsProcess.PhysicsBody`).
pub(super) struct PhysicsBody {
    entity: Entity,
    seq: u64,
    x: f32,
    y: f32,
    radius: f32,
    mass: f32,
    layer: u8,
}

/// `UnitComp.collisionLayer` (`UnitComp.java:487-489`): legs layer for
/// leg-stepping units, ground while grounded, flying otherwise
/// (`PhysicsProcess.layerLegs`/`layerGround`/`layerFlying`).
pub(super) fn collision_layer(
    world: &World,
    def: &crate::content::registries::units::UnitTypeDef,
    entity: Entity,
) -> u8 {
    if def.allow_leg_step && def.leg_physics_layer {
        1
    } else if crate::combat::bullet::unit_is_flying(world, entity) {
        2
    } else {
        0
    }
}

/// `AvoidanceProcess.begin`/`process` (`AvoidanceProcess.java:52-106`): marks
/// every ground-layer unit's collision radius on the occupancy grid with
/// `i32::MAX - unit.id`, so a lower-id unit's tile "wins".
///
/// The port rebuilds the whole grid at the start of the tick from the current
/// positions — the end of the previous tick, exactly the buffer Java's
/// double-buffered queries read — instead of the async request/process split.
pub(super) fn rebuild_avoidance(world: &World, runtime: &mut UnitRuntime) {
    let width = runtime.grid.width().max(0) as usize;
    let height = runtime.grid.height().max(0) as usize;
    runtime.avoidance.clear();
    runtime.avoidance.resize(width * height, 0);
    if width == 0 || height == 0 {
        return;
    }
    let entities = runtime.units.clone();
    for entity in entities {
        let Some(def) = world
            .get::<UnitTypeComp>(entity)
            .and_then(|comp| runtime.content.unit(comp.type_id))
        else {
            continue;
        };
        let Some(team) = world.get::<TeamComp>(entity).map(|comp| comp.team) else {
            continue;
        };
        // `team.team.isAI() && !team.team.rules().rtsAi`
        // (`AvoidanceProcess.java:61-62`).
        if crate::entities::comp::unit::lifecycle::team_ai_flags(world, team) != Some((true, false))
        {
            continue;
        }
        // `unit.collisionLayer() == PhysicsProcess.layerGround` (`:63`).
        if collision_layer(world, def, entity) != 0 {
            continue;
        }
        let Some(pos) = world.get::<Pos>(entity).copied() else {
            continue;
        };
        // Java `unit.id`; the port's id is `EntitySeq` (low 31 bits used as
        // `i32`, documented in `update_ground_ai`).
        let id = world
            .get::<EntitySeq>(entity)
            .map_or(0, |seq| (seq.0 & 0x7fff_ffff) as i32);
        // `hitSize * unitCollisionRadiusScale / tilesize * scaling` with
        // `scaling = 2f` (`AvoidanceProcess.java:68-70`).
        let rad =
            def.hit_size * crate::ai::unit_group::UNIT_COLLISION_RADIUS_SCALE / TILE_SIZE * 2.0;
        let rad2 = rad * rad;
        let r = (rad.ceil() as i32).clamp(1, 20);
        let rx = WorldGrid::to_tile(pos.x);
        let ry = WorldGrid::to_tile(pos.y);
        for dx in -r..=r {
            for dy in -r..=r {
                let x = rx + dx;
                let y = ry + dy;
                if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                    continue;
                }
                if (dx * dx + dy * dy) as f32 > rad2 {
                    continue;
                }
                let slot = &mut runtime.avoidance[x as usize + y as usize * width];
                *slot = (*slot).max(i32::MAX - id);
            }
        }
    }
}

/// `PhysicsProcess` (`PhysicsProcess.java:188-247`): one relaxation pass over
/// every physics-enabled unit, pairing only same-layer bodies.
///
/// `type.physics` units push each other apart when their
/// `hitSize * unitCollisionRadiusScale` circles overlap; exact overlaps are
/// separated in a random direction. Upstream runs this asynchronously with a
/// quad tree and a `collided` flag; this port runs a deterministic O(n²) pass in
/// `EntitySeq` order (fine below ~500 units) and applies each move immediately.
/// Java writes bodies back through `entity.move(delta)`; pushes into solid tiles
/// are dropped here to keep units out of walls.
pub fn unit_physics_system(world: &mut World) {
    let Some(mut runtime) = world.remove_non_send::<UnitRuntime>() else {
        return;
    };
    let mut bodies: Vec<PhysicsBody> = Vec::with_capacity(runtime.units.len());
    for &entity in &runtime.units {
        let Some(def) = world
            .get::<UnitTypeComp>(entity)
            .and_then(|comp| runtime.content.unit(comp.type_id))
        else {
            continue;
        };
        // `PhysicsProcess.add`: `unit.type.physics` only.
        if !def.physics {
            continue;
        }
        if world
            .get::<Health>(entity)
            .is_some_and(|health| health.dead)
        {
            continue;
        }
        let Some(pos) = world.get::<Pos>(entity).copied() else {
            continue;
        };
        let seq = world
            .get::<EntitySeq>(entity)
            .map(|seq| seq.0)
            .unwrap_or(u64::MAX);
        bodies.push(PhysicsBody {
            entity,
            seq,
            x: pos.x,
            y: pos.y,
            // `body.radius = unit.hitSize * Vars.unitCollisionRadiusScale`.
            radius: def.hit_size * 0.6,
            // `PhysicsComp.mass()`: `hitSize² * π`.
            mass: def.hit_size * def.hit_size * std::f32::consts::PI,
            layer: collision_layer(world, def, entity),
        });
    }
    bodies.sort_unstable_by_key(|body| (body.seq, body.entity.index()));

    let len = bodies.len();
    for i in 0..len {
        for j in (i + 1)..len {
            if bodies[i].layer != bodies[j].layer {
                continue;
            }
            let rs = bodies[i].radius + bodies[j].radius;
            let dx = bodies[i].x - bodies[j].x;
            let dy = bodies[i].y - bodies[j].y;
            let dst2 = dx * dx + dy * dy;
            if dst2 >= rs * rs {
                continue;
            }
            let dst = dst2.sqrt();
            let (vx, vy) = if dst > 0.0 {
                let scale = (rs - dst) / dst;
                (dx * scale, dy * scale)
            } else {
                // `vec.trns(rand.random(360f), rs - dst)`.
                let angle = runtime
                    .rng
                    .range(crate::determinism::RngStream::Sim, 0.0, 360.0)
                    .to_radians();
                (angle.cos() * rs, angle.sin() * rs)
            };
            let ms = bodies[i].mass + bodies[j].mass;
            let m1 = bodies[j].mass / ms;
            let m2 = bodies[i].mass / ms;
            // `body += vec * (other.mass / ms) / 1.25`.
            bodies[i].x += vx * m1 / 1.25;
            bodies[i].y += vy * m1 / 1.25;
            bodies[j].x -= vx * m2 / 1.25;
            bodies[j].y -= vy * m2 / 1.25;
        }
    }

    for body in &bodies {
        let Some(old) = world.get::<Pos>(body.entity).copied() else {
            continue;
        };
        if body.x == old.x && body.y == old.y {
            continue;
        }
        // Keep pushes out of solid blocks (the AI/pathfinder never lets units
        // enter them; the raw physics push would).
        let solid = runtime
            .grid
            .tiles
            .getn(WorldGrid::to_tile(body.x), WorldGrid::to_tile(body.y))
            .is_some_and(|tile| {
                crate::ai::pathfinder::path_tile::is_solid(&runtime.content, tile.block)
            });
        if solid {
            continue;
        }
        if let Some(mut stored) = world.get_mut::<Pos>(body.entity) {
            stored.x = body.x;
            stored.y = body.y;
        }
    }
    world.insert_non_send(runtime);
}
