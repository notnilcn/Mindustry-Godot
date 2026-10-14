// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `UnitComp.update` environment half (plan G01 T13): bounds repel/clamp/kill,
//! NaN kill, drowning, floor statuses, environment death, the drop-zone push,
//! `floor.damageTaken` and the solid-tile kill/boost branch. Ported from
//! `entities/comp/UnitComp.java:662-870`.
//!
//! The dead-flyer wreck fall (`Falling`) runs [`update_falling_wreck`] (T6).

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockId, ContentRegistry};
use crate::entities::comp::unit::comp::{
    CrawlComp, MechComp, PhysicsComp, StatusComp, TankComp, UnitCore, UnitTypeComp,
};
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

/// `UnitComp.update`'s bounds block (`UnitComp.java:664-700`): repel units out
/// of the (possibly `limitMapArea`-restricted) map bounds, clamp the position
/// and kill units that left the final world bounds.
///
/// Java clamps the position here (`x = Mathf.clamp(..)`, UnitComp.java:697-706);
/// the warp impulse itself is integrated by `vel_update` like the rest of `vel`.
#[allow(clippy::too_many_arguments)] // world + grid + Java inputs + in/out pos/vel
fn apply_bounds(
    world: &mut World,
    grid: &WorldGrid,
    bounded: bool,
    limit_area: Option<(i32, i32, i32, i32)>,
    team_is_ai: bool,
    entity: Entity,
    pos: &mut Pos,
    vel: &mut Vel,
) {
    if !bounded {
        return;
    }
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

/// `UnitComp.update`'s dead-flying branch (`UnitComp.java:803-830`) for a
/// `Falling` wreck: `drag = 0.01`, `elevation -= type.fallSpeed` and the bounds
/// block. The rest of the environment update is skipped while falling
/// (documented simplification; Java runs all of it on the dead unit too).
pub fn update_falling_wreck(
    world: &mut World,
    grid: &WorldGrid,
    content: &ContentRegistry,
    entity: Entity,
) {
    let Some(type_comp) = world.get::<UnitTypeComp>(entity).copied() else {
        return;
    };
    let Some(def) = content.unit(type_comp.type_id).cloned() else {
        return;
    };
    // `drag = 0.01f;` (`UnitComp.java:805`): less drag when dead.
    if let Some(mut physics) = world.get_mut::<PhysicsComp>(entity) {
        physics.drag = 0.01;
    }
    // `elevation -= type.fallSpeed * Time.delta;` (`UnitComp.java:825`).
    if let Some(mut core) = world.get_mut::<UnitCore>(entity) {
        core.elevation -= def.fall_speed;
    }
    let (Some(mut pos), Some(mut vel)) = (
        world.get::<Pos>(entity).copied(),
        world.get::<Vel>(entity).copied(),
    ) else {
        return;
    };
    let team = world.get::<TeamComp>(entity).map(|t| t.team).unwrap_or(0);
    let team_is_ai = crate::entities::comp::unit::lifecycle::team_ai_flags(world, team)
        .map(|(is_ai, _)| is_ai)
        .unwrap_or(false);
    apply_bounds(
        world,
        grid,
        def.bounded,
        grid.limit_area,
        team_is_ai,
        entity,
        &mut pos,
        &mut vel,
    );
    if let Some(mut stored) = world.get_mut::<Pos>(entity) {
        *stored = pos;
    }
    if let Some(mut stored) = world.get_mut::<Vel>(entity) {
        *stored = vel;
    }
}

/// `UnitComp.update`'s environment block (`UnitComp.java:662-870`) for one unit.
pub fn update_unit_environment(
    world: &mut World,
    grid: &WorldGrid,
    content: &ContentRegistry,
    spawns: &[TilePos],
    has_spawns: bool,
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
    let (
        limit_area,
        rules_env,
        rules_waves,
        wave_team,
        drop_zone_radius,
        has_rules,
        rules_attack,
        rules_drag,
        default_team,
    ) = {
        let rules = crate::game::runtime::rules_of(world);
        (
            grid.limit_area,
            rules.map_or(crate::game::rules::DEFAULT_ENV, |rules| rules.env),
            rules.is_some_and(|rules| rules.waves),
            rules.map_or(team, |rules| rules.wave_team),
            rules.map_or(300.0, |rules| rules.drop_zone_radius),
            rules.is_some(),
            rules.is_some_and(|rules| rules.attack_mode),
            rules.map_or(1.0, |rules| rules.drag_multiplier),
            rules.map_or(team, |rules| rules.default_team),
        )
    };
    // `state.isCampaign()` (`State.isCampaign()` = `sector != null`).
    let is_campaign = world
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .is_some_and(|campaign| campaign.session.is_campaign());
    // `UnitComp.isGrounded()` (`elevation < 0.001`, UnitComp.java:80-82).
    let grounded = world
        .get::<UnitCore>(entity)
        .is_none_or(|core| core.elevation < 0.001);
    let mut vel = world
        .get::<Vel>(entity)
        .copied()
        .unwrap_or(Vel { x: 0.0, y: 0.0 });

    // Bounds (`UnitComp.java:664-700`).
    apply_bounds(
        world,
        grid,
        def.bounded,
        limit_area,
        team_is_ai,
        entity,
        &mut pos,
        &mut vel,
    );
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
    let (floor_status, floor_status_duration, floor_damage_taken, floor_drag, drown_meta) = {
        let mut out = (None, 60.0f32, 0.0f32, 1.0f32, None);
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
                out.3 = inst.def.floor_drag_multiplier;
            }
            out.4 = drown_floor(world, grid, table, &def, entity, tile_floor)
                .and_then(|floor| floor_meta(table, floor));
        }
        out
    };
    let (floor_status, floor_status_duration, floor_damage_taken, floor_drag) = (
        floor_status,
        floor_status_duration,
        floor_damage_taken,
        floor_drag,
    );
    let _ = floor_status_duration;

    // Drowning (`updateDrowning`, UnitComp.java:706-723).
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

    // Captured-sector AI death (`UnitComp.java:750-753`):
    // `team.isOnlyAI() && state.isCampaign() && state.getSector().isCaptured()`.
    // `isCaptured()` for the sector being played is
    // `!rules.waves && !rules.attackMode`; the port has a single local player,
    // so `players.size == 0` is approximated by `team != rules.defaultTeam`
    // (documented single-player deviation).
    if has_rules
        && is_campaign
        && !rules_waves
        && !rules_attack
        && team_is_ai
        && team != default_team
    {
        kill(world, entity);
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

    // Drag (`UnitComp.java:790`):
    // `drag = type.drag * (isGrounded() ? floorOn().dragMultiplier : 1f) *
    //  dragMultiplier * state.rules.dragMultiplier`.
    let status_drag = world
        .get::<StatusComp>(entity)
        .map_or(1.0, |status| status.drag_multiplier);
    if let Some(mut physics) = world.get_mut::<PhysicsComp>(entity) {
        physics.drag =
            def.drag * if grounded { floor_drag } else { 1.0 } * status_drag * rules_drag;
    }

    // Drop-zone push (`UnitComp.java:734-744`): non-wave-team units inside a
    // spawn's `dropZoneRadius + hitSize/2 + 1` are pushed out. The gate is
    // `GameState.hasSpawns()` = `rules.waves && ((waveTeam has a core &&
    // rules.attackMode) || rules.spawns.size > 0)` (`GameState.java:68-70`),
    // computed once per tick by the caller.
    if has_spawns
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
            }
        }
    }

    // `UnitComp.java:832-845`: a grounded, non-hovering unit standing on a
    // building's tile triggers `unitOn` (shock mines, conveyor push).
    let (tx, ty) = (WorldGrid::to_tile(pos.x), WorldGrid::to_tile(pos.y));

    // `floor.damageTaken` (`UnitComp.java:837-841`).
    if grounded && !def.hovering && floor_damage_taken > 0.0 {
        crate::combat::damage::area::apply_health(world, entity, floor_damage_taken);
    }

    // `if(tile != null && !canPassOn())`: boosted units lift off, others die
    // (`UnitComp.java:849-855`). `EntityCollisions.move` prevents the corner
    // clips that made the old push-out necessary.
    if grid.tiles.in_bounds(tx, ty)
        && !crate::ai::types::ground::unit_can_pass(world, grid, content, entity, tx, ty)
    {
        if def.can_boost {
            if let Some(mut core) = world.get_mut::<UnitCore>(entity) {
                core.elevation = 1.0;
            }
        } else {
            kill(world, entity);
        }
    }

    if let Some(mut stored) = world.get_mut::<Pos>(entity) {
        *stored = pos;
    }
    if let Some(mut stored) = world.get_mut::<Vel>(entity) {
        *stored = vel;
    }

    // `UnitComp.java:832-835`: a grounded, non-hovering unit standing on a
    // building's tile triggers `unitOn` (shock mines, conveyor push). Runs
    // after the local `vel` write-back so an impulse (belt push) survives.
    let tile_build = i16::try_from(tx)
        .ok()
        .zip(i16::try_from(ty).ok())
        .and_then(|(tx, ty)| grid.entity_at(TilePos::new(tx, ty)));
    if grounded
        && !def.hovering
        && let Some(build) = tile_build
    {
        let behavior = world
            .get_resource::<crate::world::block::BlockTable>()
            .and_then(|table| {
                let block = world.get::<crate::entities::comp::Building>(build)?.block;
                table.instance(block)
            })
            .map(|inst| inst.behavior.clone());
        if let Some(behavior) = behavior {
            behavior.unit_on(world, build, entity);
        }
    }
}
