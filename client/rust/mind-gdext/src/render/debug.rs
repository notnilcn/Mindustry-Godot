// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DebugCollisionRenderer` (`graphics/DebugCollisionRenderer.java`, plan 16
//! §3.8 / M6). Draws the three Java debug passes at `Layer::OverlayUi` when
//! `drawhitboxes` is on:
//!
//! 1. green `Fill.square(x, y, hitSize/2)` for every drawable hitbox (units),
//! 2. magenta solid-tile edge lines plus the tile-aligned hitbox rect of every
//!    grounded unit (`Unit.hitboxTile`, `HitboxComp.java:69-75`),
//! 3. red physics circles at `hitSize * unitCollisionRadiusScale`
//!    (`Vars.java:121`, radius scale 0.6).
//!
//! Java paints these through `Draw`/`Fill`/`Lines`; the port bakes the same
//! flat-colour geometry into one untextured `MeshInstance2D` mesh (a canvas
//! `CanvasItem` would need a Godot class, and the pass is instanced from
//! `render.rs`). The mesh rebuilds every frame because units move; `view` only
//! bounds the solid-tile scan.

use godot::classes::mesh::PrimitiveType;
use godot::classes::{ArrayMesh, Mesh, MeshInstance2D, Node2D, SurfaceTool};
use godot::obj::NewGd;
use godot::prelude::*;

use mind_core::config::TILESIZE;
use mind_core::render::scan::CameraView;

use crate::sim_host::MindSimHost;

use super::atlas_bind::color_from;

/// Green hitbox fill (`Draw.color(Color.green, 0.3f)`).
const HITBOX_COLOR: u32 = 0x00ff_004d;
/// Magenta line tint (`Lines.stroke(0.4f, Color.magenta)`).
const EDGE_COLOR: u32 = 0xff00_ffff;
/// Red physics-circle tint (`Draw.color(Color.red, 0.5f)`).
const PHYSICS_COLOR: u32 = 0xff00_0080;
/// Stroke width in world pixels. Java uses `Lines.stroke(0.4f)`/`stroke(0.5f)`;
/// the port clamps to one pixel so the unantialiased mesh stays visible.
const EDGE_STROKE: f32 = 1.0;
/// `Vars.unitCollisionRadiusScale` (`Vars.java:121`).
const UNIT_COLLISION_RADIUS_SCALE: f32 = 0.6;
/// `HitboxComp.hitboxTile` clamps the tile hitbox to `min(hitSize * 0.66, 7.8)`
/// (`HitboxComp.java:69-75`).
const TILE_HITBOX_SCALE: f32 = 0.66;
const TILE_HITBOX_MAX: f32 = 7.8;
/// Arc `Lines.circlePrecision` (`Lines.java:17`).
const CIRCLE_PRECISION: f32 = 0.4;

/// `DebugCollisionRenderer` counters.
#[derive(Clone, Copy, Debug, Default)]
pub struct DebugStats {
    /// Whether the pass is currently visible.
    pub visible: bool,
    /// Unit hitboxes emitted in the last rebuild.
    pub hitboxes: i64,
}

/// One flat-colour debug-mesh vertex (the node's canvas texture is the default
/// white, so only position/colour matter).
#[derive(Clone, Copy)]
struct Vertex {
    x: f32,
    y: f32,
    color: u32,
}

/// Debug collision pass.
pub struct DebugCollisionRenderer {
    host: Gd<MindSimHost>,
    node: Gd<MeshInstance2D>,
    stats: DebugStats,
}

impl DebugCollisionRenderer {
    /// Builds the pass over the `Layer::OverlayUi` band.
    pub fn new(host: Gd<MindSimHost>, band: Gd<Node2D>) -> Self {
        let mut node = MeshInstance2D::new_alloc();
        node.set_name("DebugCollision");
        // code-instantiated: one pooled mesh for the debug overlay; the
        // triangle count is data-driven from the visible tiles and units.
        band.clone().add_child(&node);
        // I02 T2: hidden until the first mesh lands.
        node.set_visible(false);
        Self {
            host,
            node,
            stats: DebugStats::default(),
        }
    }

    /// Current counters.
    pub fn stats(&self) -> DebugStats {
        self.stats
    }

    /// Rebuilds the overlay when enabled (units move every tick, so there is no
    /// world-revision early-out like the tile passes have).
    pub fn update(&mut self, view: &CameraView, enabled: bool) {
        self.stats.visible = enabled;
        if !enabled {
            self.node.set_visible(false);
            return;
        }

        let mut tris: Vec<[Vertex; 3]> = Vec::new();
        let mut hitboxes = 0i64;
        {
            let host = self.host.clone();
            let host = host.bind();
            let world = host.grid();
            let Some(content) = host.content_registry() else {
                self.node.set_visible(false);
                return;
            };
            let size = TILESIZE as f32;
            let x1 = ((view.x - view.w / 2.0) / size).floor() as i32;
            let y1 = ((view.y - view.h / 2.0) / size).floor() as i32;
            let x2 = ((view.x + view.w / 2.0) / size).ceil() as i32;
            let y2 = ((view.y + view.h / 2.0) / size).ceil() as i32;
            for y in y1.max(0)..y2.min(world.height()) {
                for x in x1.max(0)..x2.min(world.width()) {
                    let solid = content
                        .block(world.tile(x, y).block)
                        .map(|def| def.solid)
                        .unwrap_or(false);
                    if !solid {
                        continue;
                    }
                    let cx = x as f32 * size + size / 2.0;
                    let cy = y as f32 * size + size / 2.0;
                    for (dx, dy) in [(-1i32, 0i32), (0, -1), (1, 0), (0, 1)] {
                        let nx = x + dx;
                        let ny = y + dy;
                        let neighbor_solid = nx >= 0
                            && ny >= 0
                            && nx < world.width()
                            && ny < world.height()
                            && content
                                .block(world.tile(nx, ny).block)
                                .map(|def| def.solid)
                                .unwrap_or(false);
                        if !neighbor_solid {
                            push_edge(&mut tris, cx, cy, dx, dy, size);
                        }
                    }
                }
            }
            // `Group.draw` hitboxes: `Fill.square(x, y, hitSize/2)` for every
            // drawable with a hitbox, plus the unit-only tile rect and physics
            // circle below (`DebugCollisionRenderer.java:32-78`).
            for unit in host.unit_draws_full() {
                push_box(
                    &mut tris,
                    unit.x,
                    unit.y,
                    unit.hit_size,
                    unit.hit_size,
                    HITBOX_COLOR,
                );
                hitboxes += 1;
                if !unit.flying {
                    let tile = unit_tile_hitbox(unit.hit_size);
                    push_rect_outline(
                        &mut tris,
                        unit.x,
                        unit.y,
                        tile,
                        tile,
                        EDGE_STROKE,
                        EDGE_COLOR,
                    );
                }
                push_circle(
                    &mut tris,
                    unit.x,
                    unit.y,
                    unit.hit_size * UNIT_COLLISION_RADIUS_SCALE,
                    EDGE_STROKE,
                    PHYSICS_COLOR,
                );
            }
        }

        // I02 T2: no geometry -> keep the node hidden (an empty `ArrayMesh` is
        // still a draw call, and a mesh-less visible node is the
        // `mesh_get_surface_count` error).
        match build_colored_mesh(&tris) {
            Some(mesh) => {
                self.node.set_mesh(&mesh.upcast::<Mesh>());
                self.node.set_visible(true);
            }
            None => self.node.set_visible(false),
        }
        self.stats.hitboxes = hitboxes;
    }
}

/// `HitboxComp.hitboxTile` size (`HitboxComp.java:69-75`): tile hitboxes are
/// never bigger than a tile so units do not get stuck.
fn unit_tile_hitbox(hit_size: f32) -> f32 {
    (hit_size * TILE_HITBOX_SCALE).min(TILE_HITBOX_MAX)
}

/// Arc `Lines.circleVertices(rad)` (`Lines.java:24-26`).
fn circle_vertices(radius: f32) -> usize {
    (11.0 + radius * CIRCLE_PRECISION).max(3.0) as usize
}

/// Bakes the flat-colour triangles into one untextured canvas mesh.
fn build_colored_mesh(tris: &[[Vertex; 3]]) -> Option<Gd<ArrayMesh>> {
    if tris.is_empty() {
        return None;
    }
    let mut tool = SurfaceTool::new_gd();
    tool.begin(PrimitiveType::TRIANGLES);
    for tri in tris {
        for vertex in tri {
            tool.set_color(color_from(vertex.color));
            // The debug node has no atlas texture; the canvas default (white)
            // is sampled at a constant UV.
            tool.set_uv(Vector2::new(0.5, 0.5));
            tool.add_vertex(Vector3::new(vertex.x, vertex.y, 0.0));
        }
    }
    tool.commit()
}

fn vertex(x: f32, y: f32, color: u32) -> Vertex {
    Vertex { x, y, color }
}

/// Pushes a flat-colour quad as two triangles.
fn push_quad(tris: &mut Vec<[Vertex; 3]>, points: [(f32, f32); 4], color: u32) {
    let [a, b, c, d] = points;
    tris.push([
        vertex(a.0, a.1, color),
        vertex(b.0, b.1, color),
        vertex(c.0, c.1, color),
    ]);
    tris.push([
        vertex(a.0, a.1, color),
        vertex(c.0, c.1, color),
        vertex(d.0, d.1, color),
    ]);
}

/// `Fill.square(x, y, radius)` (`Fill.java:357-359`): a centred square of side
/// `radius * 2`.
fn push_box(tris: &mut Vec<[Vertex; 3]>, cx: f32, cy: f32, w: f32, h: f32, color: u32) {
    let (hw, hh) = (w / 2.0, h / 2.0);
    push_quad(
        tris,
        [
            (cx - hw, cy - hh),
            (cx + hw, cy - hh),
            (cx + hw, cy + hh),
            (cx - hw, cy + hh),
        ],
        color,
    );
}

/// One stroked segment (the mesh counterpart of `Lines.line`).
fn push_line(
    tris: &mut Vec<[Vertex; 3]>,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    stroke: f32,
    color: u32,
) {
    let (dx, dy) = (x2 - x1, y2 - y1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-4 {
        return;
    }
    let (nx, ny) = (-dy / len * stroke / 2.0, dx / len * stroke / 2.0);
    push_quad(
        tris,
        [
            (x1 + nx, y1 + ny),
            (x2 + nx, y2 + ny),
            (x2 - nx, y2 - ny),
            (x1 - nx, y1 - ny),
        ],
        color,
    );
}

/// `Lines.rect` outline with the current stroke.
fn push_rect_outline(
    tris: &mut Vec<[Vertex; 3]>,
    cx: f32,
    cy: f32,
    w: f32,
    h: f32,
    stroke: f32,
    color: u32,
) {
    let (hw, hh) = (w / 2.0, h / 2.0);
    let corners = [
        (cx - hw, cy - hh),
        (cx + hw, cy - hh),
        (cx + hw, cy + hh),
        (cx - hw, cy + hh),
    ];
    for index in 0..4 {
        let (x1, y1) = corners[index];
        let (x2, y2) = corners[(index + 1) % 4];
        push_line(tris, x1, y1, x2, y2, stroke, color);
    }
}

/// `Lines.circle(x, y, rad)` (`Lines.java:320-322`): a `circleVertices(rad)`
/// line loop at the current stroke.
fn push_circle(
    tris: &mut Vec<[Vertex; 3]>,
    cx: f32,
    cy: f32,
    radius: f32,
    stroke: f32,
    color: u32,
) {
    if radius <= 0.0 {
        return;
    }
    let sides = circle_vertices(radius);
    let step = std::f32::consts::TAU / sides as f32;
    let mut previous = (cx + radius, cy);
    for index in 1..=sides {
        let angle = step * index as f32;
        let point = (cx + angle.cos() * radius, cy + angle.sin() * radius);
        push_line(
            tris, previous.0, previous.1, point.0, point.1, stroke, color,
        );
        previous = point;
    }
}

/// Pushes a thin edge line on the given side of a solid tile
/// (`DebugCollisionRenderer.java:47-62`).
fn push_edge(tris: &mut Vec<[Vertex; 3]>, cx: f32, cy: f32, dx: i32, dy: i32, size: f32) {
    let half = size / 2.0;
    let (x1, y1, x2, y2) = if dx != 0 {
        let x = cx + dx as f32 * half;
        (x, cy - half, x, cy + half)
    } else {
        let y = cy + dy as f32 * half;
        (cx - half, y, cx + half, y)
    };
    push_line(tris, x1, y1, x2, y2, EDGE_STROKE, EDGE_COLOR);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `HitboxComp.hitboxTile` (`HitboxComp.java:69-75`): `hitSize * 0.66`
    /// clamped to `7.8`, so the unit tile rect never exceeds a tile.
    #[test]
    fn tile_hitbox_clamps_to_the_java_cap() {
        assert!((unit_tile_hitbox(10.0) - 6.6).abs() < 1e-5);
        assert_eq!(unit_tile_hitbox(20.0), TILE_HITBOX_MAX);
        assert_eq!(unit_tile_hitbox(0.0), 0.0);
    }

    /// Arc `Lines.circleVertices(rad) = 11 + (int)(rad * 0.4)`
    /// (`Lines.java:17,24-26`).
    #[test]
    fn circle_vertex_count_matches_arc() {
        assert_eq!(circle_vertices(0.0), 11);
        assert_eq!(circle_vertices(10.0), 15);
        assert_eq!(circle_vertices(25.0), 21);
    }

    /// The physics circle uses `hitSize * 0.6` (`Vars.java:121`); the green
    /// hitbox square is `hitSize` across (`Fill.square(x, y, hitSize/2)`,
    /// `DebugCollisionRenderer.java:36-39`).
    #[test]
    fn unit_hitbox_geometry_matches_java() {
        assert!((12.0 * UNIT_COLLISION_RADIUS_SCALE - 7.2).abs() < 1e-5);
        let mut tris = Vec::new();
        push_box(&mut tris, 5.0, 6.0, 12.0, 12.0, HITBOX_COLOR);
        assert_eq!(tris.len(), 2, "a fill square is two triangles");
        let min_x = tris
            .iter()
            .flat_map(|tri| tri.iter())
            .map(|v| v.x)
            .fold(f32::INFINITY, f32::min);
        let max_x = tris
            .iter()
            .flat_map(|tri| tri.iter())
            .map(|v| v.x)
            .fold(f32::NEG_INFINITY, f32::max);
        assert_eq!((min_x, max_x), (-1.0, 11.0), "square side = hitSize");
    }

    /// Every primitive is finite and the ring/segment builders emit geometry
    /// for a zero-radius or degenerate input without panicking.
    #[test]
    fn line_and_circle_geometry_is_finite() {
        let mut tris = Vec::new();
        push_circle(&mut tris, 0.0, 0.0, 10.0, 1.0, PHYSICS_COLOR);
        assert_eq!(tris.len(), circle_vertices(10.0) * 2);
        assert!(
            tris.iter()
                .flat_map(|tri| tri.iter())
                .all(|v| v.x.is_finite() && v.y.is_finite())
        );
        let mut degenerate = Vec::new();
        push_line(&mut degenerate, 1.0, 1.0, 1.0, 1.0, 1.0, EDGE_COLOR);
        push_circle(&mut degenerate, 0.0, 0.0, 0.0, 1.0, EDGE_COLOR);
        assert!(degenerate.is_empty(), "no zero-length geometry");
    }
}
