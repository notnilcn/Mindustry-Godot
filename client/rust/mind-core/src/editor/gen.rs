// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Generation preview and filter application (plan 19 §3.9;
//! `editor/MapGenerateDialog.java`).
//!
//! Upstream runs `MapGenerateDialog.update()` on `mainExecutor` while reading
//! the live editor tiles. The port replaces that with an owned [`EditorSnapshot`]
//! (plan 19 §2.3.4): filters are applied to the packed snapshot, and the result
//! is a deterministic [`PreviewImage`]. `apply_to_editor` mirrors
//! `MapGenerateDialog.applyToEditor` write-through semantics and then clears the
//! undo stack (generation invalidates it).

use crate::content::ContentRegistry;
use crate::determinism::{RngStream, SimRng};
use crate::io::IoResult;
use crate::io::map::{BlockPalette, PreviewImage, generate_preview_from_tiles};
use crate::io::save::state::MapSource;
use crate::io::wire::WireWriter;
use crate::maps::filters::{GenerateFilter, GenerateInput, PackedState};
use crate::world::WorldGrid;
use crate::world::tiles::Tiles;

use super::MapEditor;

/// One filter result per tile, packed exactly like upstream `PackTile`:
/// `(block, floor, overlay)` plus the private `packedData` long.
pub type PackedTile = PackedState;

/// An owned copy of the editor tile state used for asynchronous previews.
///
/// A worker never touches the live `WorldGrid`; it reads this snapshot only.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorSnapshot {
    /// Grid width in tiles.
    pub width: i32,
    /// Grid height in tiles.
    pub height: i32,
    /// Row-major packed tiles.
    pub tiles: Vec<PackedTile>,
}

impl EditorSnapshot {
    /// Captures every tile of a live grid (`editor.tile(x, y)` reads).
    pub fn capture(grid: &WorldGrid) -> Self {
        Self::from_tiles(&grid.tiles)
    }

    /// Captures a [`Tiles`] buffer.
    pub fn from_tiles(tiles: &Tiles) -> Self {
        let mut packed = Vec::with_capacity(tiles.len());
        for index in 0..tiles.len() {
            let tile = tiles.geti(index);
            packed.push(PackedState {
                block: tile.block,
                floor: tile.floor,
                overlay: tile.overlay,
                packed_data: tile.get_packed_data(),
            });
        }
        Self {
            width: tiles.width,
            height: tiles.height,
            tiles: packed,
        }
    }

    /// Number of packed tiles.
    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    /// Whether the snapshot is empty.
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// Reads one tile (clamped to the snapshot bounds).
    pub fn get(&self, x: i32, y: i32) -> PackedTile {
        if self.tiles.is_empty() || self.width <= 0 || self.height <= 0 {
            return PackedTile::default();
        }
        let cx = x.clamp(0, self.width - 1);
        let cy = y.clamp(0, self.height - 1);
        self.tiles[(cx + cy * self.width) as usize]
    }

    /// Builds a fresh [`Tiles`] buffer from the snapshot.
    pub fn to_tiles(&self) -> Tiles {
        let mut tiles = Tiles::new(self.width.max(0), self.height.max(0));
        for (index, packed) in self.tiles.iter().enumerate() {
            let tile = tiles.geti_mut(index);
            tile.block = packed.block;
            tile.floor = packed.floor;
            tile.overlay = packed.overlay;
            tile.set_packed_data(packed.packed_data);
        }
        tiles
    }
}

/// [`MapSource`] over a [`Tiles`] buffer (preview pixels from generated tiles).
pub struct TilesMapSource<'a> {
    /// The tile buffer.
    pub tiles: &'a Tiles,
    /// Content registry (`shouldSaveData` classification).
    pub content: &'a ContentRegistry,
}

impl MapSource for TilesMapSource<'_> {
    fn width(&self) -> u16 {
        self.tiles.width as u16
    }

    fn height(&self) -> u16 {
        self.tiles.height as u16
    }

    fn floor_id(&self, index: usize) -> u16 {
        self.tiles.geti(index).floor.raw()
    }

    fn overlay_id(&self, index: usize) -> u16 {
        self.tiles.geti(index).overlay.raw()
    }

    fn block_id(&self, index: usize) -> u16 {
        self.tiles.geti(index).block.raw()
    }

    fn has_building(&self, index: usize) -> bool {
        self.tiles.geti(index).build.is_some()
    }

    fn is_center(&self, index: usize) -> bool {
        self.tiles.geti(index).build.is_none()
    }

    fn should_save_data(&self, index: usize) -> bool {
        self.tiles.geti(index).should_save_data(self.content)
    }

    fn tile_data(&self, index: usize) -> (u8, u8, u8, i32) {
        let tile = self.tiles.geti(index);
        (
            tile.data as u8,
            tile.floor_data as u8,
            tile.overlay_data as u8,
            tile.extra_data,
        )
    }

    fn write_building(&self, _index: usize, _chunk: &mut WireWriter) -> IoResult<()> {
        Ok(())
    }
}

/// Renders a deterministic [`PreviewImage`] from a snapshot + filter stack.
///
/// `filters` are applied with their **existing seeds** (no `randomize`), so the
/// output is a pure function of the input — the plan-19 `editor gen-preview`
/// oracle. Each filter writes through to the working buffer (upstream
/// `applyToEditor` semantics), then the packed tiles are color-mapped with
/// `Team.derelict` (`0`).
pub fn generate_preview(
    snapshot: &EditorSnapshot,
    filters: &mut [Box<dyn GenerateFilter>],
    content: &ContentRegistry,
    seed: u64,
) -> PreviewImage {
    let mut tiles = snapshot.to_tiles();
    for filter in filters.iter_mut() {
        apply_filter_write_through(&mut tiles, filter.as_mut(), content, seed);
    }
    let palette = BlockPalette::of(content);
    let source = TilesMapSource {
        tiles: &tiles,
        content,
    };
    generate_preview_from_tiles(&palette, &source)
}

/// Applies every filter to the live editor grid and clears the undo stack
/// (`MapGenerateDialog.applyToEditor`).
///
/// The editor's `tags["genfilters"]` is the caller's responsibility; this only
/// mutates tiles via write-through semantics.
pub fn apply_filters_to_editor(
    editor: &mut MapEditor,
    grid: &mut WorldGrid,
    content: &ContentRegistry,
    filters: &mut [Box<dyn GenerateFilter>],
    seed: u64,
) {
    for filter in filters.iter_mut() {
        apply_filter_write_through(&mut grid.tiles, filter.as_mut(), content, seed);
    }
    // Generation invalidates the undo stack (upstream `editor.clearOp()`).
    editor.clear_op();
}

/// One filter applied write-through over a mutable tile buffer.
///
/// Mirrors `applyToEditor`: read the current tile, run `filter.apply`, then write
/// floor/overlay/packed-data back and replace the block only when neither the
/// previous nor the new block is synthetic.
fn apply_filter_write_through(
    tiles: &mut Tiles,
    filter: &mut dyn GenerateFilter,
    content: &ContentRegistry,
    seed: u64,
) {
    let width = tiles.width;
    let height = tiles.height;
    let n = (width * height) as usize;
    let mut input = GenerateInput {
        content: Some(content),
        ..GenerateInput::default()
    };
    input.begin(width, height);
    input.states.reserve(n);
    for index in 0..n {
        let tile = tiles.geti(index);
        input.states.push(PackedState {
            block: tile.block,
            floor: tile.floor,
            overlay: tile.overlay,
            packed_data: tile.get_packed_data(),
        });
    }

    let mut rng = SimRng::new(seed);
    for index in 0..n {
        let x = (index as i32) % width;
        let y = (index as i32) / width;
        input.set_state(input.states[index], x, y);
        filter.apply(&mut input);
        let out = input.out_state();
        input.states[index] = out;
        apply_editor_result(tiles, content, index, out);
    }
    // The rng is threaded consistently with `apply_stack`; filters that ignore it
    // stay deterministic.
    let _ = rng.random(RngStream::MapGen, 1);
}

/// Writes one generated tile back (`applyToEditor` inner loop).
fn apply_editor_result(
    tiles: &mut Tiles,
    content: &ContentRegistry,
    index: usize,
    out: PackedState,
) {
    let old_block = tiles.geti(index).block;
    let old_synthetic = content
        .block(old_block)
        .is_some_and(crate::maps::filters::block_info::synthetic);
    let new_synthetic = content
        .block(out.block)
        .is_some_and(crate::maps::filters::block_info::synthetic);
    let tile = tiles.geti_mut(index);
    if !old_synthetic && !new_synthetic {
        tile.block = out.block;
    }
    tile.set_packed_data(out.packed_data);
    tile.floor = out.floor;
    tile.overlay = out.overlay;
}

/// Reads a `genfilters` tag and parses it into a filter stack, defaulting to the
/// upstream stack when the tag is missing (plan 19 M4 dialog helper).
pub fn filters_from_tag(
    content: &ContentRegistry,
    tag: Option<&str>,
) -> Vec<Box<dyn GenerateFilter>> {
    match tag {
        Some(value) if !value.trim().is_empty() && value.trim() != "{}" => {
            crate::maps::filters::read_filters(content, value)
        }
        _ => crate::maps::filters::default_filter_stack(content),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlockId;
    use crate::content::test_support::test_registry;
    use crate::maps::filters::{FilterRegistry, parse_filters, write_filters};

    fn snapshot(width: i32, height: i32, registry: &ContentRegistry) -> EditorSnapshot {
        let stone = registry.block_id("stone").unwrap();
        let mut tiles = Tiles::new(width, height);
        for tile in tiles.array_mut() {
            tile.floor = stone;
        }
        tiles.get_mut(2, 2).floor = registry.block_id("sand-floor").unwrap_or(stone);
        tiles.get_mut(3, 3).block = registry.block_id("copper-wall").unwrap_or(BlockId::AIR);
        EditorSnapshot::from_tiles(&tiles)
    }

    #[test]
    fn snapshot_round_trips_tiles() {
        let content = test_registry();
        let snap = snapshot(8, 8, &content);
        let tiles = snap.to_tiles();
        assert_eq!(tiles.width, 8);
        assert_eq!(
            tiles.get(2, 2).floor,
            content.block_id("sand-floor").unwrap()
        );
        assert_eq!(
            snap.get(2, 2).floor,
            content.block_id("sand-floor").unwrap()
        );
        assert_eq!(snap.len(), 64);
    }

    #[test]
    fn preview_is_deterministic() {
        let content = test_registry();
        let snap = snapshot(16, 16, &content);
        let json = r#"[{"class":"scatter","seed":1,"chance":0.5,"flooronto":"sand-floor","block":"stone-wall"}]"#;
        let mut a = parse_filters(&content, json, &FilterRegistry::vanilla()).unwrap();
        let mut b = parse_filters(&content, json, &FilterRegistry::vanilla()).unwrap();
        let image_a = generate_preview(&snap, &mut a, &content, 0);
        let image_b = generate_preview(&snap, &mut b, &content, 0);
        assert_eq!(image_a, image_b);
        assert_eq!(image_a.width, 16);
        assert_eq!(image_a.rgba.len(), 16 * 16 * 4);
    }

    #[test]
    fn preview_seed_zeroes_in_tag_round_trip() {
        let content = test_registry();
        let mut filters: Vec<Box<dyn GenerateFilter>> = parse_filters(
            &content,
            r#"[{"class":"noise","seed":99}]"#,
            &FilterRegistry::vanilla(),
        )
        .unwrap();
        for filter in filters.iter_mut() {
            filter.set_seed(0);
        }
        let json = write_filters(&content, &filters);
        assert!(json.contains("\"seed\":0"), "seed reset: {json}");
    }
}
