// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `ControlPathfinder` (plan 11 §3.7/§4.2).
//!
//! Ported from `core/src/mindustry/ai/ControlPathfinder.java`. This module owns
//! the cluster/portal build, the shared-boundary portal graph, per-cluster inner
//! edges, raycasts and the per-request result API
//! ([`ControlPathfinder::get_path_position`]).
//!
//! **Owner note — per-cluster `FieldCache` acceleration (deferred).** The
//! upstream per-cluster flowfield cache is intentionally **not** implemented:
//! it is `cache[team][cost]` keyed by packed `(goalPos, costId, team)` with a
//! `12×12` weight array + frontier per `FieldCache`, updated under the
//! deterministic control-node budget (`CONTROL_NODES_PER_TICK`, plan 11 §3.7),
//! invalidated on `clusterChanged` and dropped after 30 `update_id`s idle. The
//! request path here runs the plan-11 synchronous A* and caches the resulting
//! tile list per `(start, goal)`. The portal graph and cluster/request API are
//! complete so `CommandAI`/`LogicAI` callsites are unaffected. This is a
//! **plan-23 perf** follow-up and additionally needs a `units bench --profile
//! path` harness surface (mind-headless `units_scenarios::bench`, not owned by
//! this lane) to measure the win before landing.

use std::collections::BTreeMap;

use crate::content::ContentRegistry;
use crate::world::{TilePos, WorldGrid};

use super::astar::{self, AstarScratch, UniformCost};
use super::pathfinder::cost::Cost;
use super::pathfinder::path_tile;

/// `ControlPathfinder.clusterSize`.
pub const CLUSTER_SIZE: i32 = 12;

/// A boundary portal shared between two adjacent clusters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Portal {
    /// Tile on the low side of the boundary.
    pub a: TilePos,
    /// Tile on the high side of the boundary (adjacent to `a`).
    pub b: TilePos,
    /// Cluster index containing `a`.
    pub cluster_a: usize,
    /// Cluster index containing `b`.
    pub cluster_b: usize,
}

/// Per-request path result (`PathfindResult`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PathfindResult {
    /// Whether the destination is unreachable.
    pub unreachable: bool,
    /// Next tile to move to (`null` when already at `dest`).
    pub next: Option<TilePos>,
    /// Full tile path from the request start to the destination (exclusive start).
    pub path: Vec<TilePos>,
}

/// Cluster/portal A* pathfinder (plan 11 §3.7).
#[derive(Debug, Default)]
pub struct ControlPathfinder {
    /// Grid width in tiles.
    pub width: i32,
    /// Grid height in tiles.
    pub height: i32,
    /// Cluster columns (`ceil(width / CLUSTER_SIZE)`).
    pub cluster_w: i32,
    /// Cluster rows.
    pub cluster_h: i32,
    /// Per-tile ground passability.
    pub passable: Vec<bool>,
    /// All boundary portals.
    pub portals: Vec<Portal>,
    /// `cluster index -> portal indices` (shared between the two clusters).
    pub cluster_portals: Vec<Vec<usize>>,
    /// `cluster index -> (portal_a, portal_b, distance)` inner edges.
    pub inner_edges: Vec<Vec<(usize, usize, f32)>>,
    /// Request result cache keyed by `(start_index, goal_index)`.
    cache: BTreeMap<(usize, usize), Vec<usize>>,
    /// A* scratch (deviation 6: per-call scratch, owned here).
    scratch: AstarScratch,
    /// Monotonic update counter (determinism/debug).
    pub updates: u64,
}

impl ControlPathfinder {
    /// Creates an empty pathfinder for a `width x height` grid.
    pub fn new(width: i32, height: i32) -> Self {
        Self {
            width,
            height,
            ..Self::default()
        }
    }

    /// Rebuilds passability, clusters and portals from the world (`WorldLoadEvent`).
    pub fn build(&mut self, grid: &WorldGrid, content: &ContentRegistry, team: u8) {
        self.width = grid.width();
        self.height = grid.height();
        let tiles = path_tile::build_tiles(grid, content, team);
        self.passable = tiles
            .iter()
            .map(|tile| Cost::Ground.passable(*tile))
            .collect();
        self.cluster_w = (self.width + CLUSTER_SIZE - 1).max(0) / CLUSTER_SIZE;
        self.cluster_h = (self.height + CLUSTER_SIZE - 1).max(0) / CLUSTER_SIZE;
        let cluster_count = (self.cluster_w * self.cluster_h).max(0) as usize;
        self.cluster_portals = vec![Vec::new(); cluster_count];
        self.inner_edges = vec![Vec::new(); cluster_count];
        self.portals.clear();
        self.cache.clear();
        self.build_portals();
        self.build_inner_edges();
    }

    fn build_portals(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                if !self.at(x, y) {
                    continue;
                }
                // Horizontal boundary: tile to the right is in the next cluster.
                if (x + 1) % CLUSTER_SIZE == 0 && x + 1 < self.width && self.at(x + 1, y) {
                    let ca = self.cluster_of(x, y);
                    let cb = self.cluster_of(x + 1, y);
                    self.portals.push(Portal {
                        a: TilePos::new(x as i16, y as i16),
                        b: TilePos::new((x + 1) as i16, y as i16),
                        cluster_a: ca,
                        cluster_b: cb,
                    });
                }
                // Vertical boundary.
                if (y + 1) % CLUSTER_SIZE == 0 && y + 1 < self.height && self.at(x, y + 1) {
                    let ca = self.cluster_of(x, y);
                    let cb = self.cluster_of(x, y + 1);
                    self.portals.push(Portal {
                        a: TilePos::new(x as i16, y as i16),
                        b: TilePos::new(x as i16, (y + 1) as i16),
                        cluster_a: ca,
                        cluster_b: cb,
                    });
                }
            }
        }
        for (index, portal) in self.portals.iter().enumerate() {
            self.cluster_portals[portal.cluster_a].push(index);
            if portal.cluster_b != portal.cluster_a {
                self.cluster_portals[portal.cluster_b].push(index);
            }
        }
    }

    fn build_inner_edges(&mut self) {
        let cluster_count = self.cluster_portals.len();
        let mut edges = vec![Vec::new(); cluster_count];
        #[allow(clippy::needless_range_loop)] // `self` methods borrow while `edges` is built
        for cluster in 0..cluster_count {
            let portals = self.cluster_portals[cluster].clone();
            for &pa in &portals {
                for &pb in &portals {
                    if pa == pb {
                        continue;
                    }
                    let from = self.portal_local_tile(cluster, pa);
                    let to = self.portal_local_tile(cluster, pb);
                    let dist = self.cluster_astar(cluster, from, to);
                    if let Some(dist) = dist {
                        edges[cluster].push((pa, pb, dist));
                    }
                }
            }
        }
        self.inner_edges = edges;
    }

    /// Shortest in-cluster distance between two tiles (`inner_astar`).
    fn cluster_astar(&self, cluster: usize, from: TilePos, to: TilePos) -> Option<f32> {
        if from == to {
            return Some(0.0);
        }
        let width = self.width;
        let start = from.x() as usize + from.y() as usize * width as usize;
        let end = to.x() as usize + to.y() as usize * width as usize;
        let passable = |node: usize| {
            (node as i32) >= 0
                && node < self.passable.len()
                && self.passable[node]
                && self.cluster_of_node(node) == cluster
        };
        let mut scratch = AstarScratch::new();
        let path = astar::pathfind(
            self.width,
            self.height,
            start,
            end,
            &UniformCost(1.0),
            astar::manhattan,
            &passable,
            &mut scratch,
        );
        if path.is_empty() {
            None
        } else {
            Some(path.len() as f32)
        }
    }

    /// The endpoint of `portal` that lies in `cluster`.
    fn portal_local_tile(&self, cluster: usize, portal: usize) -> TilePos {
        let portal = self.portals[portal];
        if portal.cluster_a == cluster {
            portal.a
        } else {
            portal.b
        }
    }

    /// `ControlPathfinder.getPathPosition`-equivalent request.
    pub fn get_path_position(&mut self, from: TilePos, to: TilePos) -> PathfindResult {
        if !self.in_bounds(from)
            || !self.in_bounds(to)
            || !self.at(from.x() as i32, from.y() as i32)
            || !self.at(to.x() as i32, to.y() as i32)
        {
            return PathfindResult {
                unreachable: true,
                ..Default::default()
            };
        }
        let width = self.width;
        let start = from.x() as usize + from.y() as usize * width as usize;
        let end = to.x() as usize + to.y() as usize * width as usize;
        if start == end {
            return PathfindResult::default();
        }
        let path = if let Some(cached) = self.cache.get(&(start, end)) {
            cached.clone()
        } else {
            let passable = |node: usize| self.passable.get(node).copied().unwrap_or(false);
            let path = astar::pathfind(
                self.width,
                self.height,
                start,
                end,
                &UniformCost(1.0),
                astar::manhattan,
                &passable,
                &mut self.scratch,
            );
            self.updates = self.updates.wrapping_add(1);
            self.cache.insert((start, end), path.clone());
            path
        };
        if path.is_empty() {
            return PathfindResult {
                unreachable: true,
                ..Default::default()
            };
        }
        let tiles: Vec<TilePos> = path
            .iter()
            .map(|node| {
                TilePos::new(
                    ((*node as i32) % width) as i16,
                    ((*node as i32) / width) as i16,
                )
            })
            .collect();
        PathfindResult {
            unreachable: false,
            next: tiles.first().copied(),
            path: tiles,
        }
    }

    /// Whether `(x, y)` is passable.
    pub fn at(&self, x: i32, y: i32) -> bool {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return false;
        }
        self.passable
            .get((x + y * self.width) as usize)
            .copied()
            .unwrap_or(false)
    }

    /// Whether `(x, y)` is solid (`!passable`).
    pub fn solid(&self, x: i32, y: i32) -> bool {
        !self.at(x, y)
    }

    /// Whether any 4-neighbour of `(x, y)` is passable (`nearPassable`).
    pub fn near_passable(&self, x: i32, y: i32) -> bool {
        [(1, 0), (-1, 0), (0, 1), (0, -1)]
            .iter()
            .any(|(dx, dy)| self.at(x + dx, y + dy))
    }

    /// Bresenham tile raycast (`raycast`), returning visited tiles step by step.
    pub fn raycast(&self, x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<TilePos> {
        let mut out = Vec::new();
        let (dx, dy) = ((x1 - x0).abs(), (y1 - y0).abs());
        let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
        let mut err = dx - dy;
        let (mut x, mut y) = (x0, y0);
        loop {
            out.push(TilePos::new(x as i16, y as i16));
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 > -dy {
                err -= dy;
                x += sx;
            }
            if e2 < dx {
                err += dx;
                y += sy;
            }
        }
        out
    }

    /// Raycast clamped to the first solid tile (`raycastFastAvoid`).
    pub fn raycast_fast_avoid(&self, x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<TilePos> {
        let mut out = self.raycast(x0, y0, x1, y1);
        if let Some(index) = out
            .iter()
            .position(|tile| self.solid(tile.x() as i32, tile.y() as i32))
        {
            out.truncate(index + 1);
        }
        out
    }

    fn cluster_of(&self, x: i32, y: i32) -> usize {
        ((y / CLUSTER_SIZE) * self.cluster_w + (x / CLUSTER_SIZE)) as usize
    }

    fn cluster_of_node(&self, node: usize) -> usize {
        let x = (node as i32) % self.width;
        let y = (node as i32) / self.width;
        self.cluster_of(x, y)
    }

    fn in_bounds(&self, pos: TilePos) -> bool {
        let x = pos.x() as i32;
        let y = pos.y() as i32;
        x >= 0 && y >= 0 && x < self.width && y < self.height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(width: i32, height: i32) -> ControlPathfinder {
        let content = crate::content::create_base_content(
            &crate::content::MemoryBundle::new(),
            &crate::content::MemoryUnlockStore::new(),
            true,
        )
        .expect("content");
        let mut grid = WorldGrid::new(width, height);
        grid.fill(crate::content::BlockId::AIR, crate::content::BlockId::AIR);
        let mut path = ControlPathfinder::new(width, height);
        path.build(&grid, &content, 0);
        path
    }

    #[test]
    fn portals_shared_between_clusters() {
        let path = flat(48, 24);
        assert!(!path.portals.is_empty());
        for (index, portal) in path.portals.iter().enumerate() {
            assert!(path.cluster_portals[portal.cluster_a].contains(&index));
            assert!(path.cluster_portals[portal.cluster_b].contains(&index));
        }
    }

    #[test]
    fn inner_edges_symmetric() {
        let path = flat(48, 24);
        // Find a cluster with at least two portals.
        let Some((cluster, portals)) = path
            .cluster_portals
            .iter()
            .enumerate()
            .find(|(_, ports)| ports.len() >= 2)
        else {
            panic!("expected a cluster with portals");
        };
        let (pa, pb) = (portals[0], portals[1]);
        let ab = path.inner_edges[cluster]
            .iter()
            .find(|(a, b, _)| *a == pa && *b == pb)
            .map(|(_, _, d)| *d);
        let ba = path.inner_edges[cluster]
            .iter()
            .find(|(a, b, _)| *a == pb && *b == pa)
            .map(|(_, _, d)| *d);
        assert_eq!(ab, ba, "inner edge distances are symmetric");
    }

    #[test]
    fn request_uses_cache() {
        let mut path = flat(32, 32);
        let a = path.get_path_position(TilePos::new(1, 1), TilePos::new(30, 30));
        assert!(!a.unreachable);
        assert!(a.next.is_some());
        let updates = path.updates;
        let b = path.get_path_position(TilePos::new(1, 1), TilePos::new(30, 30));
        assert_eq!(a.path, b.path);
        assert_eq!(updates, path.updates, "second request hits the cache");
    }

    #[test]
    fn unreachable_reported() {
        // Wall off the right half with a full vertical column.
        let content = crate::content::create_base_content(
            &crate::content::MemoryBundle::new(),
            &crate::content::MemoryUnlockStore::new(),
            true,
        )
        .expect("content");
        let wall = content.block_id("copper-wall").expect("copper-wall");
        let mut grid = WorldGrid::new(16, 16);
        grid.fill(crate::content::BlockId::AIR, crate::content::BlockId::AIR);
        for y in 0..16 {
            grid.tiles.get_mut(8, y).block = wall;
        }
        let mut path = ControlPathfinder::new(16, 16);
        path.build(&grid, &content, 0);
        let result = path.get_path_position(TilePos::new(1, 1), TilePos::new(15, 15));
        assert!(result.unreachable);
    }

    #[test]
    fn raycast_stops_at_first_solid() {
        let content = crate::content::create_base_content(
            &crate::content::MemoryBundle::new(),
            &crate::content::MemoryUnlockStore::new(),
            true,
        )
        .expect("content");
        let wall = content.block_id("copper-wall").expect("copper-wall");
        let mut grid = WorldGrid::new(16, 16);
        grid.fill(crate::content::BlockId::AIR, crate::content::BlockId::AIR);
        grid.tiles.get_mut(5, 5).block = wall;
        let mut path = ControlPathfinder::new(16, 16);
        path.build(&grid, &content, 0);
        let hit = path.raycast_fast_avoid(0, 5, 10, 5);
        assert_eq!(hit.last().copied(), Some(TilePos::new(5, 5)));
    }
}
