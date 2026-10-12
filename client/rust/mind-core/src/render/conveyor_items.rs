// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Belt item placement (`Conveyor.draw` item loop; parity plan §3.2 PAR-0130).
//!
//! `Conveyor.java:162-177`:
//!
//! ```java
//! Tmp.v1.trns(rotation * 90, tilesize, 0);
//! Tmp.v2.trns(rotation * 90, -tilesize / 2f, xs[i] * tilesize / 2f);
//! float ix = (x + Tmp.v1.x * ys[i] + Tmp.v2.x),
//!       iy = (y + Tmp.v1.y * ys[i] + Tmp.v2.y);
//! Draw.rect(item.fullIcon, ix, iy, itemSize, itemSize);
//! ```
//!
//! `x`/`y` is the building centre (tile centre, `Tile.drawx()`), `ys[i]` the
//! longitudinal position along the belt (`0..1`) and `xs[i]` the lateral
//! offset. `Vars.itemSize = 5f` world px.

use crate::config::TILESIZE;

/// Arc `Vec2.trns(angle, x, y)`: rotate `(x, y)` by `angle` degrees CCW.
pub fn trns(angle_deg: f32, x: f32, y: f32) -> (f32, f32) {
    let (sin, cos) = angle_deg.to_radians().sin_cos();
    (x * cos - y * sin, x * sin + y * cos)
}

/// World-pixel centre for every item on a belt (`Conveyor.draw`).
///
/// `xs`/`ys` are the parallel belt arrays (`ConveyorBuild.xs`/`ys`); only the
/// first `xs.len()` entries are read.
pub fn belt_item_positions(
    x: f32,
    y: f32,
    rotation: u8,
    xs: &[f32],
    ys: &[f32],
) -> Vec<(f32, f32)> {
    let angle = f32::from(rotation) * 90.0;
    let size = TILESIZE as f32;
    let v1 = trns(angle, size, 0.0);
    xs.iter()
        .zip(ys.iter())
        .map(|(xs_i, ys_i)| {
            let v2 = trns(angle, -size / 2.0, xs_i * size / 2.0);
            (x + v1.0 * ys_i + v2.0, y + v1.1 * ys_i + v2.1)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trns_rotates_ccw() {
        let (x, y) = trns(90.0, 1.0, 0.0);
        assert!(x.abs() < 1e-5 && (y - 1.0).abs() < 1e-5, "({x}, {y})");
        let (x, y) = trns(90.0, 0.0, 1.0);
        assert!((x + 1.0).abs() < 1e-5 && y.abs() < 1e-5, "({x}, {y})");
    }

    #[test]
    fn rotation_zero_midpoint_sits_on_the_half_tile() {
        // Java: `v1 = (8, 0)`, `v2 = (-4, 0)`; `ys = 0.5` -> `(x, y)`.
        let points = belt_item_positions(100.0, 200.0, 0, &[0.0], &[0.5]);
        assert_eq!(points, vec![(100.0, 200.0)]);
    }

    #[test]
    fn rotation_zero_tracks_the_belt_axis() {
        // Entry (ys = 0) sits half a tile behind the centre, exit (ys = 1)
        // half a tile ahead: the step per unit of `ys` is `tilesize`.
        let points = belt_item_positions(0.0, 0.0, 0, &[0.0, 0.0], &[0.0, 1.0]);
        assert_eq!(points, vec![(-4.0, 0.0), (4.0, 0.0)]);
    }

    #[test]
    fn rotation_one_rotates_the_axis_ninety_degrees() {
        // Rotation 1 faces +y after the trns rotate (CCW): `v1 = (0, 8)`,
        // `v2 = (0, -4)`, so `ys = 0.5` stays at the centre and `ys = 1` sits
        // half a tile ahead on +y.
        let points = belt_item_positions(10.0, 20.0, 1, &[0.0, 0.0], &[0.5, 1.0]);
        assert!(
            (points[0].0 - 10.0).abs() < 1e-4 && (points[0].1 - 20.0).abs() < 1e-4,
            "centre stays at the anchor for ys = 0.5: {points:?}"
        );
        assert!(points[1].0 - 10.0 < 1e-4 && (points[1].1 - 24.0).abs() < 1e-4);
    }

    #[test]
    fn lateral_offset_moves_sideways() {
        // `xs = 1` shifts the item half a tile to the belt's left: at
        // rotation 0 the lateral component lands on +y (`v2 = (-4, xs * 4)`).
        let points = belt_item_positions(0.0, 0.0, 0, &[1.0], &[0.5]);
        assert_eq!(points, vec![(0.0, 4.0)]);
    }
}
