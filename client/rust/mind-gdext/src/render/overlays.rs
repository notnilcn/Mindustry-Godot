// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `OverlayRenderer` host (`graphics/OverlayRenderer.java`, plan 16 §3.7/M5).
//!
//! Draws the polygon core-protection edges at `Layer::OverlayUi` (the
//! `updateCoreEdges` half). The input-plan/selection/indicator/status half is
//! driven by plan 15 data and plan 07 status bars; its pure geometry lives in
//! `mind_core::render::overlay`. Deviation S16-13: team colors are a fixed
//! palette until plan 02/12 expose `Team.color`.

use godot::classes::{Mesh, MeshInstance2D, Node2D};
use godot::prelude::*;

use mind_core::render::scan::CameraView;
use mind_core::render::{build_core_edges, core_edge_displayed};

use super::atlas_bind::{Quad, build_mesh};

/// Overlay counters.
#[derive(Clone, Copy, Debug, Default)]
pub struct OverlayStats {
    /// Cores in the current edge set.
    pub cores: i64,
    /// Displayed core edges.
    pub edges: i64,
    /// Whether the overlay drew this frame.
    pub active: bool,
}

/// Team color palette (plan 02/12 replacement seam).
fn team_color(team: u8) -> u32 {
    match team {
        0 => 0x8f8f8fff,
        1 => 0xd4816bff,
        2 => 0xe55454ff,
        3 => 0x6bb2b2ff,
        _ => 0x877badff,
    }
}

/// The core-protection edge overlay.
pub struct OverlayRenderer {
    node: Gd<MeshInstance2D>,
    cores: Vec<(f32, f32, u8)>,
    player_team: u8,
    stats: OverlayStats,
}

impl OverlayRenderer {
    /// Builds the pass over the `Layer::OverlayUi` band.
    pub fn new(band: Gd<Node2D>) -> Self {
        let node = MeshInstance2D::new_alloc();
        // code-instantiated: one pooled mesh for the Voronoi edge set; the quad
        // count is data-driven from the protected cores.
        band.clone().add_child(&node);
        Self {
            node,
            cores: Vec::new(),
            player_team: 1,
            stats: OverlayStats::default(),
        }
    }

    /// Sets the viewing team (`player.team()`).
    pub fn set_player_team(&mut self, team: u8) {
        self.player_team = team;
    }

    /// Replaces the protected-core list `(x, y, team)`.
    pub fn set_cores(&mut self, cores: Vec<(f32, f32, u8)>) {
        self.cores = cores;
    }

    /// Current counters.
    pub fn stats(&self) -> OverlayStats {
        self.stats
    }

    /// Rebuilds the edge mesh from the current cores (Voronoi).
    pub fn update(&mut self, view: &CameraView) {
        self.stats.cores = self.cores.len() as i64;
        if self.cores.is_empty() {
            self.node.set_visible(false);
            self.stats.edges = 0;
            self.stats.active = false;
            return;
        }
        // Voronoi over the whole world; the view bounds approximate the map.
        let edges = build_core_edges(
            &self.cores,
            view.x - view.w,
            view.x + view.w,
            view.y - view.h,
            view.y + view.h,
            |_| true,
        );
        let mut quads = Vec::new();
        let mut displayed = 0i64;
        for edge in &edges {
            if let Some(team) = core_edge_displayed(self.player_team, edge.team1, edge.team2) {
                displayed += 1;
                line_quad(
                    &mut quads,
                    edge.x1,
                    edge.y1,
                    edge.x2,
                    edge.y2,
                    1.0,
                    team_color(team),
                );
            }
        }
        self.stats.edges = displayed;
        self.stats.active = displayed > 0;
        if quads.is_empty() {
            self.node.set_visible(false);
            return;
        }
        self.node.set_visible(true);
        if let Some(mesh) = build_mesh(&quads, 0.0) {
            self.node.set_mesh(&mesh.upcast::<Mesh>());
        }
    }
}

/// Emits a thin quad along a segment.
fn line_quad(quads: &mut Vec<Quad>, x1: f32, y1: f32, x2: f32, y2: f32, w: f32, color: u32) {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let m = (dx * dx + dy * dy).sqrt();
    if m < 1e-6 {
        return;
    }
    let nx = -dy / m * w / 2.0;
    let ny = dx / m * w / 2.0;
    // Approximate with two axis-aligned quads: horizontal and vertical spans.
    quads.push(Quad {
        cx: (x1 + x2) / 2.0,
        cy: (y1 + y2) / 2.0,
        w: dx.abs().max(w),
        h: dy.abs().max(w),
        uv: [0.0, 0.0, 1.0, 1.0],
        color,
    });
    quads.push(Quad {
        cx: (x1 + x2) / 2.0 + nx,
        cy: (y1 + y2) / 2.0 + ny,
        w: dx.abs().max(w),
        h: dy.abs().max(w),
        uv: [0.0, 0.0, 1.0, 1.0],
        color,
    });
}
