// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `ControlPathfinder` (plan 11 §3.7/§4.2).
//!
//! Ported from `core/src/mindustry/ai/ControlPathfinder.java`. This module owns
//! the cluster/portal build, the shared-boundary portal graph, per-cluster inner
//! edges, raycasts and the per-request result API
//! ([`ControlPathfinder::get_path_position`]).
//!
//! **Per-cluster `FieldCache` acceleration (implemented).** [`get_path_position`]
//! is served from a per-goal [`FieldCache`]: the goal's `12×12` weight array per
//! cluster (packed `cluster index -> weights` like upstream `FieldCache.fields`)
//! plus the frontier, keyed by the goal tile (`FieldIndex` collapses to the goal
//! here because each instance is single-team/single-cost-id; the runtime keeps
//! one instance per [`ControlCost`], mirroring Java's per-`pathCost` cluster
//! and field cache). The field is expanded
//! by the deterministic bounded BFS in [`ControlPathfinder::compute_field`]
//! (deviation 1: fixed work, no wall clock); the result path is reconstructed by
//! descending the distance field. [`get_path_position_astar`] keeps the previous
//! synchronous A* as the parity baseline, and
//! `tests::cached_field_matches_astar` asserts the two are byte-identical.
//!
//! **Plan-23 perf surface.** `mind-headless units bench --profile path` times the
//! field build + request path (`units_scenarios::bench_path`); the §7d path
//! budget is recorded from that run.

use std::collections::{BTreeMap, VecDeque};

use crate::content::ContentRegistry;
use crate::world::{TilePos, WorldGrid};

use super::astar::{self, AstarScratch};
use super::pathfinder::path_tile::{self, PathTile};

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

/// `Geometry.d4` order (`(1,0), (0,1), (-1,0), (0,-1)`), shared with [`astar`].
const D4: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// Uninitialized/impassable flow-field weight (`i32::MAX`, upstream `IntMap` `0`
/// in a fresh array is mapped to this instead so `0` can mean the goal).
const FIELD_UNINIT: i32 = i32::MAX;

/// Per-goal cached flow field (`ControlPathfinder.FieldCache`).
///
/// `fields` maps a cluster index to its `12×12` row-major weight array
/// (distance-to-goal; [`FIELD_UNINIT`] = impassable or outside the field). The
/// field is single-team and single-cost-id per [`ControlPathfinder`] instance
/// (the `FieldIndex` key reduces to the goal tile). `frontier`
/// is the BFS queue and `complete` records whether the expansion drained.
#[derive(Debug, Clone, Default)]
pub struct FieldCache {
    /// Goal tile (`x + y * width`).
    pub goal: usize,
    /// `cluster index -> weights` (`clusterSize * clusterSize` entries).
    pub fields: BTreeMap<usize, Vec<i32>>,
    /// BFS frontier (tile indices).
    pub frontier: VecDeque<usize>,
    /// Whether the expansion ran to completion.
    pub complete: bool,
}

/// Cluster/portal A* pathfinder (plan 11 §3.7).
#[derive(Debug, Default)]
pub struct ControlPathfinder {
    /// Grid width in tiles.
    pub width: i32,
    /// Grid height in tiles.
    pub height: i32,
    /// The pathing team the passability was built for
    /// (`ControlPathfinder.team`; the ground cost is team-relative).
    pub team: u8,
    /// The `ControlPathfinder.costTypes` entry the weights were built with
    /// (Java keys clusters/fields by `pathCost`; one instance per cost here).
    pub cost: ControlCost,
    /// Cluster columns (`ceil(width / CLUSTER_SIZE)`).
    pub cluster_w: i32,
    /// Cluster rows.
    pub cluster_h: i32,
    /// Per-tile ground passability.
    pub passable: Vec<bool>,
    /// Per-tile `ControlPathfinder.costGround` weight (`0` = impassable); the
    /// A*/field step cost (`ControlPathfinder.java:33-42`, `tileCost`/`tcost`).
    pub weights: Vec<i32>,
    /// All boundary portals.
    pub portals: Vec<Portal>,
    /// `cluster index -> portal indices` (shared between the two clusters).
    pub cluster_portals: Vec<Vec<usize>>,
    /// `cluster index -> (portal_a, portal_b, distance)` inner edges.
    pub inner_edges: Vec<Vec<(usize, usize, f32)>>,
    /// Per-goal flow fields (`ControlPathfinder.fields`, keyed by goal tile).
    pub fields: BTreeMap<usize, FieldCache>,
    /// A* scratch for the parity baseline [`Self::get_path_position_astar`]
    /// (deviation 6: per-call scratch, owned here).
    scratch: AstarScratch,
    /// Monotonic update counter (determinism/debug).
    pub updates: u64,
}

/// `ControlPathfinder.wallImpassableCap` (ControlPathfinder.java:25).
pub const WALL_IMPASSABLE_CAP: i32 = 1_000_000;

/// `ControlPathfinder.solidCap` (ControlPathfinder.java:26): a cost at or above
/// this is solid for the control pathfinder (`passable`/`solid`,
/// ControlPathfinder.java:1453-1470).
pub const SOLID_CAP: i32 = 7000;

/// `ControlPathfinder.costTypes` ids (`costIdGround`..`costIdNaval`,
/// ControlPathfinder.java:69-80). Indices are ABI (`FieldIndex.costId`).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ControlCost {
    /// `costIdGround` (0).
    #[default]
    Ground = 0,
    /// `costIdHover` (1).
    Hover = 1,
    /// `costIdLegs` (2).
    Legs = 2,
    /// `costIdNaval` (3).
    Naval = 3,
}

/// Number of control cost types (`ControlPathfinder.costTypes.size`).
pub const CONTROL_COST_COUNT: usize = 4;

impl ControlCost {
    /// Every cost in id order.
    pub const ALL: [ControlCost; CONTROL_COST_COUNT] = [
        ControlCost::Ground,
        ControlCost::Hover,
        ControlCost::Legs,
        ControlCost::Naval,
    ];

    /// ABI id (`pathCostId`).
    pub const fn id(self) -> u8 {
        self as u8
    }

    /// `ControlPathfinder.idToCost`; unknown ids fall back to ground like
    /// `UnitType.initPathType` (`if(pathCostId == -1) pathCostId = 0`).
    pub fn from_id(id: u8) -> ControlCost {
        match id {
            1 => ControlCost::Hover,
            2 => ControlCost::Legs,
            3 => ControlCost::Naval,
            _ => ControlCost::Ground,
        }
    }

    /// `UnitType.initPathType` (UnitType.java:901-910): `naval ? costNaval :
    /// allowLegStep ? costLegs : hovering ? costHover : costGround`.
    pub fn for_unit(naval: bool, allow_leg_step: bool, hovering: bool) -> ControlCost {
        if naval {
            ControlCost::Naval
        } else if allow_leg_step {
            ControlCost::Legs
        } else if hovering {
            ControlCost::Hover
        } else {
            ControlCost::Ground
        }
    }

    /// `PathCost.getCost(team, tile)` for this control cost
    /// (ControlPathfinder.java:31-67). `IMPASSABLE` (-1) marks a blocked tile.
    pub fn cost(self, team: u8, tile: PathTile) -> i32 {
        match self {
            ControlCost::Ground => control_ground_cost(team, tile),
            ControlCost::Hover => control_hover_cost(team, tile),
            ControlCost::Legs => control_legs_cost(team, tile),
            ControlCost::Naval => control_naval_cost(team, tile),
        }
    }

    /// The A*/field step weight of `tile` (`0` = not passable):
    /// `ControlPathfinder.passable` (ControlPathfinder.java:1453-1456) is
    /// `cost != impassable && cost < solidCap`.
    pub fn weight(self, team: u8, tile: PathTile) -> i32 {
        let cost = self.cost(team, tile);
        if cost != crate::ai::pathfinder::cost::IMPASSABLE && cost < SOLID_CAP {
            cost
        } else {
            0
        }
    }
}

/// `solid && ((team == t && !teamPassable) || t == 0)`: the same-team or
/// neutral solid block every control cost but legs treats as `impassable`.
fn own_or_neutral_solid(team: u8, tile: PathTile) -> bool {
    tile.solid() && ((tile.team() == team && !tile.team_passable()) || tile.team() == 0)
}

/// `(t != team && t != 0) && solid ? wallImpassableCap : 0`: the synthetic
/// enemy-block term.
fn enemy_wall_cost(team: u8, tile: PathTile) -> i32 {
    if tile.team() != team && tile.team() != 0 && tile.solid() {
        WALL_IMPASSABLE_CAP
    } else {
        0
    }
}

/// `ControlPathfinder.costGround` (ControlPathfinder.java:33-42): the control
/// pathfinder has its OWN ground cost. A same-team or neutral solid block (and
/// all-deep water) is `impassable`; a synthetic enemy block costs
/// `wallImpassableCap`, i.e. it blocks routing (this differs from the AI
/// flowfield's `Pathfinder.costGround`, where enemy walls are passable at
/// `health * 5` so ground units shoot their way through).
pub fn control_ground_cost(team: u8, tile: PathTile) -> i32 {
    if tile.all_deep() || own_or_neutral_solid(team, tile) {
        return crate::ai::pathfinder::cost::IMPASSABLE;
    }
    enemy_wall_cost(team, tile)
        + 1
        + if tile.near_solid() { 6 } else { 0 }
        + if tile.near_liquid() { 8 } else { 0 }
        + if tile.deep() { 6000 } else { 0 }
        + if tile.damages() { 50 } else { 0 }
}

/// `ControlPathfinder.costHover` (ControlPathfinder.java:46-53): the ground
/// cost without the liquid/deep/damage terms.
pub fn control_hover_cost(team: u8, tile: PathTile) -> i32 {
    if own_or_neutral_solid(team, tile) {
        return crate::ai::pathfinder::cost::IMPASSABLE;
    }
    enemy_wall_cost(team, tile) + 1 + if tile.near_solid() { 6 } else { 0 }
}

/// `ControlPathfinder.costLegs` (ControlPathfinder.java:55-59):
/// `legSolid ? impassable : 1 + nearDeep 8 + deep 6000 + nearLegSolid 3`.
/// Team-independent: legged units walk over every non-`legSolid` block.
pub fn control_legs_cost(_team: u8, tile: PathTile) -> i32 {
    if tile.leg_solid() {
        return crate::ai::pathfinder::cost::IMPASSABLE;
    }
    1 + if tile.near_deep() { 8 } else { 0 }
        + if tile.deep() { 6000 } else { 0 }
        + if tile.near_leg_solid() { 3 } else { 0 }
}

/// `ControlPathfinder.costNaval` (ControlPathfinder.java:61-67): an own-team
/// or neutral solid block is `impassable`; land costs `6000` (still below
/// `solidCap`, so it is passable but strongly avoided), water `1`; plus the
/// enemy-wall term and `nearGround || nearSolid ? 6`.
pub fn control_naval_cost(team: u8, tile: PathTile) -> i32 {
    if own_or_neutral_solid(team, tile) {
        return crate::ai::pathfinder::cost::IMPASSABLE;
    }
    (if !tile.liquid() { 6000 } else { 1 })
        + enemy_wall_cost(team, tile)
        + if tile.near_ground() || tile.near_solid() {
            6
        } else {
            0
        }
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

    /// Creates an empty pathfinder whose weights use the control `cost`
    /// (`ControlPathfinder.costTypes.get(pathCostId)`).
    pub fn with_cost(width: i32, height: i32, cost: ControlCost) -> Self {
        Self {
            width,
            height,
            cost,
            ..Self::default()
        }
    }

    /// Rebuilds passability, clusters and portals from the world (`WorldLoadEvent`).
    pub fn build(
        &mut self,
        grid: &WorldGrid,
        content: &ContentRegistry,
        world: &bevy_ecs::world::World,
        team: u8,
        core_capture: bool,
    ) {
        self.width = grid.width();
        self.height = grid.height();
        self.team = team;
        let tiles = path_tile::build_tiles(grid, content, world, core_capture);
        let cost = self.cost;
        self.weights = tiles.iter().map(|tile| cost.weight(team, *tile)).collect();
        self.passable = self.weights.iter().map(|weight| *weight > 0).collect();
        self.cluster_w = (self.width + CLUSTER_SIZE - 1).max(0) / CLUSTER_SIZE;
        self.cluster_h = (self.height + CLUSTER_SIZE - 1).max(0) / CLUSTER_SIZE;
        let cluster_count = (self.cluster_w * self.cluster_h).max(0) as usize;
        self.cluster_portals = vec![Vec::new(); cluster_count];
        self.inner_edges = vec![Vec::new(); cluster_count];
        self.portals.clear();
        self.fields.clear();
        self.build_portals();
        self.build_inner_edges();
    }

    /// Lightweight rebuild for the per-request field path only: refreshes the
    /// ground passability from packed [`PathTile`]s and drops every cached goal
    /// field, but skips the portal graph and the per-cluster inner-edge A*
    /// ([`Self::build`] spends ~1 s there on a 256² map; the
    /// [`Self::get_path_position`] field BFS never reads portals or edges).
    /// Used by the live `CommandAI` movement (PAR-0140).
    pub fn rebuild_passability(&mut self, width: i32, height: i32, tiles: &[PathTile], team: u8) {
        self.width = width;
        self.height = height;
        self.team = team;
        let cost = self.cost;
        self.weights = tiles.iter().map(|tile| cost.weight(team, *tile)).collect();
        self.passable = self.weights.iter().map(|weight| *weight > 0).collect();
        self.cluster_w = (self.width + CLUSTER_SIZE - 1).max(0) / CLUSTER_SIZE;
        self.cluster_h = (self.height + CLUSTER_SIZE - 1).max(0) / CLUSTER_SIZE;
        let cluster_count = (self.cluster_w * self.cluster_h).max(0) as usize;
        self.cluster_portals = vec![Vec::new(); cluster_count];
        self.inner_edges = vec![Vec::new(); cluster_count];
        self.portals.clear();
        self.fields.clear();
        self.updates = self.updates.wrapping_add(1);
    }

    /// Whether every tile on the Bresenham line `from -> to` is passable
    /// (`ControlPathfinder.raycast` for the ground cost).
    /// Line of sight between two tiles (`ControlPathfinder` smoothing): every
    /// tile along the Bresenham ray must be passable, and a diagonal step
    /// requires **both** orthogonal tiles passable — the same "diagonal corner
    /// trap" rule as `Pathfinder.getTargetTile` (Pathfinder.java:426-429).
    /// Without it the smoothing picks a waypoint that requires cutting a solid
    /// corner, and the T4 `EntityCollisions` box catches the corner instead of
    /// clipping through it.
    pub fn line_clear(&self, from: TilePos, to: TilePos) -> bool {
        let ray = self.raycast(
            from.x() as i32,
            from.y() as i32,
            to.x() as i32,
            to.y() as i32,
        );
        for (index, tile) in ray.iter().enumerate() {
            if !self.at(tile.x() as i32, tile.y() as i32) {
                return false;
            }
            let Some(next) = ray.get(index + 1) else {
                break;
            };
            let dx = (next.x() - tile.x()) as i32;
            let dy = (next.y() - tile.y()) as i32;
            if dx != 0
                && dy != 0
                && (!self.at(tile.x() as i32 + dx, tile.y() as i32)
                    || !self.at(tile.x() as i32, tile.y() as i32 + dy))
            {
                return false;
            }
        }
        true
    }

    /// Whether a unit box (`hitboxTile`: `min(hitSize * 0.66, 7.8)` wide,
    /// `HitboxComp.java:69-76`) can travel the straight segment
    /// `(from_x, from_y) -> (to_x, to_y)` without clipping a solid tile
    /// (`EntityCollisions.moveDelta` uses the tile rect
    /// `[x*8 - 4, x*8 + 4)`, `EntityCollisions.java:84`). The tile-centre
    /// visibility check above ignores where the unit actually stands inside
    /// its tile, which jams a unit hugging a wall end on a corner the box
    /// cannot cut (T4, PAR-0202 corner jam). Sampled every 2 px.
    pub fn segment_clear(&self, from_x: f32, from_y: f32, to_x: f32, to_y: f32, half: f32) -> bool {
        let ts = crate::config::TILESIZE as f32;
        let (dx, dy) = (to_x - from_x, to_y - from_y);
        let dist = (dx * dx + dy * dy).sqrt();
        let steps = (dist / 2.0).ceil().max(1.0) as i32;
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let x = from_x + dx * t;
            let y = from_y + dy * t;
            let (x0, x1) = (WorldGrid::to_tile(x - half), WorldGrid::to_tile(x + half));
            let (y0, y1) = (WorldGrid::to_tile(y - half), WorldGrid::to_tile(y + half));
            for tx in x0..=x1 {
                for ty in y0..=y1 {
                    if self.at(tx, ty) {
                        continue;
                    }
                    // Strict tile-rect overlap (`Rect.overlaps`).
                    let (cx, cy) = (tx as f32 * ts, ty as f32 * ts);
                    if x - half < cx + ts / 2.0
                        && x + half > cx - ts / 2.0
                        && y - half < cy + ts / 2.0
                        && y + half > cy - ts / 2.0
                    {
                        return false;
                    }
                }
            }
        }
        true
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

    /// Shortest in-cluster distance between two tiles (`innerAstar`).
    ///
    /// Java's `innerAstar` steps at `tileCost(team, cost, current, next)` = the
    /// DESTINATION tile's `costGround` weight (ControlPathfinder.java:637-645);
    /// with every weight `>= 1` the returned length is the weighted distance.
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
        let weights = &self.weights;
        let step_cost = |_from: usize, to: usize| weights.get(to).copied().unwrap_or(0) as f32;
        let mut scratch = AstarScratch::new();
        let path = astar::pathfind(
            self.width,
            self.height,
            start,
            end,
            &step_cost,
            astar::manhattan,
            &passable,
            &mut scratch,
        );
        if path.is_empty() {
            None
        } else {
            // `innerAstar` returns the accumulated cost (`costs.get(current)`),
            // not the step count.
            Some(
                path.iter()
                    .map(|node| weights.get(*node).copied().unwrap_or(0) as f32)
                    .sum(),
            )
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

    /// Weight of `(x, y)` in `cache` (`FIELD_UNINIT` when the tile has no
    /// registered cluster weights, mirroring upstream `getCost`'s `0`).
    fn field_cost(&self, cache: &FieldCache, x: i32, y: i32) -> i32 {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return FIELD_UNINIT;
        }
        let cluster = self.cluster_of(x, y);
        let index =
            (x % CLUSTER_SIZE) as usize + (y % CLUSTER_SIZE) as usize * CLUSTER_SIZE as usize;
        cache
            .fields
            .get(&cluster)
            .and_then(|weights| weights.get(index).copied())
            .unwrap_or(FIELD_UNINIT)
    }

    /// Registers an empty weight array for a cluster (`addFlowCluster`).
    fn add_flow_cluster(cache: &mut FieldCache, cluster: usize, cluster_count: usize) {
        if cluster < cluster_count {
            cache
                .fields
                .entry(cluster)
                .or_insert_with(|| vec![FIELD_UNINIT; (CLUSTER_SIZE * CLUSTER_SIZE) as usize]);
        }
    }

    /// Expands `cache` from its goal across the passable grid until the frontier
    /// drains (`updateFields`, deviation 1: fixed work, no wall clock).
    fn compute_field(&self, goal: usize) -> FieldCache {
        let width = self.width;
        let cluster_count = (self.cluster_w * self.cluster_h).max(0) as usize;
        let mut cache = FieldCache {
            goal,
            ..Default::default()
        };
        // Seed the goal's cluster and set the goal weight to 0.
        let gx = (goal as i32) % width;
        let gy = (goal as i32) / width;
        Self::add_flow_cluster(&mut cache, self.cluster_of(gx, gy), cluster_count);
        {
            let cluster = self.cluster_of(gx, gy);
            let index =
                (gx % CLUSTER_SIZE) as usize + (gy % CLUSTER_SIZE) as usize * CLUSTER_SIZE as usize;
            if let Some(weights) = cache.fields.get_mut(&cluster) {
                weights[index] = 0;
            }
        }
        cache.frontier.push_back(goal);
        // Weighted expansion (Java's `FieldCache` fields are `costGround`
        // distances, ControlPathfinder.java:33-42): Dijkstra over the tile
        // weights. The frontier is a min-heap keyed by accumulated cost; a
        // stale entry (already-relaxed tile) is skipped.
        let mut heap: std::collections::BinaryHeap<std::cmp::Reverse<(i32, usize)>> =
            std::collections::BinaryHeap::new();
        heap.push(std::cmp::Reverse((0, goal)));
        while let Some(std::cmp::Reverse((cost, tile))) = heap.pop() {
            let x = (tile as i32) % width;
            let y = (tile as i32) / width;
            let cluster = self.cluster_of(x, y);
            let index =
                (x % CLUSTER_SIZE) as usize + (y % CLUSTER_SIZE) as usize * CLUSTER_SIZE as usize;
            let Some(weights) = cache.fields.get(&cluster) else {
                continue;
            };
            match weights.get(index).copied() {
                Some(existing) if existing < cost => continue,
                None => continue,
                _ => {}
            }
            let base = self.field_cost(&cache, x, y);
            if base == FIELD_UNINIT {
                continue;
            }
            for (dx, dy) in D4 {
                let nx = x + dx;
                let ny = y + dy;
                if nx < 0 || ny < 0 || nx >= self.width || ny >= self.height {
                    continue;
                }
                let next = (nx + ny * width) as usize;
                if !self.passable.get(next).copied().unwrap_or(false) {
                    continue;
                }
                let step = self.weights.get(next).copied().unwrap_or(0);
                if step <= 0 {
                    continue;
                }
                let candidate = base.saturating_add(step);
                let cluster = self.cluster_of(nx, ny);
                Self::add_flow_cluster(&mut cache, cluster, cluster_count);
                let index = (nx % CLUSTER_SIZE) as usize
                    + (ny % CLUSTER_SIZE) as usize * CLUSTER_SIZE as usize;
                if let Some(weights) = cache.fields.get_mut(&cluster)
                    && (weights[index] == FIELD_UNINIT || candidate < weights[index])
                {
                    weights[index] = candidate;
                    heap.push(std::cmp::Reverse((candidate, next)));
                }
            }
        }
        cache.complete = true;
        cache
    }

    /// Reconstructs the field's shortest path into a [`PathfindResult`] (`start`
    /// exclusive, `end` inclusive). Shared by the cached and uncached entries so
    /// caching can never change the result.
    fn path_from_field(&self, cache: &FieldCache, from: TilePos, to: TilePos) -> PathfindResult {
        let width = self.width;
        let start = from.x() as usize + from.y() as usize * width as usize;
        let end = to.x() as usize + to.y() as usize * width as usize;
        if self.field_cost(cache, from.x() as i32, from.y() as i32) == FIELD_UNINIT {
            return PathfindResult {
                unreachable: true,
                ..Default::default()
            };
        }
        // Descend the flow field: from each tile step to the passable neighbour
        // with the smallest weight (`costGround` distance to the goal). `dist`
        // strictly decreases, so the walk is acyclic and must reach the goal.
        // The tie-break is fixed so the cached and uncached entry points agree
        // byte-for-byte.
        let mut path: Vec<usize> = Vec::new();
        let mut current = start;
        while current != end {
            let x = (current as i32) % width;
            let y = (current as i32) / width;
            let here = self.field_cost(cache, x, y);
            let mut next: Option<(usize, i32)> = None;
            for (dx, dy) in D4 {
                let nx = x + dx;
                let ny = y + dy;
                if nx < 0 || ny < 0 || nx >= self.width || ny >= self.height {
                    continue;
                }
                let candidate = (nx + ny * width) as usize;
                if !self.passable.get(candidate).copied().unwrap_or(false) {
                    continue;
                }
                let cost = self.field_cost(cache, nx, ny);
                if cost == FIELD_UNINIT || cost >= here {
                    continue;
                }
                next = Some(match next {
                    Some((existing, existing_cost))
                        if existing_cost < cost
                            || (existing_cost == cost && existing < candidate) =>
                    {
                        (existing, existing_cost)
                    }
                    _ => (candidate, cost),
                });
            }
            let Some((node, _)) = next else {
                // Reachable but the field has no descending neighbour; only
                // possible if the field and passability disagree (defensive).
                return PathfindResult {
                    unreachable: true,
                    ..Default::default()
                };
            };
            path.push(node);
            current = node;
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

    /// Field-cached `ControlPathfinder.getPathPosition`-equivalent request.
    ///
    /// The goal's per-cluster flow field is built once (bounded BFS) and shared
    /// by every request. Results are byte-identical to
    /// [`Self::get_path_position_uncached`] (see `tests::cached_field_matches_uncached`),
    /// which recomputes the field without touching the cache.
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
        let end = to.x() as usize + to.y() as usize * self.width as usize;
        if from == to {
            return PathfindResult::default();
        }
        if !self.fields.contains_key(&end) {
            let cache = self.compute_field(end);
            self.fields.insert(end, cache);
            self.updates = self.updates.wrapping_add(1);
        }
        match self.fields.get(&end) {
            Some(cache) => self.path_from_field(cache, from, to),
            None => PathfindResult {
                unreachable: true,
                ..Default::default()
            },
        }
    }

    /// Uncached request: recomputes the goal's flow field every call without
    /// storing it. Exists only to prove the cache is result-neutral.
    pub fn get_path_position_uncached(&self, from: TilePos, to: TilePos) -> PathfindResult {
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
        if from == to {
            return PathfindResult::default();
        }
        let end = to.x() as usize + to.y() as usize * self.width as usize;
        let cache = self.compute_field(end);
        self.path_from_field(&cache, from, to)
    }

    /// The previous synchronous A* request path (parity baseline).
    pub fn get_path_position_astar(&mut self, from: TilePos, to: TilePos) -> PathfindResult {
        self.astar_result(from, to)
    }

    fn astar_result(&mut self, from: TilePos, to: TilePos) -> PathfindResult {
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
        let passable = |node: usize| self.passable.get(node).copied().unwrap_or(false);
        let weights = &self.weights;
        let step_cost = |_from: usize, to: usize| weights.get(to).copied().unwrap_or(0) as f32;
        let path = astar::pathfind(
            self.width,
            self.height,
            start,
            end,
            &step_cost,
            astar::manhattan,
            &passable,
            &mut self.scratch,
        );
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
        path.build(&grid, &content, &bevy_ecs::world::World::new(), 0, false);
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
        path.build(&grid, &content, &bevy_ecs::world::World::new(), 0, false);
        let result = path.get_path_position(TilePos::new(1, 1), TilePos::new(15, 15));
        assert!(result.unreachable);
    }

    fn obstacle(width: i32, height: i32, seed: u64) -> ControlPathfinder {
        let content = crate::content::create_base_content(
            &crate::content::MemoryBundle::new(),
            &crate::content::MemoryUnlockStore::new(),
            true,
        )
        .expect("content");
        let wall = content.block_id("copper-wall").expect("copper-wall");
        let mut grid = WorldGrid::new(width, height);
        grid.fill(crate::content::BlockId::AIR, crate::content::BlockId::AIR);
        let mut state = seed | 1;
        for y in 0..height {
            for x in 0..width {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                // ~22% walls, keeping the map connected around the border.
                let border = x == 0 || y == 0 || x == width - 1 || y == height - 1;
                if !border && (state >> 33) % 100 < 22 {
                    grid.tiles.get_mut(x, y).block = wall;
                }
            }
        }
        let mut path = ControlPathfinder::new(width, height);
        path.build(&grid, &content, &bevy_ecs::world::World::new(), 0, false);
        path
    }

    #[test]
    fn cached_field_matches_uncached() {
        for seed in [1u64, 7, 42, 1234] {
            let mut path = obstacle(18, 18, seed);
            for y in 0..18i16 {
                for x in 0..18i16 {
                    let from = TilePos::new(x, y);
                    if !path.at(x as i32, y as i32) {
                        continue;
                    }
                    // Sample goals across the map.
                    let to = TilePos::new((x * 7 + 3) % 18, (y * 5 + 1) % 18);
                    let cached = path.get_path_position(from, to);
                    let uncached = path.get_path_position_uncached(from, to);
                    assert_eq!(
                        cached, uncached,
                        "cached vs uncached mismatch seed={seed} {from:?}->{to:?}"
                    );
                    // The field path is a shortest path: same reachability and
                    // same total `costGround` weight as A* (the two may pick
                    // different equal-cost routes, so step counts can differ).
                    let astar = path.get_path_position_astar(from, to);
                    assert_eq!(cached.unreachable, astar.unreachable);
                    if !cached.unreachable {
                        assert_eq!(cached.path.last(), astar.path.last());
                        let cost = |route: &[TilePos]| -> i64 {
                            route
                                .iter()
                                .map(|pos| {
                                    path.weights
                                        [pos.x() as usize + pos.y() as usize * path.width as usize]
                                        as i64
                                })
                                .sum()
                        };
                        assert_eq!(
                            cost(&cached.path),
                            cost(&astar.path),
                            "field and A* disagree on the optimal cost seed={seed} {from:?}->{to:?}"
                        );
                    }
                }
            }
        }
    }

    /// I01 T4: the control path uses the weighted `costGround` (deep water is
    /// `+6000`, ControlPathfinder.java:33-42), so a route around the deep water
    /// beats the shorter straight line through it.
    #[test]
    fn control_path_prefers_shallow_over_deep() {
        let content = crate::content::create_base_content(
            &crate::content::MemoryBundle::new(),
            &crate::content::MemoryUnlockStore::new(),
            true,
        )
        .expect("content");
        let deep = content.block_id("deep-water").expect("deep-water");
        let mut grid = WorldGrid::new(24, 16);
        grid.fill(crate::content::BlockId::AIR, crate::content::BlockId::AIR);
        // A deep-water column x = 12, y = 0..12; only the bottom rows are dry.
        for y in 0..13 {
            grid.tiles.get_mut(12, y).floor = deep;
        }
        let mut path = ControlPathfinder::new(24, 16);
        path.build(&grid, &content, &bevy_ecs::world::World::new(), 0, false);
        let from = TilePos::new(6, 6);
        let to = TilePos::new(18, 6);
        let deep_at = |pos: &TilePos| grid.tiles.get(pos.x() as i32, pos.y() as i32).floor == deep;
        let result = path.get_path_position(from, to);
        assert!(!result.unreachable);
        assert!(
            result.path.iter().all(|pos| !deep_at(pos)),
            "the cached field detours around deep water: {:?}",
            result.path
        );
        let astar = path.get_path_position_astar(from, to);
        assert!(!astar.unreachable);
        assert!(
            astar.path.iter().all(|pos| !deep_at(pos)),
            "the A* baseline detours around deep water: {:?}",
            astar.path
        );
        // The straight line between the two tiles crosses the deep column.
        assert!(deep_at(&TilePos::new(12, 6)));
    }

    /// `PathTile.get` argument order (Pathfinder.java:267-282) with only the
    /// named flags set.
    #[allow(clippy::too_many_arguments)]
    fn tile(
        team: u8,
        solid: bool,
        liquid: bool,
        leg_solid: bool,
        near_ground: bool,
        near_solid: bool,
        near_leg_solid: bool,
        deep: bool,
        near_deep: bool,
        team_passable: bool,
    ) -> PathTile {
        PathTile::from_bits(
            0,
            team,
            solid,
            liquid,
            leg_solid,
            false,
            near_ground,
            near_solid,
            near_leg_solid,
            deep,
            false,
            false,
            near_deep,
            team_passable,
        )
    }

    /// I-round audit (I01 T4 follow-up): `ControlPathfinder.costLegs`,
    /// `costNaval` and `costHover` (ControlPathfinder.java:46-67) term by term.
    #[test]
    fn control_legs_naval_hover_costs_match_java() {
        use crate::ai::pathfinder::cost::IMPASSABLE;
        let f = false;
        let plain = tile(0, f, f, f, f, f, f, f, f, f);
        // costLegs: `legSolid ? impassable : 1 + nearDeep 8 + deep 6000 +
        // nearLegSolid 3`; solid blocks of any team are walkable.
        assert_eq!(ControlCost::Legs.cost(1, plain), 1);
        assert_eq!(
            ControlCost::Legs.cost(1, tile(0, f, f, f, f, f, f, f, true, f)),
            9
        );
        assert_eq!(
            ControlCost::Legs.cost(1, tile(0, f, true, f, f, f, f, true, true, f)),
            6009
        );
        assert_eq!(
            ControlCost::Legs.cost(1, tile(0, f, f, f, f, f, true, f, f, f)),
            4
        );
        assert_eq!(
            ControlCost::Legs.cost(1, tile(0, f, f, true, f, f, f, f, f, f)),
            IMPASSABLE
        );
        let neutral_wall = tile(0, true, f, f, f, f, f, f, f, f);
        let own_wall = tile(1, true, f, f, f, f, f, f, f, f);
        let enemy_wall = tile(2, true, f, f, f, f, f, f, f, f);
        assert_eq!(ControlCost::Legs.cost(1, neutral_wall), 1);
        assert_eq!(ControlCost::Ground.cost(1, neutral_wall), IMPASSABLE);

        // costNaval: own/neutral solid impassable; land 6000 vs water 1; the
        // enemy-wall cap; `nearGround || nearSolid ? 6`.
        assert_eq!(ControlCost::Naval.cost(1, plain), 6000);
        assert_eq!(
            ControlCost::Naval.weight(1, plain),
            6000,
            "land is passable"
        );
        let water = tile(0, f, true, f, f, f, f, true, true, f);
        assert_eq!(ControlCost::Naval.cost(1, water), 1);
        assert_eq!(
            ControlCost::Naval.cost(1, tile(0, f, true, f, true, f, f, f, f, f)),
            7
        );
        assert_eq!(
            ControlCost::Naval.cost(1, tile(0, f, true, f, f, true, f, f, f, f)),
            7
        );
        assert_eq!(ControlCost::Naval.cost(1, own_wall), IMPASSABLE);
        assert_eq!(ControlCost::Naval.cost(1, neutral_wall), IMPASSABLE);
        assert_eq!(
            ControlCost::Naval.cost(1, enemy_wall),
            6000 + WALL_IMPASSABLE_CAP
        );
        assert_eq!(ControlCost::Naval.weight(1, enemy_wall), 0);
        // A team-passable own block does not block.
        let own_passable = tile(1, true, f, f, f, f, f, f, f, true);
        assert_eq!(ControlCost::Naval.cost(1, own_passable), 6000);

        // costHover: ground without the liquid/deep/damage terms.
        let all_deep =
            PathTile::from_bits(0, 0, f, true, f, true, f, f, f, true, true, true, true, f);
        assert_eq!(ControlCost::Ground.cost(1, all_deep), IMPASSABLE);
        assert_eq!(ControlCost::Hover.cost(1, all_deep), 1);
        assert_eq!(
            ControlCost::Hover.cost(1, tile(0, f, f, f, f, true, f, f, f, f)),
            7
        );
        assert_eq!(ControlCost::Hover.cost(1, own_wall), IMPASSABLE);
        assert_eq!(ControlCost::Hover.cost(1, own_passable), 1);
        assert_eq!(ControlCost::Hover.weight(1, enemy_wall), 0);
    }

    /// `UnitType.initPathType` (UnitType.java:901-910) priority and the frozen
    /// `costId*` ids.
    #[test]
    fn control_cost_ids_and_unit_selection() {
        assert_eq!(ControlCost::Ground.id(), 0);
        assert_eq!(ControlCost::Hover.id(), 1);
        assert_eq!(ControlCost::Legs.id(), 2);
        assert_eq!(ControlCost::Naval.id(), 3);
        assert_eq!(ControlCost::from_id(7), ControlCost::Ground);
        let mut content = crate::content::create_base_content(
            &crate::content::MemoryBundle::new(),
            &crate::content::MemoryUnlockStore::new(),
            true,
        )
        .expect("content");
        // `UnitType.init()` derives `allowLegStep`/`naval` (UnitType.java:915-936).
        content.init().expect("init");
        let cost_of = |name: &str| {
            let id = content.unit_id(name).expect("unit id");
            let def = content.unit(id).expect("unit def");
            ControlCost::for_unit(def.naval, def.allow_leg_step, def.hovering)
        };
        assert_eq!(cost_of("dagger"), ControlCost::Ground);
        // Legged units also hover; `allowLegStep` wins.
        assert_eq!(cost_of("atrax"), ControlCost::Legs);
        assert_eq!(cost_of("risso"), ControlCost::Naval);
        assert_eq!(cost_of("elude"), ControlCost::Hover);
    }

    /// A legged unit's control path crosses a neutral wall column that blocks
    /// ground routing (`costLegs` only refuses `legSolid`).
    #[test]
    fn legs_control_path_crosses_walls() {
        let f = false;
        let (width, height) = (16, 16);
        // A neutral (team 0) solid column at x = 8 that is not `legSolid`, and
        // a `legSolid` column at x = 12 with a gap at y = 15.
        let tiles: Vec<PathTile> = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .map(|(x, y)| match x {
                8 => tile(0, true, f, f, f, f, f, f, f, f),
                12 if y < 15 => tile(0, true, f, true, f, f, f, f, f, f),
                _ => tile(0, f, f, f, f, f, f, f, f, f),
            })
            .collect();
        let mut ground = ControlPathfinder::with_cost(width, height, ControlCost::Ground);
        ground.rebuild_passability(width, height, &tiles, 1);
        let mut legs = ControlPathfinder::with_cost(width, height, ControlCost::Legs);
        legs.rebuild_passability(width, height, &tiles, 1);
        let (from, to) = (TilePos::new(2, 6), TilePos::new(14, 6));
        assert!(ground.get_path_position(from, to).unreachable);
        let result = legs.get_path_position(from, to);
        assert!(!result.unreachable);
        assert!(result.path.iter().any(|pos| pos.x() == 8));
        assert!(
            result.path.iter().all(|pos| pos.x() != 12 || pos.y() == 15),
            "legSolid tiles are impassable: {:?}",
            result.path
        );
        assert_eq!(result.path.last().copied(), Some(to));
    }

    /// A naval control path keeps to water and detours around a land strip
    /// (land costs `6000`, ControlPathfinder.java:61-67), and land itself
    /// stays passable below `solidCap`.
    #[test]
    fn naval_control_path_prefers_water() {
        let content = crate::content::create_base_content(
            &crate::content::MemoryBundle::new(),
            &crate::content::MemoryUnlockStore::new(),
            true,
        )
        .expect("content");
        let deep = content.block_id("deep-water").expect("deep-water");
        let stone = content.block_id("stone").expect("stone");
        let mut grid = WorldGrid::new(24, 16);
        grid.fill(deep, crate::content::BlockId::AIR);
        // A land column x = 12, y = 0..12; the top rows stay water.
        for y in 0..13 {
            grid.tiles.get_mut(12, y).floor = stone;
        }
        let mut naval = ControlPathfinder::with_cost(24, 16, ControlCost::Naval);
        naval.build(&grid, &content, &bevy_ecs::world::World::new(), 1, false);
        assert!(naval.at(12, 6), "land is passable for the control path");
        let (from, to) = (TilePos::new(6, 6), TilePos::new(18, 6));
        let land_at = |pos: &TilePos| grid.tiles.get(pos.x() as i32, pos.y() as i32).floor == stone;
        let result = naval.get_path_position(from, to);
        assert!(!result.unreachable);
        assert!(
            result.path.iter().all(|pos| !land_at(pos)),
            "the naval field detours through water: {:?}",
            result.path
        );
        // Ground cannot cross the all-deep water at all.
        let mut ground = ControlPathfinder::with_cost(24, 16, ControlCost::Ground);
        ground.build(&grid, &content, &bevy_ecs::world::World::new(), 1, false);
        assert!(!ground.at(6, 6));
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
        path.build(&grid, &content, &bevy_ecs::world::World::new(), 0, false);
        let hit = path.raycast_fast_avoid(0, 5, 10, 5);
        assert_eq!(hit.last().copied(), Some(TilePos::new(5, 5)));
    }
}
