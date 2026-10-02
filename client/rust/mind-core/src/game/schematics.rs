// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `.msch` codec and the `Schematics` registry (plan 12 M5).
//!
//! Ported from `core/src/mindustry/game/Schematics.java`. The byte format is
//! fixed by plan 12 §6.4: raw `msch` magic + version byte, then a zlib stream
//! (`DeflaterOutputStream`) holding dimensions, tags, the first-seen block
//! dictionary and the tiles. Base64 output always begins `bXNjaAB`.
//!
//! Placement defers to plan 06 tile ops + plan 07 configure through the
//! [`SchematicWorld`] seam; rendering (`getBuffer`/`savePreview`) is plan 19.

use std::io::{Read, Write};
use std::path::Path;

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use indexmap::{IndexMap, IndexSet};

use super::rules::MAX_LOADOUT_SCHEMATIC_PAD;
use super::schematic::{Schematic, Stile};
use crate::content::registries::blocks::{BuildVisibility, TILE_SIZE};
use crate::content::{BlockId, ContentRegistry};
use crate::io::IoError;
use crate::io::fs::FileSystem;
use crate::io::save::chunk::map_fallback;
use crate::io::typeio::{pack_point2, read_object, unpack_point2_x, unpack_point2_y, write_object};
use crate::io::{WireReader, WireWriter};
use crate::world::config::{ConfigValue, config_to_type_value, type_value_to_config};
use crate::world::plan::BuildPlan;

/// `.msch` header (`{'m','s','c','h'}`).
pub const HEADER: [u8; 4] = *b"msch";
/// Current on-disk version.
pub const VERSION: u8 = 1;
/// Maximum width/height (`limitSchematicSize`).
pub const MAX_DIMENSION: i32 = 128;
/// Maximum tile count (`128 * 128`).
pub const MAX_TILES: i32 = 128 * 128;

/// Errors from the schematic codec/registry.
#[derive(Debug, thiserror::Error)]
pub enum SchematicError {
    /// Underlying IO error.
    #[error(transparent)]
    Io(#[from] IoError),
    /// Raw IO error (deflate stream).
    #[error(transparent)]
    RawIo(#[from] std::io::Error),
    /// Missing/incorrect header.
    #[error("not a schematic file (missing header)")]
    BadHeader,
    /// Newer than this build.
    #[error(
        "unknown version: {0} (are you trying to load a schematic from a newer version of the game?)"
    )]
    UnknownVersion(u8),
    /// Dimensions exceed [`MAX_DIMENSION`].
    #[error("invalid schematic: too large (max possible size is 128x128)")]
    TooLarge,
    /// Tile count exceeds [`MAX_TILES`].
    #[error("invalid schematic: too many blocks")]
    TooManyBlocks,
    /// A referenced block name is unknown (resolved to air).
    #[error("unknown block in schematic dictionary: {0}")]
    UnknownBlock(String),
    /// JSON encode error for the labels/contentMap tags.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Selection-world seam (`Schematics.create` reads finalized world state).
pub trait SchematicWorld {
    /// A placed building view at a tile.
    fn building_at(&self, x: i32, y: i32) -> Option<WorldBuilding>;
}

/// Minimal building view for [`Schematics::create`].
#[derive(Debug, Clone, PartialEq)]
pub struct WorldBuilding {
    /// Underlying block (a `ConstructBuild` resolves to its `current` block).
    pub block: BlockId,
    /// Block side length.
    pub size: i32,
    /// Tile x.
    pub x: i32,
    /// Tile y.
    pub y: i32,
    /// Rotation 0-3.
    pub rotation: i8,
    /// Current config (a `ConstructBuild` resolves to its `lastConfig`).
    pub config: ConfigValue,
    /// Visible to the selecting team.
    pub visible: bool,
    /// Discovered by the selecting team.
    pub discovered: bool,
}

/// Writes a schematic to bytes (`Schematics.write`).
pub fn write(schem: &Schematic, registry: &ContentRegistry) -> Result<Vec<u8>, SchematicError> {
    // Tags are mutated with `labels`/`contentMap` like upstream; clone first.
    let mut tags = schem.tags.clone();
    let labels_json = serde_json::to_string(&schem.labels)?;
    tags.insert("labels".to_owned(), labels_json);
    // Content remapping metadata: Rust↔Rust uses dense ids directly, so the
    // map is emitted empty (upstream emits name→id pairs). Documented dev.
    tags.insert("contentMap".to_owned(), "{}".to_owned());

    // First-seen block dictionary.
    let mut dictionary: Vec<BlockId> = Vec::new();
    let mut index_of: IndexMap<u16, u8> = IndexMap::new();
    for tile in &schem.tiles {
        if !index_of.contains_key(&tile.block.raw()) {
            let index =
                u8::try_from(dictionary.len()).map_err(|_| SchematicError::TooManyBlocks)?;
            index_of.insert(tile.block.raw(), index);
            dictionary.push(tile.block);
        }
    }

    let mut payload = Vec::new();
    {
        let mut w = WireWriter::new(&mut payload);
        w.s(schem.width as i16);
        w.s(schem.height as i16);
        w.ub(tags.len() as u8);
        for (key, value) in &tags {
            w.str(key)?;
            w.str(value)?;
        }
        w.ub(dictionary.len() as u8);
        for block in &dictionary {
            let name = registry
                .block(*block)
                .map(|record| record.name.clone())
                .unwrap_or_default();
            w.str(&name)?;
        }
        w.i(schem.tiles.len() as i32);
        for tile in &schem.tiles {
            let index = *index_of
                .get(&tile.block.raw())
                .ok_or(SchematicError::TooManyBlocks)?;
            w.ub(index);
            w.i(pack_point2(tile.x as i32, tile.y as i32));
            write_object(&mut w, &config_to_type_value(&tile.config))?;
            w.b(tile.rotation);
        }
    }

    let mut out = Vec::with_capacity(payload.len() + 16);
    out.extend_from_slice(&HEADER);
    out.push(VERSION);
    // Java `DeflaterOutputStream` defaults to level 6 (zlib header 0x78 0x9c),
    // matching the vanilla loadout payloads.
    let mut encoder = ZlibEncoder::new(out, Compression::default());
    encoder.write_all(&payload)?;
    Ok(encoder.finish()?)
}

/// Reads a schematic from bytes (`Schematics.read`).
pub fn read(bytes: &[u8], registry: &ContentRegistry) -> Result<Schematic, SchematicError> {
    if bytes.len() < 5 || bytes[..4] != HEADER {
        return Err(SchematicError::BadHeader);
    }
    let version = bytes[4];
    if version > VERSION {
        return Err(SchematicError::UnknownVersion(version));
    }
    let mut decoder = ZlibDecoder::new(&bytes[5..]);
    let mut payload = Vec::new();
    decoder.read_to_end(&mut payload)?;

    let mut reader = WireReader::new(&payload);
    let width = reader.s()? as i32;
    let height = reader.s()? as i32;
    if width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(SchematicError::TooLarge);
    }

    let mut tags: IndexMap<String, String> = IndexMap::new();
    let tag_count = reader.ub()?;
    for _ in 0..tag_count {
        let key = reader.str()?;
        let value = reader.str()?;
        tags.insert(key, value);
    }

    let labels: Vec<String> = tags
        .get("labels")
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or_default();

    let dict_count = reader.ub()?;
    let mut dictionary: Vec<BlockId> = Vec::with_capacity(dict_count as usize);
    for _ in 0..dict_count {
        let name = reader.str()?;
        let resolved = registry
            .block_by_name(&name)
            .or_else(|| registry.block_by_name(map_fallback(&name)))
            .map(|record| record.id)
            .unwrap_or(BlockId::AIR);
        dictionary.push(resolved);
    }

    let total = reader.i()?;
    if total > MAX_TILES {
        return Err(SchematicError::TooManyBlocks);
    }

    let mut tiles = Vec::with_capacity(total.max(0) as usize);
    for _ in 0..total.max(0) {
        let index = reader.ub()? as usize;
        let position = reader.i()?;
        let block = dictionary.get(index).copied().unwrap_or(BlockId::AIR);
        let config = if version == 0 {
            let raw = reader.i()?;
            map_config(registry, block, raw, position)
        } else {
            let value = read_object(&mut reader)?;
            type_value_to_config(&value)
        };
        let rotation = reader.b()?;
        if block != BlockId::AIR {
            tiles.push(Stile::new(
                block,
                unpack_point2_x(position) as i16,
                unpack_point2_y(position) as i16,
                config,
                rotation,
            ));
        }
    }

    let mut schem = Schematic::from_tiles(tiles, tags, width, height);
    schem.labels = labels;
    Ok(schem)
}

/// `Schematics.writeBase64` (always starts `bXNjaAB`).
pub fn write_base64(
    schem: &Schematic,
    registry: &ContentRegistry,
) -> Result<String, SchematicError> {
    let bytes = write(schem, registry)?;
    Ok(data_encoding::BASE64.encode(&bytes))
}

/// `Schematics.readBase64`.
pub fn read_base64(text: &str, registry: &ContentRegistry) -> Result<Schematic, SchematicError> {
    let decoded = data_encoding::BASE64
        .decode(text.trim().as_bytes())
        .map_err(|_| SchematicError::BadHeader)?;
    read(&decoded, registry)
}

/// `Schematics.isSchematic`.
pub fn is_schematic(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[..4] == HEADER
}

/// Maps legacy v0 int configs to config objects (`Schematics.mapConfig`).
fn map_config(
    registry: &ContentRegistry,
    block: BlockId,
    value: i32,
    position: i32,
) -> ConfigValue {
    use crate::content::registries::blocks::BlockKind;
    let Some(record) = registry.block(block) else {
        return ConfigValue::None;
    };
    match record.kind {
        BlockKind::Sorter | BlockKind::Unloader | BlockKind::ItemSource => registry
            .item(crate::content::ItemId::new(value as u16))
            .map_or(ConfigValue::None, |item| ConfigValue::Item(item.id)),
        BlockKind::LiquidSource => registry
            .liquid(crate::content::LiquidId::new(value as u16))
            .map_or(ConfigValue::None, |liquid| ConfigValue::Liquid(liquid.id)),
        BlockKind::MassDriver | BlockKind::ItemBridge => ConfigValue::Point2(
            unpack_point2_x(value) - unpack_point2_x(position),
            unpack_point2_y(value) - unpack_point2_y(position),
        ),
        BlockKind::LightBlock => ConfigValue::Number(value as f64),
        _ => ConfigValue::None,
    }
}

/// `Schematics` resource: the loaded library and loadout cache.
#[derive(Debug, Default)]
pub struct Schematics {
    /// All loaded schematics.
    pub all: Vec<Schematic>,
    /// Core block id -> valid loadout schematic indices.
    pub loadouts: IndexMap<u16, Vec<usize>>,
    /// Core block id -> default loadout index.
    pub default_loadouts: IndexMap<u16, usize>,
    /// Schematic indices whose preview failed (plan 16/19).
    pub errored: IndexSet<usize>,
}

impl Schematics {
    /// Empty library.
    pub fn new() -> Self {
        Self::default()
    }

    /// `Schematics.loadSync`: loads vanilla loadouts then every `.msch` in
    /// `dir` (missing dir is empty, never fatal — deviation 15).
    pub fn load(
        &mut self,
        fs: &dyn FileSystem,
        dir: &Path,
        registry: &ContentRegistry,
    ) -> Result<(), SchematicError> {
        self.all.clear();
        self.loadouts.clear();
        self.default_loadouts.clear();
        self.load_loadouts(registry);

        let files = fs.walk(dir).unwrap_or_default();
        for file in files {
            if file.extension().and_then(|ext| ext.to_str()) == Some("msch")
                && let Err(error) = self.load_file(fs, &file, registry)
            {
                log::warn!("failed to read schematic `{}`: {error}", file.display());
            }
        }
        self.all.sort_by(|a, b| a.compare_to(b));
        Ok(())
    }

    /// Decodes the 4 vanilla loadouts (`Loadouts.java`) into the cache.
    pub fn load_loadouts(&mut self, registry: &ContentRegistry) {
        let defs: Vec<(String, String)> = registry
            .loadouts()
            .iter()
            .map(|def| (def.name.clone(), def.schematic_base64.clone()))
            .collect();
        for (name, base64) in defs {
            match read_base64(&base64, registry) {
                Ok(mut schem) => {
                    schem.tags.insert("name".to_owned(), name);
                    let index = self.all.len();
                    self.all.push(schem);
                    self.check_loadout(index, false, registry);
                }
                Err(error) => log::warn!("failed to decode loadout `{name}`: {error}"),
            }
        }
    }

    /// `Schematics.loadFile`.
    pub fn load_file(
        &mut self,
        fs: &dyn FileSystem,
        file: &Path,
        registry: &ContentRegistry,
    ) -> Result<usize, SchematicError> {
        let bytes = fs.read(file)?;
        let mut schem = read(&bytes, registry)?;
        if !schem.tags.contains_key("name") {
            let stem = file
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            schem.tags.insert("name".to_owned(), stem);
        }
        schem.file = Some(file.to_string_lossy().into_owned());
        let index = self.all.len();
        self.all.push(schem);
        self.check_loadout(index, true, registry);
        self.all.sort_by(|a, b| a.compare_to(b));
        Ok(index)
    }

    /// `Schematics.checkLoadout`.
    pub fn check_loadout(
        &mut self,
        index: usize,
        custom: bool,
        registry: &ContentRegistry,
    ) -> bool {
        let Some(schem) = self.all.get(index) else {
            return false;
        };
        let Some(core) = schem.find_core(registry) else {
            return false;
        };
        let cores = schem
            .tiles
            .iter()
            .filter(|tile| {
                registry.block(tile.block).is_some_and(|block| {
                    block.kind == crate::content::registries::blocks::BlockKind::CoreBlock
                })
            })
            .count();
        let core_size = registry.block(core).map(|block| block.size).unwrap_or(1);
        let max_size = get_max_launch_size(core_size);

        if custom
            && (schem.width > max_size
                || schem.height > max_size
                || cores > 1
                || schem.tiles.iter().any(|tile| {
                    registry
                        .block(tile.block)
                        .is_some_and(|block| block.build_visibility == BuildVisibility::SandboxOnly)
                }))
        {
            return false;
        }

        self.loadouts.entry(core.raw()).or_default().push(index);
        if !custom {
            self.default_loadouts.insert(core.raw(), index);
        }
        true
    }

    /// `Schematics.getMaxLaunchSize`.
    pub fn max_launch_size(&self, registry: &ContentRegistry, core: BlockId) -> i32 {
        let size = registry.block(core).map(|block| block.size).unwrap_or(1);
        get_max_launch_size(size)
    }

    /// `Schematics.toPlans` (centered, hidden/unlocked filter, stable order).
    pub fn to_plans(
        &self,
        index: usize,
        x: i32,
        y: i32,
        check_hidden: bool,
        registry: &ContentRegistry,
    ) -> Vec<BuildPlan> {
        let Some(schem) = self.all.get(index) else {
            return Vec::new();
        };
        let mut plans: Vec<BuildPlan> = schem
            .tiles
            .iter()
            .filter(|tile| {
                if !check_hidden {
                    return true;
                }
                registry.block(tile.block).is_some_and(|block| {
                    !block.is_hidden()
                        || block.kind == crate::content::registries::blocks::BlockKind::CoreBlock
                })
            })
            .map(|tile| BuildPlan {
                x: tile.x as i32 + x - schem.width / 2,
                y: tile.y as i32 + y - schem.height / 2,
                rotation: tile.rotation as u8,
                block: tile.block,
                config: tile.config.clone(),
                breaking: false,
            })
            .collect();
        plans.sort_by_key(|plan| plan.block.raw());
        plans
    }

    /// `Schematics.add`: appends and validates, returning the new index.
    ///
    /// Deviation: the list is not re-sorted here (upstream sorts by name); the
    /// returned index is stable for the caller. Callers that need order call
    /// [`Schematic::compare_to`] themselves.
    pub fn add(&mut self, schem: Schematic, registry: &ContentRegistry) -> usize {
        let index = self.all.len();
        self.all.push(schem);
        self.check_loadout(index, true, registry);
        index
    }

    /// `Schematics.remove`.
    pub fn remove(&mut self, index: usize) {
        if index >= self.all.len() {
            return;
        }
        self.all.remove(index);
        for list in self.loadouts.values_mut() {
            list.retain(|existing| *existing != index);
            for existing in list.iter_mut() {
                if *existing > index {
                    *existing -= 1;
                }
            }
        }
        for value in self.default_loadouts.values_mut() {
            if *value > index {
                *value -= 1;
            }
        }
    }

    /// `Schematics.getLoadouts`.
    pub fn get_loadouts(&self, core: BlockId) -> &[usize] {
        self.loadouts
            .get(&core.raw())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// `Schematics.getDefaultLoadout`.
    pub fn get_default_loadout(&self, core: BlockId) -> Option<usize> {
        self.default_loadouts.get(&core.raw()).copied()
    }

    /// `Schematics.create`: scans a selection and builds a schematic.
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        &self,
        world: &dyn SchematicWorld,
        registry: &ContentRegistry,
        x: i32,
        y: i32,
        x2: i32,
        y2: i32,
        max_size: i32,
    ) -> Schematic {
        let (x, y, x2, y2) = normalize_area(x, y, x2, y2, max_size);
        let (ox, oy, ox2, oy2) = (x, y, x2, y2);

        let mut minx = x2;
        let mut miny = y2;
        let mut maxx = x;
        let mut maxy = y;
        let mut found = false;
        for cx in x..=x2 {
            for cy in y..=y2 {
                let Some(building) = world.building_at(cx, cy) else {
                    continue;
                };
                if !building.visible || !building.discovered {
                    continue;
                }
                let block = building.block;
                if registry.block(block).is_none() {
                    continue;
                }
                if registry
                    .block(block)
                    .is_some_and(|record| record.is_hidden())
                    && registry.block(block).is_none_or(|record| {
                        record.kind != crate::content::registries::blocks::BlockKind::CoreBlock
                    })
                {
                    continue;
                }
                let top = building.size / 2;
                let bot = if building.size % 2 == 1 {
                    -building.size / 2
                } else {
                    -(building.size - 1) / 2
                };
                minx = (building.x + bot).min(minx);
                miny = (building.y + bot).min(miny);
                maxx = (building.x + top).max(maxx);
                maxy = (building.y + top).max(maxy);
                found = true;
            }
        }

        if !found {
            return Schematic::from_tiles(Vec::new(), IndexMap::new(), 1, 1);
        }

        let width = maxx - minx + 1;
        let height = maxy - miny + 1;
        let offset_x = -minx;
        let offset_y = -miny;
        let mut counted: IndexSet<(i32, i32)> = IndexSet::new();
        let mut tiles = Vec::new();
        for cx in ox..=ox2 {
            for cy in oy..=oy2 {
                let Some(building) = world.building_at(cx, cy) else {
                    continue;
                };
                if !building.visible || !building.discovered {
                    continue;
                }
                if counted.contains(&(building.x, building.y)) {
                    continue;
                }
                let is_core = registry.block(building.block).is_some_and(|record| {
                    record.kind == crate::content::registries::blocks::BlockKind::CoreBlock
                });
                if registry
                    .block(building.block)
                    .is_none_or(|record| record.is_hidden())
                    && !is_core
                {
                    continue;
                }
                counted.insert((building.x, building.y));
                tiles.push(Stile::new(
                    building.block,
                    (building.x + offset_x) as i16,
                    (building.y + offset_y) as i16,
                    building.config.clone(),
                    building.rotation,
                ));
            }
        }

        Schematic::from_tiles(tiles, IndexMap::new(), width, height)
    }

    /// `Schematics.rotate`: N times 90° counter-clockwise.
    pub fn rotate(schem: &Schematic, times: i32, registry: &ContentRegistry) -> Schematic {
        if times == 0 {
            return schem.clone();
        }
        let sign = times > 0;
        let mut current = schem.clone();
        for _ in 0..times.abs() {
            current = Self::rotated(&current, sign, registry);
        }
        current
    }

    /// One 90° rotation (`Schematics.rotated`).
    pub fn rotated(input: &Schematic, counter: bool, registry: &ContentRegistry) -> Schematic {
        let direction: i32 = if counter { 1 } else { -1 };
        let mut out = input.clone();
        let ox = input.width / 2;
        let oy = input.height / 2;

        for tile in &mut out.tiles {
            tile.config = rotate_point_config(registry, tile.block, &tile.config, direction);
            let block_offset = registry
                .block(tile.block)
                .map(|block| block.size as f32 / 2.0 * TILE_SIZE)
                .unwrap_or(0.0);
            let wx = (tile.x as f32 - ox as f32) * TILE_SIZE + block_offset;
            let wy = (tile.y as f32 - oy as f32) * TILE_SIZE + block_offset;
            let (nx, ny) = if direction >= 0 { (-wy, wx) } else { (wy, -wx) };
            tile.x = (to_tile(nx - block_offset) + ox) as i16;
            tile.y = (to_tile(ny - block_offset) + oy) as i16;
            tile.rotation = (tile.rotation as i32 + direction).rem_euclid(4) as i8;
        }

        out.width = input.height;
        out.height = input.width;
        out
    }
}

/// `getMaxLaunchSize` (`core.size + 2 * maxLoadoutSchematicPad`).
pub fn get_max_launch_size(core_size: i32) -> i32 {
    core_size + MAX_LOADOUT_SCHEMATIC_PAD * 2
}

/// `Placement.normalizeArea` subset: orders and clamps the selection.
pub fn normalize_area(x: i32, y: i32, x2: i32, y2: i32, max_size: i32) -> (i32, i32, i32, i32) {
    let minx = x.min(x2);
    let miny = y.min(y2);
    let mut maxx = x.max(x2);
    let mut maxy = y.max(y2);
    if maxx - minx + 1 > max_size {
        maxx = minx + max_size - 1;
    }
    if maxy - miny + 1 > max_size {
        maxy = miny + max_size - 1;
    }
    (minx, miny, maxx, maxy)
}

/// `World.toTile` (`floor(v / tilesize)`).
fn to_tile(value: f32) -> i32 {
    (value / TILE_SIZE).floor() as i32
}

/// `BuildPlan.pointConfig` rotation for `Point2` configs.
fn rotate_point_config(
    registry: &ContentRegistry,
    block: BlockId,
    config: &ConfigValue,
    direction: i32,
) -> ConfigValue {
    let _ = (registry, block);
    if let ConfigValue::Point2(px, py) = config {
        let (mut cx, mut cy) = (*px, *py);
        let lx = cx;
        if direction >= 0 {
            cx = -cy;
            cy = lx;
        } else {
            cx = cy;
            cy = -lx;
        }
        return ConfigValue::Point2(cx, cy);
    }
    config.clone()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::*;
    use crate::content::registries::blocks::BlockKind;
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};

    fn registry() -> ContentRegistry {
        create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true).unwrap()
    }

    fn sample(registry: &ContentRegistry) -> Schematic {
        let core = registry.block_id("core-shard").unwrap();
        let wall = registry.block_id("copper-wall").unwrap();
        let mut schem = Schematic::from_tiles(
            vec![
                Stile::new(core, 1, 0, ConfigValue::None, 0),
                Stile::new(wall, 3, 0, ConfigValue::None, 2),
            ],
            IndexMap::new(),
            4,
            1,
        );
        schem.tags.insert("name".to_owned(), "test-base".to_owned());
        schem
    }

    #[test]
    fn msch_roundtrip_and_header() {
        let registry = registry();
        let schem = sample(&registry);
        let bytes = write(&schem, &registry).unwrap();
        assert_eq!(&bytes[..4], b"msch");
        assert_eq!(bytes[4], 1);
        assert!(is_schematic(&bytes));

        let back = read(&bytes, &registry).unwrap();
        assert_eq!(back.width, schem.width);
        assert_eq!(back.height, schem.height);
        assert_eq!(back.tiles.len(), schem.tiles.len());
        assert_eq!(back.name(), "test-base");
        assert_eq!(back.tiles[0].block, schem.tiles[0].block);
        // Tile order is preserved by the dictionary indices.
        assert_eq!(back.tiles[1].rotation, 2);

        // Re-encoding the decoded schematic is byte-identical.
        let bytes2 = write(&back, &registry).unwrap();
        assert_eq!(bytes, bytes2);
    }

    #[test]
    fn base64_prefix_and_roundtrip() {
        let registry = registry();
        let schem = sample(&registry);
        let encoded = write_base64(&schem, &registry).unwrap();
        assert!(
            encoded.starts_with("bXNjaA"),
            "base64 must start with bXNjaA, got {encoded}"
        );
        // Default compression matches the vanilla loadout payload prefix.
        assert!(encoded.starts_with("bXNjaAF4"), "got {encoded}");
        let back = read_base64(&encoded, &registry).unwrap();
        assert_eq!(back.tiles.len(), schem.tiles.len());
    }

    #[test]
    fn vanilla_loadouts_decode() {
        let registry = registry();
        let mut schematics = Schematics::new();
        schematics.load_loadouts(&registry);
        assert_eq!(schematics.all.len(), 4);
        let core = registry.block_id("core-shard").unwrap();
        assert!(
            !schematics.get_loadouts(core).is_empty(),
            "basicShard must register as a core-shard loadout"
        );
        assert_eq!(schematics.get_default_loadout(core), Some(0));
    }

    #[test]
    fn create_from_selection() {
        // A tiny world: two 3x3 cores (cores are never filtered as hidden).
        struct World {
            core: BlockId,
        }
        impl SchematicWorld for World {
            fn building_at(&self, x: i32, y: i32) -> Option<WorldBuilding> {
                for origin in [5, 12] {
                    if (origin..=origin + 2).contains(&x) && (5..=7).contains(&y) {
                        return Some(WorldBuilding {
                            block: self.core,
                            size: 3,
                            x: origin,
                            y: 5,
                            rotation: 0,
                            config: ConfigValue::None,
                            visible: true,
                            discovered: true,
                        });
                    }
                }
                None
            }
        }
        let registry = registry();
        let world = World {
            core: registry.block_id("core-shard").unwrap(),
        };
        let schematics = Schematics::new();
        let schem = schematics.create(&world, &registry, 4, 4, 15, 8, 64);
        // First core origin 5 spans 4..6, second origin 12 spans 11..13.
        assert_eq!(schem.width, 10);
        assert_eq!(schem.height, 3);
        assert_eq!(schem.tiles.len(), 2, "one tile per building origin");
        assert!(schem.has_core(&registry));
        let core_tile = schem
            .tiles
            .iter()
            .find(|tile| tile.block == world.core)
            .unwrap();
        // Selection top-left is tile 4, so the core origin (5,5) maps to (1,1).
        assert_eq!((core_tile.x, core_tile.y), (1, 1));
    }

    #[test]
    fn rotate_point_config_fixes() {
        let registry = registry();
        let bridge = registry.block_id("bridge-conveyor").unwrap();
        let schem = Schematic::from_tiles(
            vec![Stile::new(bridge, 1, 0, ConfigValue::Point2(2, 0), 0)],
            IndexMap::new(),
            2,
            1,
        );
        // 90° CCW: (2,0) -> (0,2).
        let rotated = Schematics::rotate(&schem, 1, &registry);
        assert_eq!(rotated.tiles[0].config, ConfigValue::Point2(0, 2));
        assert_eq!(rotated.width, 1);
        assert_eq!(rotated.height, 2);
        // Full turn returns the original config.
        let full = Schematics::rotate(&schem, 4, &registry);
        assert_eq!(full.tiles[0].config, ConfigValue::Point2(2, 0));
    }

    #[test]
    fn check_loadout_rejects_multicore() {
        let registry = registry();
        let core = registry.block_id("core-shard").unwrap();
        let mut schematics = Schematics::new();
        let schem = Schematic::from_tiles(
            vec![
                Stile::new(core, 0, 0, ConfigValue::None, 0),
                Stile::new(core, 4, 0, ConfigValue::None, 0),
            ],
            IndexMap::new(),
            5,
            1,
        );
        let index = schematics.all.len();
        schematics.all.push(schem);
        assert!(!schematics.check_loadout(index, true, &registry));
    }

    #[test]
    fn map_config_legacy_roundtrip() {
        let registry = registry();
        let copper = registry.item_id("copper").unwrap();
        let sorter = registry.block_id("sorter").unwrap();
        assert!(matches!(
            registry.block(sorter).map(|b| b.kind),
            Some(BlockKind::Sorter)
        ));
        let config = map_config(&registry, sorter, copper.raw() as i32, 0);
        assert_eq!(config, ConfigValue::Item(copper));
    }
}
