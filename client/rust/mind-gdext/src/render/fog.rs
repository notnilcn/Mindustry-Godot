// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `FogRenderer` host (`graphics/FogRenderer.java`, plan 16 §3.7/M6).
//!
//! Owns the packed `FogEvent` queue and the fog composite. Plan 12 supplies the
//! discovered bitsets (`FogControl`) and `Rules.staticFog`; until it lands this
//! host composites a single view-covering quad tinted by `Rules.dynamicColor`
//! when `Rules.fog` is on (the `Shaders.fog` band), which is the same band the
//! full static/dynamic FBO pair replaces. Deviation S16-8: the world-sized
//! static/dynamic `ImageTexture` pair and `FogControl::copy_from_cpu` are gated
//! on plan 12 (contract frozen in plan 16 §3.12).

use godot::classes::{Mesh, MeshInstance2D, Node2D};
use godot::prelude::*;

use mind_core::render::drawf::to_bits;
use mind_core::render::fog::fog_event;
use mind_core::render::rules::RulesRenderView;
use mind_core::render::scan::CameraView;

use super::atlas_bind::{Quad, build_mesh};

/// Fog pass counters.
#[derive(Clone, Copy, Debug, Default)]
pub struct FogStats {
    /// Queued static fog events.
    pub events: i64,
    /// Whether the fog composite drew this frame.
    pub active: bool,
}

/// `FogRenderer` host.
pub struct FogRenderer {
    node: Gd<MeshInstance2D>,
    events: Vec<i64>,
    stats: FogStats,
}

impl FogRenderer {
    /// Builds the pass over the `Layer::FogOfWar` band.
    pub fn new(band: Gd<Node2D>) -> Self {
        let node = MeshInstance2D::new_alloc();
        // code-instantiated: one composite quad for the fog pass; the node count
        // is fixed by the pipeline.
        band.clone().add_child(&node);
        node.clone().set_visible(false);
        Self {
            node,
            events: Vec::new(),
            stats: FogStats::default(),
        }
    }

    /// `FogRenderer.handleEvent(packed)` (`ClientHooks::fog_handle_event`).
    pub fn handle_event(&mut self, packed: i64) {
        self.events.push(packed);
    }

    /// Queues a fog event from tile coordinates.
    pub fn push_event(&mut self, x: i32, y: i32, radius: i32, team: u8) {
        self.handle_event(fog_event(x, y, radius, team));
    }

    /// Current counters.
    pub fn stats(&self) -> FogStats {
        self.stats
    }

    /// `FogRenderer.drawFog` (plan-12-free composite).
    pub fn update(&mut self, view: &CameraView, rules: &RulesRenderView) {
        self.stats.events = self.events.len() as i64;
        self.stats.active = rules.fog;
        if !rules.fog {
            self.node.set_visible(false);
            self.events.clear();
            return;
        }
        self.node.set_visible(true);
        let color = rules.dynamic_fog_color();
        let quad = Quad {
            cx: view.x,
            cy: view.y,
            w: view.w,
            h: view.h,
            uv: [0.0, 0.0, 1.0, 1.0],
            color: to_bits(color),
        };
        if let Some(mesh) = build_mesh(&[quad], 0.0) {
            self.node.set_mesh(&mesh.upcast::<Mesh>());
        }
        // Events are consumed by the plan-12 static target once it lands.
        self.events.clear();
    }
}
