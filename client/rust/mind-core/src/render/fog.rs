// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Fog-of-war view data (`graphics/FogRenderer.java`, plan 16 §3.7 / M6).
//!
//! The upstream `FogEvent` is an annotation-generated `@Struct` packing
//! `x:16 | y:16 | radius:16 | team:8` into a `long` (`FogControl.FogEventStruct`).
//! This module owns the deterministic pack/unpack and the 20-gon polygon the
//! renderer fills into the static/dynamic fog targets. All of it is view-only
//! and never feeds the sim (D8).

/// Number of sides of the fog polygon (`Fill.poly(..., 20, rad)`).
pub const FOG_POLYGON_SIDES: usize = 20;

/// `FogControl.FogEvent.get(x, y, radius, team)`.
///
/// Packs the four fields in declaration order into one `i64`
/// (`x` low 16 bits … `team` high 8 bits). Negative tile coordinates are stored
/// as sign-extended 16-bit two's complement, matching Arc's `@StructField(16)`.
pub fn fog_event(x: i32, y: i32, radius: i32, team: u8) -> i64 {
    let x = (x as i16) as u16 as i64;
    let y = (y as i16) as u16 as i64;
    let radius = (radius as i16) as u16 as i64;
    x | (y << 16) | (radius << 32) | ((team as i64) << 48)
}

/// `FogEvent.x(event)`.
pub fn event_x(event: i64) -> i32 {
    (event as u16 as i16) as i32
}

/// `FogEvent.y(event)`.
pub fn event_y(event: i64) -> i32 {
    ((event >> 16) as u16 as i16) as i32
}

/// `FogEvent.radius(event)`.
pub fn event_radius(event: i64) -> i32 {
    ((event >> 32) as u16 as i16) as i32
}

/// `FogEvent.team(event)`.
pub fn event_team(event: i64) -> u8 {
    ((event >> 48) & 0xff) as u8
}

/// The visual half-tile offset of a fog event (`FogRenderer.renderEvent`):
/// `0.5` more when the tile holds an even-sized multiblock center.
pub fn event_visual_offset(block_size: i32, is_center: bool) -> f32 {
    if is_center && block_size % 2 == 0 {
        0.5
    } else {
        0.0
    }
}

/// `Fill.poly` vertex ring: `sides` points at `radius`, starting at `start`
/// degrees, counter-clockwise. Mirrors Arc `Fill.poly(x, y, sides, radius, rot)`.
pub fn poly_vertices(
    cx: f32,
    cy: f32,
    sides: usize,
    radius: f32,
    start_degrees: f32,
) -> Vec<[f32; 2]> {
    let step = 360.0 / sides as f32;
    (0..sides)
        .map(|i| {
            let angle = (start_degrees + i as f32 * step).to_radians();
            [cx + radius * angle.cos(), cy + radius * angle.sin()]
        })
        .collect()
}

/// A single fog polygon in tile coordinates (`FogRenderer.poly`).
pub fn fog_poly(cx: f32, cy: f32, radius: f32) -> Vec<[f32; 2]> {
    poly_vertices(cx, cy, FOG_POLYGON_SIDES, radius, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fog_event_round_trips() {
        let e = fog_event(12, 34, 7, 2);
        assert_eq!(event_x(e), 12);
        assert_eq!(event_y(e), 34);
        assert_eq!(event_radius(e), 7);
        assert_eq!(event_team(e), 2);
    }

    #[test]
    fn fog_event_positive_fields_do_not_overlap() {
        let e = fog_event(0x7fff, 0x7fff, 0x7fff, 0xff);
        assert_eq!(event_x(e), 0x7fff);
        assert_eq!(event_y(e), 0x7fff);
        assert_eq!(event_radius(e), 0x7fff);
        assert_eq!(event_team(e), 0xff);
    }

    #[test]
    fn visual_offset_only_for_even_multiblock_centers() {
        assert_eq!(event_visual_offset(4, true), 0.5);
        assert_eq!(event_visual_offset(4, false), 0.0);
        assert_eq!(event_visual_offset(3, true), 0.0);
    }

    #[test]
    fn fog_poly_has_twenty_vertices_at_radius() {
        let pts = fog_poly(0.0, 0.0, 5.0);
        assert_eq!(pts.len(), FOG_POLYGON_SIDES);
        for p in &pts {
            let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
            assert!((r - 5.0).abs() < 1e-3, "radius {r}");
        }
    }
}
