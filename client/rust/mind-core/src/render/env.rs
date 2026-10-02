// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Environment flags + env-renderer registration (`world/meta/Env.java`,
//! `graphics/EnvRenderers.java`, plan 16 §3.8/M6).
//!
//! Plan 16 owns the registration/selection order and ports the default
//! underwater/scorching passes; plan 17 owns the FX bodies and re-registers
//! through the same API. View-only.

/// `Env.terrestrial`.
pub const TERRESTRIAL: u32 = 1;
/// `Env.space`.
pub const SPACE: u32 = 1 << 1;
/// `Env.underwater`.
pub const UNDERWATER: u32 = 1 << 2;
/// `Env.spores`.
pub const SPORES: u32 = 1 << 3;
/// `Env.scorching`.
pub const SCORCHING: u32 = 1 << 4;
/// `Env.groundOil`.
pub const GROUND_OIL: u32 = 1 << 5;
/// `Env.groundWater`.
pub const GROUND_WATER: u32 = 1 << 6;
/// `Env.oxygen`.
pub const OXYGEN: u32 = 1 << 7;
/// `Env.any`.
pub const ANY: u32 = 0xffff_ffff;
/// `Env.none`.
pub const NONE: u32 = 0;

/// `(renderer.env & rules.env) == renderer.env` (`Renderer.draw` stage 15).
pub fn matches(renderer_env: u32, rules_env: u32) -> bool {
    (renderer_env & rules_env) == renderer_env
}

/// `EnvRenderers` underwater constants.
pub mod underwater {
    /// `Color.valueOf("353982")` tinted at `0.4`.
    pub const COLOR: [f32; 4] = [
        0x35 as f32 / 255.0,
        0x39 as f32 / 255.0,
        0x82 as f32 / 255.0,
        0.4,
    ];
    /// `int rays = 50`.
    pub const RAYS: usize = 50;
    /// `float timeScale = 2000f`.
    pub const TIME_SCALE: f32 = 2000.0;
    /// `Color.valueOf("a7c1fa")` particle tint.
    pub const PARTICLE_COLOR: [f32; 4] = [
        0xa7 as f32 / 255.0,
        0xc1 as f32 / 255.0,
        0xfa as f32 / 255.0,
        1.0,
    ];
    /// `windSpeed = 0.03f`.
    pub const WIND_SPEED: f32 = 0.03;
    /// `windAngle = 45f`.
    pub const WIND_ANGLE: f32 = 45.0;
}

/// `EnvRenderers` scorching constants.
pub mod scorching {
    /// The distort texture (`sprites/distortAlpha.png`).
    pub const TEXTURE: &str = "distortAlpha";
    /// `Color.scarlet`.
    pub const COLOR: [f32; 4] = [1.0, 0.14, 0.14, 1.0];
    /// `Weather.drawNoiseLayers(..., 4, -1.3f, 0.7f, 0.8f, 0.9f)`.
    pub const LAYERS: i32 = 4;
    /// Layer bias (`-1.3f`).
    pub const LAYER_BIAS: f32 = -1.3;
}

/// One registered environment renderer (`Renderer.EnvRenderer`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnvRendererSpec {
    /// Environment bitmask (`EnvRenderer.env`).
    pub env: u32,
    /// Stable pass name (Godot resolves it to a draw routine).
    pub pass: &'static str,
}

/// The ordered env-renderer registry (`Renderer.envRenderers`).
#[derive(Clone, Debug, Default)]
pub struct EnvRegistry {
    renderers: Vec<EnvRendererSpec>,
}

impl EnvRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// `Renderer.addEnvRenderer(env, pass)` (append order preserved).
    pub fn add(&mut self, env: u32, pass: &'static str) {
        self.renderers.push(EnvRendererSpec { env, pass });
    }

    /// All registered specs in registration order.
    pub fn all(&self) -> &[EnvRendererSpec] {
        &self.renderers
    }

    /// `Renderer.draw` stage 15: every matching renderer in registration order.
    pub fn select(&self, rules_env: u32) -> Vec<EnvRendererSpec> {
        self.renderers
            .iter()
            .copied()
            .filter(|r| matches(r.env, rules_env))
            .collect()
    }

    /// Registers the default underwater + scorching passes (`EnvRenderers.init`).
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.add(UNDERWATER, "underwater");
        registry.add(SCORCHING, "scorching");
        registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_mask_match_requires_all_bits() {
        assert!(matches(UNDERWATER, UNDERWATER | TERRESTRIAL));
        assert!(!matches(UNDERWATER, SCORCHING | SPACE));
        assert!(matches(NONE, NONE));
    }

    #[test]
    fn registry_selects_in_registration_order() {
        let registry = EnvRegistry::with_defaults();
        let selected = registry.select(UNDERWATER | SCORCHING);
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].pass, "underwater");
        assert_eq!(selected[1].pass, "scorching");
        assert!(registry.select(SPACE).is_empty());
    }
}
