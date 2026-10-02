// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Behavioral reimplementation of Arc (Apache-2.0, revision 7445105cd2):
// `arc-core/src/arc/math/Mathf.java` (sin table, `atan2` approximation) and
// `arc-core/src/arc/math/Angles.java` (the generator-facing subset).

//! `Mathf`/`Angles` subset used by the sprite generators (plan 03 A3): the
//! 16 KB sine lookup table and the RAND-1955 `atan2` approximation, ported
//! exactly so generator math tracks Arc.

use std::sync::OnceLock;

const SIN_BITS: i32 = 14;
const SIN_MASK: i32 = !(-1 << SIN_BITS);
const SIN_COUNT: usize = (SIN_MASK + 1) as usize;
const RAD_FULL: f32 = PI * 2.0;
const DEG_FULL: f32 = 360.0;
const RAD_TO_INDEX: f32 = SIN_COUNT as f32 / RAD_FULL;
const DEG_TO_INDEX: f32 = SIN_COUNT as f32 / DEG_FULL;

/// `Mathf.PI` (float; the Arc float-truncated value, parity ABI for table math).
#[allow(clippy::approx_constant)]
pub const PI: f32 = 3.1415927_f32;
/// `Mathf.halfPi`.
pub const HALF_PI: f32 = PI / 2.0;
/// `Mathf.PI2`.
pub const PI2: f32 = PI * 2.0;
/// `Mathf.radDeg` (`180f / PI`).
pub const RAD_DEG: f32 = 180.0 / PI;
/// `Mathf.degRad` (`PI / 180`).
pub const DEG_RAD: f32 = PI / 180.0;

fn sin_table() -> &'static [f32; SIN_COUNT] {
    static TABLE: OnceLock<[f32; SIN_COUNT]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0.0f32; SIN_COUNT];
        for (i, slot) in table.iter_mut().enumerate() {
            *slot = ((i as f32 + 0.5) / SIN_COUNT as f32 * RAD_FULL).sin();
        }
        for i in (0..360).step_by(90) {
            table[(i as f32 * DEG_TO_INDEX) as i32 as usize & SIN_MASK as usize] =
                (i as f32 * DEG_RAD).sin();
        }
        table[0] = 0.0;
        table[(90.0 * DEG_TO_INDEX) as i32 as usize & SIN_MASK as usize] = 1.0;
        table[(180.0 * DEG_TO_INDEX) as i32 as usize & SIN_MASK as usize] = 0.0;
        table[(270.0 * DEG_TO_INDEX) as i32 as usize & SIN_MASK as usize] = -1.0;
        table
    })
}

/// `Mathf.sin` (table lookup).
pub fn sin(radians: f32) -> f32 {
    sin_table()[((radians * RAD_TO_INDEX) as i32 & SIN_MASK) as usize]
}

/// `Mathf.cos` (table lookup).
pub fn cos(radians: f32) -> f32 {
    sin_table()[(((radians + PI / 2.0) * RAD_TO_INDEX) as i32 & SIN_MASK) as usize]
}

/// `Mathf.sinDeg` (table lookup).
pub fn sin_deg(degrees: f32) -> f32 {
    sin_table()[((degrees * DEG_TO_INDEX) as i32 & SIN_MASK) as usize]
}

/// `Mathf.cosDeg` (table lookup).
pub fn cos_deg(degrees: f32) -> f32 {
    sin_table()[(((degrees + 90.0) * DEG_TO_INDEX) as i32 & SIN_MASK) as usize]
}

/// `Mathf.atan2` — the RAND-1955 sheet-11 approximation (note the unusual
/// argument order: `x` first, then `y`).
pub fn atan2(mut x: f32, y: f32) -> f32 {
    let mut n = y / x;
    if n.is_nan() {
        n = if y == x { 1.0 } else { -1.0 };
    } else if n.is_infinite() {
        x = 0.0;
    }

    if x > 0.0 {
        atn(n as f64)
    } else if x < 0.0 {
        if y >= 0.0 {
            atn(n as f64) + PI
        } else {
            atn(n as f64) - PI
        }
    } else if y > 0.0 {
        x + HALF_PI
    } else if y < 0.0 {
        x - HALF_PI
    } else {
        x + y
    }
}

fn atn(i: f64) -> f32 {
    let n = i.abs();
    let c = (n - 1.0) / (n + 1.0);
    let c2 = c * c;
    let c3 = c * c2;
    let c5 = c3 * c2;
    let c7 = c5 * c2;
    let c9 = c7 * c2;
    let c11 = c9 * c2;
    ((std::f64::consts::PI * 0.25)
        + (0.99997726 * c - 0.33262347 * c3 + 0.19354346 * c5 - 0.11643287 * c7 + 0.05265332 * c9
            - 0.0117212 * c11))
        .copysign(i) as f32
}

/// `Angles.angle(x, y, x2, y2)` — degrees in `[0, 360)`.
pub fn angle(x: f32, y: f32, x2: f32, y2: f32) -> f32 {
    let mut ang = atan2(x2 - x, y2 - y) * RAD_DEG;
    if ang < 0.0 {
        ang += 360.0;
    }
    ang
}

/// `Angles.trnsx(angle, len)`.
pub fn trnsx(angle: f32, len: f32) -> f32 {
    len * cos_deg(angle)
}

/// `Angles.trnsy(angle, len)`.
pub fn trnsy(angle: f32, len: f32) -> f32 {
    len * sin_deg(angle)
}

/// `Mathf.mod(float, 360f)`-style positive modulo for angles.
pub fn mod360(value: f32) -> f32 {
    value.rem_euclid(360.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_cardinals_are_exact() {
        assert_eq!(sin_deg(0.0), 0.0);
        assert_eq!(sin_deg(90.0), 1.0);
        assert_eq!(sin_deg(180.0), 0.0);
        assert_eq!(sin_deg(270.0), -1.0);
        assert_eq!(cos_deg(0.0), 1.0);
    }

    #[test]
    fn atan2_quadrants() {
        assert!((atan2(1.0, 0.0)).abs() < 1e-5);
        assert!((atan2(0.0, 1.0) - HALF_PI).abs() < 1e-5);
        assert!((atan2(-1.0, 0.0).abs() - PI).abs() < 1e-5);
        assert!(atan2(1.0, 1.0) > 0.0);
    }

    #[test]
    fn angle_wraps() {
        // The atan2 approximation is not exact at the cardinals (±1e-4 deg).
        assert!((angle(0.0, 0.0, 1.0, 0.0)).abs() < 0.01);
        let a = angle(0.0, 0.0, 0.0, -1.0);
        assert!((a - 270.0).abs() < 0.01, "{a}");
        // Generator call shape: Angles.angle(x, y, cx, cy).
        let b = angle(10.0, 10.0, 5.0, 5.0);
        assert!((b - 225.0).abs() < 0.01, "{b}");
    }

    #[test]
    fn trns_matches() {
        assert!((trnsx(0.0, 5.0) - 5.0).abs() < 1e-5);
        assert!((trnsy(90.0, 5.0) - 5.0).abs() < 1e-5);
    }
}
