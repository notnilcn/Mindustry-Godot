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
use mind_core::content::{BlockId, ContentRegistry};
use mind_core::render::floor_cache::{CHUNK_SIZE, FloorChunkGrid};
use mind_core::render::layer::CacheLayerId;
use mind_core::render::list::{block_cache_layer, floor_cache_layer, is_accessible};
use mind_core::render::scan::CameraView;
use mind_core::world::{Tile, WorldGrid};

use crate::assets::MindAssets;
use crate::sim_host::MindSimHost;

/// `FloorRenderer.growSprites` padding (0.04 world px).
const GROW: f32 = 0.04;
const _: () = assert!(GROW > 0.0);
/// Upstream error fallback region for the floor batch.
const ERROR_REGION: &str = "env-error";

/// A resolved atlas region (plan-03 page texture + normalized UVs).
#[derive(Clone)]
struct ResolvedRegion {
    /// Manifest page index.
    page: i32,
    /// Page texture (absent when no atlas is loaded — placeholder tint).
    texture: Option<Gd<Texture2D>>,
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
struct Quad {
    /// Center x in world pixels.
    cx: f32,
    /// Center y in world pixels.
    cy: f32,
    /// Quad width.
    w: f32,
    /// Quad height.
    h: f32,
    /// Normalized `u0`.
    u0: f32,
    /// Normalized `v0`.
    v0: f32,
    /// Normalized `u1`.
    u1: f32,
    /// Normalized `v1`.
    v1: f32,
    /// RGBA8888 tint.
    color: u32,
}

/// A quad before its region is resolved.
struct PendingQuad {
    region: String,
    cx: f32,
    cy: f32,
    w: f32,
    h: f32,
    color: u32,
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
    assets: Option<Gd<MindAssets>>,
    /// Floor band `Node2D` per `CacheLayerId.id()`.
    band_nodes: Vec<Gd<Node2D>>,
    grid: FloorChunkGrid,
    /// Last `MindSimHost::world_revision` seen.
    revision: i64,
    /// Region-name → resolved geometry (never strings per frame).
    regions: HashMap<String, Option<ResolvedRegion>>,
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
            assets,
            band_nodes,
            grid: FloorChunkGrid::new(width, height),
            regions: HashMap::new(),
            chunks: BTreeMap::new(),
            stats: FloorStats::default(),
        }
    }

    /// Current counters.
    pub fn stats(&self) -> FloorStats {
        self.stats
    }

    /// Rebuilds the chunks that are dirty and in view (plan 16 §3.5
    /// `drawFloor` preliminary pass). Called once per frame after view sync.
    pub fn update(&mut self, view: &CameraView) {
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

    /// Frees the current nodes for a chunk and re-bakes every used cache layer.
    fn bake_chunk(&mut self, cx: i32, cy: i32) {
        if let Some(mut old) = self.chunks.remove(&lin(cx, cy, self.grid.chunks_x())) {
            for nodes in &mut old.layers {
                for node in nodes.iter_mut() {
                    node.queue_free();
                }
            }
        }

        let used = self.used_layers(cx, cy);
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

    /// `FloorRenderer.cacheChunk` used-set (border + accessibility rule).
    fn used_layers(&self, cx: i32, cy: i32) -> SmallVec<[CacheLayerId; 9]> {
        let host = self.host.clone();
        let host = host.bind();
        let world = host.grid();
        let Some(content) = host.content_registry() else {
            return SmallVec::new();
        };
        let mut used: SmallVec<[CacheLayerId; 9]> = SmallVec::new();
        let min_x = (cx * CHUNK_SIZE - 1).max(0);
        let min_y = (cy * CHUNK_SIZE - 1).max(0);
        let max_x = ((cx + 1) * CHUNK_SIZE + 1).min(world.width());
        let max_y = ((cy + 1) * CHUNK_SIZE + 1).min(world.height());
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
                    let quad = match host.content_registry() {
                        Some(content) => tile_quad(world, content, tile, layer),
                        None => None,
                    };
                    if let Some(quad) = quad {
                        let resolved = self.resolve(&quad.region);
                        let (page, texture, uv) = resolved_uv(&resolved);
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

    /// Resolves a region name to atlas geometry (cached; `error` fallback).
    fn resolve(&mut self, name: &str) -> Option<ResolvedRegion> {
        if let Some(cached) = self.regions.get(name) {
            return cached.clone();
        }
        let resolved = self.lookup(name);
        self.regions.insert(name.to_owned(), resolved.clone());
        resolved
    }

    fn lookup(&mut self, name: &str) -> Option<ResolvedRegion> {
        let assets = self.assets.clone()?;
        let assets = assets.bind();
        let mut geometry = assets.region_geometry(GString::from(name));
        if geometry.is_empty() && name != ERROR_REGION {
            self.stats.missing_regions += 1;
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
            u0: slice[0] as f32 / width,
            v0: slice[1] as f32 / height,
            u1: (slice[0] + slice[2]) as f32 / width,
            v1: (slice[1] + slice[3]) as f32 / height,
        })
    }
}

/// `cacheChunkLayer` draw decision for one tile/layer (base + overlay only).
fn tile_quad(
    world: &WorldGrid,
    content: &ContentRegistry,
    tile: &Tile,
    layer: CacheLayerId,
) -> Option<PendingQuad> {
    let size = TILESIZE as f32;
    let cx = (tile.x as f32 + 0.5) * size;
    let cy = (tile.y as f32 + 0.5) * size;

    if layer == CacheLayerId::Walls {
        let block = content.block(tile.block)?;
        if block_cache_layer(block) == CacheLayerId::Walls {
            return Some(PendingQuad {
                region: block.region.clone(),
                cx,
                cy,
                w: block.size.max(1) as f32 * size,
                h: block.size.max(1) as f32 * size,
                color: 0xffff_ffff,
            });
        }
        return None;
    }

    // Overlays bake into the normal floor layer (`Floor.drawBase`).
    if layer == CacheLayerId::Normal
        && tile.overlay != BlockId::AIR
        && let Some(overlay) = content.block(tile.overlay)
    {
        return Some(PendingQuad {
            region: overlay.region.clone(),
            cx,
            cy,
            w: size,
            h: size,
            color: 0xffff_ffff,
        });
    }

    let floor = content.block(tile.floor)?;
    if floor_cache_layer(floor) != layer {
        return None;
    }
    let accessible = is_accessible(world, content, tile.x as i32, tile.y as i32);
    let wall_layer = content
        .block(tile.block)
        .is_some_and(|block| block_cache_layer(block) == CacheLayerId::Walls);
    (accessible || !wall_layer).then(|| PendingQuad {
        region: floor.region.clone(),
        cx,
        cy,
        w: size,
        h: size,
        color: 0xffff_ffff,
    })
}

/// The page/UV tuple for a resolved (or placeholder) region.
fn resolved_uv(resolved: &Option<ResolvedRegion>) -> (i32, Option<Gd<Texture2D>>, [f32; 4]) {
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
fn build_mesh(quads: &[Quad], grow: f32) -> Option<Gd<ArrayMesh>> {
    let mut tool = SurfaceTool::new_gd();
    tool.begin(PrimitiveType::TRIANGLES);
    for quad in quads {
        let x = quad.cx - quad.w / 2.0 - grow;
        let y = quad.cy - quad.h / 2.0 - grow;
        let w = quad.w + grow * 2.0;
        let h = quad.h + grow * 2.0;
        let color = color_from(quad.color);
        let corners = [
            (x, y, quad.u0, quad.v0),
            (x + w, y, quad.u1, quad.v0),
            (x + w, y + h, quad.u1, quad.v1),
            (x, y + h, quad.u0, quad.v1),
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
fn color_from(rgba: u32) -> Color {
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

    #[test]
    fn color_from_rgba() {
        let color = color_from(0xff80_4020);
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
}
