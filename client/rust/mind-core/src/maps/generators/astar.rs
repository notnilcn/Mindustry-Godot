// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Deterministic grid A* (plan 06 §3.10, risk R4).
//!
//! Ported from the grid-search subset of `core/src/mindustry/ai/Astar.java`:
//! 4-neighbour search, Manhattan heuristic, `BinaryHeap` ordered by
//! `(f, insertion sequence)` so equal-cost expansion is deterministic. Used by
//! the generators/filters; plan 11 may reuse or wrap it.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::world::tiles::Tiles;

/// Manhattan distance heuristic (`Astar.manhattan`).
pub fn manhattan(x1: i32, y1: i32, x2: i32, y2: i32) -> f32 {
    (x1 - x2).abs() as f32 + (y1 - y2).abs() as f32
}

#[derive(PartialEq)]
struct Node {
    f: f32,
    seq: u64,
    x: i32,
    y: i32,
}

impl Eq for Node {}

impl Ord for Node {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap is a max-heap: reverse for min-f, tie-break on seq.
        other
            .f
            .total_cmp(&self.f)
            .then_with(|| other.seq.cmp(&self.seq))
    }
}

impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A* over the tile grid (`Astar.pathfind`).
///
/// `cost(from, to)` is the movement cost into `to`; `passable(tile)` gates
/// expansion. Returns the path from start to end (excluding start), or empty.
pub fn pathfind<C, P>(
    tiles: &Tiles,
    start: (i32, i32),
    end: (i32, i32),
    cost: C,
    passable: P,
) -> Vec<(i32, i32)>
where
    C: Fn(&crate::world::Tile, &crate::world::Tile) -> f32,
    P: Fn(&crate::world::Tile) -> bool,
{
    let width = tiles.width;
    let height = tiles.height;
    let n = (width * height) as usize;
    if n == 0 || !tiles.in_bounds(start.0, start.1) || !tiles.in_bounds(end.0, end.1) {
        return Vec::new();
    }

    let idx = |x: i32, y: i32| (x + y * width) as usize;
    let mut g_score = vec![f32::INFINITY; n];
    let mut closed = vec![false; n];
    let mut parent = vec![(-1i32, -1i32); n];
    let mut seq = 0u64;
    let mut queue = BinaryHeap::new();

    g_score[idx(start.0, start.1)] = 0.0;
    queue.push(Node {
        f: manhattan(start.0, start.1, end.0, end.1),
        seq,
        x: start.0,
        y: start.1,
    });

    let mut found = false;
    while let Some(node) = queue.pop() {
        let node_index = idx(node.x, node.y);
        if closed[node_index] {
            continue;
        }
        if (node.x, node.y) == end {
            found = true;
            break;
        }
        closed[node_index] = true;
        let base = g_score[node_index];

        for (dx, dy) in [(-1, 0), (0, -1), (1, 0), (0, 1)] {
            let nx = node.x + dx;
            let ny = node.y + dy;
            if !tiles.in_bounds(nx, ny) {
                continue;
            }
            let child = tiles.get(nx, ny);
            if !passable(child) {
                continue;
            }
            let child_index = idx(nx, ny);
            if closed[child_index] {
                continue;
            }
            let new_cost = base + cost(tiles.get(node.x, node.y), child);
            if new_cost < g_score[child_index] {
                g_score[child_index] = new_cost;
                parent[child_index] = (node.x, node.y);
                seq += 1;
                queue.push(Node {
                    f: new_cost + manhattan(nx, ny, end.0, end.1),
                    seq,
                    x: nx,
                    y: ny,
                });
            }
        }
    }

    if !found {
        return Vec::new();
    }

    let mut path = Vec::new();
    let mut current = (end.0, end.1);
    while current != start {
        path.push(current);
        let (px, py) = parent[idx(current.0, current.1)];
        if px < 0 {
            return Vec::new();
        }
        current = (px, py);
    }
    path.reverse();
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlockId;

    #[test]
    fn finds_shortest_path_around_wall() {
        let mut tiles = Tiles::new(5, 5);
        // Vertical wall at x=2 with a gap at y=4.
        for y in 0..4 {
            tiles.get_mut(2, y).block = BlockId::STONE_WALL;
        }
        let path = pathfind(
            &tiles,
            (0, 0),
            (4, 0),
            |_, _| 1.0,
            |tile| tile.block == BlockId::AIR,
        );
        assert!(!path.is_empty());
        assert_eq!(*path.last().unwrap(), (4, 0));
        assert!(path.contains(&(2, 4)), "path goes through the gap");
    }

    #[test]
    fn no_path_when_blocked() {
        let mut tiles = Tiles::new(3, 3);
        for y in 0..3 {
            tiles.get_mut(1, y).block = BlockId::STONE_WALL;
        }
        let path = pathfind(
            &tiles,
            (0, 0),
            (2, 0),
            |_, _| 1.0,
            |tile| tile.block == BlockId::AIR,
        );
        assert!(path.is_empty());
    }
}
