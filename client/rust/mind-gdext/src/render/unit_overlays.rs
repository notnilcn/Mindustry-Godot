// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit view overlays (G03 T5): the mining beam + carried item stack
//! (`UnitType.drawMining`/`drawItems`, `UnitType.java:1599-1714`), engine
//! glows (`UnitEngine.draw`, `UnitType.java:2018-2050`) and flight trails
//! (`UnitType.drawTrail`, `UnitType.java:1716-1722`).
//!
//! Two instances share this class: `mode = Effects` at the `Layer.flyingUnit`
//! band (mining/items; Java draws them at the unit's z) and `mode = Engines`
//! at `Layer.flyingUnitLow` so engine circles stay under flying bodies
//! (accepted deviation for ground units: their engines land in the flying-low
//! band instead of their own layer).

use std::collections::HashMap;

use godot::builtin::Vector2;
use godot::classes::notify::CanvasItemNotification;
use godot::classes::{Camera2D, INode2D, Node2D, Texture2D, ThemeDb, Time};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::content::{ContentRegistry, Rgba};
use mind_core::render::drawf;
use mind_core::render::trail::Trail;
use mind_core::render::unit_draw::{self, UnitViewState};

use crate::render::canvas::{self, RegionCache};
use crate::sim_host::{MindSimHost, UnitDraw};

/// Which overlay this instance draws.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OverlayMode {
    /// Mining beam, carried item stack and the player's amount text.
    Effects,
    /// Engine glow circles and flight trails.
    Engines,
}

/// One unit overlay pass (`Node2D` under a unit band).
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct UnitOverlay {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
    node: Option<Gd<Node2D>>,
    regions: RegionCache,
    /// Which overlay this instance draws (`set_mode` before `add_child`).
    pub mode: OverlayMode,
    /// Per-unit view state (`hitTime`/`itemTime` mirrors; T5).
    pub view: HashMap<u64, UnitViewState>,
    /// Flight trails keyed by `EntitySeq` (`UnitComp.update`'s update call).
    pub trails: HashMap<u64, Trail>,
    /// Tick clock of the last `process` (delta_ticks for view lerps).
    pub last_tick: f32,
}

#[godot_api]
impl INode2D for UnitOverlay {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            host: None,
            node: None,
            regions: RegionCache::new(),
            mode: OverlayMode::Effects,
            view: HashMap::new(),
            trails: HashMap::new(),
            last_tick: 0.0,
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
        let Some(host) = self.host.clone() else {
            return;
        };
        let ticks = Time::singleton().get_ticks_msec() as f32 * 0.06;
        let delta_ticks = (ticks - self.last_tick).max(0.0);
        self.last_tick = ticks;
        let guard = host.bind();
        let Some(content) = guard.content_registry() else {
            return;
        };
        let draws = guard.unit_draws_full();
        let mut live = std::collections::HashSet::new();
        for unit in &draws {
            live.insert(unit.id);
            let state = self.view.entry(unit.id).or_default();
            unit_draw::advance_view(state, unit.health, unit.item.is_some(), delta_ticks);
            if self.mode != OverlayMode::Engines {
                continue;
            }
            let Some(def) = content.unit_by_name(&unit.name) else {
                continue;
            };
            if def.trail_length > 0 {
                let offset = def.engine_offset / 2.0
                    + def.engine_offset / 2.0
                        * if def.use_engine_elevation {
                            unit.elevation
                        } else {
                            1.0
                        };
                let (ox, oy) = drawf::trns(unit.rotation + 180.0, offset);
                let trail = self
                    .trails
                    .entry(unit.id)
                    .or_insert_with(|| Trail::new(def.trail_length as usize));
                trail.update(unit.x + ox, unit.y + oy, 1.0);
            }
        }
        self.view.retain(|id, _| live.contains(id));
        self.trails.retain(|id, _| live.contains(id));
        self.base_mut().queue_redraw();
    }

    fn draw(&mut self) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let Some(node) = self.node.clone() else {
            return;
        };
        let ticks = Time::singleton().get_ticks_msec() as f32 * 0.06;
        let mut cmds: Vec<Cmd> = Vec::new();
        {
            let guard = host.bind();
            let Some(content) = guard.content_registry() else {
                return;
            };
            let units = guard.unit_draws_full();
            let visible = self.visible_rect();
            for unit in &units {
                if let Some((min_x, min_y, max_x, max_y)) = visible
                    && (unit.x < min_x - 96.0
                        || unit.y < min_y - 96.0
                        || unit.x > max_x + 96.0
                        || unit.y > max_y + 96.0)
                {
                    continue;
                }
                match self.mode {
                    OverlayMode::Effects => self.push_effects(&mut cmds, &node, unit, content),
                    OverlayMode::Engines => {
                        self.push_engines(&mut cmds, unit, content, ticks);
                        self.push_trail(&mut cmds, unit, content);
                    }
                }
            }
        }

        let mut base = self.base_mut();
        for cmd in &cmds {
            match cmd {
                Cmd::Region {
                    texture: Some(texture),
                    x,
                    y,
                    w,
                    h,
                    rot,
                    color,
                } => canvas::draw_region(&mut base, texture, *x, *y, *w, *h, *rot, *color),
                Cmd::Region { texture: None, .. } => {}
                Cmd::Circle { x, y, r, color } => {
                    base.draw_circle(Vector2::new(*x, *y), *r, *color);
                }
                Cmd::Poly { points, color } => {
                    let packed: godot::builtin::PackedVector2Array = points
                        .iter()
                        .map(|(x, y)| Vector2::new(*x, *y))
                        .collect::<Vec<_>>()
                        .into_iter()
                        .collect();
                    base.draw_colored_polygon(&packed, *color);
                }
                Cmd::Text {
                    x,
                    y,
                    text,
                    size,
                    color,
                } => {
                    let font = ThemeDb::singleton().get_fallback_font();
                    if let Some(font) = font {
                        base.draw_string_ex(
                            &font,
                            Vector2::new(*x, *y),
                            &GString::from(text.as_str()),
                        )
                        .font_size((*size).round() as i32)
                        .modulate(*color)
                        .done();
                    }
                }
            }
        }
    }
}

impl UnitOverlay {
    /// Rebuilds Godot-derived refs (`ready` + hot reload).
    fn bootstrap(&mut self) {
        self.host = self
            .base()
            .try_get_node_as::<MindSimHost>("/root/Spine/SimHost");
        self.node = Some(self.to_gd().upcast::<Node2D>());
        self.base_mut().queue_redraw();
    }

    /// Mining beam + carried item (`UnitType.drawMining`/`drawItems`).
    fn push_effects(
        &mut self,
        cmds: &mut Vec<Cmd>,
        node: &Gd<Node2D>,
        unit: &UnitDraw,
        content: &ContentRegistry,
    ) {
        let Some(def) = content.unit_by_name(&unit.name) else {
            return;
        };
        if def.draw_mine_beam
            && let Some(tile) = unit.mine_tile
        {
            // `drawMiningBeam` (`UnitType.java:1599-1631`).
            let (emitter, target) = unit_draw::mine_beam_points(
                unit.x,
                unit.y,
                unit.rotation,
                def.mine_beam_offset,
                tile,
                self.last_tick,
            );
            let mut color = color4(unit_draw::mine_beam_color(self.last_tick));
            // `Draw.alpha(Renderer.unitLaserOpacity)` (`UnitType.java:1621`);
            // `unitLaserOpacity = settings.getInt("unitlaseropacity") / 100f`
            // (`Renderer.java:165`).
            color.a = crate::settings_hub::row_i32("unitlaseropacity").clamp(0, 100) as f32 / 100.0;
            if let Some(texture) = self.region(node, "minelaser") {
                // `Drawf.laser` stretches the `minelaser` region along the beam
                // (`stroke 12 * 0.75 = 9`).
                let dx = target.0 - emitter.0;
                let dy = target.1 - emitter.1;
                let len = (dx * dx + dy * dy).sqrt().max(0.001);
                cmds.push(Cmd::Region {
                    texture: Some(texture),
                    x: (emitter.0 + target.0) / 2.0,
                    y: (emitter.1 + target.1) / 2.0,
                    w: len,
                    h: 9.0,
                    rot: dy.atan2(dx).to_degrees(),
                    color,
                });
            }
            if let Some(end) = self.region(node, "minelaser-end") {
                cmds.push(Cmd::Region {
                    texture: Some(end),
                    x: emitter.0,
                    y: emitter.1,
                    w: 13.5,
                    h: 13.5,
                    rot: unit.rotation,
                    color,
                });
            }
            // Local unit: the rotating accent square on the mined tile.
            if unit.is_player {
                let mut points = Vec::with_capacity(4);
                let circum = 4.0 * std::f32::consts::SQRT_2;
                for i in 0..4 {
                    let angle = ticks_deg(self.last_tick) + 45.0 + i as f32 * 90.0;
                    let (px, py) = drawf::trns(angle, circum);
                    points.push((target.0 + px, target.1 + py));
                }
                cmds.push(Cmd::Poly {
                    points,
                    color: canvas::color_f4(drawf::pal::ACCENT),
                });
            }
        }
        // `drawItems` (`UnitType.java:1682-1714`).
        if def.draw_items
            && let Some((item, amount)) = &unit.item
        {
            let state = self.view.get(&unit.id).copied().unwrap_or_default();
            if state.item_time > 0.01 {
                let sin = drawf::absin(self.last_tick, 5.0, 1.0);
                let size = (5.0 + sin) * state.item_time;
                let (ox, oy) = drawf::trns(unit.rotation + 180.0, def.item_offset_y);
                let accent = drawf::pal::ACCENT;
                let mix = drawf::lerp(accent, [1.0, 1.0, 1.0, 1.0], sin * 0.1);
                if let Some(texture) = self.region(node, &format!("item-{item}")) {
                    cmds.push(Cmd::Region {
                        texture: Some(texture),
                        x: unit.x + ox,
                        y: unit.y + oy,
                        w: size,
                        h: size,
                        rot: unit.rotation,
                        color: canvas::color_f4(mix),
                    });
                }
                if let Some(ring) = self.region(node, "ring-item") {
                    let r = ((3.0 + sin) * state.item_time + 0.5) * 2.0;
                    cmds.push(Cmd::Region {
                        texture: Some(ring),
                        x: unit.x + ox,
                        y: unit.y + oy,
                        w: r,
                        h: r,
                        rot: 0.0,
                        color: canvas::color_f4(accent),
                    });
                }
                if unit.is_player {
                    cmds.push(Cmd::Text {
                        x: unit.x,
                        y: unit.y - 3.0,
                        text: amount.to_string(),
                        size: 4.0 * state.item_time,
                        color: canvas::color_f4(accent),
                    });
                }
            }
        }
    }

    /// `UnitEngine.draw` circles (`UnitType.java:2018-2050`).
    fn push_engines(
        &mut self,
        cmds: &mut Vec<Cmd>,
        unit: &UnitDraw,
        content: &ContentRegistry,
        ticks: f32,
    ) {
        let Some(def) = content.unit_by_name(&unit.name) else {
            return;
        };
        let team = mind_core::game::team::Team::get(unit.team)
            .color
            .to_rgba8888();
        let outer_color = def
            .engine_color
            .map(rgba_color)
            .unwrap_or_else(|| color_u32(team));
        let inner_color = rgba_color(def.engine_color_inner);
        for engine in &def.engines {
            if let Some((outer, inner)) = unit_draw::engine_circles(
                unit.x,
                unit.y,
                unit.rotation,
                unit.elevation,
                def.use_engine_elevation,
                (engine.x, engine.y, engine.radius, engine.rotation),
                ticks,
            ) {
                cmds.push(Cmd::Circle {
                    x: outer.0,
                    y: outer.1,
                    r: outer.2,
                    color: outer_color,
                });
                cmds.push(Cmd::Circle {
                    x: inner.0,
                    y: inner.1,
                    r: inner.2,
                    color: inner_color,
                });
            }
        }
    }

    /// `UnitType.drawTrail` (`UnitType.java:1716-1722`).
    fn push_trail(&mut self, cmds: &mut Vec<Cmd>, unit: &UnitDraw, content: &ContentRegistry) {
        let Some(def) = content.unit_by_name(&unit.name) else {
            return;
        };
        let Some(trail) = self.trails.get(&unit.id) else {
            return;
        };
        let team = mind_core::game::team::Team::get(unit.team)
            .color
            .to_rgba8888();
        let color = def
            .trail_color
            .map(rgba_color)
            .unwrap_or_else(|| color_u32(team));
        let width = (def.engine_size
            + drawf::absin(self.last_tick, 2.0, def.engine_size / 4.0)
                * if def.use_engine_elevation {
                    unit.elevation
                } else {
                    1.0
                })
            * def.trail_scl;
        let mut quads: Vec<[f32; 8]> = Vec::new();
        trail.draw(width, &mut quads);
        for q in &quads {
            cmds.push(Cmd::Poly {
                points: vec![(q[0], q[1]), (q[2], q[3]), (q[4], q[5]), (q[6], q[7])],
                color,
            });
        }
        if let Some((cx, cy, w, _)) = trail.draw_cap(width) {
            cmds.push(Cmd::Circle {
                x: cx,
                y: cy,
                r: w / 2.0,
                color,
            });
        }
    }

    /// Resolves an atlas region through the shared cache.
    fn region(&mut self, node: &Gd<Node2D>, name: &str) -> Option<Gd<Texture2D>> {
        self.regions.get(node, name)
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
        Some((min_x, min_y, max_x, max_y))
    }
}

/// One overlay draw command.
enum Cmd {
    Region {
        texture: Option<Gd<Texture2D>>,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        rot: f32,
        color: godot::builtin::Color,
    },
    Circle {
        x: f32,
        y: f32,
        r: f32,
        color: godot::builtin::Color,
    },
    Poly {
        points: Vec<(f32, f32)>,
        color: godot::builtin::Color,
    },
    Text {
        x: f32,
        y: f32,
        text: String,
        size: f32,
        color: godot::builtin::Color,
    },
}

/// `Time.time` in ticks, mod 360 (Arc's `Time.time` is monotonic; the square
/// rotation uses it directly, so keep the raw ticks).
fn ticks_deg(ticks: f32) -> f32 {
    ticks
}

fn color4(color: [f32; 4]) -> godot::builtin::Color {
    canvas::color_f4(color)
}

fn rgba_color(color: Rgba) -> godot::builtin::Color {
    canvas::color_f4([color.r, color.g, color.b, color.a])
}

fn color_u32(rgba: u32) -> godot::builtin::Color {
    canvas::color_u32(rgba)
}
