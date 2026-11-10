// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Turret shooting, bullet spawn and target selection.
//! Upstream: core/src/mindustry/world/blocks/defense/turrets/Turret.java

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::audio::math::AudioRng;
use crate::combat::bullet::{BulletSpawn, CombatCtx, unit_is_grounded};
use crate::combat::targeting::{TargetQueries, UnitSort};
use crate::content::registries::sound_meta::SoundId;
use crate::content::{BulletId, ContentRegistry};
use crate::entities::comp::{Health, Pos, TeamComp};
use crate::weapons::pattern::{self, PatternShot, ShotBuffer};
use crate::world::modules::LiquidModule;

use super::ammo::{
    has_ammo_state, peek_ammo_min_range_change_of, peek_ammo_range_change_of, peek_ammo_state,
    turret_range_of, use_ammo,
};
use super::math::{position, trnsx, trnsy};
use super::{PendingShot, TurretKind, TurretState};

/// `TurretBuild.shoot`.
pub(super) fn shoot(ctx: &mut CombatCtx<'_>, e: Entity, state: &mut TurretState, bullet_type: BulletId) {
    let config = state.config.clone();
    let (x, y) = position(ctx.world, e).unwrap_or((0.0, 0.0));
    let rotation = state.rotation;

    // `Turret.shoot`: `chargeSound.at(bulletX, bulletY, random(pitch))` when
    // `shoot.firstShotDelay > 0` (`Turret.java:756-758`).
    if config.shoot.first_shot_delay > 0.0 {
        let cx = x + trnsx(rotation - 90.0, config.shoot_x, config.shoot_y);
        let cy = y + trnsy(rotation - 90.0, config.shoot_x, config.shoot_y);
        let pitch = {
            let mut rng = crate::audio::sim::FxAudioRng { rng: &mut *ctx.rng };
            config.sound_pitch_min
                + rng.next_f32() * (config.sound_pitch_max - config.sound_pitch_min)
        };
        crate::audio::sim::emit_turret_shoot(ctx.audio, config.charge_sound, cx, cy, 1.0, pitch);
    }

    let mut buffer = ShotBuffer::default();
    let mut barrel = state.barrel_counter;
    pattern::emit(
        &config.shoot,
        state.total_shots,
        &mut buffer,
        &mut barrel,
        ctx.rng,
    );
    state.barrel_counter = barrel;

    // `Turret.shoot`'s handler (`Turret.java:763-778`): `if(delay > 0f)
    // Time.run(delay, …) else bullet(…)`. The pending queue is `queuedBullets`:
    // an immediate shot leaves it at once, a delayed one stays until its tick.
    for shot in &buffer.shots {
        if shot.delay > 0.0 {
            state.pending.push(PendingShot {
                bullet: bullet_type,
                shot: *shot,
                delay: shot.delay,
                barrel: shot.barrel,
            });
        } else {
            bullet(ctx, e, state, bullet_type, shot, shot.barrel);
        }
    }

    if config.consume_ammo_once {
        let _ = use_ammo(state, ctx.world, e);
    }
}

/// `TurretBuild.bullet` (`Turret.java:785-828`): spawns one pattern shot from
/// the turret's *current* pose — a delayed shot recomputes the muzzle position,
/// angle, spread, lifetime and `targetPos` at fire time and restores `barrel`
/// for the recoil index.
pub(super) fn bullet(
    ctx: &mut CombatCtx<'_>,
    e: Entity,
    state: &mut TurretState,
    bullet_type: BulletId,
    shot: &PatternShot,
    barrel: i32,
) {
    let config = state.config.clone();
    // `if(dead || (!consumeAmmoOnce && !hasAmmo())) return;`
    if !config.consume_ammo_once && !has_ammo_state(state, ctx.world, e) {
        return;
    }
    let (x, y) = position(ctx.world, e).unwrap_or((0.0, 0.0));
    let team = ctx.team(e);
    let rotation = state.rotation;

    let def_inaccuracy = ctx
        .content
        .bullet(bullet_type)
        .map(|def| def.inaccuracy)
        .unwrap_or(0.0);
    let scale_life = ctx
        .content
        .bullet(bullet_type)
        .map(|def| def.scale_life)
        .unwrap_or(false);
    let bullet_range = ctx
        .content
        .bullet(bullet_type)
        .map(|def| def.range)
        .unwrap_or(0.0);
    // `Turret.bullet`: `(type.shootSound != none ? type.shootSound : shootSound)`
    // played per spawned bullet (`Turret.java:804`); the pitch is
    // `Mathf.random(soundPitchMin, soundPitchMax)` drawn from the audio-only Fx
    // stream, so the checksum is unaffected.
    let bullet_sound = ctx
        .content
        .bullet(bullet_type)
        .map(|def| def.shoot_sound)
        .unwrap_or(SoundId::NONE);
    let shoot_sound = if !bullet_sound.is_none_or_unset() {
        bullet_sound
    } else {
        config.shoot_sound
    };

    let x_spread = ctx.rng.range(
        crate::determinism::RngStream::Sim,
        -config.x_rand,
        config.x_rand,
    );
    let bx = x + trnsx(
        rotation - 90.0,
        config.shoot_x + shot.x + x_spread,
        config.shoot_y + shot.y,
    );
    let by = y + trnsy(
        rotation - 90.0,
        config.shoot_x + shot.x + x_spread,
        config.shoot_y + shot.y,
    );
    let angle = rotation
        + shot.rotation
        + ctx.rng.range(
            crate::determinism::RngStream::Sim,
            -(config.inaccuracy + def_inaccuracy),
            config.inaccuracy + def_inaccuracy,
        );
    let base_life = (1.0 - config.life_rnd)
        + ctx
            .rng
            .range(crate::determinism::RngStream::Sim, 0.0, config.life_rnd)
        + config.extra_life;
    let life_scl = if scale_life && bullet_range > 0.0 {
        let dst = ((state.target_pos.0 - bx).powi(2) + (state.target_pos.1 - by).powi(2)).sqrt();
        // `Turret.bullet` (`Turret.java:797`): `clamp((baseLife +
        // scaleLifetimeOffset) * dst / type.range, minRange()/type.range,
        // range()/type.range)`.
        let min_range = min_range_of(state, ctx.world, e);
        let max_range = turret_range_of(state, ctx.world, e);
        ((base_life + config.scale_lifetime_offset) * dst / bullet_range)
            .clamp(min_range / bullet_range, max_range / bullet_range)
    } else {
        base_life
    };
    let velocity_scl = (1.0 - config.velocity_rnd)
        + ctx
            .rng
            .range(crate::determinism::RngStream::Sim, 0.0, config.velocity_rnd)
        + config.extra_velocity;
    let spawn = BulletSpawn {
        def: bullet_type,
        owner: Some(e),
        shooter: Some(e),
        team,
        x: bx,
        y: by,
        angle,
        velocity_scl,
        lifetime_scl: life_scl,
        aim_x: state.target_pos.0,
        aim_y: state.target_pos.1,
        mover: shot.mover,
        ..BulletSpawn::default()
    };
    let _ = ctx.spawn(&spawn);
    let pitch = {
        let mut rng = crate::audio::sim::FxAudioRng { rng: &mut *ctx.rng };
        config.sound_pitch_min + rng.next_f32() * (config.sound_pitch_max - config.sound_pitch_min)
    };
    crate::audio::sim::emit_turret_shoot(
        ctx.audio,
        shoot_sound,
        bx,
        by,
        config.shoot_sound_volume,
        pitch,
    );

    state.total_shots += 1;
    if !config.consume_ammo_once {
        let _ = use_ammo(state, ctx.world, e);
    }
    state.cur_recoil = 1.0;
    if config.recoils > 0 {
        let count = config.recoils as usize;
        if state.cur_recoils.len() < count {
            state.cur_recoils.resize(count, 0.0);
        }
        state.cur_recoils[barrel.rem_euclid(count as i32) as usize] = 1.0;
    }
    state.heat = 1.0;
}

/// `LiquidTurretBuild.findTarget` fire scan (`LiquidTurret.java:93-115`).
///
/// Scans the `range / tilesize` square around the turret, keeps the nearest
/// fire **not** on another team's building (`other.build == null ||
/// other.team() == team`), and skips the turret's own tile (`other.build !=
/// this`).
pub(super) fn nearest_fire(
    ctx: &CombatCtx<'_>,
    e: Entity,
    team: u8,
    x: f32,
    y: f32,
    range: f32,
) -> Option<Entity> {
    let ts = crate::config::TILESIZE as f32;
    let tx = crate::world::WorldGrid::to_tile(x);
    let ty = crate::world::WorldGrid::to_tile(y);
    let tr = (range / ts) as i32;
    let mut best: Option<(f32, Entity)> = None;
    for dy in -tr..=tr {
        for dx in -tr..=tr {
            let (fx, fy) = (tx + dx, ty + dy);
            if !ctx.grid.tiles.in_bounds(fx, fy) {
                continue;
            }
            let tile = ctx.grid.tiles.get(fx, fy);
            // `other.build != this`.
            if tile.build == Some(e) {
                continue;
            }
            // `other.build == null || other.team() == team`.
            if let Some(build) = tile.build
                && ctx
                    .world
                    .get::<TeamComp>(build)
                    .is_some_and(|comp| comp.team != team)
            {
                continue;
            }
            let Some(fire) = crate::combat::fires::find_at(ctx.world, fx as i16, fy as i16) else {
                continue;
            };
            let Some(pos) = ctx.world.get::<Pos>(fire) else {
                continue;
            };
            let d2 = (pos.x - x) * (pos.x - x) + (pos.y - y) * (pos.y - y);
            if d2 > range * range {
                continue;
            }
            if best.is_none_or(|(bd, _)| d2 < bd) {
                best = Some((d2, fire));
            }
        }
    }
    best.map(|(_, fire)| fire)
}

/// `TurretBuild.findTarget` / `findEnemy` (`Turret.java:631-677`) via plan 11's
/// [`TargetQueries`] (plan 10 §3.8).
///
/// Two passes: the firing range first, then `trackingRange()` when it differs
/// (a null first pass only), and finally a damaged ally when the turret heals.
#[allow(clippy::too_many_arguments)]
pub(super) fn find_target(
    ctx: &CombatCtx<'_>,
    e: Entity,
    state: &mut TurretState,
    team: u8,
    x: f32,
    y: f32,
    targets: &TargetQueries,
) {
    let config = &state.config;
    // The dispatched state is out of the world during this pass, so the ranges
    // come from the state handle (`turret_range_of`).
    let range = turret_range_of(state, ctx.world, e);

    // `LiquidTurretBuild.findTarget` (`LiquidTurret.java:91-119`): a liquid
    // turret whose current ammo liquid can extinguish targets the nearest fire
    // in its range square instead of running the standard targeting pass.
    if config.kind == TurretKind::Liquid
        && let Some(liquids) = ctx.world.get::<LiquidModule>(e)
        && ctx
            .content
            .liquid(liquids.current_liquid())
            .is_some_and(|liquid| liquid.can_extinguish())
        && let Some(fire) = nearest_fire(ctx, e, team, x, y, range)
    {
        state.target = Some(fire);
        return;
    }
    let track_range = tracking_range_of(state, ctx.world, e);
    let sort = ammo_unit_sort(state, ctx.world, e, ctx.content);
    let ammo = peek_ammo_state(state, ctx.world, e).and_then(|id| ctx.content.bullet(id));
    let missiles = ammo.is_none_or(|def| def.target_missiles);
    let buildings =
        config.target_ground && config.target_blocks && ammo.is_none_or(|def| def.target_blocks);

    let find = |query_range: f32| -> Option<Entity> {
        if config.target_air && !config.target_ground {
            // `Units.bestEnemy` with `!e.isGrounded()` (`Turret.java:645`): the
            // predicate is an ACCEPT filter and Java's `isGrounded()` is
            // `elevation < 0.001` (`UnitComp.java:80-82`), so air-only turrets
            // keep every non-grounded unit — not just `isFlying()`
            // (`elevation >= 0.09`).
            targets.best_enemy(
                ctx.world,
                ctx.content,
                team,
                x,
                y,
                query_range,
                sort,
                &mut |world, _content, unit| !unit_is_grounded(world, unit),
            )
        } else {
            // `Units.bestTarget` (`Turret.java:647-651`): the unit gate is
            // `(e.isGrounded() || targetAir) && (!e.isGrounded() || targetGround)`
            // — grounded needs `targetGround`, non-grounded needs `targetAir` —
            // and it skips missiles unless the ammo allows them.
            targets.best_target(
                ctx.world,
                ctx.content,
                team,
                x,
                y,
                query_range,
                sort,
                &mut |world, _content, unit| {
                    let grounded = unit_is_grounded(world, unit);
                    (grounded || config.target_air)
                        && (!grounded || config.target_ground)
                        && (missiles
                            || world
                                .get::<crate::entities::comp::unit::comp::TimedKillComp>(unit)
                                .is_none())
                },
                buildings,
                config.target_under_blocks,
            )
        }
    };

    let mut target = find(range);
    // `findTarget`: a different tracking range gets a second chance only when
    // nothing was found in the firing range (`Turret.java:655-658`;
    // `Mathf.equal`).
    if (track_range - range).abs() > f32::EPSILON && target.is_none() {
        target = find(track_range);
    }
    if target.is_none() && can_heal(state, ctx.world, ctx.content, e) {
        target = targets.find_ally_tile(ctx.world, team, x, y, range, Some(e));
    }
    state.target = target;
    if let Some(target) = target {
        state.target_pos = position(ctx.world, target).unwrap_or((x, y));
    }
}

/// `TurretBuild.canHeal` (`Turret.java:634-636`): `targetHealing && hasAmmo()
/// && peekAmmo().collidesTeam && peekAmmo().heals()`.
pub(super) fn can_heal(state: &TurretState, world: &World, content: &ContentRegistry, e: Entity) -> bool {
    if !state.config.target_healing || !has_ammo_state(state, world, e) {
        return false;
    }
    peek_ammo_state(state, world, e)
        .and_then(|id| content.bullet(id))
        .is_some_and(|def| def.collides_team && def.heals())
}

/// `TurretBuild.minRange()` (`Turret.java:340-344`): `minRange +
/// peekAmmo().minRangeChange`.
pub fn min_range_of(state: &TurretState, world: &World, e: Entity) -> f32 {
    state.config.min_range + peek_ammo_min_range_change_of(state, world, e)
}

/// `TurretBuild.trackingRange()` (`Turret.java:355-356`): the init-maxed
/// tracking range shifted by the ammo's `rangeChange`.
pub fn tracking_range_of(state: &TurretState, world: &World, e: Entity) -> f32 {
    state.config.tracking_range + peek_ammo_range_change_of(state, world, e)
}

/// `Turret.findEnemy`'s sort (`Turret.java:643`): `ammo.unitSort != closest ?
/// ammo.unitSort : unitSort`.
pub(super) fn ammo_unit_sort(
    state: &TurretState,
    world: &World,
    e: Entity,
    content: &ContentRegistry,
) -> UnitSort {
    peek_ammo_state(state, world, e)
        .and_then(|id| content.bullet(id))
        .map(|def| def.unit_sort)
        .filter(|sort| *sort != UnitSort::Closest)
        .unwrap_or(state.config.unit_sort)
}

/// `Turret.targetPosition` (`Turret.java:459-479`): the per-tick aim point for
/// the current ammo, leading the target through `Predict.intercept`.
pub(super) fn target_position(
    ctx: &CombatCtx<'_>,
    e: Entity,
    state: &mut TurretState,
    x: f32,
    y: f32,
    target: Entity,
) {
    if !has_ammo_state(state, ctx.world, e) {
        return;
    }
    let Some((tx, ty)) = position(ctx.world, target) else {
        return;
    };
    let speed = peek_ammo_state(state, ctx.world, e)
        .and_then(|id| ctx.content.bullet(id))
        .map(|def| def.speed)
        .unwrap_or(0.0);
    // Target per-tick displacement (`Hitboxc.deltaX/deltaY`); buildings carry
    // no `Vel` and are static here.
    let delta = ctx
        .world
        .get::<crate::entities::comp::Vel>(target)
        .copied()
        .unwrap_or(crate::entities::comp::Vel { x: 0.0, y: 0.0 });
    // `when delay is accurate, assume unit has moved by chargeTime already`.
    let config = &state.config;
    let offset = if config.accurate_delay && !config.move_while_charging {
        (
            delta.x * config.shoot.first_shot_delay,
            delta.y * config.shoot.first_shot_delay,
        )
    } else {
        (0.0, 0.0)
    };
    let aim = if config.predict_target && speed >= 0.01 {
        // `Predict.intercept(this, pos, offset.x, offset.y, bullet.speed)`:
        // the turret is not a `Hitboxc`, so the source velocity is zero.
        crate::combat::targeting::intercept_positions(
            (x, y),
            (0.0, 0.0),
            (tx + offset.0, ty + offset.1),
            (delta.x, delta.y),
            true,
            speed,
        )
    } else {
        (tx, ty)
    };
    // `if(targetPos.isZero()) targetPos.set(pos)`.
    state.target_pos = if aim.0 == 0.0 && aim.1 == 0.0 {
        (tx, ty)
    } else {
        aim
    };
}

/// `Turret.updateTile`'s `within(target, range() + target.hitSize()/1.9f)`
/// (`Turret.java:587`): the fire gate uses the TARGET'S position, not the
/// intercept point. Buildings use `BuildingComp.hitSize() = size * tilesize`
/// (`BuildingComp.java:2054`).
pub(super) fn within_target_range(ctx: &CombatCtx<'_>, target: Entity, x: f32, y: f32, range: f32) -> bool {
    let Some((tx, ty)) = position(ctx.world, target) else {
        return false;
    };
    let hit_size = ctx
        .world
        .get::<crate::entities::comp::unit::comp::HitboxComp>(target)
        .map(|hitbox| hitbox.hit_size)
        .or_else(|| {
            ctx.world
                .get::<crate::entities::comp::Building>(target)
                .and_then(|building| ctx.content.block(building.block))
                .map(|def| def.size as f32 * crate::world::block::TILE_SIZE)
        })
        .unwrap_or(0.0);
    let limit = range + hit_size / 1.9;
    let dx = tx - x;
    let dy = ty - y;
    dx * dx + dy * dy <= limit * limit
}

/// `TurretBuild.validateTarget`.
pub(super) fn validate_target(ctx: &CombatCtx<'_>, _e: Entity, state: &TurretState) -> bool {
    let Some(target) = state.target else {
        return false;
    };
    if ctx.world.get_entity(target).is_err() {
        return false;
    }
    // `Units.invalidateTarget` (`Units.java:151-157`): the dead-target rule is
    // `target instanceof Healthc h && !h.isValid()` — entities without health
    // (a `Fire` the liquid turret is extinguishing) stay valid.
    if ctx.world.get::<Health>(target).is_some_and(|h| h.dead) {
        return false;
    }
    true
}
