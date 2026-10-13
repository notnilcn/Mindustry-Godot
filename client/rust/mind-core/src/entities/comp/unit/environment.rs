// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `UnitComp.update` environment half (plan G01 T13): bounds repel/clamp/kill,
//! NaN kill, drowning, floor statuses, environment death, the drop-zone push,
//! `floor.damageTaken` and the solid-tile kill/boost branch. Ported from
//! `entities/comp/UnitComp.java:662-870`.
//!
//! Deferred (documented): the captured-sector AI death (needs the G04 captured
//! flag + `Team.isOnlyAI`), `floor.dragMultiplier` (the port never integrates
//! `PhysicsComp.drag` — velocities are set directly by the movement helpers)
//! and the dead-flyer fall.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockId, ContentRegistry};
use crate::entities::comp::unit::comp::{CrawlComp, MechComp, TankComp, UnitCore, UnitTypeComp};
use crate::entities::comp::{Health, Pos, TeamComp, Vel};
use crate::world::behavior::environment::floor_meta;
use crate::world::{TilePos, WorldGrid};

/// `UnitComp.warpDst`.
const WARP_DST: f32 = 8.0;
/// `Vars.finalWorldBounds`.
const FINAL_WORLD_BOUNDS: f32 = 250.0;
/// `UnitComp.drownTime` tick-down rate (`1/50` per tick).
const DROWN_RECOVER_RATE: f32 = 1.0 / 50.0;

/// Drowning progress (`UnitComp.drownTime`; G01 T13).
#[derive(Debug, Clone, Copy, PartialEq, bevy_ecs::component::Component, Default)]
pub struct DrownComp(pub f32);

fn kill(world: &mut World, entity: Entity) {
    if let Some(mut health) = world.get_mut::<Health>(entity) {
        health.health = 0.0;
        health.dead = true;
    }
}

/// `UnitComp.drownFloor()`: the drown-check floor for the unit's movement kind.
///
/// `MechComp` with `hitSize >= 12` only drowns when all 8 neighbours are deep
/// (`MechComp.java`); legs/crawl/tank use their `lastDeepFloor` (tank only
/// while `canDrown()`).
fn drown_floor(
    world: &World,
    grid: &WorldGrid,
    table: &crate::world::BlockTable,
    def: &crate::content::registries::units::UnitTypeDef,
    entity: Entity,
    tile_floor: Option<BlockId>,
) -> Option<BlockId> {
    let grounded = !crate::combat::bullet::unit_is_flying(world, entity);
    if world.get::<TankComp>(entity).is_some() {
        return if grounded && def.can_drown {
            world
                .get::<TankComp>(entity)
                .and_then(|tank| tank.last_deep_floor)
        } else {
            None
        };
    }
    if let Some(crawl) = world.get::<CrawlComp>(entity) {
        return crawl.last_deep_floor;
    }
    if world.get::<MechComp>(entity).is_some() && def.hit_size >= 12.0 {
        let pos = world.get::<Pos>(entity).copied()?;
        let (tx, ty) = (WorldGrid::to_tile(pos.x), WorldGrid::to_tile(pos.y));
        let all_deep = (-1..=1).all(|ox| {
            (-1..=1).all(|oy| {
                grid.tiles
                    .getn(tx + ox, ty + oy)
                    .and_then(|neighbour| floor_meta(table, neighbour.floor))
                    .is_some_and(|meta| meta.is_deep)
            })
        });
        return if all_deep { tile_floor } else { None };
    }
    tile_floor
}

/// `UnitComp.update`'s environment block (`UnitComp.java:662-870`) for one unit.
pub fn update_unit_environment(
    world: &mut World,
    grid: &WorldGrid,
    content: &ContentRegistry,
    spawns: &[TilePos],
    entity: Entity,
) {
    let (Some(mut pos), Some(type_comp)) = (
        world.get::<Pos>(entity).copied(),
        world.get::<UnitTypeComp>(entity).copied(),
    ) else {
        return;
    };

    let Some(def) = content.unit(type_comp.type_id).cloned() else {
        return;
    };
    let team = world.get::<TeamComp>(entity).map(|t| t.team).unwrap_or(0);
    let team_is_ai = crate::entities::comp::unit::lifecycle::team_ai_flags(world, team)
        .map(|(is_ai, _)| is_ai)
        .unwrap_or(false);
    let (limit_area, rules_env, rules_waves, wave_team, drop_zone_radius, has_rules) = {
        let rules = crate::game::runtime::rules_of(world);
        (
            grid.limit_area,
            rules.map_or(crate::game::rules::DEFAULT_ENV, |rules| rules.env),
            rules.is_some_and(|rules| rules.waves),
            rules.map_or(team, |rules| rules.wave_team),
            rules.map_or(300.0, |rules| rules.drop_zone_radius),
            rules.is_some(),
        )
    };
    let mut vel = world
        .get::<Vel>(entity)
        .copied()
        .unwrap_or(Vel { x: 0.0, y: 0.0 });

    // Bounds (`UnitComp.java:664-700`).
    if def.bounded {
        let ts = crate::config::TILESIZE as f32;
        let (mut left, mut bot) = (0.0f32, 0.0f32);
        let (mut right, mut top) = (grid.width() as f32 * ts, grid.height() as f32 * ts);
        if let Some((lx, ly, lw, lh)) = limit_area
            && !team_is_ai
        {
            left = lx as f32 * ts;
            bot = ly as f32 * ts;
            right = (lx + lw) as f32 * ts;
            top = (ly + lh) as f32 * ts;
        }
        let mut dx = 0.0f32;
        let mut dy = 0.0f32;
        if pos.x < left {
            dx += -(pos.x - left) / WARP_DST;
        }
        if pos.y < bot {
            dy += -(pos.y - bot) / WARP_DST;
        }
        if pos.x > right - 8.0 {
            dx -= (pos.x - (right - 8.0)) / WARP_DST;
        }
        if pos.y > top - 8.0 {
            dy -= (pos.y - (top - 8.0)) / WARP_DST;
        }
        let warp_x = dx.clamp(-10.0, 10.0);
        let warp_y = dy.clamp(-10.0, 10.0);
        vel.x += warp_x;
        vel.y += warp_y;
        // Java integrates `vel` in `VelComp.update`; the port's AI helpers set
        // the position directly, so step the warp here as well (idle units
        // would never move otherwise — documented approximation).
        pos.x += warp_x;
        pos.y += warp_y;
        pos.x = pos.x.clamp(left - 8.0, right - 8.0 + 8.0);
        pos.y = pos.y.clamp(bot - 8.0, top - 8.0 + 8.0);
        if !crate::combat::bullet::unit_is_flying(world, entity) {
            pos.x = pos.x.clamp(left, right - 8.0);
            pos.y = pos.y.clamp(bot, top - 8.0);
        }
        if pos.x < -FINAL_WORLD_BOUNDS + left
            || pos.y < -FINAL_WORLD_BOUNDS + bot
            || pos.x >= right + FINAL_WORLD_BOUNDS
            || pos.y >= top + FINAL_WORLD_BOUNDS
            || !pos.x.is_finite()
            || !pos.y.is_finite()
        {
            kill(world, entity);
        }
    }
    let dead = world
        .get::<Health>(entity)
        .is_some_and(|health| health.dead);
    if !dead
        && !world
            .get::<Health>(entity)
            .map(|h| h.health)
            .unwrap_or(0.0)
            .is_finite()
    {
        kill(world, entity);
    }

    // Floor data (read once; no table borrow survives into the mutations).
    let (floor_status, floor_status_duration, floor_damage_taken, drown_meta) = {
        let mut out = (None, 60.0f32, 0.0f32, None);
        if let Some(table) = world.get_resource::<crate::world::BlockTable>() {
            let tile_floor = grid
                .tiles
                .getn(WorldGrid::to_tile(pos.x), WorldGrid::to_tile(pos.y))
                .map(|tile| tile.floor);
            if let Some(floor) = tile_floor
                && let Some(inst) = table.get(floor)
            {
                out.0 = inst.def.floor_status;
                out.1 = inst.def.floor_status_duration;
                out.2 = inst.def.floor_damage_taken;
            }
            out.3 = drown_floor(world, grid, table, &def, entity, tile_floor)
                .and_then(|floor| floor_meta(table, floor));
        }
        out
    };
    let (floor_status, floor_status_duration, floor_damage_taken) =
        (floor_status, floor_status_duration, floor_damage_taken);
    let _ = floor_status_duration;

    // Drowning (`updateDrowning`, UnitComp.java:706-723).
    let grounded = !crate::combat::bullet::unit_is_flying(world, entity);
    let mut drown = world.get::<DrownComp>(entity).map_or(0.0, |comp| comp.0);
    if let Some(meta) = drown_meta
        && meta.is_liquid
        && meta.drown_time > 0.0
        && grounded
        && def.can_drown
    {
        drown += 1.0 / ((def.hit_size / 8.0) * def.drown_time_multiplier * meta.drown_time);
        if drown >= 0.999 {
            kill(world, entity);
        }
    } else {
        drown = (drown - DROWN_RECOVER_RATE).clamp(0.0, 1.0);
    }
    if let Some(mut stored) = world.get_mut::<DrownComp>(entity) {
        stored.0 = drown;
    } else if world.get_entity(entity).is_ok() {
        world.entity_mut(entity).insert(DrownComp(drown));
    }

    // Environment death (`if(!type.supportsEnv(rules.env) && !dead)`).
    if !world
        .get::<Health>(entity)
        .is_some_and(|health| health.dead)
        && has_rules
        && !def.supports_env(rules_env)
    {
        kill(world, entity);
    }

    // Floor status (`StatusComp.update`: grounded non-hovering units take the
    // floor's status each tick).
    if grounded
        && !def.hovering
        && let Some(status) = floor_status
    {
        crate::combat::damage::status::apply_status(world, entity, status, floor_status_duration);
    }

    // Drop-zone push (`UnitComp.java:734-744`): non-wave-team units inside a
    // spawn's `dropZoneRadius + hitSize/2 + 1` are pushed out.
    if rules_waves
        && team != wave_team
        && !spawns.is_empty()
        && world
            .get::<Health>(entity)
            .is_some_and(|health| !health.dead)
    {
        let relative_size = drop_zone_radius + def.hit_size / 2.0 + 1.0;
        for spawn in spawns {
            let ts = crate::config::TILESIZE as f32;
            let (sx, sy) = (spawn.x() as f32 * ts, spawn.y() as f32 * ts);
            let dx = pos.x - sx;
            let dy = pos.y - sy;
            let dst = (dx * dx + dy * dy).sqrt();
            if dst <= relative_size && dst > 0.0001 {
                let len = 0.1 + 1.0 - dst / relative_size;
                let push_x = dx / dst * len * 0.45;
                let push_y = dy / dst * len * 0.45;
                vel.x += push_x;
                vel.y += push_y;
                // Same integration note as the bounds warp above.
                pos.x += push_x;
                pos.y += push_y;
            }
        }
    }

    // `floor.damageTaken` (`UnitComp.java:837-841`).
    if grounded && floor_damage_taken > 0.0 {
        crate::combat::damage::area::apply_health(world, entity, floor_damage_taken);
    }

    // `if(!canPassOn())`: boosted units lift off, others die
    // (`UnitComp.java:849-855`). The port's movement helpers do not run
    // `EntityCollisions.move` (Java prevents solid overlap there), so a corner
    // clip can round into a wall tile: push the unit back out on the least
    // penetrating side and kill only when no neighbour is passable.
    let (tx, ty) = (WorldGrid::to_tile(pos.x), WorldGrid::to_tile(pos.y));
    if grid.tiles.in_bounds(tx, ty)
        && !crate::ai::types::ground::unit_can_pass(world, grid, content, entity, tx, ty)
    {
        if def.can_boost {
            if let Some(mut core) = world.get_mut::<UnitCore>(entity) {
                core.elevation = 1.0;
            }
        } else {
            let ts = crate::config::TILESIZE as f32;
            let (ix, iy) = (pos.x - tx as f32 * ts, pos.y - ty as f32 * ts);
            let options = [
                (ix, 0.0f32, -1i32, 0i32),
                (ts - ix, 0.0, 1, 0),
                (iy, 1.0, 0, -1),
                (ts - iy, 1.0, 0, 1),
            ];
            let mut exit = options.to_vec();
            exit.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut pushed = false;
            for (_, _, dx, dy) in exit {
                if crate::ai::types::ground::unit_can_pass(
                    world,
                    grid,
                    content,
                    entity,
                    tx + dx,
                    ty + dy,
                ) {
                    if dx != 0 {
                        pos.x = tx as f32 * ts + if dx < 0 { -0.5 } else { ts + 0.5 };
                    }
                    if dy != 0 {
                        pos.y = ty as f32 * ts + if dy < 0 { -0.5 } else { ts + 0.5 };
                    }
                    pushed = true;
                    break;
                }
            }
            if !pushed {
                kill(world, entity);
            }
        }
    }

    if let Some(mut stored) = world.get_mut::<Pos>(entity) {
        *stored = pos;
    }
    if let Some(mut stored) = world.get_mut::<Vel>(entity) {
        *stored = vel;
    }
}
