// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Plan-12 `Rules` render read view (plan 16 §3.12/M6).
//!
//! A thin, Godot-free adapter over plan 12's `Rules`/`FogControl`/`MapMarkers`
//! so `mind-gdext` never reaches into sim internals. Every field defaults to a
//! safe value so M0–M7 render without plan 12 (plan 16 §3.12).

use crate::render::g3d::PlanetParams;

/// The render-relevant slice of `Rules` (plan 16 §3.12).
#[derive(Clone, Debug, PartialEq)]
pub struct RulesRenderView {
    /// `Rules.staticFog`.
    pub static_fog: bool,
    /// `Rules.fog`.
    pub fog: bool,
    /// `Rules.dynamicColor`.
    pub dynamic_color: [f32; 4],
    /// `Rules.staticColor`.
    pub static_color: [f32; 4],
    /// `Rules.lighting`.
    pub lighting: bool,
    /// `Rules.ambientLight`.
    pub ambient_light: [f32; 4],
    /// `Rules.env` bitmask.
    pub env: u32,
    /// `Rules.backgroundTexture` asset path.
    pub background_texture: Option<String>,
    /// `Rules.backgroundScl`.
    pub background_scl: f32,
    /// `Rules.backgroundSpeed`.
    pub background_speed: f32,
    /// `Rules.backgroundOffsetX/Y`.
    pub background_offset: [f32; 2],
    /// `Rules.planetBackground`.
    pub planet_background: Option<PlanetParams>,
    /// `Rules.customBackgroundCallback` name.
    pub custom_background_callback: Option<String>,
    /// `Rules.limitMapArea`.
    pub limit_map_area: bool,
    /// `Rules.limitX/Y/limitW/limitH`.
    pub limit_rect: [i32; 4],
}

impl Default for RulesRenderView {
    fn default() -> Self {
        Self {
            static_fog: false,
            fog: false,
            dynamic_color: [0.0, 0.0, 0.0, 0.0],
            static_color: [0.0, 0.0, 0.0, 0.0],
            lighting: false,
            ambient_light: [0.01, 0.01, 0.04, 0.99],
            env: 0,
            background_texture: None,
            background_scl: 1.0,
            background_speed: 1.0,
            background_offset: [0.0, 0.0],
            planet_background: None,
            custom_background_callback: None,
            limit_map_area: false,
            limit_rect: [0, 0, 0, 0],
        }
    }
}

impl RulesRenderView {
    /// `FogRenderer.drawFog`: the dynamic fog color with the `0.5` alpha floor
    /// (NaN treated as `0.5`).
    pub fn dynamic_fog_color(&self) -> [f32; 4] {
        let mut color = self.dynamic_color;
        color[3] = if color[3].is_nan() {
            0.5
        } else {
            color[3].max(0.5)
        };
        color
    }

    /// `FogRenderer.drawFog`: the static fog color's alpha is forced to `1`.
    pub fn static_fog_color(&self) -> [f32; 4] {
        [
            self.static_color[0],
            self.static_color[1],
            self.static_color[2],
            1.0,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_testable_without_plan_12() {
        let rules = RulesRenderView::default();
        assert!(!rules.static_fog && !rules.fog && !rules.lighting);
        assert_eq!(rules.ambient_light, [0.01, 0.01, 0.04, 0.99]);
        assert_eq!(rules.env, 0);
    }

    #[test]
    fn fog_color_alpha_floor() {
        let low = RulesRenderView {
            dynamic_color: [0.1, 0.2, 0.3, 0.2],
            ..RulesRenderView::default()
        };
        assert_eq!(low.dynamic_fog_color()[3], 0.5);
        let high = RulesRenderView {
            dynamic_color: [0.1, 0.2, 0.3, 0.9],
            ..RulesRenderView::default()
        };
        assert!((high.dynamic_fog_color()[3] - 0.9).abs() < 1e-6);
        assert_eq!(high.static_fog_color()[3], 1.0);
    }
}
