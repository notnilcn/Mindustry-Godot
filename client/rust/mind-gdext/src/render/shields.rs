// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Shield domes and unit bubbles (`ForceProjector.drawShield`,
//! `UnitType.drawShield`; T13/PAR-0129), under `Layer.shields = 125`.
//!
//! Java renders the geometry into `effectBuffer` and blits the buffer through
//! `Shaders.shield` (`Renderer.java:403-414`). The port has no effect-buffer
//! composite yet (the shield shader samples a full-screen `u_texture`, which a
//! `CanvasGroup` cannot provide), so this pass draws Java's
//! `renderer.animateShields == false` branch verbatim:
//! `Draw.color(team.color, Color.white, clamp(hit))`, fill alpha
//! `0.09 + clamp(0.08 * hit)`, stroke 1.5, `Lines.poly`. Unit bubbles use the
//! `Fill.light` fan (centre transparent, rim `0.7 * shieldAlpha`).

use godot::classes::notify::CanvasItemNotification;
use godot::classes::{INode2D, Node2D};
use godot::obj::Base;
use godot::prelude::*;

use crate::sim_host::MindSimHost;

/// Team colour (`Team.color`) lerped toward white by `clamp(hit)`.
fn shield_color(team: u8, hit: f32) -> Color {
    let team = mind_core::game::team::Team::get(team).color;
    let t = hit.clamp(0.0, 1.0);
    Color::from_rgba(
        team.r + (1.0 - team.r) * t,
        team.g + (1.0 - team.g) * t,
        team.b + (1.0 - team.b) * t,
        1.0,
    )
}

/// `Fill.poly`/`Lines.poly` vertex ring: `sides` points at `rotation` degrees.
fn polygon(sides: u8, x: f32, y: f32, radius: f32, rotation_deg: f32) -> Vec<Vector2> {
    let count = sides.max(3) as usize;
    let mut out: Vec<Vector2> = Vec::with_capacity(count);
    for i in 0..count {
        let angle =
            rotation_deg.to_radians() + i as f32 * std::f32::consts::TAU / count as f32;
        out.push(Vector2::new(
            x + angle.cos() * radius,
            y + angle.sin() * radius,
        ));
    }
    out
}

/// Shield pass (`Node2D` under the `Layer.shields` band).
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct ShieldRenderer {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
}

#[godot_api]
impl INode2D for ShieldRenderer {
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
        if self.host.is_some() {
            // Domes rotate with `shieldRotation`; the shield state changes
            // without a world-revision bump.
            self.base_mut().queue_redraw();
        }
    }

    fn draw(&mut self) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let host = host.bind();
        let domes = host.force_shields();
        let bubbles = host.unit_shields();
        if domes.is_empty() && bubbles.is_empty() {
            return;
        }
        let mut base = self.base_mut();
        for (x, y, radius, sides, rotation, team, hit) in domes {
            let color = shield_color(team, hit);
            let points = polygon(sides, x, y, radius, rotation);
            let packed: PackedVector2Array = points.iter().copied().collect();
            // Non-animated path: `Draw.alpha(0.09f + clamp(0.08f * hit))` fill
            // then `Lines.stroke(1.5f); Lines.poly(...)`.
            let fill_alpha = 0.09 + (0.08 * hit).clamp(0.0, 1.0);
            base.draw_colored_polygon(
                &packed,
                Color::from_rgba(color.r, color.g, color.b, fill_alpha),
            );
            let mut ring = points;
            if let Some(first) = ring.first().copied() {
                ring.push(first);
            }
            let closed: PackedVector2Array = ring.into_iter().collect();
            base.draw_polyline_ex(&closed, color).width(1.5).done();
        }
        for (x, y, radius, alpha, team, hit) in bubbles {
            // `UnitType.drawShield`: `Fill.light(x, y, circleVertices(r), r,
            // clear, shieldColor.lerp(white, clamp(hitTime / 2)).a(0.7 *
            // shieldAlpha))`.
            let color = shield_color(team, hit);
            let rim = Color::from_rgba(color.r, color.g, color.b, 0.7 * alpha);
            let clear = Color::from_rgba(0.0, 0.0, 0.0, 0.0);
            let ring = polygon(24, x, y, radius, 0.0);
            let mut points: Vec<Vector2> = Vec::with_capacity(ring.len() + 1);
            let mut colors: Vec<Color> = Vec::with_capacity(ring.len() + 1);
            points.push(Vector2::new(x, y));
            colors.push(clear);
            for point in ring {
                points.push(point);
                colors.push(rim);
            }
            let packed_points: PackedVector2Array = points.into_iter().collect();
            let packed_colors: PackedColorArray = colors.into_iter().collect();
            base.draw_polygon(&packed_points, &packed_colors);
        }
    }
}

impl ShieldRenderer {
    /// Rebuilds Godot-derived refs (`ready` + hot reload).
    fn bootstrap(&mut self) {
        self.host = self
            .base()
            .try_get_node_as::<MindSimHost>("/root/Spine/SimHost");
        self.base_mut().queue_redraw();
    }
}
