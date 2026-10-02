// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Load-time state and the `WorldContext` seam (plan 04 §3.2; HLP §12 C10).
//!
//! Ported from `core/src/mindustry/io/SaveReadState.java` and
//! `core/src/mindustry/world/WorldContext.java`. Plan 04 **defines** the
//! `WorldContext` trait because 04 blocks 06; plan 06 implements it for the
//! real world (`WorldContext` impl, tile data hooks) — consumers import, never
//! fork (HLP §12 C10).

use super::super::wire::{WireReader, WireWriter};
use super::super::{IoError, IoResult, StringMap};
use super::chunk::SaveScratch;
use crate::content::{BlockId, ContentRegistry};
use crate::io::entity::EntityIdMap;
use crate::io::typeio::TypeValue;

/// Mutable state threaded through one save load (`SaveReadState`).
///
/// `reset()` runs at the start of every load attempt (upstream `logic.reset`),
/// so a failed primary load cannot leak partial state into the backup retry.
pub struct SaveReadState<'a> {
    /// The world consumer; `None` for meta-only/preview reads.
    pub context: Option<&'a mut dyn WorldContext>,
    /// Content registry receiving the temporary mapper (`content.setTemporaryMapper`);
    /// `None` for meta-only reads. Required by the map/entities regions.
    pub content: Option<&'a mut ContentRegistry>,
    /// Entity chunk consumer (`readWorldEntities`); `None` skips entity
    /// payloads by length (meta-only reads).
    pub entities: Option<&'a mut dyn EntitySink>,
    /// Map-markers consumer (plan 12 `MapMarkers::read`); `None` skips.
    pub markers: Option<&'a mut dyn MarkersSink>,
    /// Custom-chunk registry (plan 20 mods); unknown names are skipped.
    pub custom_chunks: Option<&'a indexmap::IndexMap<String, std::sync::Arc<dyn CustomChunk>>>,
    /// True when generating a map preview (`SaveReadState.preview`).
    pub preview: bool,
    /// Raw rules JSON stashed by the meta region; parsed after data patches
    /// (upstream `ruleString`, `SaveVersion.readRules`).
    pub rule_string: Option<String>,
    /// Parsed rules (native v1 parses the stashed JSON after all regions;
    /// upstream v13+ semantics). Plan 12 applies sector/planet overrides.
    pub rules: Option<crate::io::json::Rules>,
    /// Parsed per-game stats from the `meta` region (`GameStats`).
    pub stats: Option<crate::io::json::GameStats>,
    /// Parsed map locales from the `meta` region (`MapLocales`).
    pub locales: Option<crate::io::json::MapLocales>,
    /// Meta tags as read (upstream keeps them in `state.map`/`SaveMeta`).
    pub tags: StringMap,
    /// Buildings read from the map, as tile indices (`SaveReadState.allBuildings`);
    /// the `after_read_all` pass consumes them.
    pub all_buildings: Vec<usize>,
    /// Team build plans read from the `entities` region
    /// (`Teams.TeamData.plans`); plans 11/12 apply them.
    pub team_plans: Vec<(i32, Vec<TeamPlan>)>,
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
        self.rules = None;
        self.stats = None;
        self.locales = None;
        self.tags.clear();
        self.all_buildings.clear();
        self.team_plans.clear();
        self.pending_entity_refs.clear();
    }
}

impl Default for SaveReadState<'_> {
    fn default() -> Self {
        Self {
            context: None,
            content: None,
            entities: None,
            markers: None,
            custom_chunks: None,
            preview: false,
            rule_string: None,
            rules: None,
            stats: None,
            locales: None,
            tags: StringMap::new(),
            all_buildings: Vec::new(),
            team_plans: Vec::new(),
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

/// Write-side tile data source for the `map` region (the read-side is
/// [`WorldContext`]). Mirrors the `Tile` accessors `SaveVersion.writeMap`
/// uses; plan 06's world implements this, the M4 fixture does too.
pub trait MapSource {
    /// World width in tiles.
    fn width(&self) -> u16;
    /// World height in tiles.
    fn height(&self) -> u16;
    /// Floor content id at a row-major index.
    fn floor_id(&self, index: usize) -> u16;
    /// Overlay content id at a row-major index.
    fn overlay_id(&self, index: usize) -> u16;
    /// Block content id at a row-major index.
    fn block_id(&self, index: usize) -> u16;
    /// Whether the tile holds a building (`tile.build != null`).
    fn has_building(&self, index: usize) -> bool;
    /// Whether the tile is its multiblock's center (`tile.isCenter()`);
    /// entities are written only at centers.
    fn is_center(&self, index: usize) -> bool;
    /// `Tile.shouldSaveData`: `floor.saveData || overlay.saveData || block.saveData`.
    fn should_save_data(&self, index: usize) -> bool;
    /// The 7-byte tile data group: `(data, floorData, overlayData, extraData)`.
    fn tile_data(&self, index: usize) -> (u8, u8, u8, i32);
    /// Writes one tile-entity chunk payload at a center tile:
    /// `version` byte (`entity.version()`) then the entity codec body
    /// (`tile.build.writeAll`).
    fn write_building(&self, index: usize, chunk: &mut WireWriter) -> IoResult<()>;
}

/// One AI-team build plan (`mindustry.game.Teams.BlockPlan` wire shape).
///
/// Written in the `entities` region after the entity ID mapping; plan 12 owns
/// the runtime type (`TeamData.plans`).
#[derive(Debug, Clone, PartialEq)]
pub struct TeamPlan {
    /// Tile x (`i16` on the wire).
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Build rotation.
    pub rotation: i16,
    /// Block to place.
    pub block: BlockId,
    /// `TypeIO` config object.
    pub config: TypeValue,
}

/// Write-side entity data for the `entities` region
/// (`SaveVersion.writeEntities`).
pub trait EntitySource {
    /// Custom entity-ID mapping (`EntityMapping.customIdMap`); usually empty.
    fn entity_id_map(&self) -> EntityIdMap {
        EntityIdMap::new()
    }
    /// Active teams' build plans (`writeTeamBlocks`); the sharded team's
    /// plans are always included upstream.
    fn team_plans(&self) -> Vec<(i32, Vec<TeamPlan>)> {
        Vec::new()
    }
    /// Number of entity chunks that [`write_entities`](Self::write_entities)
    /// emits (`Groups.all.count(serialize) + Groups.unit.size`).
    fn entity_count(&self) -> usize;
    /// Writes the entity chunks: per entity
    /// `chunk(u8 class_id, i32 entity_id, revision + fields)`.
    fn write_entities(&self, w: &mut WireWriter, scratch: &mut SaveScratch) -> IoResult<()>;
}

/// Read-side entity consumer for the `entities` region
/// (`SaveVersion.readWorldEntities`).
pub trait EntitySink {
    /// Whether a class ID is known (unknown classes are skipped by length,
    /// `mapping[typeid] == null` upstream). `custom_name` is the save's custom
    /// ID-map name, which takes precedence over the global class ID.
    fn supports_class(&self, class_id: u8, custom_name: Option<&str>) -> bool;
    /// Reads one entity chunk payload (after class id + entity id): the codec
    /// revision (`u16`) then fields. Implementations instantiate/register the
    /// entity and honor duplicate-ID reassignment.
    fn read_entity(
        &mut self,
        class_id: u8,
        custom_name: Option<&str>,
        id: i32,
        r: &mut WireReader,
    ) -> IoResult<()>;
    /// The `afterReadAll` pass (`Groups.all/unit/allBuildings.each`).
    fn after_read_all(&mut self);
}

/// One mod custom chunk (`SaveFileReader.CustomChunk`); registered by plan 20.
pub trait CustomChunk: Send + Sync {
    /// Writes the chunk payload.
    fn write(&self, w: &mut WireWriter) -> IoResult<()>;
    /// Reads the chunk payload (`len` bytes available).
    fn read(&self, r: &mut WireReader, len: usize) -> IoResult<()>;
    /// Whether the chunk is written at all (`CustomChunk.shouldWrite`).
    fn should_write(&self) -> bool {
        true
    }
    /// Whether the chunk is sent to connecting clients (`writeNet`).
    fn write_net(&self) -> bool {
        true
    }
}

/// Write-side map markers (plan 12 `MapMarkers::write`); the payload shape is
/// plan 12's, the region framing is this plan's.
pub trait MarkersIo {
    /// Writes the markers payload.
    fn write_markers(&self, w: &mut WireWriter) -> IoResult<()>;
}

/// Read-side map markers (plan 12 `MapMarkers::read`).
pub trait MarkersSink {
    /// Reads the markers payload.
    fn read_markers(&mut self, r: &mut WireReader) -> IoResult<()>;
}

/// Write-side data patch set (plan 20 `DataPatcher`); the region carries the
/// `i32` format version + patch entries.
pub trait PatchSetIo {
    /// Writes patch entries (after the format version + count header).
    fn write_patches(&self, w: &mut WireWriter, embed: bool) -> IoResult<()>;
    /// Number of patches.
    fn patch_count(&self) -> usize;
}
