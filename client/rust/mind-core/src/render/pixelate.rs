// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Pixelator` math (`graphics/Pixelator.java`, plan 16 §3.8 / M6).
//!
//! Godot owns the `SubViewport` and the `screenspace` blit; this module owns
//! the deterministic size clamp and the half-pixel camera snap so the headless
//! tests can assert the exact low-res target without an engine. View-only.

/// Computes the low-res render target size (`Pixelator.drawPixelate`).
///
/// `scale` is `renderer.getScale()`; it is truncated before use and the camera
/// width/height are truncated too. When `cutscene` is set the size is scaled by
/// `land_scale / scale`. The result is clamped to `2..=screen` on each axis.
pub fn pixel_size(
    scale: f32,
    camera_w: f32,
    camera_h: f32,
    screen_w: i32,
    screen_h: i32,
    cutscene: bool,
    land_scale: f32,
) -> (i32, i32) {
    let scale = (scale as i32) as f32;
    let mut w = camera_w as i32;
    let mut h = camera_h as i32;
    if cutscene && scale != 0.0 {
        w = (camera_w * land_scale / scale) as i32;
        h = (camera_h * land_scale / scale) as i32;
    }
    (w.clamp(2, screen_w.max(2)), h.clamp(2, screen_h.max(2)))
}

/// `Pixelator.drawPixelate` camera position snap: the integer pixel plus a half
/// pixel when the (truncated) camera dimension is odd, so the low-res grid
/// aligns with the screen.
pub fn snap_position(px: f32, py: f32, camera_w: f32, camera_h: f32) -> (f32, f32) {
    let x = (px as i32) as f32 + if (camera_w as i32) % 2 == 0 { 0.0 } else { 0.5 };
    let y = (py as i32) as f32 + if (camera_h as i32) % 2 == 0 { 0.0 } else { 0.5 };
    (x, y)
}

/// The truncated camera scale (`scale = (int)scale`).
pub fn truncate_scale(scale: f32) -> f32 {
    (scale as i32) as f32
}

/// Whether the pixelator is active (`Pixelator.enabled`).
pub fn enabled(pixelate: bool) -> bool {
    pixelate
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_size_clamps_to_screen() {
        assert_eq!(
            pixel_size(1.0, 320.0, 180.0, 1280, 720, false, 1.0),
            (320, 180)
        );
        assert_eq!(
            pixel_size(0.5, 4000.0, 3000.0, 1280, 720, false, 1.0),
            (1280, 720)
        );
        assert_eq!(pixel_size(1.0, 1.0, 1.0, 1280, 720, false, 1.0), (2, 2));
    }

    #[test]
    fn cutscene_scales_by_land_scale() {
        // scale truncated to 2, landScale 1 => 320*1/2 = 160.
        assert_eq!(
            pixel_size(2.0, 320.0, 180.0, 1280, 720, true, 1.0),
            (160, 90)
        );
    }

    #[test]
    fn snap_uses_half_pixel_for_odd_dimensions() {
        assert_eq!(snap_position(10.4, 20.6, 320.0, 180.0), (10.0, 20.0));
        assert_eq!(snap_position(10.4, 20.6, 321.0, 181.0), (10.5, 20.5));
    }
}
