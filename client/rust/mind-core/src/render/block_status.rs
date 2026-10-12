// SPDX-License-Identifier: GPL-3.0-only
//! `BuildingComp.drawStatus` F6 marker corner math (view-only).
//!
//! Ported from `core/src/mindustry/entities/comp/BuildingComp.java:1221-1234`:
//! the status diamonds sit at the bottom-right of the building's sprite,
//! `multiplier = size > 1 ? 1 : 0.64`, grey square radius `2.5 * multiplier`,
//! status square radius `1.5 * multiplier`, both rotated 45°.

/// Marker anchor `(brcx, brcy)` and the size multiplier for a building of
/// `size` tiles centred at `(x, y)` in world pixels.
pub fn status_corner(x: f32, y: f32, size: i32) -> (f32, f32, f32) {
    let multiplier = if size > 1 { 1.0 } else { 0.64 };
    let size4 = size as f32 * 4.0;
    (
        x + size4 - 8.0 * multiplier / 2.0,
        y - size4 + 8.0 * multiplier / 2.0,
        multiplier,
    )
}

#[cfg(test)]
mod tests {
    use super::status_corner;

    /// Single-tile buildings shrink the marker to 0.64
    /// (`BuildingComp.java:1222`).
    #[test]
    fn size_one_uses_the_small_multiplier() {
        let (cx, cy, m) = status_corner(100.0, 200.0, 1);
        assert!((m - 0.64).abs() < 1e-6);
        assert!((cx - (100.0 + 4.0 - 2.56)).abs() < 1e-4, "cx {cx}");
        assert!((cy - (200.0 - 4.0 + 2.56)).abs() < 1e-4, "cy {cy}");
    }

    /// Multi-tile buildings keep multiplier 1.
    #[test]
    fn larger_buildings_use_the_full_multiplier() {
        let (cx, cy, m) = status_corner(0.0, 0.0, 2);
        assert_eq!(m, 1.0);
        assert_eq!((cx, cy), (4.0, -4.0));
    }
}
