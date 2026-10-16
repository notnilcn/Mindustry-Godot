// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Pixelator` host (`graphics/Pixelator.java`, plan 16 §3.8/M6).
//!
//! Owns the low-res target size, the `pixelate` setting gate and the blit.
//! Deviation S16-9 (narrowed): Java re-renders the world into a buffer of
//! `camera.width x camera.height` world pixels and blits it with nearest
//! filtering (`Pixelator.drawPixelate`); the port draws the world at full
//! resolution and a top-most world-space overlay resamples the screen at
//! `(int)renderer.getScale()`-pixel cells (one cell per world pixel), which is
//! the same visible result without a second render of every band.

use godot::classes::control::MouseFilter;
use godot::classes::{ColorRect, Node2D, Shader, ShaderMaterial};
use godot::obj::NewGd;
use godot::prelude::*;
use mind_core::render::pixelate::pixel_size;

/// Child name of the overlay under the world renderer.
const OVERLAY_NAME: &str = "PixelateOverlay";

/// `RenderingServer.CANVAS_ITEM_Z_MAX`: above every world band (the UI lives
/// on its own `CanvasLayer` and is never pixelated, like Java's HUD).
const OVERLAY_Z: i32 = 4096;

/// Screen-space nearest resample: every `pixel_size` x `pixel_size` block of
/// the already-drawn world shows its centre texel (`Pixelator`'s low-res
/// buffer blitted with `TextureFilter.nearest`).
const OVERLAY_SHADER: &str = "shader_type canvas_item;
render_mode unshaded, blend_disabled;
uniform sampler2D screen_tex : hint_screen_texture, filter_nearest;
uniform float pixel_size = 4.0;
void fragment() {
	vec2 px = SCREEN_UV / SCREEN_PIXEL_SIZE;
	vec2 cell = (floor(px / pixel_size) + 0.5) * pixel_size;
	COLOR = vec4(textureLod(screen_tex, cell * SCREEN_PIXEL_SIZE, 0.0).rgb, 1.0);
}
";

/// Pixelator counters + derived target.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pixelator {
    enabled: bool,
    size: (i32, i32),
    /// Last persisted `pixelate` value adopted (`Renderer.pixelate =
    /// settings.getBool("pixelate")`, `Renderer.java:178`); change-detected
    /// so the `set_pixelate` debug seam holds until the setting changes.
    last_setting: Option<bool>,
}

impl Pixelator {
    /// A disabled pixelator.
    pub fn new() -> Self {
        Self::default()
    }

    /// `Pixelator.enabled`.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Sets the `pixelate` setting.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Adopts the persisted `pixelate` setting when it changed.
    pub fn poll_setting(&mut self) {
        let value = crate::settings_hub::row_bool("pixelate");
        if self.last_setting != Some(value) {
            self.last_setting = Some(value);
            self.enabled = value;
        }
    }

    /// The low-res target size (`Pixelator.drawPixelate`).
    pub fn size(&self) -> (i32, i32) {
        self.size
    }

    /// Recomputes the target when enabled.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        scale: f32,
        camera_w: f32,
        camera_h: f32,
        screen_w: i32,
        screen_h: i32,
        cutscene: bool,
        land_scale: f32,
    ) {
        if !self.enabled {
            return;
        }
        self.size = pixel_size(
            scale, camera_w, camera_h, screen_w, screen_h, cutscene, land_scale,
        );
    }
}

/// Shows/updates (or hides) the pixelate overlay under `parent` (the world
/// renderer). `center`/`size` are the camera view in the parent's space and
/// `zoom` the camera scale (`renderer.getScale()`, truncated like Java).
pub fn sync_overlay(
    mut parent: Gd<Node2D>,
    enabled: bool,
    center: (f32, f32),
    size: (f32, f32),
    zoom: f32,
) {
    let existing = parent
        .get_node_or_null(OVERLAY_NAME)
        .and_then(|node| node.try_cast::<ColorRect>().ok());
    let mut rect = match existing {
        Some(rect) => rect,
        None if !enabled => return,
        None => {
            let mut shader = Shader::new_gd();
            shader.set_code(OVERLAY_SHADER);
            let mut material = ShaderMaterial::new_gd();
            material.set_shader(&shader);
            let mut rect = ColorRect::new_alloc();
            rect.set_name(OVERLAY_NAME);
            rect.set_mouse_filter(MouseFilter::IGNORE);
            rect.set_z_index(OVERLAY_Z);
            rect.set_z_as_relative(false);
            rect.set_material(&material);
            // code-instantiated: one pixelate blit per world renderer, created
            // the first time the setting is enabled.
            parent.add_child(&rect);
            rect
        }
    };
    rect.set_visible(enabled);
    if !enabled {
        return;
    }
    // Cover the view with a margin (camera shake offsets the drawn view).
    let w = size.0 * 1.25 + 64.0;
    let h = size.1 * 1.25 + 64.0;
    rect.set_position(Vector2::new(center.0 - w / 2.0, center.1 - h / 2.0));
    rect.set_size(Vector2::new(w, h));
    let pixel = zoom.trunc().max(1.0);
    if let Some(mut material) = rect
        .get_material()
        .and_then(|material| material.try_cast::<ShaderMaterial>().ok())
    {
        material.set_shader_parameter("pixel_size", &pixel.to_variant());
    }
}
