// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! `Liquid.getAnimationFrame` + `LiquidBlock.drawTiledFrames` (GAPS #53).
//!
//! `Renderer.fluidFrames[gas ? 1 : 0][frame]` holds the 50 generated liquid /
//! gas frames (`Renderer.java:131-141`, atlas regions `fluid-liquid-<i>` /
//! `fluid-gas-<i>`), selected per liquid by
//! `Liquid.getAnimationFrame` (`Liquid.java:106-108`) and tiled over a liquid
//! block's footprint by `LiquidBlock.drawTiledFrames`
//! (`world/blocks/liquid/LiquidBlock.java:39-77`). `DrawLiquidTile` is the
//! drawer path (`world/draw/DrawLiquidTile.java:26-30`).

use std::sync::LazyLock;

use crate::config::TILESIZE;

/// `Liquid.animationFrames` (`Liquid.java:22`).
pub const ANIMATION_FRAMES: i32 = 50;
/// `Liquid.animationScaleLiquid` ticks per animation cycle (`Liquid.java:23`).
pub const ANIMATION_SCALE_LIQUID: f32 = 230.0;
/// `Liquid.animationScaleGas` ticks per animation cycle (`Liquid.java:23`).
pub const ANIMATION_SCALE_GAS: f32 = 190.0;
/// One fluid frame's atlas size in pixels: the `fluid` stencil is 4x4 tiles.
pub const FRAME_SIZE: f32 = (TILESIZE as f32) * 4.0;

/// `Liquid.getAnimationFrame`: `(int)(time / (gas ? 190 : 230) * 50 + id * 5)
/// % 50` (`Liquid.java:106-108`).
///
/// `time` is `Time.time` in ticks. `id` is the **liquid** content id (the
/// per-liquid phase offset, not the building id).
pub fn animation_frame(time: f32, gas: bool, id: u16) -> i32 {
    let scale = if gas {
        ANIMATION_SCALE_GAS
    } else {
        ANIMATION_SCALE_LIQUID
    };
    let raw = (time / scale * ANIMATION_FRAMES as f32 + f32::from(id) * 5.0) as i32;
    raw.rem_euclid(ANIMATION_FRAMES)
}

/// Atlas region name of one fluid frame (`Renderer.fluidFrames`).
pub fn frame_region(gas: bool, frame: i32) -> &'static str {
    static NAMES: LazyLock<Vec<String>> = LazyLock::new(|| {
        let mut names = Vec::with_capacity((ANIMATION_FRAMES * 2) as usize);
        for gas in [false, true] {
            for frame in 0..ANIMATION_FRAMES {
                names.push(format!(
                    "fluid-{}-{frame}",
                    if gas { "gas" } else { "liquid" }
                ));
            }
        }
        names
    });
    let base = if gas { ANIMATION_FRAMES } else { 0 };
    let index = base + frame.rem_euclid(ANIMATION_FRAMES);
    &NAMES[index as usize]
}

/// One tiled fluid quad from `LiquidBlock.drawTiledFrames`: world-pixel center
/// plus draw size (Java's truncated `TextureRegion` draw).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TiledFrame {
    /// Quad center x (world pixels).
    pub x: f32,
    /// Quad center y (world pixels).
    pub y: f32,
    /// Quad width (the frame region's truncated width).
    pub w: f32,
    /// Quad height (the frame region's truncated height).
    pub h: f32,
}

/// `LiquidBlock.drawTiledFrames(size, x, y, padding, liquid, alpha)` geometry
/// (`LiquidBlock.java:39-77`): one frame quad per block cell, truncated against
/// the padding bounds.
///
/// Java's `toDraw.setWidth(w - squishX * 4f)` / `setY(y + squishY * 4f)` crops
/// the 32 px region; the port keeps the truncated draw size and centre (the
/// source-crop half is a sub-texel difference at 4x scale).
pub fn tiled_frames(size: i32, x: f32, y: f32, pad: f32) -> Vec<TiledFrame> {
    let size = size.max(1);
    let ts = TILESIZE as f32;
    let pad = pad.max(0.0);
    let left_bounds = size as f32 / 2.0 * ts - pad;
    let bottom_bounds = size as f32 / 2.0 * ts - pad;
    let mut out = Vec::with_capacity((size * size) as usize);
    for sx in 0..size {
        for sy in 0..size {
            let relx = sx as f32 - (size - 1) as f32 / 2.0;
            let rely = sy as f32 - (size - 1) as f32 / 2.0;
            let right_border = relx * ts + pad;
            let top_border = rely * ts + pad;
            let squish_x = right_border + ts / 2.0 - left_bounds;
            let squish_y = top_border + ts / 2.0 - bottom_bounds;
            if squish_x >= ts || squish_y >= ts {
                continue;
            }
            let mut width = FRAME_SIZE;
            let mut height = FRAME_SIZE;
            let mut ox = 0.0;
            let mut oy = 0.0;
            if squish_x > 0.0 {
                width -= squish_x * 4.0;
                ox = -squish_x / 2.0;
            }
            if squish_y > 0.0 {
                height -= squish_y * 4.0;
                oy = -squish_y / 2.0;
            }
            if width <= 0.0 || height <= 0.0 {
                continue;
            }
            out.push(TiledFrame {
                x: x + right_border + ox,
                y: y + top_border + oy,
                w: width,
                h: height,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animation_frame_matches_java_phase() {
        // `Time.time = 0`: the liquid id offset, mod 50.
        assert_eq!(animation_frame(0.0, false, 0), 0);
        assert_eq!(animation_frame(0.0, false, 42), 10);
        assert_eq!(animation_frame(0.0, false, 10), 0);
        // One full liquid cycle is 230 ticks (`50` frames), gas 190.
        assert_eq!(animation_frame(230.0, false, 0), 0);
        assert_eq!(animation_frame(115.0, false, 0), 25);
        assert_eq!(animation_frame(190.0, true, 0), 0);
        assert_eq!(animation_frame(95.0, true, 0), 25);
        // Truncation, not rounding: 229.9 ticks is still frame 49.
        assert_eq!(animation_frame(229.9, false, 0), 49);
    }

    #[test]
    fn frame_region_is_the_generated_atlas_name() {
        assert_eq!(frame_region(false, 0), "fluid-liquid-0");
        assert_eq!(frame_region(false, 49), "fluid-liquid-49");
        assert_eq!(frame_region(true, 7), "fluid-gas-7");
        // Out-of-range frames wrap like Java's `% animationFrames`.
        assert_eq!(frame_region(false, 50), "fluid-liquid-0");
        assert_eq!(frame_region(true, -1), "fluid-gas-49");
    }

    #[test]
    fn tiled_frames_single_tile_is_one_centered_frame() {
        let frames = tiled_frames(1, 100.0, 200.0, 0.0);
        assert_eq!(frames.len(), 1);
        assert_eq!(
            frames[0],
            TiledFrame {
                x: 100.0,
                y: 200.0,
                w: 32.0,
                h: 32.0
            }
        );
    }

    #[test]
    fn tiled_frames_two_tiles_overlap_by_half_a_frame() {
        let frames = tiled_frames(2, 100.0, 200.0, 0.0);
        assert_eq!(frames.len(), 4);
        for frame in &frames {
            assert_eq!((frame.w, frame.h), (32.0, 32.0));
        }
        // Cell borders at +-4 px (`relx * tilesize`); the iteration is
        // sx-major, matching Java's `for(sx) for(sy)`.
        assert_eq!((frames[0].x, frames[0].y), (96.0, 196.0));
        assert_eq!((frames[1].x, frames[1].y), (96.0, 204.0));
        assert_eq!((frames[2].x, frames[2].y), (104.0, 196.0));
        assert_eq!((frames[3].x, frames[3].y), (104.0, 204.0));
    }

    #[test]
    fn tiled_frames_truncate_against_the_padding_bounds() {
        // `LiquidBlock.java:57-73` with size 2, padding 2: the two far cells
        // squish by 4 px (`squish = 4`), cropping the frame to 16 px and
        // pulling the centre 2 px inward.
        let frames = tiled_frames(2, 100.0, 200.0, 2.0);
        assert_eq!(frames.len(), 4);
        // sx-major order: (0,0), (0,1), (1,0), (1,1).
        assert_eq!((frames[0].w, frames[0].h), (32.0, 32.0));
        assert_eq!((frames[0].x, frames[0].y), (98.0, 198.0));
        assert_eq!((frames[1].w, frames[1].h), (32.0, 16.0));
        assert_eq!(frames[1].y, 204.0);
        assert_eq!((frames[2].w, frames[2].h), (16.0, 32.0));
        assert_eq!(frames[2].x, 104.0);
        assert_eq!((frames[3].w, frames[3].h), (16.0, 16.0));
    }

    #[test]
    fn tiled_frames_skip_cells_beyond_a_tile_squish() {
        // A padding that overhangs a whole tile skips that cell
        // (`if(squishX >= 8 || squishY >= 8) continue;`).
        let frames = tiled_frames(3, 0.0, 0.0, 20.0);
        assert!(frames.len() < 9);
        for frame in &frames {
            assert!(frame.w > 0.0 && frame.h > 0.0);
        }
    }
}
