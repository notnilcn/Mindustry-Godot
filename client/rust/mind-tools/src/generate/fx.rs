// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// `tools/src/mindustry/tools/Generators.java` (`fluid`/`gasFrame`/`liquidFrame`
// helpers + the `gas-frames` pass), `core/src/mindustry/type/Liquid.java`.

//! The `gas-frames` pass (plan 03 M2): liquid (3 keyframes) and gas (4
//! keyframes) animation frames over the `fluid` stencil.

use anyhow::{Context, Result};
use mind_atlas::noise;
use mind_atlas::pixmaps::rgba8888f;

use crate::generate::{ANIMATION_FRAMES, GenCtx};

/// `Generators.fluid` — interpolated keyframe value in `[min, 1]`.
fn fluid(gas: bool, x: f64, y: f64, frame: f32) -> f32 {
    let keyframes = if gas { 4 } else { 3 };
    let cur_frame = (frame * keyframes as f32) as i32;
    let next_frame = (cur_frame + 1) % keyframes;
    let progress = (frame * keyframes as f32) % 1.0;

    if gas {
        let min = 0.56f32;
        let interpolated = lerp(
            gas_frame(x, y, cur_frame) as f32,
            gas_frame(x, y, next_frame) as f32,
            progress,
        );
        min + (1.0 - min) * interpolated
    } else {
        let min = 0.84f32;
        let rx = (x + frame as f64 * 32.0) % 32.0;
        let ry = (y + frame as f64 * 32.0) % 32.0;
        let interpolated = liquid_frame(rx, ry, 2) as f32;
        // Only two colors here.
        if interpolated >= 0.3 { 1.0 } else { min }
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// `Generators.gasFrame`.
fn gas_frame(x: f64, y: f64, frame: i32) -> f64 {
    let s = 31.0;
    // Both offsets come from the same seeded draw (upstream ox == oy).
    let offset = noise::random_seed_scaled(frame as i64, 200_000.0) as f64;
    let (ox, oy) = (offset, offset);
    let scale = 21.0;
    let second = 0.3;
    (noise::simplex_raw_tiled(x, y, ox, oy, s, s, scale)
        + noise::simplex_raw_tiled(x, y, ox, oy, s, s, scale / 1.5) * second)
        / (1.0 + second)
}

/// `Generators.liquidFrame`.
fn liquid_frame(x: f64, y: f64, frame: i32) -> f64 {
    let s = 31.0;
    let offset = noise::random_seed_scaled(frame as i64, 1.0) as f64;
    let (ox, oy) = (offset, offset);
    let scale = 26.0;
    let second = 0.5;
    (noise::simplex_raw_tiled(x, y, ox, oy, s, s, scale)
        + noise::simplex_raw_tiled(x, y, ox, oy, s, s, scale / 1.5) * second)
        / (1.0 + second)
}

/// The `gas-frames` pass: `fluid-liquid-<i>` and `fluid-gas-<i>` frames.
pub fn gas_frames(ctx: &mut GenCtx) -> Result<()> {
    let frames = ANIMATION_FRAMES;
    for (type_name, gas) in [("liquid", false), ("gas", true)] {
        let base = ctx
            .atlas
            .get("fluid")
            .context("gas-frames: `fluid` stencil missing")?;
        for i in 0..frames {
            let frame = i as f32 / frames as f32;
            let mut copy = base.copy();
            for y in 0..copy.height {
                for x in 0..copy.width {
                    if copy.get_a(x, y) > 128 {
                        let value = fluid(gas, x as f64, y as f64, frame);
                        copy.set_raw(x, y, rgba8888f(1.0, 1.0, 1.0, value));
                    }
                }
            }
            ctx.atlas
                .save(&copy, &format!("effects/fluid-{type_name}-{i}"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fluid_frames_are_deterministic() {
        for i in 0..4 {
            let frame = i as f32 / 50.0;
            assert_eq!(
                fluid(true, 5.0, 9.0, frame).to_bits(),
                fluid(true, 5.0, 9.0, frame).to_bits()
            );
        }
        // Liquid is binary (two colors).
        let v = fluid(false, 3.0, 4.0, 0.0);
        assert!(v == 1.0 || (v - 0.84).abs() < 1e-6, "{v}");
        // Gas stays within [min, 1].
        let g = fluid(true, 3.0, 4.0, 0.0);
        assert!((0.55..=1.0).contains(&g), "{g}");
    }
}
