// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DebugCollisionRenderer` (`graphics/DebugCollisionRenderer.java`, plan 16
//! §3.8 / M6). Draws block hitboxes and exposed solid-tile edges at
//! `Layer::OverlayUi` when `drawhitboxes` is on.
//!
//! Deviation S16-6: unit/entity hitboxes, avoidance squares and physics circles
//! need plan 11's unit components and plan 05's groups; this slice draws the
//! tile/block half (green block footprints + magenta solid edges). The unit half
//! lands with plan 11.

use godot::classes::{Mesh, MeshInstance2D, Node2D};
use godot::prelude::*;

use mind_core::config::TILESIZE;
use mind_core::render::scan::CameraView;

use crate::sim_host::MindSimHost;

use super::atlas_bind::{Quad, build_mesh};

/// Green block-hitbox tint (`Color.green, 0.3`).
const HITBOX_COLOR: u32 = 0x00ff004d;
/// Magenta solid-edge tint (`Color.magenta`, full alpha; thin quads).
const EDGE_COLOR: u32 = 0xff00ffff;
/// Edge stroke thickness in world pixels (~`Lines.stroke(0.4f)` rounded up).
const EDGE_STROKE: f32 = 1.0;

/// `DebugCollisionRenderer` counters.
#[derive(Clone, Copy, Debug, Default)]
pub struct DebugStats {
    /// Whether the pass is currently visible.
    pub visible: bool,
    /// Hitbox quads emitted in the last rebuild.
    pub hitboxes: i64,
}

/// Debug collision pass.
pub struct DebugCollisionRenderer {
    host: Gd<MindSimHost>,
    node: Gd<MeshInstance2D>,
    last_revision: i64,
    stats: DebugStats,
}

impl DebugCollisionRenderer {
    /// Builds the pass over the `Layer::OverlayUi` band.
    pub fn new(host: Gd<MindSimHost>, band: Gd<Node2D>) -> Self {
        let node = MeshInstance2D::new_alloc();
        // code-instantiated: one pooled mesh for the debug overlay; the quad
        // count is data-driven from the visible solid tiles.
        band.clone().add_child(&node);
        Self {
            host,
            node,
            last_revision: i64::MIN,
            stats: DebugStats::default(),
        }
    }

    /// Current counters.
    pub fn stats(&self) -> DebugStats {
        self.stats
    }

    /// Rebuilds the overlay when enabled and the world changed.
    pub fn update(&mut self, view: &CameraView, enabled: bool) {
        self.stats.visible = enabled;
        if !enabled {
            self.node.set_visible(false);
            return;
        }
        self.node.set_visible(true);

        let revision = self.host.clone().bind().world_revision();
        if revision == self.last_revision {
            return;
        }
        self.last_revision = revision;

        let (quads, hitboxes) = {
            let host = self.host.clone();
            let host = host.bind();
            let world = host.grid();
            let Some(content) = host.content_registry() else {
                return;
            };
            let size = TILESIZE as f32;
            let mut quads = Vec::new();
            let mut hitboxes = 0i64;
            let x1 = ((view.x - view.w / 2.0) / size).floor() as i32;
            let y1 = ((view.y - view.h / 2.0) / size).floor() as i32;
            let x2 = ((view.x + view.w / 2.0) / size).ceil() as i32;
            let y2 = ((view.y + view.h / 2.0) / size).ceil() as i32;
            for y in y1.max(0)..y2.min(world.height()) {
                for x in x1.max(0)..x2.min(world.width()) {
                    let tile = world.tile(x, y);
                    let solid = content
                        .block(tile.block)
                        .map(|def| def.solid)
                        .unwrap_or(false);
                    if !solid {
                        continue;
                    }
                    let cx = (x as f32 + 0.5) * size;
                    let cy = (y as f32 + 0.5) * size;
                    quads.push(Quad {
                        cx,
                        cy,
                        w: size,
                        h: size,
                        uv: [0.0, 0.0, 1.0, 1.0],
                        color: HITBOX_COLOR,
                    });
                    hitboxes += 1;
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
                            push_edge(&mut quads, cx, cy, dx, dy, size);
                        }
                    }
                }
            }
            (quads, hitboxes)
        };

        if let Some(mesh) = build_mesh(&quads, 0.0) {
            self.node.set_mesh(&mesh.upcast::<Mesh>());
        }
        self.stats.hitboxes = hitboxes;
    }
}

/// Pushes a thin edge quad on the given side of a solid tile.
fn push_edge(quads: &mut Vec<Quad>, cx: f32, cy: f32, dx: i32, dy: i32, size: f32) {
    let half = size / 2.0;
    let (w, h, ox, oy) = if dx != 0 {
        (EDGE_STROKE, size, dx as f32 * half, 0.0)
    } else {
        (size, EDGE_STROKE, 0.0, dy as f32 * half)
    };
    quads.push(Quad {
        cx: cx + ox,
        cy: cy + oy,
        w,
        h,
        uv: [0.0, 0.0, 1.0, 1.0],
        color: EDGE_COLOR,
    });
}
