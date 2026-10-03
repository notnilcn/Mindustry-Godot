// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Placement` (`core/src/mindustry/input/Placement.java`).
//!
//! Pure, allocation-free-in-steady-state placement planners: line/rectangle
//! normalization, conveyor A* (node limit 1000), span-node spacing, the bridge
//! placement DP and `ChainedBuilding` upgrade walking. The world is read through
//! the [`PlacementWorld`] trait so this stays Godot-free and testable (I1).

use std::collections::{BinaryHeap, HashMap, HashSet};

use smallvec::SmallVec;

use crate::config::TILESIZE;
use crate::content::BlockId;
use crate::world::TilePos;
use crate::world::config::ConfigValue;

use super::plan::PlanCopy;

/// Cardinal directions, mirroring Arc `Geometry.d4`.
pub const D4: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// A* node limit (`Placement.astar`).
pub const ASTAR_NODE_LIMIT: usize = 1000;
/// Maximum placement line length (`Vars.maxLength`).
pub const MAX_LENGTH: i32 = 100;

/// Read-only world view required by the placement planner.
///
/// The real implementation wraps plan 06's `WorldGrid` + plan 07/08 block data
/// (`mind-gdext`/harness); tests use a small grid. `control.input.block` is
/// parameterized (`Option<BlockId>`) per §3.6.
pub trait PlacementWorld {
    /// Tile in map bounds.
    fn in_bounds(&self, x: i32, y: i32) -> bool;
    /// Block id at a tile (`air` when none).
    fn block_at(&self, x: i32, y: i32) -> BlockId;
    /// `Floor.isDeep()` at a tile.
    fn floor_deep(&self, x: i32, y: i32) -> bool;
    /// `Block.alwaysReplace` at a tile.
    fn always_replace(&self, x: i32, y: i32) -> bool;
    /// `target.canReplace(other)` (plan 07 `can_replace`).
    fn can_replace(&self, target: BlockId, other: BlockId) -> bool;
    /// `Build.validPlace(block, team, x, y, rotation)` (plan 07).
    fn valid_place(&self, block: BlockId, x: i32, y: i32, rotation: u8) -> bool;
    /// Rotation of an existing build at `(x, y)` (`plan.build().rotation`).
    fn build_rotation(&self, x: i32, y: i32) -> Option<u8> {
        let _ = (x, y);
        None
    }
    /// Whether the tile holds a `ChainedBuilding` (plan 08).
    fn is_chained(&self, x: i32, y: i32) -> bool {
        let _ = (x, y);
        false
    }
    /// `ChainedBuilding.next()` at `(x, y)` (plan 08 seam).
    fn chained_next(&self, x: i32, y: i32) -> Option<(i32, i32)> {
        let _ = (x, y);
        None
    }
    /// Local team id (for `Build.validPlace`).
    fn team(&self) -> u8 {
        0
    }
}

/// Bridge placement callback (`Placement.BridgePlacer`).
pub trait BridgePlacer {
    /// `unlockedNow()`.
    fn unlocked_now(&self) -> bool;
    /// `positionsValid(x1, y1, x2, y2)` (plan 08 bridge validity).
    fn positions_valid(&self, x1: i32, y1: i32, x2: i32, y2: i32) -> bool;
    /// Assigns the bridge block/config onto a pair of plans.
    fn apply_to_plans(&self, cur: &mut PlanCopy, other: &mut PlanCopy);
}

/// Result of [`normalize_area`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NormalizeResult {
    /// Min x.
    pub x: i32,
    /// Min y.
    pub y: i32,
    /// Max x (always `>= x`).
    pub x2: i32,
    /// Max y (always `>= y`).
    pub y2: i32,
    /// Inferred rotation `0..=3`.
    pub rotation: i32,
}

/// Result of [`normalize_draw_area`] (world pixels).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct NormalizeDrawResult {
    /// Min x.
    pub x: f32,
    /// Min y.
    pub y: f32,
    /// Max x.
    pub x2: f32,
    /// Max y.
    pub y2: f32,
}

/// `Mathf.sign`.
fn sign(value: i32) -> i32 {
    value.signum()
}

/// `Mathf.mod` (non-negative remainder).
fn pos_mod(value: i32, modulus: i32) -> i32 {
    value.rem_euclid(modulus)
}

/// Direction index from `a` to `b` among [`D4`] (Arc `Tile.relativeTo` shape).
pub fn relative(a: TilePos, b: TilePos) -> i32 {
    match (b.x() as i32 - a.x() as i32, b.y() as i32 - a.y() as i32) {
        (1, 0) => 0,
        (0, -1) => 1,
        (-1, 0) => 2,
        (0, 1) => 3,
        _ => 0,
    }
}

/// `Placement.normalizeLine`: snap to the dominant axis, no diagonals.
pub fn normalize_line(from: TilePos, to: TilePos, out: &mut SmallVec<[TilePos; 128]>) {
    let (sx, sy) = (from.x() as i32, from.y() as i32);
    let (ex, ey) = (to.x() as i32, to.y() as i32);
    if (sx - ex).abs() > (sy - ey).abs() {
        for i in 0..=(sx - ex).abs() {
            out.push(TilePos::new((sx + i * sign(ex - sx)) as i16, sy as i16));
        }
    } else {
        for i in 0..=(sy - ey).abs() {
            out.push(TilePos::new(sx as i16, (sy + i * sign(ey - sy)) as i16));
        }
    }
}

/// `Placement.normalizeRectangle`: steps by `block_size`.
pub fn normalize_rectangle(
    from: TilePos,
    to: TilePos,
    block_size: i32,
    out: &mut SmallVec<[TilePos; 128]>,
) {
    let (sx, sy) = (from.x() as i32, from.y() as i32);
    let (ex, ey) = (to.x() as i32, to.y() as i32);
    let min_x = sx.min(ex);
    let min_y = sy.min(ey);
    let max_x = sx.max(ex);
    let max_y = sy.max(ey);
    let step = block_size.max(1);
    let mut y = 0;
    while y <= max_y - min_y {
        let mut x = 0;
        while x <= max_x - min_x {
            out.push(TilePos::new(
                (sx + x * sign(ex - sx)) as i16,
                (sy + y * sign(ey - sy)) as i16,
            ));
            x += step;
        }
        y += step;
    }
}

/// Walk `ChainedBuilding::next()` from `from` to `to` (`Placement.upgradeLine`).
pub fn upgrade_line(
    world: &dyn PlacementWorld,
    from: TilePos,
    to: TilePos,
    out: &mut SmallVec<[TilePos; 128]>,
) {
    out.push(from);
    if !world.is_chained(from.x() as i32, from.y() as i32) {
        return;
    }
    let mut closed: HashSet<i32> = HashSet::new();
    let mut build = from;
    loop {
        if build == to || !closed.insert(build.pack()) {
            return;
        }
        match world.chained_next(build.x() as i32, build.y() as i32) {
            None => {
                out.clear();
                pathfind_line(world, true, true, None, from, to, out);
                return;
            }
            Some((nx, ny)) => {
                build = TilePos::new(nx as i16, ny as i16);
                out.push(build);
            }
        }
    }
}

/// `Placement.pathfindLine`: conveyor A* or a no-diagonal line.
///
/// Returns `true` when the A* path was used, `false` when `out` holds the
/// no-diagonal/normalized fallback (the caller only cares about the filled
/// buffer; the bool feeds tests/tooling).
pub fn pathfind_line(
    world: &dyn PlacementWorld,
    conveyors: bool,
    pathfinding: bool,
    block: Option<BlockId>,
    from: TilePos,
    to: TilePos,
    out: &mut SmallVec<[TilePos; 128]>,
) -> bool {
    if conveyors && pathfinding {
        if astar(world, block, from, to, out) {
            return true;
        }
        normalize_line(from, to, out);
        false
    } else {
        line_no_diagonal(from, to, out);
        false
    }
}

/// Bresenham line without diagonal steps (Arc `Bresenham2.lineNoDiagonal`).
pub fn line_no_diagonal(from: TilePos, to: TilePos, out: &mut SmallVec<[TilePos; 128]>) {
    let (mut x, mut y) = (from.x() as i32, from.y() as i32);
    let (ex, ey) = (to.x() as i32, to.y() as i32);
    let dx = (ex - x).abs();
    let dy = -(ey - y).abs();
    let sx = if x < ex { 1 } else { -1 };
    let sy = if y < ey { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        out.push(TilePos::new(x as i16, y as i16));
        if x == ex && y == ey {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        } else {
            err += dx;
            y += sy;
        }
    }
}

/// One open-set entry ordered by `(f, packed_pos)` (deterministic).
#[derive(Debug, Clone, Copy, PartialEq)]
struct HeapNode {
    f: f32,
    pos: i32,
}

impl Eq for HeapNode {}

impl PartialOrd for HeapNode {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HeapNode {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Natural order; the call site wraps nodes in `Reverse` for a min-heap.
        self.f.total_cmp(&other.f).then(self.pos.cmp(&other.pos))
    }
}

/// `Placement.tileHeuristic`.
fn tile_heuristic(
    world: &dyn PlacementWorld,
    block: Option<BlockId>,
    parents: &HashMap<i32, i32>,
    tile: TilePos,
    other: TilePos,
) -> f32 {
    let (ox, oy) = (other.x() as i32, other.y() as i32);
    let other_block = world.block_at(ox, oy);
    let replaceable = world.always_replace(ox, oy)
        || block.is_some_and(|block| world.can_replace(block, other_block));
    if !replaceable || world.floor_deep(ox, oy) {
        return 20.0;
    }
    if let Some(prev_packed) = parents.get(&tile.pack()) {
        let prev = TilePos::from_pack(*prev_packed);
        if relative(tile, prev) != relative(other, tile) {
            return 8.0;
        }
    }
    1.0
}

/// `Placement.validNode`.
fn valid_node(world: &dyn PlacementWorld, block: Option<BlockId>, other: TilePos) -> bool {
    let (ox, oy) = (other.x() as i32, other.y() as i32);
    let other_block = world.block_at(ox, oy);
    block.is_some_and(|block| world.can_replace(block, other_block)) || world.always_replace(ox, oy)
}

fn distance_heuristic(x1: i32, y1: i32, x2: i32, y2: i32) -> f32 {
    ((x1 - x2).abs() + (y1 - y2).abs()) as f32
}

/// `Placement.astar` (node limit [`ASTAR_NODE_LIMIT`]).
pub fn astar(
    world: &dyn PlacementWorld,
    block: Option<BlockId>,
    start: TilePos,
    end: TilePos,
    out: &mut SmallVec<[TilePos; 128]>,
) -> bool {
    if start == end {
        return false;
    }
    let mut costs: HashMap<i32, f32> = HashMap::new();
    let mut parents: HashMap<i32, i32> = HashMap::new();
    let mut closed: HashSet<i32> = HashSet::new();
    let mut queue: BinaryHeap<std::cmp::Reverse<HeapNode>> = BinaryHeap::new();

    let start_pos = start.pack();
    let end_pos = end.pack();
    costs.insert(start_pos, 0.0);
    queue.push(std::cmp::Reverse(HeapNode {
        f: distance_heuristic(
            start.x() as i32,
            start.y() as i32,
            end.x() as i32,
            end.y() as i32,
        ),
        pos: start_pos,
    }));

    let mut total_nodes = 0usize;
    let mut found = false;
    while let Some(std::cmp::Reverse(node)) = queue.pop() {
        if total_nodes >= ASTAR_NODE_LIMIT {
            break;
        }
        total_nodes += 1;
        let next = TilePos::from_pack(node.pos);
        let base_cost = costs.get(&node.pos).copied().unwrap_or(0.0);
        if node.pos == end_pos {
            found = true;
            break;
        }
        closed.insert(node.pos);
        for (dx, dy) in D4 {
            let (nx, ny) = (next.x() as i32 + dx, next.y() as i32 + dy);
            if !world.in_bounds(nx, ny) {
                continue;
            }
            let child = TilePos::new(nx as i16, ny as i16);
            if !valid_node(world, block, child) {
                continue;
            }
            let child_pos = child.pack();
            if closed.contains(&child_pos) {
                continue;
            }
            parents.insert(child_pos, node.pos);
            let cost = tile_heuristic(world, block, &parents, next, child) + base_cost;
            costs.insert(child_pos, cost);
            queue.push(std::cmp::Reverse(HeapNode {
                f: cost + distance_heuristic(nx, ny, end.x() as i32, end.y() as i32),
                pos: child_pos,
            }));
        }
    }

    if !found {
        return false;
    }

    out.push(end);
    let mut current = end;
    let mut total = 0usize;
    while current != start && total < ASTAR_NODE_LIMIT {
        let Some(new_pos) = parents.get(&current.pack()).copied() else {
            return false;
        };
        if new_pos == -1 {
            return false;
        }
        let parent = TilePos::from_pack(new_pos);
        out.push(parent);
        current = parent;
        total += 1;
    }
    out.reverse();
    true
}

/// `Placement.isSidePlace`.
pub fn is_side_place(plans: &[PlanCopy]) -> bool {
    plans.len() > 1
        && pos_mod(
            relative(plans[0].tile(), plans[1].tile()) - plans[0].rotation as i32,
            2,
        ) == 1
}

/// `Placement.calculateNodes`: keep first/last + valid points, jump to the
/// furthest overlapping point (bridge/power-node spacing).
pub fn calculate_nodes(
    world: &dyn PlacementWorld,
    team: u8,
    points: &mut SmallVec<[TilePos; 128]>,
    block: BlockId,
    rotation: u8,
    overlapper: impl Fn(TilePos, TilePos) -> bool,
) {
    let _ = team;
    if points.is_empty() {
        return;
    }
    let first = points[0];
    let Some(last) = points.last().copied() else {
        return;
    };
    let base: Vec<TilePos> = points
        .iter()
        .copied()
        .filter(|point| {
            *point == first
                || *point == last
                || world.valid_place(block, point.x() as i32, point.y() as i32, rotation)
        })
        .collect();

    let mut result: SmallVec<[TilePos; 128]> = SmallVec::new();
    let mut added_last = false;
    let mut i = 0usize;
    'outer: while i < base.len() {
        let point = base[i];
        result.push(point);
        if i + 1 == base.len() {
            added_last = true;
        }
        let mut j = base.len();
        while j > i + 1 {
            j -= 1;
            if overlapper(point, base[j]) {
                i = j;
                continue 'outer;
            }
        }
        i += 1;
    }
    if !added_last && let Some(last) = base.last() {
        result.push(*last);
    }
    points.clear();
    points.extend(result);
}

/// `Placement.calculateBridges`: DP assigning bridge pairs along a line.
///
/// The [`PlacementWorld`] parameter is the documented seam for
/// `plan.tile().block()` / `plan.build().rotation` (upstream reads `Vars.world`
/// statics); the public plan §3.6 signature is otherwise unchanged.
#[allow(clippy::too_many_arguments)]
pub fn calculate_bridges(
    world: &dyn PlacementWorld,
    plans: &mut SmallVec<[PlanCopy; 128]>,
    placer: &dyn BridgePlacer,
    has_junction: bool,
    avoid: impl Fn(BlockId) -> bool,
) {
    if is_side_place(plans) || plans.is_empty() {
        return;
    }
    let first = plans[0].clone();
    let last = plans[plans.len() - 1].clone();
    if !(first.x == last.x || first.y == last.y) || !placer.unlocked_now() {
        return;
    }

    let is_first = |plan: &PlanCopy| plan.x == first.x && plan.y == first.y;
    let placeable = |plan: &PlanCopy| -> bool {
        let tile_block = world.block_at(plan.x, plan.y);
        let accessible = plan.placeable() || tile_block == plan.block;
        let rotation_mismatch = !is_first(plan)
            && world
                .build_rotation(plan.x, plan.y)
                .is_some_and(|rotation| rotation != plan.rotation && avoid(tile_block));
        accessible && !rotation_mismatch
    };

    const CONVEYOR_COST: i32 = 3;
    const JUNCTION_COST: i32 = 30;
    const BRIDGE_COST: i32 = 200;
    const BRIDGE_OVER_EMPTY_PENALTY: i32 = 5;
    const INF_COST: i32 = i32::MAX / 2;

    let n = plans.len();
    let mut dp = vec![INF_COST; 2 * n];
    let mut parent = vec![-1i32; 2 * n];
    dp[0] = 0;
    dp[n] = BRIDGE_COST;

    for i in 1..n {
        let cur = plans[i].clone();
        let can_place = placeable(&cur);
        let need_junction = has_junction && avoid(world.block_at(cur.x, cur.y));
        if !can_place && !need_junction {
            continue;
        }
        if can_place {
            dp[i] = dp[i - 1] + CONVEYOR_COST;
        } else {
            dp[i] = dp[i - 1] + JUNCTION_COST;
        }
        parent[i] = i as i32 - 1;

        if dp[i] < INF_COST && can_place {
            dp[n + i] = dp[i] + BRIDGE_COST;
            parent[n + i] = i as i32 - 1;
        }

        if i >= 2 && can_place {
            let mut empty_penalty = 0;
            if placeable(&plans[i - 1]) {
                empty_penalty += BRIDGE_OVER_EMPTY_PENALTY;
            }
            let mut j = i as i32 - 2;
            while j >= 0 {
                let other = plans[j as usize].clone();
                if !placer.positions_valid(cur.x, cur.y, other.x, other.y) {
                    break;
                }
                if placeable(&other) {
                    let cost = dp[n + j as usize] + BRIDGE_COST + empty_penalty;
                    if dp[n + i] > cost {
                        dp[n + i] = cost;
                        parent[n + i] = j;
                    }
                    empty_penalty += BRIDGE_OVER_EMPTY_PENALTY;
                }
                j -= 1;
            }
        }

        if dp[n + i] < dp[i] {
            dp[i] = dp[n + i];
            parent[i] = parent[n + i];
        }

        if can_place && dp[i] >= INF_COST {
            dp[i] = 0;
            dp[n + i] = BRIDGE_COST;
        }
    }

    let mut result: Vec<PlanCopy> = Vec::with_capacity(n);
    let mut bridge_mode = 0usize;
    let mut i = n as i32 - 1;
    while i >= 0 {
        let p = parent[bridge_mode + i as usize];
        if p == -1 || p == i - 1 {
            result.push(plans[i as usize].clone());
            bridge_mode = 0;
            i -= 1;
        } else {
            let (cur_slot, other_slot) = (i as usize, p as usize);
            let mut cur = plans[cur_slot].clone();
            let mut other = plans[other_slot].clone();
            placer.apply_to_plans(&mut cur, &mut other);
            plans[cur_slot] = cur.clone();
            plans[other_slot] = other;
            result.push(cur);
            i = p;
            bridge_mode = n;
        }
    }
    result.reverse();
    plans.clear();
    plans.extend(result);
}

/// `Placement.normalizeArea`.
pub fn normalize_area(
    tilex: i32,
    tiley: i32,
    endx: i32,
    endy: i32,
    rotation: i32,
    snap: bool,
    max_length: i32,
) -> NormalizeResult {
    let (mut tilex, mut tiley, mut endx, mut endy, mut rotation) =
        (tilex, tiley, endx, endy, rotation);
    if snap {
        if (tilex - endx).abs() > (tiley - endy).abs() {
            endy = tiley;
        } else {
            endx = tilex;
        }
    }
    if max_length > 0 {
        if (endx - tilex).abs() > max_length {
            endx = sign(endx - tilex) * max_length + tilex;
        }
        if (endy - tiley).abs() > max_length {
            endy = sign(endy - tiley) * max_length + tiley;
        }
    }
    let dx = endx - tilex;
    let dy = endy - tiley;
    if dx.abs() > dy.abs() {
        rotation = if dx >= 0 { 0 } else { 2 };
    } else if dx.abs() < dy.abs() {
        rotation = if dy >= 0 { 1 } else { 3 };
    }
    if endx < tilex {
        std::mem::swap(&mut endx, &mut tilex);
    }
    if endy < tiley {
        std::mem::swap(&mut endy, &mut tiley);
    }
    NormalizeResult {
        x: tilex,
        y: tiley,
        x2: endx,
        y2: endy,
        rotation,
    }
}

/// `Placement.normalizeDrawArea` (world-pixel rectangle around a block).
#[allow(clippy::too_many_arguments)]
pub fn normalize_draw_area(
    block_size: i32,
    block_offset: f32,
    startx: i32,
    starty: i32,
    endx: i32,
    endy: i32,
    snap: bool,
    max_length: i32,
    scaling: f32,
) -> NormalizeDrawResult {
    let result = normalize_area(startx, starty, endx, endy, 0, snap, max_length);
    let tile = TILESIZE as f32;
    let scale = block_size as f32 * scaling * tile / 2.0;
    NormalizeDrawResult {
        x: result.x as f32 * tile - scale + block_offset,
        y: result.y as f32 * tile - scale + block_offset,
        x2: result.x2 as f32 * tile + scale + block_offset,
        y2: result.y2 as f32 * tile + scale + block_offset,
    }
}

/// `ItemBridgePlacer`: sets both blocks and the relative `Point2` config.
#[derive(Debug, Clone, Copy)]
pub struct RelativeBridgePlacer {
    /// Bridge block id.
    pub block: BlockId,
    /// Whether the bridge is unlocked.
    pub unlocked: bool,
    /// Maximum span (Chebyshev range).
    pub range: i32,
    /// Extra `positions_valid` predicate (plan 08 hook).
    pub use_extra: bool,
}

impl BridgePlacer for RelativeBridgePlacer {
    fn unlocked_now(&self) -> bool {
        self.unlocked
    }

    fn positions_valid(&self, x1: i32, y1: i32, x2: i32, y2: i32) -> bool {
        if self.range <= 0 {
            return false;
        }
        if self.use_extra {
            return false;
        }
        (x1 - x2).abs().max((y1 - y2).abs()) <= self.range
    }

    fn apply_to_plans(&self, cur: &mut PlanCopy, other: &mut PlanCopy) {
        cur.block = self.block;
        other.block = self.block;
        other.config = ConfigValue::Point2(cur.x - other.x, cur.y - other.y);
    }
}

/// `DirectionBridgePlacer`: sets both blocks, no config.
#[derive(Debug, Clone, Copy)]
pub struct DirectionBridgePlacer {
    /// Bridge block id.
    pub block: BlockId,
    /// Whether the bridge is unlocked.
    pub unlocked: bool,
    /// Maximum span.
    pub range: i32,
}

impl BridgePlacer for DirectionBridgePlacer {
    fn unlocked_now(&self) -> bool {
        self.unlocked
    }

    fn positions_valid(&self, x1: i32, y1: i32, x2: i32, y2: i32) -> bool {
        if self.range <= 0 {
            return false;
        }
        (x1 - x2).abs().max((y1 - y2).abs()) <= self.range
    }

    fn apply_to_plans(&self, cur: &mut PlanCopy, other: &mut PlanCopy) {
        cur.block = self.block;
        other.block = self.block;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::TilePos;

    /// Minimal grid-backed [`PlacementWorld`] for the placement algorithm tests.
    pub struct GridWorld {
        width: i32,
        height: i32,
        blocks: Vec<BlockId>,
        always_replace: HashSet<i32>,
        deep: HashSet<i32>,
    }

    impl GridWorld {
        pub fn new(width: i32, height: i32) -> Self {
            Self {
                width,
                height,
                blocks: vec![BlockId::AIR; (width * height) as usize],
                always_replace: HashSet::new(),
                deep: HashSet::new(),
            }
        }

        fn index(&self, x: i32, y: i32) -> usize {
            (y * self.width + x) as usize
        }

        pub fn set(&mut self, x: i32, y: i32, block: BlockId) {
            let index = self.index(x, y);
            self.blocks[index] = block;
        }
    }

    impl PlacementWorld for GridWorld {
        fn in_bounds(&self, x: i32, y: i32) -> bool {
            x >= 0 && y >= 0 && x < self.width && y < self.height
        }

        fn block_at(&self, x: i32, y: i32) -> BlockId {
            if self.in_bounds(x, y) {
                self.blocks[self.index(x, y)]
            } else {
                BlockId::AIR
            }
        }

        fn floor_deep(&self, x: i32, y: i32) -> bool {
            self.deep.contains(&TilePos::new(x as i16, y as i16).pack())
        }

        fn always_replace(&self, x: i32, y: i32) -> bool {
            self.block_at(x, y) == BlockId::AIR
                || self
                    .always_replace
                    .contains(&TilePos::new(x as i16, y as i16).pack())
        }

        fn can_replace(&self, _target: BlockId, other: BlockId) -> bool {
            other == BlockId::AIR
        }

        fn valid_place(&self, _block: BlockId, _x: i32, _y: i32, _rotation: u8) -> bool {
            true
        }
    }

    fn collect(from: (i32, i32), to: (i32, i32)) -> Vec<(i32, i32)> {
        let mut out: SmallVec<[TilePos; 128]> = SmallVec::new();
        normalize_line(
            TilePos::new(from.0 as i16, from.1 as i16),
            TilePos::new(to.0 as i16, to.1 as i16),
            &mut out,
        );
        out.iter().map(|p| (p.x() as i32, p.y() as i32)).collect()
    }

    #[test]
    fn normalize_line_axis() {
        // Horizontal-dominant line snaps to y.
        assert_eq!(
            collect((0, 0), (3, 1)),
            vec![(0, 0), (1, 0), (2, 0), (3, 0)]
        );
        // Vertical-dominant line snaps to x.
        assert_eq!(collect((2, 0), (2, -2)), vec![(2, 0), (2, -1), (2, -2)]);
        // Single tile.
        assert_eq!(collect((5, 5), (5, 5)), vec![(5, 5)]);
        // Tie goes vertical (|dx| > |dy| is false at equality).
        assert_eq!(collect((0, 0), (2, 2)), vec![(0, 0), (0, 1), (0, 2)]);
    }

    #[test]
    fn normalize_rectangle_steps() {
        let mut out: SmallVec<[TilePos; 128]> = SmallVec::new();
        normalize_rectangle(TilePos::new(0, 0), TilePos::new(4, 4), 2, &mut out);
        let points: Vec<(i32, i32)> = out.iter().map(|p| (p.x() as i32, p.y() as i32)).collect();
        assert_eq!(
            points,
            vec![
                (0, 0),
                (2, 0),
                (4, 0),
                (0, 2),
                (2, 2),
                (4, 2),
                (0, 4),
                (2, 4),
                (4, 4)
            ]
        );
        // Negative direction steps still honor block size.
        let mut out2: SmallVec<[TilePos; 128]> = SmallVec::new();
        normalize_rectangle(TilePos::new(4, 4), TilePos::new(0, 0), 2, &mut out2);
        assert_eq!(out2.len(), 9);
    }

    #[test]
    fn astar_detour() {
        let mut world = GridWorld::new(8, 8);
        // Wall between start and end forcing a detour around y=4.
        for y in 0..8 {
            if y != 5 {
                world.set(4, y, BlockId::STONE_WALL);
            }
        }
        let start = TilePos::new(0, 4);
        let end = TilePos::new(7, 4);
        let mut out: SmallVec<[TilePos; 128]> = SmallVec::new();
        assert!(astar(&world, None, start, end, &mut out));
        assert_eq!(out.first().copied(), Some(start));
        assert_eq!(out.last().copied(), Some(end));
        assert!(out.len() > 8, "detour must be longer than a straight line");
        // No diagonal steps.
        for pair in out.windows(2) {
            let dx = (pair[1].x() as i32 - pair[0].x() as i32).abs();
            let dy = (pair[1].y() as i32 - pair[0].y() as i32).abs();
            assert!(dx + dy == 1, "non-adjacent step {:?}", pair);
        }
        // Path legs around the wall: passes through the gap at y=5.
        assert!(out.iter().any(|p| p.y() == 5));
    }

    #[test]
    fn astar_limit_1000() {
        let mut world = GridWorld::new(64, 64);
        // Enclose the goal so no path exists; the reachable area far exceeds
        // the node limit, so A* must give up.
        let (gx, gy) = (60, 60);
        for (dx, dy) in [
            (-1, -1),
            (-1, 0),
            (-1, 1),
            (0, -1),
            (0, 1),
            (1, -1),
            (1, 0),
            (1, 1),
        ] {
            world.set(gx + dx, gy + dy, BlockId::STONE_WALL);
        }
        let mut out: SmallVec<[TilePos; 128]> = SmallVec::new();
        assert!(!astar(
            &world,
            None,
            TilePos::new(0, 0),
            TilePos::new(gx as i16, gy as i16),
            &mut out
        ));
        assert!(out.is_empty());
    }

    #[test]
    fn upgrade_line_chain() {
        struct ChainWorld {
            chain: HashMap<i32, (i32, i32)>,
        }
        impl PlacementWorld for ChainWorld {
            fn in_bounds(&self, _x: i32, _y: i32) -> bool {
                true
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
            fn is_chained(&self, x: i32, y: i32) -> bool {
                self.chain
                    .contains_key(&TilePos::new(x as i16, y as i16).pack())
            }
            fn chained_next(&self, x: i32, y: i32) -> Option<(i32, i32)> {
                self.chain
                    .get(&TilePos::new(x as i16, y as i16).pack())
                    .copied()
            }
        }
        let mut chain = HashMap::new();
        chain.insert(TilePos::new(0, 0).pack(), (1, 0));
        chain.insert(TilePos::new(1, 0).pack(), (2, 0));
        let world = ChainWorld { chain };
        let mut out: SmallVec<[TilePos; 128]> = SmallVec::new();
        upgrade_line(&world, TilePos::new(0, 0), TilePos::new(2, 0), &mut out);
        let points: Vec<(i32, i32)> = out.iter().map(|p| (p.x() as i32, p.y() as i32)).collect();
        assert_eq!(points, vec![(0, 0), (1, 0), (2, 0)]);
    }

    #[test]
    fn upgrade_line_fallback() {
        // Start is chained but the chain ends before `to` -> A* fallback fills
        // an axis-aligned path to the target.
        struct BreakWorld {
            chain: HashMap<i32, (i32, i32)>,
        }
        impl PlacementWorld for BreakWorld {
            fn in_bounds(&self, x: i32, y: i32) -> bool {
                (0..16).contains(&x) && (0..16).contains(&y)
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
            fn is_chained(&self, x: i32, y: i32) -> bool {
                self.chain
                    .contains_key(&TilePos::new(x as i16, y as i16).pack())
            }
            fn chained_next(&self, x: i32, y: i32) -> Option<(i32, i32)> {
                self.chain
                    .get(&TilePos::new(x as i16, y as i16).pack())
                    .copied()
            }
        }
        let mut chain = HashMap::new();
        chain.insert(TilePos::new(0, 0).pack(), (1, 0));
        let world = BreakWorld { chain };
        let mut out: SmallVec<[TilePos; 128]> = SmallVec::new();
        upgrade_line(&world, TilePos::new(0, 0), TilePos::new(5, 0), &mut out);
        assert_eq!(out.last().copied(), Some(TilePos::new(5, 0)));
    }

    #[test]
    fn bridge_dp_costs() {
        let mut world = GridWorld::new(32, 4);
        let bridge = BlockId::new(400);
        let mut plans: SmallVec<[PlanCopy; 128]> = SmallVec::new();
        for x in 0..12 {
            // An unplaceable gap (different block than the plan) at x=4..8
            // forces the DP to bridge across it.
            let plan_block = if (4..8).contains(&x) {
                BlockId::AIR
            } else {
                BlockId::STONE_WALL
            };
            plans.push(PlanCopy::place(x, 0, 0, plan_block));
            if (4..8).contains(&x) {
                world.set(x, 0, BlockId::STONE_WALL);
            }
        }
        let placer = RelativeBridgePlacer {
            block: bridge,
            unlocked: true,
            range: 6,
            use_extra: false,
        };
        calculate_bridges(&world, &mut plans, &placer, false, |_| false);
        // Some plan must have been converted into a bridge, and the item bridge
        // config records the relative offset.
        let bridges: Vec<&PlanCopy> = plans.iter().filter(|plan| plan.block == bridge).collect();
        assert!(
            !bridges.is_empty(),
            "DP must place at least one bridge pair"
        );
        assert!(
            plans
                .iter()
                .any(|plan| matches!(plan.config, ConfigValue::Point2(..))),
            "item bridge endpoint must carry the relative Point2 config"
        );
        // Unplaceable gap plans are dropped and replaced by the bridge span.
        assert!(plans.iter().all(|plan| plan.block != BlockId::AIR));
        assert!(plans.len() <= 12);
    }

    #[test]
    fn bridge_side_place_skip() {
        let world = GridWorld::new(8, 8);
        let bridge = BlockId::new(400);
        let mut plans: SmallVec<[PlanCopy; 128]> = SmallVec::new();
        // First plan faces east (rotation 0); second is to the north -> side place.
        plans.push(PlanCopy::place(0, 0, 0, BlockId::STONE_WALL));
        plans.push(PlanCopy::place(0, 1, 0, BlockId::STONE_WALL));
        let placer = DirectionBridgePlacer {
            block: bridge,
            unlocked: true,
            range: 8,
        };
        let before = plans.clone();
        calculate_bridges(&world, &mut plans, &placer, false, |_| false);
        assert_eq!(plans, before);
    }

    #[test]
    fn nodes_spacing() {
        let world = GridWorld::new(16, 16);
        let mut points: SmallVec<[TilePos; 128]> = SmallVec::new();
        for x in 0..10 {
            points.push(TilePos::new(x, 0));
        }
        // Overlapper: nodes within 3 tiles overlap.
        calculate_nodes(&world, 0, &mut points, BlockId::STONE_WALL, 0, |a, b| {
            (a.x() as i32 - b.x() as i32).abs() <= 3
        });
        assert_eq!(points.first().copied(), Some(TilePos::new(0, 0)));
        assert_eq!(points.last().copied(), Some(TilePos::new(9, 0)));
        assert!(points.len() < 10, "spacing must drop overlapped nodes");
    }
}
