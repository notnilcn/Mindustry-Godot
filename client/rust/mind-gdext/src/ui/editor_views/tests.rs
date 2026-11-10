// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Unit tests for the objective-canvas packing and placement helpers.
//! Upstream: core/src/mindustry/editor/MapObjectivesCanvas.java

use super::objectives_canvas::{
    BOUNDS, OBJ_HEIGHT, OBJ_WIDTH, has_pos, node_x_at, node_y_at, pack_pos, valid_place_rect,
};

/// `NodePosition.pack`/`unpack` (`MapObjectivesCanvas.NodePosition`): the
/// signed 16-bit halves round-trip and the sentinel marks "unplaced".
#[test]
fn objective_editor_pos_packs_signed_halves() {
    for (x, y) in [(0, 0), (5, 2), (-1, -2), (-100, 99), (32767, -32768)] {
        let packed = pack_pos(i64::from(x), i64::from(y));
        assert_eq!(node_x_at(packed), x, "x for ({x},{y})");
        assert_eq!(node_y_at(packed), y, "y for ({x},{y})");
        assert!(has_pos(packed));
    }
    assert!(!has_pos(pack_pos(-999, -999)), "the default sentinel");
}

/// `ObjectiveTilemap.validPlace` (`MapObjectivesCanvas.java:305-326`): the
/// 5x2 rects may touch but not overlap, and the `[-100, 100]` bounds hold.
#[test]
fn objective_placement_rejects_overlap_but_allows_edge_contact() {
    // Free space.
    assert!(valid_place_rect(0, 0, &[]));
    // Same cell overlaps.
    assert!(!valid_place_rect(0, 0, &[(0, 0)]));
    // Overlap by one tile in either axis.
    assert!(!valid_place_rect(1, 0, &[(0, 0)]));
    assert!(!valid_place_rect(0, 1, &[(0, 0)]));
    // Edge contact is legal (strict inequalities in Java).
    assert!(valid_place_rect(OBJ_WIDTH, 0, &[(0, 0)]));
    assert!(valid_place_rect(0, OBJ_HEIGHT, &[(0, 0)]));
    assert!(valid_place_rect(-OBJ_WIDTH, 0, &[(0, 0)]));
    // Bounds are inclusive and checked before the overlap scan.
    assert!(valid_place_rect(BOUNDS, BOUNDS, &[]));
    assert!(!valid_place_rect(BOUNDS + 1, 0, &[]));
    assert!(!valid_place_rect(0, -BOUNDS - 1, &[]));
}
