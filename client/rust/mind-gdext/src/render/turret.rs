// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live `DrawTurret` pass (J03 T1).
//!
//! Executes the plan-17 `DrawTurret` program (`mind_core::fx::parts::turret`,
//! ported from `world/draw/DrawTurret.java:72-170`) for every visible turret,
//! fed from the live `TurretState` (`MindSimHost::turret_draw_input`) and the
//! real atlas (`AtlasLookup`). Two nodes share the work — a normal-blend node
//! under the `Layer.turret` band and an additive node under `Layer.turretHeat`
//! (the heat overlay, `Drawf.additive`) — so the painted z order matches
//! Java's sprite batch: shadow (`turretLayer - 0.5`), outline
//! (`turretLayer - 0.01`, only when the drawer has parts), head/liquid/top,
//! then every part at its own layer.
//!
//! The turret **base** stays in the block pass (`blocks.rs`) at `Layer.block`,
//! so this pass drops the program's base primitive (z below the turret
//! layers).

use std::collections::HashSet;

use godot::builtin::{Color, Rect2, Vector2};
use godot::classes::canvas_item_material::BlendMode;
use godot::classes::notify::CanvasItemNotification;
use godot::classes::{CanvasItemMaterial, INode2D, Node2D, Texture2D};
use godot::obj::{Base, NewGd};
use godot::prelude::*;

use mind_core::render::draw::{Blending, DrawProgram, PrimKind};
use mind_core::render::layer::Layer;
use mind_core::render::scan::{CameraView, visible_blocks};

use crate::assets::MindAssets;
use crate::fx::draw_turret::{AtlasLookup, TurretDrawInput, build_turret};
use crate::sim_host::MindSimHost;

use super::canvas::{self, RegionCache};

/// One interpreted turret draw command.
enum Cmd {
    /// One textured quad with a fractional origin (Java batch `originX/Y`).
    Region {
        texture: Gd<Texture2D>,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        rot: f32,
        origin: (f32, f32),
        color: Color,
    },
    Poly {
        x: f32,
        y: f32,
        sides: u16,
        r: f32,
        rot: f32,
        fill: bool,
        stroke: f32,
        color: Color,
    },
    Circle {
        x: f32,
        y: f32,
        r: f32,
        fill: bool,
        stroke: f32,
        color: Color,
    },
    Tri {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        rot: f32,
        color: Color,
    },
}

/// Live turret draw pass (`Node2D` under one band; `additive` selects the
/// `Layer.turretHeat` half).
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct TurretRenderer {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
    assets: Option<Gd<MindAssets>>,
    node: Option<Gd<Node2D>>,
    regions: RegionCache,
    /// `true` on the `BLEND_MODE_ADD` heat node.
    pub(crate) additive: bool,
    /// Scratch program (retained across frames, plan 17 audit).
    program: DrawProgram,
    /// Last frame's emitted regions `(name, x, y, w, h, rot, color)` — MCP
    /// debug probe (`debug_last_prims`).
    last_prims: Vec<(String, f32, f32, f32, f32, f32, u32)>,
}

#[godot_api]
impl INode2D for TurretRenderer {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            host: None,
            assets: None,
            node: None,
            regions: RegionCache::new(),
            additive: false,
            program: DrawProgram::new(),
            last_prims: Vec::new(),
        }
    }

    fn ready(&mut self) {
        if self.additive {
            let mut material = CanvasItemMaterial::new_gd();
            material.set_blend_mode(BlendMode::ADD);
            self.base_mut().set_material(&material);
        }
        self.bootstrap();
    }

    fn on_notification(&mut self, what: CanvasItemNotification) {
        if what == CanvasItemNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }

    fn process(&mut self, _delta: f64) {
        if self.host.is_some() {
            // Warmup/heat/recoil animate every frame.
            self.base_mut().queue_redraw();
        }
    }

    fn draw(&mut self) {
        // Collect first: `base_mut()` borrows `self` for the whole draw.
        let Some(host) = self.host.clone() else {
            return;
        };
        let Some(assets) = self.assets.clone() else {
            return;
        };
        let Some(node) = self.node.clone() else {
            return;
        };
        let Some(rect) = canvas::visible_world_rect(&node) else {
            return;
        };
        let additive = self.additive;
        self.last_prims.clear();
        let mut inputs: Vec<TurretDrawInput> = Vec::new();
        {
            let host_ref = host.bind();
            let Some(content) = host_ref.content_registry() else {
                return;
            };
            let world = host_ref.grid();
            let view = CameraView {
                x: rect.0 + rect.2 / 2.0,
                y: rect.1 + rect.3 / 2.0,
                w: rect.2 + 32.0,
                h: rect.3 + 32.0,
                zoom: 1.0,
                team: 0,
            };
            let mut seen = HashSet::new();
            for tile in visible_blocks(world, content, &view) {
                let Some(def) = content.block(tile.block) else {
                    continue;
                };
                if !is_draw_turret_kind(def.kind) {
                    continue;
                }
                // Multiblock: draw once per center entity (first row-major tile).
                if let Some(entity) = world.tile(tile.x, tile.y).build
                    && !seen.insert(entity)
                {
                    continue;
                }
                if let Some(input) = host_ref.turret_draw_input(tile.x, tile.y) {
                    inputs.push(input);
                }
            }
        }

        let tick = host.bind().get_tick() as f32;
        let lookup = AtlasLookup { assets: &assets };
        let mut program = std::mem::take(&mut self.program);
        let mut cmds: Vec<Cmd> = Vec::new();
        for input in &inputs {
            program.clear();
            build_turret(&mut program, &lookup, tick, Layer::Block.z(), input);
            // Sorted (stable) by z: Java's sprite batch order.
            program.sort();
            for prim in &program.prims {
                if prim.blend != blend_of(additive) {
                    continue;
                }
                // The base is drawn by the block pass at `Layer.block`.
                if prim.z <= Layer::Block.z() {
                    continue;
                }
                match &prim.kind {
                    PrimKind::Region {
                        region,
                        x,
                        y,
                        w,
                        h,
                        rotation_deg,
                        origin,
                        color,
                        ..
                    } => {
                        // A missing lookup keeps `RegionKey::ERROR`; Java never
                        // reaches this (`Draw.rect` is gated on `region.found()`
                        // by every caller). Drawing the placeholder here would
                        // stamp a magenta blob over the turret.
                        if *region == mind_core::render::draw::RegionKey::ERROR {
                            continue;
                        }
                        let Some(texture) = self.regions.get(&node, region.0) else {
                            continue;
                        };
                        self.last_prims.push((
                            region.0.to_owned(),
                            *x,
                            *y,
                            *w,
                            *h,
                            *rotation_deg,
                            ((color.r * 255.0) as u32) << 24
                                | ((color.g * 255.0) as u32) << 16
                                | ((color.b * 255.0) as u32) << 8
                                | (color.a * 255.0) as u32,
                        ));
                        cmds.push(Cmd::Region {
                            texture,
                            x: *x,
                            y: *y,
                            w: *w,
                            h: *h,
                            rot: *rotation_deg,
                            origin: *origin,
                            color: color_of(*color),
                        });
                    }
                    PrimKind::Poly {
                        x,
                        y,
                        sides,
                        r,
                        rotation_deg,
                        fill,
                        stroke,
                        color,
                    } => cmds.push(Cmd::Poly {
                        x: *x,
                        y: *y,
                        sides: *sides,
                        r: *r,
                        rot: *rotation_deg,
                        fill: *fill,
                        stroke: *stroke,
                        color: color_of(*color),
                    }),
                    PrimKind::Circle {
                        x,
                        y,
                        r,
                        fill,
                        stroke,
                        color,
                    } => cmds.push(Cmd::Circle {
                        x: *x,
                        y: *y,
                        r: *r,
                        fill: *fill,
                        stroke: *stroke,
                        color: color_of(*color),
                    }),
                    PrimKind::Tri {
                        x,
                        y,
                        w,
                        h,
                        rotation_deg,
                        color,
                    } => cmds.push(Cmd::Tri {
                        x: *x,
                        y: *y,
                        w: *w,
                        h: *h,
                        rot: *rotation_deg,
                        color: color_of(*color),
                    }),
                    // `PrimKind::Light` (part heat lights) is collected by the
                    // light pass, not drawn here; lines/arcs are not emitted by
                    // any vanilla turret part.
                    _ => {}
                }
            }
        }
        self.program = program;

        let mut base = self.base_mut();
        for cmd in cmds {
            match cmd {
                Cmd::Region {
                    texture,
                    x,
                    y,
                    w,
                    h,
                    rot,
                    origin,
                    color,
                } => draw_region_origin(&mut base, &texture, x, y, w, h, rot, origin, color),
                Cmd::Poly {
                    x,
                    y,
                    sides,
                    r,
                    rot,
                    fill,
                    stroke,
                    color,
                } => {
                    if sides < 3 || r <= 0.0 {
                        continue;
                    }
                    if fill {
                        let ring = mind_core::render::drawf::poly_ring(x, y, sides as u32, r, rot);
                        let packed: PackedVector2Array = ring
                            .iter()
                            .map(|(px, py)| Vector2::new(*px, *py))
                            .collect::<Vec<_>>()
                            .into_iter()
                            .collect();
                        base.draw_colored_polygon(&packed, color);
                    } else {
                        let mut ring =
                            mind_core::render::drawf::poly_ring(x, y, sides as u32, r, rot);
                        if let Some(first) = ring.first().copied() {
                            ring.push(first);
                        }
                        canvas::poly_line(&mut base, &ring, stroke, color);
                    }
                }
                Cmd::Circle {
                    x,
                    y,
                    r,
                    fill,
                    stroke,
                    color,
                } => {
                    if fill {
                        canvas::fill_circle(&mut base, x, y, r, color);
                    } else {
                        canvas::circle_stroke(&mut base, x, y, r, stroke, color);
                    }
                }
                Cmd::Tri {
                    x,
                    y,
                    w,
                    h,
                    rot,
                    color,
                } => {
                    // `Drawf.tri`: apex forward (+y local), base behind.
                    let local = [(-w / 2.0, 0.0), (w / 2.0, 0.0), (0.0, h)];
                    let (sin, cos) = rot.to_radians().sin_cos();
                    let mut packed = PackedVector2Array::new();
                    for (lx, ly) in local {
                        packed.push(Vector2::new(
                            x + lx * cos - ly * sin,
                            y + lx * sin + ly * cos,
                        ));
                    }
                    base.draw_colored_polygon(&packed, color);
                }
            }
        }
    }
}

#[godot_api]
impl TurretRenderer {
    /// MCP debug probe: the last frame's emitted region prims as JSON
    /// (`[{name,x,y,w,h,rot,color}]`) for the node's blend half.
    #[func]
    pub fn debug_last_prims(&self) -> GString {
        let rows: Vec<serde_json::Value> = self
            .last_prims
            .iter()
            .map(|(name, x, y, w, h, rot, color)| {
                serde_json::json!({
                    "name": name, "x": x, "y": y, "w": w, "h": h,
                    "rot": rot, "color": format!("{color:08x}")
                })
            })
            .collect();
        GString::from(serde_json::Value::Array(rows).to_string().as_str())
    }

    /// Live turret head prim count drawn last frame (debug).
    #[func]
    pub fn debug_region_count(&self) -> i64 {
        self.last_prims.len() as i64
    }
}

impl TurretRenderer {
    /// Rebuilds Godot-derived refs (`ready` + hot reload).
    fn bootstrap(&mut self) {
        self.host = self
            .base()
            .try_get_node_as::<MindSimHost>("/root/Spine/SimHost");
        self.assets = self
            .base()
            .try_get_node_as::<MindAssets>("/root/MindAssets");
        self.node = Some(self.to_gd().upcast::<Node2D>());
        self.base_mut().queue_redraw();
    }
}

/// `DrawTurret.draw` draws every `Turret` subclass (`Turret.java:161`).
/// `RepairTurret`/`RepairPoint` extend `Block` with a bespoke draw, so their
/// kind is excluded.
fn is_draw_turret_kind(kind: mind_core::content::BlockKind) -> bool {
    use mind_core::content::BlockKind;
    matches!(
        kind,
        BlockKind::ItemTurret
            | BlockKind::LiquidTurret
            | BlockKind::PowerTurret
            | BlockKind::ContinuousTurret
            | BlockKind::ContinuousLiquidTurret
            | BlockKind::LaserTurret
            | BlockKind::PointDefenseTurret
            | BlockKind::TractorBeamTurret
            | BlockKind::BuildTurret
    )
}

fn blend_of(additive: bool) -> Blending {
    if additive {
        Blending::Additive
    } else {
        Blending::Normal
    }
}

fn color_of(rgba: mind_core::content::Rgba) -> Color {
    Color::from_rgba(rgba.r, rgba.g, rgba.b, rgba.a)
}

/// Draws a region with a fractional origin exactly like `MindFx::draw_prim`
/// (negative-height rect for the y-up camera; origin anchors like Java's
/// batch).
#[allow(clippy::too_many_arguments)]
fn draw_region_origin(
    canvas: &mut Node2D,
    texture: &Gd<Texture2D>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rot: f32,
    origin: (f32, f32),
    color: Color,
) {
    canvas
        .draw_set_transform_ex(Vector2::new(x, y))
        .rotation(rot.to_radians())
        .done();
    canvas
        .draw_texture_rect_ex(
            texture,
            Rect2::new(
                Vector2::new(-w * origin.0, -h * origin.1),
                Vector2::new(w, -h),
            ),
            false,
        )
        .modulate(color)
        .done();
    canvas.draw_set_transform(Vector2::ZERO);
}
