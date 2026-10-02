// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `arc.math.Interp` curve table and `Mathf.curve` helpers (plan 17 §3.1).
//!
//! Ported from `Arc/arc-core/src/arc/math/Interp.java` and the `Mathf.curve`
//! overloads. Used by particle effects and `PartProgress` evaluation. Pure,
//! Godot-free, allocation-free.

/// An `Interp` curve. The subset upstream uses for effects/parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Interp {
    /// `Interp.linear`.
    #[default]
    Linear,
    /// `Interp.reverse`.
    Reverse,
    /// `Interp.smooth` (smoothstep).
    Smooth,
    /// `Interp.smooth2`.
    Smooth2,
    /// `Interp.one`.
    One,
    /// `Interp.zero`.
    Zero,
    /// `Interp.slope` (`Mathf.slope`).
    Slope,
    /// `Interp.smoother` / `Interp.fade`.
    Smoother,
    /// `Interp.pow2`.
    Pow2,
    /// `Interp.pow2In`.
    Pow2In,
    /// `Interp.pow2Out`.
    Pow2Out,
    /// `Interp.pow2InInverse`.
    Pow2InInverse,
    /// `Interp.pow2OutInverse`.
    Pow2OutInverse,
    /// `Interp.pow3`.
    Pow3,
    /// `Interp.pow3In`.
    Pow3In,
    /// `Interp.pow3Out`.
    Pow3Out,
    /// `Interp.pow3InInverse`.
    Pow3InInverse,
    /// `Interp.pow3OutInverse`.
    Pow3OutInverse,
    /// `Interp.pow4`.
    Pow4,
    /// `Interp.pow4In`.
    Pow4In,
    /// `Interp.pow4Out`.
    Pow4Out,
    /// `Interp.pow5`.
    Pow5,
    /// `Interp.pow5In`.
    Pow5In,
    /// `Interp.pow5Out`.
    Pow5Out,
    /// `Interp.pow10In`.
    Pow10In,
    /// `Interp.pow10Out`.
    Pow10Out,
    /// `Interp.sine`.
    Sine,
    /// `Interp.sineIn`.
    SineIn,
    /// `Interp.sineOut`.
    SineOut,
    /// `Interp.circle`.
    Circle,
    /// `Interp.circleIn`.
    CircleIn,
    /// `Interp.circleOut`.
    CircleOut,
    /// `Interp.exp5`.
    Exp5,
    /// `Interp.exp5In`.
    Exp5In,
    /// `Interp.exp5Out`.
    Exp5Out,
    /// `Interp.exp10`.
    Exp10,
    /// `Interp.exp10In`.
    Exp10In,
    /// `Interp.exp10Out`.
    Exp10Out,
}

impl Interp {
    /// `Interp.apply(float)`.
    #[inline]
    pub fn apply(self, a: f32) -> f32 {
        match self {
            Interp::Linear => a,
            Interp::Reverse => 1.0 - a,
            Interp::Smooth => a * a * (3.0 - 2.0 * a),
            Interp::Smooth2 => {
                let s = a * a * (3.0 - 2.0 * a);
                s * s * (3.0 - 2.0 * s)
            }
            Interp::One => 1.0,
            Interp::Zero => 0.0,
            Interp::Slope => 1.0 - (a - 0.5).abs() * 2.0,
            Interp::Smoother => a * a * a * (a * (a * 6.0 - 15.0) + 10.0),
            Interp::Pow2 => pow(a, 2.0),
            Interp::Pow2In => a.powf(2.0),
            Interp::Pow2Out => pow_out(a, 2.0),
            Interp::Pow2InInverse => a.sqrt(),
            Interp::Pow2OutInverse => 1.0 - (-(a - 1.0)).sqrt(),
            Interp::Pow3 => pow(a, 3.0),
            Interp::Pow3In => a.powf(3.0),
            Interp::Pow3Out => pow_out(a, 3.0),
            Interp::Pow3InInverse => a.cbrt(),
            Interp::Pow3OutInverse => 1.0 - (-(a - 1.0)).cbrt(),
            Interp::Pow4 => pow(a, 4.0),
            Interp::Pow4In => a.powf(4.0),
            Interp::Pow4Out => pow_out(a, 4.0),
            Interp::Pow5 => pow(a, 5.0),
            Interp::Pow5In => a.powf(5.0),
            Interp::Pow5Out => pow_out(a, 5.0),
            Interp::Pow10In => a.powf(10.0),
            Interp::Pow10Out => pow_out(a, 10.0),
            Interp::Sine => (1.0 - (a * std::f32::consts::PI).cos()) / 2.0,
            Interp::SineIn => 1.0 - (a * std::f32::consts::PI / 2.0).cos(),
            Interp::SineOut => (a * std::f32::consts::PI / 2.0).sin(),
            Interp::Circle => {
                if a <= 0.5 {
                    let t = a * 2.0;
                    (1.0 - (1.0 - t * t).sqrt()) / 2.0
                } else {
                    let t = (a - 1.0) * 2.0;
                    ((1.0 - t * t).sqrt() + 1.0) / 2.0
                }
            }
            Interp::CircleIn => 1.0 - (1.0 - a * a).sqrt(),
            Interp::CircleOut => {
                let t = a - 1.0;
                (1.0 - t * t).sqrt()
            }
            Interp::Exp5 => exp(a, 2.0, 5.0),
            Interp::Exp5In => exp_in(a, 2.0, 5.0),
            Interp::Exp5Out => exp_out(a, 2.0, 5.0),
            Interp::Exp10 => exp(a, 2.0, 10.0),
            Interp::Exp10In => exp_in(a, 2.0, 10.0),
            Interp::Exp10Out => exp_out(a, 2.0, 10.0),
        }
    }

    /// `Interp.apply(start, end, a)`.
    #[inline]
    pub fn apply_range(self, start: f32, end: f32, a: f32) -> f32 {
        start + (end - start) * self.apply(a)
    }
}

/// `Interp.Pow.apply`.
fn pow(a: f32, power: f32) -> f32 {
    if a <= 0.5 {
        (a * 2.0).powf(power) / 2.0
    } else {
        let denom = if (power as i64) % 2 == 0 { -2.0 } else { 2.0 };
        ((a - 1.0) * 2.0).powf(power) / denom + 1.0
    }
}

/// `Interp.PowOut.apply`.
fn pow_out(a: f32, power: f32) -> f32 {
    let sign = if (power as i64) % 2 == 0 { -1.0 } else { 1.0 };
    (a - 1.0).powf(power) * sign + 1.0
}

/// `Interp.Exp` (value `2`, given `power`).
fn exp(a: f32, value: f32, power: f32) -> f32 {
    let min = value.powf(-power);
    let scale = 1.0 / (1.0 - min);
    if a <= 0.5 {
        (value.powf(power * (a * 2.0 - 1.0)) - min) * scale / 2.0
    } else {
        (2.0 - (value.powf(-power * (a * 2.0 - 1.0)) - min) * scale) / 2.0
    }
}

/// `Interp.ExpIn`.
fn exp_in(a: f32, value: f32, power: f32) -> f32 {
    let min = value.powf(-power);
    let scale = 1.0 / (1.0 - min);
    (value.powf(power * (a - 1.0)) - min) * scale
}

/// `Interp.ExpOut`.
fn exp_out(a: f32, value: f32, power: f32) -> f32 {
    let min = value.powf(-power);
    let scale = 1.0 / (1.0 - min);
    1.0 - (value.powf(-power * a) - min) * scale
}

/// `Mathf.curve(f, offset)`.
#[inline]
pub fn curve_offset(f: f32, offset: f32) -> f32 {
    if f < offset {
        0.0
    } else {
        (f - offset) / (1.0 - offset)
    }
}

/// `Mathf.curve(f, from, to)`.
#[inline]
pub fn curve(f: f32, from: f32, to: f32) -> f32 {
    if f < from {
        0.0
    } else if f > to {
        1.0
    } else {
        (f - from) / (to - from)
    }
}

/// `Mathf.slope(fin)`.
#[inline]
pub fn slope(fin: f32) -> f32 {
    1.0 - (fin - 0.5).abs() * 2.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_and_reverse() {
        assert_eq!(Interp::Linear.apply(0.25), 0.25);
        assert_eq!(Interp::Reverse.apply(0.25), 0.75);
        assert_eq!(Interp::One.apply(0.25), 1.0);
        assert_eq!(Interp::Zero.apply(0.25), 0.0);
    }

    #[test]
    fn slope_is_zero_at_ends_one_in_middle() {
        assert!((Interp::Slope.apply(0.0) - 0.0).abs() < 1e-6);
        assert!((Interp::Slope.apply(1.0) - 0.0).abs() < 1e-6);
        assert!((Interp::Slope.apply(0.5) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn pow3_out_matches_finpow_semantics() {
        // Scaled.finpow() == Interp.pow3Out.apply(fin()).
        assert!((Interp::Pow3Out.apply(0.0)).abs() < 1e-6);
        assert!((Interp::Pow3Out.apply(1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn curve_clamps_outside_range() {
        assert_eq!(curve(0.1, 0.2, 1.0), 0.0);
        assert_eq!(curve(1.1, 0.2, 1.0), 1.0);
        assert!((curve(0.6, 0.2, 1.0) - 0.5).abs() < 1e-6);
        assert_eq!(curve_offset(0.5, 0.5), 0.0);
        assert!((curve_offset(0.75, 0.5) - 0.5).abs() < 1e-6);
    }
}
