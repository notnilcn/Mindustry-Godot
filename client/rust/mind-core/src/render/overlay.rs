// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `OverlayRenderer` view math (`graphics/OverlayRenderer.java`, plan 16
//! §3.7/M5). Owns the core-protection `CoreEdge`/`displayed` rules, the
//! off-screen player/enemy indicator geometry and the unit-possession
//! selection-arrow angles. Input/plan data comes from plan 15; markers from
//! plan 12; this module orders and computes primitives only. View-only.

use crate::render::math::voronoi;

/// `OverlayRenderer.indicatorLength`.
pub const INDICATOR_LENGTH: f32 = 14.0;

/// `OverlayRenderer.spawnerMargin` for a tilesize.
pub const fn spawner_margin(tilesize: f32) -> f32 {
    tilesize * 11.0
}

/// One core-protection Voronoi edge (`OverlayRenderer.CoreEdge`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoreEdge {
    /// Start x.
    pub x1: f32,
    /// Start y.
    pub y1: f32,
    /// End x.
    pub x2: f32,
    /// End y.
    pub y2: f32,
    /// Team owning one side.
    pub team1: u8,
    /// Team owning the other side.
    pub team2: u8,
}

/// `CoreEdge.displayed(player)`: which team's color the edge shows, or `None`.
pub fn displayed(player: u8, t1: u8, t2: u8) -> Option<u8> {
    if t1 == t2 {
        None
    } else if t1 == player {
        Some(t2)
    } else if t2 == player || t2 == 0 || (t1 < t2 && t1 != 0) {
        Some(t1)
    } else {
        Some(t2)
    }
}

/// `OverlayRenderer.updateCoreEdges`: Voronoi over the protected cores, one
/// `CoreEdge` per generated graph edge. `cores` is `(x, y, team)`.
pub fn build_core_edges(
    cores: &[(f32, f32, u8)],
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
    protect: impl Fn(u8) -> bool,
) -> Vec<CoreEdge> {
    let filtered: Vec<&(f32, f32, u8)> = cores.iter().filter(|c| protect(c.2)).collect();
    if filtered.is_empty() {
        return Vec::new();
    }
    let positions: Vec<[f32; 2]> = filtered.iter().map(|c| [c.0, c.1]).collect();
    let edges = voronoi::generate(&positions, min_x, max_x, min_y, max_y);
    edges
        .iter()
        .map(|e| CoreEdge {
            x1: e.x1,
            y1: e.y1,
            x2: e.x2,
            y2: e.y2,
            team1: filtered[e.site1].2,
            team2: filtered[e.site2].2,
        })
        .collect()
}

/// `OverlayRenderer.drawTop` selection arrow angle for arrow `i`:
/// `i * 90 + 45 - (Time.time % 360)`.
pub fn selection_arrow_angle(i: usize, time: f32) -> f32 {
    i as f32 * 90.0 + 45.0 - time.rem_euclid(360.0)
}

/// The selection arrow orbit radius (`select.hitSize * 1.5 + unitFade * 2.5`).
pub fn selection_arrow_length(hit_size: f32, unit_fade: f32) -> f32 {
    hit_size * 1.5 + unit_fade * 2.5
}

/// The selection-arrow `Draw.rect` rotation (`rot - 135`).
pub const fn selection_arrow_rotation(rot: f32) -> f32 {
    rot - 135.0
}

/// Player/enemy off-screen indicator: the offset from `source` toward `target`
/// clamped to `length`, and its angle in degrees.
pub fn indicator_offset(target: [f32; 2], source: [f32; 2], length: f32) -> ([f32; 2], f32) {
    let dx = target[0] - source[0];
    let dy = target[1] - source[1];
    let m = (dx * dx + dy * dy).sqrt();
    if m < 1e-9 {
        return ([source[0], source[1]], 0.0);
    }
    let (ox, oy) = (dx / m * length, dy / m * length);
    ([source[0] + ox, source[1] + oy], dy.atan2(dx).to_degrees())
}

/// `true` when `(x, y)` is outside the `0.9 * camera` centered rect (the
/// `if(!rect.contains(...))` indicator gate).
pub fn offscreen(center: [f32; 2], cam_w: f32, cam_h: f32, x: f32, y: f32) -> bool {
    let hw = cam_w * 0.9 / 2.0;
    let hh = cam_h * 0.9 / 2.0;
    (x - center[0]).abs() > hw || (y - center[1]).abs() > hh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displayed_team_rule() {
        // Same team never displays.
        assert_eq!(displayed(1, 2, 2), None);
        // Player's team takes precedence.
        assert_eq!(displayed(1, 1, 3), Some(3));
        assert_eq!(displayed(1, 3, 1), Some(3));
        // Neutral (team 0) loses.
        assert_eq!(displayed(5, 2, 0), Some(2));
        // Otherwise the lower non-zero id wins.
        assert_eq!(displayed(5, 2, 3), Some(2));
        assert_eq!(displayed(5, 3, 2), Some(2));
        assert_eq!(displayed(5, 0, 3), Some(3));
    }

    #[test]
    fn core_edges_use_site_indices() {
        let cores = [(0.0f32, 0.0f32, 1u8), (100.0, 0.0, 2u8)];
        let edges = build_core_edges(&cores, 0.0, 100.0, 0.0, 100.0, |_| true);
        assert!(!edges.is_empty());
        assert!(
            edges
                .iter()
                .all(|e| (e.team1, e.team2) == (1, 2) || (e.team1, e.team2) == (2, 1))
        );
        // Unprotected cores are filtered out.
        assert!(build_core_edges(&cores, 0.0, 100.0, 0.0, 100.0, |t| t == 9).is_empty());
    }

    #[test]
    fn selection_arrow_angles_rotate() {
        assert_eq!(selection_arrow_angle(0, 0.0), 45.0);
        assert_eq!(selection_arrow_angle(1, 0.0), 135.0);
        assert_eq!(selection_arrow_angle(0, 10.0), 35.0);
        assert_eq!(selection_arrow_length(8.0, 1.0), 14.5);
        assert_eq!(selection_arrow_rotation(45.0), -90.0);
    }

    #[test]
    fn indicator_and_offscreen() {
        let (offset, angle) = indicator_offset([100.0, 0.0], [0.0, 0.0], 14.0);
        assert!((offset[0] - 14.0).abs() < 1e-3);
        assert!(angle.abs() < 1e-3);
        // (100, 0) with a 50x50 camera is off-screen; (0,0) is on-screen.
        assert!(offscreen([0.0, 0.0], 50.0, 50.0, 100.0, 0.0));
        assert!(!offscreen([0.0, 0.0], 50.0, 50.0, 1.0, 1.0));
    }
}
