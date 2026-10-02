// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Headless render-list extraction (plan 16 §6.3/§7.2).
//!
//! Builds the deterministic [`RenderList`] the oracle diffs against committed
//! goldens. This is the Godot-free twin of the in-engine band replay: it walks
//! the same visible tiles, emits the same `(z, layer, region)` sprites and
//! reports the same chunk dirty/built stats.

use std::fmt::Write as _;

use crate::config::TILESIZE;
use crate::content::{BlockDef, BlockId, BlockKind, ContentRegistry};
use crate::render::commands::Blend;
use crate::render::ids::{RegionId, RegionIdTable};
use crate::render::layer::{CacheLayerId, Layer};
use crate::render::scan::CameraView;
use crate::world::WorldGrid;
use crate::world::tile::is_static_kind;

/// Render-list JSON format version.
pub const FORMAT: u32 = 1;

/// One render-list entry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderEntry {
    /// Emission sequence.
    pub seq: u32,
    /// Layer z.
    pub z: f32,
    /// Logical layer.
    pub layer: Layer,
    /// Atlas region.
    pub region: RegionId,
    /// Center x in world pixels.
    pub x: f32,
    /// Center y in world pixels.
    pub y: f32,
    /// Quad width.
    pub w: f32,
    /// Quad height.
    pub h: f32,
    /// Rotation in degrees.
    pub rot: f32,
    /// Tint (RGBA8888).
    pub color: u32,
    /// Blend mode.
    pub blend: Blend,
}

/// The extracted render list.
#[derive(Clone, Debug)]
pub struct RenderList {
    /// JSON format.
    pub format: u32,
    /// Scenario name.
    pub scenario: String,
    /// Tick the list was captured at.
    pub tick: u64,
    /// Camera view.
    pub camera: CameraView,
    /// Whether entries are sorted by `(z, seq)`.
    pub sort: bool,
    /// Emitted entries.
    pub entries: Vec<RenderEntry>,
    /// Dirty floor chunks `(cx, cy)`.
    pub floor_dirty: Vec<(i32, i32)>,
    /// Baked floor chunk count.
    pub floor_built: usize,
    /// Floor mesh epoch.
    pub floor_epoch: u64,
    /// Dirty block chunks `(cx, cy)`.
    pub block_dirty: Vec<(i32, i32)>,
    /// Baked block chunk count.
    pub block_built: usize,
    /// Block mesh epoch.
    pub block_epoch: u64,
    /// Distinct regions referenced.
    pub regions: usize,
}

/// The cache layer a floor bakes into.
pub fn floor_cache_layer(def: &BlockDef) -> CacheLayerId {
    match def.kind {
        BlockKind::ShallowLiquid => CacheLayerId::Water,
        _ => CacheLayerId::Normal,
    }
}

/// The cache layer a wall/block bakes into (`Block.cacheLayer`).
pub fn block_cache_layer(def: &BlockDef) -> CacheLayerId {
    if is_static_kind(def.kind) {
        CacheLayerId::Walls
    } else {
        CacheLayerId::Normal
    }
}

/// Whether a tile is accessible for the floor-accessibility rule
/// (`World.isAccessible` approximation: solid full walls block access).
pub fn is_accessible(world: &WorldGrid, content: &ContentRegistry, x: i32, y: i32) -> bool {
    let Some(def) = content.block(world.tile(x, y).block) else {
        return true;
    };
    !def.solid || !is_static_kind(def.kind)
}

/// Builds the ordered entries for the world, culled to the camera grow(2 tiles)
/// exactly like `BlockRenderer.processBlocks`' visible range.
pub fn build_entries(
    world: &WorldGrid,
    content: &ContentRegistry,
    ids: &mut RegionIdTable,
    view: &CameraView,
) -> Vec<RenderEntry> {
    let mut entries = Vec::new();
    let size = TILESIZE as f32;
    let grow = size * 2.0;
    let bounds = view.bounds();
    let min_x = ((bounds[0] - grow) / size).floor().max(0.0) as i32;
    let min_y = ((bounds[1] - grow) / size).floor().max(0.0) as i32;
    let max_x = (((bounds[0] + bounds[2] + grow) / size).ceil() as i32).min(world.width() - 1);
    let max_y = (((bounds[1] + bounds[3] + grow) / size).ceil() as i32).min(world.height() - 1);

    let mut seq = 0u32;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let tile = world.tile(x, y);
            let center_x = (x as f32 + 0.5) * size;
            let center_y = (y as f32 + 0.5) * size;

            // Floors + overlays bake into the floor pass (`air` is empty).
            if tile.floor != BlockId::AIR
                && let Some(def) = content.block(tile.floor)
            {
                let region = ids.intern(def.region.clone());
                entries.push(RenderEntry {
                    seq,
                    z: Layer::Floor.z(),
                    layer: Layer::Floor,
                    region,
                    x: center_x,
                    y: center_y,
                    w: size,
                    h: size,
                    rot: 0.0,
                    color: 0xffff_ffff,
                    blend: Blend::Normal,
                });
                seq += 1;
            }

            if tile.overlay != BlockId::AIR
                && let Some(def) = content.block(tile.overlay)
            {
                let region = ids.intern(def.region.clone());
                entries.push(RenderEntry {
                    seq,
                    z: Layer::Floor.z(),
                    layer: Layer::Floor,
                    region,
                    x: center_x,
                    y: center_y,
                    w: size,
                    h: size,
                    rot: 0.0,
                    color: 0xffff_ffff,
                    blend: Blend::Normal,
                });
                seq += 1;
            }

            // Walls bake into the floor pass; everything else draws at Layer.block.
            if tile.block != BlockId::AIR
                && let Some(def) = content.block(tile.block)
            {
                let (layer, z) = if is_static_kind(def.kind) {
                    (Layer::Floor, Layer::Floor.z())
                } else {
                    (Layer::Block, Layer::Block.z())
                };
                let region = ids.intern(def.region.clone());
                let w = def.size as f32 * size;
                let offset = def.offset;
                entries.push(RenderEntry {
                    seq,
                    z,
                    layer,
                    region,
                    x: center_x + offset,
                    y: center_y + offset,
                    w,
                    h: w,
                    rot: 0.0,
                    color: 0xffff_ffff,
                    blend: Blend::Normal,
                });
                seq += 1;
            }
        }
    }
    entries
}

/// Sorts entries by `(z, seq)` when `sort` is true.
pub fn sort_entries(entries: &mut [RenderEntry], sort: bool) {
    if sort {
        entries.sort_by(|a, b| {
            a.z.partial_cmp(&b.z)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.seq.cmp(&b.seq))
        });
    }
}

impl RenderList {
    /// Serializes as the canonical `format: 1` JSON.
    pub fn to_json(&self, names: &RegionIdTable) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "{{");
        let _ = writeln!(out, "  \"format\": {},", self.format);
        let _ = writeln!(out, "  \"scenario\": \"{}\",", self.scenario);
        let _ = writeln!(out, "  \"tick\": {},", self.tick);
        let _ = writeln!(
            out,
            "  \"camera\": {{ \"x\": {:.3}, \"y\": {:.3}, \"w\": {:.3}, \"h\": {:.3}, \"zoom\": {:.3}, \"team\": {} }},",
            self.camera.x,
            self.camera.y,
            self.camera.w,
            self.camera.h,
            self.camera.zoom,
            self.camera.team
        );
        let _ = writeln!(out, "  \"sort\": {},", self.sort);
        let _ = writeln!(out, "  \"entries\": [");
        for (i, entry) in self.entries.iter().enumerate() {
            let comma = if i + 1 == self.entries.len() { "" } else { "," };
            let name = names.name(entry.region).unwrap_or("error");
            let _ = writeln!(
                out,
                "    {{ \"seq\": {}, \"z\": {:.3}, \"layer\": \"{}\", \"level\": \"sprites\", \"region\": \"{}\", \"x\": {:.3}, \"y\": {:.3}, \"w\": {:.3}, \"h\": {:.3}, \"rot\": {:.3}, \"color\": \"{:08x}\", \"blend\": \"{}\" }}{}",
                entry.seq,
                entry.z,
                entry.layer.name(),
                name,
                entry.x,
                entry.y,
                entry.w,
                entry.h,
                entry.rot,
                entry.color,
                blend_name(entry.blend),
                comma
            );
        }
        let _ = writeln!(out, "  ],");
        let _ = writeln!(
            out,
            "  \"chunks\": {{ \"floor\": {{ \"dirty\": {}, \"built\": {}, \"epoch\": {} }}, \"blocks\": {{ \"dirty\": {}, \"built\": {}, \"epoch\": {} }} }},",
            json_pairs(&self.floor_dirty),
            self.floor_built,
            self.floor_epoch,
            json_pairs(&self.block_dirty),
            self.block_built,
            self.block_epoch
        );
        let _ = writeln!(
            out,
            "  \"stats\": {{ \"entries\": {}, \"regions\": {}, \"queue_max\": {} }}",
            self.entries.len(),
            self.regions,
            self.entries.len()
        );
        out.push_str("}\n");
        out
    }
}

fn blend_name(blend: Blend) -> &'static str {
    match blend {
        Blend::Normal => "normal",
        Blend::Additive => "additive",
        Blend::Multiply => "multiply",
        Blend::Disabled => "disabled",
    }
}

fn json_pairs(pairs: &[(i32, i32)]) -> String {
    let mut out = String::from("[");
    for (i, (x, y)) in pairs.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        let _ = write!(out, "[{}, {}]", x, y);
    }
    out.push(']');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};

    fn registry() -> ContentRegistry {
        let mut registry =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true).unwrap();
        registry.init().unwrap();
        registry.post_init().unwrap();
        registry.load().unwrap();
        registry
    }

    fn stone_id(content: &ContentRegistry) -> crate::content::BlockId {
        content.block_id("stone").expect("stone floor")
    }

    #[test]
    fn flat_floor_emits_one_entry_per_visible_tile() {
        let content = registry();
        let mut world = WorldGrid::new(16, 16);
        world.fill(stone_id(&content), crate::content::BlockId::AIR);
        let mut ids = RegionIdTable::new();
        let view = CameraView {
            x: 64.0,
            y: 64.0,
            w: 320.0,
            h: 180.0,
            zoom: 1.0,
            team: 0,
        };
        let entries = build_entries(&world, &content, &mut ids, &view);
        assert!(!entries.is_empty());
        assert!(entries.iter().all(|e| e.layer == Layer::Floor));
        assert_eq!(entries.len() as i32, 16 * 16);
    }

    #[test]
    fn block_change_adds_block_layer_entry() {
        let content = registry();
        let wall = content.block_id("copper-wall").expect("copper-wall");
        let mut world = WorldGrid::new(16, 16);
        world.fill(stone_id(&content), crate::content::BlockId::AIR);
        world
            .set_block(crate::world::TilePos::new(4, 4), wall, 0, 0)
            .unwrap();
        let mut ids = RegionIdTable::new();
        let view = CameraView {
            x: 64.0,
            y: 64.0,
            w: 320.0,
            h: 180.0,
            zoom: 1.0,
            team: 0,
        };
        let entries = build_entries(&world, &content, &mut ids, &view);
        assert!(
            entries
                .iter()
                .any(|e| e.layer == Layer::Block && e.z == Layer::Block.z())
        );
    }

    #[test]
    fn json_deterministic_and_sorted() {
        let content = registry();
        let mut world = WorldGrid::new(8, 8);
        world.fill(stone_id(&content), crate::content::BlockId::AIR);
        let mut ids = RegionIdTable::new();
        let view = CameraView {
            x: 32.0,
            y: 32.0,
            w: 320.0,
            h: 180.0,
            zoom: 1.0,
            team: 0,
        };
        let mut entries = build_entries(&world, &content, &mut ids, &view);
        sort_entries(&mut entries, true);
        let list = RenderList {
            format: FORMAT,
            scenario: String::from("render_flat_floor"),
            tick: 0,
            camera: view,
            sort: true,
            entries,
            floor_dirty: Vec::new(),
            floor_built: 9,
            floor_epoch: 1,
            block_dirty: Vec::new(),
            block_built: 0,
            block_epoch: 0,
            regions: ids.len(),
        };
        let a = list.to_json(&ids);
        let b = list.to_json(&ids);
        assert_eq!(a, b);
        assert!(a.contains("\"scenario\": \"render_flat_floor\""));
    }
}
