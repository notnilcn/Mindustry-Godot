// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Autotiling blend logic (`world/blocks/Autotiler.java`) — plan 08 §3.1.
//!
//! Ported from the `Autotiler` interface's default methods: `buildBlending`,
//! `transformCase`, `blends` (world + plan-directional), `blendsArmored`,
//! `facing`, `lookingAt*`, `notLookingAt` and the `sliced`/`topHalf`/`botHalf`
//! slice descriptors. Drawing is plan 16's; this module only computes the
//! integer blend state stored on the building (plan 08 M1).
//!
//! ## API for plan 09 (frozen)
//!
//! * `d4(rotation) -> (i32, i32)`, `d4x`, `d4y` — Arc `Geometry.d4` order
//!   `(1,0),(0,1),(-1,0),(0,-1)`.
//! * `BlendWorld` trait — the host supplies the per-block `blends` predicate,
//!   world neighbor lookup, `squareSprite` and `rotatedOutput`/`size`.
//! * `build_blending(&dyn BlendWorld, tile, rotation, directional, check_world)
//!   -> [i32; 5]` returning `[case, scale_x, scale_y, bits, non_square_bits]`.
//! * `BlendNeighbor` — one directional/plan neighbor (`x, y, rotation, block`).
//! * `SliceMode` + `slice_span` for the 16 renderer.

use crate::content::BlockId;
use crate::world::TilePos;
use crate::world::edges::facing_edge;

/// Arc `Geometry.d4x` (`rotation 0 = +x`).
pub const D4X: [i32; 4] = [1, 0, -1, 0];
/// Arc `Geometry.d4y` (`rotation 1 = +y`).
pub const D4Y: [i32; 4] = [0, 1, 0, -1];

/// Arc `Geometry.d4x(i)`.
pub fn d4x(rotation: u8) -> i32 {
    D4X[(rotation % 4) as usize]
}

/// Arc `Geometry.d4y(i)`.
pub fn d4y(rotation: u8) -> i32 {
    D4Y[(rotation % 4) as usize]
}

/// Arc `Geometry.d4(i)`.
pub fn d4(rotation: u8) -> (i32, i32) {
    let i = (rotation % 4) as usize;
    (D4X[i], D4Y[i])
}

/// `Mathf.mod(x, n)` for a positive `n`.
pub fn mod_i(value: i32, n: i32) -> i32 {
    ((value % n) + n) % n
}

/// `Point2.equals`.
pub fn point_equals(x1: i32, y1: i32, x2: i32, y2: i32) -> bool {
    x1 == x2 && y1 == y2
}

/// `Tile.relativeTo(cx, cy)` — direction from `(x, y)` toward `(cx, cy)` or `-1`.
pub fn relative_to(x: i32, y: i32, cx: i32, cy: i32) -> i8 {
    if x == cx && y == cy - 1 {
        1
    } else if x == cx && y == cy + 1 {
        3
    } else if x == cx - 1 && y == cy {
        0
    } else if x == cx + 1 && y == cy {
        2
    } else {
        -1
    }
}

/// One neighbor considered during blending (plan directional or world tile).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlendNeighbor {
    /// Center tile x.
    pub x: i32,
    /// Center tile y.
    pub y: i32,
    /// Neighbor rotation.
    pub rotation: u8,
    /// Neighbor block id.
    pub block: BlockId,
}

/// The block-side `blends` predicate plus world lookups (`buildBlending` host).
pub trait BlendWorld {
    /// The per-block
    /// `blends(tile, rotation, otherx, othery, otherrot, otherblock)`.
    fn blends_block(
        &self,
        source: TilePos,
        rotation: u8,
        other_x: i32,
        other_y: i32,
        other_rot: u8,
        other_block: BlockId,
    ) -> bool;

    /// `tile.nearbyBuild(direction)` — the building adjacent in absolute
    /// direction `direction` (Arc `d4`) to `source`, if any.
    fn near_build(&self, direction: u8, source: TilePos) -> Option<BlendNeighbor>;

    /// `Block.squareSprite`.
    fn square_sprite(&self, block: BlockId) -> bool;

    /// `Block.rotatedOutput(x, y)` (`== Block.rotate` in vanilla).
    fn rotated_output(&self, block: BlockId) -> bool;

    /// `Block.size`.
    fn block_size(&self, block: BlockId) -> i32;
}

/// The mode to slice a texture at (`Autotiler.SliceMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SliceMode {
    /// Full region.
    #[default]
    None,
    /// Lower (right) half.
    Bottom,
    /// Upper (left) half.
    Top,
}

/// Half of a region a [`SliceMode`] selects as `(x_fraction, width_fraction)`.
pub fn slice_span(mode: SliceMode) -> Option<(f32, f32)> {
    match mode {
        SliceMode::None => None,
        SliceMode::Top => Some((0.0, 0.5)),
        SliceMode::Bottom => Some((0.5, 0.5)),
    }
}

/// `Autotiler.transformCase` (`num = -1` leaves `[0, 1]`).
pub fn transform_case(num: i32, bits: &mut [i32; 5]) {
    match num {
        0 => bits[0] = 3,
        1 => bits[0] = 4,
        2 => bits[0] = 2,
        3 => {
            bits[0] = 2;
            bits[2] = -1;
        }
        4 => {
            bits[0] = 1;
            bits[2] = -1;
        }
        5 => bits[0] = 1,
        _ => {}
    }
}

/// `Autotiler.blends(tile, rotation, directional, direction, checkWorld)`.
pub fn blends_directional(
    world: &dyn BlendWorld,
    tile: TilePos,
    rotation: u8,
    directional: &[Option<BlendNeighbor>; 4],
    direction: u8,
    check_world: bool,
) -> bool {
    let real_dir = mod_i(rotation as i32 - direction as i32, 4) as usize;
    if let Some(req) = directional[real_dir]
        && world.blends_block(tile, rotation, req.x, req.y, req.rotation, req.block)
    {
        return true;
    }
    check_world && blends_world(world, tile, rotation, direction)
}

/// `Autotiler.blends(tile, rotation, direction)` (world only).
pub fn blends_world(world: &dyn BlendWorld, tile: TilePos, rotation: u8, direction: u8) -> bool {
    let real_dir = mod_i(rotation as i32 - direction as i32, 4) as u8;
    match world.near_build(real_dir, tile) {
        Some(other) => world.blends_block(
            tile,
            rotation,
            other.x,
            other.y,
            other.rotation,
            other.block,
        ),
        None => false,
    }
}

/// `Autotiler.buildBlending`.
///
/// Returns `[case, scale_x, scale_y, bits, non_square_bits]`. World team checks
/// are the host's responsibility inside [`BlendWorld::near_build`].
pub fn build_blending(
    world: &dyn BlendWorld,
    tile: TilePos,
    rotation: u8,
    directional: &[Option<BlendNeighbor>; 4],
    check_world: bool,
) -> [i32; 5] {
    let mut result = [0i32; 5];
    result[1] = 1;
    result[2] = 1;

    let b = |direction: u8| {
        blends_directional(world, tile, rotation, directional, direction, check_world)
    };
    let num = if b(2) && b(1) && b(3) {
        0
    } else if b(1) && b(3) {
        1
    } else if b(1) && b(2) {
        2
    } else if b(3) && b(2) {
        3
    } else if b(1) {
        4
    } else if b(3) {
        5
    } else {
        -1
    };
    transform_case(num, &mut result);

    result[3] = 0;
    for i in 0..4u8 {
        if b(i) {
            result[3] |= 1 << i;
        }
    }

    result[4] = 0;
    for i in 0..4u8 {
        let real_dir = mod_i(rotation as i32 - i as i32, 4) as u8;
        if b(i)
            && let Some(near) = world.near_build(real_dir, tile)
            && !world.square_sprite(near.block)
        {
            result[4] |= 1 << i;
        }
    }

    result
}

/// `Autotiler.facing`.
pub fn facing(x: i32, y: i32, rotation: u8, x2: i32, y2: i32) -> bool {
    let (dx, dy) = d4(rotation);
    point_equals(x + dx, y + dy, x2, y2)
}

/// `Autotiler.blendsArmored`.
pub fn blends_armored(
    world: &dyn BlendWorld,
    tile: TilePos,
    rotation: u8,
    other_x: i32,
    other_y: i32,
    other_rot: u8,
    other_block: BlockId,
) -> bool {
    let tile_x = tile.x() as i32;
    let tile_y = tile.y() as i32;
    let (dx, dy) = d4(rotation);
    if point_equals(tile_x + dx, tile_y + dy, other_x, other_y) {
        return true;
    }
    if !world.rotated_output(other_block) {
        let facing = facing_edge(world.block_size(other_block), other_x, other_y, tile);
        relative_to(facing.x() as i32, facing.y() as i32, tile_x, tile_y) == rotation as i8
    } else {
        let (ox, oy) = d4(other_rot);
        point_equals(other_x + ox, other_y + oy, tile_x, tile_y)
    }
}

/// `Autotiler.notLookingAt`.
pub fn not_looking_at(
    world: &dyn BlendWorld,
    tile: TilePos,
    other_x: i32,
    other_y: i32,
    other_rot: u8,
    other_block: BlockId,
) -> bool {
    let (ox, oy) = d4(other_rot);
    !(world.rotated_output(other_block)
        && point_equals(other_x + ox, other_y + oy, tile.x() as i32, tile.y() as i32))
}

/// `Autotiler.lookingAtEither`.
pub fn looking_at_either(
    world: &dyn BlendWorld,
    tile: TilePos,
    rotation: u8,
    other_x: i32,
    other_y: i32,
    other_rot: u8,
    other_block: BlockId,
) -> bool {
    let tile_x = tile.x() as i32;
    let tile_y = tile.y() as i32;
    let (dx, dy) = d4(rotation);
    let (ox, oy) = d4(other_rot);
    point_equals(tile_x + dx, tile_y + dy, other_x, other_y)
        || !world.rotated_output(other_block)
        || point_equals(other_x + ox, other_y + oy, tile_x, tile_y)
}

/// `Autotiler.lookingAt`.
pub fn looking_at(
    world: &dyn BlendWorld,
    tile: TilePos,
    rotation: u8,
    other_x: i32,
    other_y: i32,
    other_block: BlockId,
) -> bool {
    let facing = facing_edge(world.block_size(other_block), other_x, other_y, tile);
    let (dx, dy) = d4(rotation);
    point_equals(
        tile.x() as i32 + dx,
        tile.y() as i32 + dy,
        facing.x() as i32,
        facing.y() as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `BlendWorld` whose `blends_block` returns true for a fixed set of
    /// absolute neighbor directions.
    struct Fake {
        dirs: u8,
    }

    impl BlendWorld for Fake {
        fn blends_block(
            &self,
            source: TilePos,
            _rotation: u8,
            other_x: i32,
            other_y: i32,
            _other_rot: u8,
            _other_block: BlockId,
        ) -> bool {
            let dir = relative_to(source.x() as i32, source.y() as i32, other_x, other_y);
            dir >= 0 && self.dirs & (1 << dir) != 0
        }

        fn near_build(&self, direction: u8, source: TilePos) -> Option<BlendNeighbor> {
            if self.dirs & (1 << direction) == 0 {
                return None;
            }
            let (dx, dy) = d4(direction);
            Some(BlendNeighbor {
                x: source.x() as i32 + dx,
                y: source.y() as i32 + dy,
                rotation: 0,
                block: BlockId::AIR,
            })
        }

        fn square_sprite(&self, _block: BlockId) -> bool {
            false
        }

        fn rotated_output(&self, _block: BlockId) -> bool {
            false
        }

        fn block_size(&self, _block: BlockId) -> i32 {
            1
        }
    }

    #[test]
    fn d4_order_matches_geometry() {
        assert_eq!(d4(0), (1, 0));
        assert_eq!(d4(1), (0, 1));
        assert_eq!(d4(2), (-1, 0));
        assert_eq!(d4(3), (0, -1));
    }

    #[test]
    fn transform_case_matrix() {
        let mut bits = [0, 1, 1, 0, 0];
        transform_case(0, &mut bits);
        assert_eq!(bits[0], 3);
        transform_case(3, &mut bits);
        assert_eq!((bits[0], bits[2]), (2, -1));
        transform_case(4, &mut bits);
        assert_eq!((bits[0], bits[2]), (1, -1));
        transform_case(-1, &mut bits);
        assert_eq!((bits[0], bits[2]), (1, -1));
    }

    /// All four sides blend -> case 3 (all sides), bits `0b1111`.
    #[test]
    fn blend_matrix_all_sides() {
        let fake = Fake { dirs: 0b1111 };
        let result = build_blending(&fake, TilePos::new(4, 4), 0, &[None; 4], true);
        assert_eq!(result, [3, 1, 1, 0b1111, 0b1111]);
    }

    /// Only the +y neighbor blends (absolute dir 1); rotation 0 maps
    /// `i = 3` to that neighbor -> case 1, bit 3, non-square bit 3.
    #[test]
    fn blend_matrix_one_side() {
        let fake = Fake { dirs: 0b0010 };
        let result = build_blending(&fake, TilePos::new(4, 4), 0, &[None; 4], true);
        assert_eq!(result, [1, 1, 1, 0b1000, 0b1000]);
    }

    #[test]
    fn facing_matches_geometry() {
        assert!(facing(4, 4, 0, 5, 4));
        assert!(facing(4, 4, 2, 3, 4));
        assert!(!facing(4, 4, 0, 4, 5));
    }
}
