// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BlockRenderer` building-cache pass (`cacheChunk`/`drawCached`, plan 16 §3.6,
//! M4). One `ArrayMesh` per `(chunk, BuildingCacheLayer, atlas page)`, baked from
//! the center tiles of `drawCached` blocks whose `buildingCacheLayer` matches,
//! parented to the `blockUnder`/`block` band nodes. The per-chunk dirty/epoch
//! bookkeeping lives in `mind_core::render::block_cache::BuildingCacheGrid`.
//!
//! The upstream global `SpriteCache` page pool (16382 sprites) becomes
//! per-chunk mesh epochs (plan 16 §3.6 deviation S16-2). Team-overlay baking and
//! the `drawBaseCached` descriptor bodies are owned by plan 07; this module bakes
//! the base region and the (deferred) team overlay is recorded in the Changelog.

use std::collections::BTreeMap;

use godot::classes::{Mesh, MeshInstance2D, Node2D, Texture2D};
use godot::prelude::*;

use mind_core::config::TILESIZE;
use mind_core::render::block_cache::{BUILDING_CACHE_LAYERS, BuildingCacheGrid, CHUNK_SIZE};
use mind_core::render::block_sprite::{CachedBlockState, cached_bake_sprites, crack_index};
use mind_core::render::draw_meta::BlockDrawMeta;
use mind_core::render::layer::BuildingCacheLayer;
use mind_core::render::scan::CameraView;

use crate::assets::MindAssets;
use crate::sim_host::MindSimHost;

use super::atlas_bind::{Quad, RegionResolver, build_mesh};

/// Building-cache counters mirrored into `MindWorldRenderer`'s stats.
#[derive(Clone, Copy, Debug, Default)]
pub struct BuildingCacheStats {
    /// Chunk/layer meshes rebuilt since boot.
    pub mesh_rebuilds: u64,
    /// Cached sprites emitted in the last rebuild.
    pub sprites: i64,
    /// Dirty chunk/layer pairs at the end of the frame.
    pub dirty: i64,
    /// Regions that fell back to `error`/placeholder.
    pub missing_regions: u64,
}

/// The cached-building pass (`under` + `normal` layers).
pub struct BuildingCacheRenderer {
    host: Gd<MindSimHost>,
    /// `BuildingCacheLayer` band nodes in `UNDER, NORMAL` order.
    under_holder: Gd<Node2D>,
    normal_holder: Gd<Node2D>,
    resolver: RegionResolver,
    grid: BuildingCacheGrid,
    /// World dimensions the cache grid was built for; a change (menu world →
    /// loaded sector/map) rebuilds the grid and drops stale cached meshes.
    dims: (i32, i32),
    revision: i64,
    /// Chunk linear index → folded `cached_state_key` sum of its cached
    /// buildings (H03 T18). Config/damage changes never bump
    /// `world_revision`, so a changed sum marks the chunk dirty.
    state_sums: BTreeMap<usize, u64>,
    /// Chunk linear index → `[under, normal]` pooled nodes.
    chunks: BTreeMap<usize, [Vec<Gd<MeshInstance2D>>; BUILDING_CACHE_LAYERS]>,
    stats: BuildingCacheStats,
}

impl BuildingCacheRenderer {
    /// Builds the pass over the `blockUnder`/`block` band nodes.
    pub fn new(
        host: Gd<MindSimHost>,
        assets: Option<Gd<MindAssets>>,
        mut under_band: Gd<Node2D>,
        mut normal_band: Gd<Node2D>,
    ) -> Self {
        let (width, height) = host.bind().world_size();
        // code-instantiated: two fixed holders (not content-driven) keep the
        // cached meshes ordered under the dynamic block band (plan 16 §6.6).
        let under_holder = Node2D::new_alloc();
        under_band.add_child(&under_holder);
        let mut normal_holder = Node2D::new_alloc();
        // Relative z of -1 keeps cached buildings under the dynamic pass drawn
        // directly on the `block` band (upstream draws cached before dynamic).
        normal_holder.set_z_index(-1);
        normal_holder.set_z_as_relative(true);
        normal_band.add_child(&normal_holder);
        Self {
            host,
            under_holder,
            normal_holder,
            resolver: RegionResolver::new(assets),
            grid: BuildingCacheGrid::new(width, height),
            dims: (width, height),
            revision: i64::MIN,
            state_sums: BTreeMap::new(),
            chunks: BTreeMap::new(),
            stats: BuildingCacheStats::default(),
        }
    }

    /// Current counters.
    pub fn stats(&self) -> BuildingCacheStats {
        self.stats
    }

    /// Rebuilds dirty cached chunks overlapping `view`.
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
                    for layer in 0..BUILDING_CACHE_LAYERS {
                        self.grid
                            .recache_building(layer, cx * CHUNK_SIZE, cy * CHUNK_SIZE);
                    }
                }
            }
        }

        let mut sprites = 0i64;
        for cy in 0..self.grid.chunks_y() {
            for cx in 0..self.grid.chunks_x() {
                let aabb = chunk_aabb(cx, cy);
                if !view.overlaps(aabb) {
                    continue;
                }
                // H03 T18: config/damage state does not bump the world
                // revision, so re-bake a visible chunk when its state fold
                // changes — the port counterpart of `BuildingComp.recache()`
                // (`BuildingComp.java:1290-1292`).
                let index = chunk_index(cx, cy, self.grid.chunks_x());
                let sum = self.chunk_state_sum(cx, cy);
                if self.state_sums.insert(index, sum) != Some(sum) {
                    for layer in 0..BUILDING_CACHE_LAYERS {
                        self.grid
                            .recache_building(layer, cx * CHUNK_SIZE, cy * CHUNK_SIZE);
                    }
                }
                for layer in 0..BUILDING_CACHE_LAYERS {
                    if self.grid.is_dirty(layer, cx, cy) {
                        sprites += self.bake_chunk(layer, cx, cy);
                    }
                }
            }
        }
        self.stats.sprites = sprites;
        self.stats.dirty = self.grid.dirty_count() as i64;
        self.stats.missing_regions = self.resolver.missing();
    }

    /// Rebuilds the cache bookkeeping for a new world size and frees every
    /// cached mesh of the previous world (`BuildingCacheRenderer` is
    /// constructed per world upstream; the port keeps one instance).
    fn resize(&mut self, width: i32, height: i32) {
        for mut layers in std::mem::take(&mut self.chunks).into_values() {
            for nodes in &mut layers {
                for node in nodes.iter_mut() {
                    node.queue_free();
                }
            }
        }
        self.dims = (width, height);
        self.grid = BuildingCacheGrid::new(width, height);
        self.state_sums.clear();
        // Force the revision branch to recache every new chunk next frame.
        self.revision = i64::MIN;
    }

    /// Folded `cached_state_key` over the `drawCached` center tiles of one
    /// chunk (H03 T18); order-stable thanks to the row-major scan.
    fn chunk_state_sum(&self, cx: i32, cy: i32) -> u64 {
        let host = self.host.clone();
        let host = host.bind();
        let world = host.grid();
        let Some(content) = host.content_registry() else {
            return 0;
        };
        let mut sum = 0u64;
        for ty in cy * CHUNK_SIZE..cy * CHUNK_SIZE + CHUNK_SIZE {
            for tx in cx * CHUNK_SIZE..cx * CHUNK_SIZE + CHUNK_SIZE {
                if tx < 0 || ty < 0 || tx >= world.width() || ty >= world.height() {
                    continue;
                }
                let tile = world.tile(tx, ty);
                if !tile.is_center_with(tile.pos()) {
                    continue;
                }
                let Some(def) = content.block(tile.block) else {
                    continue;
                };
                if !BlockDrawMeta::from_def(def).draw_cached {
                    continue;
                }
                sum = sum.rotate_left(7) ^ host.cached_state_key(tx, ty);
            }
        }
        sum
    }

    /// Bakes one `(chunk, layer)` and returns the cached sprite count.
    fn bake_chunk(&mut self, layer: usize, cx: i32, cy: i32) -> i64 {
        // Free previous nodes for this chunk/layer.
        if let Some(existing) = self
            .chunks
            .get_mut(&chunk_index(cx, cy, self.grid.chunks_x()))
            && let Some(nodes) = existing.get_mut(layer)
        {
            for node in nodes.iter_mut() {
                node.queue_free();
            }
            nodes.clear();
        }

        let mut groups: BTreeMap<i32, (Option<Gd<Texture2D>>, Vec<Quad>)> = BTreeMap::new();
        let mut count = 0i64;
        {
            let host = self.host.clone();
            let host = host.bind();
            let world = host.grid();
            let Some(content) = host.content_registry() else {
                return 0;
            };
            let size = TILESIZE as f32;
            let x1 = cx * CHUNK_SIZE;
            let y1 = cy * CHUNK_SIZE;
            let x2 = x1 + CHUNK_SIZE;
            let y2 = y1 + CHUNK_SIZE;
            for ty in y1..y2 {
                for tx in x1..x2 {
                    if tx < 0 || ty < 0 || tx >= world.width() || ty >= world.height() {
                        continue;
                    }
                    let tile = world.tile(tx, ty);
                    if !tile.is_center_with(tile.pos()) {
                        continue;
                    }
                    let Some(def) = content.block(tile.block) else {
                        continue;
                    };
                    let meta = BlockDrawMeta::from_def(def);
                    if !meta.draw_cached || layer_of(meta.building_cache_layer) != layer {
                        continue;
                    }
                    // H03 T18: a bake is one or more sprites (`cached_bake_sprites`):
                    // the base region chain plus the per-kind state sprites
                    // (`Door.draw`, `Sorter.draw`, `SwitchBlock.draw`,
                    // `Unloader.draw`).
                    let state = host.block_state_draw(tx, ty);
                    let cached = CachedBlockState {
                        door_open: state.as_ref().is_some_and(|state| state.door_open),
                        enabled: state.as_ref().is_none_or(|state| state.enabled),
                        item: state.as_ref().and_then(|state| {
                            state
                                .sort_item
                                .or(state.unloader_item)
                                .or(state.source_item)
                        }),
                        damaged: state.as_ref().is_some_and(|state| state.damaged),
                        crack: state.as_ref().map_or(0, |state| crack_index(state.healthf)),
                    };
                    let anchor_x = (tx + (def.size - 1) / 2) as f32 * size + def.offset;
                    let anchor_y = (ty + (def.size - 1) / 2) as f32 * size + def.offset;
                    for sprite in
                        cached_bake_sprites(def.kind, &def.name, &def.region, def.size, &cached)
                    {
                        // Same candidate chain as the dynamic pass: `drawCached`
                        // blocks may also lack their bare `Block.region`
                        // (PAR-0152).
                        let resolved = sprite
                            .candidates
                            .iter()
                            .find_map(|name| self.resolver.find(name))
                            .or_else(|| self.resolver.resolve(&def.region));
                        let page = resolved.as_ref().map(|r| r.page).unwrap_or(-1);
                        let uv = resolved
                            .as_ref()
                            .map(|r| r.uv)
                            .unwrap_or([0.0, 0.0, 1.0, 1.0]);
                        // `None` = Java's natural region size (world sprites
                        // pack at 4 px per world unit; H03 T18).
                        let (edge_w, edge_h) = match sprite.size {
                            Some(edge) => (edge, edge),
                            None => resolved
                                .as_ref()
                                .map(|r| (r.w * 0.25, r.h * 0.25))
                                .unwrap_or((TILESIZE as f32, TILESIZE as f32)),
                        };
                        let texture = resolved.and_then(|r| r.texture);
                        // `Draw.color(item.color)` (Sorter/Unloader item square).
                        let color = sprite
                            .item_tint
                            .and_then(|item| content.item(item))
                            .map(|item| item.color.to_rgba8888())
                            .unwrap_or(0xffff_ffff);
                        let entry = groups.entry(page).or_insert((texture, Vec::new()));
                        entry.1.push(Quad {
                            cx: anchor_x,
                            cy: anchor_y,
                            w: edge_w.max(1.0),
                            h: edge_h.max(1.0),
                            uv,
                            color,
                        });
                        count += 1;
                    }
                }
            }
        }

        let holder = if layer == layer_of(BuildingCacheLayer::UNDER) {
            self.under_holder.clone()
        } else {
            self.normal_holder.clone()
        };
        let mut nodes = Vec::new();
        for (_page, (texture, quads)) in groups {
            if quads.is_empty() {
                continue;
            }
            let Some(mesh) = build_mesh(&quads, 0.0, true) else {
                continue;
            };
            let mut node = MeshInstance2D::new_alloc();
            node.set_mesh(&mesh.upcast::<Mesh>());
            if let Some(texture) = texture {
                node.set_texture(&texture);
            }
            // code-instantiated: one mesh per (chunk, BuildingCacheLayer, atlas
            // page); count/geometry are data-driven from the cache bake (§6.6).
            holder.clone().add_child(&node);
            nodes.push(node);
        }

        let slot = self
            .chunks
            .entry(chunk_index(cx, cy, self.grid.chunks_x()))
            .or_default();
        slot[layer] = nodes;
        self.grid.mark_baked(layer, cx, cy);
        self.stats.mesh_rebuilds += 1;
        count
    }
}

/// The `BuildingCacheLayer` index (`UNDER=0`, `NORMAL=1`).
fn layer_of(layer: f32) -> usize {
    if (layer - BuildingCacheLayer::UNDER).abs() < 0.001 {
        0
    } else {
        1
    }
}

fn chunk_index(cx: i32, cy: i32, chunks_x: i32) -> usize {
    (cx + cy * chunks_x) as usize
}

/// Chunk AABB in world pixels (`30 * 8` units plus the half-tile pad).
fn chunk_aabb(cx: i32, cy: i32) -> [f32; 4] {
    let unit = (CHUNK_SIZE * TILESIZE) as f32;
    [
        cx as f32 * unit - 4.0,
        cy as f32 * unit - 4.0,
        unit + 8.0,
        unit + 8.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_indices_match_building_cache_layers() {
        assert_eq!(layer_of(BuildingCacheLayer::UNDER), 0);
        assert_eq!(layer_of(BuildingCacheLayer::NORMAL), 1);
    }

    #[test]
    fn chunk_aabb_is_30_tiles() {
        assert_eq!(chunk_aabb(0, 0), [-4.0, -4.0, 248.0, 248.0]);
    }
}
