// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Build-beam pass (`BuilderComp.drawBuilding`/`drawBuildingBeam`,
//! `BuilderComp.java:315-386`; G03 T3): the fan from 3.8 world px in front of
//! the builder unit to the construct site, the pulsing emitter diamond and the
//! target-square fill while the beam fades in/out (`buildAlpha`).
//!
//! `BuilderComp.buildAlpha = lerpDelta(buildAlpha, activelyBuilding() ? 1 : 0,
//! 0.15)` (line 85); the emitted geometry is `Drawf.buildBeam`'s filled
//! triangles (`Drawf.java:131-157`) at the buildbeam shader's base alpha
//! (`0.37`, `client/shaders/buildbeam.gdshader`). T17 replaces this with the
//! real screen-texture composite.

use godot::builtin::{PackedVector2Array, Vector2};
use godot::classes::notify::CanvasItemNotification;
use godot::classes::{INode2D, Node2D, Time};
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
}

#[godot_api]
impl INode2D for BuildBeamOverlay {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            host: None,
            build_alpha: 0.0,
            last_active: None,
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
        // `Drawf.buildBeam(px, py, tx, ty, 8*size/2)` filled triangles at the
        // buildbeam shader's base alpha.
        let beam_color = canvas::color_f4(drawf::with_alpha(color, alpha * 0.37));
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
