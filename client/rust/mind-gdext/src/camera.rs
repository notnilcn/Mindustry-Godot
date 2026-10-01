// SPDX-License-Identifier: GPL-3.0-only

//! `MindCamera2D` — WASD/edge pan and wheel zoom over the tile world, plus the
//! screen↔tile conversion the MCP input oracle uses (`§3.5`/`§7c`).
//!
//! Ported from `core/src/mindustry/input/DesktopInput.java` (pan/zoom intent,
//! P0 subset); `Vars.tilesize = 8` comes from `mind_core::config`.

use godot::classes::notify::CanvasItemNotification;
use godot::classes::{Camera2D, ICamera2D, Input, InputEvent, InputEventMouseButton};
use godot::global::{Key, MouseButton};
use godot::obj::{Base, Singleton};
use godot::prelude::*;

use mind_core::config::TILESIZE;

use crate::settings;

/// Zoom clamp from the plan (`0.25–4.0`).
const MIN_ZOOM: f32 = 0.25;
/// Upper zoom clamp.
const MAX_ZOOM: f32 = 4.0;
/// Multiplicative wheel step.
const ZOOM_STEP: f32 = 1.1;
/// WASD pan speed in screen pixels per second (scaled by inverse zoom).
const PAN_SPEED: f32 = 420.0;
/// Distance from a viewport edge that starts edge panning.
const EDGE_MARGIN: f32 = 24.0;

/// 2D camera over the tile grid.
#[derive(GodotClass)]
#[class(base=Camera2D)]
pub struct MindCamera2D {
    base: Base<Camera2D>,
    /// Edge pan only when the OS cursor is actually inside the game window.
    mouse_inside: bool,
}

#[godot_api]
impl ICamera2D for MindCamera2D {
    fn init(base: Base<Camera2D>) -> Self {
        Self {
            base,
            mouse_inside: false,
        }
    }

    fn ready(&mut self) {
        self.base_mut().make_current();
        if let Some(saved) = settings::read()
            && let Some(zoom) = saved.zoom
        {
            let zoom = (zoom as f32).clamp(MIN_ZOOM, MAX_ZOOM);
            self.base_mut().set_zoom(Vector2::new(zoom, zoom));
        }
        log::debug!(
            "MindCamera2D ready at {:?} zoom {:?}",
            self.base().get_position(),
            self.base().get_zoom()
        );
    }

    fn process(&mut self, delta: f64) {
        let mut direction = Vector2::ZERO;
        let input = Input::singleton();
        if input.is_key_pressed(Key::A) || input.is_key_pressed(Key::LEFT) {
            direction.x -= 1.0;
        }
        if input.is_key_pressed(Key::D) || input.is_key_pressed(Key::RIGHT) {
            direction.x += 1.0;
        }
        if input.is_key_pressed(Key::W) || input.is_key_pressed(Key::UP) {
            direction.y -= 1.0;
        }
        if input.is_key_pressed(Key::S) || input.is_key_pressed(Key::DOWN) {
            direction.y += 1.0;
        }

        if self.mouse_inside
            && let Some(viewport) = self.base().get_viewport()
        {
            let rect = viewport.get_visible_rect();
            let mouse = viewport.get_mouse_position();
            if mouse.x < rect.position.x + EDGE_MARGIN {
                direction.x -= 1.0;
            }
            if mouse.x > rect.position.x + rect.size.x - EDGE_MARGIN {
                direction.x += 1.0;
            }
            if mouse.y < rect.position.y + EDGE_MARGIN {
                direction.y -= 1.0;
            }
            if mouse.y > rect.position.y + rect.size.y - EDGE_MARGIN {
                direction.y += 1.0;
            }
        }

        if direction == Vector2::ZERO {
            return;
        }
        let zoom = self.base().get_zoom().x.max(0.01);
        let movement = direction.normalized() * (PAN_SPEED / zoom) * delta as f32;
        let position = self.base().get_position() + movement;
        self.base_mut().set_position(position);
    }

    fn input(&mut self, event: Gd<InputEvent>) {
        let Ok(mouse) = event.try_cast::<InputEventMouseButton>() else {
            return;
        };
        if !mouse.is_pressed() {
            return;
        }
        let factor = match mouse.get_button_index() {
            MouseButton::WHEEL_UP => ZOOM_STEP,
            MouseButton::WHEEL_DOWN => 1.0 / ZOOM_STEP,
            _ => return,
        };
        let zoom = self.base().get_zoom().x;
        let next = (zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        self.base_mut().set_zoom(Vector2::new(next, next));
    }

    fn on_notification(&mut self, what: CanvasItemNotification) {
        match what {
            CanvasItemNotification::WM_MOUSE_ENTER => self.mouse_inside = true,
            CanvasItemNotification::WM_MOUSE_EXIT => self.mouse_inside = false,
            _ => {}
        }
    }
}

#[godot_api]
impl MindCamera2D {
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

    /// Centers the camera on tile `(x, y)`.
    #[func]
    pub fn center_on_tile(&mut self, x: i32, y: i32) {
        let world = Vector2::new(
            (x as f32 + 0.5) * TILESIZE as f32,
            (y as f32 + 0.5) * TILESIZE as f32,
        );
        self.base_mut().set_position(world);
    }

    /// Current zoom factor (settings persistence helper; not part of the MCP API).
    pub fn zoom_value(&self) -> f64 {
        self.base().get_zoom().x as f64
    }
}
