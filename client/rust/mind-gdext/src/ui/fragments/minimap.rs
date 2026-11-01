// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/fragments/MinimapFragment.java
//         (plan 14 §3.5, M4).
//
// Fullscreen minimap: hosts the `MindMinimapWidget`, captures its pan/zoom/tap
// intents and forwards them to the `MindCamera2D` host (plan 15). Terrain and
// entity pixels come from the native `MindMinimap` provider the widget
// consumes; this fragment owns only the camera hand-off and the show/hide
// toggle.

//! `MindMinimapFragment` — the fullscreen minimap overlay
//! (`MinimapFragment.toggle`).
//!
//! 1:1 port of `client/ui/fragments/minimap_fragment.gd`:
//!
//! - `pan_requested(Vector2)` adds the world-pixel delta to the camera position
//!   (`MindCamera2D.pan_to`).
//! - `zoom_changed(f32)` converts the absolute widget zoom into the
//!   `Renderer.scaleCamera` step `(ratio - 1) * 4` (`ZOOM_STEP_DIVISOR = 4`) and
//!   calls `MindCamera2D.zoom_by` when the step is non-zero.
//! - `tapped()` toggles the overlay (`toggle`), which raises the node to the
//!   front while shown.
//!
//! The widget is the Rust class `MindMinimapWidget` (registered as
//! `RustMindMinimapWidget` while the `.gd` files are still in the tree), so the
//! scene lookup casts to the native type instead of a script.

use godot::classes::{Control, IControl};
use godot::prelude::*;

use crate::MindCamera2D;
use crate::ui::kit;
use crate::ui::kit::widgets::minimap::MindMinimapWidget;

/// `/root/Spine/World/Camera2D` (`MindCamera2D`).
const CAMERA_PATH: &str = "/root/Spine/World/Camera2D";

/// `Renderer.ZOOM_STEP_DIVISOR` (`Renderer.scaleCamera`): a factor ratio `r`
/// becomes the step `(r - 1) * 4`.
const ZOOM_STEP_DIVISOR: f32 = 4.0;

/// Zoom floor for the ratio (`maxf(_last_zoom, 0.0001)`).
const MIN_ZOOM_DIVISOR: f32 = 0.0001;

/// Steps below this are dropped (`absf(step) > 0.0001`).
const ZOOM_EPSILON: f32 = 0.0001;

/// `MindMinimapFragment` — fullscreen minimap with camera pan/zoom forwarding.
#[derive(GodotClass)]
#[class(base=Control, init)]
pub struct MindMinimapFragment {
    base: Base<Control>,
    /// Last absolute widget zoom, converted into a `Renderer.scaleCamera` step.
    #[init(val = 1.0)]
    last_zoom: f32,
    /// `Center/Minimap` (`@onready var minimap: MindMinimapWidget`).
    minimap: Option<Gd<MindMinimapWidget>>,
}

#[godot_api]
impl IControl for MindMinimapFragment {
    fn ready(&mut self) {
        self.minimap = self
            .base()
            .get_node_or_null("Center/Minimap")
            .and_then(|node| node.try_cast::<MindMinimapWidget>().ok());
        let Some(mut minimap) = self.minimap.clone() else {
            return;
        };
        let callable = self.base().callable("_on_pan");
        if !minimap.is_connected("pan_requested", &callable) {
            minimap.connect("pan_requested", &callable);
        }
        let callable = self.base().callable("_on_zoom");
        if !minimap.is_connected("zoom_changed", &callable) {
            minimap.connect("zoom_changed", &callable);
        }
        let callable = self.base().callable("_on_tapped");
        if !minimap.is_connected("tapped", &callable) {
            minimap.connect("tapped", &callable);
        }
    }
}

#[godot_api]
impl MindMinimapFragment {
    /// `MinimapFragment` right-drag: pan the camera by the widget's world delta.
    #[func(rename = _on_pan)]
    pub fn on_pan(&mut self, world_delta: Vector2) {
        let Some(mut camera) = self.camera() else {
            return;
        };
        let camera_pos = camera.get_position();
        camera.bind_mut().pan_to(
            f64::from(camera_pos.x + world_delta.x),
            f64::from(camera_pos.y + world_delta.y),
        );
    }

    /// `Renderer.scaleCamera`: `(factor / lastZoom - 1) * ZOOM_STEP_DIVISOR`.
    #[func(rename = _on_zoom)]
    pub fn on_zoom(&mut self, factor: f32) {
        let Some(mut camera) = self.camera() else {
            return;
        };
        let step = (factor / self.last_zoom.max(MIN_ZOOM_DIVISOR) - 1.0) * ZOOM_STEP_DIVISOR;
        self.last_zoom = factor;
        if step.abs() > ZOOM_EPSILON {
            camera.bind_mut().zoom_by(f64::from(step));
        }
    }

    /// Left-tap on the map: a second toggle closes the overlay
    /// (`MinimapFragment.toggle`).
    #[func(rename = _on_tapped)]
    pub fn on_tapped(&mut self) {
        self.toggle();
    }

    /// Shows/hides the overlay, raising it to the front while shown.
    #[func]
    pub fn toggle(&mut self) {
        let visible = !self.base().is_visible();
        self.base_mut().set_visible(visible);
        if visible {
            self.base_mut().move_to_front();
        }
    }
}

impl MindMinimapFragment {
    /// `/root/Spine/World/Camera2D` as the native camera host (the GDScript's
    /// `has_method("pan_to")` guard).
    fn camera(&self) -> Option<Gd<MindCamera2D>> {
        let camera = kit::node(CAMERA_PATH)?;
        camera.try_cast::<MindCamera2D>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_step_matches_scale_camera() {
        // A 1.1x ratio is a `0.4` step (Renderer.scaleCamera's *4 divisor).
        let ratio = 1.1f32;
        let step = (ratio - 1.0) * ZOOM_STEP_DIVISOR;
        assert!((step - 0.4).abs() < f32::EPSILON);
        // A zero last zoom never divides by zero.
        assert_eq!(0.0f32.max(MIN_ZOOM_DIVISOR), MIN_ZOOM_DIVISOR);
    }

    #[test]
    fn tiny_steps_are_dropped() {
        assert!(0.000_05f32.abs() <= ZOOM_EPSILON);
        assert!(0.000_2f32.abs() > ZOOM_EPSILON);
    }
}
