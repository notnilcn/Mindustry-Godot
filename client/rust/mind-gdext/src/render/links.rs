// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Power-node lasers and item-bridge links (`PowerNode.draw`,
//! `PowerNode.java:496-512`, `ItemBridge.draw`, `ItemBridge.java:384-432`;
//! G03 T7) at the `Layer.power` band (70).

use godot::classes::notify::CanvasItemNotification;
use godot::classes::{INode2D, Node2D, Texture2D, Time};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::render::drawf;
use mind_core::render::drawf::pal;

use crate::render::canvas::{self, RegionCache};
use crate::sim_host::MindSimHost;

/// `Renderer.lasersopacity`/`bridgeopacity` default 100 (`settings.rs`).
const OPACITY_DEFAULT: i32 = 100;
/// `PowerNode.laserScale` (`PowerNode.java:36`); every node kind this pass draws
/// (`PowerNode`/`PowerSource`) keeps the default.
const POWER_LASER_SCALE: f32 = 0.25;

/// One resolved draw command.
enum Cmd {
    Region {
        texture: Option<Gd<Texture2D>>,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        rot: f32,
        color: Color,
    },
    RegionLine {
        texture: Option<Gd<Texture2D>>,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        stroke: f32,
        color: Color,
    },
}

/// `BeamNode.laserColor2` default (`BeamNode.java:35`, `ffd9c2`).
const BEAM_LASER_2: [f32; 4] = [1.0, 0.850_980_4, 0.760_784_3, 1.0];
/// `BeamNode.laserWidth` (`BeamNode.java:37`).
const BEAM_LASER_WIDTH: f32 = 0.4;
/// `BeamNode.pulseScl` / `pulseMag` (`BeamNode.java:36`).
const BEAM_PULSE_SCL: f32 = 7.0;
const BEAM_PULSE_MAG: f32 = 0.05;

/// Power/bridge link pass (`Node2D` under the `Layer.power` band).
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct LinkOverlay {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
    node: Option<Gd<Node2D>>,
    regions: RegionCache,
}

#[godot_api]
impl INode2D for LinkOverlay {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            host: None,
            node: None,
            regions: RegionCache::new(),
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
            self.base_mut().queue_redraw();
        }
    }

    fn draw(&mut self) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let Some(node) = self.node.clone() else {
            return;
        };
        let Some(rect) = self.visible_rect() else {
            return;
        };
        let ticks = Time::singleton().get_ticks_msec() as f32 * 0.06;
        let laser_opacity = self.setting("lasersopacity").max(0) as f32 / 100.0;
        let bridge_opacity = self.setting("bridgeopacity").max(0) as f32 / 100.0;
        let lod_l1 = self.renderer_bool("stats_lod_l1");
        let lod_l2 = self.renderer_bool("stats_lod_l2");
        let mut cmds: Vec<Cmd> = Vec::new();
        {
            let guard = host.bind();
            let Some(content) = guard.content_registry() else {
                return;
            };
            // Power-node lasers (`PowerNode.java:496-512`).
            if laser_opacity > 0.0 {
                for link in guard.power_lasers(rect) {
                    let Some(def) = content.block(link.block) else {
                        continue;
                    };
                    let laser = self
                        .region(&node, &format!("{}-laser", def.name))
                        .or_else(|| self.region(&node, "laser"));
                    let end = self
                        .region(&node, &format!("{}-laser-end", def.name))
                        .or_else(|| self.region(&node, "laser-end"));
                    let (x1, y1, x2, y2) = drawf::laser_endpoints(
                        link.x1, link.y1, link.size1, link.x2, link.y2, link.size2,
                    );
                    let rot = (y2 - y1).atan2(x2 - x1).to_degrees();
                    // `setupColor`: white -> `Pal.powerLight` by
                    // `(1 - satisfaction) * 0.86 + absin(time, 3, 0.1)`.
                    let mix = (1.0 - link.satisfaction) * 0.86 + drawf::absin(ticks, 3.0, 0.1);
                    let mut color = drawf::lerp([1.0, 1.0, 1.0, 1.0], pal::POWER_LIGHT, mix);
                    color[3] = laser_opacity * if lod_l2 { 1.0 } else { 0.0 };
                    let color = canvas::color_f4(color);
                    if laser.is_some() {
                        cmds.push(Cmd::RegionLine {
                            texture: laser,
                            x1,
                            y1,
                            x2,
                            y2,
                            stroke: 3.0,
                            color,
                        });
                    }
                    if let Some(end) = end {
                        // `Draw.rect(start, x, y, start.width * scale *
                        // start.scl(), ...)` (`Drawf.java:530-531`): `scl()` is
                        // `scale * Draw.scl` (Arc `TextureRegion.java:311-313`),
                        // so the cap is the natural size times
                        // `PowerNode.laserScale` (`0.25`, `PowerNode.java:36`).
                        let (w, h) = RegionCache::natural_size(&end);
                        let (w, h) = (w * POWER_LASER_SCALE, h * POWER_LASER_SCALE);
                        cmds.push(Cmd::Region {
                            texture: Some(end.clone()),
                            x: x1,
                            y: y1,
                            w,
                            h,
                            rot: rot + 180.0,
                            color,
                        });
                        cmds.push(Cmd::Region {
                            texture: Some(end),
                            x: x2,
                            y: y2,
                            w,
                            h,
                            rot,
                            color,
                        });
                    }
                }
            }
            // Beam-node beams (`BeamNode.BeamNodeBuild.draw`,
            // `BeamNode.java:207-233`; I02 T5). `Mathf.absin(scl, mag)` is
            // `absin(Time.time, ..)` (`Mathf.java:74-76`) and Arc's `Time.time`
            // accumulates `Time.delta`, i.e. ticks (60 per second), so this
            // shares the power-node branch's `ms * 0.06` clock.
            if laser_opacity > 0.0 {
                for link in guard.beam_node_lasers(rect) {
                    let Some(def) = content.block(link.block) else {
                        continue;
                    };
                    let laser = self
                        .region(&node, &format!("{}-beam", def.name))
                        .or_else(|| self.region(&node, "power-beam"));
                    let end = self
                        .region(&node, &format!("{}-beam-end", def.name))
                        .or_else(|| self.region(&node, "power-beam-end"));
                    // `Draw.color(laserColor1, laserColor2, (1 - satisfaction)
                    // * 0.86 + absin(3, 0.1))` then `Draw.alpha(laserOpacity)`.
                    let mix = (1.0 - link.satisfaction) * 0.86 + drawf::absin(ticks, 3.0, 0.1);
                    let mut color = drawf::lerp([1.0, 1.0, 1.0, 1.0], BEAM_LASER_2, mix);
                    color[3] = laser_opacity;
                    let color = canvas::color_f4(color);
                    // `float w = laserWidth + absin(pulseScl, pulseMag)`.
                    let width =
                        BEAM_LASER_WIDTH + drawf::absin(ticks, BEAM_PULSE_SCL, BEAM_PULSE_MAG);
                    let rot = (link.y2 - link.y1).atan2(link.x2 - link.x1).to_degrees();
                    if laser.is_some() {
                        cmds.push(Cmd::RegionLine {
                            texture: laser,
                            x1: link.x1,
                            y1: link.y1,
                            x2: link.x2,
                            y2: link.y2,
                            stroke: width,
                            color,
                        });
                    }
                    if let Some(end) = end {
                        let (w, h) = RegionCache::natural_size(&end);
                        cmds.push(Cmd::Region {
                            texture: Some(end.clone()),
                            x: link.x1,
                            y: link.y1,
                            w,
                            h,
                            rot: rot + 180.0,
                            color,
                        });
                        cmds.push(Cmd::Region {
                            texture: Some(end),
                            x: link.x2,
                            y: link.y2,
                            w,
                            h,
                            rot,
                            color,
                        });
                    }
                }
            }
            // Item-bridge links (`ItemBridge.java:384-432`).
            if bridge_opacity > 0.0 {
                for link in guard.bridge_links(rect) {
                    let Some(def) = content.block(link.block) else {
                        continue;
                    };
                    let (x, y) = (link.x as f32 * 8.0, link.y as f32 * 8.0);
                    let (ox, oy) = (link.ox as f32 * 8.0, link.oy as f32 * 8.0);
                    let dx = (link.ox - link.x).signum() as f32;
                    let dy = (link.oy - link.y).signum() as f32;
                    let dir = if dx > 0.0 {
                        0.0
                    } else if dy > 0.0 {
                        1.0
                    } else if dx < 0.0 {
                        2.0
                    } else {
                        3.0
                    };
                    let fade = if link.has_power {
                        link.warmup.max(0.25)
                    } else {
                        1.0
                    };
                    let mut base_color = [1.0f32, 1.0, 1.0, fade * bridge_opacity];
                    if !link.has_power {
                        base_color[3] = bridge_opacity;
                    }
                    let color = canvas::color_f4(base_color);
                    let end = self
                        .region(&node, &format!("{}-end", def.name))
                        .or_else(|| self.region(&node, "bridge-conveyor-end"));
                    if let Some(end) = end {
                        let (w, h) = RegionCache::natural_size(&end);
                        cmds.push(Cmd::Region {
                            texture: Some(end.clone()),
                            x,
                            y,
                            w,
                            h,
                            rot: dir * 90.0 + 90.0,
                            color,
                        });
                        cmds.push(Cmd::Region {
                            texture: Some(end),
                            x: ox,
                            y: oy,
                            w,
                            h,
                            rot: dir * 90.0 + 270.0,
                            color,
                        });
                    }
                    if let Some(bridge) = self
                        .region(&node, &format!("{}-bridge", def.name))
                        .or_else(|| self.region(&node, "bridge-conveyor-bridge"))
                    {
                        let len = ((ox - x).powi(2) + (oy - y).powi(2)).sqrt().max(0.001);
                        let (vx, vy) = ((ox - x) / len * 4.0, (oy - y) / len * 4.0);
                        cmds.push(Cmd::RegionLine {
                            texture: Some(bridge),
                            x1: x + vx,
                            y1: y + vy,
                            x2: ox - vx,
                            y2: oy - vy,
                            stroke: 6.5,
                            color,
                        });
                    }
                    if lod_l1 {
                        let count = drawf::bridge_arrow_count(link.x, link.y, link.ox, link.oy);
                        let arrow = self
                            .region(&node, &format!("{}-arrow", def.name))
                            .or_else(|| self.region(&node, "bridge-conveyor-arrow"));
                        if let Some(arrow) = arrow {
                            let (w, h) = RegionCache::natural_size(&arrow);
                            for a in 0..count {
                                let alpha = drawf::absin(a as f32 - link.time / 6.2, 0.4, 1.0)
                                    * link.warmup
                                    * bridge_opacity
                                    * if lod_l1 { 1.0 } else { 0.0 };
                                let (ax, ay) = drawf::bridge_arrow_pos(x, y, dx, dy, a);
                                cmds.push(Cmd::Region {
                                    texture: Some(arrow.clone()),
                                    x: ax,
                                    y: ay,
                                    w,
                                    h,
                                    rot: dir * 90.0,
                                    color: canvas::color_f4([1.0, 1.0, 1.0, alpha]),
                                });
                            }
                        }
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
                Cmd::RegionLine {
                    texture: Some(texture),
                    x1,
                    y1,
                    x2,
                    y2,
                    stroke,
                    color,
                } => canvas::draw_region_line(
                    &mut base, texture, *x1, *y1, *x2, *y2, *stroke, *color,
                ),
                Cmd::RegionLine { texture: None, .. } => {}
            }
        }
    }
}

impl LinkOverlay {
    /// Rebuilds Godot-derived refs (`ready` + hot reload).
    fn bootstrap(&mut self) {
        self.host = self
            .base()
            .try_get_node_as::<MindSimHost>("/root/Spine/SimHost");
        self.node = Some(self.to_gd().upcast::<Node2D>());
        self.base_mut().queue_redraw();
    }

    /// Resolves an atlas region through the shared cache.
    fn region(&mut self, node: &Gd<Node2D>, name: &str) -> Option<Gd<Texture2D>> {
        self.regions.get(node, name)
    }

    /// `settings_get_i32` from `/root/MindUi` (0..100 slider).
    fn setting(&self, key: &str) -> i32 {
        self.base()
            .try_get_node_as::<Node>("/root/MindUi")
            .map(|mut ui| {
                ui.call("settings_get_i32", &[GString::from(key).to_variant()])
                    .try_to::<i64>()
                    .unwrap_or(OPACITY_DEFAULT as i64) as i32
            })
            .unwrap_or(OPACITY_DEFAULT)
    }

    /// One renderer stats flag (`stats_lod_l1`/`stats_lod_l2`).
    fn renderer_bool(&self, method: &str) -> bool {
        self.base()
            .try_get_node_as::<Node>("/root/Spine/World/Renderer")
            .map(|mut renderer| renderer.call(method, &[]).booleanize())
            .unwrap_or(true)
    }

    /// World-space visible rect from the camera transform.
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
