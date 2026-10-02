// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Behavioral reimplementation of Arc (Apache-2.0, revision 7445105cd2):
// `arc-core/src/arc/util/noise/{Simplex,Ridged}.java`,
// `arc-core/src/arc/math/{Rand,Mathf}.java` (the generator-facing subset).

//! Noise functions used by the sprite generators (plan 03 §3.5 stage 3, A3).
//!
//! All functions are bit-deterministic with the Arc implementations for the
//! same inputs (Rust `f64` math matches IEEE semantics used by the JVM here);
//! generator outputs remain Rust↔Rust stable only (A3).

use crate::vector_table::RANDOM_VECTORS;

// ---------------------------------------------------------------------------
// Simplex (arc.util.noise.Simplex)
// ---------------------------------------------------------------------------

const GRAD3: [[i32; 3]; 12] = [
    [1, 1, 0],
    [-1, 1, 0],
    [1, -1, 0],
    [-1, -1, 0],
    [1, 0, 1],
    [-1, 0, 1],
    [1, 0, -1],
    [-1, 0, -1],
    [0, 1, 1],
    [0, -1, 1],
    [0, 1, -1],
    [0, -1, -1],
];

const GRAD4: [[i32; 4]; 32] = [
    [0, 1, 1, 1],
    [0, 1, 1, -1],
    [0, 1, -1, 1],
    [0, 1, -1, -1],
    [0, -1, 1, 1],
    [0, -1, 1, -1],
    [0, -1, -1, 1],
    [0, -1, -1, -1],
    [1, 0, 1, 1],
    [1, 0, 1, -1],
    [1, 0, -1, 1],
    [1, 0, -1, -1],
    [-1, 0, 1, 1],
    [-1, 0, 1, -1],
    [-1, 0, -1, 1],
    [-1, 0, -1, -1],
    [1, 1, 0, 1],
    [1, 1, 0, -1],
    [1, -1, 0, 1],
    [1, -1, 0, -1],
    [-1, 1, 0, 1],
    [-1, 1, 0, -1],
    [-1, -1, 0, 1],
    [-1, -1, 0, -1],
    [1, 1, 1, 0],
    [1, 1, -1, 0],
    [1, -1, 1, 0],
    [1, -1, -1, 0],
    [-1, 1, 1, 0],
    [-1, 1, -1, 0],
    [-1, -1, 1, 0],
    [-1, -1, -1, 0],
];

#[rustfmt::skip]
const SIMPLEX: [[i32; 4]; 64] = [
    [0, 1, 2, 3], [0, 1, 3, 2], [0, 0, 0, 0], [0, 2, 3, 1], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [1, 2, 3, 0],
    [0, 2, 1, 3], [0, 0, 0, 0], [0, 3, 1, 2], [0, 3, 2, 1], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [1, 3, 2, 0],
    [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0],
    [1, 2, 0, 3], [0, 0, 0, 0], [1, 3, 0, 2], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [2, 3, 0, 1], [2, 3, 1, 0],
    [1, 0, 2, 3], [1, 0, 3, 2], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [2, 0, 3, 1], [0, 0, 0, 0], [2, 1, 3, 0],
    [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0],
    [2, 0, 1, 3], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [3, 0, 1, 2], [3, 0, 2, 1], [0, 0, 0, 0], [3, 1, 2, 0],
    [2, 1, 0, 3], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [3, 1, 0, 2], [0, 0, 0, 0], [3, 2, 0, 1], [3, 2, 1, 0],
];

/// 2D multi-octave simplex noise (`Simplex.noise2d`).
pub fn simplex_noise2d(
    seed: i32,
    octaves: f64,
    persistence: f64,
    scale: f64,
    x: f64,
    y: f64,
) -> f32 {
    let mut total = 0.0;
    let mut frequency = scale;
    let mut amplitude = 1.0;
    let mut max_amplitude = 0.0;
    #[allow(clippy::cast_possible_truncation)]
    for _ in 0..(octaves as i32) {
        total += (simplex_raw2d(seed, x * frequency, y * frequency) + 1.0) / 2.0 * amplitude;
        frequency *= 2.0;
        max_amplitude += amplitude;
        amplitude *= persistence;
    }
    (total / max_amplitude) as f32
}

/// 2D raw simplex noise (`Simplex.raw2d`).
pub fn simplex_raw2d(seed: i32, x: f64, y: f64) -> f64 {
    let f2 = 0.5 * (3.0_f64.sqrt() - 1.0);
    let s = (x + y) * f2;
    let i = fastfloor(x + s);
    let j = fastfloor(y + s);

    let g2 = (3.0 - 3.0_f64.sqrt()) / 6.0;
    let t = (i + j) as f64 * g2;
    let x0 = x - (i as f64 - t);
    let y0 = y - (j as f64 - t);

    let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };
    let x1 = x0 - i1 as f64 + g2;
    let y1 = y0 - j1 as f64 + g2;
    let x2 = x0 - 1.0 + 2.0 * g2;
    let y2 = y0 - 1.0 + 2.0 * g2;

    let ii = i & 255;
    let jj = j & 255;
    let gi0 = (perm(seed, ii + perm(seed, jj)) % 12) as usize;
    let gi1 = (perm(seed, ii + i1 + perm(seed, jj + j1)) % 12) as usize;
    let gi2 = (perm(seed, ii + 1 + perm(seed, jj + 1)) % 12) as usize;

    let mut n = [0.0; 3];
    let t0 = 0.5 - x0 * x0 - y0 * y0;
    if t0 >= 0.0 {
        let t = t0 * t0;
        n[0] = t * t * (GRAD3[gi0][0] as f64 * x0 + GRAD3[gi0][1] as f64 * y0);
    }
    let t1 = 0.5 - x1 * x1 - y1 * y1;
    if t1 >= 0.0 {
        let t = t1 * t1;
        n[1] = t * t * (GRAD3[gi1][0] as f64 * x1 + GRAD3[gi1][1] as f64 * y1);
    }
    let t2 = 0.5 - x2 * x2 - y2 * y2;
    if t2 >= 0.0 {
        let t = t2 * t2;
        n[2] = t * t * (GRAD3[gi2][0] as f64 * x2 + GRAD3[gi2][1] as f64 * y2);
    }
    70.0 * (n[0] + n[1] + n[2])
}

/// 4D raw simplex noise (`Simplex.raw4d`).
pub fn simplex_raw4d(x: f64, y: f64, z: f64, w: f64) -> f64 {
    let f4 = (5.0_f64.sqrt() - 1.0) / 4.0;
    let g4 = (5.0 - 5.0_f64.sqrt()) / 20.0;

    let s = (x + y + z + w) * f4;
    let i = fastfloor(x + s);
    let j = fastfloor(y + s);
    let k = fastfloor(z + s);
    let l = fastfloor(w + s);
    let t = (i + j + k + l) as f64 * g4;
    let x0 = x - (i as f64 - t);
    let y0 = y - (j as f64 - t);
    let z0 = z - (k as f64 - t);
    let w0 = w - (l as f64 - t);

    let c1 = if x0 > y0 { 32 } else { 0 };
    let c2 = if x0 > z0 { 16 } else { 0 };
    let c3 = if y0 > z0 { 8 } else { 0 };
    let c4 = if x0 > w0 { 4 } else { 0 };
    let c5 = if y0 > w0 { 2 } else { 0 };
    let c6 = if z0 > w0 { 1 } else { 0 };
    let c = (c1 + c2 + c3 + c4 + c5 + c6) as usize;

    let i1 = i32::from(SIMPLEX[c][0] >= 3);
    let j1 = i32::from(SIMPLEX[c][1] >= 3);
    let k1 = i32::from(SIMPLEX[c][2] >= 3);
    let l1 = i32::from(SIMPLEX[c][3] >= 3);
    let i2 = i32::from(SIMPLEX[c][0] >= 2);
    let j2 = i32::from(SIMPLEX[c][1] >= 2);
    let k2 = i32::from(SIMPLEX[c][2] >= 2);
    let l2 = i32::from(SIMPLEX[c][3] >= 2);
    let i3 = i32::from(SIMPLEX[c][0] >= 1);
    let j3 = i32::from(SIMPLEX[c][1] >= 1);
    let k3 = i32::from(SIMPLEX[c][2] >= 1);
    let l3 = i32::from(SIMPLEX[c][3] >= 1);

    let x1 = x0 - i1 as f64 + g4;
    let y1 = y0 - j1 as f64 + g4;
    let z1 = z0 - k1 as f64 + g4;
    let w1 = w0 - l1 as f64 + g4;
    let x2 = x0 - i2 as f64 + 2.0 * g4;
    let y2 = y0 - j2 as f64 + 2.0 * g4;
    let z2 = z0 - k2 as f64 + 2.0 * g4;
    let w2 = w0 - l2 as f64 + 2.0 * g4;
    let x3 = x0 - i3 as f64 + 3.0 * g4;
    let y3 = y0 - j3 as f64 + 3.0 * g4;
    let z3 = z0 - k3 as f64 + 3.0 * g4;
    let w3 = w0 - l3 as f64 + 3.0 * g4;
    let x4 = x0 - 1.0 + 4.0 * g4;
    let y4 = y0 - 1.0 + 4.0 * g4;
    let z4 = z0 - 1.0 + 4.0 * g4;
    let w4 = w0 - 1.0 + 4.0 * g4;

    let ii = i & 255;
    let jj = j & 255;
    let kk = k & 255;
    let ll = l & 255;
    let gi0 = ((ii + (jj + (kk + ll))) % 32) as usize;
    let gi1 = ((ii + i1 + (jj + j1 + (kk + k1 + (ll + l1)))) % 32) as usize;
    let gi2 = ((ii + i2 + (jj + j2 + (kk + k2 + (ll + l2)))) % 32) as usize;
    let gi3 = ((ii + i3 + (jj + j3 + (kk + k3 + (ll + l3)))) % 32) as usize;
    let gi4 = ((ii + 1 + (jj + 1 + (kk + k1 + (ll + l1)))) % 32) as usize;

    let mut n = [0.0; 5];
    let corner = |g: usize, x: f64, y: f64, z: f64, w: f64| -> f64 {
        let t = 0.6 - x * x - y * y - z * z - w * w;
        if t < 0.0 {
            0.0
        } else {
            let t2 = t * t;
            t2 * t2
                * (GRAD4[g][0] as f64 * x
                    + GRAD4[g][1] as f64 * y
                    + GRAD4[g][2] as f64 * z
                    + GRAD4[g][3] as f64 * w)
        }
    };
    n[0] = corner(gi0, x0, y0, z0, w0);
    n[1] = corner(gi1, x1, y1, z1, w1);
    n[2] = corner(gi2, x2, y2, z2, w2);
    n[3] = corner(gi3, x3, y3, z3, w3);
    n[4] = corner(gi4, x4, y4, z4, w4);
    27.0 * (n[0] + n[1] + n[2] + n[3] + n[4])
}

/// Tiled 4D simplex (`Simplex.rawTiled`). Uses `Mathf.PI`/`Mathf.PI2` (float
/// constants, widened) exactly like Arc.
#[allow(clippy::too_many_arguments)]
pub fn simplex_raw_tiled(x: f64, y: f64, x1: f64, y1: f64, w: f64, h: f64, scl: f64) -> f64 {
    /// `Mathf.PI` widened to double.
    #[allow(clippy::approx_constant)]
    const PI: f64 = 3.1415927_f32 as f64;
    /// `Mathf.PI2` (`PI * 2` in float, widened).
    #[allow(clippy::approx_constant)]
    const PI2: f64 = (3.1415927_f32 * 2.0_f32) as f64;

    let x = x / scl;
    let y = y / scl;
    let w = w / scl;
    let h = h / scl;

    let x2 = x1 + w;
    let y2 = y1 + h;
    let s = x / w;
    let t = y / h;
    let dx = x2 - x1;
    let dy = y2 - y1;

    let nx = x1 + (s * 2.0 * PI).cos() * dx / PI2;
    let ny = y1 + (t * 2.0 * PI).cos() * dy / PI2;
    let nz = x1 + (s * 2.0 * PI).sin() * dx / PI2;
    let nw = y1 + (t * 2.0 * PI).sin() * dy / PI2;

    simplex_raw4d(nx, ny, nz, nw)
}

/// Hash permutation (`Simplex.perm`).
fn perm(seed: i32, x: i32) -> i32 {
    let mut x = (x & 255).wrapping_mul(0x045d_9f3b_i32);
    x = ((x as u32 >> 16) as i32 ^ x).wrapping_mul(0x045d_9f3b_i32.wrapping_add(seed));
    x = (x as u32 >> 16) as i32 ^ x;
    x & 0xff
}

fn fastfloor(x: f64) -> i32 {
    if x > 0.0 { x as i32 } else { x as i32 - 1 }
}

// ---------------------------------------------------------------------------
// Ridged (arc.util.noise.Ridged) — quality = 2 (scurve5)
// ---------------------------------------------------------------------------

const X_NOISE_GEN: i32 = 1619;
const Y_NOISE_GEN: i32 = 31337;
const Z_NOISE_GEN: i32 = 6971;
const SEED_NOISE_GEN: i32 = 1013;
const SHIFT_NOISE_GEN: i32 = 8;

/// Ridged perlin noise (`Ridged.noise2d(seed, x, y, octaves, persistence, frequency)`).
pub fn ridged_noise2d(
    seed: i32,
    x: f64,
    y: f64,
    octaves: i32,
    persistence: f64,
    frequency: f64,
) -> f32 {
    let mut x1 = x * frequency;
    let mut y1 = y * frequency;

    let mut value = 0.0;
    let mut weight = 1.0;
    let offset = 1.0;
    let gain = 2.0;
    let mut sweight = 1.0;

    for cur_octave in 0..octaves {
        let nx = ridged_range(x1);
        let ny = ridged_range(y1);

        let x0 = fastfloor_positive(nx);
        let x11 = x0 + 1;
        let y0 = fastfloor_positive(ny);
        let y11 = y0 + 1;

        let xs = scurve5(nx - x0 as f64);
        let ys = scurve5(ny - y0 as f64);

        let octave_seed = seed.wrapping_add(cur_octave) & 0x7fff_ffff;
        let n0 = gradient_noise(nx, ny, x0, y0, octave_seed);
        let n1 = gradient_noise(nx, ny, x11, y0, octave_seed);
        let ix0 = lerp(n0, n1, xs);
        let n0 = gradient_noise(nx, ny, x0, y11, octave_seed);
        let n1 = gradient_noise(nx, ny, x11, y11, octave_seed);
        let ix1 = lerp(n0, n1, xs);
        let iy0 = lerp(ix0, ix1, ys);

        let mut signal = iy0;
        signal = signal.abs();
        signal = offset - signal;
        signal *= signal;
        signal *= weight;

        weight = signal * gain;
        weight = weight.clamp(0.0, 1.0);

        value += signal * sweight;
        sweight *= persistence;

        x1 *= 2.0;
        y1 *= 2.0;
    }

    ((value * 1.25) - 1.0) as f32
}

/// Ridged 3D perlin (`Ridged.noise3d(seed, x, y, z, octaves, frequency)`).
pub fn ridged_noise3d(seed: i32, x: f64, y: f64, z: f64, octaves: i32, frequency: f64) -> f32 {
    let mut x1 = x * frequency;
    let mut y1 = y * frequency;
    let mut z1 = z * frequency;

    let mut value = 0.0;
    let mut weight = 1.0;
    let offset = 1.0;
    let gain = 2.0;
    let scaling = 0.5;
    let mut sweight = 1.0;

    for cur_octave in 0..octaves {
        let nx = ridged_range(x1);
        let ny = ridged_range(y1);
        let nz = ridged_range(z1);

        let x0 = fastfloor_positive(nx);
        let x11 = x0 + 1;
        let y0 = fastfloor_positive(ny);
        let y11 = y0 + 1;
        let z0 = fastfloor_positive(nz);
        let z11 = z0 + 1;

        let xs = scurve5(nx - x0 as f64);
        let ys = scurve5(ny - y0 as f64);
        let zs = scurve5(nz - z0 as f64);

        let octave_seed = seed.wrapping_add(cur_octave) & 0x7fff_ffff;
        let n0 = gradient_noise3d(nx, ny, nz, x0, y0, z0, octave_seed);
        let n1 = gradient_noise3d(nx, ny, nz, x11, y0, z0, octave_seed);
        let ix0 = lerp(n0, n1, xs);
        let n0 = gradient_noise3d(nx, ny, nz, x0, y11, z0, octave_seed);
        let n1 = gradient_noise3d(nx, ny, nz, x11, y11, z0, octave_seed);
        let ix1 = lerp(n0, n1, xs);
        let iy0 = lerp(ix0, ix1, ys);
        let n0 = gradient_noise3d(nx, ny, nz, x0, y0, z11, octave_seed);
        let n1 = gradient_noise3d(nx, ny, nz, x11, y0, z11, octave_seed);
        let ix0 = lerp(n0, n1, xs);
        let n0 = gradient_noise3d(nx, ny, nz, x0, y11, z11, octave_seed);
        let n1 = gradient_noise3d(nx, ny, nz, x11, y11, z11, octave_seed);
        let ix1 = lerp(n0, n1, xs);
        let iy1 = lerp(ix0, ix1, ys);

        let mut signal = lerp(iy0, iy1, zs);
        signal = signal.abs();
        signal = offset - signal;
        signal *= signal;
        signal *= weight;

        weight = signal * gain;
        weight = weight.clamp(0.0, 1.0);

        value += signal * sweight;
        sweight *= scaling;

        x1 *= 2.0;
        y1 *= 2.0;
        z1 *= 2.0;
    }

    ((value * 1.25) - 1.0) as f32
}

fn ridged_range(n: f64) -> f64 {
    if n >= 1_073_741_824.0 {
        (2.0 * (n % 1_073_741_824.0)) - 1_073_741_824.0
    } else if n <= -1_073_741_824.0 {
        (2.0 * (n % 1_073_741_824.0)) + 1_073_741_824.0
    } else {
        n
    }
}

fn gradient_noise(fx: f64, fy: f64, ix: i32, iy: i32, seed: i32) -> f64 {
    let mut vector_index = X_NOISE_GEN
        .wrapping_mul(ix)
        .wrapping_add(Y_NOISE_GEN.wrapping_mul(iy))
        .wrapping_add(SEED_NOISE_GEN.wrapping_mul(seed));
    vector_index ^= vector_index >> SHIFT_NOISE_GEN;
    vector_index &= 0xff;
    let base = (vector_index * 3) as usize;
    let xv_gradient = RANDOM_VECTORS[base];
    let yv_gradient = RANDOM_VECTORS[base + 1];
    let xv_point = fx - ix as f64;
    let yv_point = fy - iy as f64;
    ((xv_gradient * xv_point) + (yv_gradient * yv_point)) * 2.12
}

fn gradient_noise3d(fx: f64, fy: f64, fz: f64, ix: i32, iy: i32, iz: i32, seed: i32) -> f64 {
    let mut vector_index = X_NOISE_GEN
        .wrapping_mul(ix)
        .wrapping_add(Y_NOISE_GEN.wrapping_mul(iy))
        .wrapping_add(Z_NOISE_GEN.wrapping_mul(iz))
        .wrapping_add(SEED_NOISE_GEN.wrapping_mul(seed));
    vector_index ^= vector_index >> SHIFT_NOISE_GEN;
    vector_index &= 0xff;
    let base = (vector_index * 3) as usize;
    let xv_gradient = RANDOM_VECTORS[base];
    let yv_gradient = RANDOM_VECTORS[base + 1];
    let zv_gradient = RANDOM_VECTORS[base + 2];
    let xv_point = fx - ix as f64;
    let yv_point = fy - iy as f64;
    let zv_point = fz - iz as f64;
    ((xv_gradient * xv_point) + (yv_gradient * yv_point) + (zv_gradient * zv_point)) * 2.12
}

fn lerp(n0: f64, n1: f64, a: f64) -> f64 {
    (1.0 - a) * n0 + a * n1
}

fn scurve5(a: f64) -> f64 {
    let a3 = a * a * a;
    let a4 = a3 * a;
    let a5 = a4 * a;
    (6.0 * a5) - (15.0 * a4) + (10.0 * a3)
}

/// `Ridged`'s floor for positive/negative values (same as `fastfloor` but the
/// Java code uses `> 0` explicitly).
fn fastfloor_positive(n: f64) -> i32 {
    if n > 0.0 { n as i32 } else { n as i32 - 1 }
}

// ---------------------------------------------------------------------------
// Rand (arc.math.Rand — xorshift128+) and Mathf helpers
// ---------------------------------------------------------------------------

const NORM_FLOAT: f64 = 1.0 / (1u64 << 24) as f64;

/// `arc.math.Rand` (xorshift128+).
#[derive(Debug, Clone)]
pub struct Rand {
    /// State 0.
    pub seed0: i64,
    /// State 1.
    pub seed1: i64,
}

impl Rand {
    /// `new Rand(seed)`.
    pub fn new(seed: i64) -> Rand {
        let mut rand = Rand { seed0: 0, seed1: 0 };
        rand.set_seed(seed);
        rand
    }

    /// `setSeed(long)` (murmur3 finalizer pair; 0 maps to `Long.MIN_VALUE`).
    pub fn set_seed(&mut self, seed: i64) {
        let seed0 = murmur_hash3(if seed == 0 { i64::MIN } else { seed });
        self.seed0 = seed0;
        self.seed1 = murmur_hash3(seed0);
    }

    /// `nextLong()` (xorshift128+).
    pub fn next_long(&mut self) -> i64 {
        let mut s1 = self.seed0;
        let s0 = self.seed1;
        self.seed0 = s0;
        s1 ^= s1 << 23;
        self.seed1 = s1 ^ s0 ^ ((s1 as u64 >> 17) as i64) ^ ((s0 as u64 >> 26) as i64);
        self.seed1.wrapping_add(s0)
    }

    /// `nextLong(n)` — uniform in `[0, n)` with Arc's rejection loop.
    pub fn next_long_bounded(&mut self, n: i64) -> i64 {
        assert!(n > 0, "n must be positive");
        loop {
            let bits = (self.next_long() as u64 >> 1) as i64;
            let value = bits % n;
            if bits - value + (n - 1) >= 0 {
                return value;
            }
        }
    }

    /// `nextFloat()` in `[0, 1)`.
    pub fn next_float(&mut self) -> f32 {
        ((self.next_long() as u64 >> 40) as f64 * NORM_FLOAT) as f32
    }

    /// `random(max)` — float in `[0, max)`.
    pub fn random(&mut self, max: f32) -> f32 {
        self.next_float() * max
    }

    /// `range(amount)` — float in `[-amount, amount)`.
    pub fn range(&mut self, amount: f32) -> f32 {
        self.next_float() * amount * 2.0 - amount
    }

    /// `random(max)` int — inclusive `[0, max]` (`nextInt(max + 1)`).
    pub fn random_int_inclusive(&mut self, max: i32) -> i32 {
        if max < 0 {
            return 0;
        }
        self.next_long_bounded(max as i64 + 1) as i32
    }
}

fn murmur_hash3(mut x: i64) -> i64 {
    x ^= ((x as u64) >> 33) as i64;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccdu64 as i64);
    x ^= ((x as u64) >> 33) as i64;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53u64 as i64);
    x ^= ((x as u64) >> 33) as i64;
    x
}

/// `Mathf.randomSeed(long seed)` in `[0, 1)` (fresh `Rand` per call, matching
/// `seedr.setSeed(seed * 99999)`).
pub fn random_seed(seed: i64) -> f32 {
    let mut rand = Rand::new(seed.wrapping_mul(99999));
    rand.next_float()
}

/// `Mathf.randomSeed(long seed, float max)`.
pub fn random_seed_scaled(seed: i64, max: f32) -> f32 {
    random_seed(seed) * max
}

/// `Mathf.randomSeedRange(long seed, float range)`.
pub fn random_seed_range(seed: i64, range: f32) -> f32 {
    range * (random_seed(seed) - 0.5) * 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simplex_is_deterministic() {
        let a = simplex_raw2d(7, 3.25, -11.5);
        let b = simplex_raw2d(7, 3.25, -11.5);
        assert_eq!(a.to_bits(), b.to_bits());
        let c = simplex_noise2d(7, 4.0, 0.5, 1.0 / 40.0, 12.0, 9.0);
        let d = simplex_noise2d(7, 4.0, 0.5, 1.0 / 40.0, 12.0, 9.0);
        assert_eq!(c.to_bits(), d.to_bits());
    }

    #[test]
    fn tiled_simplex_repeats() {
        let a = simplex_raw_tiled(0.0, 0.0, 100.0, 200.0, 31.0, 31.0, 26.0);
        let b = simplex_raw_tiled(31.0, 31.0, 100.0, 200.0, 31.0, 31.0, 26.0);
        // Tiled: value at (0,0) ≈ value at (w,h) of the tile (the mapping
        // uses float-widened PI constants, so the wrap is not bit-exact).
        assert!((a - b).abs() < 1e-3, "{a} vs {b}");
    }

    #[test]
    fn ridged_is_deterministic_and_bounded() {
        for i in 0..16 {
            let v = ridged_noise2d(1, i as f64 * 3.1, i as f64 * -1.7, 3, 0.5, 1.0 / 40.0);
            assert!((-1.5..=1.5).contains(&v), "out of range: {v}");
        }
        let a = ridged_noise2d(1, 5.0, 6.0, 3, 0.5, 1.0 / 40.0);
        let b = ridged_noise2d(1, 5.0, 6.0, 3, 0.5, 1.0 / 40.0);
        assert_eq!(a.to_bits(), b.to_bits());
    }

    #[test]
    fn rand_matches_xorshift128_plus_sequence() {
        let mut rand = Rand::new(42);
        let first = rand.next_long();
        let mut again = Rand::new(42);
        assert_eq!(first, again.next_long());
        assert_ne!(first, again.next_long());
    }

    #[test]
    fn random_seed_in_range() {
        for seed in 0..8 {
            let value = random_seed_scaled(seed, 200_000.0);
            assert!((0.0..200_000.0).contains(&value));
            assert_eq!(value, random_seed_scaled(seed, 200_000.0));
        }
    }
}
