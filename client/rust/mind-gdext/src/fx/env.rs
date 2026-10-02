// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `EnvRenderers` execution (plan 17 M6 / §3.12).
//!
//! Plan 16 owns pass registration/selection; this module appends the plan-17
//! env bodies (underwater tint + caustics + rays + suspended particles, and the
//! scorching `distortAlpha` noise layers) to the frame program.

use mind_core::fx::{scorching_prims, underwater_prims};
use mind_core::render::draw::DrawProgram;
use mind_core::render::env;

/// One active env pass's inputs (`rules.env` + camera/world bounds).
#[derive(Clone, Copy, Debug)]
pub struct EnvFxInput {
    /// `Rules.env` bitmask.
    pub rules_env: u32,
    /// Camera rect `(x, y, w, h)`.
    pub cam: (f32, f32, f32, f32),
    /// World size in pixels `(w, h)`.
    pub world: (f32, f32),
    /// Native `rays.png` size `(w, h)`.
    pub ray_size: (f32, f32),
    /// Whether fog-of-war is active (scorching layer selection).
    pub fog: bool,
}

/// Appends the matching env bodies to `program`; returns the passes appended.
pub fn build_env_prims(program: &mut DrawProgram, view_tick: f32, input: &EnvFxInput) -> usize {
    let registry = env::EnvRegistry::with_defaults();
    let mut passes = 0;
    for spec in registry.select(input.rules_env) {
        match spec.pass {
            "underwater" => {
                underwater_prims(
                    view_tick,
                    input.world.0,
                    input.world.1,
                    input.cam.0,
                    input.cam.1,
                    input.cam.2,
                    input.cam.3,
                    input.ray_size.0,
                    input.ray_size.1,
                    &mut program.prims,
                );
                passes += 1;
            }
            "scorching" => {
                scorching_prims(input.fog, &mut program.prims);
                passes += 1;
            }
            _ => {}
        }
    }
    passes
}
