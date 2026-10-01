// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Load-time state and the `WorldContext` seam (plan 04 §3.2; HLP §12 C10).
//!
//! Ported from `core/src/mindustry/io/SaveReadState.java` and
//! `core/src/mindustry/world/WorldContext.java`. Plan 04 **defines** the
//! `WorldContext` trait because 04 blocks 06; plan 06 implements it for the
//! real world (`WorldContext` impl, tile data hooks) — consumers import, never
//! fork (HLP §12 C10).

use super::super::wire::WireReader;
use super::super::{IoError, StringMap};
use crate::content::ContentRegistry;

/// Mutable state threaded through one save load (`SaveReadState`).
///
/// `reset()` runs at the start of every load attempt (upstream `logic.reset`),
/// so a failed primary load cannot leak partial state into the backup retry.
pub struct SaveReadState<'a> {
    /// The world consumer; `None` for meta-only/preview reads.
    pub context: Option<&'a mut dyn WorldContext>,
    /// Content registry receiving the temporary mapper (`content.setTemporaryMapper`);
    /// `None` for meta-only reads.
    pub content: Option<&'a mut ContentRegistry>,
    /// True when generating a map preview (`SaveReadState.preview`).
    pub preview: bool,
    /// Raw rules JSON stashed by the meta region; parsed after data patches
    /// (upstream `ruleString`, `SaveVersion.readRules`).
    pub rule_string: Option<String>,
    /// Meta tags as read (upstream keeps them in `state.map`/`SaveMeta`).
    pub tags: StringMap,
    /// Entity references read before their target was loaded
    /// (`TypeIO` entity fallback); resolved by the `after_read_all` pass.
    pub pending_entity_refs: Vec<PendingEntityRef>,
}

/// One unresolved entity reference recorded during load (plan 04 §3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingEntityRef {
    /// The raw entity ID read from the stream.
    pub id: i32,
}

impl SaveReadState<'_> {
    /// Clears all per-load state (`logic.reset` equivalent for the IO layer).
    pub fn reset(&mut self) {
        self.preview = false;
        self.rule_string = None;
        self.tags.clear();
        self.pending_entity_refs.clear();
    }
}

impl Default for SaveReadState<'_> {
    fn default() -> Self {
        Self {
            context: None,
            content: None,
            preview: false,
            rule_string: None,
            tags: StringMap::new(),
            pending_entity_refs: Vec::new(),
        }
    }
}

/// The world consumer surface for save/map loading (`WorldContext`).
///
/// The map region reader drives this trait; plan 06 implements it over the
/// real `Tiles`/`World`, and `MapIO.generatePreview` implements a cached-tile
/// variant (plan 19 renders the pixels). Method names mirror upstream.
pub trait WorldContext {
    /// Returns a tile handle for the tile array slot (`WorldContext.tile`).
    /// Slot data is mutated through the `set_*` methods instead of a returned
    /// reference (Rust aliasing), in row-major index order.
    fn tile_count(&self) -> usize;

    /// Creates the tile array (`WorldContext.resize`).
    fn resize(&mut self, width: u16, height: u16);

    /// Creates one tile during the floor pass
    /// (`WorldContext.create(x, y, floorID, overlayID, wallID)`).
    fn create(&mut self, x: u16, y: u16, floor: u16, overlay: u16, wall: u16);

    /// Whether the world is already generating (`WorldContext.isGenerating`).
    fn is_generating(&self) -> bool;

    /// Begins generating (`WorldContext.begin`).
    fn begin(&mut self);

    /// Ends generating; prepares tiles (`WorldContext.end`).
    fn end(&mut self);

    /// Sets the block of a tile during the block pass (`Tile.setBlock`),
    /// creating the building entity when the block has one.
    fn set_block(&mut self, index: usize, block: u16);

    /// Whether the tile currently holds a building (`tile.build != null`).
    fn has_building(&self, index: usize) -> bool;

    /// Whether the block on a tile has building IO (`block.hasBuilding()`);
    /// entity chunks for removed blocks are skipped by length.
    fn block_has_building_io(&self, index: usize) -> bool;

    /// Assigns the 7-byte tile data group (`data`, `floorData`, `overlayData`,
    /// `extraData`) after `set_block` (`Tile.data=...` in `SaveVersion.readMap`).
    fn set_tile_data(
        &mut self,
        index: usize,
        data: u8,
        floor_data: u8,
        overlay_data: u8,
        extra_data: i32,
    );

    /// Reads one tile-entity chunk at a multiblock center
    /// (`tile.build.readAll(in, revision)`); `version` is the building format
    /// byte (`entity.version()`).
    fn read_building(
        &mut self,
        index: usize,
        reader: &mut WireReader,
        version: u8,
    ) -> Result<(), IoError>;

    /// Called when a building finished reading (`WorldContext.onReadBuilding`).
    fn on_read_building(&mut self, _index: usize) {}

    /// Called when tile data finished reading (`WorldContext.onReadTileData`).
    fn on_read_tile_data(&mut self, _index: usize) {}

    /// Whether the `SaveLoadEvent` fired after the load counts as a new map
    /// load (`WorldContext.isMap`).
    fn is_map(&self) -> bool {
        false
    }
}
