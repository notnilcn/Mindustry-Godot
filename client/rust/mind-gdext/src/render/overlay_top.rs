// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `OverlayRenderer.drawTop` (`OverlayRenderer.java:91-226`; H03 T5).
//!
//! At `Layer.overlayUI`: off-screen enemy indicators, the configured
//! building's outline, the enemy-core build-radius rings while placing, the
//! dashed drop-zone circles around the ground spawns and the hovered
//! player-team building's select overlay (turret/projector ranges, power-node
//! laser rings). H06 T12 plugs the logic overlays
//! (`ConfigOverlay::Logic` / `SelectOverlay::MessageText`/`LogicTag`) into the
//! same pass — one node only.

use godot::classes::notify::CanvasItemNotification;
use godot::classes::{Camera2D, INode2D, Node, Node2D, Time};
use godot::obj::{Base, Singleton};
use godot::prelude::*;

use mind_core::render::drawf::{absin, pal, to_bits};
use mind_core::render::overlay::{offscreen, spawn_alpha, spawner_margin};
use mind_core::render::rules::RulesRenderView;

use crate::input::MindInput;
use crate::render::canvas;
use crate::sim_host::{ConfigOverlay, MindSimHost, SelectOverlay};

/// `Rules.tilesize` (world px).
const TILESIZE: f32 = mind_core::config::TILESIZE as f32;

/// Overlay-top pass (`Node2D` under the `Layer.overlayUI` band).
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct OverlayTop {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
    input: Option<Gd<MindInput>>,
    rules: RulesRenderView,
    /// `buildFade = lerpDelta(buildFade, isPlacing ? 1 : 0, 0.06)`
    /// (`OverlayRenderer.java:164`).
    build_fade: f32,
    /// `indicators` setting (`core/ui/settings.rs:243`, default true).
    indicators: bool,
    /// Last frame delta in ticks (60 tps).
    delta_ticks: f32,
}

#[godot_api]
impl INode2D for OverlayTop {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            host: None,
            input: None,
            rules: RulesRenderView::default(),
            build_fade: 0.0,
            indicators: true,
            delta_ticks: 1.0,
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
        self.delta_ticks = (delta as f32 * 60.0).clamp(0.0, 10.0);
        self.indicators = self
            .base()
            .try_get_node_as::<Node>("/root/MindUi")
            .map(|mut ui| {
                ui.call(
                    "settings_get_bool",
                    &[GString::from("indicators").to_variant()],
                )
                .booleanize()
            })
            .unwrap_or(true);
        self.base_mut().queue_redraw();
    }

    fn draw(&mut self) {
        // Collect every input first: `base_mut()` borrows `self` for the whole
        // draw, so no `self.*` read may follow it.
        let Some(host) = self.host.clone() else {
            return;
        };
        let view = host.bind().overlay_view();
        let Some((px, py)) = view.player else {
            self.build_fade = 0.0;
            return;
        };
        // `buildFade = lerpDelta(buildFade, isPlacing ? 1 : 0, 0.06)`.
        let placing = self.placing();
        let target = if placing { 1.0 } else { 0.0 };
        let step = 1.0 - (1.0 - 0.06f32).powf(self.delta_ticks.max(0.001));
        self.build_fade += (target - self.build_fade) * step;
        let build_fade = self.build_fade;
        let indicators_enabled = self.indicators;
        let rules = self.rules.clone();
        // Configure outline (`:153-157`) selected in the input.
        let config = self
            .input
            .clone()
            .and_then(|input| input.bind().config_selected())
            .and_then(|(cx, cy)| host.bind().config_overlay(cx, cy));
        // Hover select (`:213-229`).
        let hover = if placing || self.mouse_over_stop_control() {
            None
        } else {
            self.hover_tile()
                .map(|(tx, ty)| host.bind().select_overlay(tx, ty))
        };
        let visible = self.visible_rect();
        let time = Time::singleton().get_ticks_msec() as f32 * 0.06;
        let mut base = self.base_mut();
        // 1. Enemy indicators (`OverlayRenderer.java:110-121`).
        if indicators_enabled && !rules.fog {
            for (x, y, team) in &view.enemy_units {
                if let Some((min_x, min_y, max_x, max_y)) = visible
                    && !offscreen(
                        [(min_x + max_x) / 2.0, (min_y + max_y) / 2.0],
                        max_x - min_x,
                        max_y - min_y,
                        *x,
                        *y,
                    )
                {
                    continue;
                }
                let segment =
                    mind_core::render::overlay::indicator_segment([*x, *y], [px, py], 3.0);
                canvas::poly_line(
                    &mut base,
                    &[(segment[0], segment[1]), (segment[2], segment[3])],
                    1.0,
                    team_color(*team),
                );
            }
        }
        // 2. Configure outline (`BuildingComp.java:1732-1737`).
        match config {
            Some(ConfigOverlay::Generic { x, y, size }) => {
                canvas::select(
                    &mut base,
                    x,
                    y,
                    size as f32 * 4.0 + 1.0,
                    canvas::color_u32(to_bits(pal::ACCENT)),
                );
            }
            Some(ConfigOverlay::PowerNodeLinks {
                x,
                y,
                size,
                laser_range,
                links,
            }) => {
                let accent = canvas::color_u32(to_bits(pal::ACCENT));
                let pulse = size as f32 * 4.0 + 1.0 + absin(time, 4.0, 1.0);
                canvas::circle_stroke(&mut base, x, y, pulse, 1.0, accent);
                if laser_range > 0.0 {
                    canvas::circle_stroke(&mut base, x, y, laser_range, 1.0, accent);
                }
                for (lx, ly, lsize) in links {
                    canvas::square(
                        &mut base,
                        lx,
                        ly,
                        lsize as f32 * 4.0 + 1.0,
                        45.0,
                        canvas::color_u32(to_bits(pal::PLACE)),
                    );
                }
            }
            Some(ConfigOverlay::BridgeLink {
                x,
                y,
                size,
                targets,
                ..
            }) => {
                let accent = canvas::color_u32(to_bits(pal::ACCENT));
                canvas::select(&mut base, x, y, size as f32 * 4.0 + 2.0, accent);
                for (tx, ty, linked) in targets {
                    let r =
                        size as f32 * 4.0 + 2.0 + if linked { 0.0 } else { absin(time, 4.0, 1.0) };
                    let color = if linked {
                        canvas::color_u32(to_bits(pal::PLACE))
                    } else {
                        canvas::color_u32(0xd44b3dff)
                    };
                    canvas::select(&mut base, tx, ty, r, color);
                }
            }
            // H06 T12 draws the logic config; nothing to draw until it lands
            // (the variant carries no geometry yet).
            Some(ConfigOverlay::Logic) | None => {}
        }
        // 3. Enemy-core build radius while placing (`:164-198`).
        let stroke = build_fade * 2.0;
        if build_fade > 0.005 && !rules.polygon_core_protection {
            for (x, y, team) in &view.enemy_cores {
                let radius = rules.core_build_radius_for(*team);
                if radius <= 0.0 {
                    continue;
                }
                if let Some((min_x, min_y, max_x, max_y)) = visible
                    && (*x + radius < min_x
                        || *x - radius > max_x
                        || *y + radius < min_y
                        || *y - radius > max_y)
                {
                    continue;
                }
                let dark = Color::from_rgba(0.25, 0.25, 0.25, 1.0);
                canvas::circle_stroke(&mut base, *x, *y - 2.0, radius, stroke, dark);
                let team = team_color(*team);
                let accent = canvas::color_u32(to_bits(pal::ACCENT));
                let mix = 0.5 + absin(time, 10.0, 0.5);
                let color = Color::from_rgba(
                    accent.r + (team.r - accent.r) * mix,
                    accent.g + (team.g - accent.g) * mix,
                    accent.b + (team.b - accent.b) * mix,
                    1.0,
                );
                canvas::circle_stroke(&mut base, *x, *y, radius, stroke, color);
            }
        }
        // 4. Drop-zone circles (`:200-211`).
        let margin = spawner_margin(TILESIZE);
        for (x, y) in &view.spawns {
            let dst = ((x - px).hypot(y - py)).max(0.001);
            if dst > rules.drop_zone_radius + margin {
                continue;
            }
            let alpha = spawn_alpha(dst, rules.drop_zone_radius, margin);
            if alpha <= 0.0 {
                continue;
            }
            let gray = Color::from_rgba(0.5, 0.5, 0.5, alpha);
            let light = Color::from_rgba(0.75, 0.75, 0.75, alpha);
            let mix = absin(time, 8.0, 1.0);
            let color = Color::from_rgba(
                gray.r + (light.r - gray.r) * mix,
                gray.g + (light.g - gray.g) * mix,
                gray.b + (light.b - gray.b) * mix,
                alpha,
            );
            canvas::dash_circle_stroke(&mut base, *x, *y, rules.drop_zone_radius, 2.0, color);
        }
        // 5. Hover select (`:213-229`).
        match hover {
            Some(SelectOverlay::Range {
                x,
                y,
                range,
                color,
                circles,
            }) => {
                let color = canvas::color_u32(color);
                if circles {
                    canvas::circles(&mut base, x, y, range, color);
                } else {
                    canvas::dash_circle(&mut base, x, y, range, color);
                }
            }
            Some(SelectOverlay::MessageText | SelectOverlay::LogicTag | SelectOverlay::None)
            | None => {}
        }
    }
}
impl OverlayTop {
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

    /// Live rules feed from the world renderer (polled every 30 frames there).
    pub fn update_rules(&mut self, rules: &RulesRenderView) {
        if *rules != self.rules {
            self.rules = rules.clone();
        }
    }

    /// Whether the local input has a block selected (`input.block != null`).
    fn placing(&self) -> bool {
        self.input
            .clone()
            .map(|input| input.bind().placement_view().block.is_some())
            .unwrap_or(false)
    }

    /// Cursor tile from the placement view (`input.block == null` hover path).
    fn hover_tile(&self) -> Option<(i32, i32)> {
        self.input
            .clone()
            .and_then(|input| input.bind().placement_view().cursor)
    }

    /// `Core.scene.hasMouse()` stand-in: a STOP-filter control holds the mouse.
    fn mouse_over_stop_control(&self) -> bool {
        let Some(viewport) = self.base().get_viewport() else {
            return false;
        };
        let Some(hovered) = viewport.gui_get_hovered_control() else {
            return false;
        };
        hovered.get_mouse_filter() == godot::classes::control::MouseFilter::STOP
    }

    /// World-space visible rect from the camera transform.
    fn visible_rect(&self) -> Option<(f32, f32, f32, f32)> {
        let camera = self
            .base()
            .try_get_node_as::<Camera2D>("/root/Spine/World/Camera2D")?;
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

/// Team colour (`Team.get(team).color`).
fn team_color(team: u8) -> Color {
    canvas::color_u32(mind_core::game::team::Team::get(team).color.to_rgba8888())
}
