// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `FloorRenderer` — per-`(chunk, CacheLayer)` `ArrayMesh` floor bake
//! (plan 16 §3.5).
//!
//! One `ArrayMesh`/`MeshInstance2D` per `(chunk, CacheLayer, atlas page)`,
//! parented to the matching append-only floor band node, exactly the
//! `Renderer.draw` stage-12 pass. The deterministic dirty/used/epoch math lives
//! in `mind_core::render::floor_cache`; this module owns only the Godot mesh
//! build, the plan-03 atlas binding, the `error`-region fallback and
//! `growSprites` padding.
//!
//! Deviations recorded in the plan Changelog: the 47-slice autotiling of plan
//! 02/03/06 is not applied here (the M1 slice draws the floor base + overlay
//! base), and floors/walls draw at `TILESIZE` / `size * TILESIZE` matching the
//! `render-list` oracle quads rather than upstream's natural region size.

use std::collections::{BTreeMap, HashMap};

use smallvec::SmallVec;

use godot::classes::mesh::PrimitiveType;
use godot::classes::{ArrayMesh, Mesh, MeshInstance2D, Node2D, SurfaceTool, Texture2D};
use godot::obj::NewGd;
use godot::prelude::*;

use mind_core::config::TILESIZE;
use mind_core::content::registries::blocks::BlockDef;
use mind_core::content::{BlockId, BlockKind, ContentRegistry, is_overlay_floor_kind};
use mind_core::math::rand_arc::{pack_point2, random_seed_int};
use mind_core::render::floor_cache::{CHUNK_SIZE, FloorChunkGrid};
use mind_core::render::floor_edges::{draw_edges, draw_non_layer, tile_at};
use mind_core::render::floor_main;
use mind_core::render::layer::CacheLayerId;
use mind_core::render::list::{block_cache_layer, floor_cache_layer, is_accessible};
use mind_core::render::scan::CameraView;
use mind_core::world::{Tile, WorldGrid};

use crate::assets::MindAssets;
use crate::sim_host::MindSimHost;

use super::atlas_bind::region_geometry_with_variants;

/// `FloorRenderer.growSprites` padding (0.04 world px).
const GROW: f32 = 0.04;
const _: () = assert!(GROW > 0.0);
/// Upstream error fallback region for the floor batch.
const ERROR_REGION: &str = "env-error";

/// A resolved atlas region (plan-03 page texture + normalized UVs).
#[derive(Clone)]
pub(crate) struct ResolvedRegion {
    /// Manifest page index.
    page: i32,
    /// Page texture (absent when no atlas is loaded — placeholder tint).
    texture: Option<Gd<Texture2D>>,
    /// Natural region width in atlas pixels (`TextureRegion.width`).
    pub w: f32,
    /// Natural region height in atlas pixels.
    pub h: f32,
    /// Normalized `u0`.
    u0: f32,
    /// Normalized `v0`.
    v0: f32,
    /// Normalized `u1`.
    u1: f32,
    /// Normalized `v1`.
    v1: f32,
}

/// One baked quad (positions + UVs + tint).
#[derive(Clone, Copy)]
pub(crate) struct Quad {
    /// Center x in world pixels.
    pub cx: f32,
    /// Center y in world pixels.
    pub cy: f32,
    /// Quad width.
    pub w: f32,
    /// Quad height.
    pub h: f32,
    /// Normalized `u0`.
    pub u0: f32,
    /// Normalized `v0`.
    pub v0: f32,
    /// Normalized `u1`.
    pub u1: f32,
    /// Normalized `v1`.
    pub v1: f32,
    /// RGBA8888 tint.
    pub color: u32,
    /// Overlay sprites need the y-flip camera's v swap (round 18).
    pub v_flip: bool,
}

/// A quad before its region is resolved.
#[derive(Clone)]
pub(crate) struct PendingQuad {
    pub region: String,
    pub cx: f32,
    pub cy: f32,
    pub w: f32,
    pub h: f32,
    pub color: u32,
    /// Plain floors pick a per-tile texture variant (`Floor.variant`).
    pub variant: bool,
    /// Overlay floors need the v swap (see [`Quad::v_flip`]).
    pub v_flip: bool,
    /// `Some((cols, rows, col, row))`: draw one cell of the region's split
    /// (`Floor.edge`'s 3×3 `<blendGroup>-edge` crop, `Floor.java:421-423`).
    pub crop: Option<(u8, u8, u8, u8)>,
    /// `Some((variants, x, y))`: the region is a `<name>-tile<i+1>` tiling
    /// layer and the resolver picks the variant + cell from the sprite-derived
    /// `tilingSize` (`Floor.drawMain:265-269`; `Floor.load:131-146`).
    pub tiling: Option<(i32, i32, i32)>,
}

/// A tile-sized edge cell from a `<blendGroup>-edge` region (`Floor.edge`).
fn edge_quad(
    content: &ContentRegistry,
    edge: mind_core::render::floor_edges::EdgeQuad,
    cx: f32,
    cy: f32,
    size: f32,
) -> PendingQuad {
    let region = content
        .block(edge.blend_group)
        .map(|def| format!("{}-edge", def.name))
        .unwrap_or_else(|| "env-error".to_owned());
    PendingQuad {
        region,
        cx,
        cy,
        w: size,
        h: size,
        color: 0xffff_ffff,
        variant: false,
        // Upright edge cell (Arc `Draw.rect` orientation, J02 T1).
        v_flip: true,
        crop: Some((3, 3, edge.col, edge.row)),
        tiling: None,
    }
}

/// Normalized UVs of one cell in a `cols`×`rows` split of a region.
///
/// Rows count from `v0` (the region's image top) exactly like Arc's
/// `TextureRegion.split` (`TextureRegion.java:289-309`); `Floor.edge` selects
/// `edges[rx][2 - ry]` with `rx = 1 - dx`, `ry = 1 - dy`.
pub(crate) fn crop_uv(uv: [f32; 4], cols: u8, rows: u8, col: u8, row: u8) -> [f32; 4] {
    let cols = cols.max(1) as f32;
    let rows = rows.max(1) as f32;
    let du = (uv[2] - uv[0]) / cols;
    let dv = (uv[3] - uv[1]) / rows;
    let x0 = uv[0] + col as f32 * du;
    let y0 = uv[1] + row as f32 * dv;
    [x0, y0, x0 + du, y0 + dv]
}

/// Region-name → atlas geometry resolver shared by the world and editor
/// floor passes (plan-03 atlas binding + the `error`-region fallback).
pub(crate) struct RegionAtlas {
    assets: Option<Gd<MindAssets>>,
    regions: HashMap<String, Option<ResolvedRegion>>,
    variant_counts: HashMap<String, usize>,
    /// Regions that fell back to `error`/placeholder.
    pub missing: u64,
}

impl RegionAtlas {
    /// A resolver over the client's atlas (`None` = placeholder tint).
    pub fn new(assets: Option<Gd<MindAssets>>) -> Self {
        Self {
            assets,
            regions: HashMap::new(),
            variant_counts: HashMap::new(),
            missing: 0,
        }
    }

    /// Resolves a region name to atlas geometry (cached; `error` fallback).
    pub fn resolve(&mut self, name: &str) -> Option<ResolvedRegion> {
        if let Some(cached) = self.regions.get(name) {
            return cached.clone();
        }
        let resolved = self.lookup(name);
        self.regions.insert(name.to_owned(), resolved.clone());
        resolved
    }

    /// `Floor.variantRegions.len()`: the numbered atlas variants
    /// (`name1..nameN`) when present, else the bare region (one variant).
    fn variant_count(&mut self, name: &str) -> usize {
        if let Some(count) = self.variant_counts.get(name) {
            return *count;
        }
        let count = match self.assets.clone() {
            Some(assets) => {
                let assets = assets.bind();
                if !assets.region_geometry(GString::from(name)).is_empty() {
                    1
                } else {
                    let mut count = 0usize;
                    for index in 1..=4 {
                        let candidate = format!("{name}{index}");
                        if assets
                            .region_geometry(GString::from(candidate.as_str()))
                            .is_empty()
                        {
                            break;
                        }
                        count = index;
                    }
                    count
                }
            }
            None => 1,
        }
        .max(1);
        self.variant_counts.insert(name.to_owned(), count);
        count
    }

    /// `Floor.drawMain` plain-floor variant: `variant(x, y, count)` picks the
    /// numbered region with Arc `Mathf.randomSeed(Point2.pack(x, y), 0, count-1)`.
    pub fn resolve_variant(&mut self, name: &str, x: i32, y: i32) -> Option<ResolvedRegion> {
        let count = self.variant_count(name);
        if count <= 1 {
            return self.resolve(name);
        }
        let index = random_seed_int(pack_point2(x, y), 0, count as i32 - 1);
        self.resolve(&format!("{name}{}", index + 1))
    }

    /// `Floor.drawMain`'s tiling branch (`Floor.java:265-269`): the region is
    /// `<name>-tile<v+1>` with `v = randomSeed(pack(x / tilingSize, y /
    /// tilingSize), 0, tilingVariants - 1)` and the drawn cell is `[x %
    /// tilingSize][tilingSize - 1 - y % tilingSize]`.
    ///
    /// `tilingSize` (`Floor.load:133-142`) is the cell count of the `-tile1`
    /// region at `tsize = tilesize / Draw.scl` = 32 px (`Control.java:330`);
    /// upstream disables tiling when a sprite's split is inconsistent with the
    /// first one, so a non-square split falls back to the plain region.
    pub(crate) fn resolve_tiling(
        &mut self,
        name: &str,
        variants: i32,
        x: i32,
        y: i32,
    ) -> (i32, Option<Gd<Texture2D>>, [f32; 4]) {
        let probe = self.resolve(&format!("{name}-tile1"));
        let (cols, rows) = match probe.as_ref() {
            Some(region) => floor_main::tiling_split(region.w as i32, region.h as i32),
            None => (1, 1),
        };
        if cols != rows {
            let resolved = self.resolve(name);
            return resolved_uv(&resolved);
        }
        let (block_x, block_y) = floor_main::tiling_block(x, y, cols);
        let index = random_seed_int(pack_point2(block_x, block_y), 0, variants - 1) as u32;
        let resolved = if index == 0 {
            probe
        } else {
            self.resolve(&format!("{name}-tile{}", index + 1))
        };
        let (page, texture, uv) = resolved_uv(&resolved);
        let (col, row) = floor_main::tiling_cell(x, y, cols);
        (page, texture, crop_uv(uv, cols, cols, col, row))
    }

    fn lookup(&mut self, name: &str) -> Option<ResolvedRegion> {
        let assets = self.assets.clone()?;
        let assets = assets.bind();
        let mut geometry = region_geometry_with_variants(&assets, name);
        if geometry.is_empty() && name != ERROR_REGION {
            log::warn!("floor region missing: `{name}`");
            self.missing += 1;
            geometry = assets.region_geometry(GString::from(ERROR_REGION));
            if geometry.is_empty() {
                geometry = assets.region_geometry(GString::from("error"));
            }
        }
        if geometry.len() < 5 {
            return None;
        }
        let slice = geometry.as_slice();
        let page = slice[4];
        let texture = assets
            .page_texture(page as i64)
            .map(|texture| texture.upcast::<Texture2D>());
        let (width, height) = texture
            .as_ref()
            .map(|texture| {
                (
                    texture.get_width().max(1) as f32,
                    texture.get_height().max(1) as f32,
                )
            })
            .unwrap_or((1.0, 1.0));
        Some(ResolvedRegion {
            page,
            texture,
            w: slice[2] as f32,
            h: slice[3] as f32,
            u0: slice[0] as f32 / width,
            v0: slice[1] as f32 / height,
            u1: (slice[0] + slice[2]) as f32 / width,
            v1: (slice[1] + slice[3]) as f32 / height,
        })
    }
}

/// Per-chunk baked nodes (`Vec` indexed by `CacheLayerId.id()`).
#[derive(Default)]
struct ChunkMeshes {
    layers: [Vec<Gd<MeshInstance2D>>; 9],
}

/// Floor-renderer counters mirrored into `MindWorldRenderer`'s stats.
#[derive(Clone, Copy, Debug, Default)]
pub struct FloorStats {
    /// Chunk/layer meshes rebuilt since boot.
    pub mesh_rebuilds: u64,
    /// Dirty floor chunks at the end of the frame.
    pub dirty: i64,
    /// Regions that fell back to `error`/placeholder.
    pub missing_regions: u64,
}

/// The floor pass: bake and attach per-chunk meshes to the floor band nodes.
pub struct FloorRenderer {
    host: Gd<MindSimHost>,
    atlas: RegionAtlas,
    /// Floor band `Node2D` per `CacheLayerId.id()`.
    band_nodes: Vec<Gd<Node2D>>,
    grid: FloorChunkGrid,
    /// World dimensions the chunk grid was built for; a change (menu world →
    /// loaded sector/map) rebuilds the grid and drops stale chunk meshes.
    dims: (i32, i32),
    /// Last `MindSimHost::world_revision` seen.
    revision: i64,
    /// Chunk linear index → baked nodes.
    chunks: BTreeMap<usize, ChunkMeshes>,
    stats: FloorStats,
}

impl FloorRenderer {
    /// Builds the renderer for a world and the floor band nodes.
    pub fn new(
        host: Gd<MindSimHost>,
        assets: Option<Gd<MindAssets>>,
        band_nodes: Vec<Gd<Node2D>>,
    ) -> Self {
        let (width, height) = host.bind().world_size();
        Self {
            revision: i64::MIN,
            host,
            atlas: RegionAtlas::new(assets),
            band_nodes,
            grid: FloorChunkGrid::new(width, height),
            dims: (width, height),
            chunks: BTreeMap::new(),
            stats: FloorStats::default(),
        }
    }

    /// Current counters.
    pub fn stats(&self) -> FloorStats {
        FloorStats {
            missing_regions: self.atlas.missing,
            ..self.stats
        }
    }

    /// Rebuilds the chunks that are dirty and in view (plan 16 §3.5
    /// `drawFloor` preliminary pass). Called once per frame after view sync.
    pub fn update(&mut self, view: &CameraView) {
        let (width, height) = self.host.clone().bind().world_size();
        if (width, height) != self.dims {
            self.resize(width, height);
        }
        let revision = self.host.clone().bind().world_revision();
        if revision != self.revision {
            self.revision = revision;
            for cy in 0..self.grid.chunks_y() {
                for cx in 0..self.grid.chunks_x() {
                    self.grid.mark_dirty(cx, cy);
                }
            }
        }

        // Lazy bake: only dirty chunks in view are rebuilt; out-of-view chunks
        // stay dirty until they scroll in (plan 16 §3.5 drawFloor preliminary).
        for (cx, cy) in self.grid.dirty_chunks() {
            if view.overlaps(FloorChunkGrid::chunk_aabb(cx, cy)) {
                self.bake_chunk(cx, cy);
            }
        }
        self.stats.dirty = self.grid.dirty_count() as i64;
    }

    /// Rebuilds the chunk bookkeeping for a new world size and frees every
    /// baked mesh of the previous world (`FloorRenderer` is constructed per
    /// world upstream; the port keeps one instance and resizes it).
    fn resize(&mut self, width: i32, height: i32) {
        for mut old in std::mem::take(&mut self.chunks).into_values() {
            for nodes in &mut old.layers {
                for node in nodes.iter_mut() {
                    node.queue_free();
                }
            }
        }
        self.dims = (width, height);
        self.grid = FloorChunkGrid::new(width, height);
        // Force the revision branch to dirty every new chunk on the next frame.
        self.revision = i64::MIN;
    }

    /// Frees the current nodes for a chunk and re-bakes every used cache layer.
    fn bake_chunk(&mut self, cx: i32, cy: i32) {
        if let Some(mut old) = self.chunks.remove(&lin(cx, cy, self.grid.chunks_x())) {
            for nodes in &mut old.layers {
                for node in nodes.iter_mut() {
                    node.queue_free();
                }
            }
        }

        let used = {
            let host = self.host.clone();
            let host = host.bind();
            match host.content_registry() {
                Some(content) => chunk_used_layers(host.grid(), content, cx, cy),
                None => SmallVec::new(),
            }
        };
        self.grid.set_used_layers(cx, cy, used.clone());

        let mut meshes = ChunkMeshes::default();
        for layer in used {
            let nodes = self.build_layer(cx, cy, layer);
            if !nodes.is_empty() {
                meshes.layers[layer.id() as usize] = nodes;
            }
        }
        self.chunks
            .insert(lin(cx, cy, self.grid.chunks_x()), meshes);
        self.grid.mark_baked(cx, cy);
        self.stats.mesh_rebuilds += 1;
    }

    /// Builds the `MeshInstance2D` nodes for one `(chunk, CacheLayer)`.
    fn build_layer(&mut self, cx: i32, cy: i32, layer: CacheLayerId) -> Vec<Gd<MeshInstance2D>> {
        let mut groups: BTreeMap<i32, (Option<Gd<Texture2D>>, Vec<Quad>)> = BTreeMap::new();
        let host = self.host.clone();
        {
            let host = host.bind();
            let world = host.grid();
            for ty in cy * CHUNK_SIZE..(cy + 1) * CHUNK_SIZE {
                for tx in cx * CHUNK_SIZE..(cx + 1) * CHUNK_SIZE {
                    if tx < 0 || ty < 0 || tx >= world.width() || ty >= world.height() {
                        continue;
                    }
                    let tile = world.tile(tx, ty);
                    let quads = match host.content_registry() {
                        Some(content) => tile_quads(world, content, tile, layer),
                        None => SmallVec::new(),
                    };
                    // Collect first; region resolution (a `&mut self.atlas`
                    // borrow) must not nest inside the host bind.
                    for quad in quads {
                        // Tiling layers resolve their region and crop from the
                        // sprite-derived `tilingSize` (`resolve_tiling`).
                        let (page, texture, uv) = match quad.tiling {
                            Some((variants, tqx, tqy)) => {
                                self.atlas.resolve_tiling(&quad.region, variants, tqx, tqy)
                            }
                            None => {
                                let resolved = if quad.variant {
                                    self.atlas.resolve_variant(&quad.region, tx, ty)
                                } else {
                                    self.atlas.resolve(&quad.region)
                                };
                                let (page, texture, uv) = resolved_uv(&resolved);
                                let uv = match quad.crop {
                                    Some((cols, rows, col, row)) => {
                                        crop_uv(uv, cols, rows, col, row)
                                    }
                                    None => uv,
                                };
                                (page, texture, uv)
                            }
                        };
                        let entry = groups.entry(page).or_insert((texture, Vec::new()));
                        entry.1.push(Quad {
                            cx: quad.cx,
                            cy: quad.cy,
                            w: quad.w,
                            h: quad.h,
                            u0: uv[0],
                            v0: uv[1],
                            u1: uv[2],
                            v1: uv[3],
                            color: quad.color,
                            v_flip: quad.v_flip,
                        });
                    }
                }
            }
        }

        let Some(band) = self.band_nodes.get(layer.id() as usize).cloned() else {
            return Vec::new();
        };
        let grow = if layer == CacheLayerId::Walls {
            GROW
        } else {
            0.0
        };
        let mut nodes = Vec::new();
        for (_page, (texture, quads)) in groups {
            if quads.is_empty() {
                continue;
            }
            let Some(mesh) = build_mesh(&quads, grow) else {
                continue;
            };
            let mut node = MeshInstance2D::new_alloc();
            node.set_mesh(&mesh.upcast::<Mesh>());
            if let Some(texture) = texture {
                node.set_texture(&texture);
            }
            // code-instantiated: one mesh per (chunk, CacheLayer, atlas page);
            // count/geometry are data-driven from the world bake (plan 16 §6.6).
            band.clone().add_child(&node);
            nodes.push(node);
        }
        nodes
    }
}

/// `FloorRenderer.cacheChunk` used-set (border + accessibility rule).
pub(crate) fn chunk_used_layers(
    world: &WorldGrid,
    content: &ContentRegistry,
    cx: i32,
    cy: i32,
) -> SmallVec<[CacheLayerId; 9]> {
    chunk_used_layers_sized(world, content, cx, cy, CHUNK_SIZE)
}

/// [`chunk_used_layers`] for a chunk grid of `chunk` tiles per side (the
/// editor bakes `EditorRenderer.chunkSize` = 60, the world cache 30).
pub(crate) fn chunk_used_layers_sized(
    world: &WorldGrid,
    content: &ContentRegistry,
    cx: i32,
    cy: i32,
    chunk: i32,
) -> SmallVec<[CacheLayerId; 9]> {
    let mut used: SmallVec<[CacheLayerId; 9]> = SmallVec::new();
    let min_x = (cx * chunk - 1).max(0);
    let min_y = (cy * chunk - 1).max(0);
    let max_x = ((cx + 1) * chunk + 1).min(world.width());
    let max_y = ((cy + 1) * chunk + 1).min(world.height());
    for ty in min_y..max_y {
        for tx in min_x..max_x {
            let tile = world.tile(tx, ty);
            let Some(block) = content.block(tile.block) else {
                continue;
            };
            let wall = block_cache_layer(block) != CacheLayerId::Normal;
            if wall {
                push_unique(&mut used, block_cache_layer(block));
            }
            let accessible = !wall || is_accessible(world, content, tx, ty);
            if accessible && let Some(floor) = content.block(tile.floor) {
                push_unique(&mut used, floor_cache_layer(floor));
            }
        }
    }
    used.sort_by_key(|layer| layer.id());
    used
}

/// `FloorRenderer.cacheChunkLayer` draw decision for one tile/layer (base
/// floor, edges, overlay floor, then the wall block).
///
/// `Floor.drawBase` (`Floor.java:249-262`) draws `drawMain`, `drawEdges` when
/// `drawEdgeIn`, then `drawOverlay` → `overlay.drawBase`; tiles whose floor is
/// on another cache layer emit the neighbour floors' edges instead
/// (`drawNonLayer`, `Floor.java:343-361`; `FloorRenderer.java:318-336`). Walls
/// stay a separate cache layer.
pub(crate) fn tile_quads(
    world: &WorldGrid,
    content: &ContentRegistry,
    tile: &Tile,
    layer: CacheLayerId,
) -> SmallVec<[PendingQuad; 4]> {
    let mut out = SmallVec::new();
    let size = TILESIZE as f32;
    // Java world convention: tile (x, y)'s centre is `(x * tilesize, y * tilesize)`
    // (`Tile.worldx()`; Arc's `Draw.rect(region, x, y)` centres the sprite there).
    let cx = tile.x as f32 * size;
    let cy = tile.y as f32 * size;

    if layer == CacheLayerId::Walls {
        let Some(block) = content.block(tile.block) else {
            return out;
        };
        if block_cache_layer(block) == CacheLayerId::Walls {
            out.push(PendingQuad {
                region: block.region.clone(),
                cx,
                cy,
                w: block.size.max(1) as f32 * size,
                h: block.size.max(1) as f32 * size,
                color: 0xffff_ffff,
                variant: false,
                // Arc `Draw.rect` puts the region's top at the world's top; the
                // world pass runs through the y-flip camera, so every upright
                // sprite takes the v swap (J02 T1; same rule overlays always
                // had, "round 18").
                v_flip: true,
                crop: None,
                tiling: None,
            });
        }
        return out;
    }

    let floor_def = content.block(tile.floor);
    let floor_matches_layer = floor_def.is_some_and(|floor| floor_cache_layer(floor) == layer)
        && floor_def.is_some_and(|floor| floor.kind != BlockKind::AirBlock);

    if floor_matches_layer {
        let accessible = is_accessible(world, content, tile.x as i32, tile.y as i32);
        let wall_layer = content
            .block(tile.block)
            .is_some_and(|block| block_cache_layer(block) == CacheLayerId::Walls);
        if accessible || !wall_layer {
            out.extend(floor_draw_base_quads(world, content, tile, layer));
        }
    } else if floor_def.is_some_and(|floor| floor_cache_layer(floor) != layer)
        && layer != CacheLayerId::Walls
    {
        // `Floor.drawNonLayer(tile, layer)` (`Floor.java:343-361`): floors on
        // other cache layers draw their edges on this tile's layer bake (water
        // edges over shallow, etc.).
        for edge in draw_non_layer(content, world, tile, layer) {
            out.push(edge_quad(content, edge, cx, cy, size));
        }
    }

    out
}

/// `Floor.drawBase` (`Floor.java:249-262`) quads for one tile: `drawMain`, then
/// `drawEdges` when `drawEdgeIn`, then `drawOverlay` → `overlay.drawBase`.
///
/// No accessibility/building gate: `MenuRenderer.cache()` (`:191-197`) calls
/// `tile.floor().drawBase(tile)` for every tile of the menu world, and the
/// chunked `FloorRenderer` path applies its own gate in [`tile_quads`].
pub(crate) fn floor_draw_base_quads(
    world: &WorldGrid,
    content: &ContentRegistry,
    tile: &Tile,
    layer: CacheLayerId,
) -> SmallVec<[PendingQuad; 4]> {
    let mut out = SmallVec::new();
    let size = TILESIZE as f32;
    let cx = tile.x as f32 * size;
    let cy = tile.y as f32 * size;

    // `AirBlock.drawBase()` overrides `Floor.drawBase` with an empty body, so
    // an `air` floor contributes no base, no edges and no overlay
    // (`AirBlock.java`; the region is never drawn, not even as `error`).
    let Some(floor) = content.block(tile.floor) else {
        return out;
    };
    if floor_cache_layer(floor) != layer || floor.kind == BlockKind::AirBlock {
        return out;
    }

    // `Floor.drawMain` (`Floor.java:264-292`): the tiling, autotile or plain
    // variant branch (J02 T2).
    let main = main_quad(world, content, tile, floor, false);
    out.push(main.clone());

    // `Floor.drawBase`: `drawEdges` when `drawEdgeIn` (`Floor.java:252`).
    // The tile's own edge sprites plus the neighbours' — this is the
    // `drawEdges`/`drawBlended`/`doEdge` port (`Floor.java:363-440`).
    if floor.draw_edge_in {
        for edge in draw_edges(content, world, tile, tile.floor) {
            out.push(edge_quad(content, edge, cx, cy, size));
        }
    }

    // `Floor.drawOverlay` → `overlay.drawBase` (`Floor.java:304-309`):
    // an overlay floor draws on top of the base floor it belongs to.
    if tile.overlay != BlockId::AIR
        && tile.overlay != tile.floor
        && let Some(overlay) = content.block(tile.overlay)
    {
        // The overlay's own `drawMain` (`Floor.drawOverlay` → `overlay.drawBase`,
        // `Floor.java:304-309`): its autotile same-check reads `tile.overlay`.
        out.push(main_quad(world, content, tile, overlay, true));
        // `OverlayFloor.drawBase` (`OverlayFloor.java:22-24`) draws the
        // variant only; a plain floor used as an overlay runs the full
        // `Floor.drawBase` (its own edges; the overlay-of-overlay step
        // cannot recurse).
        if !is_overlay_floor_kind(overlay.kind) && overlay.draw_edge_in {
            for edge in draw_edges(content, world, tile, tile.overlay) {
                out.push(edge_quad(content, edge, cx, cy, size));
            }
        }
    }

    // `Floor.drawBase` (`Floor.java:257-261`): when the tile has an overlay and
    // this (liquid) floor is the tile's floor, `drawMain` runs again at
    // `1 - overlayAlpha` over the overlay (no edges, no overlay pass).
    if tile.overlay != BlockId::AIR && floor.is_liquid {
        let mut repeat = main;
        repeat.color = 0xffff_ff00u32 | overlay_redraw_alpha(floor.overlay_alpha);
        out.push(repeat);
    }

    out
}

/// The `Draw.alpha(1f - overlayAlpha)` byte of the liquid-overlay redraw
/// (`Floor.java:258`).
fn overlay_redraw_alpha(overlay_alpha: f32) -> u32 {
    let value = (1.0 - overlay_alpha).clamp(0.0, 1.0);
    (value * 255.0).round() as u32
}

/// `Floor.checkAutotileSame` (`Floor.java:284-286`): the neighbour on the same
/// layer as the drawn def (`other.floor` when `this == tile.floor`, else
/// `other.overlay`) shares the drawn def's `blendGroup`. An out-of-bounds
/// neighbour (`Tile.nearby` returns null) is never the same.
fn autotile_same(
    world: &WorldGrid,
    content: &ContentRegistry,
    tile: &Tile,
    group: BlockId,
    is_overlay: bool,
    dx: i32,
    dy: i32,
) -> bool {
    let Some(other) = tile_at(world, tile.x as i32 + dx, tile.y as i32 + dy) else {
        return false;
    };
    let id = if is_overlay {
        other.overlay
    } else {
        other.floor
    };
    content
        .block(id)
        .is_some_and(|def| def.blend_group_id() == group)
}

/// `Floor.drawMain` (`Floor.java:264-292`): the tiling, autotile or plain
/// variant region for one tile of a floor layer. `is_overlay` picks the
/// `tile.overlay` layer for the autotile same-check.
fn main_quad(
    world: &WorldGrid,
    content: &ContentRegistry,
    tile: &Tile,
    def: &BlockDef,
    is_overlay: bool,
) -> PendingQuad {
    let x = tile.x as i32;
    let y = tile.y as i32;
    // Same frame as `tile_quads` (`Tile.worldx()` × `tilesize`).
    let size = TILESIZE as f32;
    let cx = tile.x as f32 * size;
    let cy = tile.y as f32 * size;
    let plain = |region: String,
                 crop: Option<(u8, u8, u8, u8)>,
                 tiling: Option<(i32, i32, i32)>| PendingQuad {
        region,
        cx,
        cy,
        w: size,
        h: size,
        color: 0xffff_ffff,
        variant: false,
        v_flip: true,
        crop,
        tiling,
    };
    if def.tiling_variants > 0 {
        // `Floor.drawMain:265-269`: the variant is chosen per block of
        // `tilingSize` tiles and the drawn cell is `[x % tilingSize][tilingSize
        // - 1 - y % tilingSize]`. Both need the sprite-derived `tilingSize`
        // (`Floor.load:133-142`), so the resolver (`resolve_tiling`) owns the
        // region name and the crop.
        return plain(def.name.clone(), None, Some((def.tiling_variants, x, y)));
    }

    if def.autotile {
        // `Floor.drawMain:270-283`: 8-neighbour bitmask through
        // `TileBitmask.values`, with `autotileVariants`/`autotileMidVariants`
        // rows and the bit-13 mid slices.
        let group = def.blend_group_id();
        let mut same = [false; 8];
        for (index, (dx, dy)) in floor_main::D8.iter().enumerate() {
            same[index] = autotile_same(world, content, tile, group, is_overlay, *dx, *dy);
        }
        let bit = floor_main::autotile_index(floor_main::autotile_bits(same));
        let variant_pick = if def.autotile_variants > 1 {
            random_seed_int(pack_point2(x, y), 0, def.autotile_variants - 1) as u32
        } else {
            0
        };
        let mid_pick = if def.autotile_mid_variants > 1 {
            random_seed_int(pack_point2(x, y), 0, def.autotile_mid_variants - 1) as u32
        } else {
            0
        };
        return plain(
            floor_main::autotile_region(
                &def.name,
                bit,
                def.autotile_variants,
                variant_pick,
                def.autotile_mid_variants,
                mid_pick,
            ),
            None,
            None,
        );
    }

    // The plain random variant (`Floor.drawMain:285-287`); the resolver picks
    // the numbered region.
    PendingQuad {
        region: def.region.clone(),
        cx,
        cy,
        w: size,
        h: size,
        color: 0xffff_ffff,
        variant: true,
        v_flip: true,
        crop: None,
        tiling: None,
    }
}

/// The page/UV tuple for a resolved (or placeholder) region.
pub(crate) fn resolved_uv(
    resolved: &Option<ResolvedRegion>,
) -> (i32, Option<Gd<Texture2D>>, [f32; 4]) {
    match resolved {
        Some(region) => (
            region.page,
            region.texture.clone(),
            [region.u0, region.v0, region.u1, region.v1],
        ),
        // No atlas: one shared "page" key so all placeholders coalesce.
        None => (-1, None, [0.0, 0.0, 1.0, 1.0]),
    }
}

/// Builds an `ArrayMesh` from quads (two triangles each).
pub(crate) fn build_mesh(quads: &[Quad], grow: f32) -> Option<Gd<ArrayMesh>> {
    // I02 T2: an empty slice gets no mesh (see `atlas_bind::has_mesh_quads`).
    if !crate::render::atlas_bind::has_mesh_quads(quads) {
        return None;
    }
    let mut tool = SurfaceTool::new_gd();
    tool.begin(PrimitiveType::TRIANGLES);
    for quad in quads {
        let x = quad.cx - quad.w / 2.0 - grow;
        let y = quad.cy - quad.h / 2.0 - grow;
        let w = quad.w + grow * 2.0;
        let h = quad.h + grow * 2.0;
        let color = color_from(quad.color);
        let (v_top, v_bottom) = if quad.v_flip {
            (quad.v1, quad.v0)
        } else {
            (quad.v0, quad.v1)
        };
        let corners = [
            (x, y, quad.u0, v_top),
            (x + w, y, quad.u1, v_top),
            (x + w, y + h, quad.u1, v_bottom),
            (x, y + h, quad.u0, v_bottom),
        ];
        for index in [0usize, 1, 2, 0, 2, 3] {
            let (vx, vy, u, v) = corners[index];
            tool.set_color(color);
            tool.set_uv(Vector2::new(u, v));
            tool.add_vertex(Vector3::new(vx, vy, 0.0));
        }
    }
    tool.commit()
}

/// RGBA8888 → `Color`.
pub(crate) fn color_from(rgba: u32) -> Color {
    Color::from_rgba8(
        ((rgba >> 24) & 0xff) as u8,
        ((rgba >> 16) & 0xff) as u8,
        ((rgba >> 8) & 0xff) as u8,
        (rgba & 0xff) as u8,
    )
}

fn push_unique(layers: &mut SmallVec<[CacheLayerId; 9]>, layer: CacheLayerId) {
    if !layers.contains(&layer) {
        layers.push(layer);
    }
}

fn lin(cx: i32, cy: i32, chunks_x: i32) -> usize {
    (cx + cy * chunks_x) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use mind_core::content::registries::create_base_content;
    use mind_core::content::{MemoryBundle, MemoryUnlockStore};

    #[test]
    fn color_from_rgba() {
        let color = color_from(0x8040_20ff);
        assert!((color.r - 0.5).abs() < 0.01);
        assert!((color.g - 0.25).abs() < 0.01);
        assert!((color.b - 0.125).abs() < 0.01);
        assert!((color.a - 1.0).abs() < 0.01);
    }

    #[test]
    fn lin_indexes_row_major() {
        assert_eq!(lin(1, 0, 3), 1);
        assert_eq!(lin(0, 1, 3), 3);
        assert_eq!(lin(2, 2, 3), 8);
    }

    fn test_registry() -> ContentRegistry {
        let bundle = MemoryBundle::new();
        let store = MemoryUnlockStore::new();
        let Ok(mut registry) = create_base_content(&bundle, &store, true) else {
            panic!("base content");
        };
        if registry.init().is_err() {
            panic!("content init");
        }
        if registry.post_init().is_err() {
            panic!("content post init");
        }
        registry
    }

    fn block(registry: &ContentRegistry, name: &str) -> BlockId {
        match registry.block_id(name) {
            Some(id) => id,
            None => panic!("block {name}"),
        }
    }

    fn base_region(quads: &[PendingQuad]) -> &str {
        match quads.first() {
            Some(quad) => &quad.region,
            None => panic!("base quad"),
        }
    }

    fn fill(world: &mut WorldGrid, floor: BlockId) {
        for y in 0..world.height() {
            for x in 0..world.width() {
                world.tiles.get_mut(x, y).floor = floor;
            }
        }
    }

    /// `Floor.drawMain`'s tiling branch (`Floor.java:265-269`): the variant is
    /// picked per block of `tilingSize` tiles (`resolve_tiling`) and the quad
    /// carries the def's variant count plus the raw tile coords. Vanilla has no
    /// tiling floors, so the branch runs on a patched `stone` def.
    #[test]
    fn tiling_floor_picks_tile_layer_and_carries_cell_coords() {
        let mut registry = test_registry();
        let stone = block(&registry, "stone");
        match registry.block_mut(stone) {
            Some(def) => def.tiling_variants = 2,
            None => panic!("stone def"),
        }
        let mut world = WorldGrid::new(4, 4);
        fill(&mut world, stone);
        let quads = tile_quads(&world, &registry, world.tile(0, 0), CacheLayerId::Normal);
        let base = match quads.first() {
            Some(quad) => quad,
            None => panic!("base quad"),
        };
        // The region name is the def's; the resolver picks the `-tile<v+1>`
        // region and the cell once the sprite-derived split is known.
        assert_eq!(base.region, "stone");
        assert_eq!(base.tiling, Some((2, 0, 0)));
        assert!(!base.variant);
        let other = tile_quads(&world, &registry, world.tile(1, 2), CacheLayerId::Normal);
        let other = match other.first() {
            Some(quad) => quad,
            None => panic!("base quad"),
        };
        assert_eq!(other.region, "stone");
        assert_eq!(other.tiling, Some((2, 1, 2)));
    }

    /// `Floor.drawMain`'s autotile branch (`Floor.java:270-283`): the 8-neighbour
    /// bitmask through `TileBitmask.values` picks `metal-tiles-1-<index>` —
    /// 0 bits (all neighbours different) → 39, E/NE/N (a 2×2 block corner) → 24,
    /// W/SW/S (the opposite corner) → 2.
    #[test]
    fn autotile_floor_uses_the_bitmask_region() {
        let registry = test_registry();
        let metal = block(&registry, "metal-tiles-1");
        let stone = block(&registry, "stone");
        let mut world = WorldGrid::new(4, 4);
        fill(&mut world, stone);
        // A lone tile at (0, 0) and a 2×2 block at x∈{2,3}, y∈{1,2}.
        world.tiles.get_mut(0, 0).floor = metal;
        for (x, y) in [(2, 1), (3, 1), (2, 2), (3, 2)] {
            world.tiles.get_mut(x, y).floor = metal;
        }
        let lone = tile_quads(&world, &registry, world.tile(0, 0), CacheLayerId::Normal);
        assert_eq!(base_region(&lone), "metal-tiles-1-39");
        // The block's south-west corner: E, NE and N are the same floor.
        let corner = tile_quads(&world, &registry, world.tile(2, 1), CacheLayerId::Normal);
        assert_eq!(base_region(&corner), "metal-tiles-1-24");
        // The block's north-east corner: W, SW and S are the same floor.
        let far = tile_quads(&world, &registry, world.tile(3, 2), CacheLayerId::Normal);
        assert_eq!(base_region(&far), "metal-tiles-1-2");
    }

    /// The bit-13 mid slices (`Floor.autotileMidVariants`): a fully surrounded
    /// `metal-tiles-7` tile picks `-13` or `-mid-<i+1>` by the shared
    /// `variant(x, y, len)` pick.
    #[test]
    fn autotile_mid_variants_take_the_fully_surrounded_tile() {
        let registry = test_registry();
        let metal = block(&registry, "metal-tiles-7");
        let mut world = WorldGrid::new(3, 3);
        fill(&mut world, metal);
        let mid_index = random_seed_int(pack_point2(1, 1), 0, 8) as u32;
        let expected = if mid_index == 0 {
            "metal-tiles-7-13".to_owned()
        } else {
            format!("metal-tiles-7-mid-{}", mid_index + 1)
        };
        let quads = tile_quads(&world, &registry, world.tile(1, 1), CacheLayerId::Normal);
        assert_eq!(base_region(&quads), expected);
    }

    /// `Floor.drawBase`'s liquid pass (`Floor.java:257-261`): a liquid floor
    /// under any overlay redraws its main quad at `1 - overlayAlpha` (0.65 →
    /// alpha 89) after the overlay.
    #[test]
    fn liquid_floor_redraws_main_over_its_overlay() {
        let registry = test_registry();
        let water = block(&registry, "shallow-water");
        let sand = block(&registry, "sand-floor");
        let mut world = WorldGrid::new(3, 3);
        fill(&mut world, sand);
        let tile = world.tiles.get_mut(1, 1);
        tile.floor = water;
        tile.overlay = sand;
        let quads = tile_quads(&world, &registry, world.tile(1, 1), CacheLayerId::Water);
        let (Some(base), Some(last)) = (quads.first(), quads.last()) else {
            panic!("base and redraw quads");
        };
        assert_eq!(last.region, base.region);
        assert_eq!(last.color, 0xffff_ff59);
        // `pooled-cryofluid` sets `overlayAlpha = 0.35f` (`Blocks.java:300`).
        assert_eq!(overlay_redraw_alpha(0.35), 166);
        // A liquid without an overlay draws no redraw pass.
        world.tiles.get_mut(2, 2).floor = water;
        let bare = tile_quads(&world, &registry, world.tile(2, 2), CacheLayerId::Water);
        assert!(!bare.is_empty());
        assert!(bare.iter().all(|quad| quad.color != 0xffff_ff59));
    }
}
