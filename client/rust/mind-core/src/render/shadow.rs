// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Shadow + darkness data math (`BlockRenderer` half, plan 16 §3.6 / M3).
//!
//! Pure, Godot-free ports of the shadow/darkness predicates and the
//! `updateDarkness`/`drawDarkness` falloff curve. The actual Godot data
//! textures and `Shaders.darkness` composite live in
//! `mind-gdext::render::shadow`.

/// `BlockRenderer.shadowColor` (`Color(0, 0, 0, 0.71)`).
pub const SHADOW_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 0.71];

/// `BlockRenderer.blendShadowColor` = `white.lerp(black, 0.71)`.
pub const BLEND_SHADOW_COLOR: [f32; 4] = [0.29, 0.29, 0.29, 1.0];

/// Opaque white (the shadow-map base / no-shadow color).
pub const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// Opaque black (the `limitMapArea` darkness base).
pub const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// `updateDarkness`/`drawDarkness` curve: `1 - min((d + 0.5) / 4, 1)`.
pub fn darkness_value(darkness: f32) -> f32 {
    1.0 - ((darkness + 0.5) / 4.0).min(1.0)
}

/// The `updateDarkness` tile contribution: tiles with `d <= 0` keep the base.
pub fn darkness_tile_value(darkness: f32) -> Option<f32> {
    (darkness > 0.0).then(|| darkness_value(darkness))
}

/// `processShadows` per-tile color: `blendShadowColor` when the tile casts a
/// shadow, opaque white otherwise (`Color.white` covers `drawShadows` too).
#[allow(clippy::too_many_arguments)]
pub fn shadow_tile_color(
    display_shadow: bool,
    fog: bool,
    has_build: bool,
    build_was_visible: bool,
    ignore_buildings: bool,
    is_static: bool,
    ignore_terrain: bool,
) -> [f32; 4] {
    let casts = display_shadow
        && !(fog && has_build && !build_was_visible)
        && !(ignore_buildings && !is_static)
        && !(ignore_terrain && is_static);
    if casts { BLEND_SHADOW_COLOR } else { WHITE }
}

/// `updateDarkness` `limitMapArea` clip: `Rect.contains(limitX, limitY,
/// limitWidth - 1, limitHeight - 1, x, y)` (Arc: `>= x && < x + w`).
pub fn in_limited_rect(x: i32, y: i32, limit: [i32; 4]) -> bool {
    let [lx, ly, lw, lh] = limit;
    x >= lx && y >= ly && x < lx + lw - 1 && y < ly + lh - 1
}

/// The `Fill.crect(limitX, limitY, limitWidth, limitHeight)` white base rect.
pub fn in_fill_rect(x: i32, y: i32, limit: [i32; 4]) -> bool {
    let [lx, ly, lw, lh] = limit;
    x >= lx && y >= ly && x < lx + lw && y < ly + lh
}

/// `checkChanges`: a filling tile's `data` is rewritten to its wall darkness.
pub fn wall_data_update(fills_tile: bool, wall_darkness: i8) -> Option<i8> {
    fills_tile.then_some(wall_darkness)
}

/// Builds the full darkness map (`updateDarkness`): `limit` is the optional
/// `[limitX, limitY, limitWidth, limitHeight]` rect. Returns row-major RGBA
/// with `1px` per tile; `darkness_at` is `world.get_darkness` (plan 06 + 12).
pub fn build_darkness_map(
    width: i32,
    height: i32,
    limit: Option<[i32; 4]>,
    mut darkness_at: impl FnMut(i32, i32) -> f32,
) -> Vec<[f32; 4]> {
    let width = width.max(0);
    let height = height.max(0);
    let base = if limit.is_some() { BLACK } else { WHITE };
    let mut out = vec![base; (width as usize) * (height as usize)];
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            if let Some(limit) = limit {
                // `Fill.crect(limitX, limitY, limitWidth, limitHeight)`.
                if in_fill_rect(x, y, limit) {
                    out[idx] = WHITE;
                }
                // Darkness is only applied inside the `width - 1` clip rect.
                if !in_limited_rect(x, y, limit) {
                    continue;
                }
            }
            if let Some(value) = darkness_tile_value(darkness_at(x, y)) {
                out[idx] = [value, value, value, 1.0];
            }
        }
    }
    out
}

/// Builds the shadow map: opaque white base, `shadow_tile_color` per event.
pub fn build_shadow_map(width: i32, height: i32, events: &[(i32, i32, [f32; 4])]) -> Vec<[f32; 4]> {
    let width = width.max(0);
    let height = height.max(0);
    let mut out = vec![WHITE; (width as usize) * (height as usize)];
    for (x, y, color) in events {
        if *x < 0 || *y < 0 || *x >= width || *y >= height {
            continue;
        }
        out[(*y as usize) * (width as usize) + (*x as usize)] = *color;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn darkness_curve() {
        // d <= 0 keeps the white base (no entry).
        assert_eq!(darkness_tile_value(0.0), None);
        assert_eq!(darkness_tile_value(-1.0), None);
        // (0.5 + 0.5) / 4 = 0.25 -> 0.75.
        assert!((darkness_value(0.5) - 0.75).abs() < 1e-6);
        // (3.5 + 0.5) / 4 = 1.0 -> 0.0.
        assert!((darkness_value(3.5) - 0.0).abs() < 1e-6);
        // Clamped at large darkness.
        assert!((darkness_value(10.0) - 0.0).abs() < 1e-6);
        // (1.5 + 0.5) / 4 = 0.5 -> 0.5.
        assert!((darkness_value(1.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn limit_map_area_fill_clip() {
        let limit = [2, 2, 3, 3]; // fill x 2..=4, clip x 2..=3
        let map = build_darkness_map(6, 6, Some(limit), |_, _| 0.0);
        // Outside the fill stays black.
        assert_eq!(map[0], BLACK);
        // The full `Fill.crect` area is cleared white.
        let filled = (4usize) * 6 + 4;
        assert_eq!(map[filled], WHITE);
        let inside = (2usize) * 6 + 2;
        assert_eq!(map[inside], WHITE);
        // Arc `Rect.contains(..., width - 1, ...)`: x < lx + lw - 1.
        assert!(in_limited_rect(3, 3, limit));
        assert!(!in_limited_rect(4, 4, limit));
        // Darkness applies only inside the clip rect; (4,4) stays white.
        let map = build_darkness_map(6, 6, Some(limit), |_, _| 0.5);
        assert_eq!(map[inside], [0.75, 0.75, 0.75, 1.0]);
        assert_eq!(map[filled], WHITE);
        // Without a limit the base is white and darkness applies everywhere.
        let map = build_darkness_map(4, 1, None, |x, _| x as f32);
        assert_eq!(map[0], WHITE);
        assert_eq!(map[1], [0.625, 0.625, 0.625, 1.0]);
    }

    #[test]
    fn shadow_tile_predicate() {
        // A static shadow-casting tile with no fog is blend-shadow colored.
        assert_eq!(
            shadow_tile_color(true, false, false, true, false, true, false),
            BLEND_SHADOW_COLOR
        );
        // Fog hides an unseen building.
        assert_eq!(
            shadow_tile_color(true, true, true, false, false, true, false),
            WHITE
        );
        // ignoreBuildings skips non-static blocks.
        assert_eq!(
            shadow_tile_color(true, false, false, true, true, false, false),
            WHITE
        );
        // ignoreTerrain skips static blocks.
        assert_eq!(
            shadow_tile_color(true, false, false, true, false, true, true),
            WHITE
        );
    }

    #[test]
    fn blend_shadow_color_is_lerp_white_black_071() {
        // white.lerp(black, 0.71) = 1 - 0.71 = 0.29.
        assert!((BLEND_SHADOW_COLOR[0] - 0.29).abs() < 1e-6);
        assert_eq!(BLEND_SHADOW_COLOR[3], 1.0);
    }

    #[test]
    fn shadow_map_base_and_event() {
        let events = vec![(1, 0, BLEND_SHADOW_COLOR)];
        let map = build_shadow_map(3, 2, &events);
        assert_eq!(map[0], WHITE, "non-event tile is white");
        assert_eq!(map[1], BLEND_SHADOW_COLOR, "event tile uses blend color");
        // Out-of-bounds events are ignored.
        let map = build_shadow_map(2, 2, &[(9, 9, BLEND_SHADOW_COLOR)]);
        assert!(map.iter().all(|c| *c == WHITE));
    }

    #[test]
    fn check_changes_filling_only() {
        assert_eq!(wall_data_update(true, 3), Some(3));
        assert_eq!(wall_data_update(false, 3), None);
    }
}
