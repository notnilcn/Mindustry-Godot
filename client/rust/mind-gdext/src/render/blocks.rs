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
use mind_core::content::{BlockDef, BlockKind};
use mind_core::render::scan::{CameraKey, CameraView, visible_blocks};

use crate::sim_host::MindSimHost;

use super::atlas_bind::{RegionResolver, ResolvedRegion};
use super::units::{UnitQuad, build_unit_mesh};

/// `Pal.accent` (construct-site tint, `Draw.color(Pal.accent, ...)`).
const ACCENT: u32 = 0xffd3_7fff;


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
    /// Construct sites were drawn last frame: their progress animates without
    /// a world-revision bump, so keep rebuilding while any are visible.
    animating: bool,
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
            animating: false,
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
        if !self.animating && self.last_key == Some(key) && self.last_revision == revision {
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

        let mut groups: BTreeMap<i32, (Option<Gd<Texture2D>>, Vec<UnitQuad>)> = BTreeMap::new();
        let mut seen = HashSet::new();
        let mut dynamic = 0i64;
        let mut animating = false;
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
                let w = def.size.max(1) as f32 * size;
                // Java anchors a building sprite at its footprint centre
                // (`Building.x = tile.drawx() + Block.offset`, `Tile.drawx =
                // x*tilesize`); the scan yields the first row-major tile, so odd
                // sizes shift to the centre tile and `Block.offset` covers the
                // even-size half-tile.
                let anchor_x = (tile.x + (def.size - 1) / 2) as f32 * size + def.offset;
                let anchor_y = (tile.y + (def.size - 1) / 2) as f32 * size + def.offset;
                let rotation = host.building_rotation(tile.x, tile.y);
                let mut push = |region: &ResolvedRegion, rot: f32, color: u32, frac: f32| {
                    let entry = groups
                        .entry(region.page)
                        .or_insert((region.texture.clone(), Vec::new()));
                    let [u0, v0, u1, v1] = region.uv;
                    // `frac < 1`: bottom-up reveal (world y-up; v1 is the
                    // sprite's bottom row through the y-flip camera).
                    let h = w * frac;
                    entry.1.push(UnitQuad {
                        cx: anchor_x,
                        cy: anchor_y - (w - h) / 2.0,
                        w,
                        h,
                        rot,
                        uv: [u0, v1 - (v1 - v0) * frac, u1, v1],
                        color,
                    });
                };

                // `ConstructBuild.draw`: the target block's icon through the
                // blockbuild shader, tinted `Pal.accent`. First slice: an
                // accent ghost plus a bottom-up reveal at `progress`.
                if def.kind == BlockKind::ConstructBlock {
                    if let Some((current, progress)) = host.construct_draw(tile.x, tile.y)
                        && let Some(target) = content.block(current)
                        && let Some(region) = self.block_region(target)
                    {
                        let rot = if rotates_sprite(target) {
                            f32::from(rotation) * 90.0
                        } else {
                            0.0
                        };
                        push(&region, rot, (ACCENT & !0xff) | 0x70, 1.0);
                        let progress = progress.clamp(0.0, 1.0);
                        if progress > 0.0 {
                            push(&region, rot, 0xffff_ffff, progress);
                        }
                        animating = true;
                        dynamic += 1;
                    }
                    continue;
                }

                let rot = if rotates_sprite(def) {
                    f32::from(rotation) * 90.0
                } else {
                    0.0
                };
                match self.block_region(def) {
                    Some(region) => push(&region, rot, 0xffff_ffff, 1.0),
                    None => {
                        if let Some(region) = self.resolver.resolve(&def.region) {
                            push(&region, rot, 0xffff_ffff, 1.0);
                        }
                    }
                }
                // `Block.drawTeamOverlay` -> `Building.drawTeam`: the per-team
                // pre-coloured region (`<name>-team-<team>`) over the base
                // sprite, else the `<name>-team` mask tinted with the team
                // colour (`BlockRenderer` draws it after the base).
                if def.draw_team_overlay
                    && let Some(team) = host.building_team(tile.x, tile.y)
                {
                    let team_name = mind_core::game::team::Team::get(team).name.clone();
                    let (resolved, tint) = match self
                        .resolver
                        .resolve(&format!("{}-team-{}", def.name, team_name))
                    {
                        Some(region) => (Some(region), 0xffff_ffff),
                        None => (
                            self.resolver.resolve(&format!("{}-team", def.name)),
                            mind_core::game::team::Team::get(team).color.to_rgba8888(),
                        ),
                    };
                    if let Some(region) = resolved {
                        push(&region, 0.0, tint, 1.0);
                    }
                }
                dynamic += 1;
            }
        }
        self.animating = animating;

        self.apply_groups(groups);
        self.stats.dynamic_sprites = dynamic;
        self.stats.mesh_rebuilds += 1;
        self.stats.missing_regions = self.resolver.missing();
    }

    /// Assigns the grouped page meshes to the pooled nodes, hiding spares.
    /// The in-world sprite for a block: the composed `block-<name>-full`
    /// generated icon (base + top layers, e.g. turret base + barrel, drill
    /// rotator/top) when the generator emitted one, else `Block.region`, else
    /// the `Conveyor.regions[0][0]` frame. `None` when nothing is packed.
    fn block_region(&mut self, def: &BlockDef) -> Option<ResolvedRegion> {
        self.resolver
            .find(&format!("block-{}-full", def.name))
            .or_else(|| self.resolver.find(&def.region))
            .or_else(|| self.resolver.find(&format!("{}-0-0", def.name)))
    }

    fn apply_groups(&mut self, groups: BTreeMap<i32, (Option<Gd<Texture2D>>, Vec<UnitQuad>)>) {
        let mut index = 0usize;
        for (_page, (texture, quads)) in groups {
            if quads.is_empty() {
                continue;
            }
            let Some(mesh) = build_unit_mesh(&quads) else {
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

/// Whether the block's sprite follows `Building.rotation` (`rotate` blocks
/// that draw their region with `rotdeg()`: belts, conduits, ducts).
fn rotates_sprite(def: &BlockDef) -> bool {
    matches!(
        def.kind,
        BlockKind::Conveyor
            | BlockKind::ArmoredConveyor
            | BlockKind::StackConveyor
            | BlockKind::Conduit
            | BlockKind::ArmoredConduit
            | BlockKind::Duct
            | BlockKind::PayloadConveyor
    )
}
