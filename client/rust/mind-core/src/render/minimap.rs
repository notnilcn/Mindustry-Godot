// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Minimap provider math (`graphics/MinimapRenderer.java`, plan 16 §3.7 / M6).
//!
//! The interactive widget is plan 14; this module owns the provider's
//! deterministic zoom clamp, camera region crop and the per-tile shading
//! multipliers. Tile pixels themselves come from plan 04's
//! `io::map::preview::color_for` over plan 02 `BlockDef` metadata. View-only.

/// `MinimapRenderer.baseSize`.
pub const BASE_SIZE: f32 = 16.0;
/// `MinimapRenderer.updateInterval` (frames).
pub const UPDATE_INTERVAL: f32 = 2.0;

/// `MinimapRenderer.setZoom` clamp: `1 .. min(w,h)/baseSize/2`.
pub fn zoom_clamp(zoom: f32, world_width: i32, world_height: i32) -> f32 {
    let max = world_width.min(world_height) as f32 / BASE_SIZE / 2.0;
    zoom.clamp(1.0, max.max(1.0))
}

/// `MinimapRenderer.getRegion` camera crop in **tiles**: `[x, y, w, h]`.
///
/// `world_y` is flipped so the top of the texture is the map bottom, matching
/// `y = world.height() - dy - sz`.
pub fn region_for_camera(
    camera_x: f32,
    camera_y: f32,
    tilesize: f32,
    world_width: i32,
    world_height: i32,
    zoom: f32,
) -> [f32; 4] {
    let sz = (BASE_SIZE * zoom).clamp(BASE_SIZE, world_width.min(world_height) as f32);
    let dx = (camera_x / tilesize).clamp(sz, (world_width as f32 - sz).max(sz));
    let dy = (camera_y / tilesize).clamp(sz, (world_height as f32 - sz).max(sz));
    [dx - sz, world_height as f32 - dy - sz, sz * 2.0, sz * 2.0]
}

/// `MinimapRenderer.colorFor` darkness multiply: `1 - clamp(darkness / 4)`.
pub fn darkness_multiplier(darkness: f32) -> f32 {
    1.0 - (darkness / 4.0).clamp(0.0, 1.0)
}

/// The `0.7` shade applied when an air tile sits below a solid block.
pub const ABOVE_SOLID_MULTIPLIER: f32 = 0.7;

/// The `(0.84, 0.84, 0.9)` tint on a liquid tile's exposed edge.
pub const LIQUID_EDGE_MULTIPLIER: [f32; 3] = [0.84, 0.84, 0.9];

/// Multiplies an RGBA8888 color's RGB by `m` (alpha preserved).
pub fn shade(rgba: u32, m: f32) -> u32 {
    shade_rgb(rgba, m, m, m)
}

/// Multiplies an RGBA8888 color's RGB by per-channel factors.
pub fn shade_rgb(rgba: u32, r: f32, g: f32, b: f32) -> u32 {
    let ch = rgba.to_be_bytes();
    let r2 = (ch[0] as f32 * r).round().clamp(0.0, 255.0) as u8;
    let g2 = (ch[1] as f32 * g).round().clamp(0.0, 255.0) as u8;
    let b2 = (ch[2] as f32 * b).round().clamp(0.0, 255.0) as u8;
    u32::from_be_bytes([r2, g2, b2, ch[3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_clamps_to_world_half_min_axis() {
        // 64x32 world => max = 32/16/2 = 1.
        assert_eq!(zoom_clamp(0.1, 64, 32), 1.0);
        // 250x250 => max = 250/32 = 7.8125.
        assert!((zoom_clamp(100.0, 250, 250) - 7.8125).abs() < 1e-3);
        assert_eq!(zoom_clamp(4.0, 250, 250), 4.0);
    }

    #[test]
    fn region_is_tiles_and_flips_y() {
        // 256x256, zoom 4 => sz 64; camera at tile 100 => [100-64, 256-100-64].
        let region = region_for_camera(100.0 * 8.0, 100.0 * 8.0, 8.0, 256, 256, 4.0);
        assert_eq!(region, [36.0, 92.0, 128.0, 128.0]);
        // Camera at the origin clamps dx/dy up to sz => x = 0, y = h - 2*sz.
        let low = region_for_camera(0.0, 0.0, 8.0, 256, 256, 4.0);
        assert_eq!(low, [0.0, 128.0, 128.0, 128.0]);
    }

    #[test]
    fn shade_matches_upstream_multipliers() {
        let white = 0xffff_ffffu32;
        assert_eq!(shade(white, 0.7), 0xb3b3b3ff);
        assert_eq!(
            shade_rgb(white, 0.84, 0.84, 0.9),
            0xd6d6e6ff,
            "0.84*255=214.2 -> 214=0xd6, 0.9*255=229.5 -> 230=0xe6"
        );
    }

    #[test]
    fn darkness_multiplier_curve() {
        assert_eq!(darkness_multiplier(0.0), 1.0);
        assert_eq!(darkness_multiplier(2.0), 0.5);
        assert_eq!(darkness_multiplier(8.0), 0.0);
    }
}
