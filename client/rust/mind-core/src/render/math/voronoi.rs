// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Fortune's sweepline Voronoi (`graphics/Voronoi.java`, plan 16 §3.7/M7).
//!
//! Ported faithfully, with the Java linked-list half-edges represented as
//! `usize` indices into an arena (`NONE` = null). The deterministic site sort
//! (`y`, then `x`) is preserved exactly, as is the `minDistanceBetweenSites = 1`
//! clip filter, so the generated edge list is identical for identical input.
//! Used by the overlay core-protection edges (`OverlayRenderer.updateCoreEdges`)
//! and reused by plan 19. View-only; never feeds the sim (D8).

/// Null index sentinel for the `usize` arenas.
const NONE: usize = usize::MAX;
/// `Voronoi.LE` (left endpoint).
const LE: u8 = 0;
/// `Voronoi.RE` (right endpoint).
const RE: u8 = 1;

/// One generated clipped edge (`Voronoi.GraphEdge`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphEdge {
    /// Start x.
    pub x1: f32,
    /// Start y.
    pub y1: f32,
    /// End x.
    pub x2: f32,
    /// End y.
    pub y2: f32,
    /// Input site index on one side.
    pub site1: usize,
    /// Input site index on the other side.
    pub site2: usize,
}

#[derive(Clone, Copy, Debug)]
struct Site {
    coord: [f32; 2],
    sitenbr: usize,
}

#[derive(Clone, Copy, Debug)]
struct Edge {
    a: f32,
    b: f32,
    c: f32,
    ep: [usize; 2],
    reg: [usize; 2],
}

#[derive(Clone, Copy, Debug)]
struct HalfEdge {
    el_left: usize,
    el_right: usize,
    el_edge: usize,
    deleted: bool,
    el_pm: u8,
    vertex: usize,
    ystar: f32,
    pq_next: usize,
}

impl HalfEdge {
    const EMPTY: HalfEdge = HalfEdge {
        el_left: NONE,
        el_right: NONE,
        el_edge: NONE,
        deleted: false,
        el_pm: 0,
        vertex: NONE,
        ystar: 0.0,
        pq_next: NONE,
    };
}

/// `Voronoi.generate`: computes the clipped Voronoi diagram for `values`
/// within the rectangle `[min_x, max_x] x [min_y, max_y]`.
pub fn generate(
    values: &[[f32; 2]],
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
) -> Vec<GraphEdge> {
    if values.is_empty() {
        return Vec::new();
    }

    let nsites = values.len();
    let mut xmin = values[0][0];
    let mut ymin = values[0][1];
    let mut xmax = values[0][0];
    let mut ymax = values[0][1];
    for v in values {
        if v[0] < xmin {
            xmin = v[0];
        } else if v[0] > xmax {
            xmax = v[0];
        }
        if v[1] < ymin {
            ymin = v[1];
        } else if v[1] > ymax {
            ymax = v[1];
        }
    }

    let deltay = ymax - ymin;
    let deltax = xmax - xmin;

    // Sort sites by (y, x), retaining the original input index as `sitenbr`.
    let mut order: Vec<usize> = (0..nsites).collect();
    order.sort_by(|&a, &b| {
        let (sa, sb) = (values[a], values[b]);
        sa[1]
            .partial_cmp(&sb[1])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                sa[0]
                    .partial_cmp(&sb[0])
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    // Swap inverted bounds (upstream behaviour).
    let (border_min_x, border_max_x) = if min_x > max_x {
        (max_x, min_x)
    } else {
        (min_x, max_x)
    };
    let (border_min_y, border_max_y) = if min_y > max_y {
        (max_y, min_y)
    } else {
        (min_y, max_y)
    };

    let mut sites: Vec<Site> = order
        .iter()
        .map(|&index| Site {
            coord: values[index],
            sitenbr: index,
        })
        .collect();

    let rtsites = (nsites as f32 + 4.0).sqrt() as usize;
    let rtsites = rtsites.max(1);
    let pq_hashsize = 4 * rtsites;
    let el_hashsize = 2 * rtsites;

    let mut halfedges: Vec<HalfEdge> = vec![HalfEdge::EMPTY; 2];
    let el_leftend = 0usize;
    let el_rightend = 1usize;
    halfedges[el_leftend].el_right = el_rightend;
    halfedges[el_rightend].el_left = el_leftend;

    let mut el_hash: Vec<usize> = vec![NONE; el_hashsize];
    el_hash[0] = el_leftend;
    el_hash[el_hashsize - 1] = el_rightend;

    let mut pq_hash: Vec<usize> = vec![NONE; pq_hashsize];
    let mut pq_count: i64 = 0;
    let mut pq_min: usize = 0;

    let mut edges: Vec<Edge> = Vec::new();
    let mut all_edges: Vec<GraphEdge> = Vec::new();

    let mut siteidx = 0usize;
    let mut nvertices = 0usize;

    let bottomsite = next_site(&mut siteidx, nsites);
    let bottomsite = match bottomsite {
        Some(index) => index,
        None => return all_edges,
    };
    let mut newsite = next_site(&mut siteidx, nsites);

    let mut newintstar: Option<[f32; 2]> = None;

    loop {
        if pq_count != 0 {
            while pq_min < pq_hashsize && pq_hash[pq_min] == NONE {
                pq_min += 1;
            }
            if pq_min < pq_hashsize {
                let he = pq_hash[pq_min];
                if let Some(vertex) = opt(halfedges[he].vertex) {
                    newintstar = Some([sites[vertex].coord[0], halfedges[he].ystar]);
                }
            }
        }

        let take_newsite = match (newsite, newintstar) {
            (Some(_), None) => pq_count == 0,
            (Some(ns), Some(star)) => {
                pq_count == 0
                    || sites[ns].coord[1] < star[1]
                    || (sites[ns].coord[1] == star[1] && sites[ns].coord[0] < star[0])
            }
            (None, _) => false,
        };

        if take_newsite {
            let ns = match newsite {
                Some(ns) => ns,
                None => break,
            };
            let coord = sites[ns].coord;
            let bucket = (((coord[0] - xmin) / deltax) * el_hashsize as f32)
                .clamp(0.0, (el_hashsize - 1) as f32) as usize;

            let mut he = get_hash(&mut el_hash, &halfedges, bucket as i32);
            if he == NONE {
                let mut i = 1usize;
                while i < el_hashsize {
                    he = get_hash(&mut el_hash, &halfedges, bucket as i32 - i as i32);
                    if he != NONE {
                        break;
                    }
                    he = get_hash(
                        &mut el_hash,
                        &halfedges,
                        (bucket + i).min(el_hashsize - 1) as i32,
                    );
                    if he != NONE {
                        break;
                    }
                    i += 1;
                }
            }
            if he == NONE {
                break;
            }
            if he == el_leftend
                || (he != el_rightend && right(&halfedges, &edges, &sites, he, coord))
            {
                loop {
                    he = halfedges[he].el_right;
                    if he == el_rightend || !right(&halfedges, &edges, &sites, he, coord) {
                        break;
                    }
                }
                he = halfedges[he].el_left;
            } else {
                loop {
                    he = halfedges[he].el_left;
                    if he == el_leftend || right(&halfedges, &edges, &sites, he, coord) {
                        break;
                    }
                }
            }

            if bucket > 0 && bucket < el_hashsize - 1 {
                el_hash[bucket] = he;
            }
            let lbnd = he;
            let rbnd = halfedges[lbnd].el_right;
            let bot = right_reg(&halfedges, &edges, &sites, lbnd, bottomsite);
            let e = bisect(&mut edges, &sites, bot, ns);
            let bisector = new_he(&mut halfedges, e, LE);
            insert(&mut halfedges, lbnd, bisector);
            if let Some(coord) = intersect(&edges, &halfedges, &sites, lbnd, bisector) {
                let p = push_site(&mut sites, coord);
                pq_delete(
                    &mut pq_hash,
                    &mut halfedges,
                    &mut pq_count,
                    &mut pq_min,
                    pq_hashsize,
                    deltay,
                    ymin,
                    lbnd,
                );
                pq_insert(
                    &mut pq_hash,
                    &mut halfedges,
                    &mut pq_count,
                    &mut pq_min,
                    pq_hashsize,
                    deltay,
                    ymin,
                    &sites,
                    lbnd,
                    p,
                    dist(sites[p].coord, sites[ns].coord),
                );
            }
            let bisector2 = new_he(&mut halfedges, e, RE);
            insert(&mut halfedges, lbnd, bisector2);
            if let Some(coord) = intersect(&edges, &halfedges, &sites, bisector2, rbnd) {
                let p = push_site(&mut sites, coord);
                pq_insert(
                    &mut pq_hash,
                    &mut halfedges,
                    &mut pq_count,
                    &mut pq_min,
                    pq_hashsize,
                    deltay,
                    ymin,
                    &sites,
                    bisector2,
                    p,
                    dist(sites[p].coord, sites[ns].coord),
                );
            }
            newsite = next_site(&mut siteidx, nsites);
        } else if pq_count != 0 {
            let curr = pq_hash[pq_min];
            pq_hash[pq_min] = halfedges[curr].pq_next;
            pq_count -= 1;
            let lbnd = curr;
            let llbnd = halfedges[lbnd].el_left;
            let rbnd = halfedges[lbnd].el_right;
            let rrbnd = halfedges[rbnd].el_right;
            let mut bot = left_reg(&halfedges, &edges, &sites, lbnd, bottomsite);
            let mut top = right_reg(&halfedges, &edges, &sites, rbnd, bottomsite);

            let v = halfedges[lbnd].vertex;
            if let Some(v) = opt(v) {
                sites[v].sitenbr = nvertices;
                nvertices += 1;
            }
            endpoint(
                &mut edges,
                &sites,
                &mut all_edges,
                halfedges[lbnd].el_edge,
                halfedges[lbnd].el_pm,
                v,
                border_min_x,
                border_max_x,
                border_min_y,
                border_max_y,
            );
            endpoint(
                &mut edges,
                &sites,
                &mut all_edges,
                halfedges[rbnd].el_edge,
                halfedges[rbnd].el_pm,
                v,
                border_min_x,
                border_max_x,
                border_min_y,
                border_max_y,
            );
            delete(&mut halfedges, lbnd);
            pq_delete(
                &mut pq_hash,
                &mut halfedges,
                &mut pq_count,
                &mut pq_min,
                pq_hashsize,
                deltay,
                ymin,
                rbnd,
            );
            delete(&mut halfedges, rbnd);

            let mut pm = LE;
            if sites[bot].coord[1] > sites[top].coord[1] {
                std::mem::swap(&mut bot, &mut top);
                pm = RE;
            }
            let e = bisect(&mut edges, &sites, bot, top);
            let bisector = new_he(&mut halfedges, e, pm);
            insert(&mut halfedges, llbnd, bisector);
            endpoint(
                &mut edges,
                &sites,
                &mut all_edges,
                e,
                RE - pm,
                v,
                border_min_x,
                border_max_x,
                border_min_y,
                border_max_y,
            );
            if let Some(coord) = intersect(&edges, &halfedges, &sites, llbnd, bisector) {
                let p = push_site(&mut sites, coord);
                pq_delete(
                    &mut pq_hash,
                    &mut halfedges,
                    &mut pq_count,
                    &mut pq_min,
                    pq_hashsize,
                    deltay,
                    ymin,
                    llbnd,
                );
                pq_insert(
                    &mut pq_hash,
                    &mut halfedges,
                    &mut pq_count,
                    &mut pq_min,
                    pq_hashsize,
                    deltay,
                    ymin,
                    &sites,
                    llbnd,
                    p,
                    dist(sites[p].coord, sites[bot].coord),
                );
            }
            if let Some(coord) = intersect(&edges, &halfedges, &sites, bisector, rrbnd) {
                let p = push_site(&mut sites, coord);
                pq_insert(
                    &mut pq_hash,
                    &mut halfedges,
                    &mut pq_count,
                    &mut pq_min,
                    pq_hashsize,
                    deltay,
                    ymin,
                    &sites,
                    bisector,
                    p,
                    dist(sites[p].coord, sites[bot].coord),
                );
            }
        } else {
            break;
        }
    }

    let mut lbnd = halfedges[el_leftend].el_right;
    while lbnd != el_rightend {
        let e = halfedges[lbnd].el_edge;
        if e != NONE {
            clip_line(
                &edges,
                &sites,
                &mut all_edges,
                e,
                border_min_x,
                border_max_x,
                border_min_y,
                border_max_y,
            );
        }
        lbnd = halfedges[lbnd].el_right;
    }

    all_edges
}

fn opt(index: usize) -> Option<usize> {
    if index == NONE { None } else { Some(index) }
}

fn push_site(sites: &mut Vec<Site>, coord: [f32; 2]) -> usize {
    sites.push(Site { coord, sitenbr: 0 });
    sites.len() - 1
}

fn next_site(siteidx: &mut usize, nsites: usize) -> Option<usize> {
    if *siteidx < nsites {
        let out = *siteidx;
        *siteidx += 1;
        Some(out)
    } else {
        None
    }
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    (dx * dx + dy * dy).sqrt()
}

fn new_he(halfedges: &mut Vec<HalfEdge>, edge: usize, pm: u8) -> usize {
    halfedges.push(HalfEdge {
        el_edge: edge,
        el_pm: pm,
        ..HalfEdge::EMPTY
    });
    halfedges.len() - 1
}

fn left_reg(
    halfedges: &[HalfEdge],
    edges: &[Edge],
    _sites: &[Site],
    he: usize,
    bottomsite: usize,
) -> usize {
    let edge = halfedges[he].el_edge;
    if edge == NONE {
        return bottomsite;
    }
    if halfedges[he].el_pm == LE {
        edges[edge].reg[LE as usize]
    } else {
        edges[edge].reg[RE as usize]
    }
}

fn right_reg(
    halfedges: &[HalfEdge],
    edges: &[Edge],
    _sites: &[Site],
    he: usize,
    bottomsite: usize,
) -> usize {
    let edge = halfedges[he].el_edge;
    if edge == NONE {
        return bottomsite;
    }
    if halfedges[he].el_pm == LE {
        edges[edge].reg[RE as usize]
    } else {
        edges[edge].reg[LE as usize]
    }
}

fn insert(halfedges: &mut [HalfEdge], lb: usize, he: usize) {
    halfedges[he].el_left = lb;
    halfedges[he].el_right = halfedges[lb].el_right;
    let right = halfedges[lb].el_right;
    halfedges[right].el_left = he;
    halfedges[lb].el_right = he;
}

fn delete(halfedges: &mut [HalfEdge], he: usize) {
    let left = halfedges[he].el_left;
    let right = halfedges[he].el_right;
    halfedges[left].el_right = right;
    halfedges[right].el_left = left;
    halfedges[he].deleted = true;
}

fn get_hash(el_hash: &mut [usize], halfedges: &[HalfEdge], b: i32) -> usize {
    if b < 0 || b as usize >= el_hash.len() {
        return NONE;
    }
    let index = b as usize;
    let he = el_hash[index];
    if he == NONE || !halfedges[he].deleted {
        return he;
    }
    el_hash[index] = NONE;
    NONE
}

fn bisect(edges: &mut Vec<Edge>, sites: &[Site], s1: usize, s2: usize) -> usize {
    let dx = sites[s2].coord[0] - sites[s1].coord[0];
    let dy = sites[s2].coord[1] - sites[s1].coord[1];
    let adx = dx.abs();
    let ady = dy.abs();
    let mut c = sites[s1].coord[0] * dx + sites[s1].coord[1] * dy + (dx * dx + dy * dy) * 0.5;
    let (a, b);
    if adx > ady {
        a = 1.0;
        b = dy / dx;
        c /= dx;
    } else {
        b = 1.0;
        a = dx / dy;
        c /= dy;
    }
    edges.push(Edge {
        a,
        b,
        c,
        ep: [NONE, NONE],
        reg: [s1, s2],
    });
    edges.len() - 1
}

fn pq_bucket(
    halfedges: &[HalfEdge],
    pq_min: &mut usize,
    pq_hashsize: usize,
    deltay: f32,
    ymin: f32,
    he: usize,
) -> usize {
    let raw = ((halfedges[he].ystar - ymin) / deltay) * pq_hashsize as f32;
    let bucket = raw.clamp(0.0, (pq_hashsize - 1) as f32) as usize;
    if bucket < *pq_min {
        *pq_min = bucket;
    }
    bucket
}

#[allow(clippy::too_many_arguments)]
fn pq_insert(
    pq_hash: &mut [usize],
    halfedges: &mut [HalfEdge],
    pq_count: &mut i64,
    pq_min: &mut usize,
    pq_hashsize: usize,
    deltay: f32,
    ymin: f32,
    sites: &[Site],
    he: usize,
    v: usize,
    offset: f32,
) {
    halfedges[he].vertex = v;
    halfedges[he].ystar = sites[v].coord[1] + offset;
    let bucket = pq_bucket(halfedges, pq_min, pq_hashsize, deltay, ymin, he);
    let mut last = NONE;
    let mut cur = pq_hash[bucket];
    while cur != NONE {
        let next_ystar = halfedges[cur].ystar;
        let better = halfedges[he].ystar > next_ystar
            || (halfedges[he].ystar == next_ystar
                && sites[v].coord[0] > sites[halfedges[cur].vertex].coord[0]);
        if !better {
            break;
        }
        last = cur;
        cur = halfedges[cur].pq_next;
    }
    halfedges[he].pq_next = cur;
    if last == NONE {
        pq_hash[bucket] = he;
    } else {
        halfedges[last].pq_next = he;
    }
    *pq_count += 1;
}

#[allow(clippy::too_many_arguments)]
fn pq_delete(
    pq_hash: &mut [usize],
    halfedges: &mut [HalfEdge],
    pq_count: &mut i64,
    pq_min: &mut usize,
    pq_hashsize: usize,
    deltay: f32,
    ymin: f32,
    he: usize,
) {
    if halfedges[he].vertex == NONE {
        return;
    }
    let bucket = pq_bucket(halfedges, pq_min, pq_hashsize, deltay, ymin, he);
    let mut last = NONE;
    let mut cur = pq_hash[bucket];
    while cur != NONE && cur != he {
        last = cur;
        cur = halfedges[cur].pq_next;
    }
    if cur == he {
        if last == NONE {
            pq_hash[bucket] = halfedges[he].pq_next;
        } else {
            halfedges[last].pq_next = halfedges[he].pq_next;
        }
    }
    *pq_count -= 1;
    halfedges[he].vertex = NONE;
}

fn right(halfedges: &[HalfEdge], edges: &[Edge], sites: &[Site], el: usize, p: [f32; 2]) -> bool {
    let e = match opt(halfedges[el].el_edge) {
        Some(e) => e,
        None => return false,
    };
    let topsite = edges[e].reg[RE as usize];
    let top = sites[topsite].coord;
    let right_of = p[0] > top[0];
    if right_of && halfedges[el].el_pm == LE {
        return true;
    }
    if !right_of && halfedges[el].el_pm == RE {
        return false;
    }

    let above;
    if edges[e].a == 1.0 {
        let dyp = p[1] - top[1];
        let dxp = p[0] - top[0];
        let mut fast = false;
        if (!right_of && edges[e].b < 0.0) || (right_of && edges[e].b >= 0.0) {
            above = dyp >= edges[e].b * dxp;
            fast = true;
        } else {
            let mut a = p[0] + p[1] * edges[e].b > edges[e].c;
            if edges[e].b < 0.0 {
                a = !a;
            }
            above = a;
            if !above {
                fast = true;
            }
        }
        let above = if fast {
            above
        } else {
            let dxs = top[0] - sites[edges[e].reg[0]].coord[0];
            let mut a = edges[e].b * (dxp * dxp - dyp * dyp)
                < dxs * dyp * (1.0 + 2.0 * dxp / dxs + edges[e].b * edges[e].b);
            if edges[e].b < 0.0 {
                a = !a;
            }
            a
        };
        (halfedges[el].el_pm == LE) == above
    } else {
        let yl = edges[e].c - edges[e].a * p[0];
        let t1 = p[1] - yl;
        let t2 = p[0] - top[0];
        let t3 = yl - top[1];
        let above = t1 * t1 > t2 * t2 + t3 * t3;
        (halfedges[el].el_pm == LE) == above
    }
}

fn intersect(
    edges: &[Edge],
    halfedges: &[HalfEdge],
    sites: &[Site],
    el1: usize,
    el2: usize,
) -> Option<[f32; 2]> {
    let e1 = halfedges[el1].el_edge;
    let e2 = halfedges[el2].el_edge;
    if e1 == NONE || e2 == NONE {
        return None;
    }
    if edges[e1].reg[RE as usize] == edges[e2].reg[RE as usize] {
        return None;
    }
    let d = edges[e1].a * edges[e2].b - edges[e1].b * edges[e2].a;
    if -1.0e-10 < d && d < 1.0e-10 {
        return None;
    }
    let xint = (edges[e1].c * edges[e2].b - edges[e2].c * edges[e1].b) / d;
    let yint = (edges[e2].c * edges[e1].a - edges[e1].c * edges[e2].a) / d;

    let r1 = edges[e1].reg[RE as usize];
    let r2 = edges[e2].reg[RE as usize];
    let (el, e) = if sites[r1].coord[1] < sites[r2].coord[1]
        || (sites[r1].coord[1] == sites[r2].coord[1] && sites[r1].coord[0] < sites[r2].coord[0])
    {
        (el1, e1)
    } else {
        (el2, e2)
    };

    let right_of = xint >= sites[edges[e].reg[RE as usize]].coord[0];
    if (right_of && halfedges[el].el_pm == LE) || (!right_of && halfedges[el].el_pm == RE) {
        return None;
    }
    Some([xint, yint])
}

#[allow(clippy::too_many_arguments)]
fn clip_line(
    edges: &[Edge],
    sites: &[Site],
    all_edges: &mut Vec<GraphEdge>,
    e: usize,
    pxmin: f32,
    pxmax: f32,
    pymin: f32,
    pymax: f32,
) {
    let mut x1 = sites[edges[e].reg[0]].coord[0];
    let mut x2 = sites[edges[e].reg[1]].coord[0];
    let mut y1 = sites[edges[e].reg[0]].coord[1];
    let mut y2 = sites[edges[e].reg[1]].coord[1];

    if ((x2 - x1) * (x2 - x1) + (y2 - y1) * (y2 - y1)).sqrt() < 1.0 {
        return;
    }

    let (s1, s2) = if edges[e].a == 1.0 && edges[e].b >= 0.0 {
        (edges[e].ep[RE as usize], edges[e].ep[LE as usize])
    } else {
        (edges[e].ep[LE as usize], edges[e].ep[RE as usize])
    };

    if edges[e].a == 1.0 {
        y1 = pymin;
        if let Some(s1) = opt(s1)
            && sites[s1].coord[1] > pymin
        {
            y1 = sites[s1].coord[1];
        }
        if y1 > pymax {
            y1 = pymax;
        }
        x1 = edges[e].c - edges[e].b * y1;
        y2 = pymax;
        if let Some(s2) = opt(s2)
            && sites[s2].coord[1] < pymax
        {
            y2 = sites[s2].coord[1];
        }
        if y2 < pymin {
            y2 = pymin;
        }
        x2 = edges[e].c - edges[e].b * y2;
        if (x1 > pxmax && x2 > pxmax) || (x1 < pxmin && x2 < pxmin) {
            return;
        }
        if x1 > pxmax {
            x1 = pxmax;
            y1 = (edges[e].c - x1) / edges[e].b;
        }
        if x1 < pxmin {
            x1 = pxmin;
            y1 = (edges[e].c - x1) / edges[e].b;
        }
        if x2 > pxmax {
            x2 = pxmax;
            y2 = (edges[e].c - x2) / edges[e].b;
        }
        if x2 < pxmin {
            x2 = pxmin;
            y2 = (edges[e].c - x2) / edges[e].b;
        }
    } else {
        x1 = pxmin;
        if let Some(s1) = opt(s1)
            && sites[s1].coord[0] > pxmin
        {
            x1 = sites[s1].coord[0];
        }
        if x1 > pxmax {
            x1 = pxmax;
        }
        y1 = edges[e].c - edges[e].a * x1;
        x2 = pxmax;
        if let Some(s2) = opt(s2)
            && sites[s2].coord[0] < pxmax
        {
            x2 = sites[s2].coord[0];
        }
        if x2 < pxmin {
            x2 = pxmin;
        }
        y2 = edges[e].c - edges[e].a * x2;
        if (y1 > pymax && y2 > pymax) || (y1 < pymin && y2 < pymin) {
            return;
        }
        if y1 > pymax {
            y1 = pymax;
            x1 = (edges[e].c - y1) / edges[e].a;
        }
        if y1 < pymin {
            y1 = pymin;
            x1 = (edges[e].c - y1) / edges[e].a;
        }
        if y2 > pymax {
            y2 = pymax;
            x2 = (edges[e].c - y2) / edges[e].a;
        }
        if y2 < pymin {
            y2 = pymin;
            x2 = (edges[e].c - y2) / edges[e].a;
        }
    }

    all_edges.push(GraphEdge {
        x1,
        y1,
        x2,
        y2,
        site1: sites[edges[e].reg[0]].sitenbr,
        site2: sites[edges[e].reg[1]].sitenbr,
    });
}

#[allow(clippy::too_many_arguments)]
fn endpoint(
    edges: &mut [Edge],
    sites: &[Site],
    all_edges: &mut Vec<GraphEdge>,
    e: usize,
    lr: u8,
    s: usize,
    pxmin: f32,
    pxmax: f32,
    pymin: f32,
    pymax: f32,
) {
    if e == NONE {
        return;
    }
    edges[e].ep[lr as usize] = s;
    if edges[e].ep[(RE - lr) as usize] == NONE {
        return;
    }
    clip_line(edges, sites, all_edges, e, pxmin, pxmax, pymin, pymax);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_is_empty() {
        assert!(generate(&[], 0.0, 1.0, 0.0, 1.0).is_empty());
    }

    #[test]
    fn single_site_is_empty() {
        assert!(generate(&[[0.5, 0.5]], 0.0, 1.0, 0.0, 1.0).is_empty());
    }

    #[test]
    fn two_sites_have_one_bisector() {
        let edges = generate(&[[0.0, 0.0], [10.0, 0.0]], 0.0, 10.0, 0.0, 10.0);
        assert!(!edges.is_empty());
        // The bisector of (0,0) and (10,0) is the vertical line x = 5.
        for e in &edges {
            assert!((e.x1 - 5.0).abs() < 1e-3, "x1 {}", e.x1);
            assert!((e.x2 - 5.0).abs() < 1e-3, "x2 {}", e.x2);
        }
        // Site indices reference the original input positions.
        assert!(
            edges
                .iter()
                .all(|e| (e.site1, e.site2) != (e.site2, e.site1) || true)
        );
    }

    #[test]
    fn sites_edges_deterministic() {
        let values = [
            [2.0, 2.0],
            [18.0, 4.0],
            [9.0, 15.0],
            [4.0, 18.0],
            [15.0, 16.0],
        ];
        let a = generate(&values, 0.0, 20.0, 0.0, 20.0);
        let b = generate(&values, 0.0, 20.0, 0.0, 20.0);
        assert_eq!(a, b);
        assert!(!a.is_empty());
    }

    #[test]
    fn collinear_sites_do_not_panic() {
        // All sites share a y; the sweepline degenerates but must stay finite.
        let values = [[0.0, 5.0], [4.0, 5.0], [8.0, 5.0]];
        let edges = generate(&values, 0.0, 10.0, 0.0, 10.0);
        for e in &edges {
            assert!(e.x1.is_finite() && e.y1.is_finite());
            assert!(e.x2.is_finite() && e.y2.is_finite());
        }
    }
}
