// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Shield domes and unit bubbles (`ForceProjector.drawShield`,
//! `UnitType.drawShield`; T13/PAR-0129), under `Layer.shields = 125`.
//!
//! Java renders the geometry into `effectBuffer` and blits the buffer through
//! `Shaders.shield` (`Renderer.java:403-414`) when `renderer.animateShields`
//! (desktop default) is on; the same flag switches `ForceProjector.drawShield`
//! to its opaque `Fill.poly` branch. The port mirrors that with a
//! `CanvasGroup` parent (created by `MindWorldRenderer::build_floor`) carrying
//! `res://shaders/shield_group.gdshader`: animated -> opaque team fill, group
//! material on; off -> Java's `renderer.animateShields == false` branch
//! (`Draw.alpha(0.09 + 0.08 * hit)`, fill + stroke 1.5) with no group
//! material. Unit bubbles keep the `Fill.light` fan in both branches (Java
//! `UnitType.java:1643-1650`).

use godot::classes::notify::CanvasItemNotification;
use godot::classes::{CanvasGroup, INode2D, Material, Node, Node2D, ShaderMaterial, Time};
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
        let angle = rotation_deg.to_radians() + i as f32 * std::f32::consts::TAU / count as f32;
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
    /// `animatedshields` setting, re-read every frame.
    animated: bool,
    /// `None` until the first frame's setting read installs/clears the parent
    /// `CanvasGroup` material.
    last_animated: Option<bool>,
    /// Group-composite material (`shield_group.gdshader`), installed by
    /// `build_floor`.
    material: Option<Gd<ShaderMaterial>>,
}

#[godot_api]
impl INode2D for ShieldRenderer {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            host: None,
            animated: false,
            last_animated: None,
            material: None,
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
        if self.host.is_some() {
            // Domes rotate with `shieldRotation`; the shield state changes
            // without a world-revision bump. The group material follows the
            // `animatedshields` setting (`Renderer.animateShields`).
            self.sync_group_material();
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
        let animated = self.animated;
        let mut base = self.base_mut();
        for (x, y, radius, sides, rotation, team, hit) in domes {
            let color = shield_color(team, hit);
            let points = polygon(sides, x, y, radius, rotation);
            let packed: PackedVector2Array = points.iter().copied().collect();
            if animated {
                // `renderer.animateShields`: opaque `Fill.poly` at
                // `team.color.lerp(white, clamp(hit))`; the group shader adds
                // the outline, stripes and `ALPHA 0.18`
                // (`ForceProjector.java:308-330`).
                base.draw_colored_polygon(&packed, color);
                continue;
            }
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

    /// Installs the `shield_group` material (called from
    /// `MindWorldRenderer::build_floor`; a hot reload recreates the pass).
    pub(crate) fn set_group_material(&mut self, material: Option<Gd<ShaderMaterial>>) {
        self.material = material;
    }

    /// `animatedshields` setting (`SettingsMenuDialog` `graphics.checkPref`,
    /// desktop default true).
    fn animated_setting(&self) -> bool {
        self.base()
            .try_get_node_as::<Node>("/root/MindUi")
            .map(|mut ui| {
                ui.call(
                    "settings_get_bool",
                    &[GString::from("animatedshields").to_variant()],
                )
                .booleanize()
            })
            .unwrap_or(true)
    }

    /// Applies/clears the parent `CanvasGroup`'s shield material from the
    /// setting and refreshes its per-frame uniforms (`Shaders.ShieldShader`).
    fn sync_group_material(&mut self) {
        let animated = self.animated_setting();
        self.animated = animated;
        let Some(parent) = self
            .base()
            .get_parent()
            .and_then(|parent| parent.try_cast::<CanvasGroup>().ok())
        else {
            return;
        };
        if self.last_animated != Some(animated) {
            let mut parent = parent;
            match (&self.material, animated) {
                (Some(material), true) => {
                    parent.set_material(&material.clone().upcast::<Material>());
                }
                _ => parent.set_material(Gd::<Material>::null_arg()),
            }
            self.last_animated = Some(animated);
        }
        if !animated {
            return;
        }
        let Some(material) = self.material.clone() else {
            return;
        };
        let Some((x, y, w, h)) = self.visible_rect() else {
            return;
        };
        let mut material = material;
        // `Shaders.ShieldShader.apply`: `u_dp = Scl.scl(1)`, `u_time =
        // Time.time`, `u_offset = camera top-left`, `u_texsize = camera size`,
        // `u_invsize = 1 / size`.
        material.set_shader_parameter("u_dp", &1.0f32.to_variant());
        material.set_shader_parameter(
            "u_time",
            &(Time::singleton().get_ticks_msec() as f32 * 0.06).to_variant(),
        );
        material.set_shader_parameter("u_offset", &Vector2::new(x, y).to_variant());
        material.set_shader_parameter("u_texsize", &Vector2::new(w, h).to_variant());
        material.set_shader_parameter(
            "u_invsize",
            &Vector2::new(1.0 / w.max(1.0), 1.0 / h.max(1.0)).to_variant(),
        );
    }

    /// World-space visible rect `(min x, min y, w, h)` from the camera
    /// transform (the group shader's `u_offset`/`u_texsize`).
    fn visible_rect(&self) -> Option<(f32, f32, f32, f32)> {
        let camera = self
            .base()
            .try_get_node_as::<godot::classes::Camera2D>("/root/Spine/World/Camera2D")?;
        let viewport = self.base().get_viewport()?;
        let rect = viewport.get_visible_rect();
        let inverse = camera.get_canvas_transform().affine_inverse();
        let corners = [
            inverse * rect.position,
            inverse * (rect.position + Vector2::new(rect.size.x, 0.0)),
            inverse * rect.position + rect.size,
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
        Some((min_x, min_y, max_x - min_x, max_y - min_y))
    }
}
