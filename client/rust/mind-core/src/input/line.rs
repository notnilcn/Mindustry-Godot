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
            super::placement::line_angle_rotation(
                start.x() as i32,
                start.y() as i32,
                end.x() as i32,
                end.y() as i32,
            )
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
            // `Tile.relativeTo` returns `-1` for a non-adjacent pair; Java then
            // keeps `line.rotation` (`if(result != -1)`), modelled by leaving
            // the `base_rotation` initializer in place.
            let resolved = if let Some(next) = next {
                let r = relative(*point, next);
                (r >= 0).then_some(r as u8)
            } else if let Some(end_rotation) = end_rotation {
                Some(end_rotation)
            } else if block.is_some_and(|block| block.conveyor_placement) && index > 0 {
                let r = relative(points[index - 1], *point);
                (r >= 0).then_some(r as u8)
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

/// Mobile legacy; desktop uses [`rotate_plans_about`] with the cursor origin.
/// `Schematics.rotatePlans`: rotate coordinates and rotations around the origin.
///
/// `direction` is 1 for clockwise, 3 (`-1`) for counter-clockwise; other values
/// are taken modulo 4.
pub fn rotate_plans(plans: &mut [super::plan::ClientPlan], direction: i32) {
    let steps = direction.rem_euclid(4);
    for plan in plans.iter_mut() {
        for _ in 0..steps {
            let x = plan.x;
            plan.x = -plan.y;
            plan.y = x;
            plan.rotation = ((plan.rotation as i32 + 1).rem_euclid(4)) as u8;
        }
    }
}

/// Mobile legacy; desktop uses [`flip_plans_about`] with the cursor origin.
/// `Schematics.flipPlans`: mirror coordinates across the origin (`x = -x - 1`).
pub fn flip_plans(plans: &mut [super::plan::ClientPlan], flip_x: bool, flip_y: bool) {
    for plan in plans.iter_mut() {
        if flip_x {
            plan.x = -plan.x - 1;
            plan.rotation = ((4 - plan.rotation as i32).rem_euclid(4)) as u8;
        }
        if flip_y {
            plan.y = -plan.y - 1;
            plan.rotation = ((4 - plan.rotation as i32).rem_euclid(4)) as u8;
        }
    }
}

/// Per-block knobs `InputHandler.rotatePlans`/`flipPlans` read
/// (`Block.java:1653-1660`): size/offset position the multiblock centre and
/// `rotate`/`invert_flip` drive the rotation transforms.
#[derive(Debug, Clone, Copy)]
pub struct PlanBlockInfo {
    /// `Block.size`.
    pub size: i32,
    /// `Block.offset`.
    pub offset: f32,
    /// `Block.rotate`.
    pub rotate: bool,
    /// `Block.invertFlip` (`electrolyzer` only, `Blocks.java:1363`).
    pub invert_flip: bool,
}

/// `Block.planRotation`: `!rotate && lockRotation ? 0 : rot`; `lockRotation`
/// defaults true and no ported block flips it (`Block.java:1653-1655`).
fn plan_rotation(rotation: u8, info: &PlanBlockInfo) -> u8 {
    if info.rotate { rotation } else { 0 }
}

/// Applies a `Point2` transform to the plan config (`BuildPlan.pointConfig`,
/// `BuildPlan.java:88-109`), covering single points and packed point arrays.
fn transform_point_config(
    config: &mut crate::world::config::ConfigValue,
    transform: impl Fn(i32, i32) -> (i32, i32),
) {
    use crate::io::typeio::{pack_point2, unpack_point2_x, unpack_point2_y};
    use crate::world::config::ConfigValue;
    match config {
        ConfigValue::Point2(x, y) => {
            let (nx, ny) = transform(*x, *y);
            *x = nx;
            *y = ny;
        }
        ConfigValue::Point2Array(points) => {
            for packed in points.iter_mut() {
                let (nx, ny) = transform(unpack_point2_x(*packed), unpack_point2_y(*packed));
                *packed = pack_point2(nx, ny);
            }
        }
        _ => {}
    }
}

/// `InputHandler.rotatePlans` (`InputHandler.java:1558-1587`) about the raw
/// cursor origin `(ox, oy)`: clockwise for `direction >= 0`, counter-clockwise
/// otherwise. `info` resolves each plan's [`PlanBlockInfo`].
pub fn rotate_plans_about(
    plans: &mut [super::plan::ClientPlan],
    direction: i32,
    ox: i32,
    oy: i32,
    info: &dyn Fn(BlockId) -> PlanBlockInfo,
) {
    // `World.toTile` = `Math.round(v / tilesize)` (`World.java:174`).
    let to_tile = |value: f32| (value / 8.0 + 0.5).floor() as i32;
    for plan in plans.iter_mut() {
        if plan.breaking {
            continue;
        }
        let meta = info(plan.block);
        let off = if meta.size % 2 == 0 { -0.5 } else { 0.0 };
        transform_point_config(&mut plan.config, |x, y| {
            let (mut cx, mut cy) = (x as f32 + off, y as f32 + off);
            let lx = cx;
            if direction >= 0 {
                cx = -cy;
                cy = lx;
            } else {
                cx = cy;
                cy = -lx;
            }
            ((cx - off).floor() as i32, (cy - off).floor() as i32)
        });

        // `World.toTile` and the multiblock centre offset.
        let wx = (plan.x - ox) as f32 * 8.0 + meta.offset;
        let wy = (plan.y - oy) as f32 * 8.0 + meta.offset;
        let (wx, wy) = if direction >= 0 { (-wy, wx) } else { (wy, -wx) };
        plan.x = to_tile(wx - meta.offset) + ox;
        plan.y = to_tile(wy - meta.offset) + oy;
        plan.rotation = plan_rotation(
            (plan.rotation as i32 + direction).rem_euclid(4) as u8,
            &meta,
        );
    }
}

/// `InputHandler.flipPlans` (`InputHandler.java:1596-1620`): mirror across the
/// horizontal (`flip_x`) or vertical axis through the origin. The coordinate
/// cast truncates toward zero like Java's `(int)`.
pub fn flip_plans_about(
    plans: &mut [super::plan::ClientPlan],
    flip_x: bool,
    ox: i32,
    oy: i32,
    info: &dyn Fn(BlockId) -> PlanBlockInfo,
) {
    let origin = (if flip_x { ox } else { oy }) as f32 * 8.0;
    for plan in plans.iter_mut() {
        if plan.breaking {
            continue;
        }
        let meta = info(plan.block);
        let value = -(((if flip_x { plan.x } else { plan.y }) as f32) * 8.0 - origin + meta.offset)
            + origin;
        if flip_x {
            plan.x = ((value - meta.offset) / 8.0) as i32;
        } else {
            plan.y = ((value - meta.offset) / 8.0) as i32;
        }

        transform_point_config(&mut plan.config, |x, y| {
            if flip_x {
                let mut px = x;
                if meta.size % 2 == 0 {
                    px -= 1;
                }
                (-px, y)
            } else {
                let mut py = y;
                if meta.size % 2 == 0 {
                    py -= 1;
                }
                (x, -py)
            }
        });

        // `Block.flipRotation` (`Block.java:1657-1660`).
        let even = plan.rotation.is_multiple_of(2);
        if (flip_x == even) != meta.invert_flip {
            plan.rotation = plan_rotation((plan.rotation as i32 + 2).rem_euclid(4) as u8, &meta);
        }
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

    /// Flat 1x1 conveyor line block (`conveyorPlacement` true).
    fn conveyor_block() -> LineBlock {
        LineBlock {
            id: BlockId::STONE_WALL,
            size: 1,
            offset: 0.0,
            allow_diagonal: true,
            conveyor_placement: true,
            allow_rectangle_placement: false,
            swap_diagonal_placement: false,
            ignore_line_rotation: false,
        }
    }

    #[test]
    fn vertical_conveyor_drag_points_up() {
        // `Tile.relativeTo`: (5,5) -> (5,6) is north (+y) = rotation 1
        // (Java points a dragged-up conveyor up; `Tile.java:95-101`).
        let world = FlatWorld {
            width: 16,
            height: 16,
        };
        let block = conveyor_block();
        let params = LineParams {
            rotation: 0,
            ..LineParams::default()
        };
        let mut out = Vec::new();
        iterate_line(
            &world,
            Some(&block),
            &params,
            TilePos::new(5, 5),
            TilePos::new(5, 8),
            &NoLineHooks,
            &mut out,
        );
        assert_eq!(out.len(), 4);
        assert!(
            out.iter().all(|line| line.rotation == 1),
            "all steps point up: {out:?}"
        );
    }

    #[test]
    fn downward_drag_points_down() {
        let world = FlatWorld {
            width: 16,
            height: 16,
        };
        let block = conveyor_block();
        let params = LineParams {
            rotation: 0,
            ..LineParams::default()
        };
        let mut out = Vec::new();
        iterate_line(
            &world,
            Some(&block),
            &params,
            TilePos::new(5, 8),
            TilePos::new(5, 5),
            &NoLineHooks,
            &mut out,
        );
        assert_eq!(out.len(), 4);
        assert!(
            out.iter().all(|line| line.rotation == 3),
            "all steps point down: {out:?}"
        );
    }

    #[test]
    fn single_tile_keeps_rotation() {
        // `startX == endX && startY == endY` keeps `input.rotation` (IH:2467).
        let world = FlatWorld {
            width: 16,
            height: 16,
        };
        let block = conveyor_block();
        let params = LineParams {
            rotation: 2,
            ..LineParams::default()
        };
        let mut out = Vec::new();
        iterate_line(
            &world,
            Some(&block),
            &params,
            TilePos::new(5, 5),
            TilePos::new(5, 5),
            &NoLineHooks,
            &mut out,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rotation, 2);
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

    #[test]
    fn rotate_plans_math() {
        use crate::input::plan::ClientPlan;
        let mut plans = vec![
            ClientPlan::place(2, 0, 0, BlockId::STONE_WALL),
            ClientPlan::place(0, 1, 3, BlockId::STONE_WALL),
        ];
        rotate_plans(&mut plans, 1);
        // (x, y) -> (-y, x): (2,0)->(0,2); (0,1)->(-1,0).
        assert_eq!((plans[0].x, plans[0].y), (0, 2));
        assert_eq!((plans[1].x, plans[1].y), (-1, 0));
        assert_eq!(plans[0].rotation, 1);
        // Four rotations are the identity.
        let original = plans.clone();
        for _ in 0..4 {
            rotate_plans(&mut plans, 1);
        }
        assert_eq!(plans, original);
    }

    #[test]
    fn flip_plans_math() {
        use crate::input::plan::ClientPlan;
        let mut plans = vec![ClientPlan::place(2, 3, 0, BlockId::STONE_WALL)];
        flip_plans(&mut plans, true, false);
        assert_eq!((plans[0].x, plans[0].y), (-3, 3));
        // Double flip is the identity for coordinates.
        flip_plans(&mut plans, true, false);
        assert_eq!((plans[0].x, plans[0].y), (2, 3));
        assert_eq!(plans[0].rotation, 0);
    }

    /// Flat 1x1 `PlanBlockInfo` (rotating, no flip inversion).
    fn flat_info(_block: BlockId) -> PlanBlockInfo {
        PlanBlockInfo {
            size: 1,
            offset: 0.0,
            rotate: true,
            invert_flip: false,
        }
    }

    #[test]
    fn rotate_about_quarter_turn() {
        use crate::input::plan::ClientPlan;
        let mut plans = vec![ClientPlan::place(6, 5, 0, BlockId::STONE_WALL)];
        rotate_plans_about(&mut plans, 1, 5, 5, &flat_info);
        assert_eq!((plans[0].x, plans[0].y, plans[0].rotation), (5, 6, 1));
    }

    #[test]
    fn rotate_about_four_times_identity() {
        use crate::input::plan::ClientPlan;
        let mut plans = vec![ClientPlan::place(7, 3, 2, BlockId::STONE_WALL)];
        for _ in 0..4 {
            rotate_plans_about(&mut plans, 1, 5, 5, &flat_info);
        }
        assert_eq!((plans[0].x, plans[0].y, plans[0].rotation), (7, 3, 2));
    }

    #[test]
    fn rotate_about_2x2() {
        use crate::input::plan::ClientPlan;
        let mut plans = vec![ClientPlan::place(6, 5, 0, BlockId::STONE_WALL)];
        let info = |_block: BlockId| PlanBlockInfo {
            size: 2,
            offset: 4.0,
            rotate: true,
            invert_flip: false,
        };
        rotate_plans_about(&mut plans, 1, 5, 5, &info);
        // wx=12, wy=4 -> (-4, 12) -> round(-8/8)+5=4, round(8/8)+5=6.
        assert_eq!((plans[0].x, plans[0].y, plans[0].rotation), (4, 6, 1));
    }

    #[test]
    fn rotate_nonrotating_block_resets_rotation() {
        use crate::input::plan::ClientPlan;
        let mut plans = vec![ClientPlan::place(6, 5, 3, BlockId::STONE_WALL)];
        let info = |_block: BlockId| PlanBlockInfo {
            size: 1,
            offset: 0.0,
            rotate: false,
            invert_flip: false,
        };
        rotate_plans_about(&mut plans, 1, 5, 5, &info);
        assert_eq!(plans[0].rotation, 0, "planRotation resets");
    }

    #[test]
    fn flip_x_about() {
        use crate::input::plan::ClientPlan;
        let mut plans = vec![
            ClientPlan::place(6, 5, 0, BlockId::STONE_WALL),
            ClientPlan::place(6, 5, 1, BlockId::STONE_WALL),
        ];
        flip_plans_about(&mut plans, true, 5, 5, &flat_info);
        // value = -(48-40)+40 = 32 -> x 4; rot%2==0 flips, odd stays.
        assert_eq!((plans[0].x, plans[0].rotation), (4, 2));
        assert_eq!((plans[1].x, plans[1].rotation), (4, 1));
    }

    #[test]
    fn flip_y_about() {
        use crate::input::plan::ClientPlan;
        let mut plans = vec![ClientPlan::place(5, 7, 1, BlockId::STONE_WALL)];
        flip_plans_about(&mut plans, false, 5, 5, &flat_info);
        // value = -(56-40)+40 = 24 -> y 3; rot 1 flips to 3.
        assert_eq!((plans[0].x, plans[0].y, plans[0].rotation), (5, 3, 3));
    }

    #[test]
    fn flip_point_config() {
        use crate::input::plan::ClientPlan;
        use crate::world::config::ConfigValue;
        let mut plans = vec![ClientPlan::place(3, 3, 0, BlockId::STONE_WALL)];
        plans[0].config = ConfigValue::Point2(3, 0);
        flip_plans_about(&mut plans, true, 3, 3, &flat_info);
        assert_eq!(plans[0].config, ConfigValue::Point2(-3, 0));

        let mut rotated = vec![ClientPlan::place(3, 3, 0, BlockId::STONE_WALL)];
        rotated[0].config = ConfigValue::Point2(3, 0);
        rotate_plans_about(&mut rotated, 1, 3, 3, &flat_info);
        assert_eq!(rotated[0].config, ConfigValue::Point2(0, 3));
    }
}
