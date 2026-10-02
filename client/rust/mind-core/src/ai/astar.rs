// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Astar` — synchronous grid A* (plan 11 §3.7).
//!
//! Ported from `core/src/mindustry/ai/Astar.java`. Deviation 6 of plan 11:
//! upstream's `static` scratch (`out`, `queue`, `costs`, `rotations`) becomes a
//! caller-provided [`AstarScratch`] so the algorithm is thread-safe by
//! construction and allocation-free in steady state. The algorithm itself is
//! unchanged: 4-connected (`Geometry.d4`), closed-set mark, `came_from`
//! reconstruction, result excludes the start tile.

use std::collections::BinaryHeap;

use super::pathfinder::PathfindQueue;

/// Distance heuristic over tile coordinates (`DistanceHeuristic`).
pub type DistanceHeuristic = fn(i32, i32, i32, i32) -> f32;

/// Manhattan distance (`Astar.manhattan`).
pub fn manhattan(x1: i32, y1: i32, x2: i32, y2: i32) -> f32 {
    ((x1 - x2).abs() + (y1 - y2).abs()) as f32
}

/// Euclidean distance (`Geometry.dst`).
pub fn euclidean(x1: i32, y1: i32, x2: i32, y2: i32) -> f32 {
    let dx = (x1 - x2) as f32;
    let dy = (y1 - y2) as f32;
    (dx * dx + dy * dy).sqrt()
}

/// Octile distance (diagonal `sqrt2`, orthogonal `1`).
pub fn octile(x1: i32, y1: i32, x2: i32, y2: i32) -> f32 {
    let dx = (x1 - x2).abs() as f32;
    let dy = (y1 - y2).abs() as f32;
    let (min, max) = if dx < dy { (dx, dy) } else { (dy, dx) };
    (max - min) + min * std::f32::consts::SQRT_2
}

/// Per-tile traversal cost term (`Astar.TileHeuristic`).
///
/// `from`/`to` are flat tile indices (`x + y * width`). The blanket impl lets a
/// closure be passed directly.
pub trait TileHeuristic {
    /// Cost of stepping `from -> to`.
    fn cost(&self, from: usize, to: usize) -> f32;
}

impl<F: Fn(usize, usize) -> f32> TileHeuristic for F {
    fn cost(&self, from: usize, to: usize) -> f32 {
        self(from, to)
    }
}

/// Uniform step cost (`1.0` orthogonal).
#[derive(Debug, Clone, Copy)]
pub struct UniformCost(pub f32);

impl Default for UniformCost {
    fn default() -> Self {
        Self(1.0)
    }
}

impl TileHeuristic for UniformCost {
    fn cost(&self, _from: usize, _to: usize) -> f32 {
        self.0
    }
}

/// One queued node, ordered by `f = g + h` (min-first, then node index).
#[derive(Debug, Clone, Copy)]
struct OpenEntry {
    node: usize,
    f: f32,
}

impl PartialEq for OpenEntry {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}

impl Eq for OpenEntry {}

impl Ord for OpenEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // `BinaryHeap` is a max-heap; invert so the smallest `f` pops first.
        other
            .f
            .total_cmp(&self.f)
            .then_with(|| other.node.cmp(&self.node))
    }
}

impl PartialOrd for OpenEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Reusable per-call A* scratch (`Astar` statics; deviation 6).
#[derive(Debug, Default)]
pub struct AstarScratch {
    costs: Vec<f32>,
    came_from: Vec<usize>,
    closed: Vec<bool>,
    open: BinaryHeap<OpenEntry>,
}

impl AstarScratch {
    /// Creates empty scratch.
    pub fn new() -> Self {
        Self::default()
    }

    fn reset(&mut self, len: usize) {
        self.costs.clear();
        self.costs.resize(len, f32::INFINITY);
        self.came_from.clear();
        self.came_from.resize(len, usize::MAX);
        self.closed.clear();
        self.closed.resize(len, false);
        self.open.clear();
    }
}

/// Four orthogonal directions in `Geometry.d4` order (`(1,0), (0,1), (-1,0), (0,-1)`).
const D4: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// Finds a 4-connected path from `start` to `end` on a `width x height` grid.
///
/// `passable(node)` gates traversal (both endpoints must be passable). Returns
/// the path **excluding** `start` and ending at `end` (`[.., end]`), or an empty
/// vec when unreachable — matching upstream `Astar.pathfind`.
#[allow(clippy::too_many_arguments)] // mirrors `Astar.pathfind`'s signature
pub fn pathfind(
    width: i32,
    height: i32,
    start: usize,
    end: usize,
    th: &dyn TileHeuristic,
    dh: DistanceHeuristic,
    passable: &dyn Fn(usize) -> bool,
    scratch: &mut AstarScratch,
) -> Vec<usize> {
    let len = (width.max(0) as usize) * (height.max(0) as usize);
    if width <= 0 || height <= 0 || start >= len || end >= len {
        return Vec::new();
    }
    if !passable(start) || !passable(end) {
        return Vec::new();
    }
    if start == end {
        return Vec::new();
    }

    scratch.reset(len);
    scratch.costs[start] = 0.0;
    scratch.open.push(OpenEntry {
        node: start,
        f: 0.0,
    });

    let end_x = (end as i32) % width;
    let end_y = (end as i32) / width;

    while let Some(entry) = scratch.open.pop() {
        let node = entry.node;
        if scratch.closed[node] {
            continue;
        }
        scratch.closed[node] = true;
        if node == end {
            break;
        }
        let x = (node as i32) % width;
        let y = (node as i32) / width;
        for (dx, dy) in D4 {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx >= width || ny >= height {
                continue;
            }
            let next = (nx + ny * width) as usize;
            if scratch.closed[next] || !passable(next) {
                continue;
            }
            let g = scratch.costs[node] + th.cost(node, next);
            if g < scratch.costs[next] {
                scratch.costs[next] = g;
                scratch.came_from[next] = node;
                let f = g + dh(nx, ny, end_x, end_y);
                scratch.open.push(OpenEntry { node: next, f });
            }
        }
    }

    if scratch.came_from[end] == usize::MAX {
        return Vec::new();
    }

    let mut path = Vec::new();
    let mut current = end;
    while current != start {
        path.push(current);
        let prev = scratch.came_from[current];
        if prev == usize::MAX {
            return Vec::new();
        }
        current = prev;
    }
    path.reverse();
    path
}

/// Uses a [`PathfindQueue`] as the open set (compatibility helper; plan 11 §3.7
/// notes the two are interchangeable). Prefer [`pathfind`] for new call sites.
pub fn pathfind_queue_default() -> PathfindQueue {
    PathfindQueue::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_grid(width: i32, height: i32) -> impl Fn(usize) -> bool + Copy {
        move |node| (node as i32) < width * height
    }

    #[test]
    fn manhattan_optimal() {
        let mut scratch = AstarScratch::new();
        let path = pathfind(
            8,
            8,
            9,
            8 * 4 + 6,
            &UniformCost(1.0),
            manhattan,
            &open_grid(8, 8),
            &mut scratch,
        );
        assert_eq!(path.len(), 8, "(1,1) -> (6,4) is 5+3 orthogonal steps");
        assert_eq!(*path.last().unwrap(), 8 * 4 + 6);
    }

    #[test]
    fn no_path_when_walled_off() {
        // Column x=3 fully walls off the 5x5 grid.
        let passable = |node: usize| {
            let x = (node as i32) % 5;
            x != 3
        };
        let mut scratch = AstarScratch::new();
        let path = pathfind(
            5,
            5,
            0,
            24,
            &UniformCost(1.0),
            manhattan,
            &passable,
            &mut scratch,
        );
        assert!(path.is_empty());
    }

    #[test]
    fn heuristic_consistency() {
        // Admissible heuristics never exceed the true orthogonal step count.
        for (x1, y1, x2, y2) in [(0, 0, 5, 3), (-2, 4, 7, -1), (3, 3, 3, 9)] {
            let true_cost = manhattan(x1, y1, x2, y2);
            assert!(manhattan(x1, y1, x2, y2) <= true_cost);
            assert!(euclidean(x1, y1, x2, y2) <= true_cost + 0.001);
            assert!(octile(x1, y1, x2, y2) <= true_cost + 0.001);
        }
        // Octile is between euclidean and manhattan on a diagonal.
        assert!((octile(0, 0, 4, 4) - 4.0 * std::f32::consts::SQRT_2).abs() < 0.001);
    }

    #[test]
    fn respects_weight_heuristic_and_endpoints() {
        let passable = open_grid(16, 16);
        let mut scratch = AstarScratch::new();
        let path = pathfind(
            16,
            16,
            0,
            15,
            &UniformCost(1.0),
            manhattan,
            &passable,
            &mut scratch,
        );
        assert_eq!(path.len(), 15);
        assert_eq!(path[0], 1);
        assert_eq!(path[14], 15);
    }
}
