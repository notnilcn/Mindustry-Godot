// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `EnvRenderers` host (`graphics/EnvRenderers.java`, plan 16 §3.8/M6).
//!
//! Owns the ordered [`EnvRegistry`] and the default underwater pass. The
//! scorching noise pass and the particle/ray FX bodies are plan 17's; this
//! module draws the underwater tint quad at `Layer.light + 1` (reusing the
//! `Layer::Light` band) and exposes the selection contract
//! `add_env_renderer(env, pass)`.
//!
//! Deviation S16-7: the additive caustics blit and 50-ray/suspended-particle
//! passes need plan 03's `rays`/`particle` region binding and plan 17's
//! `Weather.drawParticles`; this slice draws the water-tint fill only.

use godot::classes::{Mesh, MeshInstance2D, Node2D};
use godot::prelude::*;

use mind_core::render::drawf::to_bits;
use mind_core::render::env::{EnvRegistry, UNDERWATER, underwater};
use mind_core::render::scan::CameraView;

use super::atlas_bind::{Quad, build_mesh};

/// Env pass counters.
#[derive(Clone, Copy, Debug, Default)]
pub struct EnvStats {
    /// Whether the underwater pass drew this frame.
    pub underwater: bool,
}

/// `EnvRenderers` host.
pub struct EnvRenderer {
    node: Gd<MeshInstance2D>,
    registry: EnvRegistry,
    rules_env: u32,
    stats: EnvStats,
}

impl EnvRenderer {
    /// Builds the pass over a band node (the `Layer::Light` band).
    pub fn new(band: Gd<Node2D>) -> Self {
        let node = MeshInstance2D::new_alloc();
        // code-instantiated: one pooled mesh for the env fill; the quad count is
        // fixed by the pass, not content.
        band.clone().add_child(&node);
        let mut registry = EnvRegistry::new();
        registry.add(UNDERWATER, "underwater");
        registry.add(mind_core::render::env::SCORCHING, "scorching");
        Self {
            node,
            registry,
            rules_env: 0,
            stats: EnvStats::default(),
        }
    }

    /// `Renderer.addEnvRenderer(env, pass)` (plan 17 re-registration seam).
    pub fn add(&mut self, env: u32, pass: &'static str) {
        self.registry.add(env, pass);
    }

    /// Sets the active `Rules.env` mask.
    pub fn set_rules_env(&mut self, mask: u32) {
        self.rules_env = mask;
    }

    /// Current counters.
    pub fn stats(&self) -> EnvStats {
        self.stats
    }

    /// Redraws the matching env passes.
    pub fn update(&mut self, view: &CameraView) {
        let selected = self.registry.select(self.rules_env);
        self.stats.underwater = selected.iter().any(|r| r.pass == "underwater");
        if !self.stats.underwater {
            self.node.set_visible(false);
            return;
        }
        self.node.set_visible(true);
        let quad = Quad {
            cx: view.x,
            cy: view.y,
            w: view.w,
            h: view.h,
            uv: [0.0, 0.0, 1.0, 1.0],
            color: to_bits(underwater::COLOR),
        };
        if let Some(mesh) = build_mesh(&[quad], 0.0) {
            self.node.set_mesh(&mesh.upcast::<Mesh>());
        }
    }
}
