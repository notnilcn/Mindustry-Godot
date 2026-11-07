// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `TiledFloor`/`TiledWall` packed tiling and `WobbleProp` wobble math.
//!
//! Upstream `TiledFloor`/`TiledWall` (`world/blocks/environment/TiledFloor.java`,
//! `TiledWall.java`) greedily pack adjacent same-block tiles into the largest
//! square the atlas has a `-<size>` (or `-<size>-<variant>`) sprite for, capped
//! by `maxSize`, and cache each tile's `(x, y, size, changes)` state in the
//! tile's temporary state slot. This module ports the state layout, the scan,
//! the variant/cell selection and the `WobbleProp` vertex warp; the Godot mesh
//! builds live in `mind-gdext::render` (`floor.rs`, `blocks.rs`).

use std::collections::HashMap;

use crate::math::rand_arc::{pack_point2, random_seed_int};

/// `TiledState.x` bit width (`@StructField(4)`, `TiledFloor.java:128`).
pub const TILED_X_BITS: u32 = 4;
/// `TiledState.y` bit width (`@StructField(4)`, `TiledFloor.java:129`).
pub const TILED_Y_BITS: u32 = 4;
/// `TiledState.size` bit width (`@StructField(8)`, `TiledFloor.java:130`).
pub const TILED_SIZE_BITS: u32 = 8;
/// `TiledState.changes` bit width (the frame epoch the state was scanned in).
pub const TILED_CHANGES_BITS: u32 = 32;

const TILED_Y_SHIFT: u32 = TILED_X_BITS;
const TILED_SIZE_SHIFT: u32 = TILED_Y_SHIFT + TILED_Y_BITS;
const TILED_CHANGES_SHIFT: u32 = TILED_SIZE_SHIFT + TILED_SIZE_BITS;

/// Packs one `TiledState` (`TiledFloor.TiledState.get`, `:99`).
///
/// `x`/`y` are the tile's offset inside the packed square (0-based, top-left
/// origin), `size` the square edge in tiles and `changes` the
/// `world.floorChanges`/`world.tileChanges` epoch the scan ran in.
pub fn tiled_state(x: i32, y: i32, size: i32, changes: i32) -> u64 {
    let mask = |bits: u32| (1u64 << bits) - 1;
    ((x as u64) & mask(TILED_X_BITS))
        | (((y as u64) & mask(TILED_Y_BITS)) << TILED_Y_SHIFT)
        | (((size as u64) & mask(TILED_SIZE_BITS)) << TILED_SIZE_SHIFT)
        | (((changes as u32 as u64) & mask(TILED_CHANGES_BITS)) << TILED_CHANGES_SHIFT)
}

/// `TiledState.x(state)`.
pub fn tiled_state_x(state: u64) -> i32 {
    (state & ((1u64 << TILED_X_BITS) - 1)) as i32
}

/// `TiledState.y(state)`.
pub fn tiled_state_y(state: u64) -> i32 {
    ((state >> TILED_Y_SHIFT) & ((1u64 << TILED_Y_BITS) - 1)) as i32
}

/// `TiledState.size(state)`.
pub fn tiled_state_size(state: u64) -> i32 {
    ((state >> TILED_SIZE_SHIFT) & ((1u64 << TILED_SIZE_BITS) - 1)) as i32
}

/// `TiledState.changes(state)`.
pub fn tiled_state_changes(state: u64) -> i32 {
    ((state >> TILED_CHANGES_SHIFT) & ((1u64 << TILED_CHANGES_BITS) - 1)) as u32 as i32
}

/// The greedy maximum-size scan (`TiledFloor.scan`, `:64-80`).
///
/// `same(dx, dy)` returns whether the tile at the scan origin + `(dx, dy)` can
/// join the packed square: in bounds, the same block/floor, and *not already
/// scanned in this epoch* (`TiledState.changes(state(other)) == changes`
/// shrinks the square). The origin itself is always joinable in Java (its own
/// state is stale), so a non-empty map never collapses below 1.
pub fn scan_size(max_size: i32, same: impl Fn(i32, i32) -> bool) -> i32 {
    let mut size = max_size.max(1);
    let mut cx = 0;
    while cx < size {
        let mut cy = 0;
        while cy < size {
            if !same(cx, cy) {
                // `int max = Math.max(cx, cy); size = Math.min(max, size);`
                size = size.min(cx.max(cy));
            }
            cy += 1;
        }
        cx += 1;
    }
    // `TiledFloor.drawMain:112` clamps the drawn size to `1..=maxSize`; the
    // origin always joins itself in Java (its own state is stale), so this
    // only matters for a scan that starts on an invalid tile.
    size.max(1)
}

/// Writes the scanned state into every tile of the `size`-square
/// (`TiledFloor.scan`, `:83-96`).
pub fn assign_square(
    states: &mut HashMap<(i32, i32), u64>,
    x: i32,
    y: i32,
    size: i32,
    changes: i32,
) {
    for cx in 0..size.max(0) {
        for cy in 0..size.max(0) {
            states.insert((x + cx, y + cy), tiled_state(cx, cy, size, changes));
        }
    }
}

/// The Java lazy scan for one tile: returns the packed state, scanning from the
/// tile when it has no current-epoch state yet.
///
/// `same` receives the partial state map (so it can reject tiles already
/// scanned in this epoch) and absolute tile coordinates.
pub fn ensure_state(
    states: &mut HashMap<(i32, i32), u64>,
    x: i32,
    y: i32,
    max_size: i32,
    changes: i32,
    same: impl Fn(&HashMap<(i32, i32), u64>, i32, i32) -> bool,
) -> u64 {
    if let Some(state) = states.get(&(x, y)) {
        return *state;
    }
    let size = {
        let snapshot: &HashMap<(i32, i32), u64> = states;
        scan_size(max_size, |dx, dy| same(snapshot, x + dx, y + dy))
    };
    assign_square(states, x, y, size, changes);
    states.get(&(x, y)).copied().unwrap_or(0)
}

/// `Floor.variant(x, y, max)` (`Floor.java:300`): `randomSeed(Point2.pack(x,
/// y), 0, max(0, max - 1))`, evaluated on the packed square's **origin**
/// (`tile.x - cx`, `tile.y - cy` in `TiledFloor.drawMain:113`).
pub fn variant(x: i32, y: i32, variants: i32) -> i32 {
    let count = variants.max(1);
    random_seed_int(pack_point2(x, y), 0, count - 1)
}

/// The split cell a tile draws (`TiledFloor.drawMain:117`):
/// `regions[clamp(cx)][clamp(size - 1 - cy)]` → `(column, row)`.
pub fn cell(size: i32, offset_x: i32, offset_y: i32) -> (i32, i32) {
    let last = (size - 1).max(0);
    (
        offset_x.clamp(0, last),
        (size - 1 - offset_y).clamp(0, last),
    )
}

/// The primary atlas region for a packed square
/// (`TiledFloor.load:39-48`): `<name>-<size>-<variant>`.
pub fn region_name(name: &str, size: i32, variant: i32) -> String {
    format!("{name}-{size}-{variant}")
}

/// The fallback atlas region (`TiledFloor.load:44`): `<name>-<size>`.
pub fn fallback_region_name(name: &str, size: i32) -> String {
    format!("{name}-{size}")
}

/// `WobbleProp`'s four vertex-warp terms (`WobbleProp.java:9,17-20`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wobble {
    /// `WobbleProp.wscl` (`:9`, default 25).
    pub wscl: f32,
    /// `WobbleProp.wmag` (default 0.4).
    pub wmag: f32,
    /// `WobbleProp.wtscl` (default 1).
    pub wtscl: f32,
    /// `WobbleProp.wmag2` (default 1).
    pub wmag2: f32,
}

impl Default for Wobble {
    fn default() -> Self {
        Self {
            wscl: 25.0,
            wmag: 0.4,
            wtscl: 1.0,
            wmag2: 1.0,
        }
    }
}

impl Wobble {
    /// The per-vertex displacement in `WobbleProp.drawBase` (`:17-20`):
    ///
    /// ```text
    /// x += sin(y*3 + t, wscl, wmag) + sin(x*3 - t, 70*wtscl, 0.8*wmag2)
    /// y += cos(x*3 + t + 8, wscl + 6, wmag*1.1) + sin(y*3 - t, 50*wtscl, 0.2*wmag2)
    /// ```
    ///
    /// `vx`/`vy` are the **world-space** vertex coordinates (`Draw.rectv`
    /// hands the warp callback world positions), `time` is `Time.time` in
    /// ticks.
    pub fn offset(&self, vx: f32, vy: f32, time: f32) -> (f32, f32) {
        let dx = sin_scl(vy * 3.0 + time, self.wscl, self.wmag)
            + sin_scl(vx * 3.0 - time, 70.0 * self.wtscl, 0.8 * self.wmag2);
        let dy = cos_scl(vx * 3.0 + time + 8.0, self.wscl + 6.0, self.wmag * 1.1)
            + sin_scl(vy * 3.0 - time, 50.0 * self.wtscl, 0.2 * self.wmag2);
        (dx, dy)
    }

    /// The four warped corners of the `w`×`h` quad centered at `(x, y)`
    /// (`Draw.rectv` vertex order: bottom-left, bottom-right, top-right,
    /// top-left).
    pub fn quad(&self, x: f32, y: f32, w: f32, h: f32, time: f32) -> [[f32; 2]; 4] {
        let (hw, hh) = (w / 2.0, h / 2.0);
        [
            (x - hw, y - hh),
            (x + hw, y - hh),
            (x + hw, y + hh),
            (x - hw, y + hh),
        ]
        .map(|(vx, vy)| {
            let (dx, dy) = self.offset(vx, vy, time);
            [vx + dx, vy + dy]
        })
    }
}

/// `Mathf.sin(in, scl, mag)` (`Mathf.java:100`): `sin(in / scl) * mag`.
#[inline]
fn sin_scl(in_: f32, scl: f32, mag: f32) -> f32 {
    (in_ / scl).sin() * mag
}

/// `Mathf.cos(in, scl, mag)`: `cos(in / scl) * mag`.
#[inline]
fn cos_scl(in_: f32, scl: f32, mag: f32) -> f32 {
    (in_ / scl).cos() * mag
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The struct layout round-trips every field, including negative direction
    /// values and the wrap-around change counter.
    #[test]
    fn tiled_state_round_trips() {
        for (x, y, size, changes) in [
            (0, 0, 1, 0),
            (1, 2, 3, 7),
            (15, 15, 255, -1),
            (3, 0, 2, i32::MIN),
        ] {
            let state = tiled_state(x, y, size, changes);
            assert_eq!(tiled_state_x(state), x & 0xF);
            assert_eq!(tiled_state_y(state), y & 0xF);
            assert_eq!(tiled_state_size(state), size & 0xFF);
            assert_eq!(tiled_state_changes(state), changes);
        }
    }

    /// `TiledFloor.scan` shrinks the candidate square to the largest one whose
    /// interior is all packable, and stops scanning once the square is 1.
    #[test]
    fn scan_size_shrinks_to_the_packable_square() {
        // A fully same 4x4 with maxSize 3.
        assert_eq!(scan_size(3, |_, _| true), 3);
        // A block at (2, 2) caps the square at 2.
        assert_eq!(scan_size(3, |cx, cy| !(cx == 2 && cy == 2)), 2);
        // A block at (1, 0) caps it at 1.
        assert_eq!(scan_size(3, |cx, cy| !(cx == 1 && cy == 0)), 1);
        // maxSize 1 never scans past the first tile.
        assert_eq!(scan_size(1, |_, _| false), 1);
    }

    /// The Java lazy scan assigns the whole square from the first tile that
    /// scans, so later tiles reuse the origin's offsets.
    #[test]
    fn ensure_state_assigns_the_whole_square() {
        let mut states = HashMap::new();
        let changes = 9;
        let same = |_states: &HashMap<(i32, i32), u64>, _x: i32, _y: i32| true;
        let origin = ensure_state(&mut states, 0, 0, 3, changes, same);
        assert_eq!(tiled_state_size(origin), 3);
        assert_eq!(
            (tiled_state_x(origin), tiled_state_y(origin)),
            (0, 0),
            "the scanning tile is the square origin"
        );
        assert_eq!(states.len(), 9);
        // A member of the square keeps the origin's `size` and changes epoch.
        let member = ensure_state(&mut states, 2, 1, 3, changes, same);
        assert_eq!(tiled_state_size(member), 3);
        assert_eq!((tiled_state_x(member), tiled_state_y(member)), (2, 1));
        assert_eq!(tiled_state_changes(member), changes);

        // A different-epoch neighbour blocks the square like Java's
        // `changes(other) == changes` check.
        let mut states = HashMap::new();
        let same = |states: &HashMap<(i32, i32), u64>, x: i32, y: i32| {
            states
                .get(&(x, y))
                .is_none_or(|state| tiled_state_changes(*state) != changes)
        };
        let origin = ensure_state(&mut states, 0, 0, 2, changes, same);
        assert_eq!(tiled_state_size(origin), 2);
        assert_eq!(states.len(), 4);
    }

    /// `TiledFloor.drawMain` picks the variant from the square origin and the
    /// split cell from the tile's offset (`regions[cx][size - 1 - cy]`).
    #[test]
    fn variant_and_cell_follow_the_java_indices() {
        // `Floor.variant` is deterministic and origin-seeded.
        assert_eq!(variant(4, 7, 4), variant(4, 7, 4));
        assert!((0..4).contains(&variant(4, 7, 4)));
        assert_eq!(variant(4, 7, 1), 0, "one variant has no random pick");

        // The origin is `(cx, cy)` in the packed square; the cell row is
        // flipped (`size - 1 - cy`).
        assert_eq!(cell(3, 0, 0), (0, 2));
        assert_eq!(cell(3, 2, 0), (2, 2));
        assert_eq!(cell(3, 0, 2), (0, 0));
        assert_eq!(cell(3, 2, 2), (2, 0));
        assert_eq!(cell(1, 5, 5), (0, 0), "offsets clamp");
        assert_eq!(region_name("tiled-stone", 2, 1), "tiled-stone-2-1");
        assert_eq!(fallback_region_name("tiled-stone", 2), "tiled-stone-2");
    }

    /// `WobbleProp.drawBase` displaces each vertex by the four sin terms; the
    /// default fields and a zero time reproduce the Java arithmetic.
    #[test]
    fn wobble_offsets_follow_the_java_terms() {
        let w = Wobble::default();
        assert_eq!(w.wscl, 25.0);
        assert_eq!(w.wmag, 0.4);
        let (dx, dy) = w.offset(0.0, 0.0, 0.0);
        // dx = sin(0, 25, 0.4) + sin(0, 70, 0.8) = 0
        assert!(dx.abs() < 1e-5);
        // dy = cos(8, 31, 0.44) + sin(0, 50, 0.2)
        let expected = (8f32 / 31.0).cos() * 0.44;
        assert!((dy - expected).abs() < 1e-5, "dy {dy} vs {expected}");

        // A non-zero vertex/time produces bounded offsets inside the documented
        // amplitudes: |dx| <= 0.4 + 0.8, |dy| <= 0.44 + 0.2.
        let (dx, dy) = w.offset(13.0, 21.0, 17.0);
        assert!(dx.abs() <= 1.2 + 1e-4, "dx {dx}");
        assert!(dy.abs() <= 0.64 + 1e-4, "dy {dy}");

        // The wobble is frame-dependent (unlike a static quad).
        assert_ne!(w.offset(13.0, 21.0, 17.0), w.offset(13.0, 21.0, 18.0));
    }

    /// The warped quad keeps the drawn center and vertex order; every corner
    /// moves by its own offset.
    #[test]
    fn wobble_quad_warps_all_four_corners() {
        let w = Wobble::default();
        let corners = w.quad(100.0, 200.0, 32.0, 32.0, 0.0);
        let base = [(84.0, 184.0), (116.0, 184.0), (116.0, 216.0), (84.0, 216.0)];
        for (index, corner) in corners.iter().enumerate() {
            let (bx, by) = base[index];
            let (dx, dy) = w.offset(bx, by, 0.0);
            assert!((corner[0] - (bx + dx)).abs() < 1e-4, "corner {index}");
            assert!((corner[1] - (by + dy)).abs() < 1e-4, "corner {index}");
        }
        // A zeroed wobble is the identity.
        let flat = Wobble {
            wscl: f32::INFINITY,
            wmag: 0.0,
            wtscl: 1.0,
            wmag2: 0.0,
        };
        let corners = flat.quad(0.0, 0.0, 10.0, 10.0, 3.0);
        assert_eq!(
            corners,
            [[-5.0, -5.0], [5.0, -5.0], [5.0, 5.0], [-5.0, 5.0]]
        );
    }
}
