// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! View/FX seams for combat (plan 10 §3.12, HLP §12 C6).
//!
//! Plan 17 owns the FX registry and playback. This module defines the minimal
//! [`FxSink`] trait that simulaton code calls instead of `Effect.at`/`Sound.at`
//! and screen shake, so the sim never links the renderer. A [`NoopFx`] default
//! preserves the upstream structural `headless` guards.
//!
//! **Reconciliation (plan 10 → 17):** `FxSink` and the `CombatFx` alias below
//! are the frozen seam; plan 17 implements `FxSink` on its `FxSinkImpl` and
//! re-exports this trait. Do not add a second FX dispatch path.

use crate::content::registries::fx_meta::EffectRef;
use crate::content::registries::sound_meta::SoundId;
use crate::content::{EffectId, Rgba};
use crate::fx::TrailChannelId;

/// Combat effect sink (plan-17 playback boundary; no-op in headless).
pub trait FxSink: Send + Sync {
    /// Plays a named or inline effect (`Effect.at`).
    fn effect(&self, effect: &EffectRef, x: f32, y: f32, rotation: f32, color: Rgba) {
        let _ = (effect, x, y, rotation, color);
    }

    /// Adds screen shake (`Effect.shake`).
    fn shake(&self, amount: f32) {
        let _ = amount;
    }

    /// Emits a light (`Vars.renderer.addLight` view hook).
    fn light(&self, x: f32, y: f32, radius: f32, color: Rgba) {
        let _ = (x, y, radius, color);
    }

    /// Plays a one-shot sound (`Sound.at`).
    fn sound(&self, sound: SoundId, volume: f32, pitch: f32) {
        let _ = (sound, volume, pitch);
    }

    /// Appends a bullet/weapon trail point (plan 17 owns `Trail`).
    fn trail(&self, x: f32, y: f32, rotation: f32, color: Rgba, width: f32, length: f32) {
        let _ = (x, y, rotation, color, width, length);
    }

    /// Appends a trail point to a known channel (plan 17 §3.9). The default
    /// forwards to [`FxSink::trail`] so existing sinks keep their behavior; a
    /// channel-aware sink can resolve the per-channel `TrailRegistry` tint.
    #[allow(clippy::too_many_arguments)]
    fn trail_channel(
        &self,
        channel: TrailChannelId,
        x: f32,
        y: f32,
        rotation: f32,
        color: Rgba,
        width: f32,
        length: f32,
    ) {
        let _ = channel;
        self.trail(x, y, rotation, color, width, length);
    }
}

/// Resolves an [`EffectRef`] to its named [`EffectId`] when possible.
pub fn resolve_effect(effect: &EffectRef) -> EffectId {
    match effect {
        EffectRef::Named(id) => *id,
        EffectRef::Inline(_) => EffectId::NONE,
    }
}

/// Default no-op sink (headless/bootstrap).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopFx;

impl FxSink for NoopFx {}

/// Plan-10 alias for the plan-17-owned sink (HLP §12 C6).
pub type CombatFx = dyn FxSink;

/// Shared, cheaply cloneable sink handle used by systems.
pub type FxHandle = std::sync::Arc<dyn FxSink>;

/// Builds the default headless FX handle.
pub fn noop_fx() -> FxHandle {
    std::sync::Arc::new(NoopFx)
}

/// Bullet render state consumed by plan 16 (`Layer::bullet`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BulletDrawState {
    /// Content id.
    pub def: crate::content::BulletId,
    /// World position.
    pub x: f32,
    /// World position.
    pub y: f32,
    /// Rotation in degrees.
    pub rotation: f32,
    /// Effect parameter (`Bullet.fdata`).
    pub fin: f32,
    /// Shield absorption alpha (`0..1`).
    pub shield_alpha: f32,
}

/// Turret render state consumed by plan 16 (`Layer::turret`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurretDrawState {
    /// Turret rotation in degrees.
    pub rotation: f32,
    /// Recoil offset.
    pub recoil: f32,
    /// Heat `0..1`.
    pub heat: f32,
    /// Warmup `0..1`.
    pub warmup: f32,
    /// Charge `0..1`.
    pub charge: f32,
    /// Ammo fraction `0..1`.
    pub ammo_fraction: f32,
}

/// Shield render state consumed by plan 16 (`Layer::shields`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShieldDrawState {
    /// World center.
    pub x: f32,
    /// World center.
    pub y: f32,
    /// Radius in world units.
    pub radius: f32,
    /// Regular polygon side count.
    pub sides: i32,
    /// Rotation in degrees.
    pub rotation: f32,
    /// Tint.
    pub color: Rgba,
    /// Whether the shield was hit this frame.
    pub hit: bool,
    /// Whether the shield is broken.
    pub broken: bool,
}

/// Laser/beam render state consumed by plan 16 (`Layer::effect`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LaserDrawState {
    /// Start point.
    pub from: (f32, f32),
    /// End point.
    pub to: (f32, f32),
    /// Stroke width.
    pub width: f32,
    /// Grade color.
    pub color: Rgba,
    /// Remaining life in ticks.
    pub life: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_sink_is_default_and_resolves_named_effects() {
        let fx = noop_fx();
        let effect = EffectRef::Named(EffectId::BLOCK_CRASH);
        fx.effect(&effect, 1.0, 2.0, 0.0, Rgba::WHITE);
        assert_eq!(resolve_effect(&effect), EffectId::BLOCK_CRASH);
        assert_eq!(resolve_effect(&EffectRef::default()), EffectId::NONE);
    }
}
