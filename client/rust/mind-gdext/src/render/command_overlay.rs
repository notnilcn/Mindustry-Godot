// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Command-mode selection overlay (`InputHandler.drawCommanded`; T16/PAR-0157).
//!
//! While command mode is on and units are selected, Java draws per unit
//! (`InputHandler.java:1211-1300`): a `Fill.lightInner` hexagon fan (centre
//! transparent, rim `color.a(0.7)`), a `Lines.poly` ring at `rad + 0.5` with
//! stroke 1, and — when a move command exists — a `Drawf.limitLine` to the
//! target (`color.a(0.5)`, shortened by `rad`) plus a 3.5-half-size
//! `Drawf.square` at the destination. `rad = hitSize / unitSelectRadScl + 1`.
//! Colour is `Pal.accent` (`#ffd37f`). The pass lives at `Layer.plans`.

use godot::classes::notify::CanvasItemNotification;
use godot::classes::{INode2D, Node2D};
use godot::obj::Base;
use godot::prelude::*;

use crate::input::MindInput;
use crate::sim_host::MindSimHost;

/// `Pal.accent` (`Pal.java`), the command overlay colour.
const ACCENT: (f32, f32, f32) = (1.0, 0.827, 0.498);
/// `InputHandler.unitSelectRadScl`.
const UNIT_SELECT_RAD_SCL: f32 = 1.0;
/// `InputHandler.drawCommanded`: hexagon sides and fill alpha.
const SIDES: u8 = 6;
const ALPHA: f32 = 0.5;

/// World-space command overlay (`Node2D` under the `Layer.plans` band).
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct CommandOverlay {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
    input: Option<Gd<MindInput>>,
}

#[godot_api]
impl INode2D for CommandOverlay {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            host: None,
            input: None,
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn on_notification(&mut self, what: CanvasItemNotification) {
        if what == CanvasItemNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }

    fn process(&mut self, _delta: f64) {
        let Some(input) = self.input.clone() else {
            return;
        };
        if input.bind().command_mode() {
            self.base_mut().queue_redraw();
        }
    }

    fn draw(&mut self) {
        let Some(input) = self.input.clone() else {
            return;
        };
        if !input.bind().command_mode() {
            return;
        }
        let ids: Vec<i32> =
            serde_json::from_str(&input.bind().selection_json().to_string()).unwrap_or_default();
        if ids.is_empty() {
            return;
        }
        let Some(host) = self.host.clone() else {
            return;
        };
        let rows = host.bind().unit_command_view(&ids);
        if rows.is_empty() {
            return;
        }
        let accent = Color::from_rgba(ACCENT.0, ACCENT.1, ACCENT.2, 1.0);
        let mut base = self.base_mut();
        for (x, y, hit_size, _flying, target) in rows {
            let rad = hit_size / UNIT_SELECT_RAD_SCL + 1.0;
            // `Fill.lightInner(x, y, 6, max(0, rad * 0.8), rad, 0, a(0), a(0.7))`:
            // a centre fan, transparent in the middle, accent at the rim.
            let clear = Color::from_rgba(0.0, 0.0, 0.0, 0.0);
            let rim = Color::from_rgba(ACCENT.0, ACCENT.1, ACCENT.2, 0.7);
            let ring = hexagon(x, y, rad);
            let mut points: Vec<Vector2> = Vec::with_capacity(ring.len() + 1);
            let mut colors: Vec<Color> = Vec::with_capacity(ring.len() + 1);
            points.push(Vector2::new(x, y));
            colors.push(clear);
            for point in &ring {
                points.push(*point);
                colors.push(rim);
            }
            let packed_points: PackedVector2Array = points.into_iter().collect();
            let packed_colors: PackedColorArray = colors.into_iter().collect();
            base.draw_polygon(&packed_points, &packed_colors);
            // `Lines.stroke(1); Lines.poly(x, y, 6, rad + 0.5)`.
            let mut ring_closed = hexagon(x, y, rad + 0.5);
            if let Some(first) = ring_closed.first().copied() {
                ring_closed.push(first);
            }
            let closed: PackedVector2Array = ring_closed.into_iter().collect();
            base.draw_polyline_ex(&closed, accent).width(1.0).done();
            // `Drawf.limitLine(unit, dest, rad, 6.5, color.a(0.5))` + the
            // destination square (3.5 half-size) when no attack target.
            if let Some((tx, ty)) = target {
                let (dx, dy) = (tx - x, ty - y);
                let len = (dx * dx + dy * dy).sqrt().max(0.001);
                let (sx, sy) = (x + dx / len * rad, y + dy / len * rad);
                let line = Color::from_rgba(ACCENT.0, ACCENT.1, ACCENT.2, ALPHA);
                base.draw_line_ex(Vector2::new(sx, sy), Vector2::new(tx, ty), line)
                    .width(1.0)
                    .done();
                let rect = Rect2::new(Vector2::new(tx - 3.5, ty - 3.5), Vector2::new(7.0, 7.0));
                base.draw_rect_ex(rect, line).filled(false).done();
            }
        }
    }
}

impl CommandOverlay {
    /// Rebuilds Godot-derived refs (`ready` + hot reload).
    fn bootstrap(&mut self) {
        self.host = self
            .base()
            .try_get_node_as::<MindSimHost>("/root/Spine/SimHost");
        self.input = self
            .base()
            .try_get_node_as::<MindInput>("/root/Spine/Input");
        self.base_mut().queue_redraw();
    }
}

/// `Lines.poly`/`Fill.poly` hexagon ring at `rotation = 0`.
fn hexagon(x: f32, y: f32, radius: f32) -> Vec<Vector2> {
    (0..SIDES)
        .map(|i| {
            let angle = i as f32 * std::f32::consts::TAU / SIDES as f32;
            Vector2::new(x + angle.cos() * radius, y + angle.sin() * radius)
        })
        .collect()
}
