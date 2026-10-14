// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Flowfield` — the ground-AI cost field (plan 11 §3.7).
//!
//! Ported from `core/src/mindustry/ai/Pathfinder.java` (`Flowfield`,
//! `PositionTarget`, `EnemyCoreField`). Weights are Java's `int` costs
//! (`Pathfinder.costTypes`); the frontier is a deterministic priority queue
//! whose converged weights equal upstream's label-correcting `updateFrontier`
//! (all costs are non-negative).

use super::cost::{Cost, IMPASSABLE};
use super::path_tile::PathTile;
use super::queue::PathfindQueue;

/// Unreachable node weight. The port tracks "not reached" with `i32::MAX`
/// (Java's weight array is zero-initialized and uses the `searches` array
/// instead, Pathfinder.java:497-521); `Pathfinder.impassable` (`-1`) is the
/// cost-function sentinel, [`IMPASSABLE`].
pub const UNREACHABLE: i32 = i32::MAX;

/// Direction order (`Geometry.d8`, Arc Geometry.java:25-34) so neighbour
/// tie-breaks match `getTargetTile`; `advance` skips the diagonals (`d4`).
const DIRS: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// A cached flow field targeting a set of tiles (upstream `Flowfield`).
#[derive(Debug, Clone)]
pub struct Flowfield {
    /// Target tile indices (upstream `targets`).
    pub targets: Vec<usize>,
    /// Complete traversal weights (`completeWeights`; `-1` = unreachable).
    pub complete_weights: Vec<i32>,
    /// The cost function this field was built for (`Flowfield.cost`).
    pub cost: Cost,
    /// The pathing team (`Flowfield.team`).
    pub team: u8,
    /// Whether the field finished computing.
    pub done: bool,
    /// Incremental frontier (M3 worker state; empty after `update`).
    pub frontier: PathfindQueue,
    /// Scratch `cameFrom` (reserved for path reconstruction).
    pub(crate) came_from: Vec<usize>,
}

impl Flowfield {
    /// Creates an empty field for a `width x height` grid.
    pub fn new(width: i32, height: i32) -> Self {
        let len = (width.max(0) as usize) * (height.max(0) as usize);
        Self {
            targets: Vec::new(),
            complete_weights: vec![UNREACHABLE; len],
            cost: Cost::Ground,
            team: 0,
            done: false,
            frontier: PathfindQueue::new(),
            came_from: vec![usize::MAX; len],
        }
    }

    /// Sets the targets and recomputes the field (`Flowfield.updateTargetPositions`
    /// and full frontier solve). Equivalent to [`begin_update`](Self::begin_update)
    /// followed by an unbounded [`advance`](Self::advance).
    pub fn update(&mut self, tiles: &[PathTile], width: i32, height: i32, cost: Cost, team: u8) {
        self.cost = cost;
        self.team = team;
        self.begin_update(width, height);
        let _ = self.advance(tiles, width, height, team, u32::MAX);
    }

    /// Restarts the incremental solve: resets the working weights, clears the
    /// frontier and seeds the targets (`Pathfinder.registerPath`). The field is
    /// then advanced by [`advance`](Self::advance).
    pub fn begin_update(&mut self, width: i32, height: i32) {
        let len = self.complete_weights.len();
        self.complete_weights.fill(UNREACHABLE);
        self.came_from.fill(usize::MAX);
        self.frontier.clear();
        self.done = false;
        if width <= 0 || height <= 0 || len == 0 {
            self.done = true;
            return;
        }
        for &target in &self.targets {
            if target < len {
                self.complete_weights[target] = 0;
                self.frontier.add(target, 0);
            }
        }
    }

    /// Advances the frontier by at most `budget` pops (`Pathfinder.updateFrontier`)
    /// and returns whether the field is complete. The persisted frontier and
    /// working weights make repeated budgeted calls converge on exactly the same
    /// weights as a single unbounded solve.
    ///
    /// Neighbours are `Geometry.d4` only, like upstream (`Pathfinder.java:505-520`).
    pub fn advance(
        &mut self,
        tiles: &[PathTile],
        width: i32,
        height: i32,
        team: u8,
        budget: u32,
    ) -> bool {
        if self.done {
            return true;
        }
        let mut processed: u32 = 0;
        while let Some((node, weight)) = self.frontier.poll() {
            if weight <= self.complete_weights[node] {
                let x = (node as i32) % width;
                let y = (node as i32) / width;
                for (dx, dy) in DIRS {
                    // `Geometry.d4` only.
                    if dx != 0 && dy != 0 {
                        continue;
                    }
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx < 0 || ny < 0 || nx >= width || ny >= height {
                        continue;
                    }
                    let next = (nx + ny * width) as usize;
                    let other_cost = self.cost.cost(team, tiles[next]);
                    if other_cost == IMPASSABLE {
                        continue;
                    }
                    let next_weight = self.complete_weights[node].saturating_add(other_cost);
                    // `weights[newPos] > cost + otherCost`
                    if next_weight < self.complete_weights[next] {
                        self.complete_weights[next] = next_weight;
                        self.came_from[next] = node;
                        self.frontier.add(next, next_weight);
                    }
                }
            }
            processed = processed.saturating_add(1);
            if processed >= budget {
                // Budget exhausted. An empty frontier means the solve already
                // converged (Dijkstra with non-negative costs), so mark it done;
                // otherwise `done` stays false until the frontier drains.
                if self.frontier.is_empty() {
                    self.done = true;
                    return true;
                }
                return false;
            }
        }
        self.done = true;
        true
    }

    /// Whether a target is reachable.
    pub fn has_targets(&self) -> bool {
        !self.targets.is_empty()
    }

    /// Whether `node` is a target tile.
    pub fn is_target(&self, node: usize) -> bool {
        self.targets.contains(&node)
    }

    /// `Flowfield.passable` (Pathfinder.java:698-703) for this field's cost/team.
    pub fn passable(&self, tiles: &[PathTile], node: usize) -> bool {
        tiles
            .get(node)
            .is_some_and(|tile| self.cost.passable(self.team, *tile))
    }

    /// The next tile to step to from `node` (upstream `getTargetTile`,
    /// Pathfinder.java:382-436), or `None` when the unit should stay (no
    /// targets, already there, no cheaper passable neighbour, or the step would
    /// enter a dangerous tile from a safe one on the ground cost).
    ///
    /// `dangerous` is `Tile.dangerous()` per flat index (built with the packed
    /// tiles); `diagonals` selects `Geometry.d8` over `d4`.
    pub fn get_target_tile(
        &self,
        tiles: &[PathTile],
        dangerous: &[bool],
        node: usize,
        width: i32,
        height: i32,
        diagonals: bool,
    ) -> Option<usize> {
        if self.targets.is_empty()
            || node >= self.complete_weights.len()
            || width <= 0
            || height <= 0
        {
            return None;
        }
        if self.is_target(node) {
            return None;
        }
        let value = self.complete_weights[node];
        let x = (node as i32) % width;
        let y = (node as i32) / width;
        let mut current: Option<usize> = None;
        let mut current_cost = 0i32;
        for (dx, dy) in DIRS {
            let diagonal = dx != 0 && dy != 0;
            if diagonal && !diagonals {
                continue;
            }
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx >= width || ny >= height {
                continue;
            }
            let packed = (nx + ny * width) as usize;
            let cost = self.complete_weights[packed];
            if cost >= value
                || !self.passable(tiles, packed)
                || (current.is_some() && cost >= current_cost)
            {
                continue;
            }
            // Diagonal corner trap (Pathfinder.java:429-431).
            if diagonal {
                let side_a = ((x + dx) + y * width) as usize;
                let side_b = (x + (y + dy) * width) as usize;
                if !self.passable(tiles, side_a) || !self.passable(tiles, side_b) {
                    continue;
                }
            }
            current = Some(packed);
            current_cost = cost;
        }
        let next = current?;
        // `if(current == null || tl == impassable ||
        //   (path.cost == costTypes.get(costGround) && current.dangerous() && !tile.dangerous())) return tile;`
        // (`tl == impassable` cannot be selected here: an unreached neighbour
        // has weight `UNREACHABLE` and never compares below `value`.)
        let _ = IMPASSABLE;
        if self.cost == Cost::Ground
            && dangerous.get(next).copied().unwrap_or(false)
            && !dangerous.get(node).copied().unwrap_or(false)
        {
            return None;
        }
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::super::path_tile::PathTile;
    use super::*;

    fn tiles_flat(width: i32, height: i32) -> Vec<PathTile> {
        vec![PathTile::default(); (width * height) as usize]
    }

    #[test]
    fn converges_downhill_to_target() {
        // 16x1 corridor; target at the right end.
        let width = 16;
        let height = 1;
        let tiles = tiles_flat(width, height);
        let dangerous = vec![false; tiles.len()];
        let mut field = Flowfield::new(width, height);
        field.targets = vec![15];
        field.update(&tiles, width, height, Cost::Ground, 0);
        assert!(field.done);
        assert_eq!(field.complete_weights[15], 0);
        assert_eq!(field.complete_weights[0], 15);
        // Steps strictly decrease until the target is reached.
        let mut node = 0usize;
        let mut steps = 0;
        while let Some(next) = field.get_target_tile(&tiles, &dangerous, node, width, height, false)
        {
            assert!(field.complete_weights[next] < field.complete_weights[node]);
            node = next;
            steps += 1;
            assert!(steps <= 15);
        }
        assert_eq!(node, 15);
    }

    #[test]
    fn walls_block_and_corners_are_not_cut() {
        // A solid wall at x=1 fully blocks the 3x3 grid's left column.
        let width = 3;
        let height = 3;
        let mut tiles = tiles_flat(width, height);
        for y in 0..3 {
            tiles[(1 + y * width) as usize] = PathTile::from_parts(0, 0, true, false, false);
        }
        let dangerous = vec![false; tiles.len()];
        let mut field = Flowfield::new(width, height);
        field.targets = vec![8]; // bottom-right
        field.update(&tiles, width, height, Cost::Ground, 0);
        assert_eq!(field.complete_weights[0], UNREACHABLE);
        assert_ne!(field.complete_weights[8], UNREACHABLE);
        // No diagonal cut through the wall corner.
        assert!(
            field
                .get_target_tile(&tiles, &dangerous, 0, width, height, true)
                .is_none()
        );
    }

    /// T15: a 9x5 grid with a 3x3 deep pool in the middle routes around it.
    #[test]
    fn flowfield_routes_around_deep_water() {
        let width = 9;
        let height = 5;
        let mut tiles = tiles_flat(width, height);
        // Deep pool at tiles (3..5, 1..3): deep + all-deep bits.
        for y in 1..=3i32 {
            for x in 3..=5i32 {
                tiles[(x + y * width) as usize] = PathTile::from_bits(
                    0, 0, false, false, false, false, true, false, false, true, false, true, true,
                    false,
                );
            }
        }
        let dangerous = vec![false; tiles.len()];
        let mut field = Flowfield::new(width, height);
        field.targets = vec![(8 + 2 * width) as usize];
        field.update(&tiles, width, height, Cost::Ground, 0);
        assert!(field.done);
        // Walk the downhill path from (0,2) and never enter a deep tile.
        let mut node = (2 * width) as usize;
        let mut steps = 0;
        while let Some(next) = field.get_target_tile(&tiles, &dangerous, node, width, height, true)
        {
            assert!(!tiles[next].deep(), "path entered a deep tile at {next}");
            node = next;
            steps += 1;
            assert!(steps <= (width * height) as usize, "path did not converge");
        }
        assert_eq!(node, (8 + 2 * width) as usize, "reached the target");
    }

    /// T15: `getTargetTile` refuses to step from a safe tile onto a dangerous
    /// one on the ground cost (Pathfinder.java:434).
    #[test]
    fn get_target_tile_refuses_dangerous_step() {
        let width = 4;
        let height = 1;
        let mut tiles = tiles_flat(width, height);
        // A shallow damaging floor on the only downhill neighbour.
        tiles[2] = PathTile::from_bits(
            0, 0, false, false, false, false, true, false, false, false, true, false, false, false,
        );
        let mut dangerous = vec![false; tiles.len()];
        dangerous[2] = true;
        let mut field = Flowfield::new(width, height);
        field.targets = vec![3];
        field.update(&tiles, width, height, Cost::Ground, 0);
        // From tile 1 (safe) the only downhill neighbour is the damaging tile.
        assert_eq!(
            field.get_target_tile(&tiles, &dangerous, 1, width, height, true),
            None,
            "staying is better than walking into a damaging floor"
        );
        // Standing on the dangerous tile itself, the unit keeps moving.
        assert_eq!(
            field.get_target_tile(&tiles, &dangerous, 2, width, height, true),
            Some(3)
        );
    }
}
