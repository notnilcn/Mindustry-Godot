// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Pure camera rig math (plan 15 §3.11).
//!
//! Ported from `core/src/mindustry/core/Renderer.java` (scale/lerp/clamp/shake),
//! `input/DesktopInput.java` (pan intent), `input/MobileInput.java` (drag pan +
//! edge auto-pan + clamp) and `input/InputHandler.java` (`panCamera`/`spectate`/
//! `logicCutscene`). This module is Godot-free and view-only: nothing here ever
//! feeds the simulation or the checksum (plan invariant I1–I3). `mind-gdext`'s
//! `MindCamera2D` drives a [`CameraState`] and applies the rendered transform.

use crate::config::TILESIZE;
use crate::fx::{FxEvent, ShakeState};

/// `Renderer` camera scale lerp factor.
pub const CAMERA_LERP: f32 = 0.1;
/// `Renderer` snap epsilon (`Mathf.equal(camerascale, dest, 0.001f)`).
pub const CAMERA_SNAP: f32 = 0.001;
/// `smoothcamera` follow lerp factor.
pub const SMOOTH_CAMERA_LERP: f32 = 0.08;
/// Cutscene zoom clamp (`Renderer.minZoom`).
pub const MIN_ZOOM: f32 = 1.5;
/// Cutscene zoom clamp (`Renderer.maxZoom`).
pub const MAX_ZOOM: f32 = 6.0;
/// Gameplay zoom-out clamp (`Renderer.minZoomInGame`).
pub const MIN_ZOOM_IN_GAME: f32 = 0.5;
/// Gameplay zoom-in clamp (`Renderer.maxZoomInGame`).
pub const MAX_ZOOM_IN_GAME: f32 = 6.0;
/// `DesktopInput.panScale`.
pub const PAN_SCALE: f32 = 0.005;
/// `DesktopInput.panSpeed`.
pub const PAN_SPEED: f32 = 4.5;
/// `DesktopInput.panBoostSpeed`.
pub const PAN_BOOST_SPEED: f32 = 15.0;
/// `MobileInput` camera-move speed.
pub const MOBILE_CAM_SPEED: f32 = 6.0;
/// `MobileInput.edgePan` base (`Scl.scl(60)`).
pub const EDGE_PAN: f32 = 60.0;
/// `MobileInput.maxPanSpeed`.
pub const MAX_PAN_SPEED: f32 = 1.3;
/// `Renderer.scaleCamera`: `amount / 4 + 1`.
pub const ZOOM_STEP_DIVISOR: f32 = 4.0;

/// Linear interpolation (`Mathf.lerp`).
pub fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

/// `Mathf.lerpDelta`: lerp with `clamp(progress * delta)`.
pub fn lerp_delta(from: f32, to: f32, progress: f32, delta: f32) -> f32 {
    lerp(from, to, (progress * delta).clamp(0.0, 1.0))
}

/// A minimap source region (`MinimapRenderer.getRegion`), in normalized map UV.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MinimapRegion {
    /// Left U.
    pub u: f32,
    /// Bottom V.
    pub v: f32,
    /// Right U.
    pub u2: f32,
    /// Top V.
    pub v2: f32,
}

impl MinimapRegion {
    /// The whole map (`u=0, v=0, u2=1, v2=1`).
    pub const FULL: MinimapRegion = MinimapRegion {
        u: 0.0,
        v: 0.0,
        u2: 1.0,
        v2: 1.0,
    };
}

/// The resolved camera view handed to plan 16 (`Renderer.camera` subset).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CameraView {
    /// Camera center x in world pixels (shake included).
    pub x: f32,
    /// Camera center y in world pixels (shake included).
    pub y: f32,
    /// `camera.width`.
    pub width: f32,
    /// `camera.height`.
    pub height: f32,
    /// Current display scale (`renderer.getDisplayScale()`).
    pub scale: f32,
    /// Screen-shake offset `(x, y)`.
    pub shake: (f32, f32),
}

/// Full RTS camera rig state (`Renderer` camera globals).
#[derive(Debug, Clone)]
pub struct CameraState {
    /// `targetscale` (lerped toward by `camerascale`).
    pub target_scale: f32,
    /// `camerascale` (current actual scale).
    pub camerascale: f32,
    /// `camera.position` in world pixels.
    pub position: (f32, f32),
    /// Viewport size in pixels (`graphics.getWidth/Height`).
    pub viewport: (f32, f32),
    /// World size in pixels (`world.unitWidth/Height`); `None` disables clamping.
    pub world_size: Option<(f32, f32)>,
    /// UI scale factor (`Scl.scl`); `1.0` headless / at 100% UI scale.
    pub scl: f32,
    /// `InputHandler.logicCutscene`.
    pub logic_cutscene: bool,
    /// `InputHandler.logicCutsceneZoom` (`-1` = governed by gameplay target).
    pub logic_cutscene_zoom: f32,
    /// `InputHandler.logicCamPan`.
    pub logic_cam_pan: (f32, f32),
    /// `InputHandler.logicCamSpeed`.
    pub logic_cam_speed: f32,
    /// Screen-shake accumulator (plan 17).
    pub shake: ShakeState,
    /// `minzoomingamemultiplier` (`Renderer.minZoomInGame` divisor).
    pub min_zoom_multiplier: f32,
    /// `maxzoomingamemultiplier` (`Renderer.maxZoomInGame` multiplier).
    pub max_zoom_multiplier: f32,
    /// Resolved `minZoomInGame` (settings applied) from the last [`update`].
    pub min_zoom_in_game: f32,
    /// Resolved `maxZoomInGame` (settings applied) from the last [`update`].
    pub max_zoom_in_game: f32,
}

impl Default for CameraState {
    fn default() -> Self {
        Self::new((1920.0, 1080.0))
    }
}

impl CameraState {
    /// Creates a rig with `targetscale = camerascale = Scl.scl(4)`.
    pub fn new(viewport: (f32, f32)) -> Self {
        let scale = 4.0;
        Self {
            target_scale: scale,
            camerascale: scale,
            position: (0.0, 0.0),
            viewport,
            world_size: None,
            scl: 1.0,
            logic_cutscene: false,
            logic_cutscene_zoom: -1.0,
            logic_cam_pan: (0.0, 0.0),
            logic_cam_speed: 0.1,
            shake: ShakeState::default(),
            min_zoom_multiplier: 1.0,
            max_zoom_multiplier: 1.0,
            min_zoom_in_game: MIN_ZOOM_IN_GAME,
            max_zoom_in_game: MAX_ZOOM_IN_GAME,
        }
    }

    /// `Renderer.minScale()`.
    pub fn min_scale(&self) -> f32 {
        self.scl
            * if self.logic_cutscene {
                MIN_ZOOM
            } else {
                self.min_zoom_in_game
            }
    }

    /// `Renderer.maxScale()` (`Mathf.round`).
    pub fn max_scale(&self) -> f32 {
        let base = if self.logic_cutscene {
            MAX_ZOOM
        } else {
            self.max_zoom_in_game
        };
        (self.scl * base).round()
    }

    /// `Renderer.clampScale()`.
    pub fn clamp_scale(&mut self) {
        self.target_scale = self.target_scale.clamp(self.min_scale(), self.max_scale());
    }

    /// `Renderer.scaleCamera(amount)`: `targetscale *= amount / 4 + 1`.
    pub fn scale_camera(&mut self, amount: f32) {
        self.target_scale *= amount / ZOOM_STEP_DIVISOR + 1.0;
        self.clamp_scale();
    }

    /// `Renderer.setScale`.
    pub fn set_scale(&mut self, scale: f32) {
        self.target_scale = scale;
        self.clamp_scale();
    }

    /// `Renderer.setScale` with an immediate (unsmoothed) display scale.
    pub fn set_scale_immediate(&mut self, scale: f32) {
        self.set_scale(scale);
        self.camerascale = self.target_scale;
    }

    /// `camera.width`.
    pub fn camera_width(&self) -> f32 {
        self.viewport.0 / self.camerascale.max(f32::EPSILON)
    }

    /// `camera.height`.
    pub fn camera_height(&self) -> f32 {
        self.viewport.1 / self.camerascale.max(f32::EPSILON)
    }

    /// `Renderer.update()` scale/shake half. `delta` is in 60 Hz frames
    /// (`Time.delta`), `screenshake` the `0..=4` setting, `view_tick` the
    /// deterministic shake direction seed.
    pub fn update(&mut self, delta: f32, screenshake: i32, view_tick: u64) {
        let base_target = if self.logic_cutscene && self.logic_cutscene_zoom >= 0.0 {
            lerp(MIN_ZOOM, MAX_ZOOM, self.logic_cutscene_zoom)
        } else {
            self.target_scale
        };
        let dest = base_target.clamp(self.min_scale(), self.max_scale());
        self.camerascale = lerp_delta(self.camerascale, dest, CAMERA_LERP, delta);
        if (self.camerascale - dest).abs() <= CAMERA_SNAP {
            self.camerascale = dest;
        }

        // `Renderer.update` applies the settings multipliers after the lerp; keep
        // the upstream order so the first frame after a settings change is exact.
        self.max_zoom_in_game = self.max_zoom_multiplier * MAX_ZOOM;
        self.min_zoom_in_game = MIN_ZOOM / self.min_zoom_multiplier.max(f32::EPSILON);

        self.shake.offset(screenshake, view_tick);
    }

    /// Applies a plan-17 [`FxEvent`] (the `ShakeEvent { intensity, duration }`
    /// contract); non-shake events are ignored.
    pub fn on_fx_event(&mut self, event: &FxEvent) -> bool {
        if let FxEvent::Shake {
            intensity,
            duration,
        } = event
        {
            self.shake.add(*intensity, duration.max(0.0));
            true
        } else {
            false
        }
    }

    /// `Renderer` frame offset. `screenshake` is the `0..=4` setting.
    pub fn shake_offset(&mut self, screenshake: i32, view_tick: u64) -> (f32, f32) {
        self.shake.offset(screenshake, view_tick)
    }

    /// The camera center including the current shake offset.
    pub fn render_position(&self) -> (f32, f32) {
        (
            self.position.0 + self.shake.offset.0,
            self.position.1 + self.shake.offset.1,
        )
    }

    /// The plan-16 view snapshot.
    pub fn view(&self) -> CameraView {
        let (x, y) = self.render_position();
        CameraView {
            x,
            y,
            width: self.camera_width(),
            height: self.camera_height(),
            scale: self.camerascale,
            shake: self.shake.offset,
        }
    }

    /// `DesktopInput.update` axis pan (`moveX`/`moveY`), normalized.
    pub fn pan_axis(&mut self, axis_x: f32, axis_y: f32, delta: f32, boost: bool) {
        let len = axis_x.hypot(axis_y);
        if len <= f32::EPSILON {
            return;
        }
        let speed = if boost { PAN_BOOST_SPEED } else { PAN_SPEED } * delta;
        self.position.0 += axis_x / len * speed;
        self.position.1 += axis_y / len * speed;
    }

    /// `DesktopInput.update` mouse-offset pan (`pan`/`mouseMove`, `panScale`).
    pub fn pan_mouse(&mut self, mouse_x: f32, mouse_y: f32, delta: f32, boost: bool) {
        let speed = if boost { PAN_BOOST_SPEED } else { PAN_SPEED } * delta;
        self.position.0 += ((mouse_x - self.viewport.0 / 2.0) * PAN_SCALE).clamp(-1.0, 1.0) * speed;
        self.position.1 += ((mouse_y - self.viewport.1 / 2.0) * PAN_SCALE).clamp(-1.0, 1.0) * speed;
    }

    /// `InputHandler.spectate`/`DesktopInput` smooth follow. `smooth` uses the
    /// `smoothcamera` lerp (`0.08`), otherwise a snap (`1.0`).
    pub fn follow(&mut self, target_x: f32, target_y: f32, smooth: bool, delta: f32) {
        let alpha = if smooth { SMOOTH_CAMERA_LERP } else { 1.0 };
        self.position.0 = lerp_delta(self.position.0, target_x, alpha, delta);
        self.position.1 = lerp_delta(self.position.1, target_y, alpha, delta);
    }

    /// `MobileInput.update` keyboard-less camera move.
    pub fn mobile_move(&mut self, axis_x: f32, axis_y: f32, delta: f32) {
        let len = axis_x.hypot(axis_y);
        if len <= f32::EPSILON {
            return;
        }
        let speed = MOBILE_CAM_SPEED * delta;
        self.position.0 += axis_x / len * speed;
        self.position.1 += axis_y / len * speed;
    }

    /// `MobileInput.pan`: drag delta in viewport pixels, scaled by
    /// `camera.width / viewport.width`, then clamped.
    pub fn mobile_pan(&mut self, delta_x: f32, delta_y: f32) {
        let scale = self.camera_width() / self.viewport.0;
        self.position.0 -= delta_x * scale;
        self.position.1 -= delta_y * scale;
        self.clamp_position();
    }

    /// `MobileInput.autoPan`: screen-edge pan, scaled, limited to
    /// [`MAX_PAN_SPEED`].
    pub fn auto_pan(&mut self, mouse_x: f32, mouse_y: f32) {
        let edge = EDGE_PAN * self.scl;
        let mut pan_x = 0.0;
        let mut pan_y = 0.0;
        if mouse_x <= edge {
            pan_x = -(edge - mouse_x);
        }
        if mouse_x >= self.viewport.0 - edge {
            pan_x = (mouse_x - self.viewport.0) + edge;
        }
        if mouse_y <= edge {
            pan_y = -(edge - mouse_y);
        }
        if mouse_y >= self.viewport.1 - edge {
            pan_y = (mouse_y - self.viewport.1) + edge;
        }
        let scale = self.camera_width() / self.viewport.0;
        let mut vx = pan_x * scale;
        let mut vy = pan_y * scale;
        let len = vx.hypot(vy);
        if len > MAX_PAN_SPEED {
            vx = vx / len * MAX_PAN_SPEED;
            vy = vy / len * MAX_PAN_SPEED;
        }
        self.position.0 += vx;
        self.position.1 += vy;
    }

    /// `MobileInput.pan` clamp: `[-w/4, -h/4, world + w/4, world + h/4]`.
    pub fn clamp_position(&mut self) {
        let Some((world_w, world_h)) = self.world_size else {
            return;
        };
        let w = self.camera_width() / 4.0;
        let h = self.camera_height() / 4.0;
        self.position.0 = self.position.0.clamp(-w, world_w + w);
        self.position.1 = self.position.1.clamp(-h, world_h + h);
    }

    /// `InputHandler.panCamera`: force the center (not gated on locks here; the
    /// caller applies [`crate::input::FocusGuards`]).
    pub fn pan_camera(&mut self, x: f32, y: f32) {
        self.position = (x, y);
    }

    /// Centers on tile `(x, y)` (tile center).
    pub fn center_on_tile(&mut self, x: i32, y: i32) {
        self.position = (
            (x as f32 + 0.5) * TILESIZE as f32,
            (y as f32 + 0.5) * TILESIZE as f32,
        );
    }

    /// `InputHandler.spectate`/lock-to-target: snap the center to a world point.
    pub fn spectate(&mut self, x: f32, y: f32) {
        self.position = (x, y);
    }

    /// `InputHandler.logicCutscene` pan+zoom. `zoom` is the `0..=1` blend (or
    /// `-1` to keep the gameplay target).
    pub fn set_logic_cutscene(&mut self, pan: (f32, f32), zoom: f32, speed: f32) {
        self.logic_cutscene = true;
        self.logic_cam_pan = pan;
        self.logic_cutscene_zoom = zoom;
        self.logic_cam_speed = speed;
    }

    /// `InputHandler.update` cutscene pan step.
    pub fn step_logic_cutscene(&mut self, delta: f32) {
        if self.logic_cutscene {
            self.position.0 = lerp_delta(
                self.position.0,
                self.logic_cam_pan.0,
                self.logic_cam_speed,
                delta,
            );
            self.position.1 = lerp_delta(
                self.position.1,
                self.logic_cam_pan.1,
                self.logic_cam_speed,
                delta,
            );
        }
    }

    /// `InputHandler` cutscene clear (restores gameplay zoom).
    pub fn clear_logic_cutscene(&mut self) {
        self.logic_cutscene = false;
        self.logic_cutscene_zoom = -1.0;
    }

    /// `Minimap.java` right-click mapping: a `0..=1` fraction of the minimap
    /// widget to a world-pixel point (`Mathf.lerp` over the region UV, scaled by
    /// the world size).
    pub fn minimap_target(
        &self,
        fraction_x: f32,
        fraction_y: f32,
        region: MinimapRegion,
    ) -> (f32, f32) {
        let (world_w, world_h) = self.world_size.unwrap_or((0.0, 0.0));
        (
            lerp(region.u, region.u2, fraction_x) * world_w,
            lerp(1.0 - region.v2, 1.0 - region.v, fraction_y) * world_h,
        )
    }

    /// `Minimap.java` right-click: center the camera on the mapped point.
    pub fn minimap_pan(&mut self, fraction_x: f32, fraction_y: f32, region: MinimapRegion) {
        let (x, y) = self.minimap_target(fraction_x, fraction_y, region);
        self.pan_camera(x, y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rig() -> CameraState {
        let mut state = CameraState::new((1920.0, 1080.0));
        state.world_size = Some((256.0, 256.0));
        state
    }

    #[test]
    fn scale_camera_step_and_clamp() {
        let mut state = rig();
        assert_eq!(state.target_scale, 4.0);
        // amount = 1 -> *1.25 (upstream `Renderer.scaleCamera`).
        state.scale_camera(1.0);
        assert!((state.target_scale - 5.0).abs() < 1e-6);
        state.scale_camera(-1.0);
        assert!((state.target_scale - 3.75).abs() < 1e-6);
        // Clamp to the gameplay max (6) and min (0.5).
        for _ in 0..20 {
            state.scale_camera(1.0);
        }
        assert!((state.target_scale - state.max_scale()).abs() < 1e-6);
        for _ in 0..40 {
            state.scale_camera(-1.0);
        }
        assert!((state.target_scale - state.min_scale()).abs() < 1e-6);
    }

    #[test]
    fn update_lerps_then_snaps() {
        let mut state = rig();
        state.set_scale(6.0);
        state.update(1.0, 4, 1);
        assert!((state.camerascale - lerp(4.0, 6.0, 0.1)).abs() < 1e-5);
        // A large delta lands exactly on the target (clamped progress = 1).
        state.update(1000.0, 4, 1);
        assert!((state.camerascale - 6.0).abs() < 1e-6);
        // Viewport-derived dimensions follow the display scale.
        assert!((state.camera_width() - 320.0).abs() < 1e-4);
    }

    #[test]
    fn pan_axis_is_normalized_and_speeds() {
        let mut state = rig();
        state.pan_axis(1.0, 1.0, 1.0, false);
        let expect = PAN_SPEED / 2f32.sqrt();
        assert!((state.position.0 - expect).abs() < 1e-5);
        assert!((state.position.1 - expect).abs() < 1e-5);
        state.pan_axis(0.0, 0.0, 1.0, false);
        assert!((state.position.0 - expect).abs() < 1e-5);
        state.pan_axis(1.0, 0.0, 1.0, true);
        assert!((state.position.0 - (expect + PAN_BOOST_SPEED)).abs() < 1e-5);
    }

    #[test]
    fn mouse_pan_clamps_to_unit() {
        let mut state = rig();
        // Far right: factor clamps to +1.
        state.pan_mouse(10_000.0, 540.0, 1.0, false);
        assert!((state.position.0 - PAN_SPEED).abs() < 1e-5);
        assert!(state.position.1.abs() < 1e-6);
        // Center: no pan.
        state.pan_mouse(960.0, 540.0, 1.0, false);
        assert!((state.position.0 - PAN_SPEED).abs() < 1e-5);
    }

    #[test]
    fn mobile_pan_scales_and_clamps() {
        let mut state = rig();
        state.set_scale_immediate(4.0);
        // camera.width = 480 -> scale = 480 / 1920 = 0.25.
        state.mobile_pan(48.0, 0.0);
        assert!((state.position.0 - (-12.0)).abs() < 1e-5);
        // Pushing way past the world clamps at world + camera.width/4.
        for _ in 0..100 {
            state.mobile_pan(-10_000.0, 0.0);
        }
        let max = 256.0 + state.camera_width() / 4.0;
        assert!((state.position.0 - max).abs() < 1e-3);
    }

    #[test]
    fn auto_pan_edge_direction_and_limit() {
        let mut state = rig();
        // Left edge -> negative x.
        state.auto_pan(0.0, 540.0);
        assert!(state.position.0 < 0.0);
        // Right edge -> positive x.
        let before = state.position.0;
        state.auto_pan(1920.0, 540.0);
        assert!(state.position.0 > before);
        // The step never exceeds maxPanSpeed in magnitude.
        let mut fresh = rig();
        fresh.auto_pan(0.0, 540.0);
        assert!(fresh.position.0.abs() <= MAX_PAN_SPEED + 1e-5);
    }

    #[test]
    fn follow_smooth_vs_snap() {
        let mut smooth = rig();
        smooth.follow(100.0, 100.0, true, 1.0);
        assert!((smooth.position.0 - 8.0).abs() < 1e-5);
        let mut snap = rig();
        snap.follow(100.0, 100.0, false, 1.0);
        assert!((snap.position.0 - 100.0).abs() < 1e-5);
    }

    #[test]
    fn cutscene_zoom_and_pan() {
        let mut state = rig();
        state.set_logic_cutscene((40.0, 40.0), 0.5, 0.1);
        assert!(state.logic_cutscene);
        // baseTarget = lerp(1.5, 6, 0.5) = 3.75; min/max are the cutscene clamps.
        state.update(1000.0, 0, 1);
        assert!((state.target_scale - 4.0).abs() < 1e-6); // target unchanged
        assert!((state.camerascale - 3.75).abs() < 1e-4);
        state.step_logic_cutscene(1.0);
        assert!((state.position.0 - 4.0).abs() < 1e-5);
        state.clear_logic_cutscene();
        assert!(!state.logic_cutscene);
        assert_eq!(state.logic_cutscene_zoom, -1.0);
    }

    #[test]
    fn shake_consumed_from_fx_event_and_decays() {
        let mut state = rig();
        assert!(state.on_fx_event(&FxEvent::Shake {
            intensity: 10.0,
            duration: 5.0,
        }));
        assert_eq!(state.shake.intensity, 10.0);
        let first = state.shake_offset(4, 7);
        assert!(first.0.hypot(first.1) > 0.0);
        // After the duration the offset returns to the baseline.
        for _ in 0..10 {
            state.shake_offset(4, 7);
        }
        assert_eq!(state.shake.offset, (0.0, 0.0));
    }

    #[test]
    fn minimap_target_maps_fraction() {
        let state = rig();
        let (x, y) = state.minimap_target(0.5, 0.5, MinimapRegion::FULL);
        assert!((x - 128.0).abs() < 1e-4);
        // y uses `1 - v2 .. 1 - v`, so the top of the widget maps to world y=0.
        assert!((y - 128.0).abs() < 1e-4);
        let (x0, y0) = state.minimap_target(0.0, 1.0, MinimapRegion::FULL);
        assert!(x0.abs() < 1e-6);
        // Bottom of the widget maps to `world.height * tilesize`.
        assert!((y0 - 256.0).abs() < 1e-4);
        // Top of the widget maps to world y = 0.
        let (_, y_top) = state.minimap_target(0.0, 0.0, MinimapRegion::FULL);
        assert!(y_top.abs() < 1e-6);
    }
}
