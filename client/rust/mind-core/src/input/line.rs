// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `InputHandler.iterateLine` / `PlaceLine` (plan 15 §3.6).
//!
//! Dispatches a drag into upgrade-line / conveyor-A* / rectangle / straight
//! points and resolves per-point rotation. Block-specific path rewriting
//! (`Block.changePlacementPath`) is the plan-07 [`LineHooks`] seam.

use smallvec::SmallVec;

use crate::content::BlockId;
use crate::world::TilePos;

use super::placement::{
    PlacementWorld, normalize_line, normalize_rectangle, pathfind_line, relative, upgrade_line,
};

/// One resolved placement step (`InputHandler.PlaceLine`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlaceLine {
    /// Tile x.
    pub x: i32,
    /// Tile y.
    pub y: i32,
    /// Resolved rotation `0..=3`.
    pub rotation: u8,
    /// Whether this is the last point.
    pub last: bool,
}

/// The block properties `iterateLine` reads (plan 07 `BlockView` seam).
#[derive(Debug, Clone, Copy)]
pub struct LineBlock {
    /// Block id.
    pub id: BlockId,
    /// Block footprint size in tiles (`Block.size`).
    pub size: i32,
    /// `Block.offset`.
    pub offset: f32,
    /// `Block.allowDiagonal`.
    pub allow_diagonal: bool,
    /// `Block.conveyorPlacement`.
    pub conveyor_placement: bool,
    /// `Block.allowRectanglePlacement`.
    pub allow_rectangle_placement: bool,
    /// `Block.swapDiagonalPlacement`.
    pub swap_diagonal_placement: bool,
    /// `Block.ignoreLineRotation`.
    pub ignore_line_rotation: bool,
}

/// Client-side line parameters (`InputHandler` fields + settings).
#[derive(Debug, Clone, Copy, Default)]
pub struct LineParams {
    /// `diagonal_placement` key held.
    pub diagonal: bool,
    /// Mobile handler active.
    pub mobile: bool,
    /// `swapdiagonal` setting.
    pub swap_diagonal: bool,
    /// `overrideLineRotation` field.
    pub override_line_rotation: bool,
    /// `input.rotation`.
    pub rotation: u8,
}

/// Block-specific path rewriting seam (`Block.changePlacementPath`, plan 07).
pub trait LineHooks {
    /// Rewrites `points` for the active block (junction insertion, etc.).
    fn change_placement_path(
        &self,
        _points: &mut SmallVec<[TilePos; 128]>,
        _rotation: u8,
        _diagonal: bool,
    ) {
    }
}

/// A no-op [`LineHooks`] (plan-07 block path rewriting not yet installed).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoLineHooks;

impl LineHooks for NoLineHooks {}

/// `InputHandler.iterateLine`: resolves a drag into [`PlaceLine`] steps.
#[allow(clippy::too_many_arguments)]
pub fn iterate_line(
    world: &dyn PlacementWorld,
    block: Option<&LineBlock>,
    params: &LineParams,
    start: TilePos,
    end: TilePos,
    hooks: &dyn LineHooks,
    out: &mut Vec<PlaceLine>,
) {
    out.clear();

    let mut diagonal = params.diagonal;
    if params.swap_diagonal && params.mobile {
        diagonal = !diagonal;
    }
    if block.is_some_and(|block| block.swap_diagonal_placement) {
        diagonal = !diagonal;
    }

    let mut points: SmallVec<[TilePos; 128]> = SmallVec::new();
    let mut end_rotation: Option<u8> = None;

    let start_chained = world.is_chained(start.x() as i32, start.y() as i32);
    let end_chained = world.is_chained(end.x() as i32, end.y() as i32);
    if diagonal && block.is_none_or(|block| block.allow_diagonal) {
        if block.is_some() && start_chained && end_chained {
            upgrade_line(world, start, end, &mut points);
        } else {
            let conveyors = block.is_some_and(|block| block.conveyor_placement);
            pathfind_line(
                world,
                conveyors,
                true,
                block.map(|b| b.id),
                start,
                end,
                &mut points,
            );
        }
    } else if block.is_some_and(|block| block.allow_rectangle_placement) {
        normalize_rectangle(start, end, block.map(|b| b.size).unwrap_or(1), &mut points);
    } else {
        normalize_line(start, end, &mut points);
    }

    if points.len() > 1 && end_chained {
        let second_to_last = points[points.len() - 2];
        if !world.is_chained(second_to_last.x() as i32, second_to_last.y() as i32) {
            end_rotation = world.build_rotation(end.x() as i32, end.y() as i32);
        }
    }

    if let Some(block) = block {
        hooks.change_placement_path(&mut points, params.rotation, diagonal);
        let _ = block;
    }

    let mut base_rotation = params.rotation;
    if !params.override_line_rotation || diagonal {
        base_rotation = if start == end {
            params.rotation
        } else {
            relative(start, end) as u8
        };
    }

    // Running multiblock-overlap rect (`Tmp.r3`).
    let mut covered: Option<(f32, f32, f32)> = None;
    for (index, point) in points.iter().enumerate() {
        if let Some(block) = block {
            let size = block.size.max(1) as f32;
            let center = (
                point.x() as f32 + block.offset,
                point.y() as f32 + block.offset,
            );
            if let Some((cx, cy, csize)) = covered {
                let half = size / 2.0;
                let chalf = csize / 2.0;
                if (center.0 - cx).abs() < half + chalf && (center.1 - cy).abs() < half + chalf {
                    continue;
                }
            }
            covered = Some((center.0, center.1, size));
        }

        let next = points.get(index + 1).copied();
        let mut line = PlaceLine {
            x: point.x() as i32,
            y: point.y() as i32,
            rotation: base_rotation,
            last: next.is_none(),
        };
        let ignore_line_rotation =
            block.is_some_and(|block| block.ignore_line_rotation) && !params.mobile;
        if (!params.override_line_rotation || diagonal) && !ignore_line_rotation {
            let resolved = if let Some(next) = next {
                Some(relative(*point, next) as u8)
            } else if let Some(end_rotation) = end_rotation {
                Some(end_rotation)
            } else if block.is_some_and(|block| block.conveyor_placement) && index > 0 {
                Some(relative(points[index - 1], *point) as u8)
            } else {
                None
            };
            if let Some(rotation) = resolved {
                line.rotation = rotation;
            }
        } else {
            line.rotation = params.rotation;
        }
        out.push(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::placement::PlacementWorld;
    use std::collections::HashSet;

    struct FlatWorld {
        width: i32,
        height: i32,
    }

    impl PlacementWorld for FlatWorld {
        fn in_bounds(&self, x: i32, y: i32) -> bool {
            x >= 0 && y >= 0 && x < self.width && y < self.height
        }
        fn block_at(&self, _x: i32, _y: i32) -> BlockId {
            BlockId::AIR
        }
        fn floor_deep(&self, _x: i32, _y: i32) -> bool {
            false
        }
        fn always_replace(&self, _x: i32, _y: i32) -> bool {
            true
        }
        fn can_replace(&self, _t: BlockId, _o: BlockId) -> bool {
            true
        }
        fn valid_place(&self, _b: BlockId, _x: i32, _y: i32, _r: u8) -> bool {
            true
        }
        fn is_chained(&self, _x: i32, _y: i32) -> bool {
            false
        }
    }

    #[test]
    fn straight_line_rotation() {
        let world = FlatWorld {
            width: 16,
            height: 16,
        };
        let params = LineParams {
            rotation: 0,
            ..LineParams::default()
        };
        let mut out = Vec::new();
        iterate_line(
            &world,
            None,
            &params,
            TilePos::new(0, 0),
            TilePos::new(3, 0),
            &NoLineHooks,
            &mut out,
        );
        assert_eq!(out.len(), 4);
        assert_eq!(out[0].rotation, 0);
        assert!(out[3].last);
        assert!(!out[0].last);
    }

    #[test]
    fn rectangle_line_for_rect_block() {
        let world = FlatWorld {
            width: 16,
            height: 16,
        };
        let block = LineBlock {
            id: BlockId::STONE_WALL,
            size: 2,
            offset: 0.0,
            allow_diagonal: false,
            conveyor_placement: false,
            allow_rectangle_placement: true,
            swap_diagonal_placement: false,
            ignore_line_rotation: false,
        };
        let params = LineParams::default();
        let mut out = Vec::new();
        iterate_line(
            &world,
            Some(&block),
            &params,
            TilePos::new(0, 0),
            TilePos::new(4, 4),
            &NoLineHooks,
            &mut out,
        );
        // 2x2 footprint overlap suppression keeps a sparse rect.
        assert!(!out.is_empty());
        assert_eq!(out[0].x, 0);
        assert_eq!(out[0].y, 0);
        let _ = HashSet::<i32>::new();
    }
}
