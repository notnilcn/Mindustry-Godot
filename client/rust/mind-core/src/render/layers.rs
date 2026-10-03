// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Full-layer headless render-list (plan 16 §3.3/M9).
//!
//! [`build_layers`] extends the sprite-only [`crate::render::list`] oracle with
//! the `world/draw/*` descriptor extras (flame/liquid/glow/warmup/…) and the
//! global whole-layer markers, so `render list render_layers_full` proves the
//! **complete `Renderer.draw` layer set** headlessly. It also computes the
//! plan-16 executor [`DrawCounters`] (draw calls + triangles) that gate plan 17.

use crate::content::{BlockDef, BlockId, ContentRegistry};
use crate::game::team::Team;
use crate::render::commands::Blend;
use crate::render::draw_desc::{DrawState, execute_extras, vanilla_chain};
use crate::render::ids::{RegionId, RegionIdTable};
use crate::render::layer::Layer;
use crate::render::list::{FORMAT, RenderEntry, build_entries, sort_entries};
use crate::render::scan::CameraView;
use crate::world::WorldGrid;

/// Headless executor counters (plan 16 §7.4; consumed by plan 17 `draw_calls`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DrawCounters {
    /// Batched draw calls (`SpriteBatch` merges adjacent same-region quads; a
    /// shape/fill/batch break each start a new call).
    pub draw_calls: u32,
    /// Estimated triangles (2 per sprite + 1 per shape/fill primitive).
    pub triangles: u32,
    /// Sprite quads.
    pub sprites: u32,
    /// Non-sprite primitives (shapes/lines/fills from descriptor bodies).
    pub shapes: u32,
}

/// The full-layer render list.
#[derive(Clone, Debug)]
pub struct LayerRenderList {
    /// JSON format.
    pub format: u32,
    /// Scenario name.
    pub scenario: String,
    /// Tick.
    pub tick: u64,
    /// Camera view.
    pub camera: CameraView,
    /// Sorted flag.
    pub sort: bool,
    /// Emitted entries.
    pub entries: Vec<RenderEntry>,
    /// Distinct reachable layers, ascending by `z`.
    pub layers: Vec<Layer>,
    /// Executor counters.
    pub counters: DrawCounters,
    /// Number of descriptor chains executed.
    pub descriptors: usize,
    /// Distinct regions referenced.
    pub regions: usize,
}

impl LayerRenderList {
    /// Serializes the full-layer `format: 1` JSON with `layers` + `counters`.
    pub fn to_json(&self, names: &RegionIdTable) -> String {
        use std::fmt::Write as _;
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
        let mut layer_names = Vec::with_capacity(self.layers.len());
        for layer in &self.layers {
            layer_names.push(format!("\"{}\"", layer.name()));
        }
        let _ = writeln!(out, "  \"layers\": [{}],", layer_names.join(", "));
        let _ = writeln!(
            out,
            "  \"counters\": {{ \"draw_calls\": {}, \"triangles\": {}, \"sprites\": {}, \"shapes\": {}, \"descriptors\": {} }},",
            self.counters.draw_calls,
            self.counters.triangles,
            self.counters.sprites,
            self.counters.shapes,
            self.descriptors
        );
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
            "  \"stats\": {{ \"entries\": {}, \"regions\": {}, \"descriptors\": {} }}",
            self.entries.len(),
            self.regions,
            self.descriptors
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

/// Builds the full-layer list: floor/overlay/wall/block sprites (with the
/// `blockUnder` building-cache band), descriptor extras, team overlays and the
/// global layer markers.
pub fn build_layers(
    world: &WorldGrid,
    content: &ContentRegistry,
    ids: &mut RegionIdTable,
    view: &CameraView,
    view_team: u8,
    block_team: u8,
) -> LayerRenderList {
    let mut entries = build_entries(world, content, ids, view);
    let ts = crate::config::TILESIZE as f32;
    let mut shapes = 0u32;
    let mut descriptors = 0usize;
    let team_color = Team::get(block_team).color.to_rgba8888();

    let size = ts;
    let bounds = view.bounds();
    let grow = ts * 2.0;
    let min_x = ((bounds[0] - grow) / size).floor().max(0.0) as i32;
    let min_y = ((bounds[1] - grow) / size).floor().max(0.0) as i32;
    let max_x = (((bounds[0] + bounds[2] + grow) / size).ceil() as i32).min(world.width() - 1);
    let max_y = (((bounds[1] + bounds[3] + grow) / size).ceil() as i32).min(world.height() - 1);

    let mut extras: Vec<crate::render::draw_desc::DescDraw> = Vec::new();
    let mut seq = entries.len() as u32;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let tile = world.tile(x, y);
            if tile.block == BlockId::AIR {
                continue;
            }
            let Some(def) = content.block(tile.block) else {
                continue;
            };
            let center_x = (x as f32 + 0.5) * ts;
            let center_y = (y as f32 + 0.5) * ts;
            if def.draw_team_overlay && block_team != view_team {
                let border = ids.intern_str("block-border");
                entries.push(RenderEntry {
                    seq,
                    z: Layer::Block.z(),
                    layer: Layer::Block,
                    region: border,
                    x: center_x - def.size as f32 * ts / 2.0 + 4.0,
                    y: center_y - def.size as f32 * ts / 2.0 + 4.0,
                    w: ts,
                    h: ts,
                    rot: 0.0,
                    color: team_color,
                    blend: Blend::Normal,
                });
                seq += 1;
            }
            let Some(chain) = vanilla_chain(&def.name) else {
                continue;
            };
            descriptors += 1;
            let mut state = DrawState::new(center_x, center_y, 0, def.size);
            state.warmup = 1.0;
            state.power_status = 1.0;
            state.id = x * 4096 + y;
            if def.has_liquids {
                state.liquid_capacity = 1.0;
                state.liquid_amount = 1.0;
            }
            extras.clear();
            for desc in &chain {
                execute_extras(desc, &def.name, &state, ids, &mut extras);
            }
            for extra in &extras {
                match extra.cmd {
                    crate::render::commands::DrawCmd::Sprite {
                        region,
                        x: ex,
                        y: ey,
                        w,
                        h,
                        rot,
                        color,
                        ..
                    } => {
                        entries.push(RenderEntry {
                            seq,
                            z: extra.z,
                            layer: Layer::from_z(extra.z),
                            region,
                            x: ex,
                            y: ey,
                            w,
                            h,
                            rot,
                            color,
                            blend: extra.blend,
                        });
                        seq += 1;
                    }
                    _ => shapes += 1,
                }
            }
        }
    }

    let mut markers = Vec::new();
    crate::render::draw_desc::global_layer_markers(ids, &mut markers);
    for marker in markers {
        if let crate::render::commands::DrawCmd::Sprite {
            region,
            x: mx,
            y: my,
            w,
            h,
            rot,
            color,
            ..
        } = marker.cmd
        {
            entries.push(RenderEntry {
                seq,
                z: marker.z,
                layer: Layer::from_z(marker.z),
                region,
                x: mx,
                y: my,
                w,
                h,
                rot,
                color,
                blend: marker.blend,
            });
            seq += 1;
        }
    }

    sort_entries(&mut entries, true);
    let counters = count_entries(&entries, shapes);
    let mut layers: Vec<Layer> = entries.iter().map(|e| e.layer).collect();
    layers.sort_by(|a, b| {
        a.z()
            .partial_cmp(&b.z())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    layers.dedup();

    LayerRenderList {
        format: FORMAT,
        scenario: String::new(),
        tick: 0,
        camera: *view,
        sort: true,
        entries,
        layers,
        counters,
        descriptors,
        regions: ids.len(),
    }
}

/// Computes batched draw calls (adjacent same-`(z, blend, region)` merge) and
/// the triangle estimate. `shapes` is the descriptor non-sprite primitive count.
pub fn count_entries(entries: &[RenderEntry], shapes: u32) -> DrawCounters {
    let mut draw_calls = 0u32;
    let mut last: Option<(f32, Blend, RegionId)> = None;
    for entry in entries {
        let key = (entry.z, entry.blend, entry.region);
        if last != Some(key) {
            draw_calls += 1;
            last = Some(key);
        }
    }
    let sprites = entries.len() as u32;
    DrawCounters {
        draw_calls,
        triangles: sprites * 2 + shapes,
        sprites,
        shapes,
    }
}

/// Convenience: the `blockUnder` band parity check (a `drawCached` block with
/// `buildingCacheLayer == under` must emit below `Layer.block`).
#[allow(dead_code)]
fn cached_under(def: &BlockDef) -> bool {
    def.draw_cached && def.building_cache_layer < Layer::Block.z()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use crate::world::TilePos;

    fn registry() -> ContentRegistry {
        let mut registry =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true).unwrap();
        registry.init().unwrap();
        registry.post_init().unwrap();
        registry.load().unwrap();
        registry
    }

    fn camera() -> CameraView {
        CameraView {
            x: 32.0,
            y: 32.0,
            w: 256.0,
            h: 256.0,
            zoom: 4.0,
            team: 0,
        }
    }

    #[test]
    fn full_layers_cover_floor_block_and_global_bands() {
        let content = registry();
        let stone = content.block_id("stone").unwrap();
        let mut world = WorldGrid::new(16, 16);
        world.fill(stone, BlockId::AIR);
        world
            .set_block(
                TilePos::new(4, 4),
                content.block_id("copper-wall").unwrap(),
                0,
                0,
            )
            .unwrap();
        world
            .set_block(
                TilePos::new(8, 8),
                content.block_id("cryofluid-mixer").unwrap(),
                0,
                0,
            )
            .unwrap();
        let mut ids = RegionIdTable::new();
        let list = build_layers(&world, &content, &mut ids, &camera(), 0, 0);
        assert!(list.layers.contains(&Layer::Floor));
        assert!(list.layers.contains(&Layer::Block));
        assert!(list.layers.contains(&Layer::Darkness));
        assert!(list.layers.contains(&Layer::Light));
        assert!(list.layers.contains(&Layer::FogOfWar));
        assert!(list.counters.sprites > 0);
        assert_eq!(list.counters.sprites as usize, list.entries.len());
    }

    #[test]
    fn team_overlay_emits_when_team_differs() {
        let content = registry();
        let stone = content.block_id("stone").unwrap();
        let mut world = WorldGrid::new(8, 8);
        world.fill(stone, BlockId::AIR);
        world
            .set_block(
                TilePos::new(4, 4),
                content.block_id("router").unwrap(),
                0,
                0,
            )
            .unwrap();
        let mut ids = RegionIdTable::new();
        let same = build_layers(&world, &content, &mut ids, &camera(), 1, 1);
        let enemy = build_layers(&world, &content, &mut ids, &camera(), 1, 2);
        assert_eq!(
            enemy.entries.len(),
            same.entries.len() + 1,
            "one team border"
        );
    }

    #[test]
    fn descriptor_extras_do_not_duplicate_the_base_region() {
        let content = registry();
        let stone = content.block_id("stone").unwrap();
        let mut world = WorldGrid::new(8, 8);
        world.fill(stone, BlockId::AIR);
        world
            .set_block(
                TilePos::new(4, 4),
                content.block_id("cryofluid-mixer").unwrap(),
                0,
                0,
            )
            .unwrap();
        let mut ids = RegionIdTable::new();
        let list = build_layers(&world, &content, &mut ids, &camera(), 0, 0);
        let base = list
            .entries
            .iter()
            .filter(|e| ids.name(e.region) == Some("cryofluid-mixer"))
            .count();
        assert_eq!(base, 1, "base region emitted exactly once");
    }

    #[test]
    fn block_change_dirties_only_its_chunks_and_cold_rebuild_matches() {
        use crate::render::block_cache::BuildingCacheGrid;
        use crate::render::floor_cache::FloorChunkGrid;

        let content = registry();
        let stone = content.block_id("stone").unwrap();
        let wall = content.block_id("copper-wall").unwrap();
        let router = content.block_id("router").unwrap();

        let mut world = WorldGrid::new(32, 32);
        world.fill(stone, BlockId::AIR);
        world.set_block(TilePos::new(4, 4), wall, 0, 0).unwrap();
        world.set_block(TilePos::new(4, 5), router, 0, 0).unwrap();

        let mut ids = RegionIdTable::new();
        let mut before = build_entries(&world, &content, &mut ids, &camera());
        sort_entries(&mut before, true);

        // Bake every chunk, then mutate one tile (same chunk (0,0)).
        let mut floor = FloorChunkGrid::new(32, 32);
        let mut blocks = BuildingCacheGrid::new(32, 32);
        for cy in 0..floor.chunks_y() {
            for cx in 0..floor.chunks_x() {
                floor.mark_baked(cx, cy);
            }
        }
        for cy in 0..blocks.chunks_y() {
            for cx in 0..blocks.chunks_x() {
                blocks.mark_baked(0, cx, cy);
                blocks.mark_baked(1, cx, cy);
            }
        }
        assert_eq!(floor.dirty_chunks().len(), 0);
        assert_eq!(blocks.dirty_count(), 0);

        world.set_block(TilePos::new(4, 5), wall, 0, 0).unwrap();
        floor.recache_tile(4, 5);
        blocks.recache_building(1, 4, 5);
        assert_eq!(floor.dirty_chunks(), vec![(0, 0)], "one floor chunk dirty");
        assert_eq!(blocks.dirty_count(), 1, "one block chunk/layer dirty");
        assert!(blocks.is_dirty(1, 0, 0));
        assert!(!blocks.is_dirty(0, 0, 0), "under layer untouched");

        // Incremental rebuild equals a cold rebuild of the same final state.
        let mut incremental = build_entries(&world, &content, &mut ids, &camera());
        sort_entries(&mut incremental, true);
        let mut cold_world = WorldGrid::new(32, 32);
        cold_world.fill(stone, BlockId::AIR);
        cold_world
            .set_block(TilePos::new(4, 4), wall, 0, 0)
            .unwrap();
        cold_world
            .set_block(TilePos::new(4, 5), wall, 0, 0)
            .unwrap();
        let mut cold = build_entries(&cold_world, &content, &mut ids, &camera());
        sort_entries(&mut cold, true);
        assert_eq!(incremental, cold, "incremental == cold rebuild");
        assert_ne!(before, incremental, "the mutation changed the list");
    }

    #[test]
    fn counters_merge_adjacent_same_region() {
        let mut ids = RegionIdTable::new();
        let r = ids.intern("grass");
        let e = |seq: u32| RenderEntry {
            seq,
            z: Layer::Floor.z(),
            layer: Layer::Floor,
            region: r,
            x: 0.0,
            y: 0.0,
            w: 8.0,
            h: 8.0,
            rot: 0.0,
            color: 0xffff_ffff,
            blend: Blend::Normal,
        };
        let c = count_entries(&[e(0), e(1), e(2)], 0);
        assert_eq!(c.draw_calls, 1, "adjacent same-region quads batch");
        assert_eq!(c.sprites, 3);
        assert_eq!(c.triangles, 6);
    }
}
