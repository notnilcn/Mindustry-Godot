// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Bullet draw pass (`BulletType.draw` and subclasses; G03 T4).
//!
//! Ports the Basic-family sprite draw (`BasicBulletType.java:48-67`), the
//! laser bolt lines (`LaserBoltBulletType.java:26-34`), the lancer laser
//! layers (`LaserBulletType.java:76-104`), shrapnel (`ShrapnelBulletType.java:
//! 55-70`), liquid orbs (`LiquidBulletType.java:70-81`) and trails
//! (`BulletType.drawTrail`, `BulletType.java:687-695`). The pass lives at
//! `Layer.bullet` (100).

use std::collections::HashMap;

use godot::builtin::{Color, PackedVector2Array, Rect2, Vector2};
use godot::classes::notify::CanvasItemNotification;
use godot::classes::{Camera2D, INode2D, Node2D, Texture2D};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::content::registries::bullets::{BulletDef, BulletKind};
use mind_core::content::registries::units::parts::InterpKind;
use mind_core::content::{ContentRegistry, Rgba};
use mind_core::math::interp::Interp;
use mind_core::render::bullet_draw;
use mind_core::render::drawf::{self, pal};
use mind_core::render::trail::Trail;

use crate::render::canvas::{self, RegionCache};
use crate::sim_host::{BulletDraw, MindSimHost};

/// Culling margin around the camera rect (world px).
const CULL_MARGIN: f32 = 64.0;

/// `ContinuousFlameBulletType.lengthWidthPans`
/// (`ContinuousFlameBulletType.java:25-31`): lengths, widths and ellipse pans,
/// interleaved as `LWPO1 LWPO2 LWPO3...` (five colour layers).
const FLAME_LENGTH_WIDTH_PANS: [f32; 15] = [
    1.12, 1.3, 0.32, //
    1.0, 1.0, 0.3, //
    0.8, 0.9, 0.2, //
    0.5, 0.8, 0.15, //
    0.25, 0.7, 0.1,
];
/// `ContinuousFlameBulletType.flareColor` (`:18`, `Color.valueOf("e189f5")`).
const FLAME_FLARE_COLOR: Rgba = Rgba::from_rgba8888(0xe189f5ff);
/// `ContinuousFlameBulletType.flareWidth` (`:19`, 3).
const FLAME_FLARE_WIDTH: f32 = 3.0;
/// `ContinuousFlameBulletType.flareLength` (`:19`, 40).
const FLAME_FLARE_LENGTH: f32 = 40.0;
/// `ContinuousFlameBulletType.flareInnerScl` (`:19`, 0.5).
const FLAME_FLARE_INNER_SCL: f32 = 0.5;
/// `ContinuousFlameBulletType.flareInnerLenScl` (`:19`, 0.5).
const FLAME_FLARE_INNER_LEN_SCL: f32 = 0.5;
/// `ContinuousFlameBulletType.flareRotSpeed` (`:19`, 1.2).
const FLAME_FLARE_ROT_SPEED: f32 = 1.2;

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
    Circle {
        x: f32,
        y: f32,
        r: f32,
        stroke: f32,
        color: Color,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        width: f32,
        color: Color,
    },
    Poly {
        points: Vec<(f32, f32)>,
        color: Color,
    },
}

/// Bullet pass (`Node2D` under the `Layer.bullet` band).
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct BulletRenderer {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
    node: Option<Gd<Node2D>>,
    regions: RegionCache,
    /// `Bullet.id` → trail (`BulletType.updateTrail`); the stamp is the frame
    /// the trail was last seen for (unseen trails are dropped).
    trails: HashMap<u64, (Trail, u64)>,
    frame: u64,
    /// This frame's bullet snapshot (`process` fills, `draw` consumes).
    bullets: Vec<BulletDraw>,
}

#[godot_api]
impl INode2D for BulletRenderer {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            host: None,
            node: None,
            regions: RegionCache::new(),
            trails: HashMap::new(),
            frame: 0,
            bullets: Vec::new(),
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
        self.frame += 1;
        let Some(host) = self.host.clone() else {
            return;
        };
        let guard = host.bind();
        let Some(content) = guard.content_registry() else {
            return;
        };
        let bullets = guard.bullet_draws();
        for bullet in &bullets {
            let Some(def) = content.bullet(bullet.def) else {
                continue;
            };
            if def.trail_length > 0 {
                let fin = (bullet.time / bullet.lifetime.max(1e-4)).clamp(0.0, 1.0);
                let trail = self
                    .trails
                    .entry(bullet.id)
                    .or_insert_with(|| (Trail::new(def.trail_length as usize), self.frame));
                trail
                    .0
                    .update(bullet.x, bullet.y, interp(def.trail_interp, fin));
                trail.1 = self.frame;
            }
        }
        self.bullets = bullets;
        let frame = self.frame;
        self.trails.retain(|_, (_, last)| *last == frame);
        self.base_mut().queue_redraw();
    }

    fn draw(&mut self) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let Some(node) = self.node.clone() else {
            return;
        };
        let mut cmds: Vec<Cmd> = Vec::new();
        {
            let guard = host.bind();
            let Some(content) = guard.content_registry() else {
                return;
            };
            let visible = self.visible_rect();
            // `Time.time` (the effect clock; frozen while paused like Java).
            let tick = guard.get_tick() as f32;
            for bullet in self.bullets.clone() {
                if let Some((min_x, min_y, max_x, max_y)) = visible
                    && (bullet.x < min_x - CULL_MARGIN
                        || bullet.y < min_y - CULL_MARGIN
                        || bullet.x > max_x + CULL_MARGIN
                        || bullet.y > max_y + CULL_MARGIN)
                {
                    continue;
                }
                let Some(def) = content.bullet(bullet.def) else {
                    continue;
                };
                // Trails first, under the body (`z - 0.0001`).
                if let Some((trail, _)) = self.trails.get(&bullet.id) {
                    let mut quads: Vec<[f32; 8]> = Vec::new();
                    trail.draw(def.trail_width, &mut quads);
                    let color = color4(rgba(def.trail_color));
                    for q in &quads {
                        cmds.push(Cmd::Poly {
                            points: vec![(q[0], q[1]), (q[2], q[3]), (q[4], q[5]), (q[6], q[7])],
                            color,
                        });
                    }
                    if let Some((cx, cy, w, _rot)) = trail.draw_cap(def.trail_width) {
                        cmds.push(Cmd::Circle {
                            x: cx,
                            y: cy,
                            r: w / 2.0,
                            stroke: 0.0,
                            color,
                        });
                    }
                }
                self.push_bullet(&mut cmds, &node, &bullet, def, content, tick);
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
                Cmd::Circle {
                    x,
                    y,
                    r,
                    stroke,
                    color,
                } => {
                    if *stroke > 0.0 {
                        base.draw_arc_ex(
                            Vector2::new(*x, *y),
                            *r,
                            0.0,
                            std::f32::consts::TAU,
                            32,
                            *color,
                        )
                        .width(*stroke)
                        .done();
                    } else {
                        base.draw_circle(Vector2::new(*x, *y), *r, *color);
                    }
                }
                Cmd::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    width,
                    color,
                } => {
                    base.draw_line_ex(Vector2::new(*x1, *y1), Vector2::new(*x2, *y2), *color)
                        .width((*width).max(1.0))
                        .done();
                }
                Cmd::Poly { points, color } => {
                    let packed: PackedVector2Array = points
                        .iter()
                        .map(|(x, y)| Vector2::new(*x, *y))
                        .collect::<Vec<_>>()
                        .into_iter()
                        .collect();
                    base.draw_colored_polygon(&packed, *color);
                }
            }
        }
    }
}

impl BulletRenderer {
    /// Rebuilds Godot-derived refs (`ready` + hot reload).
    fn bootstrap(&mut self) {
        self.host = self
            .base()
            .try_get_node_as::<MindSimHost>("/root/Spine/SimHost");
        self.node = Some(self.to_gd().upcast::<Node2D>());
        self.base_mut().queue_redraw();
    }

    /// One bullet's body by kind (`BulletType.draw` dispatch).
    fn push_bullet(
        &mut self,
        cmds: &mut Vec<Cmd>,
        node: &Gd<Node2D>,
        bullet: &BulletDraw,
        def: &BulletDef,
        content: &ContentRegistry,
        tick: f32,
    ) {
        let fin = (bullet.time / bullet.lifetime.max(1e-4)).clamp(0.0, 1.0);
        let fout = 1.0 - fin;
        match def.kind {
            BulletKind::Basic
            | BulletKind::Missile
            | BulletKind::Artillery
            | BulletKind::Flak
            | BulletKind::Bomb
            | BulletKind::Emp
            | BulletKind::Interceptor
            | BulletKind::LaserBolt => {
                let shrink = interp(def.shrink_interp, fin);
                let (w, h) = bullet_draw::basic_size(
                    def.width,
                    def.height,
                    def.shrink_x,
                    def.shrink_y,
                    shrink,
                );
                let seed = if def.spin != 0.0 {
                    // `Mathf.randomSeed(b.id, 360)` replaced by a stable hash
                    // (documented deviation).
                    let hash = bullet.id.wrapping_mul(0x9E37_79B9_7F4A_7C15);
                    ((hash >> 40) % 360) as f32 + bullet.time * def.spin
                } else {
                    0.0
                };
                let rot = bullet.rotation - 90.0 + seed + def.rotation_offset;
                let mut back = rgba(def.back_color);
                let mut front = rgba(def.front_color);
                let mix = lerp4(rgba(def.mix_color_from), rgba(def.mix_color_to), fin);
                if mix[3] > 0.0 {
                    back = lerp4(back, mix, mix[3]);
                    front = lerp4(front, mix, mix[3]);
                }
                let sprite = def.sprite.as_deref().unwrap_or("bullet");
                let back_name = def
                    .back_sprite
                    .clone()
                    .unwrap_or_else(|| format!("{sprite}-back"));
                if let Some(texture) = self.region(node, &back_name) {
                    cmds.push(Cmd::Region {
                        texture: Some(texture),
                        x: bullet.x,
                        y: bullet.y,
                        w,
                        h,
                        rot,
                        color: color4(back),
                    });
                }
                if let Some(texture) = self.region(node, sprite) {
                    cmds.push(Cmd::Region {
                        texture: Some(texture),
                        x: bullet.x,
                        y: bullet.y,
                        w,
                        h,
                        rot,
                        color: color4(front),
                    });
                }
                if def.kind == BulletKind::LaserBolt {
                    // `LaserBoltBulletType.draw` (`:26-34`): the port merged
                    // Basic's width/height fields into `def.width/def.height`;
                    // Java uses `width`/`height` for both parts (documented).
                    let stroke = def.width.max(1.0);
                    let back_line =
                        line_angle_center(bullet.x, bullet.y, bullet.rotation, def.height);
                    cmds.push(Cmd::Line {
                        x1: back_line.0,
                        y1: back_line.1,
                        x2: back_line.2,
                        y2: back_line.3,
                        width: stroke,
                        color: color4(rgba(def.back_color)),
                    });
                    let front_line =
                        line_angle_center(bullet.x, bullet.y, bullet.rotation, def.height / 2.0);
                    cmds.push(Cmd::Line {
                        x1: front_line.0,
                        y1: front_line.1,
                        x2: front_line.2,
                        y2: front_line.3,
                        width: stroke,
                        color: color4(rgba(def.front_color)),
                    });
                }
            }
            BulletKind::Laser => {
                // `LaserBulletType.draw` (`:76-104`): `fdata` is the resolved
                // beam length (`LaserBulletType.java:15-33`).
                let real_length = bullet.fdata;
                let f = curve(fin, 0.0, 0.2);
                let base_len = real_length * f;
                let colors = if def.colors.is_empty() {
                    vec![
                        drawf::with_alpha(pal::LANCER_LASER, 0.4),
                        pal::LANCER_LASER,
                        [1.0, 1.0, 1.0, 1.0],
                    ]
                } else {
                    def.colors.iter().map(|color| rgba(*color)).collect()
                };
                let layers = bullet_draw::laser_layers(def.width.max(1.0), 0.5, fout, colors.len());
                for (layer, (color, (stroke, tri_len, circle_r, side_len))) in
                    colors.iter().zip(layers.iter()).enumerate()
                {
                    let _ = layer;
                    let line = line_angle(bullet.x, bullet.y, bullet.rotation, base_len);
                    cmds.push(Cmd::Line {
                        x1: line.0,
                        y1: line.1,
                        x2: line.2,
                        y2: line.3,
                        width: *stroke,
                        color: color4(*color),
                    });
                    let (vx, vy) = drawf::trns(bullet.rotation, base_len);
                    let tip = drawf::tri(
                        bullet.x + vx,
                        bullet.y + vy,
                        *stroke,
                        *tri_len,
                        bullet.rotation,
                    );
                    cmds.push(Cmd::Poly {
                        points: tip.to_vec(),
                        color: color4(*color),
                    });
                    cmds.push(Cmd::Circle {
                        x: bullet.x,
                        y: bullet.y,
                        r: *circle_r * fout,
                        stroke: 0.0,
                        color: color4(*color),
                    });
                    for side in [-1.0f32, 1.0] {
                        let angle = bullet.rotation + def.side_angle * side;
                        let wing = drawf::tri(
                            bullet.x,
                            bullet.y,
                            def.side_width * fout * circle_r,
                            *side_len,
                            angle,
                        );
                        cmds.push(Cmd::Poly {
                            points: wing.to_vec(),
                            color: color4(*color),
                        });
                    }
                }
            }
            BulletKind::Shrapnel => {
                // `ShrapnelBulletType.draw` (`:55-70`).
                let real_length = bullet.fdata;
                let color = lerp4(rgba(def.from_color), rgba(def.to_color), fin);
                let count = (def.serrations as f32 * real_length / def.length.max(1e-3)) as i32;
                for i in 0..count {
                    let (vx, vy) = drawf::trns(bullet.rotation, i as f32 * def.serration_spacing);
                    let sl = (fout - def.serration_fade_offset).clamp(0.0, 1.0)
                        * (def.serration_space_offset - i as f32 * def.serration_len_scl);
                    for side in [90.0f32, -90.0] {
                        let serration = drawf::tri(
                            bullet.x + vx,
                            bullet.y + vy,
                            def.serration_width,
                            sl,
                            bullet.rotation + side,
                        );
                        cmds.push(Cmd::Poly {
                            points: serration.to_vec(),
                            color: color4(color),
                        });
                    }
                }
                let body = drawf::tri(
                    bullet.x,
                    bullet.y,
                    def.width * fout,
                    real_length + 4.0,
                    bullet.rotation,
                );
                cmds.push(Cmd::Poly {
                    points: body.to_vec(),
                    color: color4(color),
                });
                let tail = drawf::tri(
                    bullet.x,
                    bullet.y,
                    def.width * fout,
                    10.0,
                    bullet.rotation + 180.0,
                );
                cmds.push(Cmd::Poly {
                    points: tail.to_vec(),
                    color: color4(color),
                });
            }
            BulletKind::ContinuousLaser => {
                // `ContinuousLaserBulletType.draw` (`:48-74`): one line +
                // back/front `Drawf.flameFront` ellipse per colour layer.
                let fout =
                    bullet_draw::continuous_laser_fout(bullet.time, bullet.lifetime, def.fade_time);
                let target = def.length * fout;
                // The behavior's `Damage.findLength` cut (`fdata`) is at most
                // the fade-scaled length; before the first damage tick it is
                // still 0, so fall back to the full beam.
                let real_length = if bullet.fdata > 0.0 {
                    bullet.fdata.min(target)
                } else {
                    target
                };
                let colors: Vec<Rgba> = if def.colors.is_empty() {
                    vec![
                        Rgba::from_rgba8888(0xec745855),
                        Rgba::from_rgba8888(0xec7458aa),
                        Rgba::from_rgba8888(0xff9c5aff),
                        Rgba::WHITE,
                    ]
                } else {
                    def.colors.clone()
                };
                let layers = bullet_draw::continuous_laser_layers(
                    def.width,
                    def.stroke_from,
                    def.stroke_to,
                    def.pointy_scaling,
                    def.osc_scl,
                    def.osc_mag,
                    tick,
                    fout,
                    colors.len(),
                );
                for (color, layer) in colors.iter().zip(layers.iter()) {
                    if layer.stroke <= 0.0 {
                        continue;
                    }
                    let tint = color4(rgba(*color));
                    let body_length = (real_length - def.front_length).max(0.0);
                    let line = line_angle(bullet.x, bullet.y, bullet.rotation, body_length);
                    cmds.push(Cmd::Line {
                        x1: line.0,
                        y1: line.1,
                        x2: line.2,
                        y2: line.3,
                        width: layer.stroke,
                        color: tint,
                    });
                    // Back ellipse at `rot + 180`.
                    let back = drawf::flame_front_points(
                        bullet.x,
                        bullet.y,
                        def.divisions,
                        bullet.rotation + 180.0,
                        def.back_length,
                        layer.stroke / 2.0,
                    );
                    if back.len() >= 3 {
                        cmds.push(Cmd::Poly {
                            points: back,
                            color: tint,
                        });
                    }
                    // Front ellipse at the beam tip.
                    let (vx, vy) = drawf::trns(bullet.rotation, body_length);
                    let front = drawf::flame_front_points(
                        bullet.x + vx,
                        bullet.y + vy,
                        def.divisions,
                        bullet.rotation,
                        def.front_length * layer.ellipse_len_scl,
                        layer.stroke / 2.0,
                    );
                    if front.len() >= 3 {
                        cmds.push(Cmd::Poly {
                            points: front,
                            color: tint,
                        });
                    }
                }
            }
            BulletKind::ContinuousFlame => {
                // `ContinuousFlameBulletType.draw` (`:49-79`): one
                // `Drawf.flame` polygon per colour layer plus the four-triangle
                // flare. `lengthInterp` is the class default `Interp.slope`.
                let mult = bullet_draw::continuous_flame_length_mult(fin);
                let target = def.length * mult;
                let real_length = if bullet.fdata > 0.0 {
                    bullet.fdata.min(target)
                } else {
                    target
                };
                let sin = bullet_draw::continuous_osc(tick, def.osc_scl.max(1e-6), def.osc_mag);
                let colors: Vec<Rgba> = if def.colors.is_empty() {
                    vec![
                        Rgba::from_rgba8888(0xeb7abe8c),
                        Rgba::from_rgba8888(0xe189f5b3),
                        Rgba::from_rgba8888(0x907ef7cc),
                        Rgba::from_rgba8888(0x91a4ffff),
                        Rgba::WHITE,
                    ]
                } else {
                    def.colors.clone()
                };
                for (index, color) in colors.iter().enumerate() {
                    let tint = bullet_draw::continuous_flame_color(rgba(*color), tick);
                    let Some((length, width, pan)) = bullet_draw::continuous_flame_layer(
                        index,
                        real_length,
                        def.width,
                        mult,
                        sin,
                        &FLAME_LENGTH_WIDTH_PANS,
                    ) else {
                        break;
                    };
                    let points = drawf::flame_points(
                        bullet.x,
                        bullet.y,
                        def.divisions,
                        bullet.rotation,
                        length,
                        width,
                        pan,
                    );
                    if points.len() >= 3 {
                        cmds.push(Cmd::Poly {
                            points,
                            color: color4(tint),
                        });
                    }
                }
                // `drawFlare` (class default true): four `flareColor` triangles
                // then four inner white triangles (`:61-78`).
                let angle = tick * FLAME_FLARE_ROT_SPEED;
                for step in 0..4 {
                    let rotation = step as f32 * 90.0 + 45.0 + angle;
                    let flare = drawf::tri(
                        bullet.x,
                        bullet.y,
                        FLAME_FLARE_WIDTH,
                        FLAME_FLARE_LENGTH * (mult + sin),
                        rotation,
                    );
                    cmds.push(Cmd::Poly {
                        points: flare.to_vec(),
                        color: color4(rgba(FLAME_FLARE_COLOR)),
                    });
                }
                for step in 0..4 {
                    let rotation = step as f32 * 90.0 + 45.0 + angle;
                    let inner = drawf::tri(
                        bullet.x,
                        bullet.y,
                        FLAME_FLARE_WIDTH * FLAME_FLARE_INNER_SCL,
                        FLAME_FLARE_LENGTH * FLAME_FLARE_INNER_LEN_SCL * (mult + sin),
                        rotation,
                    );
                    cmds.push(Cmd::Poly {
                        points: inner.to_vec(),
                        color: Color::WHITE,
                    });
                }
            }
            BulletKind::Liquid => {
                // `LiquidBulletType.draw` (`:70-81`), non-boiling branch.
                let liquid_color = content
                    .liquid(def.puddle_liquid)
                    .map(|liquid| rgba(liquid.color))
                    .unwrap_or([0.35, 0.42, 0.72, 1.0]);
                let color = lerp4(liquid_color, [1.0, 1.0, 1.0, 1.0], fout / 100.0);
                cmds.push(Cmd::Circle {
                    x: bullet.x,
                    y: bullet.y,
                    r: 3.0,
                    stroke: 0.0,
                    color: color4(color),
                });
            }
            // Trail-only kinds (`Plain`, `Fire`, `SpaceLiquid`, `Sap`,
            // `Lightning`, `Explosion`, `Rail`, `Continuous*`, `Multi`,
            // `Point`, `PointLaser`, `MassDriver`, `Empty`).
            _ => {}
        }
    }

    /// Resolves an atlas region through the shared cache.
    fn region(&mut self, node: &Gd<Node2D>, name: &str) -> Option<Gd<Texture2D>> {
        self.regions.get(node, name)
    }

    /// World-space visible rect from the camera transform (`None` without a
    /// camera).
    fn visible_rect(&self) -> Option<(f32, f32, f32, f32)> {
        let camera = self
            .base()
            .try_get_node_as::<Camera2D>("/root/Spine/World/Camera2D")?;
        let viewport = self.base().get_viewport()?;
        let rect: Rect2 = viewport.get_visible_rect();
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

/// `InterpKind` → `Interp` (the content enum is metadata-only).
fn interp(kind: InterpKind, a: f32) -> f32 {
    match kind {
        InterpKind::Linear => Interp::Linear.apply(a),
        InterpKind::One => Interp::One.apply(a),
        InterpKind::Slope => Interp::Slope.apply(a),
        InterpKind::Pow2In => Interp::Pow2In.apply(a),
        InterpKind::Pow5In => Interp::Pow5In.apply(a),
    }
}

/// `Mathf.curve(a, from, to)`: `clamp((a - from) / (to - from), 0, 1)`.
fn curve(a: f32, from: f32, to: f32) -> f32 {
    ((a - from) / (to - from)).clamp(0.0, 1.0)
}

/// `Lines.lineAngleCenter(point)`: centre, length, rotation.
fn line_angle_center(x: f32, y: f32, rotation: f32, length: f32) -> (f32, f32, f32, f32) {
    let (vx, vy) = drawf::trns(rotation, length);
    (x - vx / 2.0, y - vy / 2.0, x + vx / 2.0, y + vy / 2.0)
}

/// `Lines.lineAngle(x, y, angle, length)`: from the origin along the angle.
fn line_angle(x: f32, y: f32, rotation: f32, length: f32) -> (f32, f32, f32, f32) {
    let (vx, vy) = drawf::trns(rotation, length);
    (x, y, x + vx, y + vy)
}

/// Core `Rgba` → `[f32; 4]`.
fn rgba(color: Rgba) -> [f32; 4] {
    [color.r, color.g, color.b, color.a]
}

/// `[f32; 4]` → Godot `Color`.
fn color4(color: [f32; 4]) -> Color {
    Color::from_rgba(color[0], color[1], color[2], color[3])
}

/// `Color.lerp`.
fn lerp4(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}
