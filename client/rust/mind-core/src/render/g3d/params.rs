// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PlanetParams` view data (`graphics/g3d/PlanetParams.java`, plan 16
//! §3.10/M7). Fields 1:1 with upstream; the Godot `PlanetRenderer` host consumes
//! this. `viewW`/`viewH` are `-1` to mean "use screen size".

/// Parameters for rendering a solar system (`PlanetParams`).
#[derive(Clone, Debug, PartialEq)]
pub struct PlanetParams {
    /// Camera direction relative to the planet (`camPos`).
    pub cam_pos: [f32; 3],
    /// Previous planet position for camera interpolation (`otherCamPos`).
    pub other_cam_pos: Option<[f32; 3]>,
    /// Interpolation value for `other_cam_pos` (`otherCamAlpha`).
    pub other_cam_alpha: f32,
    /// Camera up vector (`camUp`).
    pub cam_up: [f32; 3],
    /// Unit-length camera direction (`camDir`).
    pub cam_dir: [f32; 3],
    /// Logical planet id (`Planet.registry id`).
    pub planet: u32,
    /// Zoom relative to the planet.
    pub zoom: f32,
    /// Orbit/UI alpha.
    pub ui_alpha: f32,
    /// Whether orbit/sector grid is drawn.
    pub draw_ui: bool,
    /// Whether the space skybox is drawn.
    pub draw_skybox: bool,
    /// Viewport width (`-1` = screen).
    pub view_w: i32,
    /// Viewport height (`-1` = screen).
    pub view_h: i32,
    /// Force atmosphere regardless of options.
    pub always_draw_atmosphere: bool,
}

impl Default for PlanetParams {
    fn default() -> Self {
        Self {
            cam_pos: [0.0, 0.0, 4.0],
            other_cam_pos: None,
            other_cam_alpha: 0.0,
            cam_up: [0.0, 1.0, 0.0],
            cam_dir: [0.0, 0.0, -1.0],
            planet: 0,
            zoom: 1.0,
            ui_alpha: 1.0,
            draw_ui: false,
            draw_skybox: true,
            view_w: -1,
            view_h: -1,
            always_draw_atmosphere: false,
        }
    }
}
