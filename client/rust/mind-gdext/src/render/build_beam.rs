// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Build-beam pass (`BuilderComp.drawBuilding`/`drawBuildingBeam`,
//! `BuilderComp.java:315-386`; G03 T3): the fan from 3.8 world px in front of
//! the builder unit to the construct site, the pulsing emitter diamond and the
//! target-square fill while the beam fades in/out (`buildAlpha`).
//!
//! `BuilderComp.buildAlpha = lerpDelta(buildAlpha, activelyBuilding() ? 1 : 0,
//! 0.15)` (line 85); the emitted geometry is `Drawf.buildBeam`'s filled
//! triangles (`Drawf.java:131-157`). With `animatedshields` on (the H03 T11
//! `CanvasGroup` composite), the CPU alpha is `buildAlpha` and the group
//! shader (`buildbeam_group.gdshader`) applies Java's
//! `0.37 + abs(sin(u_time / 15)) * 0.05 + stripe` modulation; off, the flat
//! path bakes `* 0.37` in.

use godot::builtin::{PackedVector2Array, Vector2};
use godot::classes::notify::CanvasItemNotification;
use godot::classes::{CanvasGroup, INode2D, Material, Node, Node2D, ShaderMaterial, Time};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::render::drawf;
use mind_core::render::drawf::pal;
use mind_core::sim::builder::UnitBuildPlan;

use crate::render::canvas;
use crate::sim_host::MindSimHost;

/// Arc `lerpDelta(a, b, t) = lerp(a, b, 1 - (1 - t)^delta)` in ticks.
fn lerp_delta(a: f32, b: f32, t: f32, delta_ticks: f32) -> f32 {
    a + (b - a) * (1.0 - (1.0 - t).powf(delta_ticks))
}

/// Build-beam pass (`Node2D` under the `Layer.buildBeam` band).
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct BuildBeamOverlay {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
    /// `BuilderComp.buildAlpha`.
    build_alpha: f32,
    /// `BuilderComp.lastActive`: the plan (and its block size) drawn while the
    /// alpha fades out.
    last_active: Option<(UnitBuildPlan, i32, bool)>,
    /// `animatedshields` setting, re-read every frame.
    animated: bool,
    /// `None` until the first frame's setting read installs/clears the parent
    /// `CanvasGroup` material.
    last_animated: Option<bool>,
    /// Group-composite material (`buildbeam_group.gdshader`), installed by
    /// `build_floor`.
    material: Option<Gd<ShaderMaterial>>,
}

#[godot_api]
impl INode2D for BuildBeamOverlay {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            host: None,
            build_alpha: 0.0,
            last_active: None,
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

    fn process(&mut self, delta: f64) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let delta_ticks = ((delta * 60.0) as f32).max(0.0);
        let view = host.bind().player_builder_view();
        let active = view.as_ref().is_some_and(|view| {
            view.update_building
                && !view.plans.is_empty()
                && view.plans.first().is_some_and(|head| within(view, head))
        });
        self.build_alpha = lerp_delta(
            self.build_alpha,
            f32::from(u8::from(active)),
            0.15,
            delta_ticks,
        );
        if active {
            if let Some(view) = view.as_ref()
                && let Some(head) = view.plans.first()
            {
                let size = self.plan_size(head);
                self.last_active = Some((head.clone(), size, true));
            }
        } else if self.build_alpha <= 0.01 {
            self.last_active = None;
        }
        self.sync_group_material();
        self.base_mut().queue_redraw();
    }

    fn draw(&mut self) {
        if self.build_alpha <= 0.001 {
            return;
        }
        let Some((plan, size, active)) = self.last_active.clone() else {
            return;
        };
        let Some(host) = self.host.clone() else {
            return;
        };
        let view = host.bind().player_builder_view();
        let Some(view) = view else {
            return;
        };
        if !view.draw_beam {
            return;
        }
        let (dx, dy) = (plan.x as f32 * 8.0 - view.x, plan.y as f32 * 8.0 - view.y);
        if dx.hypot(dy) > view.build_range {
            return;
        }
        let ticks = Time::singleton().get_ticks_msec() as f32 * 0.06;
        // `focusLen = buildBeamOffset + absin(Time.time, 3, 0.6)`.
        let focus = view.beam_offset + drawf::absin(ticks, 3.0, 0.6);
        let (fx, fy) = drawf::trns(view.rotation, focus);
        let (px, py) = (view.x + fx, view.y + fy);
        let (tx, ty) = (plan.x as f32 * 8.0, plan.y as f32 * 8.0);
        let color = if plan.breaking {
            pal::REMOVE
        } else {
            pal::ACCENT
        };
        let alpha = self.build_alpha;
        let animated = self.animated;
        let half = size as f32 * 4.0;
        let construct_site = host
            .bind()
            .construct_at(plan.x as i32, plan.y as i32)
            .is_some();

        let mut base = self.base_mut();
        // `if(!active && !(tile.build instanceof ConstructBuild)) Fill.square(tx, ty, size*8/2)`.
        if !active && !construct_site {
            canvas::fill_rect(
                &mut base,
                tx - half,
                ty - half,
                tx + half,
                ty + half,
                canvas::color_f4(drawf::with_alpha(color, alpha)),
            );
        }
        // `Drawf.buildBeam` filled triangles. With the animated composite the
        // shader applies the base alpha/stripe; the flat path bakes `* 0.37`.
        let beam_scale = if animated { 1.0 } else { 0.37 };
        let beam_color = canvas::color_f4(drawf::with_alpha(color, alpha * beam_scale));
        for tri in drawf::build_beam_tris(px, py, tx, ty, half) {
            canvas::fill_tri(&mut base, tri, beam_color);
        }
        // `Fill.square(px, py, 1.8 + absin(Time.time, 2.2, 1.1), rotation + 45)`:
        // `Lines.square`'s vertices sit on the circumradius at
        // `rotation + 45 + k*90 - 45`.
        let emitter = 1.8 + drawf::absin(ticks, 2.2, 1.1);
        let mut poly = PackedVector2Array::new();
        for k in 0..4 {
            let (vx, vy) = drawf::trns(view.rotation + k as f32 * 90.0, emitter);
            poly.push(Vector2::new(px + vx, py + vy));
        }
        base.draw_colored_polygon(&poly, canvas::color_f4(drawf::with_alpha(color, alpha)));
    }
}

impl BuildBeamOverlay {
    /// Rebuilds Godot-derived refs (`ready` + hot reload).
    fn bootstrap(&mut self) {
        self.host = self
            .base()
            .try_get_node_as::<MindSimHost>("/root/Spine/SimHost");
        self.base_mut().queue_redraw();
    }

    /// Installs the `buildbeam_group` material (called from
    /// `MindWorldRenderer::build_floor`; a hot reload recreates the pass).
    pub(crate) fn set_group_material(&mut self, material: Option<Gd<ShaderMaterial>>) {
        self.material = material;
    }

    /// `animatedshields` setting (shared by the shield and build-beam gates,
    /// `Renderer.java:403-414`; desktop default true).
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

    /// Applies/clears the parent `CanvasGroup`'s beam material from the
    /// setting and refreshes its per-frame uniforms
    /// (`Shaders.BuildBeamShader.apply`).
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

    /// `size = plan.breaking ? tile.block().size : plan.block.size`
    /// (`BuilderComp.java:328-333`).
    fn plan_size(&self, plan: &UnitBuildPlan) -> i32 {
        let Some(host) = self.host.clone() else {
            return 1;
        };
        let guard = host.bind();
        if plan.breaking {
            return guard
                .construct_at(plan.x as i32, plan.y as i32)
                .and_then(|_| {
                    let content = guard.content_registry()?;
                    let tile = guard.grid().tile(plan.x as i32, plan.y as i32);
                    content.block(tile.block).map(|def| def.size)
                })
                .unwrap_or(1);
        }
        guard
            .content_registry()
            .and_then(|content| content.block(plan.block))
            .map(|def| def.size)
            .unwrap_or(1)
    }
}

/// `Position.within(x, y, range)`: `dst < range` (Java `dst` is `<`).
fn within(view: &crate::sim_host::BuilderView, plan: &UnitBuildPlan) -> bool {
    (plan.x as f32 * 8.0 - view.x).hypot(plan.y as f32 * 8.0 - view.y) < view.build_range
}
