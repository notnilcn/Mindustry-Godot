// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `arc.util.noise.Simplex` port (plan 06 §3.10).
//!
//! Arc's `Simplex` is stateless: `perm(seed, x)` is a deterministic integer hash,
//! so the port reproduces the JVM algorithm exactly for 2D/3D (deviation §2.3.6:
//! structural/reference parity; Rust↔Rust determinism). `Ridged` is deferred to
//! M8 (only the vanilla planet generators need it) with a clear hand-off.

/// 12 gradient vectors used by `raw2d`/`raw3d` (`Simplex.grad3`).
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

/// Arc `Simplex.perm`: a stateless permutation hash. From the jar bytecode
/// (`javap -c arc.util.noise.Simplex`): `x = (x & 255) * 0x45d9f3b;
/// x = ((x >>> 16) ^ x) * (0x45d9f3b + seed); x = (x >>> 16) ^ x;
/// return x & 255` - the input is masked first and both shifts are unsigned
/// (the port had neither, so every generated terrain differed from Java;
/// PAR-0424).
#[inline]
fn perm(seed: i32, x: i32) -> i32 {
    let mut x = (x & 0xff).wrapping_mul(0x45d9f3b);
    x = (((x as u32) >> 16) as i32 ^ x).wrapping_mul(0x45d9f3b_i32.wrapping_add(seed));
    x = ((x as u32) >> 16) as i32 ^ x;
    x & 0xff
}

#[inline]
fn fastfloor(x: f64) -> i32 {
    if x > 0.0 { x as i32 } else { x as i32 - 1 }
}

#[inline]
fn dot2(g: [i32; 3], x: f64, y: f64) -> f64 {
    g[0] as f64 * x + g[1] as f64 * y
}

#[inline]
fn dot3(g: [i32; 3], x: f64, y: f64, z: f64) -> f64 {
    g[0] as f64 * x + g[1] as f64 * y + g[2] as f64 * z
}

/// `Simplex.noise2d`: multi-octave 2D simplex noise normalized to `[0, 1]`.
pub fn noise2d(seed: i32, octaves: i32, persistence: f64, scale: f64, x: f64, y: f64) -> f32 {
    let mut total = 0.0;
    let mut frequency = scale;
    let mut amplitude = 1.0;
    let mut max_amplitude = 0.0;
    for _ in 0..octaves {
        total += (raw2d(seed, x * frequency, y * frequency) + 1.0) / 2.0 * amplitude;
        frequency *= 2.0;
        max_amplitude += amplitude;
        amplitude *= persistence;
    }
    if max_amplitude == 0.0 {
        return 0.0;
    }
    (total / max_amplitude) as f32
}

/// `Simplex.noise3d`: multi-octave 3D simplex noise normalized to `[0, 1]`.
pub fn noise3d(
    seed: i32,
    octaves: i32,
    persistence: f64,
    scale: f64,
    x: f64,
    y: f64,
    z: f64,
) -> f32 {
    let mut total = 0.0;
    let mut frequency = scale;
    let mut amplitude = 1.0;
    let mut max_amplitude = 0.0;
    for _ in 0..octaves {
        total += (raw3d(seed, x * frequency, y * frequency, z * frequency) + 1.0) / 2.0 * amplitude;
        frequency *= 2.0;
        max_amplitude += amplitude;
        amplitude *= persistence;
    }
    if max_amplitude == 0.0 {
        return 0.0;
    }
    (total / max_amplitude) as f32
}

/// Arc `Noise` 1-D Perlin tables (`arc/util/noise/Noise.java:222-262`, seed
/// 100). Only `p` and `g1` are needed by `rawNoise`.
struct ArcPerlin1 {
    p: [usize; 514],
    g1: [f64; 514],
}

static ARC_PERLIN1: std::sync::OnceLock<ArcPerlin1> = std::sync::OnceLock::new();

fn arc_perlin1() -> &'static ArcPerlin1 {
    ARC_PERLIN1.get_or_init(|| {
        const B: usize = 256;
        const M: i64 = 255;
        let mut r = crate::random::JavaRandom::new(100);
        let mut p = [0usize; 514];
        let mut g1 = [0.0f64; 514];
        for i in 0..B {
            p[i] = i;
            g1[i] = 2.0 * r.next_double() - 1.0;
            // The 2-D and 3-D rejection loops' values are discarded, but the
            // draws shift the `java.util.Random` stream (same positions as
            // upstream).
            loop {
                let u = 2.0 * r.next_double() - 1.0;
                let v = 2.0 * r.next_double() - 1.0;
                if !(u * u + v * v > 1.0
                    || u.abs() > 2.5 * v.abs()
                    || v.abs() > 2.5 * u.abs()
                    || (u.abs() - v.abs()).abs() < 0.4)
                {
                    break;
                }
            }
            loop {
                let u = 2.0 * r.next_double() - 1.0;
                let v = 2.0 * r.next_double() - 1.0;
                let w = 2.0 * r.next_double() - 1.0;
                let (au, av, aw) = (u.abs(), v.abs(), w.abs());
                let lo = au.min(av).min(aw);
                let hi = au.max(av).max(aw);
                if !(u * u + v * v + w * w > 1.0
                    || hi > 4.0 * lo
                    || (au - av).abs().min((au - aw).abs()).min((av - aw).abs()) < 0.2)
                {
                    break;
                }
            }
        }
        // `while(--i > 0){ k = p[i]; j = (int)(r.nextLong() & M); ... }`:
        // bodies run for i = 255..=1.
        let mut i = B;
        while i > 1 {
            i -= 1;
            let k = p[i];
            let j = (r.next_long() & M) as usize;
            p[i] = p[j];
            p[j] = k;
        }
        for i in 0..B + 2 {
            p[B + i] = p[i];
            g1[B + i] = g1[i];
        }
        ArcPerlin1 { p, g1 }
    })
}

/// Arc `Noise.rawNoise(double)` (`arc/util/noise/Noise.java:30-45`): the 1-D
/// Perlin value Mindustry's weather sound oscillator reads. `Noise.setSeed` is
/// never called upstream, so seed 100 is fixed.
pub fn raw_noise_1d(x: f64) -> f64 {
    const M: i64 = 255;
    const N: f64 = 256.0;
    let table = arc_perlin1();
    let t = x + N;
    let ti = t as i32;
    let bx0 = (i64::from(ti) & M) as usize;
    let bx1 = ((i64::from(ti) + 1) & M) as usize;
    let rx0 = t - f64::from(ti);
    let rx1 = rx0 - 1.0;
    let sx = rx0 * rx0 * (3.0 - 2.0 * rx0);
    let u = rx0 * table.g1[table.p[bx0]];
    let v = rx1 * table.g1[table.p[bx1]];
    u + sx * (v - u)
}

/// Test-only table probe: `([p[0..4]], g1[0])` for the reference test.
#[cfg(test)]
pub(crate) fn arc_perlin1_probe() -> ([usize; 4], f64) {
    let table = arc_perlin1();
    (
        [table.p[0], table.p[1], table.p[2], table.p[3]],
        table.g1[0],
    )
}

/// `Simplex.raw2d`: single-octave 2D simplex noise in `[-1, 1]`.
pub fn raw2d(seed: i32, x: f64, y: f64) -> f64 {
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
    let gi0 = perm(seed, ii + perm(seed, jj)) % 12;
    let gi1 = perm(seed, ii + i1 + perm(seed, jj + j1)) % 12;
    let gi2 = perm(seed, ii + 1 + perm(seed, jj + 1)) % 12;

    let t0 = 0.5 - x0 * x0 - y0 * y0;
    let n0 = if t0 < 0.0 {
        0.0
    } else {
        let t0 = t0 * t0;
        t0 * t0 * dot2(GRAD3[gi0 as usize], x0, y0)
    };
    let t1 = 0.5 - x1 * x1 - y1 * y1;
    let n1 = if t1 < 0.0 {
        0.0
    } else {
        let t1 = t1 * t1;
        t1 * t1 * dot2(GRAD3[gi1 as usize], x1, y1)
    };
    let t2 = 0.5 - x2 * x2 - y2 * y2;
    let n2 = if t2 < 0.0 {
        0.0
    } else {
        let t2 = t2 * t2;
        t2 * t2 * dot2(GRAD3[gi2 as usize], x2, y2)
    };

    70.0 * (n0 + n1 + n2)
}

/// `Simplex.raw3d`: single-octave 3D simplex noise in `[-1, 1]`.
pub fn raw3d(seed: i32, x: f64, y: f64, z: f64) -> f64 {
    let f3 = 1.0 / 3.0;
    let s = (x + y + z) * f3;
    let i = fastfloor(x + s);
    let j = fastfloor(y + s);
    let k = fastfloor(z + s);
    let g3 = 1.0 / 6.0;
    let t = (i + j + k) as f64 * g3;
    let x0 = x - (i as f64 - t);
    let y0 = y - (j as f64 - t);
    let z0 = z - (k as f64 - t);

    let (i1, j1, k1, i2, j2, k2) = if x0 >= y0 {
        if y0 >= z0 {
            (1, 0, 0, 1, 1, 0)
        } else if x0 >= z0 {
            (1, 0, 0, 1, 0, 1)
        } else {
            (0, 0, 1, 1, 0, 1)
        }
    } else if y0 < z0 {
        (0, 0, 1, 0, 1, 1)
    } else if x0 < z0 {
        (0, 1, 0, 0, 1, 1)
    } else {
        (0, 1, 0, 1, 1, 0)
    };

    let x1 = x0 - i1 as f64 + g3;
    let y1 = y0 - j1 as f64 + g3;
    let z1 = z0 - k1 as f64 + g3;
    let x2 = x0 - i2 as f64 + 2.0 * g3;
    let y2 = y0 - j2 as f64 + 2.0 * g3;
    let z2 = z0 - k2 as f64 + 2.0 * g3;
    let x3 = x0 - 1.0 + 3.0 * g3;
    let y3 = y0 - 1.0 + 3.0 * g3;
    let z3 = z0 - 1.0 + 3.0 * g3;

    let ii = i & 255;
    let jj = j & 255;
    let kk = k & 255;
    let gi0 = perm(seed, ii + perm(seed, jj + perm(seed, kk))) % 12;
    let gi1 = perm(seed, ii + i1 + perm(seed, jj + j1 + perm(seed, kk + k1))) % 12;
    let gi2 = perm(seed, ii + i2 + perm(seed, jj + j2 + perm(seed, kk + k2))) % 12;
    let gi3 = perm(seed, ii + 1 + perm(seed, jj + 1 + perm(seed, kk + 1))) % 12;

    let t0 = 0.6 - x0 * x0 - y0 * y0 - z0 * z0;
    let n0 = if t0 < 0.0 {
        0.0
    } else {
        let t0 = t0 * t0;
        t0 * t0 * dot3(GRAD3[gi0 as usize], x0, y0, z0)
    };
    let t1 = 0.6 - x1 * x1 - y1 * y1 - z1 * z1;
    let n1 = if t1 < 0.0 {
        0.0
    } else {
        let t1 = t1 * t1;
        t1 * t1 * dot3(GRAD3[gi1 as usize], x1, y1, z1)
    };
    let t2 = 0.6 - x2 * x2 - y2 * y2 - z2 * z2;
    let n2 = if t2 < 0.0 {
        0.0
    } else {
        let t2 = t2 * t2;
        t2 * t2 * dot3(GRAD3[gi2 as usize], x2, y2, z2)
    };
    let t3 = 0.6 - x3 * x3 - y3 * y3 - z3 * z3;
    let n3 = if t3 < 0.0 {
        0.0
    } else {
        let t3 = t3 * t3;
        t3 * t3 * dot3(GRAD3[gi3 as usize], x3, y3, z3)
    };

    32.0 * (n0 + n1 + n2 + n3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_seeded_stable() {
        let a = noise2d(7, 4, 0.5, 1.0 / 24.0, 12.0, 34.0);
        let b = noise2d(7, 4, 0.5, 1.0 / 24.0, 12.0, 34.0);
        assert_eq!(a, b);
        let c = noise2d(8, 4, 0.5, 1.0 / 24.0, 12.0, 34.0);
        assert_ne!(a, c);
    }

    #[test]
    fn noise2d_is_bounded_and_not_constant() {
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for i in 0..64 {
            let v = noise2d(1, 3, 0.5, 1.0 / 32.0, i as f64, (i * 7 % 64) as f64);
            assert!((0.0..=1.0).contains(&v), "out of range: {v}");
            min = min.min(v);
            max = max.max(v);
        }
        assert!(max - min > 0.05, "noise looks constant: {min}..{max}");
    }

    #[test]
    fn raw3d_is_seeded_stable() {
        let a = raw3d(42, 0.3, 0.6, 0.9);
        let b = raw3d(42, 0.3, 0.6, 0.9);
        assert_eq!(a, b);
        assert!((-1.0..=1.0).contains(&a));
    }

    /// H07 T8: Arc `Noise.rawNoise` values from an independent Python port of
    /// `Noise.java` (seed 100, `java.util.Random` LCG re-implemented; **not** a
    /// JVM run — see the plan's reference table).
    #[test]
    fn arc_raw_noise_1d_matches_reference() {
        let (p, g1_0) = arc_perlin1_probe();
        assert!(
            (g1_0 - 0.4440193097192868).abs() < 1.0e-12,
            "g1[0] = {g1_0}"
        );
        assert_eq!(p, [66, 74, 50, 45], "p[0..4]");

        let cases = [
            (0.25, 0.12894795904560868),
            (0.5, 0.24520408728404314),
            (1.7, 0.10345653783232334),
            (1000.0 / 1100.0, 0.07698456651943808),
            (12.345, -0.028115906860696198),
        ];
        for (x, expected) in cases {
            let value = raw_noise_1d(x);
            assert!(
                (value - expected).abs() < 1.0e-12,
                "raw_noise_1d({x}) = {value}, expected {expected}"
            );
        }
    }

    /// Integer lattice points are zero for this Perlin variant.
    #[test]
    fn arc_raw_noise_1d_zero_at_integers() {
        for k in 0..10 {
            let value = raw_noise_1d(f64::from(k));
            assert!(value.abs() < 1.0e-12, "raw_noise_1d({k}) = {value}");
        }
    }
}
