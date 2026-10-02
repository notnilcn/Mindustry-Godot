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
