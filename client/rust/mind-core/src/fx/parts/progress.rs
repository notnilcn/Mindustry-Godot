// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DrawPart.PartProgress` + `CompatFix` combinators (plan 17 §3.7).
//!
//! Ported verbatim from `entities/part/DrawPart.java:90-296`. `Time.time` is
//! the fixed view tick (deviation #7).

use crate::math::{Interp, curve, slope};

use super::params::PartParams;

/// A binary `PartFunc` (`DrawPart.PartFunc`).
pub type PartFunc = fn(f32, f32) -> f32;

/// The data tree for a `PartProgress` expression.
#[allow(unpredictable_function_pointer_comparisons)]
#[derive(Clone, Debug, PartialEq, Default)]
pub enum PartProgressSpec {
    /// `PartProgress.reload`.
    Reload,
    /// `PartProgress.smoothReload`.
    SmoothReload,
    /// `PartProgress.warmup`.
    #[default]
    Warmup,
    /// `PartProgress.charge`.
    Charge,
    /// `PartProgress.recoil`.
    Recoil,
    /// `PartProgress.heat`.
    Heat,
    /// `PartProgress.life`.
    Life,
    /// `PartProgress.time`.
    Time,
    /// `PartProgress.constant(value)`.
    Constant(f32),
    /// `inv()`.
    Inv(Box<PartProgressSpec>),
    /// `slope()`.
    Slope(Box<PartProgressSpec>),
    /// `clamp()`.
    Clamp(Box<PartProgressSpec>),
    /// `add(other)`.
    Add(Box<PartProgressSpec>, Box<PartProgressSpec>),
    /// `add(amount)`.
    AddConst(Box<PartProgressSpec>, f32),
    /// `delay(amount)`.
    Delay(Box<PartProgressSpec>, f32),
    /// `curve(offset, duration)`.
    Curve(Box<PartProgressSpec>, f32, f32),
    /// `sustain(offset, grow, sustain)`.
    Sustain(Box<PartProgressSpec>, f32, f32, f32),
    /// `shorten(amount)`.
    Shorten(Box<PartProgressSpec>, f32),
    /// `compress(start, end)`.
    Compress(Box<PartProgressSpec>, f32, f32),
    /// `blend(other, amount)`.
    Blend(Box<PartProgressSpec>, Box<PartProgressSpec>, f32),
    /// `mul(other)`.
    Mul(Box<PartProgressSpec>, Box<PartProgressSpec>),
    /// `mul(amount)`.
    MulConst(Box<PartProgressSpec>, f32),
    /// `min(other)`.
    Min(Box<PartProgressSpec>, Box<PartProgressSpec>),
    /// `sin(offset, scl, mag)`.
    SinTime(Box<PartProgressSpec>, f32, f32, f32),
    /// `sin(scl, mag)`.
    SinScl(Box<PartProgressSpec>, f32, f32),
    /// `absin(scl, mag)`.
    Absin(Box<PartProgressSpec>, f32, f32),
    /// `mod(amount)`.
    Mod(Box<PartProgressSpec>, f32),
    /// `loop(time)`.
    Loop(Box<PartProgressSpec>, f32),
    /// `apply(other, func)`.
    Apply(Box<PartProgressSpec>, Box<PartProgressSpec>, PartFunc),
    /// `curve(interp)`.
    CurveInterp(Box<PartProgressSpec>, Interp),
}

/// `Mathf.mod(f, amount)`.
#[inline]
fn fmod(f: f32, amount: f32) -> f32 {
    if amount == 0.0 {
        0.0
    } else {
        f - (f / amount).floor() * amount
    }
}

/// `Mathf.sin(radians, scl, mag)`.
#[inline]
fn sin_scl_mag(radians: f32, scl: f32, mag: f32) -> f32 {
    (radians / scl).sin() * mag
}

impl PartProgressSpec {
    /// `PartProgress.get(PartParams)`; `time` replaces `Time.time`.
    pub fn get(&self, p: &PartParams, time: f32) -> f32 {
        match self {
            PartProgressSpec::Reload => p.reload,
            PartProgressSpec::SmoothReload => p.smooth_reload,
            PartProgressSpec::Warmup => p.warmup,
            PartProgressSpec::Charge => p.charge,
            PartProgressSpec::Recoil => p.recoil,
            PartProgressSpec::Heat => p.heat,
            PartProgressSpec::Life => p.life,
            PartProgressSpec::Time => time,
            PartProgressSpec::Constant(value) => *value,
            PartProgressSpec::Inv(inner) => 1.0 - inner.get(p, time),
            PartProgressSpec::Slope(inner) => slope(inner.get(p, time)),
            PartProgressSpec::Clamp(inner) => inner.get(p, time).clamp(0.0, 1.0),
            PartProgressSpec::Add(a, b) => a.get(p, time) + b.get(p, time),
            PartProgressSpec::AddConst(inner, amount) => inner.get(p, time) + amount,
            PartProgressSpec::Delay(inner, amount) => {
                (inner.get(p, time) - amount) / (1.0 - amount)
            }
            PartProgressSpec::Curve(inner, offset, duration) => {
                (inner.get(p, time) - offset) / duration
            }
            PartProgressSpec::Sustain(inner, offset, grow, sustain) => {
                let val = inner.get(p, time) - offset;
                (val.max(0.0) / grow).min((grow + sustain + grow - val) / grow)
            }
            PartProgressSpec::Shorten(inner, amount) => inner.get(p, time) / (1.0 - amount),
            PartProgressSpec::Compress(inner, start, end) => {
                curve(inner.get(p, time), *start, *end)
            }
            PartProgressSpec::Blend(a, b, amount) => lerp(a.get(p, time), b.get(p, time), *amount),
            PartProgressSpec::Mul(a, b) => a.get(p, time) * b.get(p, time),
            PartProgressSpec::MulConst(inner, amount) => inner.get(p, time) * amount,
            PartProgressSpec::Min(a, b) => a.get(p, time).min(b.get(p, time)),
            PartProgressSpec::SinTime(inner, offset, scl, mag) => {
                inner.get(p, time) + sin_scl_mag(time + offset, *scl, *mag)
            }
            PartProgressSpec::SinScl(inner, scl, mag) => {
                inner.get(p, time) + sin_scl_mag(time, *scl, *mag)
            }
            PartProgressSpec::Absin(inner, scl, mag) => {
                inner.get(p, time) + (sin_scl_mag(time, scl * 2.0, *mag) + mag) / 2.0
            }
            PartProgressSpec::Mod(inner, amount) => fmod(inner.get(p, time), *amount),
            PartProgressSpec::Loop(inner, time_amount) => {
                fmod(inner.get(p, time) / time_amount, 1.0)
            }
            PartProgressSpec::Apply(a, b, func) => func(a.get(p, time), b.get(p, time)),
            PartProgressSpec::CurveInterp(inner, interp) => interp.apply(inner.get(p, time)),
        }
    }

    /// `getClamp(p)` (clamps to `0..1`).
    pub fn get_clamp(&self, p: &PartParams, time: f32) -> f32 {
        self.get(p, time).clamp(0.0, 1.0)
    }
}

/// `Mathf.lerp` (local to avoid importing the color lerp).
#[inline]
fn lerp(a: f32, b: f32, f: f32) -> f32 {
    a + (b - a) * f
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(reload: f32, warmup: f32, charge: f32) -> PartParams {
        PartParams {
            reload,
            warmup,
            charge,
            ..Default::default()
        }
    }

    #[test]
    fn leaves_read_params() {
        let params = p(0.25, 0.5, 0.75);
        assert_eq!(PartProgressSpec::Reload.get(&params, 0.0), 0.25);
        assert_eq!(PartProgressSpec::Warmup.get(&params, 0.0), 0.5);
        assert_eq!(PartProgressSpec::Charge.get(&params, 0.0), 0.75);
        assert_eq!(PartProgressSpec::Time.get(&params, 3.0), 3.0);
        assert_eq!(PartProgressSpec::Constant(0.4).get(&params, 0.0), 0.4);
    }

    #[test]
    fn combinators_match_compatfix() {
        let params = p(0.25, 0.5, 0.75);
        use PartProgressSpec::*;
        assert_eq!(Inv(Box::new(Reload)).get(&params, 0.0), 0.75);
        assert!((Slope(Box::new(Constant(0.5))).get(&params, 0.0) - 1.0).abs() < 1e-6);
        assert_eq!(
            Clamp(Box::new(AddConst(Box::new(Reload), 2.0))).get(&params, 0.0),
            1.0
        );
        assert_eq!(
            Add(Box::new(Reload), Box::new(Warmup)).get(&params, 0.0),
            0.75
        );
        assert_eq!(Delay(Box::new(Warmup), 0.0).get(&params, 0.0), 0.5);
        assert_eq!(Curve(Box::new(Warmup), 0.0, 2.0).get(&params, 0.0), 0.25);
        assert_eq!(Shorten(Box::new(Warmup), 0.0).get(&params, 0.0), 0.5);
        assert_eq!(Compress(Box::new(Warmup), 0.0, 1.0).get(&params, 0.0), 0.5);
        assert_eq!(
            Blend(Box::new(Reload), Box::new(Warmup), 0.5).get(&params, 0.0),
            0.375
        );
        assert_eq!(
            Mul(Box::new(Reload), Box::new(Warmup)).get(&params, 0.0),
            0.125
        );
        assert_eq!(MulConst(Box::new(Warmup), 2.0).get(&params, 0.0), 1.0);
        assert_eq!(
            Min(Box::new(Warmup), Box::new(Charge)).get(&params, 0.0),
            0.5
        );
        assert!((Mod(Box::new(Constant(7.5)), 2.0).get(&params, 0.0) - 1.5).abs() < 1e-5);
        assert!((Loop(Box::new(Constant(7.5)), 2.0).get(&params, 0.0) - 0.75).abs() < 1e-5);
        assert_eq!(
            Apply(Box::new(Reload), Box::new(Warmup), |a, b| a + b).get(&params, 0.0),
            0.75
        );
        assert_eq!(
            CurveInterp(Box::new(Warmup), Interp::Reverse).get(&params, 0.0),
            0.5
        );
    }

    #[test]
    fn sustain_matches_java() {
        // val = warmup(0.5) - offset(0.5) = 0 => min(0/grow, (grow+sustain+grow-0)/grow) = 0
        let params = p(0.0, 0.5, 0.0);
        let spec = PartProgressSpec::Sustain(Box::new(PartProgressSpec::Warmup), 0.5, 2.0, 1.0);
        assert_eq!(spec.get(&params, 0.0), 0.0);
    }

    #[test]
    fn sin_terms_match_mathf() {
        let params = p(0.0, 0.5, 0.0);
        // sin(Time.time/scl)*mag at time 1, scl 1, mag 1 => sin(1)
        let spec = PartProgressSpec::SinScl(Box::new(PartProgressSpec::Warmup), 1.0, 1.0);
        let expected = 0.5 + 1.0_f32.sin();
        assert!((spec.get(&params, 1.0) - expected).abs() < 1e-5);
    }
}
