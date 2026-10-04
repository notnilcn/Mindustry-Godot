// SPDX-License-Identifier: GPL-3.0-only

//! `MindCamera2D` — the RTS camera rig node (plan 15 §3.11).
//!
//! All camera math lives in the Godot-free [`mind_core::input::CameraState`];
//! this node only polls Godot input, applies the transform and exposes the MCP
//! probe API. Ported from `core/src/mindustry/core/Renderer.java` (scale/shake),
//! `input/DesktopInput.java` (pan/zoom) and `input/MobileInput.java` (edge pan).
//! `Vars.tilesize = 8` comes from `mind_core::config`.

use godot::classes::notify::CanvasItemNotification;
use godot::classes::{Camera2D, ICamera2D, Input, InputEvent, InputEventMouseButton};
use godot::global::{Key, MouseButton};
use godot::obj::{Base, Singleton};
use godot::prelude::*;

use mind_core::config::TILESIZE;
use mind_core::fx::FxEvent;
use mind_core::input::{CameraState, MinimapRegion};

use crate::settings;

/// Default `screenshake` setting (`Renderer` reads it each frame).
const DEFAULT_SCREENSHAKE: i32 = 4;
/// Distance from a viewport edge that starts edge panning (px, upstream base).
const EDGE_MARGIN: f32 = 60.0;

/// 2D camera over the tile grid.
#[derive(GodotClass)]
#[class(base=Camera2D)]
pub struct MindCamera2D {
    base: Base<Camera2D>,
    /// Pure camera rig state.
    camera: CameraState,
    /// Edge pan only when the OS cursor is actually inside the game window.
    mouse_inside: bool,
    /// Monotonic render tick (deterministic shake direction seed).
    view_tick: u64,
    /// `screenshake` setting `0..=4`.
    screenshake: i32,
    /// Last `(x, y)` position exchanged with the Godot node (echo guard).
    shadow_position: (f32, f32),
    /// Last zoom exchanged with the Godot node (echo guard).
    shadow_scale: f32,
}

#[godot_api]
impl ICamera2D for MindCamera2D {
    fn init(base: Base<Camera2D>) -> Self {
        Self {
            base,
            camera: CameraState::new((1920.0, 1080.0)),
            mouse_inside: false,
            view_tick: 0,
            screenshake: DEFAULT_SCREENSHAKE,
            shadow_position: (0.0, 0.0),
            shadow_scale: 1.0,
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn process(&mut self, delta: f64) {
        // Shadow guard (godot-bevy `TransformSyncMetadata::shadow` pattern):
        // adopt any position Godot authored since our last write before stepping
        // the rig, so editor/direct-node moves are not immediately clobbered.
        let node_position = self.base().get_position();
        if node_position.x != self.shadow_position.0 || node_position.y != self.shadow_position.1 {
            self.camera.position = (node_position.x, node_position.y);
            self.shadow_position = (node_position.x, node_position.y);
        }

        self.view_tick = self.view_tick.wrapping_add(1);
        if let Some(viewport) = self.base().get_viewport() {
            let size = viewport.get_visible_rect().size;
            self.camera.viewport = (size.x, size.y);
        }

        let input = Input::singleton();
        let mut axis = Vector2::ZERO;
        if input.is_key_pressed(Key::A) || input.is_key_pressed(Key::LEFT) {
            axis.x -= 1.0;
        }
        if input.is_key_pressed(Key::D) || input.is_key_pressed(Key::RIGHT) {
            axis.x += 1.0;
        }
        if input.is_key_pressed(Key::W) || input.is_key_pressed(Key::UP) {
            axis.y -= 1.0;
        }
        if input.is_key_pressed(Key::S) || input.is_key_pressed(Key::DOWN) {
            axis.y += 1.0;
        }

        let delta_frames = (delta * 60.0) as f32;
        let boost = input.is_key_pressed(Key::SHIFT);
        self.camera.pan_axis(axis.x, axis.y, delta_frames, boost);

        if self.mouse_inside
            && let Some(viewport) = self.base().get_viewport()
        {
            let rect = viewport.get_visible_rect();
            let mouse = viewport.get_mouse_position();
            let margin = EDGE_MARGIN;
            if mouse.x < rect.position.x + margin
                || mouse.x > rect.position.x + rect.size.x - margin
                || mouse.y < rect.position.y + margin
                || mouse.y > rect.position.y + rect.size.y - margin
            {
                self.camera.auto_pan(mouse.x, mouse.y);
            }
        }

        self.camera.step_logic_cutscene(delta_frames);
        self.camera
            .update(delta_frames, self.screenshake, self.view_tick);

        let zoom = self.camera.camerascale.max(0.01);
        if zoom != self.shadow_scale {
            self.base_mut().set_zoom(Vector2::new(zoom, zoom));
            self.shadow_scale = zoom;
        }
        let (x, y) = self.camera.render_position();
        if x != self.shadow_position.0 || y != self.shadow_position.1 {
            self.base_mut().set_position(Vector2::new(x, y));
            self.shadow_position = (x, y);
        }
    }

    fn input(&mut self, event: Gd<InputEvent>) {
        let Ok(mouse) = event.try_cast::<InputEventMouseButton>() else {
            return;
        };
        if !mouse.is_pressed() {
            return;
        }
        let amount = match mouse.get_button_index() {
            MouseButton::WHEEL_UP => 1.0,
            MouseButton::WHEEL_DOWN => -1.0,
            _ => return,
        };
        // `Renderer.scaleCamera(Core.input.axisTap(Binding.zoom))`.
        self.camera.scale_camera(amount);
    }

    fn on_notification(&mut self, what: CanvasItemNotification) {
        match what {
            CanvasItemNotification::WM_MOUSE_ENTER => self.mouse_inside = true,
            CanvasItemNotification::WM_MOUSE_EXIT => self.mouse_inside = false,
            CanvasItemNotification::EXTENSION_RELOADED => self.bootstrap(),
            _ => {}
        }
    }
}

#[godot_api]
impl MindCamera2D {
    /// Rebuilds the rig's Godot-derived state (runs from `ready()` and on
    /// `EXTENSION_RELOADED`, which does not re-run `ready()`).
    fn bootstrap(&mut self) {
        self.base_mut().make_current();
        if let Some(saved) = settings::read()
            && let Some(zoom) = saved.zoom
        {
            self.camera.set_scale_immediate(zoom as f32);
        }
        let position = self.base().get_position();
        self.camera.position = (position.x, position.y);
        // Seed the echo-guard shadows from the node so no write is issued for
        // state Godot already owns.
        self.shadow_position = (position.x, position.y);
        self.shadow_scale = self.base().get_zoom().x;
        log::debug!(
            "MindCamera2D ready at {:?} zoom {:?}",
            self.base().get_position(),
            self.base().get_zoom()
        );
    }

    /// Viewport position → tile `(x, y)`.
    ///
    /// `(x, y)` is a viewport-space point (e.g. `InputEventMouse.position`);
    /// the tile is `floor(world / TILESIZE)`.
    #[func]
    pub fn screen_to_tile(&self, x: f64, y: f64) -> Vector2i {
        let canvas = self.base().get_canvas_transform();
        let world = canvas.affine_inverse() * Vector2::new(x as f32, y as f32);
        let size = TILESIZE as f32;
        Vector2i::new(
            (world.x / size).floor() as i32,
            (world.y / size).floor() as i32,
        )
    }

    /// Tile `(x, y)` → its **center** in viewport coordinates.
    ///
    /// This is the exact point to feed `godot_input mouse_button` with
    /// `coords: "viewport"` (see `§7c` step 6): `(x + 0.5, y + 0.5) * TILESIZE`
    /// transformed by the camera's canvas transform.
    #[func]
    pub fn tile_to_screen(&self, x: i32, y: i32) -> Vector2 {
        let world = Vector2::new(
            (x as f32 + 0.5) * TILESIZE as f32,
            (y as f32 + 0.5) * TILESIZE as f32,
        );
        self.base().get_canvas_transform() * world
    }

    /// Centers the camera on tile `(x, y)` (`tile + 0.5` center).
    #[func]
    pub fn center_on_tile(&mut self, x: i32, y: i32) {
        self.camera.center_on_tile(x, y);
        let (px, py) = self.camera.position;
        self.base_mut().set_position(Vector2::new(px, py));
        self.shadow_position = (px, py);
    }

    /// `InputHandler.panCamera`: force the camera center (world pixels).
    #[func]
    pub fn pan_to(&mut self, x: f64, y: f64) {
        self.camera.pan_camera(x as f32, y as f32);
        self.base_mut()
            .set_position(Vector2::new(x as f32, y as f32));
        self.shadow_position = (x as f32, y as f32);
    }

    /// `Renderer.scaleCamera(amount)` (relative zoom-by).
    #[func]
    pub fn zoom_by(&mut self, amount: f64) {
        self.camera.scale_camera(amount as f32);
    }

    /// `Renderer.setScale` (absolute target scale).
    #[func]
    pub fn set_camera_scale(&mut self, scale: f64) {
        self.camera.set_scale(scale as f32);
    }

    /// `Minimap` right-click: center on the mapped minimap fraction.
    #[func]
    pub fn minimap_pan(&mut self, fraction_x: f64, fraction_y: f64) {
        self.camera
            .minimap_pan(fraction_x as f32, fraction_y as f32, MinimapRegion::FULL);
        let (px, py) = self.camera.position;
        self.base_mut().set_position(Vector2::new(px, py));
    }

    /// `MinimapRenderer.zoomBy(amounty)`.
    #[func]
    pub fn minimap_zoom(&mut self, amount: f64) {
        self.camera.scale_camera(amount as f32);
    }

    /// Enables world-edge position clamping (`world.unitWidth/Height` px).
    #[func]
    pub fn set_world_size(&mut self, width: f64, height: f64) {
        self.camera.world_size = Some((width as f32, height as f32));
    }

    /// Pushes the `screenshake` setting (`0..=4`).
    #[func]
    pub fn set_screenshake(&mut self, value: i64) {
        self.screenshake = (value as i32).clamp(0, 4);
    }

    /// Consumes a plan-17 `ShakeEvent { intensity, duration }`.
    #[func]
    pub fn apply_shake_event(&mut self, intensity: f64, duration: f64) {
        self.camera.on_fx_event(&FxEvent::Shake {
            intensity: intensity as f32,
            duration: duration as f32,
        });
    }

    /// Locks the camera to a unit/position (`InputHandler.spectate`).
    #[func]
    pub fn spectate(&mut self, x: f64, y: f64) {
        self.camera.spectate(x as f32, y as f32);
        self.base_mut()
            .set_position(Vector2::new(x as f32, y as f32));
        self.shadow_position = (x as f32, y as f32);
    }

    /// `logicCutscene` pan+zoom (`zoom` `0..=1`, `-1` keeps the gameplay target).
    #[func]
    pub fn set_logic_cutscene(&mut self, pan_x: f64, pan_y: f64, zoom: f64, speed: f64) {
        self.camera
            .set_logic_cutscene((pan_x as f32, pan_y as f32), zoom as f32, speed as f32);
    }

    /// Ends `logicCutscene`.
    #[func]
    pub fn clear_logic_cutscene(&mut self) {
        self.camera.clear_logic_cutscene();
    }

    /// Current zoom factor (settings persistence helper; not part of the MCP API).
    pub fn zoom_value(&self) -> f64 {
        self.camera.camerascale as f64
    }

    /// The camera rig state as JSON (MCP `§7c` camera probes).
    #[func]
    pub fn camera_state_json(&self) -> GString {
        let value = serde_json::json!({
            "format": 1,
            "position": [self.camera.position.0, self.camera.position.1],
            "render_position": [
                self.camera.render_position().0,
                self.camera.render_position().1,
            ],
            "target_scale": self.camera.target_scale,
            "camerascale": self.camera.camerascale,
            "width": self.camera.camera_width(),
            "height": self.camera.camera_height(),
            "shake_offset": [self.camera.shake.offset.0, self.camera.shake.offset.1],
            "shake_intensity": self.camera.shake.intensity,
            "logic_cutscene": self.camera.logic_cutscene,
            "min_scale": self.camera.min_scale(),
            "max_scale": self.camera.max_scale(),
        });
        GString::from(&value.to_string())
    }
}
