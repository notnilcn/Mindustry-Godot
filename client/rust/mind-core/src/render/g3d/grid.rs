// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PlanetGrid` icosphere sector grid (`graphics/g3d/PlanetGrid.java`, plan 16
//! §3.1/M7). Exact icosahedron constants and the same `create`/`subdividedGrid`
//! construction, with Java object arrays represented as `Vec`s and index
//! sentinels. Deterministic: identical call order yields identical topology.
//! Consumed by plan 12's sector grid and `mg::g3d`.

/// Null index sentinel for the areans.
const NONE: usize = usize::MAX;

const X: f32 = -0.525_731_1;
const Z: f32 = -0.850_650_8;

/// `PlanetGrid.iTiles`.
const ITILES: [[f32; 3]; 12] = [
    [-X, 0.0, Z],
    [X, 0.0, Z],
    [-X, 0.0, -Z],
    [X, 0.0, -Z],
    [0.0, Z, X],
    [0.0, Z, -X],
    [0.0, -Z, X],
    [0.0, -Z, -X],
    [Z, X, 0.0],
    [-Z, X, 0.0],
    [Z, -X, 0.0],
    [-Z, -X, 0.0],
];

/// `PlanetGrid.iTilesP`.
const ITILES_P: [[usize; 5]; 12] = [
    [9, 4, 1, 6, 11],
    [4, 8, 10, 6, 0],
    [11, 7, 3, 5, 9],
    [2, 7, 10, 8, 5],
    [9, 5, 8, 1, 0],
    [2, 3, 8, 4, 9],
    [0, 1, 10, 7, 11],
    [11, 6, 10, 3, 2],
    [5, 3, 10, 1, 4],
    [2, 5, 4, 0, 11],
    [3, 7, 6, 1, 8],
    [7, 2, 9, 0, 6],
];

/// One icosphere tile (`PlanetGrid.Ptile`).
#[derive(Clone, Debug)]
pub struct Ptile {
    /// Stable id (index in [`PlanetGrid::tiles`]).
    pub id: usize,
    /// Number of edges/corners (`5` for the original 12, else `6`).
    pub edge_count: usize,
    /// Neighbouring tile ids.
    pub tiles: Vec<usize>,
    /// Corner ids (one per edge).
    pub corners: Vec<usize>,
    /// Edge ids (one per edge).
    pub edges: Vec<usize>,
    /// Unit sphere position.
    pub v: [f32; 3],
}

/// One icosphere corner (`PlanetGrid.Corner`).
#[derive(Clone, Debug)]
pub struct Corner {
    /// Stable id (index in [`PlanetGrid::corners`]).
    pub id: usize,
    /// Adjacent tile ids (`[3]`).
    pub tiles: [usize; 3],
    /// Adjacent corner ids (`[3]`).
    pub corners: [usize; 3],
    /// Adjacent edge ids (`[3]`).
    pub edges: [usize; 3],
    /// Unit sphere position.
    pub v: [f32; 3],
}

/// One icosphere edge (`PlanetGrid.Edge`).
#[derive(Clone, Debug)]
pub struct Edge {
    /// Stable id (index in [`PlanetGrid::edges`]).
    pub id: usize,
    /// Adjacent tile ids.
    pub tiles: [usize; 2],
    /// Adjacent corner ids.
    pub corners: [usize; 2],
}

/// The icosphere grid (`PlanetGrid`).
#[derive(Clone, Debug)]
pub struct PlanetGrid {
    /// Subdivision level (`0` = icosahedron).
    pub size: usize,
    /// Tiles.
    pub tiles: Vec<Ptile>,
    /// Corners.
    pub corners: Vec<Corner>,
    /// Edges.
    pub edges: Vec<Edge>,
}

/// `PlanetGrid.tileCount`.
pub fn tile_count(size: usize) -> usize {
    10 * 3usize.pow(size as u32) + 2
}

/// `PlanetGrid.cornerCount`.
pub fn corner_count(size: usize) -> usize {
    20 * 3usize.pow(size as u32)
}

/// `PlanetGrid.edgeCount`.
pub fn edge_count(size: usize) -> usize {
    30 * 3usize.pow(size as u32)
}

impl PlanetGrid {
    fn empty(size: usize) -> Self {
        let tiles = (0..tile_count(size))
            .map(|i| {
                let ec = if i < 12 { 5 } else { 6 };
                Ptile {
                    id: i,
                    edge_count: ec,
                    tiles: vec![NONE; ec],
                    corners: vec![NONE; ec],
                    edges: vec![NONE; ec],
                    v: [0.0; 3],
                }
            })
            .collect();
        let corners = (0..corner_count(size))
            .map(|i| Corner {
                id: i,
                tiles: [NONE; 3],
                corners: [NONE; 3],
                edges: [NONE; 3],
                v: [0.0; 3],
            })
            .collect();
        let edges = (0..edge_count(size))
            .map(|i| Edge {
                id: i,
                tiles: [NONE; 2],
                corners: [NONE; 2],
            })
            .collect();
        Self {
            size,
            tiles,
            corners,
            edges,
        }
    }

    /// `PlanetGrid.create(size)` (uncached; callers cache if needed).
    pub fn create(size: usize) -> Self {
        if size == 0 {
            initial_grid()
        } else {
            subdivided_grid(&Self::create(size - 1))
        }
    }
}

/// `PlanetGrid.initialGrid`.
pub fn initial_grid() -> PlanetGrid {
    let mut grid = PlanetGrid::empty(0);
    for i in 0..grid.tiles.len() {
        grid.tiles[i].v = ITILES[i];
        for (k, &neighbour) in ITILES_P[i].iter().enumerate() {
            grid.tiles[i].tiles[k] = neighbour;
        }
    }
    for i in 0..5 {
        add_corner(i, &mut grid, 0, ITILES_P[0][(i + 4) % 5], ITILES_P[0][i]);
    }
    for i in 0..5 {
        add_corner(
            i + 5,
            &mut grid,
            3,
            ITILES_P[3][(i + 4) % 5],
            ITILES_P[3][i],
        );
    }
    add_corner(10, &mut grid, 10, 1, 8);
    add_corner(11, &mut grid, 1, 10, 6);
    add_corner(12, &mut grid, 6, 10, 7);
    add_corner(13, &mut grid, 6, 7, 11);
    add_corner(14, &mut grid, 11, 7, 2);
    add_corner(15, &mut grid, 11, 2, 9);
    add_corner(16, &mut grid, 9, 2, 5);
    add_corner(17, &mut grid, 9, 5, 4);
    add_corner(18, &mut grid, 4, 5, 8);
    add_corner(19, &mut grid, 4, 8, 1);

    for c in 0..grid.corners.len() {
        for k in 0..3 {
            let t = grid.corners[c].tiles[k];
            let p = pos_tile_corner(&grid.tiles[t], c);
            let next = ((p + 1).rem_euclid(5)) as usize;
            grid.corners[c].corners[k] = grid.tiles[t].corners[next];
        }
    }

    let mut next_edge = 0usize;
    for (t, pattern) in ITILES_P.iter().enumerate() {
        for (k, &neighbour) in pattern.iter().enumerate() {
            if grid.tiles[t].edges[k] == NONE {
                add_edge(next_edge, &mut grid, t, neighbour);
                next_edge += 1;
            }
        }
    }
    grid
}

/// `PlanetGrid.subdividedGrid`.
pub fn subdivided_grid(prev: &PlanetGrid) -> PlanetGrid {
    let mut grid = PlanetGrid::empty(prev.size + 1);
    let prev_tiles = prev.tiles.len();
    let prev_corners = prev.corners.len();

    for i in 0..prev_tiles {
        grid.tiles[i].v = prev.tiles[i].v;
        for k in 0..grid.tiles[i].edge_count {
            grid.tiles[i].tiles[k] = prev.tiles[i].corners[k] + prev_tiles;
        }
    }
    for i in 0..prev_corners {
        grid.tiles[i + prev_tiles].v = prev.corners[i].v;
        for k in 0..3 {
            grid.tiles[i + prev_tiles].tiles[2 * k] = prev.corners[i].corners[k] + prev_tiles;
            grid.tiles[i + prev_tiles].tiles[2 * k + 1] = prev.corners[i].tiles[k];
        }
    }

    let mut next_corner = 0usize;
    for n in 0..prev_tiles {
        let t = n;
        let ec = grid.tiles[t].edge_count;
        for k in 0..ec {
            let t2 = grid.tiles[t].tiles[(k + ec - 1) % ec];
            let t3 = grid.tiles[t].tiles[k];
            add_corner(next_corner, &mut grid, t, t2, t3);
            next_corner += 1;
        }
    }

    for c in 0..grid.corners.len() {
        for k in 0..3 {
            let t = grid.corners[c].tiles[k];
            let ec = grid.tiles[t].edge_count;
            let p = pos_tile_corner(&grid.tiles[t], c);
            let next = (p + 1).rem_euclid(ec as i32) as usize;
            grid.corners[c].corners[k] = grid.tiles[t].corners[next];
        }
    }

    let mut next_edge = 0usize;
    for t in 0..grid.tiles.len() {
        for k in 0..grid.tiles[t].edge_count {
            if grid.tiles[t].edges[k] == NONE {
                let other = grid.tiles[t].tiles[k];
                add_edge(next_edge, &mut grid, t, other);
                next_edge += 1;
            }
        }
    }

    grid
}

fn add_corner(id: usize, grid: &mut PlanetGrid, t1: usize, t2: usize, t3: usize) {
    let ts = [t1, t2, t3];
    let mut sum = [0.0f32; 3];
    for &t in &ts {
        for (i, s) in sum.iter_mut().enumerate() {
            *s += grid.tiles[t].v[i];
        }
    }
    let m = (sum[0] * sum[0] + sum[1] * sum[1] + sum[2] * sum[2]).sqrt();
    let m = if m < 1e-9 { 1.0 } else { m };
    grid.corners[id].v = [sum[0] / m, sum[1] / m, sum[2] / m];
    for i in 0..3 {
        let other = ts[(i + 2) % 3];
        let p = pos_tile_tile(&grid.tiles[ts[i]], other);
        grid.tiles[ts[i]].corners[p as usize] = id;
        grid.corners[id].tiles[i] = ts[i];
    }
}

fn add_edge(id: usize, grid: &mut PlanetGrid, t1: usize, t2: usize) {
    let ts = [t1, t2];
    let p0 = pos_tile_tile(&grid.tiles[ts[0]], ts[1]);
    let ec0 = grid.tiles[ts[0]].edge_count;
    let c0 = grid.tiles[ts[0]].corners[p0 as usize];
    let c1 = grid.tiles[ts[0]].corners[((p0 + 1).rem_euclid(ec0 as i32)) as usize];
    let cs = [c0, c1];
    for i in 0..2 {
        let other = ts[(i + 1) % 2];
        let p = pos_tile_tile(&grid.tiles[ts[i]], other);
        grid.tiles[ts[i]].edges[p as usize] = id;
        grid.edges[id].tiles[i] = ts[i];
        let pc = pos_corner_corner(&grid.corners[cs[i]], cs[(i + 1) % 2]);
        grid.corners[cs[i]].edges[pc as usize] = id;
        grid.edges[id].corners[i] = cs[i];
    }
}

fn pos_tile_tile(t: &Ptile, n: usize) -> i32 {
    for i in 0..t.edge_count {
        if t.tiles[i] == n {
            return i as i32;
        }
    }
    -1
}

fn pos_tile_corner(t: &Ptile, c: usize) -> i32 {
    for i in 0..t.edge_count {
        if t.corners[i] == c {
            return i as i32;
        }
    }
    -1
}

fn pos_corner_corner(c: &Corner, n: usize) -> i32 {
    for i in 0..3 {
        if c.corners[i] == n {
            return i as i32;
        }
    }
    -1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_corner_edge_counts() {
        assert_eq!(tile_count(0), 12);
        assert_eq!(corner_count(0), 20);
        assert_eq!(edge_count(0), 30);
        assert_eq!(tile_count(1), 32);
        assert_eq!(corner_count(1), 60);
        assert_eq!(edge_count(1), 90);
        assert_eq!(tile_count(2), 92);
        assert_eq!(corner_count(2), 180);
        assert_eq!(edge_count(2), 270);

        let g0 = initial_grid();
        assert_eq!(g0.tiles.len(), 12);
        assert_eq!(g0.corners.len(), 20);
        assert_eq!(g0.edges.len(), 30);
    }

    #[test]
    fn subdivide_links() {
        let g1 = PlanetGrid::create(1);
        assert_eq!(g1.size, 1);
        assert_eq!(g1.tiles.len(), 32);
        assert_eq!(g1.corners.len(), 60);
        assert_eq!(g1.edges.len(), 90);
        // Every tile neighbour link is filled.
        for t in &g1.tiles {
            for k in 0..t.edge_count {
                assert_ne!(t.tiles[k], NONE, "tile {} neighbour {}", t.id, k);
                assert_ne!(t.corners[k], NONE, "tile {} corner {}", t.id, k);
                assert_ne!(t.edges[k], NONE, "tile {} edge {}", t.id, k);
            }
        }
        for c in &g1.corners {
            for k in 0..3 {
                assert_ne!(c.tiles[k], NONE);
                assert_ne!(c.corners[k], NONE);
                assert_ne!(c.edges[k], NONE);
            }
        }
        // Positions are unit length.
        for t in &g1.tiles {
            let m = (t.v[0] * t.v[0] + t.v[1] * t.v[1] + t.v[2] * t.v[2]).sqrt();
            assert!((m - 1.0).abs() < 1e-4, "tile {} |v| {}", t.id, m);
        }
    }

    #[test]
    fn create_is_deterministic() {
        let a = PlanetGrid::create(2);
        let b = PlanetGrid::create(2);
        assert_eq!(a.tiles.len(), b.tiles.len());
        for (ta, tb) in a.tiles.iter().zip(&b.tiles) {
            assert_eq!(ta.v, tb.v);
            assert_eq!(ta.tiles, tb.tiles);
        }
    }
}
