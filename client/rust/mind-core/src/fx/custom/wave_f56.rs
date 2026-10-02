// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Catalogue waves F5–F6: the remaining 176 one-off `content/Fx.java` bodies.
//!
//! These are the effects not covered by the F1–F4 batch in [`super::basic`].
//! Each arm mirrors the corresponding Java `Cons<EffectContainer>` 1:1 (using
//! the same Arc primitive vocabulary) and is dispatched by effect name from
//! [`super::CustomFxId::Catalogue`] (the registry keeps a per-effect id; only
//! the body dispatcher is shared, plan 17 §3.4). All bodies are pure and
//! deterministic.

use crate::content::Rgba;
use crate::math::{ArcRand, Interp};
use crate::render::draw::RegionKey;
use crate::render::drawf::pal;

use super::super::angles::{
    angle, rand_len_vectors, rand_len_vectors_cone, rand_len_vectors_fin, random_seed,
    random_seed_range, trnsx, trnsy,
};
use super::super::container::EffectContainer;
use super::super::data::{EffectData, ViewSnapshot};
use super::FxEmit;

/// `Vars.tilesize`.
const TILESIZE: f32 = 32.0;

/// `Color.scarlet`.
const SCARLET: Rgba = Rgba::new(1.0, 0.204, 0.11, 1.0);
/// `Color.orange` (`0xffa500`).
const ORANGE: Rgba = Rgba::new(1.0, 0.647, 0.0, 1.0);
/// `Color.yellow` (`0xffff00`).
const YELLOW: Rgba = Rgba::new(1.0, 1.0, 0.0, 1.0);
/// `Color.gray`.
const GRAY: Rgba = Rgba::GRAY;
/// `Color.lightGray`.
const LIGHT_GRAY: Rgba = Rgba::LIGHT_GRAY;
/// `Color.darkGray`.
const DARK_GRAY: Rgba = Rgba::new(0.247, 0.247, 0.247, 1.0);
/// `Liquids.cryofluid.color` (`6ecdec`).
const CRYO: Rgba = Rgba::from_rgba8888(0x6e_cd_ec_ff);
/// `Liquids.water.color` (`596ab8`).
const WATER: Rgba = Rgba::from_rgba8888(0x59_6a_b8_ff);
/// `Liquids.slag.color` (`ffa166`).
const SLAG: Rgba = Rgba::from_rgba8888(0xff_a1_66_ff);
/// `Liquids.oil.color` (`313131`).
const OIL: Rgba = Rgba::from_rgba8888(0x31_31_31_ff);

/// Content-derived regions resolve through plan 03; until then use the
/// missing-region sentinel (plan 17 §3.6).
fn content_region() -> RegionKey {
    RegionKey::ERROR
}

#[inline]
fn lerp(a: f32, b: f32, f: f32) -> f32 {
    a + (b - a) * f
}

/// `Color.lerp` for [`Rgba`].
fn lerp_rgba(a: Rgba, b: Rgba, f: f32) -> Rgba {
    Rgba::new(
        lerp(a.r, b.r, f),
        lerp(a.g, b.g, f),
        lerp(a.b, b.b, f),
        lerp(a.a, b.a, f),
    )
}

fn seeded(e: &EffectContainer) -> ArcRand {
    ArcRand::new(e.id.raw() as u64)
}

/// `randLenVectors(e.id, amount, length, cons)` emitting filled circles.
fn cloud(
    emit: &mut FxEmit,
    e: &EffectContainer,
    amount: i32,
    length: f32,
    from: Rgba,
    to: Rgba,
    size: impl Fn(f32) -> f32,
) {
    emit.color_lerp(from, to, e.fin());
    rand_len_vectors(e.id.raw() as u64, amount, length, |x, y| {
        emit.circle(e.x + x, e.y + y, size(e.fout()));
    });
}

/// Cone variant of [`cloud`].
#[allow(clippy::too_many_arguments)]
fn cloud_cone(
    emit: &mut FxEmit,
    e: &EffectContainer,
    amount: i32,
    length: f32,
    cone: f32,
    from: Rgba,
    to: Rgba,
    size: impl Fn(f32) -> f32,
) {
    emit.color_lerp(from, to, e.fin());
    rand_len_vectors_cone(
        e.id.raw() as u64,
        amount,
        length,
        e.rotation,
        cone,
        |x, y| {
            emit.circle(e.x + x, e.y + y, size(e.fout()));
        },
    );
}

/// Cone variant emitting rotated `Fill.square`s.
fn cloud_squares(
    emit: &mut FxEmit,
    e: &EffectContainer,
    amount: i32,
    length: f32,
    from: Rgba,
    to: Rgba,
    size: impl Fn(f32) -> f32,
) {
    emit.color_lerp(from, to, e.fin());
    rand_len_vectors(e.id.raw() as u64, amount, length, |x, y| {
        emit.square(e.x + x, e.y + y, size(e.fout()), 45.0);
    });
}

/// `randLenVectors` spark fan using `Lines.lineAngle`.
#[allow(clippy::too_many_arguments)]
fn sparks(
    emit: &mut FxEmit,
    e: &EffectContainer,
    amount: i32,
    length: f32,
    from: Rgba,
    to: Rgba,
    stroke: impl Fn(f32) -> f32,
    len: impl Fn(f32) -> f32,
) {
    emit.color_lerp(from, to, e.fin());
    emit.stroke(stroke(e.fout()));
    rand_len_vectors(e.id.raw() as u64, amount, length, |x, y| {
        let a = angle(x, y);
        emit.line_angle(e.x + x, e.y + y, a, len(e.fout()));
    });
}

/// Cone variant of [`sparks`].
#[allow(clippy::too_many_arguments)]
fn sparks_cone(
    emit: &mut FxEmit,
    e: &EffectContainer,
    amount: i32,
    length: f32,
    cone: f32,
    from: Rgba,
    to: Rgba,
    stroke: impl Fn(f32) -> f32,
    len: impl Fn(f32) -> f32,
) {
    emit.color_lerp(from, to, e.fin());
    emit.stroke(stroke(e.fout()));
    rand_len_vectors_cone(
        e.id.raw() as u64,
        amount,
        length,
        e.rotation,
        cone,
        |x, y| {
            let a = angle(x, y);
            emit.line_angle(e.x + x, e.y + y, a, len(e.fout()));
        },
    );
}

/// `drawSimpleSmoke`: randomized circles that scale with `e.fin()`.
#[allow(clippy::too_many_arguments)]
fn smoke_scaled(
    emit: &mut FxEmit,
    e: &EffectContainer,
    amount: i32,
    length: f32,
    intensity: f32,
    color: Rgba,
    light: bool,
) {
    emit.color(color);
    emit.alpha(0.7);
    for i in 0..4u64 {
        let mut rng = ArcRand::new((e.id.raw() as u64).wrapping_mul(2).wrapping_add(i));
        let len_scl = rng.random_range_float(0.5, 1.0);
        let life = e.lifetime * len_scl;
        e.scaled_view(life, |inner| {
            rand_len_vectors_fin(
                (inner.id.raw() as u64).wrapping_add(i).wrapping_sub(1),
                inner.fin_with(Interp::Pow10Out),
                amount,
                length,
                |x, y, _fin, _fout| {
                    let f = inner.fout_with(Interp::Pow5Out) * rng.random_range_float(0.5, 1.0);
                    let rad = f * ((2.0 + intensity) * 2.35);
                    emit.circle(inner.x + x, inner.y + y, rad);
                    if light {
                        emit.light(inner.x + x, inner.y + y, rad * 2.5, color, 0.5);
                    }
                },
            );
        });
    }
}

/// Shared body for the three reactor/dynamic explosions (`Fx.dynamicExplosion`
/// family); `intensity`, palette and light differ per effect.
fn bomb(
    emit: &mut FxEmit,
    e: &EffectContainer,
    intensity: f32,
    smoke: Rgba,
    ring_a: Rgba,
    ring_b: Rgba,
    light_col: Rgba,
) {
    let base_lifetime = 26.0 + intensity * 15.0;
    smoke_scaled(
        emit,
        e,
        (3.0 * intensity) as i32,
        14.0 * intensity,
        intensity,
        smoke,
        false,
    );
    e.scaled_view(base_lifetime, |inner| {
        inner.scaled_view(5.0 + intensity * 2.5, |i2| {
            emit.stroke((3.1 + intensity / 5.0) * i2.fout());
            emit.circle_line(inner.x, inner.y, (3.0 + i2.fin() * 14.0) * intensity);
            emit.light(
                inner.x,
                inner.y,
                i2.fin() * 14.0 * 2.0 * intensity,
                Rgba::WHITE,
                0.9 * i2.fout(),
            );
        });
        emit.color_lerp3(ring_a, ring_b, GRAY, inner.fin());
        emit.stroke(1.7 * inner.fout() * (1.0 + (intensity - 1.0) / 2.0));
        rand_len_vectors_fin(
            inner.id.raw() as u64 + 1,
            inner.finpow() + 0.001,
            (9.0 * intensity) as i32,
            40.0 * intensity,
            |x, y, _fin, out| {
                emit.line_angle(
                    inner.x + x,
                    inner.y + y,
                    angle(x, y),
                    1.0 + out * 4.0 * (3.0 + intensity),
                );
                emit.light(
                    inner.x + x,
                    inner.y + y,
                    (out * 4.0 * (3.0 + intensity)) * 3.5,
                    emit.get_color(),
                    0.8,
                );
            },
        );
    });
    let _ = light_col;
}

/// Dispatches a wave F5–F6 body by `Fx.java` field name.
pub fn dispatch(name: &str, emit: &mut FxEmit, e: &EffectContainer, _snap: &dyn ViewSnapshot) {
    match name {
        "unitCapKill" => {
            emit.color(SCARLET);
            emit.alpha(e.fout_with(Interp::Pow4Out));
            let size = 10.0 + e.fout_with(Interp::Pow10In) * 25.0;
            emit.rect(content_region(), e.x, e.y, size, size, 0.0);
        }
        "unitEnvKill" => {
            emit.color(SCARLET);
            emit.alpha(e.fout_with(Interp::Pow4Out));
            let size = 10.0 + e.fout_with(Interp::Pow10In) * 25.0;
            emit.rect(content_region(), e.x, e.y, size, size, 0.0);
        }
        "upgradeCore" => {
            if !matches!(e.data, EffectData::Block(_)) {
                return;
            }
            emit.mixcol(
                lerp_rgba(emit.get_color(), Rgba::from(pal::ACCENT), e.fin()),
                1.0,
            );
            emit.alpha(e.fout());
            emit.rect_native(content_region(), e.x, e.y, 0.0);
        }
        "unitWreck" => {
            emit.mixcol(Rgba::from(pal::RUBBLE), 1.0);
            emit.alpha(e.fout_with(Interp::Pow5Out));
            let vel = e.fin_with(Interp::Pow5Out) * 2.0 * random_seed(e.id.raw() as i64, 1.0);
            let total_rot = random_seed(e.id.raw() as i64 + 1, 10.0);
            let ang = random_seed(e.id.raw() as i64 + 2, 360.0);
            emit.rect(
                content_region(),
                e.x + trnsx(ang, vel),
                e.y + trnsy(ang, vel),
                32.0,
                32.0,
                e.rotation - 90.0 + total_rot * e.fin_with(Interp::Pow5Out),
            );
        }
        "unitAssemble" => {
            if !matches!(e.data, EffectData::UnitType(_) | EffectData::Unit { .. }) {
                return;
            }
            emit.alpha(e.fout());
            emit.mixcol(Rgba::from(pal::ACCENT), e.fout());
            emit.rect_native(content_region(), e.x, e.y, e.rotation);
        }
        "titanSmoke" => smoke_scaled(emit, e, 8, 22.0 * 3.0, 3.0, e.color, true),
        "titanSmokeLarge" => smoke_scaled(emit, e, 11, 26.0 * 4.0, 4.0, e.color, true),
        "titanSmokeSmall" => smoke_scaled(emit, e, 7, 18.0 * 2.5, 2.5, e.color, true),
        "missileTrailSmoke" => smoke_scaled(emit, e, 5, 13.0 * 2.0, 2.0, e.color, true),
        "missileTrailSmokeSmall" => smoke_scaled(emit, e, 3, 13.0 * 1.3, 1.3, e.color, true),
        "neoplasmSplat" => {
            emit.color(Rgba::from(pal::NEOPLASM1));
            for i in 0..4u64 {
                let mut rng = ArcRand::new((e.id.raw() as u64).wrapping_mul(2).wrapping_add(i));
                let len_scl = rng.random_range_float(0.5, 1.0);
                let life = e.lifetime * len_scl;
                e.scaled_view(life, |inner| {
                    rand_len_vectors_fin(
                        (inner.id.raw() as u64).wrapping_add(i).wrapping_sub(1),
                        inner.fin_with(Interp::Pow10Out),
                        15,
                        22.0 * 3.0,
                        |x, y, _fin, _fout| {
                            let f =
                                inner.fout_with(Interp::Pow5Out) * rng.random_range_float(0.5, 1.0);
                            let rad = f * ((2.0 + 3.0) * 1.35);
                            emit.circle(inner.x + x, inner.y + y, rad);
                            emit.light(inner.x + x, inner.y + y, rad * 2.5, inner.color, 0.5);
                        },
                    );
                });
            }
        }
        "disperseTrail" => {
            emit.color_lerp(Rgba::WHITE, e.color, e.fin());
            emit.stroke(0.6 + e.fout() * 1.7);
            let mut rng = seeded(e);
            for _ in 0..2 {
                let rot = e.rotation + rng.range_float(15.0) + 180.0;
                let len = rng.random_float(e.fin() * 27.0);
                emit.line_angle(
                    e.x + trnsx(rot, len),
                    e.y + trnsy(rot, len),
                    rot,
                    e.fout() * rng.random_range_float(2.0, 7.0) + 1.5,
                );
            }
        }
        "squareWaveEffect" => {
            let mut rng = seeded(e);
            emit.color_lerp(
                Rgba::WHITE,
                e.color,
                rng.random_range_float(0.8, 1.5) * e.fin(),
            );
            emit.stroke(rng.random_range_float(0.4, 0.8) + e.fout() * 2.0);
            let rot = rng.random_range_float(45.0, 180.0) * e.fin();
            let rotation = if rng.random_float(1.0) > 0.5 {
                rot
            } else {
                -rot
            };
            emit.square_line(
                e.x,
                e.y,
                e.fin() * rng.random_range_float(4.0, 11.0) + 4.0,
                e.rotation + rng.random_float(360.0) + rotation,
            );
            emit.light(e.x, e.y, 23.0, e.color, e.fout() * 0.7);
        }
        "hitLaserColor" => {
            emit.color_lerp(Rgba::WHITE, e.color, e.fin());
            emit.stroke(0.5 + e.fout());
            emit.circle_line(e.x, e.y, e.fin() * 5.0);
            emit.light(e.x, e.y, 23.0, e.color, e.fout() * 0.7);
        }
        "despawn" => sparks_cone(
            emit,
            e,
            7,
            e.fin() * 7.0,
            40.0,
            Rgba::from(pal::LIGHTER_ORANGE),
            GRAY,
            |fout| fout,
            |fout| e.fout() * 2.0 + 1.0 - fout + fout,
        ),
        "airBubble" => {
            rand_len_vectors(e.id.raw() as u64, 1, e.fin() * 12.0, |x, y| {
                emit.rect_native(RegionKey("bubble"), e.x + x, e.y + y, 0.0);
            });
        }
        "flakExplosion" => {
            e.scaled_view(6.0, |inner| {
                emit.stroke(3.0 * inner.fout());
                emit.circle_line(inner.x, inner.y, 3.0 + inner.fin() * 10.0);
            });
            cloud_simple(emit, e, 5, 2.0 + 23.0 * e.finpow(), e.fout() * 3.0 + 0.5);
            sparks(
                emit,
                e,
                4,
                1.0 + 23.0 * e.finpow(),
                Rgba::from(pal::LIGHTER_ORANGE),
                Rgba::from(pal::LIGHTER_ORANGE),
                |fout| fout,
                |fout| 1.0 + fout * 3.0,
            );
        }
        "plasticExplosion" => {
            e.scaled_view(7.0, |inner| {
                emit.stroke(3.0 * inner.fout());
                emit.circle_line(inner.x, inner.y, 3.0 + inner.fin() * 24.0);
            });
            cloud_simple(emit, e, 7, 2.0 + 28.0 * e.finpow(), 4.0 + 0.5);
            sparks_cone(
                emit,
                e,
                4,
                1.0 + 25.0 * e.finpow(),
                360.0,
                Rgba::from(pal::PLASTANIUM_BACK),
                Rgba::from(pal::PLASTANIUM_BACK),
                |fout| fout,
                |fout| 1.0 + fout * 3.0,
            );
            emit.light(e.x, e.y, 50.0, pal::PLASTANIUM_BACK, 0.8 * e.fout());
        }
        "plasticExplosionFlak" => {
            e.scaled_view(7.0, |inner| {
                emit.stroke(3.0 * inner.fout());
                emit.circle_line(inner.x, inner.y, 3.0 + inner.fin() * 34.0);
            });
            cloud_simple(emit, e, 7, 2.0 + 30.0 * e.finpow(), 4.5);
            sparks_cone(
                emit,
                e,
                4,
                1.0 + 30.0 * e.finpow(),
                360.0,
                Rgba::from(pal::PLASTANIUM_BACK),
                Rgba::from(pal::PLASTANIUM_BACK),
                |fout| fout,
                |fout| 1.0 + fout * 3.0,
            );
        }
        "blastExplosion" => {
            e.scaled_view(6.0, |inner| {
                emit.stroke(3.0 * inner.fout());
                emit.circle_line(inner.x, inner.y, 3.0 + inner.fin() * 15.0);
            });
            cloud_simple(emit, e, 5, 2.0 + 23.0 * e.finpow(), 4.5);
            sparks(
                emit,
                e,
                4,
                1.0 + 23.0 * e.finpow(),
                Rgba::from(pal::MISSILE_YELLOW_BACK),
                Rgba::from(pal::MISSILE_YELLOW_BACK),
                |fout| fout,
                |fout| 1.0 + fout * 3.0,
            );
            emit.light(e.x, e.y, 45.0, pal::MISSILE_YELLOW_BACK, 0.8 * e.fout());
        }
        "sapExplosion" => {
            e.scaled_view(6.0, |inner| {
                emit.stroke(3.0 * inner.fout());
                emit.circle_line(inner.x, inner.y, 3.0 + inner.fin() * 80.0);
            });
            cloud_simple(emit, e, 9, 2.0 + 70.0 * e.finpow(), 4.5);
            sparks(
                emit,
                e,
                8,
                1.0 + 60.0 * e.finpow(),
                Rgba::from(pal::SAP_BULLET_BACK),
                Rgba::from(pal::SAP_BULLET_BACK),
                |fout| fout,
                |fout| 1.0 + fout * 3.0,
            );
            emit.light(e.x, e.y, 90.0, pal::SAP_BULLET_BACK, 0.8 * e.fout());
        }
        "massiveExplosion" => {
            e.scaled_view(7.0, |inner| {
                emit.stroke(3.0 * inner.fout());
                emit.circle_line(inner.x, inner.y, 4.0 + inner.fin() * 30.0);
            });
            cloud_simple(emit, e, 8, 2.0 + 30.0 * e.finpow(), 4.5);
            sparks(
                emit,
                e,
                6,
                1.0 + 29.0 * e.finpow(),
                Rgba::from(pal::MISSILE_YELLOW_BACK),
                Rgba::from(pal::MISSILE_YELLOW_BACK),
                |fout| fout,
                |fout| 1.0 + fout * 4.0,
            );
            emit.light(e.x, e.y, 50.0, pal::MISSILE_YELLOW_BACK, 0.8 * e.fout());
        }
        "artilleryTrail" => {
            emit.color(e.color);
            emit.circle(e.x, e.y, e.rotation * e.fout());
        }
        "incendTrail" => {
            emit.color(Rgba::from(pal::LIGHT_ORANGE));
            emit.circle(e.x, e.y, e.rotation * e.fout());
        }
        "missileTrail" | "missileTrailShort" | "colorTrail" => {
            emit.color(e.color);
            emit.circle(e.x, e.y, e.rotation * e.fout());
        }
        "bulletSparkSmokeTrailSmall" => {
            emit.color(e.color);
            let mut rng = seeded(e);
            for _ in 0..3 {
                let len_r = rng.random_range_float(0.3, 0.8);
                let a = rng.random_float(360.0);
                let alpha = 0.4 - (e.fin() - 0.5).abs() * 1.5;
                emit.alpha(rng.random_range_float(alpha, alpha * 2.0));
                emit.circle(
                    e.x + trnsx(a, e.fin() * 10.0 * len_r),
                    e.y + trnsy(a, e.fin() * 10.0 * len_r),
                    0.4 + e.fout() * 3.5,
                );
            }
            let n = rng.random_range_int(0, 2);
            for _ in 0..n {
                let len_r = rng.random_range_float(0.5, 1.2);
                let a = rng.random_float(360.0);
                emit.color_lerp(Rgba::from(pal::SURGE), Rgba::WHITE, e.fin());
                emit.alpha(e.fout() * 0.9);
                emit.stroke(1.5 * e.fout());
                emit.line_angle(
                    e.x + trnsx(a, e.fin() * 10.0 * len_r),
                    e.y + trnsy(a, e.fin() * 10.0 * len_r),
                    a,
                    2.5 + 3.0 * e.fout(),
                );
            }
        }
        "absorb" => {
            emit.color(Rgba::from(pal::ACCENT));
            emit.stroke(2.0 * e.fout());
            emit.circle_line(e.x, e.y, 5.0 * e.fout());
        }
        "forceShrink" => {
            emit.color(e.color);
            emit.alpha(e.fout());
            emit.poly(e.x, e.y, 6, e.rotation * e.fout(), 0.0);
        }
        "flakExplosionBig" => {
            e.scaled_view(6.0, |inner| {
                emit.stroke(3.0 * inner.fout());
                emit.circle_line(inner.x, inner.y, 3.0 + inner.fin() * 25.0);
            });
            cloud_simple(emit, e, 6, 2.0 + 23.0 * e.finpow(), 4.5);
            sparks(
                emit,
                e,
                4,
                1.0 + 23.0 * e.finpow(),
                Rgba::from(pal::BULLET_YELLOW),
                Rgba::from(pal::BULLET_YELLOW),
                |fout| fout,
                |fout| 1.0 + fout * 3.0,
            );
            emit.light(e.x, e.y, 60.0, pal::BULLET_YELLOW_BACK, 0.7 * e.fout());
        }
        "burning" => cloud(
            emit,
            e,
            3,
            2.0 + e.fin() * 7.0,
            Rgba::from(pal::LIGHT_FLAME),
            Rgba::from(pal::DARK_FLAME),
            |fout| 0.1 + fout * 1.4,
        ),
        "fireRemove" => {
            emit.alpha(e.fout());
            emit.rect_native(content_region(), e.x, e.y, 0.0);
            emit.light(
                e.x,
                e.y,
                50.0 + (5.0_f32).abs() * 0.0 + 5.0,
                pal::LIGHT_FLAME,
                0.6 * e.fout(),
            );
        }
        "fire" => {
            cloud(
                emit,
                e,
                2,
                2.0 + e.fin() * 9.0,
                Rgba::from(pal::LIGHT_FLAME),
                Rgba::from(pal::DARK_FLAME),
                |_| 0.2 + e.fslope() * 1.5,
            );
            emit.color(Rgba::WHITE);
            emit.light(e.x, e.y, 20.0 * e.fslope(), pal::LIGHT_FLAME, 0.5);
        }
        "fireHit" => cloud(
            emit,
            e,
            3,
            2.0 + e.fin() * 10.0,
            Rgba::from(pal::LIGHT_FLAME),
            Rgba::from(pal::DARK_FLAME),
            |fout| 0.2 + fout * 1.6,
        ),
        "fireSmoke" => cloud(emit, e, 1, 2.0 + e.fin() * 7.0, GRAY, GRAY, |_| {
            0.2 + e.fslope() * 1.5
        }),
        "neoplasmHeal" => cloud(
            emit,
            e,
            1,
            e.fin() * 3.0,
            Rgba::from(pal::NEOPLASM1),
            Rgba::from(pal::NEOPLASM2),
            |_| 0.2 + e.fslope() * 2.0,
        ),
        "steam" => cloud(
            emit,
            e,
            2,
            2.0 + e.fin() * 7.0,
            LIGHT_GRAY,
            LIGHT_GRAY,
            |_| 0.2 + e.fslope() * 1.5,
        ),
        "ventSteam" => {
            emit.color_lerp(e.color, Rgba::from(pal::VENT2), e.fin());
            emit.alpha(e.fslope() * 0.78);
            let length = 3.0 + e.finpow() * 10.0;
            let mut rng = seeded(e);
            for _ in 0..rng.random_range_int(3, 5) {
                let a = rng.random_float(360.0);
                let d = rng.random_float(length);
                emit.circle(
                    e.x + trnsx(a, d),
                    e.y + trnsy(a, d),
                    rng.random_range_float(1.2, 3.5) + e.fslope() * 1.1,
                );
            }
        }
        "drillSteam" => {
            let length = 3.0 + e.finpow() * 20.0;
            let mut rng = seeded(e);
            for _ in 0..13 {
                let a = rng.random_float(360.0);
                let d = rng.random_float(length);
                let sizer = rng.random_range_float(1.3, 3.7);
                e.scaled_view(e.lifetime * rng.random_range_float(0.5, 1.0), |inner| {
                    emit.color(GRAY);
                    emit.alpha(inner.fslope() * 0.93);
                    emit.circle(
                        inner.x + trnsx(a, d),
                        inner.y + trnsy(a, d),
                        sizer + inner.fslope() * 1.2,
                    );
                });
            }
        }
        "fluxVapor" => {
            emit.color(e.color);
            emit.alpha(e.fout() * 0.7);
            rand_len_vectors(e.id.raw() as u64, 2, 3.0 + e.finpow() * 10.0, |x, y| {
                emit.circle(e.x + x, e.y + y, 0.6 + e.fin() * 5.0);
            });
        }
        "corrosionVapor" => {
            emit.color(e.color);
            emit.alpha(Interp::Pow2Out.apply(e.fslope()) * 0.5);
            rand_len_vectors(e.id.raw() as u64, 2, 8.0 + e.finpow() * 3.0, |x, y| {
                emit.circle(e.x + x, e.y + y, 3.0);
            });
        }
        "vapor" => {
            emit.color(e.color);
            emit.alpha(e.fout());
            rand_len_vectors(e.id.raw() as u64, 3, 2.0 + e.finpow() * 11.0, |x, y| {
                emit.circle(e.x + x, e.y + y, 0.6 + e.fin() * 5.0);
            });
        }
        "vaporSmall" => {
            emit.color(e.color);
            emit.alpha(e.fout());
            rand_len_vectors(e.id.raw() as u64, 4, 2.0 + e.finpow() * 5.0, |x, y| {
                emit.circle(e.x + x, e.y + y, 1.0 + e.fin() * 4.0);
            });
        }
        "fireballsmoke" => cloud(emit, e, 1, 2.0 + e.fin() * 7.0, GRAY, GRAY, |fout| {
            0.2 + fout * 1.5
        }),
        "ballfire" => cloud(
            emit,
            e,
            2,
            2.0 + e.fin() * 7.0,
            Rgba::from(pal::LIGHT_FLAME),
            Rgba::from(pal::DARK_FLAME),
            |fout| 0.2 + fout * 1.5,
        ),
        "freezing" => cloud(emit, e, 2, 1.0 + e.fin() * 2.0, CRYO, CRYO, |fout| {
            fout * 1.2
        }),
        "melting" => {
            emit.color_lerp(
                SLAG,
                Rgba::WHITE,
                e.fout() / 5.0 + random_seed_range(e.id.raw() as i64, 0.12),
            );
            rand_len_vectors(e.id.raw() as u64, 2, 1.0 + e.fin() * 3.0, |x, y| {
                emit.circle(e.x + x, e.y + y, 0.2 + e.fout() * 1.2);
            });
        }
        "wet" => {
            emit.color(WATER);
            emit.alpha((e.fin() * 2.0).clamp(0.0, 1.0));
            emit.circle(e.x, e.y, e.fout());
        }
        "muddy" => {
            emit.color(Rgba::from(pal::MUDDY));
            emit.alpha((e.fin() * 2.0).clamp(0.0, 1.0));
            emit.circle(e.x, e.y, e.fout());
        }
        "sapped" => cloud_squares(
            emit,
            e,
            2,
            1.0 + e.fin() * 2.0,
            Rgba::from(pal::SAP),
            Rgba::from(pal::SAP),
            |_| e.fslope() * 1.1,
        ),
        "electrified" => cloud_squares(
            emit,
            e,
            2,
            1.0 + e.fin() * 2.0,
            Rgba::from(pal::HEAL),
            Rgba::from(pal::HEAL),
            |_| e.fslope() * 1.1,
        ),
        "sporeSlowed" => {
            emit.color(Rgba::from(pal::SPORE));
            emit.circle(e.x, e.y, e.fslope() * 1.1);
        }
        "oily" => cloud(emit, e, 2, 1.0 + e.fin() * 2.0, OIL, OIL, |fout| fout),
        "overdriven" => cloud_squares(emit, e, 2, 1.0 + e.fin() * 2.0, e.color, e.color, |fout| {
            fout * 2.3 + 0.5
        }),
        "overclocked" => {
            emit.color(e.color);
            emit.square(e.x, e.y, e.fslope() * 2.0, 45.0);
        }
        "dropItem" => {
            if !matches!(e.data, EffectData::Item(_)) {
                return;
            }
            let length = 20.0 * e.finpow();
            let size = 7.0 * e.fout();
            emit.rect_native(
                content_region(),
                e.x + trnsx(e.rotation, length),
                e.y + trnsy(e.rotation, length),
                0.0,
            );
            let _ = size;
        }
        "shockwaveSmaller" => {
            emit.color_lerp(Rgba::WHITE, LIGHT_GRAY, e.fin());
            emit.stroke(e.fout() * 2.0 + 0.2);
            emit.circle_line(e.x, e.y, e.fin() * 22.0);
        }
        "bigShockwave" => {
            emit.color_lerp(Rgba::WHITE, LIGHT_GRAY, e.fin());
            emit.stroke(e.fout() * 3.0);
            emit.circle_line(e.x, e.y, e.fin() * 50.0);
        }
        "spawnShockwave" => {
            emit.color_lerp(Rgba::WHITE, LIGHT_GRAY, e.fin());
            emit.stroke(e.fout() * 3.0 + 0.5);
            emit.circle_line(e.x, e.y, e.fin() * (e.rotation + 50.0));
        }
        "podLandShockwave" => {
            emit.color(Rgba::from(pal::ACCENT));
            emit.stroke(e.fout() * 2.0 + 0.2);
            emit.circle_line(e.x, e.y, e.fin() * 26.0);
        }
        "dynamicExplosion" => bomb(
            emit,
            e,
            e.rotation,
            Rgba::GRAY,
            Rgba::from(pal::LIGHTER_ORANGE),
            Rgba::from(pal::LIGHT_ORANGE),
            Rgba::WHITE,
        ),
        "reactorExplosion" => bomb(
            emit,
            e,
            6.8,
            Rgba::from(pal::REACTOR_PURPLE2),
            Rgba::from(pal::LIGHTER_ORANGE),
            Rgba::from(pal::REACTOR_PURPLE),
            Rgba::WHITE,
        ),
        "impactReactorExplosion" => bomb(
            emit,
            e,
            8.0,
            Rgba::from(pal::LIGHTER_ORANGE),
            Rgba::WHITE,
            Rgba::from(pal::LIGHTER_ORANGE),
            Rgba::WHITE,
        ),
        "blockExplosionSmoke" => {
            emit.color(GRAY);
            rand_len_vectors(e.id.raw() as u64, 6, 4.0 + 30.0 * e.finpow(), |x, y| {
                emit.circle(e.x + x, e.y + y, e.fout() * 3.0);
                emit.circle(e.x + x / 2.0, e.y + y / 2.0, e.fout());
            });
        }
        "steamCoolSmoke" => {
            emit.color_lerp(WATER, LIGHT_GRAY, e.fin_with(Interp::Pow2Out));
            emit.alpha(e.fout_with(Interp::Pow3Out));
            rand_len_vectors_cone(
                e.id.raw() as u64,
                4,
                e.finpow() * 7.0,
                e.rotation,
                30.0,
                |x, y| {
                    emit.circle(
                        e.x + x,
                        e.y + y,
                        e.fout().max((e.fin() * 8.0).min(1.0)) * 2.8,
                    );
                },
            );
        }
        "smokePuff" => {
            emit.color(e.color);
            rand_len_vectors(e.id.raw() as u64, 6, 4.0 + 30.0 * e.finpow(), |x, y| {
                emit.circle(e.x + x, e.y + y, e.fout() * 3.0);
                emit.circle(e.x + x / 2.0, e.y + y / 2.0, e.fout());
            });
        }
        "shootSmallColor" => {
            emit.color_lerp(e.color, GRAY, e.fin());
            let w = 1.0 + 5.0 * e.fout();
            emit.tri(e.x, e.y, w, 15.0 * e.fout(), e.rotation);
            emit.tri(e.x, e.y, w, 3.0 * e.fout(), e.rotation + 180.0);
        }
        "shootHeal" => {
            emit.color(Rgba::from(pal::HEAL));
            let w = 1.0 + 5.0 * e.fout();
            emit.tri(e.x, e.y, w, 17.0 * e.fout(), e.rotation);
            emit.tri(e.x, e.y, w, 4.0 * e.fout(), e.rotation + 180.0);
        }
        "shootHealYellow" => {
            emit.color(Rgba::from(pal::LIGHT_TRAIL));
            let w = 1.0 + 5.0 * e.fout();
            emit.tri(e.x, e.y, w, 17.0 * e.fout(), e.rotation);
            emit.tri(e.x, e.y, w, 4.0 * e.fout(), e.rotation + 180.0);
        }
        "shootSmallSmoke" => cloud_cone(
            emit,
            e,
            5,
            e.finpow() * 6.0,
            20.0,
            Rgba::from(pal::LIGHTER_ORANGE),
            GRAY,
            |fout| e.fout() * 1.5 + fout * 0.0,
        ),
        "shootBig2" => {
            emit.color_lerp(Rgba::from(pal::LIGHT_ORANGE), GRAY, e.fin());
            let w = 1.2 + 8.0 * e.fout();
            emit.tri(e.x, e.y, w, 29.0 * e.fout(), e.rotation);
            emit.tri(e.x, e.y, w, 5.0 * e.fout(), e.rotation + 180.0);
        }
        "shootBigColor" => {
            emit.color_lerp(e.color, GRAY, e.fin());
            let w = 1.2 + 9.0 * e.fout();
            emit.tri(e.x, e.y, w, 32.0 * e.fout(), e.rotation);
            emit.tri(e.x, e.y, w, 3.0 * e.fout(), e.rotation + 180.0);
        }
        "shootScepterSecondary" => {
            let w = 1.2 + 7.0 * e.fout();
            for i in [-1.0_f32, 1.0] {
                emit.color_lerp(
                    Rgba::from(pal::BULLET_YELLOW),
                    Rgba::from(pal::BULLET_YELLOW_BACK),
                    e.fout() * 1.5,
                );
                emit.tri(e.x, e.y, w, 10.0 + e.fout() * 2.0, e.rotation + i * 90.0);
            }
            emit.color_lerp(
                Rgba::from(pal::BULLET_YELLOW),
                Rgba::from(pal::BULLET_YELLOW_BACK),
                e.fout() * 0.5,
            );
            emit.tri(e.x, e.y, w, 15.0 * e.fout(), e.rotation);
            emit.tri(e.x, e.y, w, 3.0 * e.fout(), e.rotation + 180.0);
        }
        "shootQuellPulse" => {
            let mut rng = seeded(e);
            let fout = e.fout() * rng.random_range_float(0.9, 1.0);
            let core_radius = 30.0 * Interp::Smooth2.apply(e.fout());
            e.scaled_view(10.0, |inner| {
                emit.stroke(4.0 * inner.fout());
                emit.circle_line(inner.x, inner.y, 2.0 + inner.fin() * 40.0);
            });
            let count = 8;
            for i in 0..count {
                let t = (i as f32 + 1.0) / count as f32;
                emit.color(Rgba::from(pal::ACCENT));
                emit.alpha((1.0 - t).powf(2.5) * fout * 0.5);
                emit.circle(e.x, e.y, lerp(core_radius * 0.6, core_radius * 1.7, t));
            }
            emit.color(Rgba::from(pal::ACCENT));
            emit.stroke(3.0 * e.fout());
            let circle_rad = e.finpow() * 28.0;
            emit.circle_line(e.x, e.y, circle_rad);
            for _ in 0..9 {
                let a = rng.random_float(360.0);
                let len_rand = rng.random_range_float(0.5, 1.2);
                let x = trnsx(a, circle_rad);
                let y = trnsy(a, circle_rad);
                for s in [-1.0_f32, 1.0] {
                    emit.tri(
                        e.x + x,
                        e.y + y,
                        e.fout() * 10.0,
                        e.fout() * 10.0 * len_rand + 8.0,
                        a + 90.0 + s * 90.0,
                    );
                }
            }
            emit.reset();
        }
        "shootTitan" => {
            emit.color_lerp(Rgba::from(pal::LIGHT_ORANGE), e.color, e.fin());
            let w = 1.3 + 10.0 * e.fout();
            emit.tri(e.x, e.y, w, 35.0 * e.fout(), e.rotation);
            emit.tri(e.x, e.y, w, 6.0 * e.fout(), e.rotation + 180.0);
        }
        "shootBigSmoke" => cloud_cone(
            emit,
            e,
            8,
            e.finpow() * 19.0,
            10.0,
            Rgba::from(pal::LIGHTER_ORANGE),
            GRAY,
            |fout| fout * 2.0 + 0.2,
        ),
        "shootBigSmoke2" => cloud_cone(
            emit,
            e,
            9,
            e.finpow() * 23.0,
            20.0,
            Rgba::from(pal::LIGHT_ORANGE),
            GRAY,
            |fout| fout * 2.4 + 0.2,
        ),
        "shootSmokeDisperse" => cloud_cone(
            emit,
            e,
            9,
            e.finpow() * 29.0,
            18.0,
            Rgba::from(pal::LIGHT_ORANGE),
            GRAY,
            |fout| fout * 2.2 + 0.1,
        ),
        "shootSmokeSquare" => {
            emit.color_lerp(Rgba::WHITE, e.color, e.fin());
            let mut rng = seeded(e);
            for _ in 0..6 {
                let rot = e.rotation + rng.range_float(22.0);
                let d = rng.random_float(e.finpow() * 21.0);
                emit.poly(
                    e.x + trnsx(rot, d),
                    e.y + trnsy(rot, d),
                    4,
                    e.fout() * 2.0 + 0.2,
                    rng.random_float(360.0),
                );
            }
        }
        "shootSmokeSquareSparse" => {
            emit.color_lerp(Rgba::WHITE, e.color, e.fin());
            let mut rng = seeded(e);
            for _ in 0..2 {
                let rot = e.rotation + rng.range_float(30.0);
                let d = rng.random_float(e.finpow() * 27.0);
                emit.poly(
                    e.x + trnsx(rot, d),
                    e.y + trnsy(rot, d),
                    4,
                    e.fout() * 3.8 + 0.2,
                    rng.random_float(360.0),
                );
            }
        }
        "shootSmokeSquareBig" => {
            emit.color_lerp(Rgba::WHITE, e.color, e.fin());
            let mut rng = seeded(e);
            for _ in 0..13 {
                let rot = e.rotation + rng.range_float(26.0);
                let d = rng.random_float(e.finpow() * 30.0);
                emit.poly(
                    e.x + trnsx(rot, d),
                    e.y + trnsy(rot, d),
                    4,
                    e.fout() * 4.0 + 0.2,
                    rng.random_float(360.0),
                );
            }
        }
        "shootSmokeTitan" => {
            let mut rng = seeded(e);
            for _ in 0..13 {
                let rot = e.rotation + rng.range_float(30.0);
                let d = rng.random_float(e.finpow() * 40.0);
                e.scaled_view(e.lifetime * rng.random_range_float(0.3, 1.0), |inner| {
                    emit.color_lerp(inner.color, Rgba::from(pal::LIGHTISH_GRAY), inner.fin());
                    emit.circle(
                        inner.x + trnsx(rot, d),
                        inner.y + trnsy(rot, d),
                        inner.fout() * 3.4 + 0.3,
                    );
                });
            }
        }
        "shootSmokeSmite" => {
            let mut rng = seeded(e);
            for _ in 0..13 {
                let a = e.rotation + rng.range_float(30.0);
                let d = rng.random_float(e.finpow() * 50.0);
                e.scaled_view(e.lifetime * rng.random_range_float(0.3, 1.0), |inner| {
                    emit.color(inner.color);
                    emit.stroke(inner.fout() * 3.0 + 0.5);
                    emit.line_angle(
                        inner.x + trnsx(a, d),
                        inner.y + trnsy(a, d),
                        a,
                        inner.fout() * 8.0 + 0.4,
                    );
                });
            }
        }
        "shootSmokeMissile" => {
            emit.color(Rgba::from(pal::RED_LIGHT));
            emit.alpha(0.5);
            let mut rng = seeded(e);
            for _ in 0..35 {
                let a = e.rotation + 180.0 + rng.range_float(21.0);
                let d = rng.random_float(e.finpow() * 90.0);
                let ox = rng.range_float(3.0);
                let oy = rng.range_float(3.0);
                e.scaled_view(e.lifetime * rng.random_range_float(0.2, 1.0), |inner| {
                    emit.circle(
                        inner.x + trnsx(a, d) + ox,
                        inner.y + trnsy(a, d) + oy,
                        inner.fout() * 9.0 + 0.3,
                    );
                });
            }
        }
        "shootSmokeMissileColor" => {
            emit.color(e.color);
            emit.alpha(0.5);
            let mut rng = seeded(e);
            for _ in 0..35 {
                let a = e.rotation + 180.0 + rng.range_float(21.0);
                let d = rng.random_float(e.finpow() * 90.0);
                let ox = rng.range_float(3.0);
                let oy = rng.range_float(3.0);
                e.scaled_view(e.lifetime * rng.random_range_float(0.2, 1.0), |inner| {
                    emit.circle(
                        inner.x + trnsx(a, d) + ox,
                        inner.y + trnsy(a, d) + oy,
                        inner.fout() * 9.0 + 0.3,
                    );
                });
            }
        }
        "regenParticle" => {
            emit.color(Rgba::from(pal::REGEN));
            emit.square(e.x, e.y, e.fslope() * 1.5 + 0.14, 45.0);
        }
        "regenSuppressParticle" => {
            emit.color_lerp(e.color, Rgba::WHITE, e.fin());
            emit.stroke(e.fout() * 1.4 + 0.5);
            rand_len_vectors(e.id.raw() as u64, 4, 17.0 * e.fin(), |x, y| {
                emit.line_angle(e.x + x, e.y + y, angle(x, y), e.fslope() * 3.0 + 0.5);
            });
        }
        "regenSuppressSeek" => {
            if let EffectData::Position { x: tx, y: ty } = e.data {
                let dx = tx - e.x;
                let dy = ty - e.y;
                let len = (dx * dx + dy * dy).sqrt().max(1e-6);
                let nx = -dy / len;
                let ny = dx / len;
                let off = random_seed_range(e.id.raw() as i64, 1.0) * 50.0;
                let f = e.fout();
                let x = lerp(e.x, tx, f);
                let y = lerp(e.y, ty, f);
                // Quadratic control point at (x + offset) approximates Tmp.bz2.
                let mx = lerp(lerp(e.x, tx, 0.5), x + nx * off, 0.5);
                let my = lerp(lerp(e.y, ty, 0.5), y + ny * off, 0.5);
                emit.color(e.color);
                emit.circle(mx, my, e.fslope() * 2.0 + 0.1);
            }
        }
        "surgeCruciSmoke" => {
            emit.color(Rgba::from(pal::SLAG_ORANGE));
            emit.alpha(0.6);
            let mut rng = seeded(e);
            for _ in 0..3 {
                let len = rng.random_float(6.0);
                let rot = rng.range_float(40.0) + e.rotation;
                e.scaled_view(e.lifetime * rng.random_range_float(0.3, 1.0), |inner| {
                    emit.circle(
                        inner.x + trnsx(rot, len * inner.finpow()),
                        inner.y + trnsy(rot, len * inner.finpow()),
                        2.0 * inner.fslope() + 0.2,
                    );
                });
            }
        }
        "neoplasiaSmoke" => {
            emit.color(Rgba::from(pal::NEOPLASM_MID));
            emit.alpha(0.6);
            let mut rng = seeded(e);
            for _ in 0..6 {
                let len = rng.random_float(10.0);
                let rot = rng.range_float(120.0) + e.rotation;
                e.scaled_view(e.lifetime * rng.random_range_float(0.3, 1.0), |inner| {
                    emit.circle(
                        inner.x + trnsx(rot, len * inner.finpow()),
                        inner.y + trnsy(rot, len * inner.finpow()),
                        3.3 * inner.fslope() + 0.2,
                    );
                });
            }
        }
        "heatReactorSmoke" => {
            emit.color(GRAY);
            let mut rng = seeded(e);
            for _ in 0..5 {
                let len = rng.random_float(6.0);
                let rot = rng.range_float(50.0) + e.rotation;
                e.scaled_view(e.lifetime * rng.random_range_float(0.3, 1.0), |inner| {
                    emit.alpha(0.9 * inner.fout());
                    emit.circle(
                        inner.x + trnsx(rot, len * inner.finpow()),
                        inner.y + trnsy(rot, len * inner.finpow()),
                        2.4 * inner.fin() + 0.6,
                    );
                });
            }
        }
        "circleColorSpark" => sparks(
            emit,
            e,
            9,
            27.0 * e.fin(),
            Rgba::WHITE,
            e.color,
            |fout| fout * 1.1 + 0.5,
            |_| e.fslope() * 5.0 + 0.5,
        ),
        "colorSpark" => sparks_cone(
            emit,
            e,
            5,
            27.0 * e.fin(),
            9.0,
            Rgba::WHITE,
            e.color,
            |fout| fout * 1.1 + 0.5,
            |_| e.fslope() * 5.0 + 0.5,
        ),
        "colorSparkBig" => sparks_cone(
            emit,
            e,
            8,
            41.0 * e.fin(),
            10.0,
            Rgba::WHITE,
            e.color,
            |fout| fout * 1.3 + 0.7,
            |_| e.fslope() * 6.0 + 0.5,
        ),
        "randLifeSpark" => {
            emit.color_lerp(Rgba::WHITE, e.color, e.fin());
            emit.stroke(e.fout() * 1.5 + 0.5);
            let mut rng = seeded(e);
            for _ in 0..15 {
                let a = e.rotation + rng.range_float(9.0);
                let len = rng.random_float(90.0 * e.finpow());
                e.scaled_view(e.lifetime * rng.random_range_float(0.5, 1.0), |inner| {
                    emit.line_angle(
                        inner.x + trnsx(a, len),
                        inner.y + trnsy(a, len),
                        a,
                        inner.fout() * 7.0 + 0.5,
                    );
                });
            }
        }
        "shootPayloadDriver" => {
            emit.color(Rgba::from(pal::ACCENT));
            emit.stroke(0.5 + 0.5 * e.fout());
            let spread = 9.0;
            let mut rng = seeded(e);
            for _ in 0..20 {
                let a = e.rotation + rng.range_float(17.0);
                let d = rng.random_float(e.fin() * 55.0);
                emit.line_angle(
                    e.x + trnsx(a, d) + rng.range_float(spread),
                    e.y + trnsy(a, d) + rng.range_float(spread),
                    a,
                    e.fout() * 5.0 * rng.random_float(1.0) + 1.0,
                );
            }
        }
        "shootSmallFlame" => cloud_cone(
            emit,
            e,
            12,
            e.finpow() * 60.0,
            10.0,
            Rgba::from(pal::LIGHT_FLAME),
            GRAY,
            |fout| 0.65 + fout * 1.5,
        ),
        "shootPyraFlame" => cloud_cone(
            emit,
            e,
            13,
            e.finpow() * 70.0,
            10.0,
            Rgba::from(pal::LIGHT_PYRA_FLAME),
            GRAY,
            |fout| 0.65 + fout * 1.6,
        ),
        "shootLiquid" => cloud_cone(
            emit,
            e,
            2,
            e.finpow() * 15.0,
            11.0,
            e.color,
            e.color,
            |fout| 0.5 + fout * 2.5,
        ),
        "casing2" => casing(
            emit,
            e,
            2.0,
            ColorPair::new(
                Rgba::from(pal::LIGHT_ORANGE),
                LIGHT_GRAY,
                Rgba::from(pal::LIGHTISH_GRAY),
            ),
            2.0,
            3.0,
            0.5,
            3,
            true,
        ),
        "casing3" => casing(
            emit,
            e,
            4.0,
            ColorPair::new(
                Rgba::from(pal::LIGHT_ORANGE),
                Rgba::from(pal::LIGHTISH_GRAY),
                Rgba::from(pal::LIGHTISH_GRAY),
            ),
            2.5,
            4.0,
            0.5,
            3,
            true,
        ),
        "casing4" => casing(
            emit,
            e,
            4.0,
            ColorPair::new(
                Rgba::from(pal::LIGHT_ORANGE),
                Rgba::from(pal::LIGHTISH_GRAY),
                Rgba::from(pal::LIGHTISH_GRAY),
            ),
            3.0,
            6.0,
            0.5,
            3,
            true,
        ),
        "casing2Double" => casing_double(
            emit,
            e,
            2.0,
            Rgba::from(pal::LIGHT_ORANGE),
            LIGHT_GRAY,
            2.0,
            3.0,
            0.5,
            3,
        ),
        "casing3Double" => casing_double(
            emit,
            e,
            4.0,
            Rgba::from(pal::LIGHT_ORANGE),
            Rgba::from(pal::LIGHTISH_GRAY),
            2.5,
            4.0,
            0.5,
            3,
        ),
        "railShoot" => {
            e.scaled_view(10.0, |inner| {
                emit.color_lerp(Rgba::WHITE, LIGHT_GRAY, inner.fin());
                emit.stroke(inner.fout() * 3.0 + 0.2);
                emit.circle_line(inner.x, inner.y, inner.fin() * 50.0);
            });
            emit.color(Rgba::from(pal::ORANGE_SPARK));
            for i in [-1.0_f32, 1.0] {
                emit.tri(e.x, e.y, 13.0 * e.fout(), 85.0, e.rotation + 90.0 * i);
            }
        }
        "railTrail" => {
            emit.color(Rgba::from(pal::ORANGE_SPARK));
            for i in [-1.0_f32, 1.0] {
                emit.tri(
                    e.x,
                    e.y,
                    10.0 * e.fout(),
                    24.0,
                    e.rotation + 90.0 + 90.0 * i,
                );
            }
            emit.light(e.x, e.y, 60.0 * e.fout(), pal::ORANGE_SPARK, 0.5);
        }
        "railHit" => {
            emit.color(Rgba::from(pal::ORANGE_SPARK));
            for i in [-1.0_f32, 1.0] {
                emit.tri(e.x, e.y, 10.0 * e.fout(), 60.0, e.rotation + 140.0 * i);
            }
        }
        "lancerLaserShoot" => {
            emit.color(Rgba::from(pal::LANCER_LASER));
            for i in [-1.0_f32, 1.0] {
                emit.tri(e.x, e.y, 4.0 * e.fout(), 29.0, e.rotation + 90.0 * i);
            }
        }
        "lancerLaserShootSmoke" => {
            emit.color(Rgba::WHITE);
            let length = match e.data {
                EffectData::Float(f) => f,
                _ => 70.0,
            };
            emit.stroke(e.fout() * 9.0);
            rand_len_vectors_cone(e.id.raw() as u64, 7, length, e.rotation, 0.0, |x, y| {
                emit.line_angle(e.x + x, e.y + y, angle(x, y), e.fout() * 9.0);
            });
        }
        "lancerLaserCharge" => {
            emit.color(Rgba::from(pal::LANCER_LASER));
            emit.stroke(1.0);
            rand_len_vectors_cone(
                e.id.raw() as u64,
                14,
                1.0 + 20.0 * e.fout(),
                e.rotation,
                120.0,
                |x, y| {
                    emit.line_angle(e.x + x, e.y + y, angle(x, y), e.fslope() * 3.0 + 1.0);
                },
            );
        }
        "lancerLaserChargeBegin" => {
            let margin = 1.0 - crate::math::curve(e.fin(), 0.9, 1.0);
            let fin = margin.min(e.fin());
            emit.color(Rgba::from(pal::LANCER_LASER));
            emit.circle(e.x, e.y, fin * 3.0);
            emit.color(Rgba::WHITE);
            emit.circle(e.x, e.y, fin * 2.0);
        }
        "lightningCharge" => {
            emit.color(Rgba::from(pal::LANCER_LASER));
            rand_len_vectors_cone(
                e.id.raw() as u64,
                2,
                1.0 + 20.0 * e.fout(),
                e.rotation,
                120.0,
                |x, y| {
                    emit.tri(
                        e.x + x,
                        e.y + y,
                        e.fslope() * 3.0 + 1.0,
                        e.fslope() * 3.0 + 1.0,
                        angle(x, y),
                    );
                },
            );
        }
        "sparkShoot" => sparks_cone(
            emit,
            e,
            7,
            25.0 * e.finpow(),
            3.0,
            Rgba::WHITE,
            e.color,
            |fout| fout * 1.2 + 0.6,
            |_| e.fslope() * 5.0 + 0.5,
        ),
        "lightningShoot" => sparks_cone(
            emit,
            e,
            7,
            25.0 * e.finpow(),
            50.0,
            Rgba::WHITE,
            Rgba::from(pal::LANCER_LASER),
            |fout| fout * 1.2 + 0.5,
            |_| e.fin() * 5.0 + 2.0,
        ),
        "thoriumShoot" => sparks_cone(
            emit,
            e,
            7,
            25.0 * e.finpow(),
            50.0,
            Rgba::WHITE,
            Rgba::from(pal::THORIUM_PINK),
            |fout| fout * 1.2 + 0.5,
            |_| e.fin() * 5.0 + 2.0,
        ),
        "reactorsmoke" => {
            rand_len_vectors(e.id.raw() as u64, 4, e.fin() * 8.0, |x, y| {
                emit.color_lerp(LIGHT_GRAY, GRAY, e.fin());
                emit.circle(e.x + x, e.y + y, (1.0 + e.fout() * 5.0) / 2.0);
            });
        }
        "redgeneratespark" => {
            emit.color(Rgba::from(pal::RED_SPARK));
            emit.alpha(e.fslope());
            let mut rng = seeded(e);
            for _ in 0..2 {
                let a = rng.random_float(360.0);
                let d = rng.random_float(e.finpow() * 9.0);
                emit.circle(
                    e.x + trnsx(a, d),
                    e.y + trnsy(a, d),
                    rng.random_range_float(1.4, 2.4),
                );
            }
        }
        "turbinegenerate" => {
            emit.color(Rgba::from(pal::VENT));
            emit.alpha(e.fslope() * 0.8);
            let mut rng = seeded(e);
            for _ in 0..3 {
                let a = rng.random_float(360.0);
                let d = rng.random_float(e.finpow() * 14.0);
                emit.circle(
                    e.x + trnsx(a, d),
                    e.y + trnsy(a, d),
                    rng.random_range_float(1.4, 3.4),
                );
            }
        }
        "generatespark" => {
            rand_len_vectors(e.id.raw() as u64, 5, e.fin() * 8.0, |x, y| {
                emit.color_lerp(Rgba::from(pal::ORANGE_SPARK), GRAY, e.fin());
                emit.circle(e.x + x, e.y + y, e.fout() * 4.0 / 2.0);
            });
        }
        "fuelburn" => {
            rand_len_vectors(e.id.raw() as u64, 5, e.fin() * 9.0, |x, y| {
                emit.color_lerp(LIGHT_GRAY, GRAY, e.fin());
                emit.circle(e.x + x, e.y + y, e.fout() * 2.0);
            });
        }
        "incinerateSlag" => {
            rand_len_vectors(e.id.raw() as u64, 4, e.finpow() * 5.0, |x, y| {
                emit.color_lerp(Rgba::from(pal::SLAG_ORANGE), GRAY, e.fin());
                emit.circle(e.x + x, e.y + y, e.fout() * 1.7);
            });
        }
        "coreBurn" => {
            rand_len_vectors(e.id.raw() as u64, 5, e.fin() * 9.0, |x, y| {
                let len = e.fout() * 4.0;
                emit.color_lerp(Rgba::from(pal::ACCENT), GRAY, e.fin());
                emit.circle(e.x + x, e.y + y, len / 2.0);
            });
        }
        "plasticburn" => {
            rand_len_vectors(e.id.raw() as u64, 5, 3.0 + e.fin() * 5.0, |x, y| {
                emit.color_lerp(Rgba::from(pal::PLASTIC_BURN), GRAY, e.fin());
                emit.circle(e.x + x, e.y + y, e.fout());
            });
        }
        "conveyorPoof" => {
            emit.color_lerp(Rgba::from(pal::PLASTIC_BURN), GRAY, e.fin());
            rand_len_vectors(e.id.raw() as u64, 4, 3.0 + e.fin() * 4.0, |x, y| {
                emit.circle(e.x + x, e.y + y, e.fout() * 1.11);
            });
        }
        "pulverize" | "pulverizeSmall" | "pulverizeMedium" | "pulverizeRed" => {
            let (amount, length) = if name == "pulverizeSmall" {
                (3, e.fin() * 5.0)
            } else {
                (5, 3.0 + e.fin() * 8.0)
            };
            emit.color_lerp(
                Rgba::from(pal::RED_DUST),
                Rgba::from(pal::STONE_GRAY),
                e.fin(),
            );
            rand_len_vectors(e.id.raw() as u64, amount, length, |x, y| {
                emit.square(e.x + x, e.y + y, e.fout() * 2.0 + 0.5, 45.0);
            });
        }
        "unitMine" => cloud_squares(
            emit,
            e,
            4,
            e.fin() * 6.0,
            e.color,
            Rgba::from(pal::STONE_GRAY),
            |fout| fout * 1.3 + 0.4,
        ),
        "producesmoke" => {
            rand_len_vectors(e.id.raw() as u64, 8, 4.0 + e.fin() * 18.0, |x, y| {
                emit.color_lerp(Rgba::WHITE, Rgba::from(pal::ACCENT), e.fin());
                emit.square(e.x + x, e.y + y, 1.0 + e.fout() * 3.0, 45.0);
            });
        }
        "artilleryTrailSmoke" => {
            emit.color(e.color);
            let mut rng = seeded(e);
            for _ in 0..13 {
                let fin = e.fin() / rng.random_range_float(0.5, 1.0);
                let fout = 1.0 - fin;
                let a = rng.random_float(360.0);
                let l = rng.random_range_float(0.5, 1.0);
                if fin <= 1.0 {
                    emit.alpha((0.5 - (fin - 0.5).abs()) * 2.0);
                    emit.circle(
                        e.x + trnsx(a, fin * 24.0 * l),
                        e.y + trnsy(a, fin * 24.0 * l),
                        0.5 + fout * 4.0,
                    );
                }
            }
        }
        "smokeCloud" => {
            rand_len_vectors_fin(e.id.raw() as u64, e.fin(), 30, 30.0, |x, y, fin, fout| {
                emit.color(GRAY);
                emit.alpha((0.5 - (fin - 0.5).abs()) * 2.0);
                emit.circle(e.x + x, e.y + y, 0.5 + fout * 4.0);
            });
        }
        "smeltsmoke" => {
            rand_len_vectors(e.id.raw() as u64, 6, 4.0 + e.fin() * 5.0, |x, y| {
                emit.color_lerp(Rgba::WHITE, e.color, e.fin());
                emit.square(e.x + x, e.y + y, 0.5 + e.fout() * 2.0, 45.0);
            });
        }
        "coalSmeltsmoke" => {
            rand_len_vectors_fin(
                e.id.raw() as u64,
                0.2 + e.fin(),
                4,
                6.3,
                |x, y, _fin, out| {
                    emit.color_lerp(
                        DARK_GRAY,
                        Rgba::from(pal::COAL_BLACK),
                        interp_powdown(e.fin()),
                    );
                    emit.circle(e.x + x, e.y + y, out * 2.0 + 0.35);
                },
            );
        }
        "formsmoke" => {
            rand_len_vectors(e.id.raw() as u64, 6, 5.0 + e.fin() * 8.0, |x, y| {
                emit.color_lerp(Rgba::from(pal::PLASTIC_SMOKE), LIGHT_GRAY, e.fin());
                emit.square(e.x + x, e.y + y, 0.2 + e.fout() * 2.0, 45.0);
            });
        }
        "blastsmoke" => {
            rand_len_vectors(e.id.raw() as u64, 12, 1.0 + e.fin() * 23.0, |x, y| {
                let size = 2.0 + e.fout() * 6.0;
                emit.color_lerp(LIGHT_GRAY, DARK_GRAY, e.fin());
                emit.circle(e.x + x, e.y + y, size / 2.0);
            });
        }
        "lava" => {
            rand_len_vectors(e.id.raw() as u64, 3, 1.0 + e.fin() * 10.0, |x, y| {
                let size = e.fslope() * 4.0;
                emit.color_lerp(ORANGE, GRAY, e.fin());
                emit.circle(e.x + x, e.y + y, size / 2.0);
            });
        }
        "dooropen" => {
            emit.stroke(e.fout() * 1.6);
            emit.square_line(e.x, e.y, e.rotation * TILESIZE / 2.0 + e.fin() * 2.0, 0.0);
        }
        "doorclose" => {
            emit.stroke(e.fout() * 1.6);
            emit.square_line(e.x, e.y, e.rotation * TILESIZE / 2.0 + e.fout() * 2.0, 0.0);
        }
        "dooropenlarge" => {
            emit.stroke(e.fout() * 1.6);
            emit.square_line(e.x, e.y, TILESIZE + e.fin() * 2.0, 0.0);
        }
        "doorcloselarge" => {
            emit.stroke(e.fout() * 1.6);
            emit.square_line(e.x, e.y, TILESIZE + e.fout() * 2.0, 0.0);
        }
        "generate" => {
            emit.color_lerp(ORANGE, YELLOW, e.fin());
            emit.stroke(1.0);
            emit.spikes(e.x, e.y, e.fin() * 5.0, 2.0, 8);
        }
        "mineWallSmall" => cloud(emit, e, 2, e.fin() * 6.0, e.color, DARK_GRAY, |fout| {
            fout + 0.5
        }),
        "mineSmall" => cloud_squares(emit, e, 3, e.fin() * 5.0, e.color, LIGHT_GRAY, |fout| {
            fout + 0.5
        }),
        "mine" => cloud_squares(
            emit,
            e,
            6,
            3.0 + e.fin() * 6.0,
            e.color,
            LIGHT_GRAY,
            |fout| fout * 2.0,
        ),
        "mineBig" => cloud_squares(
            emit,
            e,
            6,
            4.0 + e.fin() * 8.0,
            e.color,
            LIGHT_GRAY,
            |fout| fout * 2.0 + 0.2,
        ),
        "mineHuge" => cloud_squares(
            emit,
            e,
            8,
            5.0 + e.fin() * 10.0,
            e.color,
            LIGHT_GRAY,
            |fout| fout * 2.0 + 0.5,
        ),
        "mineImpact" => cloud_squares(
            emit,
            e,
            12,
            5.0 + e.finpow() * 22.0,
            e.color,
            LIGHT_GRAY,
            |fout| fout * 2.5 + 0.5,
        ),
        "mineImpactWave" => {
            emit.color(e.color);
            emit.stroke(e.fout() * 1.5);
            rand_len_vectors(
                e.id.raw() as u64,
                12,
                4.0 + e.finpow() * e.rotation,
                |x, y| {
                    emit.line_angle(e.x + x, e.y + y, angle(x, y), e.fout() * 5.0 + 1.0);
                },
            );
            e.scaled_view(30.0, |inner| {
                emit.stroke(5.0 * inner.fout());
                emit.circle_line(inner.x, inner.y, inner.finpow() * 28.0);
            });
        }
        "payloadReceive" => cloud_squares(
            emit,
            e,
            12,
            7.0 + e.fin() * 13.0,
            Rgba::WHITE,
            Rgba::from(pal::ACCENT),
            |fout| fout * 2.1 + 0.5,
        ),
        "teleportActivate" => {
            emit.color(e.color);
            e.scaled_view(8.0, |inner| {
                emit.stroke(inner.fout() * 4.0);
                emit.circle_line(inner.x, inner.y, 4.0 + inner.fin() * 27.0);
            });
            emit.stroke(e.fout() * 2.0);
            rand_len_vectors(e.id.raw() as u64, 30, 4.0 + 40.0 * e.fin(), |x, y| {
                emit.line_angle(e.x + x, e.y + y, angle(x, y), e.fin() * 4.0 + 1.0);
            });
        }
        "teleport" => {
            emit.color(e.color);
            emit.stroke(e.fin() * 2.0);
            emit.circle_line(e.x, e.y, 7.0 + e.fout() * 8.0);
            rand_len_vectors(e.id.raw() as u64, 20, 6.0 + 20.0 * e.fout(), |x, y| {
                emit.line_angle(e.x + x, e.y + y, angle(x, y), e.fin() * 4.0 + 1.0);
            });
        }
        "teleportOut" => {
            emit.color(e.color);
            emit.stroke(e.fout() * 2.0);
            emit.circle_line(e.x, e.y, 7.0 + e.fin() * 8.0);
            rand_len_vectors(e.id.raw() as u64, 20, 4.0 + 20.0 * e.fin(), |x, y| {
                emit.line_angle(e.x + x, e.y + y, angle(x, y), e.fslope() * 4.0 + 1.0);
            });
        }
        "ripple" => {
            emit.color(e.color.mul_rgb(1.5));
            emit.stroke(e.fout() * 1.4);
            emit.circle_line(e.x, e.y, (2.0 + e.fin() * 4.0) * e.rotation);
        }
        "bubble" => {
            emit.color(e.color.mul_rgb(1.05));
            emit.stroke(e.fout() + 0.2);
            rand_len_vectors(e.id.raw() as u64, 2, e.rotation * 0.9, |x, y| {
                emit.circle_line(e.x + x, e.y + y, 1.0 + e.fin() * 3.0);
            });
        }
        "launchAccelerator" => {
            emit.color(Rgba::from(pal::ACCENT));
            emit.stroke(e.fout() * 2.0);
            emit.circle_line(e.x, e.y, 4.0 + e.finpow() * 160.0);
        }
        "launch" => {
            emit.color(Rgba::from(pal::COMMAND));
            emit.stroke(e.fout() * 2.0);
            emit.circle_line(e.x, e.y, 4.0 + e.finpow() * 120.0);
        }
        "launchPod" => {
            emit.color(Rgba::from(pal::ENGINE));
            e.scaled_view(25.0, |inner| {
                emit.stroke(inner.fout() * 2.0);
                emit.circle_line(inner.x, inner.y, 4.0 + inner.finpow() * 30.0);
            });
            emit.stroke(e.fout() * 2.0);
            rand_len_vectors(e.id.raw() as u64, 24, e.finpow() * 50.0, |x, y| {
                emit.line_angle(e.x + x, e.y + y, angle(x, y), e.fout() * 4.0 + 1.0);
            });
        }
        "healWaveMend" => {
            emit.color(e.color);
            emit.stroke(e.fout() * 2.0);
            emit.circle_line(e.x, e.y, e.finpow() * e.rotation);
        }
        "overdriveWave" => {
            emit.color(e.color);
            emit.stroke(e.fout());
            emit.circle_line(e.x, e.y, e.finpow() * e.rotation);
        }
        "healBlock" => {
            emit.color(Rgba::from(pal::HEAL));
            emit.stroke(2.0 * e.fout() + 0.5);
            emit.square_line(
                e.x,
                e.y,
                1.0 + (e.fin() * e.rotation * TILESIZE / 2.0 - 1.0),
                0.0,
            );
        }
        "healBlockFull" => {
            if !matches!(e.data, EffectData::Block(_)) {
                return;
            }
            emit.mixcol(e.color, 1.0);
            emit.alpha(e.fout());
            emit.rect_native(content_region(), e.x, e.y, 0.0);
        }
        "rotateBlock" => {
            emit.color(Rgba::from(pal::ACCENT));
            emit.alpha(e.fout());
            emit.square(e.x, e.y, e.rotation * TILESIZE / 2.0, 0.0);
        }
        "lightBlock" => {
            emit.color(e.color);
            emit.alpha(e.fout());
            emit.square(e.x, e.y, e.rotation * TILESIZE / 2.0, 0.0);
        }
        "overdriveBlockFull" => {
            emit.color(e.color);
            emit.alpha(e.fslope() * 0.4);
            emit.square(e.x, e.y, e.rotation * TILESIZE, 0.0);
        }
        "shieldBreak" => {
            emit.color(e.color);
            emit.stroke(3.0 * e.fout());
            let (sides, rot) = match &e.data {
                EffectData::Shield {
                    sides, rotation, ..
                } => (*sides as u16, *rotation),
                _ => (6u16, 6.0),
            };
            emit.poly_line(e.x, e.y, sides, e.rotation + e.fin(), rot);
        }
        "arcShieldBreak" => {
            emit.stroke(3.0 * e.fout());
            emit.color(e.color);
            if let EffectData::Unit { .. } = e.data {
                emit.circle_line(e.x, e.y, e.rotation.max(4.0));
            }
        }
        "coreLandDust" | "podLandDust" => {
            emit.color(e.color);
            emit.alpha(e.fout_margin(0.1));
            let mut rng = seeded(e);
            let scale = if name == "coreLandDust" { 90.0 } else { 35.0 };
            let size = if name == "coreLandDust" { 8.0 } else { 5.0 };
            let d = e.finpow() * scale * rng.random_range_float(0.2, 1.0);
            emit.circle(
                e.x + trnsx(e.rotation, d),
                e.y + trnsy(e.rotation, d),
                size * rng.random_range_float(0.6, 1.0) * e.fout_margin(0.2),
            );
        }
        "unitShieldBreak" => {
            let radius = match e.data {
                EffectData::Unit { .. } => 8.0 * 1.3,
                _ => 8.0 * 1.3,
            };
            e.scaled_view(16.0, |inner| {
                emit.color(inner.color);
                emit.alpha(0.9);
                emit.stroke(inner.fout() * 2.0 + 0.1);
                rand_len_vectors(
                    inner.id.raw() as u64,
                    (radius * 1.2) as i32,
                    radius / 2.0 + inner.finpow() * radius * 1.25,
                    |x, y| {
                        emit.line_angle(
                            inner.x + x,
                            inner.y + y,
                            angle(x, y),
                            inner.fout() * 5.0 + 1.0,
                        );
                    },
                );
            });
            emit.color(e.color);
            emit.alpha(e.fout() * 0.9);
            emit.stroke(e.fout());
            emit.circle_line(e.x, e.y, radius);
        }
        "chainLightning" => chain(emit, e, 2.5),
        "chainEmp" => chain(emit, e, 4.0),
        "legDestroy" => {
            if let EffectData::LegDestroy(d) = e.data {
                emit.stroke(d.length.max(1.0) * 0.3);
                emit.alpha(e.foutpow());
                emit.line(
                    d.x,
                    d.y,
                    d.x + trnsx(d.rotation, d.length),
                    d.y + trnsy(d.rotation, d.length),
                    false,
                );
            }
        }
        "debugLine" => {
            let pts = match &e.data {
                EffectData::Vec2Array(v) | EffectData::Positions(v) => v.clone(),
                _ => return,
            };
            emit.color(e.color);
            emit.stroke(2.0);
            for w in pts.windows(2) {
                emit.line(w[0].0, w[0].1, w[1].0, w[1].1, false);
            }
            emit.reset();
        }
        "debugRect" => {
            if let EffectData::Rect { x, y, w, h } = e.data {
                emit.color(e.color);
                emit.stroke(2.0);
                let hw = w / 2.0;
                let hh = h / 2.0;
                emit.line(x - hw, y - hh, x + hw, y - hh, false);
                emit.line(x + hw, y - hh, x + hw, y + hh, false);
                emit.line(x + hw, y + hh, x - hw, y + hh, false);
                emit.line(x - hw, y + hh, x - hw, y - hh, false);
                emit.reset();
            }
        }
        _ => {}
    }
}

/// `Interp.pow3In` for `e.finpowdown()`-style coal smoke.
fn interp_powdown(f: f32) -> f32 {
    Interp::Pow3In.apply(f)
}

/// Java's `ColorPair` helper for the casing family.
struct ColorPair {
    a: Rgba,
    b: Rgba,
    c: Rgba,
}

impl ColorPair {
    fn new(a: Rgba, b: Rgba, c: Rgba) -> Self {
        Self { a, b, c }
    }
}

/// `Fx.casing2/3/4`: single rotating rect casing.
#[allow(clippy::too_many_arguments)]
fn casing(
    emit: &mut FxEmit,
    e: &EffectContainer,
    base_len: f32,
    colors: ColorPair,
    w: f32,
    h: f32,
    _alpha_margin: f32,
    _seed_off: i64,
    _region: bool,
) {
    emit.color_lerp3(colors.a, colors.b, colors.c, e.fin());
    emit.alpha(e.fout_margin(0.5));
    let rot = e.rotation.abs() + 90.0;
    let i = -crate::fx::angles::sign(e.rotation);
    let len = (base_len + e.finpow() * 6.0) * i as f32;
    let lr = rot + e.fin() * 30.0 * i as f32;
    let x =
        e.x + trnsx(lr, len) + random_seed_range(e.id.raw() as i64 + i as i64 + 7, 3.0 * e.fin());
    let y =
        e.y + trnsy(lr, len) + random_seed_range(e.id.raw() as i64 + i as i64 + 8, 3.0 * e.fin());
    emit.rect_fill(x, y, w, h, rot + e.fin() * 50.0 * i as f32);
}

/// `Fx.casing2Double`/`casing3Double`: mirrored pair.
#[allow(clippy::too_many_arguments)]
fn casing_double(
    emit: &mut FxEmit,
    e: &EffectContainer,
    base_len: f32,
    a: Rgba,
    c: Rgba,
    w: f32,
    h: f32,
    _alpha_margin: f32,
    _seed_off: i64,
) {
    emit.color_lerp(a, c, e.fin());
    emit.alpha(e.fout_margin(0.5));
    let rot = e.rotation.abs() + 90.0;
    for i in [-1_i32, 1] {
        let len = (base_len + e.finpow() * 6.0) * i as f32;
        let lr = rot + e.fin() * 20.0 * i as f32;
        let x = e.x
            + trnsx(lr, len)
            + random_seed_range(e.id.raw() as i64 + i as i64 + 7, 3.0 * e.fin());
        let y = e.y
            + trnsy(lr, len)
            + random_seed_range(e.id.raw() as i64 + i as i64 + 8, 3.0 * e.fin());
        emit.rect_fill(x, y, w, h, rot + e.fin() * 50.0 * i as f32);
    }
}

/// `Fx.chainLightning`/`Fx.chainEmp`: jagged polyline to a `Position` payload.
fn chain(emit: &mut FxEmit, e: &EffectContainer, stroke: f32) {
    let EffectData::Position { x: tx, y: ty } = e.data else {
        return;
    };
    let dx = tx - e.x;
    let dy = ty - e.y;
    let dst = (dx * dx + dy * dy).sqrt();
    if dst <= 0.0 {
        return;
    }
    let normx = dx / dst;
    let normy = dy / dst;
    let range = 6.0;
    let links = (dst / range).ceil().max(1.0) as i32;
    let spacing = dst / links as f32;
    emit.stroke(stroke * e.fout());
    emit.color_lerp(Rgba::WHITE, e.color, e.fin());
    let mut rng = seeded(e);
    let mut px = e.x;
    let mut py = e.y;
    for i in 0..links {
        let (nx, ny) = if i == links - 1 {
            (tx, ty)
        } else {
            let len = (i + 1) as f32 * spacing;
            let mut dir = ArcRand::new(rng.next_long());
            let rx = dir.random_float(360.0);
            let off = range / 2.0;
            (
                e.x + normx * len + trnsx(rx, off),
                e.y + normy * len + trnsy(rx, off),
            )
        };
        emit.line(px, py, nx, ny, false);
        px = nx;
        py = ny;
    }
}

/// A `Rgba` helper used by a couple of bodies.
trait RgbaMul {
    fn mul_rgb(self, factor: f32) -> Rgba;
}

impl RgbaMul for Rgba {
    fn mul_rgb(self, factor: f32) -> Rgba {
        Rgba::new(self.r * factor, self.g * factor, self.b * factor, self.a)
    }
}

/// Simple gray cloud helper used by the explosion family.
fn cloud_simple(emit: &mut FxEmit, e: &EffectContainer, amount: i32, length: f32, size: f32) {
    emit.color(GRAY);
    rand_len_vectors(e.id.raw() as u64, amount, length, |x, y| {
        emit.circle(e.x + x, e.y + y, size);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::def::registry;
    use crate::render::draw::DrawProgram;

    #[test]
    fn every_wave_body_dispatches_without_panic() {
        for def in registry().iter() {
            if def.name == "none" {
                continue;
            }
            let mut program = DrawProgram::new();
            let mut emit = FxEmit::new(&mut program, def.layer);
            let e = EffectContainer {
                id: def.id,
                x: 10.0,
                y: 20.0,
                time: def.lifetime * 0.5,
                lifetime: def.lifetime,
                rotation: 30.0,
                color: Rgba::WHITE,
                data: EffectData::None,
                inner: None,
            };
            dispatch(def.name, &mut emit, &e, &crate::fx::data::EmptySnapshot);
            let _ = emit.get_color();
        }
    }
}
