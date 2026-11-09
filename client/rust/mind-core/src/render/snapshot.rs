// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Map-screenshot planning (`core/Renderer.takeMapScreenshot`, `Vars.checkScreenshotMemory`,
//! plan 16 §3.10/M7). Godot performs the readback + PNG write; this module owns
//! the deterministic memory check, buffer size and camera override math.

/// Mobile screenshot memory limit in MB (`mobile ? 65 : 120`).
pub const MOBILE_MEMORY_LIMIT_MB: i64 = 65;
/// Desktop screenshot memory limit in MB.
pub const DESKTOP_MEMORY_LIMIT_MB: i64 = 120;

/// `w * h * 4 / 1024 / 1024` (integer division, as upstream).
pub fn screenshot_memory_mb(width: i32, height: i32) -> i64 {
    (width as i64) * (height as i64) * 4 / 1024 / 1024
}

/// `Vars.checkScreenshotMemory && memory >= (mobile ? 65 : 120)`.
pub fn memory_ok(mobile: bool, check: bool, width: i32, height: i32) -> bool {
    if !check {
        return true;
    }
    let limit = if mobile {
        MOBILE_MEMORY_LIMIT_MB
    } else {
        DESKTOP_MEMORY_LIMIT_MB
    };
    screenshot_memory_mb(width, height) < limit
}

/// The camera/buffer setup for a map screenshot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenshotPlan {
    /// Full-map pixel width (`world.width * tilesize`).
    pub width: i32,
    /// Full-map pixel height.
    pub height: i32,
    /// Camera center in world pixels (`w/2 + tilesize/2`).
    pub camera_pos: [f32; 2],
    /// `Lod.disable = true` during capture.
    pub lod_disabled: bool,
    /// `drawWeather = false` during capture.
    pub draw_weather: bool,
    /// `disableUI = true` during capture.
    pub disable_ui: bool,
    /// Alpha is forced to `255` before writing the PNG.
    pub force_alpha: u8,
}

/// `Renderer.takeMapScreenshot` setup: `None` when the memory check fails.
pub fn plan(
    world_width: i32,
    world_height: i32,
    tilesize: i32,
    mobile: bool,
    check: bool,
) -> Option<ScreenshotPlan> {
    let width = world_width * tilesize;
    let height = world_height * tilesize;
    if !memory_ok(mobile, check, width, height) {
        return None;
    }
    Some(ScreenshotPlan {
        width,
        height,
        camera_pos: [
            width as f32 / 2.0 + tilesize as f32 / 2.0,
            height as f32 / 2.0 + tilesize as f32 / 2.0,
        ],
        lod_disabled: true,
        draw_weather: false,
        disable_ui: true,
        force_alpha: 255,
    })
}

/// Camera zoom that fits the whole plan into `viewport` (the closest
/// fixed-window equivalent of Java's plan-sized `FrameBuffer`): `min(vw/w,
/// vh/h)`, clamped to a positive value.
pub fn fit_zoom(viewport_w: f32, viewport_h: f32, plan: &ScreenshotPlan) -> f32 {
    if plan.width <= 0 || plan.height <= 0 || viewport_w <= 0.0 || viewport_h <= 0.0 {
        return 1.0;
    }
    (viewport_w / plan.width as f32)
        .min(viewport_h / plan.height as f32)
        .max(0.0001)
}

/// The viewport-pixel rect `[x, y, w, h]` covered by the plan at `zoom`,
/// centred in the viewport. Java's `FrameBuffer(w, h)` captures exactly
/// `plan.width × plan.height` at 1:1; a fixed window instead renders the plan
/// scaled to fit and this rect crops the captured image back to the map.
pub fn capture_rect(
    viewport_w: f32,
    viewport_h: f32,
    plan: &ScreenshotPlan,
    zoom: f32,
) -> [i32; 4] {
    if viewport_w <= 0.0 || viewport_h <= 0.0 || plan.width <= 0 || plan.height <= 0 {
        return [0, 0, 1, 1];
    }
    let width = ((plan.width as f32 * zoom).round() as i32).clamp(1, viewport_w as i32);
    let height = ((plan.height as f32 * zoom).round() as i32).clamp(1, viewport_h as i32);
    let x = ((viewport_w as i32 - width) / 2).max(0);
    let y = ((viewport_h as i32 - height) / 2).max(0);
    [x, y, width, height]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_math_matches_upstream() {
        // 2048*2048*4 = 16 MB.
        assert_eq!(screenshot_memory_mb(2048, 2048), 16);
        // A 250x250 map at tilesize 8 => 2000x2000 => 15 MB.
        assert_eq!(screenshot_memory_mb(2000, 2000), 15);
    }

    #[test]
    fn mobile_limit_is_smaller() {
        // 6000x6000*4 = 137 MB.
        assert!(!memory_ok(false, true, 6000, 6000));
        assert!(!memory_ok(true, true, 6000, 6000));
        // 5000x5000*4 = 95 MB: ok on desktop, too big on mobile.
        assert!(memory_ok(false, true, 5000, 5000));
        assert!(!memory_ok(true, true, 5000, 5000));
    }

    #[test]
    fn plan_sets_capture_state() {
        let planned = plan(250, 250, 8, false, true).expect("allowed");
        assert_eq!((planned.width, planned.height), (2000, 2000));
        assert_eq!(planned.camera_pos, [1004.0, 1004.0]);
        assert!(planned.lod_disabled && !planned.draw_weather && planned.disable_ui);
        assert_eq!(planned.force_alpha, 255);
        // With the memory check off, huge maps still plan.
        assert!(plan(10000, 10000, 8, false, false).is_some());
    }

    /// GAPS #53 `ScreenshotPlan` consumption: a 2000x2000 map fits a 1600x900
    /// viewport at zoom 0.45 and crops a centred 900x900 rect at 1:1 map pixels
    /// after the fit scale.
    #[test]
    fn fit_zoom_and_capture_rect_center_the_plan() {
        let planned = plan(250, 250, 8, false, true).expect("allowed");
        let zoom = fit_zoom(1600.0, 900.0, &planned);
        assert!((zoom - 0.45).abs() < 1e-6);
        let rect = capture_rect(1600.0, 900.0, &planned, zoom);
        assert_eq!(rect, [350, 0, 900, 900]);
        // A map smaller than the viewport zooms in until one axis is filled.
        let small = plan(20, 10, 8, false, true).expect("allowed");
        let zoom = fit_zoom(1600.0, 900.0, &small);
        assert!((zoom - 10.0).abs() < 1e-6);
        assert_eq!(
            capture_rect(1600.0, 900.0, &small, zoom),
            [0, 50, 1600, 800]
        );
        // Degenerate viewports keep a positive zoom.
        assert_eq!(fit_zoom(0.0, 0.0, &planned), 1.0);
        assert_eq!(capture_rect(0.0, 0.0, &planned, 1.0), [0, 0, 1, 1]);
    }
}
