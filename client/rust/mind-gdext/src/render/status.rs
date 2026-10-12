// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! F6 block status markers (`BuildingComp.drawStatus`; T14/PAR-0097).
//!
//! Java draws a `Pal.gray` diamond plus the `BlockStatus` colour diamond at
//! the bottom-right corner of every consuming building while the
//! `blockstatus` setting is on and `Lod.l2` is active
//! (`Renderer.java:170`, `BlockRenderer.java:825-833`,
//! `BuildingComp.java:1221-1234`).

use godot::classes::notify::CanvasItemNotification;
use godot::classes::{INode2D, Node2D};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::render::block_status::status_corner;

use crate::sim_host::MindSimHost;

/// `Pal.gray` (`Pal.java:63`) backing diamond.
const GRAY: (u8, u8, u8) = (0x45, 0x45, 0x45);

/// One 45° square (Arc `Fill.square(x, y, radius, 45)`: half-width `radius`).
fn square_points(cx: f32, cy: f32, radius: f32) -> PackedVector2Array {
    // A square with half-width `radius` has a circumradius of `radius * sqrt2`.
    let circum = radius * std::f32::consts::SQRT_2;
    let mut out: Vec<Vector2> = Vec::with_capacity(4);
    for i in 0..4 {
        let angle = std::f32::consts::FRAC_PI_4 + i as f32 * std::f32::consts::TAU / 4.0;
        out.push(Vector2::new(
            cx + angle.cos() * circum,
            cy + angle.sin() * circum,
        ));
    }
    out.into_iter().collect()
}

/// F6 status-marker pass (`Node2D` under the `Layer.power` band).
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct StatusRenderer {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
}

#[godot_api]
impl INode2D for StatusRenderer {
    fn init(base: Base<Node2D>) -> Self {
        Self { base, host: None }
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
        if self.enabled() {
            // Positions are static but the pulse colour is not; the marker set
            // changes with placement, so redraw while visible.
            self.base_mut().queue_redraw();
        }
    }

    fn draw(&mut self) {
        if !self.enabled() || !self.lod_l2() {
            return;
        }
        let Some(host) = self.host.clone() else {
            return;
        };
        let rows = host.bind().building_statuses();
        if rows.is_empty() {
            return;
        }
        let visible = self.visible_rect();
        let mut base = self.base_mut();
        for (_tx, _ty, x, y, size, rgba) in rows {
            // Java only draws buildings `wasVisible`; the camera rect is the
            // port's cheap stand-in (`BlockRenderer.java:825-833`).
            if let Some((min_x, min_y, max_x, max_y)) = visible
                && (x < min_x || y < min_y || x > max_x || y > max_y)
            {
                continue;
            }
            let (cx, cy, m) = status_corner(x, y, size);
            base.draw_colored_polygon(
                &square_points(cx, cy, 2.5 * m),
                Color::from_rgba8(GRAY.0, GRAY.1, GRAY.2, 0xff),
            );
            base.draw_colored_polygon(
                &square_points(cx, cy, 1.5 * m),
                Color::from_rgba8(
                    ((rgba >> 24) & 0xff) as u8,
                    ((rgba >> 16) & 0xff) as u8,
                    ((rgba >> 8) & 0xff) as u8,
                    (rgba & 0xff) as u8,
                ),
            );
        }
    }
}

impl StatusRenderer {
    /// Rebuilds Godot-derived refs (`ready` + hot reload).
    fn bootstrap(&mut self) {
        self.host = self
            .base()
            .try_get_node_as::<MindSimHost>("/root/Spine/SimHost");
        self.base_mut().queue_redraw();
    }

    /// `blockstatus` setting (`MindUi.settings_get_bool`).
    fn enabled(&self) -> bool {
        self.base()
            .try_get_node_as::<Node>("/root/MindUi")
            .map(|mut ui| {
                ui.call(
                    "settings_get_bool",
                    &[GString::from("blockstatus").to_variant()],
                )
                .booleanize()
            })
            .unwrap_or(false)
    }

    /// `Lod.l2` from the world renderer's stats (`BlockRenderer` only draws
    /// status markers at L2).
    fn lod_l2(&self) -> bool {
        self.base()
            .try_get_node_as::<Node>("/root/Spine/World/Renderer")
            .map(|mut renderer| renderer.call("stats_lod_l2", &[]).booleanize())
            .unwrap_or(false)
    }

    /// World-space visible rect from the camera transform (`None` without a
    /// camera).
    fn visible_rect(&self) -> Option<(f32, f32, f32, f32)> {
        let camera = self
            .base()
            .try_get_node_as::<godot::classes::Camera2D>("/root/Spine/World/Camera2D")?;
        let viewport = self.base().get_viewport()?;
        let rect = viewport.get_visible_rect();
        let inverse = camera.get_canvas_transform().affine_inverse();
        let corners = [
            inverse * (rect.position),
            inverse * (rect.position + Vector2::new(rect.size.x, 0.0)),
            inverse * (rect.position + rect.size),
            inverse * (rect.position + Vector2::new(0.0, rect.size.y)),
        ];
        let min_x = corners.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
        let min_y = corners.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let max_x = corners
            .iter()
            .map(|p| p.x)
            .fold(f32::NEG_INFINITY, f32::max);
        let max_y = corners
            .iter()
            .map(|p| p.y)
            .fold(f32::NEG_INFINITY, f32::max);
        Some((min_x, min_y, max_x, max_y))
    }
}
