// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Damage.collideLine` for live bullets (`Damage.java:256-337`) and
//! `Damage.linecast` (`Damage.java:369-433`).
//!
//! The line-hit half of the instant/beam weapons: laser, shrapnel, continuous
//! and rail bullets call [`collide_line_bullet`]; sap bullets use
//! [`linecast_enemy`]. The building-only
//! [`crate::combat::damage::line::collide_line`] stays for its golden tests.
//!
//! Ordering and gates follow Java exactly: building candidates from the
//! no-diagonal tile raycast (plus their `Geometry.d4` neighbours), unit
//! candidates from the segment AABB grown by `expand * 2`, a stable sort by
//! `hitter.dst2(c.x, c.y)`, then hits while `damage > 0` and under the pierce
//! cap. Units iterate in `(EntitySeq, index)` order (the deterministic stand-in
//! for `Units.nearbyEnemies`' spatial order).

use bevy_ecs::entity::Entity;

use crate::content::registries::bullets::BulletKind;
use crate::ecs::EntitySeq;
use crate::entities::comp::unit::comp::{HitboxComp, UnitCore, UnitTypeComp};
use crate::entities::comp::{Health, Pos, TeamComp, Unit};

use super::{
    Bullet, CombatCtx, HIT, behavior_for, behavior_hit, hit_building, hit_entity,
    passes_under_build, unit_is_flying,
};

/// `Geometry.d4` (left/counter-clockwise: `(1,0), (0,1), (-1,0), (0,-1)`).
const D4: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// `World.toTile` (rounds; `World.java:174`).
fn to_tile(pixel: f32) -> i32 {
    (pixel / crate::config::TILESIZE as f32).round() as i32
}

/// `Intersector.intersectSegments` (`Intersector.java:601-616`).
#[allow(clippy::too_many_arguments)]
fn intersect_segments(
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    x3: f32,
    y3: f32,
    x4: f32,
    y4: f32,
) -> bool {
    let d = (y4 - y3) * (x2 - x1) - (x4 - x3) * (y2 - y1);
    if d == 0.0 {
        return false;
    }
    let yd = y1 - y3;
    let xd = x1 - x3;
    let ua = ((x4 - x3) * yd - (y4 - y3) * xd) / d;
    if ua < 0.0 || ua > 1.0 {
        return false;
    }
    let ub = ((x2 - x1) * yd - (y2 - y1) * xd) / d;
    if ub < 0.0 || ub > 1.0 {
        return false;
    }
    true
}

/// `Intersector.intersectSegmentRectangle` (`Intersector.java:495-505`).
#[allow(clippy::too_many_arguments)]
fn intersect_segment_rect(
    sx: f32,
    sy: f32,
    ex: f32,
    ey: f32,
    rx: f32,
    ry: f32,
    rw: f32,
    rh: f32,
) -> bool {
    let rendx = rx + rw;
    let rendy = ry + rh;
    intersect_segments(sx, sy, ex, ey, rx, ry, rx, rendy)
        || intersect_segments(sx, sy, ex, ey, rx, ry, rendx, ry)
        || intersect_segments(sx, sy, ex, ey, rendx, ry, rendx, rendy)
        || intersect_segments(sx, sy, ex, ey, rx, rendy, rendx, rendy)
        // `Rect.contains` (`Rect.java:209-211`).
        || (rx <= sx && rendx >= sx && ry <= sy && rendy >= sy)
}

/// `Units.nearbyEnemies` + `checkTarget`/`hittable` unit candidates of a line
/// (`Damage.collideLine` unit half, `Damage.java:306-320`).
///
/// `expand` is the per-side growth of the unit hitbox and the segment AABB;
/// callers pass 3.0 for `Damage.linecast` and `expand * 2` = 6.0 for
/// `collideLine` (Java grows by `expand * 2` there).
pub(crate) fn line_units(
    ctx: &CombatCtx<'_>,
    bullet: Entity,
    x: f32,
    y: f32,
    x2: f32,
    y2: f32,
    expand: f32,
) -> Vec<(f32, f32, Entity)> {
    let Some(state) = ctx.bullet(bullet) else {
        return Vec::new();
    };
    let Some(def) = ctx.content.bullet(state.def) else {
        return Vec::new();
    };
    let (collides_air, collides_ground) = (def.collides_air, def.collides_ground);
    let team = ctx.team(bullet);
    // Segment AABB grown by `expand` on each side (`Damage.java:334-335`).
    let min_x = x.min(x2) - expand;
    let max_x = x.max(x2) + expand;
    let min_y = y.min(y2) - expand;
    let max_y = y.max(y2) + expand;

    let mut units: Vec<Entity> = ctx
        .world
        .iter_entities()
        .filter(|entity| entity.contains::<Unit>())
        .map(|entity| entity.id())
        .collect();
    units.sort_by_key(|entity| {
        (
            ctx.world
                .get::<EntitySeq>(*entity)
                .map_or(u64::MAX, |seq| seq.0),
            entity.index(),
        )
    });

    let mut out = Vec::new();
    for unit in units {
        if ctx
            .world
            .get::<UnitCore>(unit)
            .is_some_and(|core| core.dead)
            || ctx.world.get::<Health>(unit).is_none_or(|h| h.dead)
        {
            continue;
        }
        if ctx.world.get::<TeamComp>(unit).map(|t| t.team) == Some(team) {
            continue;
        }
        let flying = unit_is_flying(ctx.world, unit);
        if !((flying && collides_air) || (!flying && collides_ground)) {
            continue;
        }
        let hittable = ctx
            .world
            .get::<UnitTypeComp>(unit)
            .and_then(|comp| ctx.content.unit(comp.type_id))
            .is_none_or(|def| def.hittable);
        if !hittable {
            continue;
        }
        let Some(pos) = ctx.world.get::<Pos>(unit).copied() else {
            continue;
        };
        let hit_size = ctx
            .world
            .get::<HitboxComp>(unit)
            .map_or(8.0, |h| h.hit_size);
        let half = hit_size / 2.0;
        // Broad phase against the grown segment AABB.
        if pos.x + half < min_x
            || pos.x - half > max_x
            || pos.y + half < min_y
            || pos.y - half > max_y
        {
            continue;
        }
        let Some((cx, cy)) = crate::combat::damage::line::raycast_rect(
            x,
            y,
            x2,
            y2,
            pos.x,
            pos.y,
            half + expand,
            half + expand,
        ) else {
            continue;
        };
        out.push((cx, cy, unit));
    }
    out
}

/// `BulletComp.collision` for a unit target (`BulletComp.java:124-145`) as run
/// from `Damage.collideLine`: `type.hit`, pierce bookkeeping, `hitEntity`.
pub(crate) fn line_hit_unit(
    ctx: &mut CombatCtx<'_>,
    bullet: Entity,
    unit: Entity,
    cx: f32,
    cy: f32,
) {
    let Some(state) = ctx.bullet(bullet).cloned() else {
        return;
    };
    let Some(def) = ctx.content.bullet(state.def) else {
        return;
    };
    let (pierce, kind) = (def.pierce, def.kind);
    behavior_hit(ctx, bullet, cx, cy, true);
    let health = ctx.world.get::<Health>(unit).map_or(0.0, |h| h.health);
    if pierce {
        if let Some(mut b) = ctx.world.get_mut::<Bullet>(bullet) {
            b.collided.push(unit);
        }
    } else if let Some(mut b) = ctx.world.get_mut::<Bullet>(bullet) {
        b.set(HIT);
    }
    hit_entity(ctx, bullet, unit, health);
    // `RailBulletType.hitEntity` (`RailBulletType.java:52-58`): the base
    // `hitEntity` already ran the falloff; the rail override runs it again and
    // shrinks the beam (upstream's double application, kept bug-for-bug).
    if kind == BulletKind::Rail {
        rail_handle_pierce(ctx, bullet, health, cx, cy);
    }
}

/// `RailBulletType.hitEntity`/`handlePierce` (`RailBulletType.java:31-58`):
/// subtract the falloff from `bullet.damage` (a second time per Java's
/// `super.hitEntity` + explicit call) and shrink `fdata` to the hit distance
/// once the beam is drained.
pub(crate) fn rail_handle_pierce(
    ctx: &mut CombatCtx<'_>,
    bullet: Entity,
    initial_health: f32,
    x: f32,
    y: f32,
) {
    let Some(bx) = ctx.pos(bullet) else {
        return;
    };
    let dst = ((x - bx.0).powi(2) + (y - bx.1).powi(2)).sqrt();
    let Some(state) = ctx.bullet(bullet).cloned() else {
        return;
    };
    let factor = ctx
        .content
        .bullet(state.def)
        .map_or(0.0, |def| def.pierce_damage_factor);
    let sub = (initial_health * factor).max(0.0);
    if state.damage <= 0.0 {
        if let Some(mut b) = ctx.world.get_mut::<Bullet>(bullet) {
            b.fdata = b.fdata.min(dst);
        }
        return;
    }
    let mut drained = false;
    if let Some(mut b) = ctx.world.get_mut::<Bullet>(bullet) {
        b.damage -= sub.min(b.damage);
        drained = b.damage <= 0.0;
    }
    if drained && let Some(mut b) = ctx.world.get_mut::<Bullet>(bullet) {
        b.fdata = b.fdata.min(dst);
    }
}

/// `Damage.collideLine` (`Damage.java:256-337`) for a live bullet.
///
/// Damages enemy units **and** buildings along the ray in `dst2` order while
/// `bullet.damage > 0` and under the pierce cap; sets `bullet.fdata` to the
/// beam length (Java sets it after `findLength`; this port keeps the existing
/// no-`findLength` behaviour) and returns the final value. `large` adds the
/// `Geometry.d4` neighbour buildings unconditionally (`largeHit`).
#[allow(clippy::too_many_arguments)]
pub fn collide_line_bullet(
    ctx: &mut CombatCtx<'_>,
    bullet: Entity,
    x: f32,
    y: f32,
    angle: f32,
    length: f32,
    large: bool,
    pierce_cap: i32,
) -> f32 {
    let Some(state) = ctx.bullet(bullet).cloned() else {
        return length;
    };
    let Some(def) = ctx.content.bullet(state.def) else {
        return length;
    };
    let kind = def.kind;
    let (collides_ground, collides_tiles) = (def.collides_ground, def.collides_tiles);
    let (hit_under, piercing) = (def.hit_under, def.pierce);
    let team = ctx.team(bullet);
    let ts = crate::config::TILESIZE as f32;
    let rad = angle.to_radians();
    let (x2, y2) = (x + rad.cos() * length, y + rad.sin() * length);
    // `Damage.collideLine`: `hitter.fdata = length` (post-`findLength`).
    if let Some(mut b) = ctx.world.get_mut::<Bullet>(bullet) {
        b.fdata = length;
    }

    // (hit x, hit y, target, is_building), in Java insertion order.
    let mut candidates: Vec<(f32, f32, Entity, bool)> = Vec::new();
    let mut collided_blocks: Vec<i32> = Vec::new();

    if collides_ground && collides_tiles {
        crate::world::raycast::raycast_each_no_diagonal(
            to_tile(x),
            to_tile(y),
            to_tile(x2),
            to_tile(y2),
            |cx, cy| {
                let Some(tile) = ctx.grid.tiles.getn(cx, cy) else {
                    return false;
                };
                let Some(build) = tile.build else {
                    return false;
                };
                // `tile.collide(hitter)` is the base `BuildingComp.collide`
                // (`true`; Wall deflect lives in G02) and `testCollision` is
                // the behavior hook (base `true`, rail enemy-only).
                let target_team = ctx.world.get::<TeamComp>(build).map_or(0, |t| t.team);
                let test_collision =
                    behavior_for(kind).test_collision(ctx.content, state.def, team, target_team);
                let collide =
                    passes_under_build(ctx, &state, bullet, build, hit_under, piercing, team)
                        && (target_team != team || test_collision);
                let pos = ctx
                    .world
                    .get::<crate::entities::comp::Building>(build)
                    .map(|comp| comp.tile);
                let Some(pos) = pos else {
                    return false;
                };
                if collide && !collided_blocks.contains(&pos.pack()) {
                    collided_blocks.push(pos.pack());
                    candidates.push((cx as f32 * ts, cy as f32 * ts, build, true));
                    // `Geometry.d4` neighbours (`Damage.java:288-297`); the
                    // mixed coordinate math is upstream's (bug-for-bug).
                    for (dx, dy) in D4 {
                        let Some(other) = ctx.grid.tiles.getn(cx + dx, cy + dy) else {
                            continue;
                        };
                        let Some(other_build) = other.build else {
                            continue;
                        };
                        let bounds = other.get_hitbox(ctx.content);
                        if large
                            || intersect_segment_rect(
                                x, y, x2, y2, bounds[0], bounds[1], bounds[2], bounds[3],
                            )
                        {
                            let other_pos = ctx
                                .world
                                .get::<crate::entities::comp::Building>(other_build)
                                .map(|comp| comp.tile);
                            if let Some(other_pos) = other_pos
                                && passes_under_build(
                                    ctx,
                                    &state,
                                    bullet,
                                    other_build,
                                    hit_under,
                                    piercing,
                                    team,
                                )
                                && !collided_blocks.contains(&other_pos.pack())
                            {
                                collided_blocks.push(other_pos.pack());
                                candidates.push((
                                    dx as f32 + cx as f32 * ts,
                                    (dy + cy) as f32 * ts,
                                    other_build,
                                    true,
                                ));
                            }
                        }
                    }
                }
                false
            },
        );
    }

    // Unit candidates (`Damage.java:306-320`): `expand * 2` = 6 per side.
    for (cx, cy, unit) in line_units(ctx, bullet, x, y, x2, y2, 3.0 * 2.0) {
        candidates.push((cx, cy, unit, false));
    }

    // `collided.sort(c -> hitter.dst2(c.x, c.y))` — stable.
    candidates.sort_by(|a, b| {
        let da = (a.0 - x).powi(2) + (a.1 - y).powi(2);
        let db = (b.0 - x).powi(2) + (b.1 - y).powi(2);
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut count: i32 = 0;
    for (cx, cy, target, is_building) in candidates {
        let damage = ctx.bullet(bullet).map_or(0.0, |b| b.damage);
        if !(damage > 0.0 && (pierce_cap <= 0 || count < pierce_cap)) {
            continue;
        }
        if is_building {
            let target_team = ctx.world.get::<TeamComp>(target).map_or(0, |t| t.team);
            let initial_health = ctx.world.get::<Health>(target).map_or(0.0, |h| h.health);
            if target_team != team {
                hit_building(ctx, bullet, target);
                behavior_hit(ctx, bullet, cx, cy, true);
                count += 1;
            }
            // `if(hitter.type.testCollision(hitter, tile))
            //     hitter.type.hitTile(hitter, tile, c.x, c.y, health, false);`
            if behavior_for(kind).test_collision(ctx.content, state.def, team, target_team) {
                behavior_for(kind).hit_tile(ctx, bullet, target, cx, cy, initial_health, false);
            }
        } else {
            line_hit_unit(ctx, bullet, target, cx, cy);
            count += 1;
        }
    }

    ctx.bullet(bullet).map_or(length, |b| b.fdata)
}

/// `Damage.linecast` (`Damage.java:369-433`): the first enemy building on the
/// ray (when the bullet collides with ground), or the closest enemy unit whose
/// grown hitbox intersects the segment — whichever is closer.
pub fn linecast_enemy(
    ctx: &CombatCtx<'_>,
    bullet: Entity,
    x: f32,
    y: f32,
    angle: f32,
    length: f32,
) -> Option<Entity> {
    let state = ctx.bullet(bullet)?;
    let def = ctx.content.bullet(state.def)?;
    let collides_ground = def.collides_ground;
    let team = ctx.team(bullet);
    let rad = angle.to_radians();
    let (x2, y2) = (x + rad.cos() * length, y + rad.sin() * length);

    let mut building: Option<Entity> = None;
    if collides_ground {
        crate::world::raycast::raycast_each(
            to_tile(x),
            to_tile(y),
            to_tile(x2),
            to_tile(y2),
            |cx, cy| {
                let Some(tile) = ctx.grid.tiles.getn(cx, cy) else {
                    return false;
                };
                let Some(build) = tile.build else {
                    return false;
                };
                if ctx.world.get::<TeamComp>(build).map(|t| t.team) != Some(team) {
                    building = Some(build);
                    return true;
                }
                false
            },
        );
    }

    let unit = line_units(ctx, bullet, x, y, x2, y2, 3.0)
        .into_iter()
        .min_by(|a, b| {
            let da = (a.0 - x).powi(2) + (a.1 - y).powi(2);
            let db = (b.0 - x).powi(2) + (b.1 - y).powi(2);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(_, _, unit)| unit);

    match (building, unit) {
        (Some(b), Some(u)) => {
            let bpos = ctx
                .world
                .get::<crate::entities::comp::Building>(b)
                .map(|comp| comp.tile);
            let upos = ctx.world.get::<Pos>(u).map(|p| (p.x, p.y));
            match (bpos, upos) {
                (Some(bp), Some((ux, uy))) => {
                    let ts = crate::config::TILESIZE as f32;
                    let (bx, by) = ((bp.x() as f32 + 0.5) * ts, (bp.y() as f32 + 0.5) * ts);
                    let bd = (bx - x).powi(2) + (by - y).powi(2);
                    let ud = (ux - x).powi(2) + (uy - y).powi(2);
                    if bd <= ud { Some(b) } else { Some(u) }
                }
                (Some(_), None) => Some(b),
                _ => Some(u),
            }
        }
        (Some(b), None) => Some(b),
        (None, Some(u)) => Some(u),
        (None, None) => None,
    }
}
