// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BlockRenderer` — dynamic block draws (plan 16 §3.6, M2 slice).
//!
//! The M2 slice rebuilds a per-atlas-page `ArrayMesh` of the visible dynamic
//! (non-static) blocks and assigns it to a pooled `MeshInstance2D` under the
//! `Layer.block` band. Cached buildings (`drawCached`), cracks, team overlays,
//! status bars, shadows and darkness land in M3/M4. The deterministic visible
//! set comes from `mind_core::render::scan::visible_blocks`; this module owns
//! only the Godot mesh build and the `processBlocks`-style early-out.

use std::collections::{BTreeMap, HashSet};

use godot::classes::{Mesh, MeshInstance2D, Node2D, Texture2D};
use godot::prelude::*;

use mind_core::config::TILESIZE;
use mind_core::render::scan::{CameraKey, CameraView, visible_blocks};

use crate::sim_host::MindSimHost;

use super::atlas_bind::{Quad, RegionResolver, build_mesh};

/// `BlockRenderer.growSprites` for dynamic sprites is off; 0 padding.
const GROW: f32 = 0.0;

/// Block-renderer counters mirrored into `MindWorldRenderer`'s stats.
#[derive(Clone, Copy, Debug, Default)]
pub struct BlockStats {
    /// Dynamic sprites emitted this frame.
    pub dynamic_sprites: i64,
    /// Mesh rebuilds since boot.
    pub mesh_rebuilds: u64,
    /// Regions that fell back to `error`/placeholder.
    pub missing_regions: u64,
}

/// Dynamic block draw pass.
pub struct BlockRenderer {
    host: Gd<MindSimHost>,
    band_node: Gd<Node2D>,
    resolver: RegionResolver,
    /// Pooled per-page nodes (index → node); reused across rebuilds.
    nodes: Vec<Gd<MeshInstance2D>>,
    last_key: Option<CameraKey>,
    last_revision: i64,
    stats: BlockStats,
}

impl BlockRenderer {
    /// Builds the renderer over the `Layer.block` band node.
    pub fn new(
        host: Gd<MindSimHost>,
        assets: Option<Gd<crate::assets::MindAssets>>,
        band_node: Gd<Node2D>,
    ) -> Self {
        Self {
            host,
            band_node,
            resolver: RegionResolver::new(assets),
            nodes: Vec::new(),
            last_key: None,
            last_revision: i64::MIN,
            stats: BlockStats::default(),
        }
    }

    /// Current counters.
    pub fn stats(&self) -> BlockStats {
        self.stats
    }

    /// Whether the visible set or the world changed (early-out, §3.6).
    fn needs_rebuild(&mut self, view: &CameraView) -> bool {
        let revision = self.host.clone().bind().world_revision();
        let key = CameraKey::from_view(view, TILESIZE as f32);
        if self.last_key == Some(key) && self.last_revision == revision {
            return false;
        }
        self.last_key = Some(key);
        self.last_revision = revision;
        true
    }

    /// Rebuilds the dynamic block meshes when the visible set changed.
    pub fn update(&mut self, view: &CameraView) {
        if !self.needs_rebuild(view) {
            return;
        }

        let mut groups: BTreeMap<i32, (Option<Gd<Texture2D>>, Vec<Quad>)> = BTreeMap::new();
        let mut seen = HashSet::new();
        let mut dynamic = 0i64;
        {
            let host = self.host.clone();
            let host = host.bind();
            let world = host.grid();
            let Some(content) = host.content_registry() else {
                return;
            };
            let size = TILESIZE as f32;
            for tile in visible_blocks(world, content, view) {
                let Some(def) = content.block(tile.block) else {
                    continue;
                };
                // Multiblock: draw once per center entity (first row-major tile).
                if let Some(entity) = world.tile(tile.x, tile.y).build
                    && !seen.insert(entity)
                {
                    continue;
                }
                let resolved = self.resolver.resolve(&def.region);
                let page = resolved.as_ref().map(|r| r.page).unwrap_or(-1);
                let uv = resolved
                    .as_ref()
                    .map(|r| r.uv)
                    .unwrap_or([0.0, 0.0, 1.0, 1.0]);
                let texture = resolved.and_then(|r| r.texture);
                let entry = groups.entry(page).or_insert((texture, Vec::new()));
                let w = def.size.max(1) as f32 * size;
                entry.1.push(Quad {
                    cx: (tile.x as f32 + 0.5) * size + def.offset,
                    cy: (tile.y as f32 + 0.5) * size + def.offset,
                    w,
                    h: w,
                    uv,
                    color: 0xffff_ffff,
                });
                dynamic += 1;
            }
        }

        self.apply_groups(groups);
        self.stats.dynamic_sprites = dynamic;
        self.stats.mesh_rebuilds += 1;
        self.stats.missing_regions = self.resolver.missing();
    }

    /// Assigns the grouped page meshes to the pooled nodes, hiding spares.
    fn apply_groups(&mut self, groups: BTreeMap<i32, (Option<Gd<Texture2D>>, Vec<Quad>)>) {
        let mut index = 0usize;
        for (_page, (texture, quads)) in groups {
            if quads.is_empty() {
                continue;
            }
            let Some(mesh) = build_mesh(&quads, GROW) else {
                continue;
            };
            let node = if index < self.nodes.len() {
                self.nodes[index].clone()
            } else {
                let node = MeshInstance2D::new_alloc();
                // code-instantiated: one pooled node per atlas page; count is
                // page-driven (plan 16 §6.6).
                self.band_node.clone().add_child(&node);
                self.nodes.push(node.clone());
                node
            };
            let mut node = node;
            node.set_mesh(&mesh.upcast::<Mesh>());
            if let Some(texture) = texture {
                node.set_texture(&texture);
            }
            node.set_visible(true);
            index += 1;
        }
        for node in self.nodes.iter_mut().skip(index) {
            node.set_visible(false);
        }
    }
}
