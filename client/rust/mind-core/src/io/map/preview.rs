// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Preview pixels (plan 04 §3.9, deviation 7).
//!
//! Ported from `io/MapIO.java` (`generatePreview`, `writeImage`, `colorFor`).
//! `mind-core` only computes the raw pixels (`PreviewImage`); Godot textures
//! and the on-disk cache are plan 19. Block minimap colors come from plan
//! 02's `BlockDef.map_color`; missing colors default deterministically (R9).

use super::super::IoResult;
use super::super::save::fixture::creates_building_kind;
use super::super::save::state::{MapSource, WorldContext};
use super::super::wire::WireReader;
use crate::content::{BlockId, BlockKind, ContentRegistry};

/// Black (`Color.rgba8888(0,0,0,1)`-ish sentinel used by the preview blend).
const BLACK: u32 = 0x0000_00ff;
/// 50% black shading drawn behind walls (`Color.rgba8888(0,0,0,0.5)`).
const SHADE: u32 = 0x0000_0080;

/// Raw RGBA8888 pixels of a map preview (`Pixmap` equivalent, plan 19 wraps it
/// in a `Texture2D`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewImage {
    /// Width in pixels (tiles).
    pub width: u32,
    /// Height in pixels (tiles).
    pub height: u32,
    /// `width * height * 4` bytes, row-major RGBA8888, **top row first**
    /// (the upstream preview flips y).
    pub rgba: Vec<u8>,
}

impl PreviewImage {
    /// A black image of `width`×`height`.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgba: vec![0u8; width as usize * height as usize * 4],
        }
    }

    /// Sets one pixel (top-left origin).
    pub fn set(&mut self, x: u32, y: u32, rgba8888: u32) {
        if x >= self.width || y >= self.height {
            return;
        }
        let index = ((y * self.width + x) * 4) as usize;
        self.rgba[index..index + 4].copy_from_slice(&rgba8888.to_be_bytes());
    }

    /// One pixel (top-left origin), `0` outside.
    pub fn get(&self, x: u32, y: u32) -> u32 {
        if x >= self.width || y >= self.height {
            return 0;
        }
        let index = ((y * self.width + x) * 4) as usize;
        u32::from_be_bytes([
            self.rgba[index],
            self.rgba[index + 1],
            self.rgba[index + 2],
            self.rgba[index + 3],
        ])
    }
}

/// Deterministic fallback when a block has no `map_color` (R9): opaque dark
/// gray derived from the id (never random, never black).
fn fallback_color(id: u16) -> u32 {
    let v = 0x20u32 + u32::from(id % 48);
    (v << 24) | (v << 16) | (v << 8) | 0xff
}

/// Owned per-id block metadata used by preview rendering.
///
/// Built once from the registry (previews then run without borrowing it, so
/// the save load can hold the registry mutably for the temporary mapper).
#[derive(Debug, Clone)]
pub struct BlockPalette {
    kinds: Vec<BlockKind>,
    map_colors: Vec<Option<u32>>,
    solid: Vec<bool>,
    wall_ore: Vec<bool>,
    has_color: Vec<bool>,
    /// `stone` floor id (0 when absent).
    pub stone: u16,
}

impl BlockPalette {
    /// Snapshots the registry's block metadata.
    pub fn of(registry: &ContentRegistry) -> Self {
        let blocks = registry.blocks();
        Self {
            kinds: blocks.iter().map(|def| def.kind).collect(),
            map_colors: blocks
                .iter()
                .map(|def| def.map_color.map(|c| c.to_rgba8888()))
                .collect(),
            solid: blocks.iter().map(|def| def.solid).collect(),
            wall_ore: blocks.iter().map(|def| def.wall_ore).collect(),
            has_color: blocks.iter().map(|def| def.has_color).collect(),
            stone: registry.block_id("stone").map(|id| id.raw()).unwrap_or(0),
        }
    }

    fn get<T: Copy>(&self, table: &[T], id: u16) -> Option<T> {
        table.get(id as usize).copied()
    }

    /// Block kind by id.
    pub fn kind(&self, id: u16) -> Option<BlockKind> {
        self.get(&self.kinds, id)
    }

    /// `Block.mapColor.rgba()` with the deterministic R9 fallback.
    pub fn map_color(&self, id: u16) -> u32 {
        self.get(&self.map_colors, id)
            .flatten()
            .unwrap_or_else(|| fallback_color(id))
    }

    /// Minimap block color (0 for air).
    pub fn block_color(&self, id: u16) -> u32 {
        if id == 0 {
            return 0;
        }
        self.map_color(id)
    }

    /// `Block.synthetic` approximation (plan 16/19 refine the flag set).
    pub fn synthetic(&self, id: u16) -> bool {
        if id == 0 {
            return false;
        }
        self.kind(id).map(creates_building_kind).unwrap_or(false)
    }

    fn solid(&self, id: u16) -> bool {
        self.get(&self.solid, id).unwrap_or(false)
    }

    fn wall_ore(&self, id: u16) -> bool {
        self.get(&self.wall_ore, id).unwrap_or(false)
    }

    fn use_color(&self, id: u16) -> bool {
        // `Floor.useColor` is not ported yet (plan 06/16); overlays with an
        // explicit map_color behave as colored.
        self.get(&self.has_color, id).unwrap_or(false)
    }

    fn is_overlay(&self, id: u16) -> bool {
        matches!(
            self.kind(id),
            Some(BlockKind::OverlayFloor | BlockKind::OreBlock)
        )
    }

    /// Whether a block id is a floor-like kind for image import.
    pub fn is_floor_like(&self, id: u16) -> bool {
        matches!(
            self.kind(id),
            Some(
                BlockKind::Floor
                    | BlockKind::EmptyFloor
                    | BlockKind::ColoredFloor
                    | BlockKind::ShallowLiquid
            )
        )
    }

    /// Whether a block id is a spawn overlay.
    pub fn is_spawn(&self, id: u16) -> bool {
        matches!(self.kind(id), Some(BlockKind::SpawnBlock))
    }

    /// Whether the block creates a building (`hasBuilding` approximation).
    pub fn creates_building(&self, id: u16) -> bool {
        self.kind(id).map(creates_building_kind).unwrap_or(false)
    }
}

/// `Pixmap.blend` (src-over with src alpha).
fn blend(src: u32, dst: u32) -> u32 {
    let [sr, sg, sb, sa] = src.to_be_bytes();
    let [dr, dg, db, _] = dst.to_be_bytes();
    let a = u32::from(sa);
    let inv = 255 - a;
    let r = (u32::from(sr) * a + u32::from(dr) * inv) / 255;
    let g = (u32::from(sg) * a + u32::from(dg) * inv) / 255;
    let b = (u32::from(sb) * a + u32::from(db) * inv) / 255;
    u32::from_be_bytes([0, 0, 0, 0]) | (r << 24) | (g << 16) | (b << 8) | 0xff
}

/// `MapIO.colorFor(wall, floor, overlay, team)` ported over plan-02 block
/// metadata (team color arrives with plan 12 `Team`; derelict gray for now).
pub fn color_for(palette: &BlockPalette, wall: u16, floor: u16, overlay: u16, team: u8) -> u32 {
    if palette.synthetic(wall) {
        return team_color(team);
    }
    if palette.wall_ore(overlay) {
        return palette.map_color(overlay);
    }
    if palette.solid(wall) {
        return palette.map_color(wall);
    }
    if !palette.use_color(overlay) {
        return palette.map_color(floor);
    }
    if !palette.is_overlay(overlay) {
        return blend(
            (palette.map_color(overlay) & !0xff) | 128,
            palette.map_color(floor),
        );
    }
    palette.map_color(overlay)
}

/// Team color (`Team.<t>.color.rgba8888`); placeholder palette until plan 12
/// owns `Team` (derelict gray + the six base-team hues, deterministic).
pub fn team_color(team: u8) -> u32 {
    match team {
        0 => 0x4d4e58ff,
        1 => 0x2a9df4ff, // sharded blue
        2 => 0xe84c3dff, // crux red
        3 => 0x9b59b6ff, // malis purple
        4 => 0x2ecc71ff, // green
        5 => 0x3498dbff, // blue
        _ => 0xf1c40fff, // neoplastic yellow-ish
    }
}

/// `MapIO.generatePreview(Tiles)`: pixels from a live tile source.
pub fn generate_preview_from_tiles(palette: &BlockPalette, map: &dyn MapSource) -> PreviewImage {
    let width = u32::from(map.width());
    let height = u32::from(map.height());
    let mut image = PreviewImage::new(width, height);
    let len = width as usize * height as usize;
    for i in 0..len {
        let x = (i % width as usize) as u32;
        let y = (i / width as usize) as u32;
        let wall = map.block_id(i);
        let floor = map.floor_id(i);
        let overlay = map.overlay_id(i);
        let mut color = 0u32;
        if !palette.synthetic(wall) && wall != 0 {
            color = palette.block_color(wall);
        } else if overlay == 0 && wall == 0 {
            color = palette.block_color(floor);
        }
        if color == 0 {
            color = color_for(palette, wall, floor, overlay, 0);
        }
        image.set(x, height - 1 - y, color);
    }
    image
}

/// The preview world consumer (`MapIO.generatePreview(Map)` `CachedTile`).
///
/// Drives the standard v1 map read with `preview = true`; pixels + teams +
/// spawns are collected without allocating a world or firing events.
pub struct PreviewContext {
    palette: BlockPalette,
    image: PreviewImage,
    floors: PreviewImage,
    floor_ids: Vec<u16>,
    overlays: Vec<u16>,
    block_ids: Vec<u16>,
    /// Teams with core buildings.
    pub teams: std::collections::BTreeSet<u8>,
    /// Spawn overlay count.
    pub spawns: u32,
    generating: bool,
}

impl PreviewContext {
    /// An empty collector.
    pub fn new(palette: BlockPalette) -> Self {
        Self {
            palette,
            image: PreviewImage::new(0, 0),
            floors: PreviewImage::new(0, 0),
            floor_ids: Vec::new(),
            overlays: Vec::new(),
            block_ids: Vec::new(),
            teams: std::collections::BTreeSet::new(),
            spawns: 0,
            generating: false,
        }
    }

    /// The composed preview (floors + walls), valid after `end()`.
    pub fn image(&self) -> PreviewImage {
        // `floors.draw(walls, true)`: draw walls over floors with alpha.
        let mut out = self.floors.clone();
        for y in 0..out.height {
            for x in 0..out.width {
                let wall = self.image.get(x, y);
                if wall != 0 {
                    out.set(x, y, blend(wall, out.get(x, y)));
                }
            }
        }
        out
    }

    fn index(&self, x: u16, y: u16) -> usize {
        x as usize + y as usize * self.image.width as usize
    }
}

impl WorldContext for PreviewContext {
    fn tile_count(&self) -> usize {
        self.floor_ids.len()
    }

    fn resize(&mut self, width: u16, height: u16) {
        let len = width as usize * height as usize;
        self.image = PreviewImage::new(u32::from(width), u32::from(height));
        self.floors = PreviewImage::new(u32::from(width), u32::from(height));
        self.floor_ids = vec![0; len];
        self.overlays = vec![0; len];
        self.block_ids = vec![0; len];
    }

    fn create(&mut self, x: u16, y: u16, floor: u16, overlay: u16, _wall: u16) {
        let h = self.image.height;
        self.floors.set(
            u32::from(x),
            h - 1 - u32::from(y),
            color_for(&self.palette, 0, floor, overlay, 0),
        );
        let spawn = self.palette.is_spawn(overlay);
        if spawn {
            self.spawns = self.spawns.saturating_add(1);
        }
        let index = self.index(x, y);
        self.floor_ids[index] = floor;
        self.overlays[index] = u16::from(overlay != 0);
    }

    fn is_generating(&self) -> bool {
        self.generating
    }

    fn begin(&mut self) {
        self.generating = true;
    }

    fn end(&mut self) {
        self.generating = false;
    }

    fn set_block(&mut self, index: usize, block: u16) {
        self.block_ids[index] = block;
        // `CachedTile.setBlock`: wall color + floor shading behind it.
        let x = (index % self.image.width as usize) as u32;
        let y = (index / self.image.width as usize) as u32;
        let color = color_for(&self.palette, block, 0, 0, 0);
        if color != BLACK && color != 0 {
            self.image.set(x, y, color);
            self.floors.set(x, y, SHADE);
        }
    }

    fn has_building(&self, _index: usize) -> bool {
        false
    }

    fn block_has_building_io(&self, index: usize) -> bool {
        self.palette.creates_building(self.block_ids[index])
    }

    fn set_tile_data(&mut self, _index: usize, _d: u8, _fd: u8, _od: u8, _ed: i32) {}

    fn read_building(
        &mut self,
        _index: usize,
        reader: &mut WireReader,
        _version: u8,
    ) -> IoResult<()> {
        // Previews skip entity payloads but must consume the chunk.
        reader.skip_to_end();
        Ok(())
    }

    fn on_read_building(&mut self, _index: usize) {}

    fn on_read_tile_data(&mut self, index: usize) {
        // `CachedTile.onReadTileData`: minimap colors for data tiles.
        let block = self.block_ids[index];
        let x = (index % self.image.width as usize) as u32;
        let y = (index / self.image.width as usize) as u32;
        if !self.palette.synthetic(block) && block != 0 {
            let color = self.palette.block_color(block);
            if color != 0 {
                self.image.set(x, y, color);
            }
        } else if self.overlays[index] == 0 && block == 0 {
            let floor = self.floor_ids[index];
            let color = self.palette.block_color(floor);
            if color != 0 {
                self.floors.set(x, y, color);
            }
        }
    }
}

/// `MapIO.writeImage`: color-mapped pixels of the environment (floors and
/// colored non-building blocks); editor import/export format.
pub fn write_image(palette: &BlockPalette, map: &dyn MapSource) -> PreviewImage {
    let width = u32::from(map.width());
    let height = u32::from(map.height());
    let mut image = PreviewImage::new(width, height);
    let len = width as usize * height as usize;
    for i in 0..len {
        let x = (i % width as usize) as u32;
        let y = (i / width as usize) as u32;
        let block = map.block_id(i);
        let colored_non_building = palette.get(&palette.has_color, block).unwrap_or(false)
            && !palette.creates_building(block);
        let color = if colored_non_building {
            palette.map_color(block)
        } else {
            palette.map_color(map.floor_id(i))
        };
        image.set(x, height - 1 - y, color);
    }
    image
}

/// Color-to-block mapping for image maps (`editor.ColorMapper`, plan 19 owns
/// the content-loaded implementation; this is the plan-04 stub seam).
pub trait ColorMapper {
    /// The environment block for one RGBA8888 pixel, if any.
    fn block_for_color(&self, rgba: u32) -> Option<BlockId>;
}

/// Closure-based color mapper (tests + plan-19 adapters).
pub struct FnColorMapper<F: Fn(u32) -> Option<BlockId>>(pub F);

impl<F: Fn(u32) -> Option<BlockId>> ColorMapper for FnColorMapper<F> {
    fn block_for_color(&self, rgba: u32) -> Option<BlockId> {
        (self.0)(rgba)
    }
}

/// The tile sink for image import (the editor's `Tiles`, plan 19; the fixture
/// and tests implement it).
pub trait ImageTileSink {
    /// Width in tiles.
    fn width(&self) -> u16;
    /// Height in tiles.
    fn height(&self) -> u16;
    /// Sets the floor of one tile.
    fn set_floor(&mut self, x: u16, y: u16, floor: BlockId);
    /// Sets the overlay of one tile.
    fn set_overlay(&mut self, x: u16, y: u16, overlay: BlockId);
}

/// `MapIO.readImage`: assigns floors/overlays from color-mapped pixels.
///
/// Buildings are ignored (image maps are environment-only); unmapped pixels
/// default the floor to `stone` (upstream behavior).
pub fn read_image(
    palette: &BlockPalette,
    image: &PreviewImage,
    tiles: &mut dyn ImageTileSink,
    mapper: &dyn ColorMapper,
) -> IoResult<()> {
    let stone = palette.stone;
    for y in 0..tiles.height() {
        for x in 0..tiles.width() {
            let pixel = image.get(u32::from(x), u32::from(tiles.height()) - 1 - u32::from(y));
            let Some(block) = mapper.block_for_color(pixel) else {
                tiles.set_floor(x, y, BlockId::new(stone));
                continue;
            };
            // Skip buildings: image import only targets environment tiles.
            if palette.creates_building(block.raw()) {
                continue;
            }
            if palette.is_overlay(block.raw()) {
                tiles.set_overlay(x, y, block);
            } else if palette.is_floor_like(block.raw()) {
                tiles.set_floor(x, y, block);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;

    #[test]
    fn preview_image_pixels() {
        let mut image = PreviewImage::new(2, 2);
        image.set(0, 0, 0xff0000ff);
        image.set(1, 1, 0x00ff00ff);
        assert_eq!(image.get(0, 0), 0xff0000ff);
        assert_eq!(image.get(1, 1), 0x00ff00ff);
        assert_eq!(image.get(2, 2), 0);
    }

    #[test]
    fn blend_is_src_over() {
        let blended = blend(0xff00_0080, 0x00ff_00ff);
        // 50% red over green: r=128, g=127, a=255.
        assert_eq!(blended, (128 << 24) | (127 << 16) | 0xff);
    }

    #[test]
    fn color_for_defaults_are_deterministic() {
        let registry = test_registry();
        let palette = BlockPalette::of(&registry);
        let a = color_for(&palette, 0, 0, 0, 0);
        let b = color_for(&palette, 0, 0, 0, 0);
        assert_eq!(a, b);
        // Synthetic walls take the team color (upstream `wall.synthetic()`).
        let conveyor = registry.block_id("conveyor").unwrap().raw();
        assert!(palette.synthetic(conveyor));
        assert_eq!(color_for(&palette, conveyor, 0, 0, 1), team_color(1));
    }
}
