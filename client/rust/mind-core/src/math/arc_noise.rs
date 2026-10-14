// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Arc `arc.util.noise.Noise` (H05 T3).
//!
//! Exact port of `ARC/arc-core/src/arc/util/noise/Noise.java` (Perlin noise
//! with the static tables initialised from `new java.util.Random(100)`;
//! Mindustry never calls `Noise.setSeed`). Values stay `f64` like Java and are
//! cast to `f32` where Java casts. Tables build once via [`OnceLock`].

use std::sync::OnceLock;

use crate::random::JavaRandom;

/// `Noise.seed`.
const SEED: u64 = 100;
/// `Noise.P`.
const P: usize = 8;
/// `Noise.B = 1 << P`.
const B: usize = 1 << P;
/// `Noise.M = B - 1`.
const M: i32 = B as i32 - 1;
/// `Noise.NP`.
const NP: usize = 8;
/// `Noise.N = 1 << NP`.
const N: f64 = (1u32 << NP) as f64;

/// The `Noise` static tables (`p`/`g1`/`g2`/`points`, length `B + B + 2`).
struct Tables {
    p: [i32; B + B + 2],
    g1: [f64; B + B + 2],
    g2: [[f64; 2]; B + B + 2],
    points: [[f64; 3]; 32],
}

static TABLES: OnceLock<Tables> = OnceLock::new();

fn tables() -> &'static Tables {
    TABLES.get_or_init(init)
}

/// `Noise.init`: builds the tables from `new java.util.Random(100)`. The 3-D
/// gradient loop consumes RNG draws (its points are only used by
/// [`raw_noise3d`]), and the shuffle uses `nextLong`, exactly as Java.
fn init() -> Tables {
    let mut p = [0i32; B + B + 2];
    let mut g1 = [0f64; B + B + 2];
    let mut g2 = [[0f64; 2]; B + B + 2];
    let mut points = [[0f64; 3]; 32];
    let mut r = JavaRandom::new(SEED);

    let mut i = 0usize;
    while i < B {
        p[i] = i as i32;
        g1[i] = 2.0 * r.next_double() - 1.0;

        let (mut u, mut v);
        loop {
            u = 2.0 * r.next_double() - 1.0;
            v = 2.0 * r.next_double() - 1.0;
            if !(u * u + v * v > 1.0
                || u.abs() > 2.5 * v.abs()
                || v.abs() > 2.5 * u.abs()
                || (u.abs() - v.abs()).abs() < 0.4)
            {
                break;
            }
        }
        g2[i][0] = u;
        g2[i][1] = v;
        normalize2(&mut g2[i]);

        loop {
            let u3 = 2.0 * r.next_double() - 1.0;
            let v3 = 2.0 * r.next_double() - 1.0;
            let w3 = 2.0 * r.next_double() - 1.0;
            let au = u3.abs();
            let av = v3.abs();
            let aw = w3.abs();
            let lo = au.min(av).min(aw);
            let hi = au.max(av).max(aw);
            if !(u3 * u3 + v3 * v3 + w3 * w3 > 1.0
                || hi > 4.0 * lo
                || (au - av).abs().min((au - aw).abs()).min((av - aw).abs()) < 0.2)
            {
                break;
            }
        }

        i += 1;
    }

    // `while(--i > 0)`: after the loop `i == B`, so this runs 7..1.
    i = B;
    while {
        i -= 1;
        i > 0
    } {
        let k = p[i];
        let j = (r.next_long() & M as i64) as usize;
        p[i] = p[j];
        p[j] = k;
    }

    for i in 0..B + 2 {
        p[B + i] = p[i];
        g1[B + i] = g1[i];
        g2[B + i][0] = g2[i][0];
        g2[B + i][1] = g2[i][1];
    }

    points[3][0] = (1.0f64 / 3.0).sqrt();
    points[3][1] = points[3][0];
    points[3][2] = points[3][0];
    let r2 = (1.0f64 / 2.0).sqrt();
    let s = (2.0 + r2 + r2).sqrt();

    for (i, row) in points.iter_mut().take(3).enumerate() {
        for (j, value) in row.iter_mut().take(3).enumerate() {
            *value = if i == j { 1.0 + r2 + r2 } else { r2 } / s;
        }
    }
    for i in 0..=1 {
        for j in 0..=1 {
            for k in 0..=1 {
                let n = i + j * 2 + k * 4;
                if n > 0 {
                    let source = [points[0], points[1], points[2], points[3]];
                    let sx = if i == 0 { 1.0 } else { -1.0 };
                    let sy = if j == 0 { 1.0 } else { -1.0 };
                    let sz = if k == 0 { 1.0 } else { -1.0 };
                    for (m, value) in points[4 * n..4 * n + 4].iter_mut().enumerate() {
                        *value = [source[m][0] * sx, source[m][1] * sy, source[m][2] * sz];
                    }
                }
            }
        }
    }

    Tables { p, g1, g2, points }
}

/// `Noise.normalize2`.
fn normalize2(v: &mut [f64; 2]) {
    let s = (v[0] * v[0] + v[1] * v[1]).sqrt();
    v[0] /= s;
    v[1] /= s;
}

fn lerp(t: f64, a: f64, b: f64) -> f64 {
    a + t * (b - a)
}

fn s_curve(t: f64) -> f64 {
    t * t * (3.0 - t - t)
}

/// `Noise.rawNoise(double x)`.
fn raw_noise1(x: f64) -> f64 {
    let t = x + N;
    let bx0 = (t as i32) & M;
    let bx1 = (bx0 + 1) & M;
    let rx0 = t - (t as i32) as f64;
    let rx1 = rx0 - 1.0;
    let tables = tables();
    let sx = s_curve(rx0);
    let u = rx0 * tables.g1[tables.p[bx0 as usize] as usize];
    let v = rx1 * tables.g1[tables.p[bx1 as usize] as usize];
    lerp(sx, u, v)
}

/// `Noise.rawNoise(double x, double y)`.
fn raw_noise2(x: f64, y: f64) -> f64 {
    let tables = tables();
    let t = x + N;
    let bx0 = (t as i32) & M;
    let bx1 = (bx0 + 1) & M;
    let rx0 = t - (t as i32) as f64;
    let rx1 = rx0 - 1.0;

    let t = y + N;
    let by0 = (t as i32) & M;
    let by1 = (by0 + 1) & M;
    let ry0 = t - (t as i32) as f64;
    let ry1 = ry0 - 1.0;

    let i = tables.p[bx0 as usize] as usize;
    let j = tables.p[bx1 as usize] as usize;

    let b00 = tables.p[i + by0 as usize] as usize;
    let b10 = tables.p[j + by0 as usize] as usize;
    let b01 = tables.p[i + by1 as usize] as usize;
    let b11 = tables.p[j + by1 as usize] as usize;

    let sx = s_curve(rx0);
    let sy = s_curve(ry0);

    let q = tables.g2[b00];
    let u = rx0 * q[0] + ry0 * q[1];
    let q = tables.g2[b10];
    let v = rx1 * q[0] + ry0 * q[1];
    let a = lerp(sx, u, v);

    let q = tables.g2[b01];
    let u = rx0 * q[0] + ry1 * q[1];
    let q = tables.g2[b11];
    let v = rx1 * q[0] + ry1 * q[1];
    let b = lerp(sx, u, v);

    lerp(sy, a, b)
}

/// `Noise.G(i)`.
fn g(i: i32) -> &'static [f64; 3] {
    &tables().points[(i % 32) as usize]
}

/// `Noise.rawNoise(double x, double y, double z)` (not used by the 2-D
/// `noise`; transcribed for the full port).
pub fn raw_noise3d(x: f64, y: f64, z: f64) -> f64 {
    let mut bx = (x.floor() as i64 % B as i64) as i32;
    if bx < 0 {
        bx += B as i32;
    }
    let rx0 = x - x.floor();
    let rx1 = rx0 - 1.0;

    let mut by = (y.floor() as i64 % B as i64) as i32;
    if by < 0 {
        by += B as i32;
    }
    let ry0 = y - y.floor();
    let ry1 = ry0 - 1.0;

    let mut bz = (z.floor() as i64 % B as i64) as i32;
    if bz < 0 {
        bz += B as i32;
    }
    let mut rz = z - z.floor();

    let tables = tables();
    let b0 = tables.p[bx as usize];
    bx += 1;
    let b1 = tables.p[bx as usize];
    let b00 = tables.p[(b0 + by) as usize];
    let b10 = tables.p[(b1 + by) as usize];
    by += 1;
    let b01 = tables.p[(b0 + by) as usize];
    let b11 = tables.p[(b1 + by) as usize];

    let sx = s_curve(rx0);
    let sy = s_curve(ry0);
    let sz = s_curve(rz);

    let q = g(b00 + bz);
    let u = rx0 * q[0] + ry0 * q[1] + rz * q[2];
    let q = g(b10 + bz);
    let v = rx1 * q[0] + ry0 * q[1] + rz * q[2];
    let a = lerp(sx, u, v);
    let q = g(b01 + bz);
    let u = rx0 * q[0] + ry1 * q[1] + rz * q[2];
    let q = g(b11 + bz);
    let v = rx1 * q[0] + ry1 * q[1] + rz * q[2];
    let b = lerp(sx, u, v);
    let c = lerp(sy, a, b);

    bz += 1;
    rz -= 1.0;
    let q = g(b00 + bz);
    let u = rx0 * q[0] + ry0 * q[1] + rz * q[2];
    let q = g(b10 + bz);
    let v = rx1 * q[0] + ry0 * q[1] + rz * q[2];
    let a = lerp(sx, u, v);
    let q = g(b01 + bz);
    let u = rx0 * q[0] + ry1 * q[1] + rz * q[2];
    let q = g(b11 + bz);
    let v = rx1 * q[0] + ry1 * q[1] + rz * q[2];
    let b = lerp(sx, u, v);
    let d = lerp(sy, a, b);

    lerp(sz, c, d)
}

/// `Noise.snoise(float x, float y, float scale, float mag)`.
pub fn snoise(x: f32, y: f32, scale: f32, mag: f32) -> f32 {
    let xf = x / scale;
    let yf = y / scale;
    (raw_noise2(xf as f64, yf as f64) * mag as f64) as f32
}

/// `Noise.noise(float x, float y, float scale, float mag)`.
pub fn noise(x: f32, y: f32, scale: f32, mag: f32) -> f32 {
    (snoise(x, y, scale, mag) as f64 / 2.0) as f32
}

/// `Noise.fnoise(x, y, scale, mag)`.
pub fn fnoise(x: f32, y: f32, scale: f32, mag: f32) -> f32 {
    let xf = x / scale;
    let yf = y / scale;
    (raw_noise2(xf as f64, yf as f64) * mag as f64) as f32
}

/// `Noise.rawNoise(double x)` (public single-axis form).
pub fn raw_noise(x: f64) -> f64 {
    raw_noise1(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle values computed with the Arc jar (round 3, 2026-10-14):
    /// `Noise.noise(x, y, 11f, 7f)` / `Noise.noise(x, y, 22f, 15f)`.
    #[test]
    fn arc_noise_matches_java() {
        let cases: [(f32, f32, f32, f32); 4] = [
            (10.0, 20.0, 0.19717087, -0.7115014),
            (150.0, 150.0, 0.4026778, -1.1208266),
            (37.0, 211.0, -0.762948, -0.89685315),
            (0.0, 0.0, 0.0, 0.0),
        ];
        for (x, y, n11, n22) in cases {
            let a = noise(x, y, 11.0, 7.0);
            let b = noise(x, y, 22.0, 15.0);
            assert!((a - n11).abs() < 1e-6, "noise({x},{y},11,7) = {a} != {n11}");
            assert!(
                (b - n22).abs() < 1e-6,
                "noise({x},{y},22,15) = {b} != {n22}"
            );
        }
    }

    #[test]
    fn raw_noise_is_deterministic_and_ranged() {
        let a = noise(123.0, 456.0, 11.0, 7.0);
        let b = noise(123.0, 456.0, 11.0, 7.0);
        assert_eq!(a, b);
        assert!(a.abs() <= 7.0);
    }
}
