// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live unit/weapon `DrawPart` pass (L04 T1).
//!
//! `UnitType.draw` runs the unit's `parts` after the body/weapons/items
//! (`type/UnitType.java:1561-1578`), and `Weapon.draw` runs its own `parts`
//! around the weapon region (`type/Weapon.java:236-278`). The hand-written unit
//! pass draws body/cell/weapon quads only; this node executes the compiled
//! part program (all `DrawPart` kinds) against the live mount state.
//!
//! Three nodes share the work per unit band, mirroring the turret pass:
//! weapon `under` parts below the unit mesh (`z_index = -1`), unit parts and
//! weapon over-parts above it (`z_index = 1`), and the additive half of those
//! at `z_index = 2` (`RegionPart.blending = additive` heat/glow regions).

use godot::builtin::{Color, PackedVector2Array, Rect2, Vector2};
use godot::classes::canvas_item_material::BlendMode;
use godot::classes::notify::CanvasItemNotification;
use godot::classes::{CanvasItemMaterial, INode2D, Node2D, Texture2D};
use godot::obj::{Base, NewGd};
use godot::prelude::*;

use mind_core::fx::parts::PartParams;
use mind_core::fx::parts::draw::PartEmit;
use mind_core::fx::parts::weapons::{part_recoil, weapon_part_params, weapon_pose};
use mind_core::render::draw::{Blending, DrawProgram, PrimKind, RegionKey};
use mind_core::render::layer::Layer;

use crate::assets::MindAssets;
use crate::fx::draw_turret::AtlasLookup;
use crate::sim_host::{MindSimHost, UnitDraw};

use super::canvas::{self, RegionCache};

/// Which half of `Weapon.draw` this node renders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartsPhase {
    /// Weapon `under` parts (`DrawPart.under`), below the unit body.
    Under,
    /// Unit `parts` + weapon over-parts, normal blend.
    Over,
    /// The additive-blend half of [`PartsPhase::Over`].
    OverAdditive,
}

/// One interpreted part draw command.
enum Cmd {
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
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
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
    Polygon {
        points: Vec<(f32, f32)>,
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
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        stroke: f32,
        color: Color,
        cap: bool,
    },
    Polyline {
        points: Vec<(f32, f32)>,
        stroke: f32,
        color: Color,
    },
    Arc {
        x: f32,
        y: f32,
        r: f32,
        start_deg: f32,
        sweep_deg: f32,
        stroke: f32,
        color: Color,
    },
}

/// Live unit-part pass; one node per `(unit band, phase)`.
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct UnitPartsRenderer {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
    assets: Option<Gd<MindAssets>>,
    /// Unit band index (`0` ground, `1` flying-low, `2` flying).
    pub(crate) band: usize,
    /// Which `DrawPart` half this node draws.
    pub(crate) phase: PartsPhase,
    /// Scratch program (retained across frames).
    program: DrawProgram,
    /// Atlas texture cache (resolved before `base_mut()`).
    regions: RegionCache,
}

impl PartsPhase {
    fn z_index(self) -> i32 {
        match self {
            PartsPhase::Under => -1,
            PartsPhase::Over => 1,
            PartsPhase::OverAdditive => 2,
        }
    }

    fn additive(self) -> bool {
        self == PartsPhase::OverAdditive
    }
}

#[godot_api]
impl INode2D for UnitPartsRenderer {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            host: None,
            assets: None,
            band: 0,
            phase: PartsPhase::Over,
            program: DrawProgram::new(),
            regions: RegionCache::new(),
        }
    }

    fn ready(&mut self) {
        if self.phase.additive() {
            let mut material = CanvasItemMaterial::new_gd();
            material.set_blend_mode(BlendMode::ADD);
            self.base_mut().set_material(&material);
        }
        let z_index = self.phase.z_index();
        self.base_mut().set_z_index(z_index);
        self.bootstrap();
    }

    fn on_notification(&mut self, what: CanvasItemNotification) {
        if what == CanvasItemNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }

    fn process(&mut self, _delta: f64) {
        if self.host.is_some() {
            // Warmup/reload/recoil/charge animate every frame.
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
        let node = self.to_gd().upcast::<Node2D>();
        let phase = self.phase;
        let band = self.band;
        let draws = host.bind().unit_draws_full();
        if draws.is_empty() {
            return;
        }
        let tick = host.bind().get_tick() as f32;
        let lookup = AtlasLookup { assets: &assets };
        let mut program = std::mem::take(&mut self.program);
        program.clear();
        for unit in &draws {
            if unit_band(unit) != band {
                continue;
            }
            let z = unit_z(unit);
            match phase {
                PartsPhase::Under => {
                    let mut emit = PartEmit::new(&mut program, &lookup, tick);
                    emit.set_z(z);
                    draw_weapon_parts(&mut emit, unit, false);
                }
                PartsPhase::Over | PartsPhase::OverAdditive => {
                    let mut emit = PartEmit::new(&mut program, &lookup, tick);
                    emit.set_z(z);
                    for part in &unit.parts {
                        let params = part_params(unit, part);
                        emit.draw_part(part, &params, &unit.name);
                    }
                    draw_weapon_parts(&mut emit, unit, true);
                }
            }
        }
        // Stable z sort: Java's sprite batch order across parts/layer offsets.
        program.sort();

        let additive = phase.additive();
        let mut cmds: Vec<Cmd> = Vec::new();
        for prim in &program.prims {
            if (prim.blend == Blending::Additive) != additive {
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
                    if *region == RegionKey::ERROR {
                        continue;
                    }
                    let Some(texture) = self.regions.get(&node, region.0) else {
                        continue;
                    };
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
                PrimKind::Rect { x, y, w, h, color } => cmds.push(Cmd::Rect {
                    x: *x,
                    y: *y,
                    w: *w,
                    h: *h,
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
                PrimKind::Polygon {
                    points,
                    fill,
                    stroke,
                    color,
                } => cmds.push(Cmd::Polygon {
                    points: points.iter().copied().collect(),
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
                PrimKind::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    stroke,
                    color,
                    cap,
                } => cmds.push(Cmd::Line {
                    x1: *x1,
                    y1: *y1,
                    x2: *x2,
                    y2: *y2,
                    stroke: *stroke,
                    color: color_of(*color),
                    cap: *cap,
                }),
                PrimKind::Polyline {
                    points,
                    stroke,
                    color,
                } => cmds.push(Cmd::Polyline {
                    points: points.iter().copied().collect(),
                    stroke: *stroke,
                    color: color_of(*color),
                }),
                PrimKind::Arc {
                    x,
                    y,
                    r,
                    start_deg,
                    sweep_deg,
                    stroke,
                    color,
                } => cmds.push(Cmd::Arc {
                    x: *x,
                    y: *y,
                    r: *r,
                    start_deg: *start_deg,
                    sweep_deg: *sweep_deg,
                    stroke: *stroke,
                    color: color_of(*color),
                }),
                // `Light` is routed to the light pass; effects/shader layers
                // are not emitted by any unit part.
                _ => {}
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
                Cmd::Rect { x, y, w, h, color } => canvas::fill_rect(
                    &mut base,
                    x - w / 2.0,
                    y - h / 2.0,
                    x + w / 2.0,
                    y + h / 2.0,
                    color,
                ),
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
                    let ring = mind_core::render::drawf::poly_ring(x, y, sides as u32, r, rot);
                    if fill {
                        if !valid_polygon(&ring) {
                            continue;
                        }
                        let packed: PackedVector2Array = ring
                            .iter()
                            .map(|(px, py)| Vector2::new(*px, *py))
                            .collect::<Vec<_>>()
                            .into_iter()
                            .collect();
                        base.draw_colored_polygon(&packed, color);
                    } else {
                        let mut ring = ring;
                        if let Some(first) = ring.first().copied() {
                            ring.push(first);
                        }
                        canvas::poly_line(&mut base, &ring, stroke, color);
                    }
                }
                Cmd::Polygon {
                    points,
                    fill,
                    stroke,
                    color,
                } => {
                    if fill {
                        if !valid_polygon(&points) {
                            continue;
                        }
                        let packed: PackedVector2Array = points
                            .iter()
                            .map(|(px, py)| Vector2::new(*px, *py))
                            .collect::<Vec<_>>()
                            .into_iter()
                            .collect();
                        base.draw_colored_polygon(&packed, color);
                    } else {
                        canvas::poly_line(&mut base, &points, stroke, color);
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
                    let local = [(-w / 2.0, 0.0), (w / 2.0, 0.0), (0.0, h)];
                    let (sin, cos) = rot.to_radians().sin_cos();
                    let points: Vec<(f32, f32)> = local
                        .iter()
                        .map(|(lx, ly)| (x + lx * cos - ly * sin, y + lx * sin + ly * cos))
                        .collect();
                    if !valid_polygon(&points) {
                        continue;
                    }
                    let packed: PackedVector2Array = points
                        .into_iter()
                        .map(|(px, py)| Vector2::new(px, py))
                        .collect();
                    base.draw_colored_polygon(&packed, color);
                }
                Cmd::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    stroke,
                    color,
                    cap,
                } => {
                    canvas::poly_line(&mut base, &[(x1, y1), (x2, y2)], stroke, color);
                    if cap {
                        canvas::fill_circle(&mut base, x1, y1, stroke / 2.0, color);
                        canvas::fill_circle(&mut base, x2, y2, stroke / 2.0, color);
                    }
                }
                Cmd::Polyline {
                    points,
                    stroke,
                    color,
                } => canvas::poly_line(&mut base, &points, stroke, color),
                Cmd::Arc {
                    x,
                    y,
                    r,
                    start_deg,
                    sweep_deg,
                    stroke,
                    color,
                } => {
                    let steps = ((sweep_deg.abs() / 5.0).ceil() as usize).max(2);
                    let mut points = Vec::with_capacity(steps + 1);
                    for i in 0..=steps {
                        let angle = start_deg + sweep_deg * i as f32 / steps as f32;
                        let (sin, cos) = angle.to_radians().sin_cos();
                        points.push((x + cos * r, y + sin * r));
                    }
                    canvas::poly_line(&mut base, &points, stroke, color);
                }
            }
        }
    }
}

impl UnitPartsRenderer {
    /// Rebuilds Godot-derived refs (`ready` + hot reload).
    fn bootstrap(&mut self) {
        self.host = self
            .base()
            .try_get_node_as::<MindSimHost>("/root/Spine/SimHost");
        self.assets = self
            .base()
            .try_get_node_as::<MindAssets>("/root/MindAssets");
        self.base_mut().queue_redraw();
    }
}

/// Band index of a unit (`UnitRenderer` bucket order).
fn unit_band(unit: &UnitDraw) -> usize {
    if unit.low_altitude {
        1
    } else if unit.flying {
        2
    } else {
        0
    }
}

/// `UnitType.draw` z for the unit (`UnitType.java:1487-1493`).
fn unit_z(unit: &UnitDraw) -> f32 {
    match unit_band(unit) {
        1 => Layer::FlyingUnitLow.z(),
        2 => Layer::FlyingUnit.z(),
        _ => unit.ground_layer + (unit.hit_size / 4000.0).clamp(0.0, 0.01),
    }
}

/// `DrawPart.params` for a unit part (`UnitType.java:1563-1577`).
fn part_params(
    unit: &UnitDraw,
    part: &mind_core::content::registries::units::parts::DrawPartSpec,
) -> PartParams {
    let mut params = PartParams::default();
    let mount = unit
        .weapons
        .get(part.weapon_index.max(0) as usize)
        .map(|weapon| &weapon.mount);
    if let Some(mount) = mount {
        let reload = unit
            .weapons
            .get(part.weapon_index.max(0) as usize)
            .map(|weapon| {
                if weapon.def.reload != 0.0 {
                    mount.reload / weapon.def.reload
                } else {
                    0.0
                }
            })
            .unwrap_or(0.0);
        params.set(
            mount.warmup,
            reload,
            mount.smooth_reload,
            mount.heat,
            mount.recoil,
            mount.charge,
            unit.x,
            unit.y,
            unit.rotation,
        );
    } else {
        params.set(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, unit.x, unit.y, unit.rotation);
    }
    params.life = unit.fin;
    params
}

/// `Weapon.draw`'s under/over parts (`Weapon.java:236-278`).
fn draw_weapon_parts(emit: &mut PartEmit, unit: &UnitDraw, over: bool) {
    for weapon in &unit.weapons {
        let pose = weapon_pose(unit.x, unit.y, unit.rotation, &weapon.mount, &weapon.def);
        let mut params = weapon_part_params(pose, &weapon.mount, &weapon.def);
        let z = emit.z();
        emit.set_z(z + weapon.def.layer_offset);
        for part in &weapon.def.parts {
            if part.under != over {
                continue;
            }
            params.recoil = part_recoil(part, &weapon.mount);
            emit.draw_part(part, &params, &weapon.def.name);
        }
        emit.set_z(z);
    }
}

fn color_of(rgba: mind_core::content::Rgba) -> Color {
    Color::from_rgba(rgba.r, rgba.g, rgba.b, rgba.a)
}

/// Godot rejects degenerate/self-intersecting fills (`triangulation failed`);
/// skip zero-area rings before they hit `draw_colored_polygon`.
fn valid_polygon(points: &[(f32, f32)]) -> bool {
    if points.len() < 3 || points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
        return false;
    }
    let mut area = 0.0f32;
    for i in 0..points.len() {
        let (x1, y1) = points[i];
        let (x2, y2) = points[(i + 1) % points.len()];
        area += x1 * y2 - x2 * y1;
    }
    area.abs() > 1e-4
}

/// Draws a region with a fractional origin exactly like the turret pass.
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
