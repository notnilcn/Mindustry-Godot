// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Angles` helpers + `randLenVectors` with the view-side `ArcRand` (plan 17
//! §3.4/§3.15). Seeded only by the effect id / offset passed in, matching Java
//! `randLenVectors(e.id, …)`. Never touches `RngStream::Sim`.

use crate::math::ArcRand;

/// `Angles.trnsx(angle, length)` (degrees).
#[inline]
pub fn trnsx(angle: f32, length: f32) -> f32 {
    length * angle.to_radians().cos()
}

/// `Angles.trnsy(angle, length)` (degrees).
#[inline]
pub fn trnsy(angle: f32, length: f32) -> f32 {
    length * angle.to_radians().sin()
}

/// `Angles.trnsx(angle, x, y)` — rotates `(x, y)` counter-clockwise.
#[inline]
pub fn trnsx_vec(angle: f32, x: f32, y: f32) -> f32 {
    let (sin, cos) = angle.to_radians().sin_cos();
    x * cos - y * sin
}

/// `Angles.trnsy(angle, x, y)`.
#[inline]
pub fn trnsy_vec(angle: f32, x: f32, y: f32) -> f32 {
    let (sin, cos) = angle.to_radians().sin_cos();
    x * sin + y * cos
}

/// `Mathf.angle(x, y)` — degrees in `[0, 360)`.
#[inline]
pub fn angle(x: f32, y: f32) -> f32 {
    let result = x.atan2(y).to_degrees();
    if result < 0.0 { result + 360.0 } else { result }
}

/// `Mathf.sign(float)`.
#[inline]
pub fn sign(f: f32) -> i32 {
    if f < 0.0 { -1 } else { 1 }
}

/// `Mathf.randomSeed(seed, max)` (`seed * 99999` xorshift draw).
#[inline]
pub fn random_seed(seed: i64, max: f32) -> f32 {
    let mut rand = ArcRand::new((seed.wrapping_mul(99999)) as u64);
    rand.next_float() * max
}

/// `Mathf.randomSeed(seed)`.
#[inline]
pub fn random_seed_unit(seed: i64) -> f32 {
    let mut rand = ArcRand::new((seed.wrapping_mul(99999)) as u64);
    rand.next_float()
}

/// `Mathf.randomSeedRange(seed, range)`.
#[inline]
pub fn random_seed_range(seed: i64, range: f32) -> f32 {
    let mut rand = ArcRand::new((seed.wrapping_mul(99999)) as u64);
    range * (rand.next_float() - 0.5) * 2.0
}

/// `randLenVectors(seed, amount, length, cons)`.
pub fn rand_len_vectors(seed: u64, amount: i32, length: f32, mut cons: impl FnMut(f32, f32)) {
    let mut rand = ArcRand::new(seed);
    for _ in 0..amount {
        let a = rand.random_float(360.0);
        let l = rand.random_float(length);
        cons(trnsx(a, l), trnsy(a, l));
    }
}

/// `randLenVectors(seed, amount, minLength, length, cons)`.
pub fn rand_len_vectors_min(
    seed: u64,
    amount: i32,
    min_length: f32,
    length: f32,
    mut cons: impl FnMut(f32, f32),
) {
    let mut rand = ArcRand::new(seed);
    for _ in 0..amount {
        let a = rand.random_float(360.0);
        let l = min_length + rand.random_float(length);
        cons(trnsx(a, l), trnsy(a, l));
    }
}

/// `randLenVectors(seed, amount, length, angle, range, cons)`.
pub fn rand_len_vectors_cone(
    seed: u64,
    amount: i32,
    length: f32,
    angle: f32,
    range: f32,
    mut cons: impl FnMut(f32, f32),
) {
    let mut rand = ArcRand::new(seed);
    for _ in 0..amount {
        let a = angle + rand.range_float(range);
        let l = rand.random_float(length);
        cons(trnsx(a, l), trnsy(a, l));
    }
}

/// `randLenVectors(seed, fin, amount, length, ParticleConsumer)`: yields
/// `(x, y, fin_part, fout_part)`.
pub fn rand_len_vectors_fin(
    seed: u64,
    fin: f32,
    amount: i32,
    length: f32,
    mut cons: impl FnMut(f32, f32, f32, f32),
) {
    let mut rand = ArcRand::new(seed);
    for _ in 0..amount {
        let l = rand.next_float();
        let a = rand.random_float(360.0);
        let len = length * l * fin;
        cons(trnsx(a, len), trnsy(a, len), fin * l, (1.0 - fin) * l);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trns_matches_trig() {
        assert!((trnsx(0.0, 5.0) - 5.0).abs() < 1e-5);
        assert!((trnsy(90.0, 5.0) - 5.0).abs() < 1e-5);
        assert!((trnsx(180.0, 5.0) + 5.0).abs() < 1e-4);
    }

    #[test]
    fn angle_is_normalized() {
        assert!((angle(0.0, 1.0) - 0.0).abs() < 1e-5);
        assert!((angle(1.0, 0.0) - 90.0).abs() < 1e-4);
        assert!((angle(-1.0, 0.0) - 270.0).abs() < 1e-4);
    }

    #[test]
    fn rand_len_vectors_is_deterministic() {
        let mut a = Vec::new();
        rand_len_vectors(7, 4, 10.0, |x, y| a.push((x, y)));
        let mut b = Vec::new();
        rand_len_vectors(7, 4, 10.0, |x, y| b.push((x, y)));
        assert_eq!(a, b);
        assert_eq!(a.len(), 4);
    }
}
