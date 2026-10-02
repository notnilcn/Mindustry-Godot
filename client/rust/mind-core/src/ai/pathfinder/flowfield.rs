// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Flowfield` — the ground-AI cost field (plan 11 §3.7).
//!
//! Ported from `core/src/mindustry/ai/Pathfinder.java` (`Flowfield`,
//! `PositionTarget`, `EnemyCoreField`). M0 computes complete weights with an
//! exact Dijkstra pass; M3 moves the frontier into the deterministic incremental
//! worker (`worker.rs`). The output is identical either way.

use super::cost::Cost;
use super::path_tile::PathTile;
use super::queue::PathfindQueue;

/// Unreachable node weight.
pub const UNREACHABLE: f32 = f32::INFINITY;

/// Diagonal step weight (`Mathf.sqrt2`).
const SQRT2: f32 = std::f32::consts::SQRT_2;

/// Direction order (fixed so Dijkstra tie-breaks are deterministic).
const DIRS: [(i32, i32); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];

/// A cached flow field targeting a set of tiles (upstream `Flowfield`).
#[derive(Debug, Clone)]
pub struct Flowfield {
    /// Target tile indices (upstream `targets`).
    pub targets: Vec<usize>,
    /// Complete traversal weights (`completeWeights`; `INF` = unreachable).
    pub complete_weights: Vec<f32>,
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
            done: false,
            frontier: PathfindQueue::new(),
            came_from: vec![usize::MAX; len],
        }
    }

    /// Sets the targets and recomputes the field (`Flowfield.updateTargetPositions`
    /// + full frontier solve).
    pub fn update(
        &mut self,
        tiles: &[PathTile],
        width: i32,
        height: i32,
        cost: Cost,
        diagonals: bool,
    ) {
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
                self.complete_weights[target] = 0.0;
                self.frontier.add(target, 0.0);
            }
        }
        while let Some((node, weight)) = self.frontier.poll() {
            if weight > self.complete_weights[node] {
                continue;
            }
            let x = (node as i32) % width;
            let y = (node as i32) / width;
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
                let next = (nx + ny * width) as usize;
                if !cost.passable(tiles[next]) {
                    continue;
                }
                // No corner cutting: both orthogonal neighbors must be passable.
                if diagonal {
                    let side_a = ((x + dx) + y * width) as usize;
                    let side_b = (x + (y + dy) * width) as usize;
                    if !cost.passable(tiles[side_a]) || !cost.passable(tiles[side_b]) {
                        continue;
                    }
                }
                let step = if diagonal { SQRT2 } else { 1.0 };
                let next_weight = self.complete_weights[node] + step;
                if next_weight < self.complete_weights[next] {
                    self.complete_weights[next] = next_weight;
                    self.came_from[next] = node;
                    self.frontier.add(next, next_weight);
                }
            }
        }
        self.done = true;
    }

    /// Whether a target is reachable.
    pub fn has_targets(&self) -> bool {
        !self.targets.is_empty()
    }

    /// Whether `node` is a target tile.
    pub fn is_target(&self, node: usize) -> bool {
        self.targets.contains(&node)
    }

    /// The next tile to step to from `node` (upstream `getTargetTile`), or `None`
    /// when the field has no targets / the node is already there.
    pub fn get_target_tile(
        &self,
        node: usize,
        width: i32,
        height: i32,
        diagonals: bool,
    ) -> Option<usize> {
        if self.targets.is_empty() || node >= self.complete_weights.len() {
            return None;
        }
        if self.is_target(node) {
            return None;
        }
        let current = self.complete_weights[node];
        if !current.is_finite() {
            return None;
        }
        let x = (node as i32) % width;
        let y = (node as i32) / width;
        let mut best: Option<(usize, f32)> = None;
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
            let next = (nx + ny * width) as usize;
            let weight = self.complete_weights[next];
            if !weight.is_finite() || weight >= current {
                continue;
            }
            match best {
                Some((_, best_weight)) if weight >= best_weight => {}
                _ => best = Some((next, weight)),
            }
        }
        best.map(|(next, _)| next)
    }
}

#[cfg(test)]
mod tests {
    use super::super::path_tile::PathTile;
    use super::*;

    #[test]
    fn converges_downhill_to_target() {
        // 16x1 corridor; target at the right end.
        let width = 16;
        let height = 1;
        let tiles = vec![PathTile::default(); (width * height) as usize];
        let mut field = Flowfield::new(width, height);
        field.targets = vec![15];
        field.update(&tiles, width, height, Cost::Ground, false);
        assert!(field.done);
        assert_eq!(field.complete_weights[15], 0.0);
        assert_eq!(field.complete_weights[0], 15.0);
        // Steps strictly decrease until the target is reached.
        let mut node = 0usize;
        let mut steps = 0;
        while let Some(next) = field.get_target_tile(node, width, height, false) {
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
        let mut tiles = vec![PathTile::default(); 9];
        for y in 0..3 {
            tiles[(1 + y * width) as usize] = PathTile::from_parts(0, 0, true, false, false);
        }
        let mut field = Flowfield::new(width, height);
        field.targets = vec![8]; // bottom-right
        field.update(&tiles, width, height, Cost::Ground, true);
        assert!(!field.complete_weights[0].is_finite());
        assert!(field.complete_weights[8].is_finite());
    }
}
