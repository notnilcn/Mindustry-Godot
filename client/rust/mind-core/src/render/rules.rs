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
    /// `Rules.dropZoneRadius` (`OverlayRenderer.java:200-211`).
    pub drop_zone_radius: f32,
    /// `Rules.enemyCoreBuildRadius` (core no-build circles).
    pub enemy_core_build_radius: f32,
    /// `Rules.polygonCoreProtection`.
    pub polygon_core_protection: bool,
    /// `Rules.unitLight` (light pass gate).
    pub unit_light: bool,
    /// `Rules.buildRadius(team)` per team present in `Rules.teams`
    /// (`protectCores ? enemyCoreBuildRadius + extraCoreBuildRadius : 0`);
    /// teams absent from the map default to protected
    /// (`fallbackTeamRule().protectCores = true`, `Rules.java:355,412`).
    pub core_build_radius: Vec<(u8, f32)>,
}

impl Default for RulesRenderView {
    fn default() -> Self {
        // Java defaults come from the live `Rules` model, never hard-coded.
        let rules = crate::io::json::rules::Rules::default();
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
            drop_zone_radius: rules.drop_zone_radius,
            enemy_core_build_radius: rules.enemy_core_build_radius,
            polygon_core_protection: rules.polygon_core_protection,
            unit_light: rules.unit_light,
            core_build_radius: Vec::new(),
        }
    }
}

impl RulesRenderView {
    /// plan-12 `Rules` → render view (G03 T12): the renderer's live rules feed.
    /// Background/planet fields (not on `Rules`) keep their `Default` values.
    pub fn from_rules(rules: &crate::io::json::rules::Rules) -> Self {
        let color = |value: &crate::io::json::content_serde::ColorHex| {
            let c = value.0;
            [c.r, c.g, c.b, c.a]
        };
        Self {
            static_fog: rules.static_fog,
            fog: rules.fog,
            dynamic_color: color(&rules.dynamic_color),
            static_color: color(&rules.static_color),
            lighting: rules.lighting,
            ambient_light: color(&rules.ambient_light),
            env: rules.env as u32,
            limit_map_area: rules.limit_map_area,
            limit_rect: [
                rules.limit_x,
                rules.limit_y,
                rules.limit_width,
                rules.limit_height,
            ],
            drop_zone_radius: rules.drop_zone_radius,
            enemy_core_build_radius: rules.enemy_core_build_radius,
            polygon_core_protection: rules.polygon_core_protection,
            unit_light: rules.unit_light,
            core_build_radius: rules
                .teams
                .0
                .keys()
                .map(|team| (*team, rules.build_radius(*team)))
                .collect(),
            ..Self::default()
        }
    }

    /// `Rules.buildRadius(team)`: the view's per-team radius, falling back to
    /// `enemyCoreBuildRadius` for teams absent from `Rules.teams` (the Java
    /// fallback team rule has `protectCores = true`, `Rules.java:355,412`).
    pub fn core_build_radius_for(&self, team: u8) -> f32 {
        self.core_build_radius
            .iter()
            .find(|(t, _)| *t == team)
            .map(|(_, radius)| *radius)
            .unwrap_or(self.enemy_core_build_radius)
    }

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
        // Java defaults (`Rules.java:125-151,213`): 400 / false / 300 / true.
        assert_eq!(rules.enemy_core_build_radius, 400.0);
        assert!(!rules.polygon_core_protection);
        assert_eq!(rules.drop_zone_radius, 300.0);
        assert!(rules.unit_light);
    }

    #[test]
    fn from_rules_maps_the_render_slice() {
        let rules = crate::io::json::rules::Rules::default();
        let view = RulesRenderView::from_rules(&rules);
        assert!(!view.lighting);
        assert_eq!(view.env, rules.env as u32);
        assert_eq!(view.limit_map_area, rules.limit_map_area);
        assert_eq!(
            view.limit_rect,
            [
                rules.limit_x,
                rules.limit_y,
                rules.limit_width,
                rules.limit_height
            ]
        );
        let ambient = rules.ambient_light.0;
        assert_eq!(
            view.ambient_light,
            [ambient.r, ambient.g, ambient.b, ambient.a]
        );
    }

    #[test]
    fn from_rules_core_radius() {
        // Default rules have no team entries; absent teams use the protected
        // fallback -> 400 for team 2 (`Rules.buildRadius`).
        let rules = crate::io::json::rules::Rules::default();
        let view = RulesRenderView::from_rules(&rules);
        assert_eq!(view.core_build_radius_for(2), 400.0);
        // A team rule with `protect_cores = false` -> 0, even with an extra
        // radius; `protectCores` wins.
        let mut rules = crate::io::json::rules::Rules::default();
        rules.team_rule_mut(2).protect_cores = false;
        rules.team_rule_mut(2).extra_core_build_radius = 50.0;
        let view = RulesRenderView::from_rules(&rules);
        assert_eq!(view.core_build_radius, vec![(2, 0.0)]);
        rules.team_rule_mut(2).protect_cores = true;
        let view = RulesRenderView::from_rules(&rules);
        assert_eq!(view.core_build_radius, vec![(2, 450.0)]);
        assert_eq!(view.core_build_radius_for(2), 450.0);
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
