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
}
