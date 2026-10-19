// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Preview pixels (plan 04 §3.9, deviation 7).
//!
//! Ported from `io/MapIO.java` (`generatePreview`, `writeImage`, `colorFor`).
//! `mind-core` only computes the raw pixels (`PreviewImage`); Godot textures
//! and the on-disk cache are plan 19. Block minimap colors come from plan
//! 02's `BlockDef.map_color`; missing colors default deterministically (R9).
//!
//! **Plan-19 addition (OD19-D).** [`encode_png`]/[`decode_png`] are a minimal
//! RGBA8 PNG codec behind [`PreviewImage`], requested by plan 19 (map image
//! import/export + preview files). This is the only plan-04 `io` surface 19
//! adds; it uses the workspace-pinned `png` crate already used by plan 03.

use super::super::save::fixture::creates_building_kind;
use super::super::save::state::{MapSource, WorldContext};
use super::super::wire::WireReader;
use super::super::{IoError, IoResult};
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
    use_colors: Vec<bool>,
    sizes: Vec<i32>,
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
            use_colors: blocks.iter().map(|def| def.use_color).collect(),
            sizes: blocks.iter().map(|def| def.size.max(1)).collect(),
            stone: registry.block_id("stone").map(|id| id.raw()).unwrap_or(0),
        }
    }

    /// `ContentLoader.loadColors`: row 0 of `sprites/block_colors.png` holds
    /// one packed `rgba8888` per block id; `0`/`255` (no colour) are skipped,
    /// otherwise the colour becomes the opaque `mapColor` and `hasColor` is set.
    pub fn apply_block_colors(&mut self, row: &[u32]) {
        for (id, &color) in row.iter().enumerate().take(self.map_colors.len()) {
            if color == 0 || color == 255 {
                continue;
            }
            self.map_colors[id] = Some(color | 0xff);
            if let Some(flag) = self.has_color.get_mut(id) {
                *flag = true;
            }
        }
    }

    /// `Block.size` (1 for unknown/air ids).
    pub fn block_size(&self, id: u16) -> i32 {
        self.get(&self.sizes, id).unwrap_or(1).max(1)
    }

    /// `Block.isMultiblock()`.
    pub fn is_multiblock(&self, id: u16) -> bool {
        self.block_size(id) > 1
    }

    fn get<T: Copy>(&self, table: &[T], id: u16) -> Option<T> {
        table.get(id as usize).copied()
    }

    /// Block kind by id.
    pub fn kind(&self, id: u16) -> Option<BlockKind> {
        self.get(&self.kinds, id)
    }

    /// `Block.mapColor.rgba()` with the deterministic R9 fallback.
    ///
    /// Air keeps Java's default `mapColor` `(0,0,0,1)` — the `black` sentinel
    /// `MapIO.generatePreview`'s `CachedTile.setBlock` skips — instead of the
    /// grey fallback, so air walls never overwrite a floor pixel.
    pub fn map_color(&self, id: u16) -> u32 {
        if id == 0 {
            return BLACK;
        }
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
        // `Block.useColor` (`Block.java:183`; `OverlayFloor`/`AirBlock` false,
        // `OreBlock` true) — read from the registry since J02 T3.
        self.get(&self.use_colors, id).unwrap_or(false)
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
/// metadata; synthetic walls take `team.color` ([`team_color`]).
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

/// Team color (`Team.get(team).color.rgba8888()`, `game/Team.java`): the base
/// teams plus the generated `team#N` palette colours of the 256-team registry.
pub fn team_color(team: u8) -> u32 {
    crate::game::team::Team::get(team).color.to_rgba8888()
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

/// `MapIO.generatePreview`'s scan facts (`Map.spawns` + `Map.teams`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreviewFacts {
    /// Spawn-overlay count (`Map.spawns`).
    pub spawns: u32,
    /// Teams with a core building (`Map.teams`).
    pub teams: std::collections::BTreeSet<u8>,
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
    /// Legacy/v2 tile-entity chunk layout (`set_legacy_entities`).
    legacy_entities: bool,
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
            legacy_entities: false,
        }
    }

    /// The scan facts of this preview (`Map.spawns`/`Map.teams`).
    pub fn facts(&self) -> PreviewFacts {
        PreviewFacts {
            spawns: self.spawns,
            teams: self.teams.clone(),
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

    /// `CachedTile.onReadBuilding` (`MapIO.java:123-141`): the building's
    /// team colour fills its whole `size`×`size` footprint on the wall layer
    /// (`walls.set(drawx, floors.height - 1 - drawy, team.color)`), and a
    /// core's team joins `Map.teams`.
    fn paint_building(&mut self, index: usize, team: u8) {
        let Some(block) = self.block_ids.get(index).copied() else {
            return;
        };
        let width = self.image.width as usize;
        if width == 0 {
            return;
        }
        let color = team_color(team);
        let size = self.palette.block_size(block);
        let offset = -(size - 1) / 2;
        let (tx, ty) = ((index % width) as i32, (index / width) as i32);
        let height = self.image.height as i32;
        for dx in 0..size {
            for dy in 0..size {
                let (x, y) = (tx + dx + offset, ty + dy + offset);
                // `Pixmap.set` ignores out-of-bounds pixels.
                if x < 0 || y < 0 || y >= height {
                    continue;
                }
                self.image.set(x as u32, (height - 1 - y) as u32, color);
            }
        }
        if self.palette.kind(block) == Some(BlockKind::CoreBlock) {
            self.teams.insert(team);
        }
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
        // `CachedTile.setBlock` (`MapIO.java:95-103`): wall colour + floor
        // shading, both on the flipped image row (`floors.height - 1 - y`); the
        // shade goes one row below the wall (`- y + 1`).
        let width = self.image.width as usize;
        let x = (index % width) as u32;
        let y = (index / width) as u32;
        let row = self.image.height - 1 - y;
        let color = color_for(&self.palette, block, 0, 0, 0);
        if color != BLACK && color != 0 {
            self.image.set(x, row, color);
            self.floors.set(x, row + 1, SHADE);
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
        index: usize,
        reader: &mut WireReader,
        version: u8,
    ) -> IoResult<()> {
        // `MapIO.generatePreview`'s `CachedTile.onReadBuilding`: the base
        // carries the team, so a core's team lands in `Map.teams`
        // (`MapIO.java:134-140`); the body is skipped as before.
        let mut decoded = crate::world::building_io::DecodedBase::default();
        if self.legacy_entities {
            crate::world::building_io::read_base_legacy(&mut decoded, reader)?;
        } else {
            crate::world::building_io::read_base(&mut decoded, reader, version)?;
        }
        self.paint_building(index, decoded.team);
        reader.skip_to_end();
        Ok(())
    }

    fn read_building_pre_short(
        &mut self,
        index: usize,
        reader: &mut WireReader,
        content: &ContentRegistry,
        block: BlockId,
    ) -> IoResult<()> {
        // Pre-short `MSAV` v1-3 chunks carry the same core teams (the base
        // holds the team byte); the body is skipped like `read_building`.
        let mut decoded = crate::world::building_io::DecodedBase::default();
        crate::world::building_io::read_base_pre_short(&mut decoded, reader, content, block)?;
        self.paint_building(index, decoded.team);
        reader.skip_to_end();
        Ok(())
    }

    fn set_legacy_entities(&mut self, legacy: bool) {
        // The reader selects this for legacy `MSAV` and native v2 chunks.
        self.legacy_entities = legacy;
    }

    fn on_read_building(&mut self, _index: usize) {}

    fn on_read_tile_data(&mut self, index: usize) {
        // `CachedTile.onReadTileData`: minimap colors for data tiles.
        // Both layers are written on the flipped row (`walls.height - 1 -
        // tile.y`, `MapIO.java:168-178`).
        let block = self.block_ids[index];
        let x = (index % self.image.width as usize) as u32;
        let y = self.image.height - 1 - (index / self.image.width as usize) as u32;
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
    /// Sets a wall/block (and its multiblock footprint) with a team + rotation
    /// (`Tile.setBlock`; plan 19 image-import block path).
    fn set_block(&mut self, x: u16, y: u16, block: BlockId, team: u8, rot: u8);
}

/// `MapIO.readImage`: assigns floors/overlays/blocks from color-mapped pixels.
///
/// Buildings are ignored (image maps are environment-only); unmapped pixels
/// default the floor to `stone` (upstream behavior). The block/wall path is
/// ported from `MapIO.readImage` (`tile.setBlock(block)` /
/// `setBlock(block, Team.derelict, 0)` for multiblocks); the editor invokes it
/// through `MapEditor.beginEdit(Pixmap)`.
pub fn read_image(
    palette: &BlockPalette,
    image: &PreviewImage,
    tiles: &mut dyn ImageTileSink,
    mapper: &dyn ColorMapper,
) -> IoResult<()> {
    let stone = palette.stone;
    let width = tiles.width();
    let height = tiles.height();
    for y in 0..height {
        for x in 0..width {
            let pixel = image.get(u32::from(x), u32::from(height) - 1 - u32::from(y));
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
            } else {
                // `tile.setBlock(block, Team.derelict, 0)`: expand the footprint
                // for multiblocks, otherwise a single wall tile.
                let size = palette.block_size(block.raw());
                let offset = -(size - 1) / 2;
                for dx in 0..size {
                    for dy in 0..size {
                        let tx = x as i32 + offset + dx;
                        let ty = y as i32 + offset + dy;
                        if tx < 0 || ty < 0 || tx >= width as i32 || ty >= height as i32 {
                            continue;
                        }
                        tiles.set_block(tx as u16, ty as u16, block, 0, 0);
                    }
                }
            }
        }
    }
    // Default air floors to stone (upstream final pass). The editor pre-fills
    // the grid with `stone` and the unmapped-pixel branch above writes `stone`,
    // so this pass only affects a caller that supplied air floors; callers
    // without a floor reader are covered by `beginEdit(Pixmap)`'s pre-fill.
    Ok(())
}

/// Encodes a [`PreviewImage`] as an RGBA8 PNG (`MapIO` image export, plan 19).
///
/// The `png` crate is a plan-04 addition requested by 19 (OD19-D); it is the
/// workspace-pinned `=0.17.16` already used by plan 03.
pub fn encode_png(image: &PreviewImage) -> IoResult<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|error| IoError::corrupt(format!("PNG encode header: {error}")))?;
        writer
            .write_image_data(&image.rgba)
            .map_err(|error| IoError::corrupt(format!("PNG encode data: {error}")))?;
    }
    Ok(out)
}

/// Decodes an 8-bit PNG into a [`PreviewImage`] (`MapIO` image import, plan 19).
///
/// RGB and RGBA inputs are accepted; an alpha-less image becomes fully opaque.
/// Indexed/16-bit/grayscale inputs are rejected as unsupported rather than
/// guessed at (the editor only imports colored PNG maps).
pub fn decode_png(bytes: &[u8]) -> IoResult<PreviewImage> {
    let decoder = png::Decoder::new(bytes);
    let mut reader = decoder
        .read_info()
        .map_err(|error| IoError::corrupt(format!("PNG decode info: {error}")))?;
    let mut buffer = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|error| IoError::corrupt(format!("PNG decode frame: {error}")))?;
    if info.bit_depth != png::BitDepth::Eight {
        return Err(IoError::corrupt(format!(
            "unsupported PNG bit depth {:?} (expected 8-bit)",
            info.bit_depth
        )));
    }
    let data = &buffer[..info.buffer_size()];
    let (width, height) = (info.width, info.height);
    let pixel_count = width as usize * height as usize;
    let mut rgba = Vec::with_capacity(pixel_count * 4);
    match info.color_type {
        png::ColorType::Rgba => rgba.extend_from_slice(data),
        png::ColorType::Rgb => {
            for pixel in data.as_chunks::<3>().0 {
                rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 0xff]);
            }
        }
        other => {
            return Err(IoError::corrupt(format!(
                "unsupported PNG color type {other:?} (expected RGB/RGBA)"
            )));
        }
    }
    if rgba.len() != pixel_count * 4 {
        return Err(IoError::corrupt("PNG payload length mismatch"));
    }
    Ok(PreviewImage {
        width,
        height,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;

    #[test]
    fn block_colors_row_overrides_map_color() {
        let registry = test_registry();
        let mut palette = BlockPalette::of(&registry);
        let before = palette.map_color(1);
        palette.apply_block_colors(&[0, 0x11223380, 255]);
        assert_eq!(palette.map_color(1), 0x112233ff);
        assert_ne!(before, 0x112233ff);
        // `0`/`255` entries keep the registry colour.
        assert_eq!(
            palette.map_color(0),
            BlockPalette::of(&registry).map_color(0)
        );
    }

    #[test]
    fn png_roundtrip_preserves_pixels() {
        let mut image = PreviewImage::new(3, 2);
        image.set(0, 0, 0xff0000ff);
        image.set(1, 1, 0x00ff00ff);
        image.set(2, 1, 0x0000ffff);
        let bytes = encode_png(&image).unwrap();
        assert!(bytes.starts_with(&super::super::PNG_SIGNATURE));
        let decoded = decode_png(&bytes).unwrap();
        assert_eq!(decoded, image);
    }

    #[test]
    fn decode_rejects_garbage() {
        assert!(decode_png(b"not a png").is_err());
    }

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

    /// J02 T3: `MapIO.colorFor`'s `overlay.useColor` branch
    /// (`MapIO.java:235`) — an `OreBlock` (`useColor = true`,
    /// `OreBlock.java:23`) contributes its item-coloured `mapColor`; air and
    /// the editor overlays keep `OverlayFloor.useColor = false`
    /// (`OverlayFloor.java:13`) and fall back to the floor colour.
    /// `CachedTile.onReadBuilding` (`MapIO.java:123-141`): a building paints
    /// `team.color` over its whole footprint on the flipped rows (not the
    /// derelict `setBlock` colour) and a core joins `Map.teams`; the data-tile
    /// pass writes on the flipped row too (`MapIO.java:168-178`).
    #[test]
    fn read_building_paints_team_footprint() {
        let registry = test_registry();
        let core = registry.block_id("core-shard").expect("core").raw();
        let wall = registry.block_id("stone-wall").expect("wall").raw();
        let mut ctx = PreviewContext::new(BlockPalette::of(&registry));
        ctx.resize(8, 6);
        let center = ctx.index(3, 2);
        ctx.set_block(center, core);
        ctx.paint_building(center, 1);
        assert!(ctx.teams.contains(&1));
        let image = ctx.image();
        for x in 2..=4 {
            for y in 1..=3u32 {
                assert_eq!(image.get(x, 6 - 1 - y), 0xffd37fff, "({x},{y})");
            }
        }
        assert_eq!(team_color(2), 0xf25555ff);
        assert_eq!(team_color(3), 0xa27ce5ff);

        let data_tile = ctx.index(6, 0);
        ctx.set_block(data_tile, wall);
        ctx.on_read_tile_data(data_tile);
        assert_eq!(ctx.image().get(6, 5), ctx.palette.block_color(wall));
        // Nothing lands on the unflipped row.
        assert_eq!(ctx.image().get(6, 0), 0);
    }

    #[test]
    fn ore_overlays_use_their_map_color() {
        let registry = test_registry();
        let palette = BlockPalette::of(&registry);
        let stone = registry.block_id("stone").unwrap().raw();
        let floor_color = palette.map_color(stone);

        // `ore-scrap`'s `mapColor` is the scrap item colour (grey `777777`).
        let scrap = registry.block_id("ore-scrap").unwrap().raw();
        let scrap_item = registry.item_id("scrap").unwrap();
        let scrap_color = registry.item(scrap_item).unwrap().color.to_rgba8888();
        assert_eq!(palette.map_color(scrap), scrap_color);
        assert_eq!(color_for(&palette, 0, stone, scrap, 0), scrap_color);

        // `ore-copper` -> the copper item colour.
        let copper = registry.block_id("ore-copper").unwrap().raw();
        let copper_item = registry.item_id("copper").unwrap();
        let copper_color = registry.item(copper_item).unwrap().color.to_rgba8888();
        assert_eq!(color_for(&palette, 0, stone, copper, 0), copper_color);

        // Air (no overlay) and `spawn` (an `OverlayFloor`) use the floor.
        assert_eq!(color_for(&palette, 0, stone, 0, 0), floor_color);
        let spawn = registry.block_id("spawn").unwrap().raw();
        assert!(!palette.use_color(spawn));
        assert_eq!(color_for(&palette, 0, stone, spawn, 0), floor_color);

        // A non-`OverlayFloor` overlay keeps the alpha-128 blend
        // (`MapIO.java:236`); `metal-floor` is such a floor-kind overlay.
        let metal = registry.block_id("metal-floor").unwrap().raw();
        assert!(palette.use_color(metal));
        assert!(!palette.is_overlay(metal));
        assert_eq!(
            color_for(&palette, 0, stone, metal, 0),
            blend(
                (palette.map_color(metal) & !0xff) | 128,
                palette.map_color(stone)
            )
        );
    }

    /// Minimal in-memory sink capturing the image-import writes.
    #[derive(Default)]
    struct RecordingSink {
        width: u16,
        height: u16,
        floors: std::collections::BTreeMap<(u16, u16), BlockId>,
        overlays: std::collections::BTreeMap<(u16, u16), BlockId>,
        blocks: std::collections::BTreeMap<(u16, u16), (BlockId, u8, u8)>,
    }

    impl ImageTileSink for RecordingSink {
        fn width(&self) -> u16 {
            self.width
        }
        fn height(&self) -> u16 {
            self.height
        }
        fn set_floor(&mut self, x: u16, y: u16, floor: BlockId) {
            self.floors.insert((x, y), floor);
        }
        fn set_overlay(&mut self, x: u16, y: u16, overlay: BlockId) {
            self.overlays.insert((x, y), overlay);
        }
        fn set_block(&mut self, x: u16, y: u16, block: BlockId, team: u8, rot: u8) {
            self.blocks.insert((x, y), (block, team, rot));
        }
    }

    /// Plan 19 image import block path: a wall-colored pixel writes a block with
    /// `Team.derelict`/rotation 0 (`MapIO.readImage`), not silently dropped.
    #[test]
    fn read_image_places_blocks() {
        let registry = test_registry();
        let palette = BlockPalette::of(&registry);
        // `copper-wall` has a building and is skipped (upstream `hasBuilding`);
        // `cliff` is an environment block and exercises the block path.
        let wall = registry.block_id("cliff").unwrap();
        let stone = registry.block_id("stone").unwrap();
        let wall_color = palette.map_color(wall.raw());
        let stone_color = palette.map_color(stone.raw());

        let mut image = PreviewImage::new(2, 1);
        image.set(0, 0, wall_color);
        image.set(1, 0, stone_color);

        let mapper = FnColorMapper(move |rgba: u32| {
            if rgba == wall_color {
                Some(wall)
            } else if rgba == stone_color {
                Some(stone)
            } else {
                None
            }
        });
        let mut sink = RecordingSink {
            width: 2,
            height: 1,
            ..RecordingSink::default()
        };
        read_image(&palette, &image, &mut sink, &mapper).unwrap();

        assert_eq!(sink.blocks.get(&(0, 0)), Some(&(wall, 0, 0)));
        assert_eq!(sink.floors.get(&(1, 0)), Some(&stone));
    }

    /// `MapIO.generatePreview` end-to-end against the real atlas colours:
    /// air walls must not overwrite floor pixels (air `mapColor` is the black
    /// sentinel), and walls draw on their flipped row (`height - 1 - y`).
    #[test]
    #[cfg(feature = "msav-import")]
    fn map_preview_matches_java_rows_and_colors() {
        use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
        use crate::io::save::SaveIo;
        use crate::io::save::state::SaveReadState;

        let root = format!("{}/../../..", env!("CARGO_MANIFEST_DIR"));
        let colors = std::fs::read(format!("{root}/assets/sprites/block_colors.png"))
            .expect("assets/sprites/block_colors.png");
        let row: Vec<u32> = {
            let image = decode_png(&colors).expect("block_colors.png decodes");
            (0..image.width).map(|x| image.get(x, 0)).collect()
        };
        let mut registry =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
                .expect("content registry");
        let mut palette = BlockPalette::of(&registry);
        palette.apply_block_colors(&row);

        let bytes = std::fs::read(format!("{root}/assets/maps/default/archipelago.msav"))
            .expect("archipelago.msav");
        let mut context = PreviewContext::new(palette);
        let mut state = SaveReadState {
            preview: true,
            context: Some(&mut context),
            content: Some(&mut registry),
            ..SaveReadState::default()
        };
        SaveIo::load_bytes(&bytes, &mut state).expect("archipelago loads");
        let image = context.image();

        // (250,250) deep-water floor, air wall: the floor colour survives
        // (`shipped block_colors`: deep-water `3d4980ff`).
        assert_eq!(image.get(250, 499 - 250), 0x3d4980ff, "deep-water floor");
        // (10,249) grass floor (its northern neighbour is air: no shade).
        assert_eq!(image.get(10, 499 - 249), 0x548449ff, "grass floor");
        // (250,10) shrubs wall on its flipped row (`70bb67ff`).
        assert_eq!(image.get(250, 499 - 10), 0x70bb67ff, "shrubs wall");
    }
}
