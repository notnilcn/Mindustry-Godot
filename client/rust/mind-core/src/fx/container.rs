// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `EffectContainer` and `Scaled` semantics (plan 17 §3.5).
//!
//! Ported from `entities/Effect.java`'s nested `EffectContainer implements
//! Scaled`. `scaled` reuses one inner container exactly like Java; `fin` family
//! match `arc.math.Scaled`.

use crate::content::{EffectId, Rgba};
use crate::math::{Interp, slope};

use super::data::EffectData;

/// The per-state render context passed to effect bodies.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectContainer {
    /// Effect id.
    pub id: EffectId,
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// Elapsed ticks (`life`).
    pub time: f32,
    /// Total ticks.
    pub lifetime: f32,
    /// Rotation in degrees (`baseRotation + rotation`).
    pub rotation: f32,
    /// Tint.
    pub color: Rgba,
    /// Typed payload.
    pub data: EffectData,
    /// Shared inner container for `scaled` (plan parity).
    pub(crate) inner: Option<Box<EffectContainer>>,
}

impl Default for EffectContainer {
    fn default() -> Self {
        Self {
            id: EffectId::NONE,
            x: 0.0,
            y: 0.0,
            time: 0.0,
            lifetime: 0.0,
            rotation: 0.0,
            color: Rgba::WHITE,
            data: EffectData::None,
            inner: None,
        }
    }
}

impl EffectContainer {
    /// `EffectContainer.set(...)`.
    #[allow(clippy::too_many_arguments)]
    pub fn set(
        &mut self,
        id: EffectId,
        color: Rgba,
        life: f32,
        lifetime: f32,
        rotation: f32,
        x: f32,
        y: f32,
        data: EffectData,
    ) {
        self.id = id;
        self.color = color;
        self.time = life;
        self.lifetime = lifetime;
        self.rotation = rotation;
        self.x = x;
        self.y = y;
        self.data = data;
    }

    /// `Scaled.fin()`.
    #[inline]
    pub fn fin(&self) -> f32 {
        if self.lifetime == 0.0 {
            0.0
        } else {
            self.time / self.lifetime
        }
    }

    /// `Scaled.fin(Interp)`.
    #[inline]
    pub fn fin_with(&self, interp: Interp) -> f32 {
        interp.apply(self.fin())
    }

    /// `Scaled.fout()`.
    #[inline]
    pub fn fout(&self) -> f32 {
        1.0 - self.fin()
    }

    /// `Scaled.fout(Interp)`.
    #[inline]
    pub fn fout_with(&self, interp: Interp) -> f32 {
        interp.apply(self.fout())
    }

    /// `Scaled.finpow()` (`Interp.pow3Out`).
    #[inline]
    pub fn finpow(&self) -> f32 {
        Interp::Pow3Out.apply(self.fin())
    }

    /// `Scaled.fslope()`.
    #[inline]
    pub fn fslope(&self) -> f32 {
        slope(self.fin())
    }

    /// `Scaled.fout(float margin)`.
    #[inline]
    pub fn fout_margin(&self, margin: f32) -> f32 {
        let f = self.fin();
        if f >= 1.0 - margin {
            1.0 - (f - (1.0 - margin)) / margin
        } else {
            1.0
        }
    }

    /// `EffectContainer.inner()`.
    pub fn inner_mut(&mut self) -> &mut EffectContainer {
        self.inner
            .get_or_insert_with(|| Box::new(EffectContainer::default()))
    }

    /// `EffectContainer.scaled(lifetime, cons)`: runs `f` on a shared inner
    /// container when `time <= lifetime`.
    pub fn scaled(&mut self, lifetime: f32, f: impl FnOnce(&EffectContainer)) {
        if self.time <= lifetime {
            let (id, x, y, time, rotation, color) = (
                self.id,
                self.x,
                self.y,
                self.time,
                self.rotation,
                self.color,
            );
            let data = self.data.clone();
            let inner = self
                .inner
                .get_or_insert_with(|| Box::new(EffectContainer::default()));
            inner.id = id;
            inner.x = x;
            inner.y = y;
            inner.time = time;
            inner.lifetime = lifetime;
            inner.rotation = rotation;
            inner.color = color;
            inner.data = data;
            f(inner);
        }
    }

    /// Convenience accessor for the typed payload.
    pub fn data(&self) -> &EffectData {
        &self.data
    }

    /// Read-only form of [`EffectContainer::scaled`] for effect bodies that
    /// receive `&EffectContainer` (plan 17 dispatch signature). Builds a
    /// temporary inner container instead of mutating; observable output is
    /// identical.
    pub fn scaled_view(&self, lifetime: f32, f: impl FnOnce(&EffectContainer)) {
        if self.time <= lifetime {
            let mut inner = self.clone();
            inner.lifetime = lifetime;
            inner.inner = None;
            f(&inner);
        }
    }
}

/// `Effect.render` result: the (possibly mutated) lifetime, used by `trailFade`.
pub type RenderLifetime = f32;

#[cfg(test)]
mod tests {
    use super::*;

    fn container(life: f32, lifetime: f32) -> EffectContainer {
        let mut c = EffectContainer::default();
        c.set(
            EffectId(1),
            Rgba::WHITE,
            life,
            lifetime,
            0.0,
            0.0,
            0.0,
            EffectData::None,
        );
        c
    }

    #[test]
    fn fin_fout_finpow_fslope_exact() {
        let c = container(25.0, 100.0);
        assert_eq!(c.fin(), 0.25);
        assert_eq!(c.fout(), 0.75);
        assert_eq!(c.finpow(), Interp::Pow3Out.apply(0.25));
        assert_eq!(c.fslope(), slope(0.25));
        assert_eq!(c.fin_with(Interp::Linear), 0.25);
        assert_eq!(c.fout_with(Interp::Linear), 0.75);
    }

    #[test]
    fn zero_lifetime_does_not_divide_by_zero() {
        let c = container(5.0, 0.0);
        assert_eq!(c.fin(), 0.0);
    }

    #[test]
    fn scaled_gates_and_reuses_inner() {
        let mut c = container(10.0, 100.0);
        let mut calls = 0;
        c.scaled(20.0, |inner| {
            calls += 1;
            assert_eq!(inner.lifetime, 20.0);
            assert_eq!(inner.time, 10.0);
            assert_eq!(inner.id, EffectId(1));
        });
        // time 10 > 20? no; runs once.
        assert_eq!(calls, 1);
        c.scaled(5.0, |_| calls += 100);
        // time 10 > 5 => skipped.
        assert_eq!(calls, 1);
        // Inner is reused (same pointer across calls).
        let first = c.inner.as_ref().map(|b| (&**b) as *const _);
        c.scaled(50.0, |_| {});
        let second = c.inner.as_ref().map(|b| (&**b) as *const _);
        assert_eq!(first, second);
    }

    #[test]
    fn fout_margin_matches_java() {
        let c = container(99.0, 100.0);
        // margin 0.02: f=0.99 >= 0.98 => 1 - (0.99-0.98)/0.02 = 0.5
        assert!((c.fout_margin(0.02) - 0.5).abs() < 1e-5);
        let c = container(10.0, 100.0);
        assert_eq!(c.fout_margin(0.02), 1.0);
    }
}
