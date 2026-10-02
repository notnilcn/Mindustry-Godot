// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Hand-ported `Fx.java` bodies (wave F1/F2 subset). Each function mirrors the
//! corresponding Java `Cons<EffectContainer>` body.

use crate::content::Rgba;
use crate::render::draw::RegionKey;
use crate::render::drawf::pal;

use super::super::angles::{
    angle, rand_len_vectors, rand_len_vectors_cone, random_seed_range, sign, trnsx, trnsy,
};
use super::super::container::EffectContainer;
use super::super::data::{EffectData, Pose, ViewSnapshot};
use super::FxEmit;
use crate::math::Interp;

#[inline]
fn lerp(a: f32, b: f32, f: f32) -> f32 {
    a + (b - a) * f
}

/// Content-derived regions (block/unit icons) resolve through plan 03; until
/// then custom bodies use the missing-region sentinel (plan 17 §3.6).
const CONTENT_REGION: RegionKey = RegionKey::ERROR;

fn unit_pose(e: &EffectContainer, snapshot: &dyn ViewSnapshot) -> Option<Pose> {
    if let EffectData::Unit { id, .. } = &e.data {
        snapshot.unit_pose(*id)
    } else {
        None
    }
}

/// `Fx.blockCrash`.
pub fn block_crash(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    if !matches!(e.data, EffectData::Block(_)) {
        return;
    }
    let offset = lerp(0.0, 180.0, e.fout());
    emit.alpha(e.fin() + 0.5);
    emit.color(Rgba::new(0.0, 0.0, 0.0, 0.44));
    emit.rect(CONTENT_REGION, e.x - offset * 4.0, e.y, 8.0, 8.0, 0.0);
    emit.color(Rgba::WHITE);
    emit.rect(
        CONTENT_REGION,
        e.x + offset,
        e.y + offset * 5.0,
        8.0,
        8.0,
        0.0,
    );
}

/// `Fx.trailFade`: the trail geometry is emitted by the gdext trail registry;
/// the core body only carries the lifetime hand-off (plan 17 §3.9).
pub fn trail_fade(_emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    if let EffectData::Trail(_) = e.data {
        // `e.lifetime = trail.length * 1.4f` is resolved in gdext from the
        // detach payload; the program's lifetime override is applied there.
    }
}

/// `Fx.unitSpawn`.
pub fn unit_spawn(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    if !matches!(e.data, EffectData::UnitType(_) | EffectData::Unit { .. }) {
        return;
    }
    let scl = 1.0 + e.fout() * 2.0;
    emit.alpha(e.fout());
    emit.mixcol(Rgba::WHITE, e.fin());
    emit.rect_native(CONTENT_REGION, e.x, e.y, 180.0);
    emit.reset();
    emit.alpha(e.fin());
    emit.rect(
        CONTENT_REGION,
        e.x,
        e.y,
        32.0 * scl,
        32.0 * scl,
        e.rotation - 90.0,
    );
}

/// `Fx.unitControl`.
pub fn unit_control(emit: &mut FxEmit, e: &EffectContainer, snap: &dyn ViewSnapshot) {
    let Some(pose) = unit_pose(e, snap) else {
        return;
    };
    emit.mixcol(pal::ACCENT, 1.0);
    emit.alpha(e.fout());
    emit.rect(
        CONTENT_REGION,
        pose.x,
        pose.y,
        32.0,
        32.0,
        pose.rotation - 90.0,
    );
    emit.alpha(1.0);
    emit.stroke(e.fslope());
    emit.square_line(pose.x, pose.y, e.fout() * 20.0, 45.0);
    emit.stroke(e.fslope() * 2.0);
    emit.square_line(pose.x, pose.y, e.fout() * 30.0, 45.0);
    emit.reset();
}

/// `Fx.unitDespawn`.
pub fn unit_despawn(emit: &mut FxEmit, e: &EffectContainer, snap: &dyn ViewSnapshot) {
    if !matches!(e.data, EffectData::Unit { .. }) {
        return;
    }
    let Some(pose) = unit_pose(e, snap) else {
        return;
    };
    emit.mixcol(pal::ACCENT, 1.0);
    emit.rect(
        CONTENT_REGION,
        pose.x,
        pose.y,
        32.0,
        32.0,
        pose.rotation - 90.0,
    );
    emit.reset();
}

/// `Fx.unitSpirit`.
pub fn unit_spirit(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    let EffectData::Position { x: tx, y: ty } = e.data else {
        return;
    };
    emit.color(pal::ACCENT);
    let f1 = Interp::Pow2In.apply(e.fin());
    let x1 = lerp(e.x, tx, f1);
    let y1 = lerp(e.y, ty, f1);
    let size = 2.5 * e.fin();
    emit.square(x1, y1, 1.5 * size, 45.0);
    let f2 = Interp::Pow5In.apply(e.fin());
    let x2 = lerp(e.x, tx, f2);
    let y2 = lerp(e.y, ty, f2);
    emit.square(x2, y2, size, 45.0);
}

/// `Fx.itemTransfer`.
pub fn item_transfer(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    let EffectData::Position { x: tx, y: ty } = e.data else {
        return;
    };
    let f = Interp::Pow3.apply(e.fin());
    // `.nor().rotate90(1).scl(randomSeedRange(id,1) * fslope * 10)`
    let dx = tx - e.x;
    let dy = ty - e.y;
    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
    let nx = -dy / len;
    let ny = dx / len;
    let off = random_seed_range(e.id.raw() as i64, 1.0) * e.fslope() * 10.0;
    let x = lerp(e.x, tx, f) + nx * off;
    let y = lerp(e.y, ty, f) + ny * off;
    emit.color(pal::ACCENT);
    emit.circle(x, y, e.fslope() * 3.0);
    emit.color(e.color);
    emit.circle(x, y, e.fslope() * 1.5);
}

/// `Fx.pointBeam`.
pub fn point_beam(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    let EffectData::Position { x: tx, y: ty } = e.data else {
        return;
    };
    emit.color(e.color.with_alpha(e.fout()));
    emit.stroke(1.5);
    emit.line(e.x, e.y, tx, ty, true);
    emit.light(e.x, e.y, 20.0, e.color, 0.6 * e.fout());
}

/// `Fx.pointHit`.
pub fn point_hit(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color_lerp(Rgba::WHITE, e.color, e.fin());
    emit.stroke(e.fout() + 0.2);
    emit.circle_line(e.x, e.y, e.fin() * 6.0);
}

/// `Fx.hitScepterSecondary`.
pub fn hit_scepter_secondary(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    for i in [-1_i32, 1] {
        emit.color_lerp(pal::BULLET_YELLOW, pal::BULLET_YELLOW_BACK, e.fout() * 1.2);
        emit.tri(
            e.x,
            e.y,
            e.fout() * 0.2 + 2.0,
            5.0 + 30.0 * e.fout(),
            e.rotation + 155.0 * i as f32,
        );
    }
    let mut rng = crate::math::ArcRand::new(e.id.raw() as u64);
    let count = rng.random_range_int(1, 5);
    for _ in 0..count {
        let stroke = rng.random_range_float(0.5 * e.fin(), e.fin());
        let ang = rng.random_range_float(e.rotation - 20.0, e.rotation + 20.0);
        let dist = rng.random_range_float(2.0, 40.0) * e.fin();
        emit.alpha(e.fout() * rng.random_range_float(0.4, 2.0));
        emit.color_lerp(pal::SURGE, Rgba::WHITE, e.fin() * 0.8);
        emit.stroke(stroke * 1.5 * e.fin() + 0.2);
        let len = rng.random_range_float(3.0, 9.0) + 1.5 * e.fin();
        emit.line_angle(e.x + trnsx(ang, dist), e.y + trnsy(ang, dist), ang, len);
    }
}

/// `Fx.lightning`.
pub fn lightning(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    let EffectData::Positions(points) = &e.data else {
        return;
    };
    emit.stroke(3.0 * e.fout());
    emit.color_lerp(e.color, Rgba::WHITE, e.fin());
    for i in 0..points.len().saturating_sub(1) {
        let cur = points[i];
        let next = points[i + 1];
        emit.line(cur.0, cur.1, next.0, next.1, false);
    }
    for p in points {
        emit.circle(p.0, p.1, emit.get_stroke() / 2.0);
    }
}

/// `Fx.coreBuildShockwave`.
pub fn core_build_shockwave(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.program.lifetime_override = Some(e.rotation);
    emit.color(pal::COMMAND);
    emit.stroke(e.fout_with(Interp::Pow5Out) * 4.0);
    emit.circle_line(e.x, e.y, e.fin() * e.rotation * 2.0);
}

/// `Fx.coreBuildBlock`.
pub fn core_build_block(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    if !matches!(e.data, EffectData::Block(_)) {
        return;
    }
    emit.mixcol(pal::ACCENT, 1.0);
    emit.alpha(e.fout());
    emit.rect_native(CONTENT_REGION, e.x, e.y, 0.0);
}

/// `Fx.pointShockwave`.
pub fn point_shockwave(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(e.color);
    emit.stroke(e.fout() * 2.0);
    emit.circle_line(e.x, e.y, e.finpow() * e.rotation);
    emit.stroke(1.0);
    rand_len_vectors(e.id.raw() as u64 + 1, 8, 1.0 + 23.0 * e.finpow(), |x, y| {
        emit.line_angle(e.x + x, e.y + y, angle(x, y), 1.0 + e.fout() * 3.0);
    });
}

/// `Fx.moveCommand`.
pub fn move_command(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::COMMAND);
    emit.stroke(e.fout() * 5.0);
    emit.circle_line(e.x, e.y, 6.0 + e.fin() * 2.0);
}

/// `Fx.attackCommand`.
pub fn attack_command(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::REMOVE);
    emit.stroke(e.fout() * 5.0);
    emit.poly_line(e.x, e.y, 4, 7.0 + e.fin() * 2.0, 45.0);
}

/// `Fx.placeBlock`.
pub fn place_block(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::ACCENT);
    emit.stroke(3.0 - e.fin() * 2.0);
    emit.square_line(e.x, e.y, 4.0 * e.rotation + e.fin() * 3.0, 45.0);
}

/// `Fx.tapBlock`.
pub fn tap_block(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::ACCENT);
    emit.stroke(3.0 - e.fin() * 2.0);
    emit.circle_line(e.x, e.y, 4.0 + (32.0 / 1.5 * e.rotation) * e.fin());
}

/// `Fx.breakBlock`.
pub fn break_block(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::REMOVE);
    emit.stroke(3.0 - e.fin() * 2.0);
    emit.square_line(e.x, e.y, 4.0 * e.rotation + e.fin() * 3.0, 45.0);
    emit.stroke(1.0);
    let count = 3 + (e.rotation * 3.0) as i32;
    let length = e.rotation * 2.0 + (32.0 * e.rotation) * e.finpow();
    rand_len_vectors(e.id.raw() as u64, count, length, |x, y| {
        emit.square(e.x + x, e.y + y, 1.0 + e.fout() * (3.0 + e.rotation), 45.0);
    });
}

/// `Fx.payloadDeposit` (simplified: no live payload item).
pub fn payload_deposit(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.mixcol(pal::ACCENT, e.fin());
    emit.alpha(e.fout());
    emit.circle(e.x, e.y, 4.0 * e.fout_with(Interp::Pow3Out));
    emit.reset();
}

/// `Fx.select`.
pub fn select(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::ACCENT);
    emit.stroke(e.fout() * 3.0);
    emit.circle_line(e.x, e.y, 3.0 + e.fin() * 14.0);
}

/// `Fx.hitBulletSmall` / `Fx.hitBulletColor` shared body.
fn hit_bullet(emit: &mut FxEmit, e: &EffectContainer, tint: Rgba) {
    emit.color_lerp(Rgba::WHITE, tint, e.fin());
    e.scaled_view(7.0, |s| {
        emit.stroke(0.5 + s.fout());
        emit.circle_line(e.x, e.y, s.fin() * 5.0);
    });
    emit.stroke(0.5 + e.fout());
    rand_len_vectors(e.id.raw() as u64, 5, e.fin() * 15.0, |x, y| {
        emit.line_angle(e.x + x, e.y + y, angle(x, y), e.fout() * 3.0 + 1.0);
    });
    emit.light(e.x, e.y, 20.0, tint, 0.6 * e.fout());
}

/// `Fx.hitBulletSmall`.
pub fn hit_bullet_small(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    hit_bullet(emit, e, pal::LIGHT_ORANGE.into());
}

/// `Fx.hitBulletColor`.
pub fn hit_bullet_color(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    let tint = e.color;
    hit_bullet(emit, e, tint);
}

/// `Fx.hitBulletBig`.
pub fn hit_bullet_big(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color_lerp(Rgba::WHITE, pal::LIGHT_ORANGE, e.fin());
    emit.stroke(0.5 + e.fout() * 1.5);
    rand_len_vectors_cone(
        e.id.raw() as u64,
        8,
        e.finpow() * 30.0,
        e.rotation,
        50.0,
        |x, y| {
            emit.line_angle(e.x + x, e.y + y, angle(x, y), e.fout() * 4.0 + 1.5);
        },
    );
}

/// `Fx.hitFlameSmall`.
pub fn hit_flame_small(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color_lerp(pal::LIGHT_FLAME, pal::DARK_FLAME, e.fin());
    emit.stroke(0.5 + e.fout());
    rand_len_vectors_cone(
        e.id.raw() as u64,
        2,
        1.0 + e.fin() * 15.0,
        e.rotation,
        50.0,
        |x, y| {
            emit.line_angle(e.x + x, e.y + y, angle(x, y), e.fout() * 3.0 + 1.0);
        },
    );
}

/// `Fx.hitLiquid`.
pub fn hit_liquid(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(e.color);
    rand_len_vectors_cone(
        e.id.raw() as u64,
        5,
        1.0 + e.fin() * 15.0,
        e.rotation,
        60.0,
        |x, y| {
            emit.circle(e.x + x, e.y + y, e.fout() * 2.0);
        },
    );
}

/// `Fx.shootSmall`.
pub fn shoot_small(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color_lerp(pal::LIGHTER_ORANGE, pal::LIGHT_ORANGE, e.fin());
    let w = 1.0 + 5.0 * e.fout();
    emit.tri(e.x, e.y, w, 15.0 * e.fout(), e.rotation);
    emit.tri(e.x, e.y, w, 3.0 * e.fout(), e.rotation + 180.0);
}

/// `Fx.shootBig`.
pub fn shoot_big(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color_lerp(pal::LIGHTER_ORANGE, pal::LIGHT_ORANGE, e.fin());
    let w = 1.2 + 7.0 * e.fout();
    emit.tri(e.x, e.y, w, 25.0 * e.fout(), e.rotation);
    emit.tri(e.x, e.y, w, 4.0 * e.fout(), e.rotation + 180.0);
}

/// `Fx.casing1`.
pub fn casing1(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color_lerp3(
        pal::LIGHT_ORANGE,
        pal::LIGHT_GRAY,
        pal::LIGHTISH_GRAY,
        e.fin(),
    );
    emit.alpha(e.fout_margin(0.3));
    let rot = e.rotation.abs() + 90.0;
    let i = -sign(e.rotation);
    let len = (2.0 + e.finpow() * 6.0) * i as f32;
    let lr = rot + e.fin() * 30.0 * i as f32;
    let jitter_x = random_seed_range(e.id.raw() as i64 + i as i64 + 7, 3.0 * e.fin());
    let jitter_y = random_seed_range(e.id.raw() as i64 + i as i64 + 8, 3.0 * e.fin());
    emit.rect_fill(
        e.x + trnsx(lr, len) + jitter_x,
        e.y + trnsy(lr, len) + jitter_y,
        1.0,
        2.0,
        rot + e.fin() * 50.0 * i as f32,
    );
}

/// `Fx.healWave`.
pub fn heal_wave(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::HEAL);
    emit.stroke(e.fout() * 2.0);
    emit.circle_line(e.x, e.y, 4.0 + e.finpow() * 60.0);
}

/// `Fx.shockwave`.
pub fn shockwave(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color_lerp(Rgba::WHITE, Rgba::LIGHT_GRAY, e.fin());
    emit.stroke(e.fout() * 2.0 + 0.2);
    emit.circle_line(e.x, e.y, e.fin() * 28.0);
}

/// `Fx.smoke`.
pub fn smoke(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color_lerp(Rgba::GRAY, pal::DARKISH_GRAY, e.fin());
    emit.circle(e.x, e.y, (7.0 - e.fin() * 7.0) / 2.0);
}

/// `Fx.explosion`.
pub fn explosion(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    e.scaled_view(7.0, |s| {
        emit.stroke(3.0 * s.fout());
        emit.circle_line(e.x, e.y, 3.0 + s.fin() * 10.0);
    });
    emit.color(Rgba::GRAY);
    rand_len_vectors(e.id.raw() as u64, 6, 2.0 + 19.0 * e.finpow(), |x, y| {
        emit.circle(e.x + x, e.y + y, e.fout() * 3.0 + 0.5);
        emit.circle(e.x + x / 2.0, e.y + y / 2.0, e.fout());
    });
    emit.color_lerp3(pal::LIGHTER_ORANGE, pal::LIGHT_ORANGE, Rgba::GRAY, e.fin());
    emit.stroke(1.5 * e.fout());
    rand_len_vectors(e.id.raw() as u64 + 1, 8, 1.0 + 23.0 * e.finpow(), |x, y| {
        emit.line_angle(e.x + x, e.y + y, angle(x, y), 1.0 + e.fout() * 3.0);
    });
}

/// `Fx.hitLaser`.
pub fn hit_laser(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color_lerp(Rgba::WHITE, pal::HEAL, e.fin());
    emit.stroke(0.5 + e.fout());
    emit.circle_line(e.x, e.y, e.fin() * 5.0);
    emit.light(e.x, e.y, 23.0, pal::HEAL, e.fout() * 0.7);
}

// ---------------------------------------------------------------------------
// Wave F3/F4 bodies (plan 17 M3). Ported verbatim from `content/Fx.java`.
// ---------------------------------------------------------------------------

/// `Color.mul(f)` channel-scale used by the dust/wreck bodies.
#[inline]
fn mul(color: Rgba, factor: f32) -> Rgba {
    Rgba::new(
        color.r * factor,
        color.g * factor,
        color.b * factor,
        color.a,
    )
}

/// `Mathf.clamp(f)` (`0..1`).
#[inline]
fn clamp01(f: f32) -> f32 {
    f.clamp(0.0, 1.0)
}

/// A view-seeded `Rand` matching `Fx.rand.setSeed(e.id)`.
#[inline]
fn view_rand(e: &EffectContainer) -> crate::math::ArcRand {
    crate::math::ArcRand::new(e.id.raw() as u64)
}

/// `Fx.commandSend`.
pub fn command_send(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::COMMAND);
    emit.stroke(e.fout() * 2.0);
    emit.circle_line(e.x, e.y, 4.0 + e.finpow() * e.rotation);
}

/// `Fx.upgradeCoreBloom`.
pub fn upgrade_core_bloom(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::ACCENT);
    emit.stroke(4.0 * e.fout());
    emit.square_line(e.x, e.y, 4.0 * e.rotation + 2.0, 0.0);
}

/// `Fx.coreLaunchConstruct`.
pub fn core_launch_construct(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::ACCENT);
    emit.stroke(4.0 - e.fin() * 3.0);
    emit.square_line(e.x, e.y, 4.0 * e.rotation * 1.2 + e.fin() * 5.0, 0.0);
    let count = 5 + (e.rotation * 5.0) as i32;
    let length = e.rotation * 3.0 + (8.0 * e.rotation) * e.finpow() * 1.5;
    rand_len_vectors(e.id.raw() as u64, count, length, |x, y| {
        emit.line_angle(
            e.x + x,
            e.y + y,
            angle(x, y),
            1.0 + e.fout() * (4.0 + e.rotation),
        );
    });
}

/// `Fx.fallSmoke`.
pub fn fall_smoke(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color_lerp(Rgba::GRAY, pal::DARK_GRAY, e.rotation);
    emit.circle(e.x, e.y, e.fout() * 3.5);
}

/// `Fx.rocketSmoke`.
pub fn rocket_smoke(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(Rgba::GRAY);
    emit.alpha(clamp01(
        e.fout() * 1.6 - Interp::Pow3In.apply(e.rotation) * 1.2,
    ));
    emit.circle(e.x, e.y, (1.0 + 6.0 * e.rotation) - e.fin() * 2.0);
}

/// `Fx.rocketSmokeLarge`.
pub fn rocket_smoke_large(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(Rgba::GRAY);
    emit.alpha(clamp01(
        e.fout() * 1.6 - Interp::Pow3In.apply(e.rotation) * 1.2,
    ));
    emit.circle(e.x, e.y, (1.0 + 6.0 * e.rotation * 1.3) - e.fin() * 2.0);
}

/// `Fx.magmasmoke`.
pub fn magma_smoke(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(Rgba::GRAY);
    emit.circle(e.x, e.y, e.fslope() * 6.0);
}

/// `Fx.spawn`.
pub fn spawn(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.stroke(2.0 * e.fout());
    emit.color(pal::ACCENT);
    emit.poly_line(e.x, e.y, 4, 5.0 + e.fin() * 12.0, 0.0);
}

/// `Fx.padlaunch`.
pub fn padlaunch(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.stroke(4.0 * e.fout());
    emit.color(pal::ACCENT);
    emit.poly_line(e.x, e.y, 4, 5.0 + e.fin() * 60.0, 0.0);
}

/// `Fx.breakProp`.
pub fn break_prop(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    let scl = e.rotation.max(1.0);
    emit.color(mul(e.color, 1.1));
    rand_len_vectors(e.id.raw() as u64, 6, 19.0 * e.finpow() * scl, |x, y| {
        emit.circle(e.x + x, e.y + y, e.fout() * 3.5 * scl + 0.3);
    });
}

/// `Fx.unitDrop`.
pub fn unit_drop(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::LIGHTISH_GRAY);
    rand_len_vectors(e.id.raw() as u64, 9, 3.0 + 20.0 * e.finpow(), |x, y| {
        emit.circle(e.x + x, e.y + y, e.fout() * 4.0 + 0.4);
    });
}

/// `Fx.unitLand`.
pub fn unit_land(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(mul(e.color, 1.1));
    rand_len_vectors(e.id.raw() as u64, 6, 17.0 * e.finpow(), |x, y| {
        emit.circle(e.x + x, e.y + y, e.fout() * 4.0 + 0.3);
    });
}

/// `Fx.unitDust`.
pub fn unit_dust(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(mul(e.color, 1.3));
    rand_len_vectors_cone(
        e.id.raw() as u64,
        3,
        8.0 * e.finpow(),
        e.rotation,
        30.0,
        |x, y| {
            emit.circle(e.x + x, e.y + y, e.fout() * 3.0 + 0.3);
        },
    );
}

/// `Fx.unitLandSmall`.
pub fn unit_land_small(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(mul(e.color, 1.1));
    rand_len_vectors(
        e.id.raw() as u64,
        (6.0 * e.rotation) as i32,
        12.0 * e.finpow() * e.rotation,
        |x, y| {
            emit.circle(e.x + x, e.y + y, e.fout() * 3.0 + 0.1);
        },
    );
}

/// `Fx.unitPickup`.
pub fn unit_pickup(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::LIGHTISH_GRAY);
    emit.stroke(e.fin() * 2.0);
    emit.poly_line(e.x, e.y, 4, 13.0 * e.fout(), 0.0);
}

/// `Fx.crawlDust`.
pub fn crawl_dust(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(mul(e.color, 1.6));
    rand_len_vectors(e.id.raw() as u64, 2, 10.0 * e.finpow(), |x, y| {
        emit.circle(e.x + x, e.y + y, e.fslope() * 4.0 + 0.3);
    });
}

/// `Fx.landShock`.
pub fn land_shock(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::LANCER_LASER);
    emit.stroke(e.fout() * 3.0);
    emit.poly_line(e.x, e.y, 12, 20.0 * e.fout(), 0.0);
}

/// `Fx.pickup`.
pub fn pickup(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::LIGHTISH_GRAY);
    emit.stroke(e.fout() * 2.0);
    emit.spikes(e.x, e.y, 1.0 + e.fin() * 6.0, e.fout() * 4.0, 6);
}

/// `Fx.sparkExplosion`.
pub fn spark_explosion(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(e.color);
    emit.stroke(e.fout() * 3.0);
    emit.circle_line(e.x, e.y, 6.0 + e.finpow() * e.rotation);
    let mut rand = view_rand(e);
    for _ in 0..16 {
        let a = rand.random_float(360.0);
        let len_rand = rand.random_range_float(0.5, 1.0);
        let len = e.foutpow() * e.rotation * 0.8 * rand.random_range_float(0.6, 1.0) + 2.0;
        let offset = e.finpow() * e.rotation * 1.2 * len_rand + 6.0;
        emit.line_angle_offset(e.x, e.y, a, len, offset);
    }
}

/// Shared body for the `titanExplosion*` family.
fn titan_explosion_body(
    emit: &mut FxEmit,
    e: &EffectContainer,
    circle_to: f32,
    rays: i32,
    len_base: f32,
    len_mul: f32,
) {
    emit.color(e.color);
    emit.stroke(e.fout() * 3.0);
    emit.circle_line(e.x, e.y, 6.0 + e.finpow() * circle_to);
    let mut rand = view_rand(e);
    for _ in 0..rays {
        let a = rand.random_float(360.0);
        let len_rand = rand.random_range_float(0.5, 1.0);
        let len = e.foutpow() * len_base * rand.random_range_float(0.6, 1.0) + 2.0;
        let offset = e.finpow() * len_mul * len_rand + 6.0;
        emit.line_angle_offset(e.x, e.y, a, len, offset);
    }
}

/// `Fx.titanExplosion`.
pub fn titan_explosion(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    titan_explosion_body(emit, e, 60.0, 16, 50.0, 70.0);
}

/// `Fx.titanExplosionLarge`.
pub fn titan_explosion_large(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    titan_explosion_body(emit, e, 110.0, 21, 50.0, 100.0);
}

/// `Fx.titanExplosionSmall`.
pub fn titan_explosion_small(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    titan_explosion_body(emit, e, 45.0, 12, 50.0, 50.0);
}

/// `Fx.titanExplosionFrag`.
pub fn titan_explosion_frag(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(e.color);
    emit.stroke(e.fout() * 2.0);
    let circle_rad = 6.0 + e.finpow() * 20.0;
    emit.circle_line(e.x, e.y, circle_rad);
    let mut rand = view_rand(e);
    for _ in 0..8 {
        let a = rand.random_float(360.0);
        let len_rand = rand.random_range_float(0.5, 1.0);
        let vx = trnsx(a, circle_rad);
        let vy = trnsy(a, circle_rad);
        for s in [-1.0f32, 1.0] {
            emit.tri(
                e.x + vx,
                e.y + vy,
                e.foutpow() * 15.0,
                e.fout() * 20.0 * len_rand + 6.0,
                a + 90.0 + s * 90.0,
            );
        }
    }
}

/// `Fx.coreExplosion`.
pub fn core_explosion(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(e.color);
    emit.stroke(e.fout() * 4.0);
    emit.circle_line(e.x, e.y, 6.0 + e.finpow() * 120.0);
    emit.stroke(e.fout() * 2.5);
    let mut rand = view_rand(e);
    for _ in 0..30 {
        let a = rand.random_float(360.0);
        let len_rand = rand.random_range_float(0.5, 1.0);
        let len = e.foutpow() * 50.0 * rand.random_range_float(0.6, 1.0) + 2.0;
        emit.line_angle_offset(e.x, e.y, a, len, e.finpow() * 100.0 * len_rand + 6.0);
    }
    for _ in 0..30 {
        let a = rand.random_float(360.0);
        let len_rand = rand.random_range_float(0.5, 1.0);
        let speed = rand.random_range_float(0.6, 1.0);
        let fin = e.finpow() / rand.random_range_float(0.3, 1.0);
        let fout = 1.0 - fin;
        if fin < 1.0 {
            emit.stroke(fout * 2.0);
            emit.line_angle_offset(
                e.x,
                e.y,
                a,
                Interp::Pow3Out.apply(fin) * 80.0 * speed + 2.0,
                fin * 100.0 * len_rand + 6.0,
            );
        }
    }
}

/// `Fx.smokeAoeCloud`.
pub fn smoke_aoe_cloud(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(e.color.with_alpha(0.65));
    rand_len_vectors(e.id.raw() as u64, 80, 90.0, |x, y| {
        let f = clamp01(e.fin() / 0.1) * clamp01(e.fout() / 0.1);
        emit.circle(e.x + x, e.y + y, 6.0 * f);
    });
}

/// `Fx.scatheExplosion`.
pub fn scathe_explosion(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(e.color);
    emit.stroke(e.fout() * 5.0);
    let circle_rad = 6.0 + e.finpow() * 60.0;
    emit.circle_line(e.x, e.y, circle_rad);
    let mut rand = view_rand(e);
    for _ in 0..16 {
        let a = rand.random_float(360.0);
        let len_rand = rand.random_range_float(0.5, 1.0);
        let vx = trnsx(a, circle_rad);
        let vy = trnsy(a, circle_rad);
        for s in [-1.0f32, 1.0] {
            emit.tri(
                e.x + vx,
                e.y + vy,
                e.foutpow() * 40.0,
                e.fout() * 30.0 * len_rand + 6.0,
                a + 90.0 + s * 90.0,
            );
        }
    }
}

/// `Fx.scatheExplosionSmall`.
pub fn scathe_explosion_small(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(e.color);
    emit.stroke(e.fout() * 4.0);
    let circle_rad = 6.0 + e.finpow() * 40.0;
    emit.circle_line(e.x, e.y, circle_rad);
    let mut rand = view_rand(e);
    for _ in 0..16 {
        let a = rand.random_float(360.0);
        let len_rand = rand.random_range_float(0.5, 1.0);
        let vx = trnsx(a, circle_rad);
        let vy = trnsy(a, circle_rad);
        for s in [-1.0f32, 1.0] {
            emit.tri(
                e.x + vx,
                e.y + vy,
                e.foutpow() * 30.0,
                e.fout() * 25.0 * len_rand + 6.0,
                a + 90.0 + s * 90.0,
            );
        }
    }
}

/// `Fx.scatheLight`.
pub fn scathe_light(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(e.color.with_alpha(e.foutpow()));
    emit.circle(e.x, e.y, 6.0 + e.finpow() * 60.0);
}

/// `Fx.scatheLightSmall`.
pub fn scathe_light_small(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(e.color.with_alpha(e.foutpow()));
    emit.circle(e.x, e.y, 6.0 + e.finpow() * 40.0);
}

/// `Fx.titanLightSmall`.
pub fn titan_light_small(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(e.color.with_alpha(e.foutpow()));
    emit.circle(e.x, e.y, 6.0 + e.finpow() * 20.0);
}

/// `Fx.scatheSlash`.
pub fn scathe_slash(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(e.color);
    for s in [-1.0f32, 1.0] {
        emit.tri(
            e.x,
            e.y,
            e.fout() * 25.0,
            e.foutpow() * 66.0 + 6.0,
            e.rotation + s * 90.0,
        );
    }
}

/// `Fx.dynamicSpikes`.
pub fn dynamic_spikes(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(e.color);
    emit.stroke(e.fout() * 2.0);
    let circle_rad = 4.0 + e.finpow() * e.rotation;
    emit.circle_line(e.x, e.y, circle_rad);
    for i in 0..4 {
        emit.tri(e.x, e.y, 6.0, e.rotation * 1.5 * e.fout(), i as f32 * 90.0);
    }
    emit.color(Rgba::WHITE);
    for i in 0..4 {
        emit.tri(
            e.x,
            e.y,
            3.0,
            e.rotation * 1.45 / 3.0 * e.fout(),
            i as f32 * 90.0,
        );
    }
    emit.light(e.x, e.y, circle_rad * 1.6, pal::HEAL, e.fout());
}

/// `Fx.greenBomb`.
pub fn green_bomb(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::HEAL);
    emit.stroke(e.fout() * 2.0);
    let circle_rad = 4.0 + e.finpow() * 65.0;
    emit.circle_line(e.x, e.y, circle_rad);
    for i in 0..4 {
        emit.tri(e.x, e.y, 6.0, 100.0 * e.fout(), i as f32 * 90.0);
    }
    emit.color(Rgba::WHITE);
    for i in 0..4 {
        emit.tri(e.x, e.y, 3.0, 35.0 * e.fout(), i as f32 * 90.0);
    }
    emit.light(e.x, e.y, circle_rad * 1.6, pal::HEAL, e.fout());
}

/// `Fx.greenLaserCharge`.
pub fn green_laser_charge(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::HEAL);
    emit.stroke(e.fin() * 2.0);
    emit.circle_line(e.x, e.y, 4.0 + e.fout() * 100.0);
    emit.circle(e.x, e.y, e.fin() * 20.0);
    rand_len_vectors(e.id.raw() as u64, 20, 40.0 * e.fout(), |x, y| {
        emit.circle(e.x + x, e.y + y, e.fin() * 5.0);
        emit.light(e.x + x, e.y + y, e.fin() * 15.0, pal::HEAL, 0.7);
    });
    emit.color(Rgba::WHITE);
    emit.circle(e.x, e.y, e.fin() * 10.0);
    emit.light(e.x, e.y, e.fin() * 20.0, pal::HEAL, 0.7);
}

/// `Fx.greenLaserChargeSmall`.
pub fn green_laser_charge_small(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::HEAL);
    emit.stroke(e.fin() * 2.0);
    emit.circle_line(e.x, e.y, e.fout() * 50.0);
}

/// `Fx.greenCloud`.
pub fn green_cloud(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::HEAL);
    crate::fx::angles::rand_len_vectors_fin(
        e.id.raw() as u64,
        e.fin(),
        7,
        9.0,
        |x, y, _fin, fout| {
            emit.circle(e.x + x, e.y + y, 5.0 * fout);
        },
    );
}

/// `Fx.healWaveDynamic`.
pub fn heal_wave_dynamic(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::HEAL);
    emit.stroke(e.fout() * 2.0);
    emit.circle_line(e.x, e.y, 4.0 + e.finpow() * e.rotation);
}

/// `Fx.heal`.
pub fn heal(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::HEAL);
    emit.stroke(e.fout() * 2.0);
    emit.circle_line(e.x, e.y, 2.0 + e.finpow() * 7.0);
}

/// `Fx.dynamicWave`.
pub fn dynamic_wave(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(e.color.with_alpha(0.7));
    emit.stroke(e.fout() * 2.0);
    emit.circle_line(e.x, e.y, 4.0 + e.finpow() * e.rotation);
}

/// `Fx.shieldWave`.
pub fn shield_wave(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(e.color.with_alpha(0.7));
    emit.stroke(e.fout() * 2.0);
    emit.circle_line(e.x, e.y, 4.0 + e.finpow() * 60.0);
}

/// `Fx.shieldApply`.
pub fn shield_apply(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(e.color.with_alpha(0.7));
    emit.stroke(e.fout() * 2.0);
    emit.circle_line(e.x, e.y, 2.0 + e.finpow() * 7.0);
}

/// `Fx.hitSquaresColor`.
pub fn hit_squares_color(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color_lerp(Rgba::WHITE, e.color, e.fin());
    e.scaled_view(7.0, |s| {
        emit.stroke(0.5 + s.fout());
        emit.circle_line(e.x, e.y, s.fin() * 5.0);
    });
    emit.stroke(0.5 + e.fout());
    rand_len_vectors(e.id.raw() as u64, 5, e.fin() * 17.0, |x, y| {
        let ang = angle(x, y);
        emit.square(e.x + x, e.y + y, e.fout() * 3.2, ang);
    });
    emit.light(e.x, e.y, 20.0, e.color, 0.6 * e.fout());
}

/// `Fx.hitFuse`.
pub fn hit_fuse(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color_lerp(Rgba::WHITE, pal::SURGE, e.fin());
    e.scaled_view(7.0, |s| {
        emit.stroke(0.5 + s.fout());
        emit.circle_line(e.x, e.y, s.fin() * 7.0);
    });
    emit.stroke(0.5 + e.fout());
    rand_len_vectors(e.id.raw() as u64, 6, e.fin() * 15.0, |x, y| {
        emit.line_angle(e.x + x, e.y + y, angle(x, y), e.fout() * 3.0 + 1.0);
    });
}

/// `Fx.hitFlamePlasma`.
pub fn hit_flame_plasma(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color_lerp(Rgba::WHITE, pal::HEAL, e.fin());
    emit.stroke(0.5 + e.fout());
    rand_len_vectors_cone(
        e.id.raw() as u64,
        2,
        1.0 + e.fin() * 15.0,
        e.rotation,
        50.0,
        |x, y| {
            emit.line_angle(e.x + x, e.y + y, angle(x, y), e.fout() * 3.0 + 1.0);
        },
    );
}

/// `Fx.hitLaserBlast`.
pub fn hit_laser_blast(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(e.color);
    emit.stroke(e.fout() * 1.5);
    rand_len_vectors(e.id.raw() as u64, 8, e.finpow() * 17.0, |x, y| {
        emit.line_angle(e.x + x, e.y + y, angle(x, y), e.fout() * 4.0 + 1.0);
    });
}

/// `Fx.hitEmpSpark`.
pub fn hit_emp_spark(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::HEAL);
    emit.stroke(e.fout() * 1.6);
    rand_len_vectors_cone(
        e.id.raw() as u64,
        18,
        e.finpow() * 27.0,
        e.rotation,
        360.0,
        |x, y| {
            emit.line_angle(e.x + x, e.y + y, angle(x, y), e.fout() * 6.0 + 1.0);
        },
    );
}

/// `Fx.hitLancer`.
pub fn hit_lancer(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(Rgba::WHITE);
    emit.stroke(e.fout() * 1.5);
    rand_len_vectors(e.id.raw() as u64, 8, e.finpow() * 17.0, |x, y| {
        emit.line_angle(e.x + x, e.y + y, angle(x, y), e.fout() * 4.0 + 1.0);
    });
}

/// `Fx.hitLancerLow`.
pub fn hit_lancer_low(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(Rgba::WHITE);
    emit.stroke(e.fout() * 1.5);
    rand_len_vectors(e.id.raw() as u64, 4, e.finpow() * 17.0, |x, y| {
        emit.line_angle(e.x + x, e.y + y, angle(x, y), e.fout() * 4.0 + 1.0);
    });
}

/// `Fx.hitBeam`.
pub fn hit_beam(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(e.color);
    emit.stroke(e.fout() * 2.0);
    rand_len_vectors(e.id.raw() as u64, 6, e.finpow() * 18.0, |x, y| {
        emit.line_angle(e.x + x, e.y + y, angle(x, y), e.fout() * 4.0 + 1.0);
    });
}

/// `Fx.hitFlameBeam`.
pub fn hit_flame_beam(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(e.color);
    rand_len_vectors(e.id.raw() as u64, 7, e.finpow() * 11.0, |x, y| {
        emit.circle(e.x + x, e.y + y, e.fout() * 2.0 + 0.5);
    });
}

/// `Fx.hitMeltdown`.
pub fn hit_meltdown(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::MELTDOWN_HIT);
    emit.stroke(e.fout() * 2.0);
    rand_len_vectors(e.id.raw() as u64, 6, e.finpow() * 18.0, |x, y| {
        emit.line_angle(e.x + x, e.y + y, angle(x, y), e.fout() * 4.0 + 1.0);
    });
}

/// `Fx.hitMeltHeal`.
pub fn hit_melt_heal(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::HEAL);
    emit.stroke(e.fout() * 2.0);
    rand_len_vectors(e.id.raw() as u64, 6, e.finpow() * 18.0, |x, y| {
        emit.line_angle(e.x + x, e.y + y, angle(x, y), e.fout() * 4.0 + 1.0);
    });
}

/// `Fx.instBomb`.
pub fn inst_bomb(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    emit.color(pal::BULLET_YELLOW_BACK);
    emit.stroke(e.fout() * 4.0);
    emit.circle_line(e.x, e.y, 4.0 + e.finpow() * 20.0);
    for i in 0..4 {
        emit.tri(e.x, e.y, 6.0, 80.0 * e.fout(), i as f32 * 90.0 + 45.0);
    }
    emit.color(Rgba::WHITE);
    for i in 0..4 {
        emit.tri(e.x, e.y, 3.0, 30.0 * e.fout(), i as f32 * 90.0 + 45.0);
    }
    emit.light(e.x, e.y, 150.0, pal::BULLET_YELLOW_BACK, 0.9 * e.fout());
}

/// `Fx.instTrail`.
pub fn inst_trail(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    for i in 0..2 {
        emit.color(if i == 0 {
            Rgba::from(pal::BULLET_YELLOW_BACK)
        } else {
            Rgba::from(pal::BULLET_YELLOW)
        });
        let m = if i == 0 { 1.0 } else { 0.5 };
        let rot = e.rotation + 180.0;
        let w = 15.0 * e.fout() * m;
        emit.tri(
            e.x,
            e.y,
            w,
            (30.0 + random_seed_range(e.id.raw() as i64, 15.0)) * m,
            rot,
        );
        emit.tri(e.x, e.y, w, 10.0 * m, rot + 180.0);
    }
    emit.light(e.x, e.y, 60.0, pal::BULLET_YELLOW_BACK, 0.6 * e.fout());
}

/// `Fx.instShoot`.
pub fn inst_shoot(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    e.scaled_view(10.0, |b| {
        emit.color_lerp(Rgba::WHITE, pal::BULLET_YELLOW_BACK, b.fin());
        emit.stroke(b.fout() * 3.0 + 0.2);
        emit.circle_line(b.x, b.y, b.fin() * 50.0);
    });
    emit.color(pal::BULLET_YELLOW_BACK);
    for s in [-1.0f32, 1.0] {
        emit.tri(e.x, e.y, 13.0 * e.fout(), 85.0, e.rotation + 90.0 * s);
        emit.tri(e.x, e.y, 13.0 * e.fout(), 50.0, e.rotation + 20.0 * s);
    }
    emit.light(e.x, e.y, 180.0, pal::BULLET_YELLOW_BACK, 0.9 * e.fout());
}

/// `Fx.instHit`.
pub fn inst_hit(emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    for i in 0..2 {
        emit.color(if i == 0 {
            Rgba::from(pal::BULLET_YELLOW_BACK)
        } else {
            Rgba::from(pal::BULLET_YELLOW)
        });
        let m = if i == 0 { 1.0 } else { 0.5 };
        for j in 0..5 {
            let rot = e.rotation + random_seed_range(e.id.raw() as i64 + j, 50.0);
            let w = 23.0 * e.fout() * m;
            emit.tri(
                e.x,
                e.y,
                w,
                (80.0 + random_seed_range(e.id.raw() as i64 + j, 40.0)) * m,
                rot,
            );
            emit.tri(e.x, e.y, w, 20.0 * m, rot + 180.0);
        }
    }
    e.scaled_view(10.0, |c| {
        emit.color(pal::BULLET_YELLOW);
        emit.stroke(c.fout() * 2.0 + 0.2);
        emit.circle_line(e.x, e.y, c.fin() * 30.0);
    });
    e.scaled_view(12.0, |c| {
        emit.color(pal::BULLET_YELLOW_BACK);
        rand_len_vectors_cone(
            e.id.raw() as u64,
            25,
            5.0 + e.fin() * 80.0,
            e.rotation,
            60.0,
            |x, y| {
                emit.square(e.x + x, e.y + y, c.fout() * 3.0, 45.0);
            },
        );
    });
}
