// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Native save format v1 (plan 04 §3.3, current writer).
//!
//! Ported from `core/src/mindustry/io/SaveVersion.java` (region layout and
//! read/write logic). Native v1 always writes all seven regions; readers
//! tolerate unknown region names and missing trailing regions for forward
//! growth.
//!
//! Framing (plan 04 §6.1):
//! - `meta`: `i16` count + (`u16`-len UTF-8 key, value) pairs.
//! - `patches`: `i32` patch format version + `i32` asset count (payload plan 20).
//! - `content`: `u8` mapped types + per type (`u8` ordinal, `u16` count, names).
//! - `map`: `u16` width, `u16` height, floor/overlay RLE pass, then the block
//!   pass (`u16` block, packed entity/data byte, optional 7-byte tile data,
//!   center flag + nested tile-entity chunk, `u8` RLE run).
//! - `entities`: `u16` custom-ID map + names, team build plans, entity chunks.
//! - `markers`: plan 12 `MapMarkers` payload.
//! - `custom`: `i32` chunk count + (name, chunk) pairs (registry plan 20).

use super::super::chunk::{
    REGION_CONTENT, REGION_CUSTOM, REGION_ENTITIES, REGION_MAP, REGION_MARKERS, REGION_META,
    REGION_PATCHES, SaveReader, SaveWriter, map_fallback, require_consumed,
};
use super::super::meta::SaveMeta;
use super::super::options::SaveOptions;
use super::super::state::{SaveReadState, TeamPlan};
use super::super::version::SaveVersion;
use crate::content::load::TemporaryMapper;
use crate::content::{BlockId, ContentRegistry, ContentType};
use crate::io::entity::EntityIdMap;
use crate::io::json::JsonIo;
use crate::io::typeio;
use crate::io::wire::{WireReader, WireWriter};
use crate::io::{IoError, StringMap};

/// `DataPatcher.patchFormatVersion` (payload parsing is plan 20's; the version
/// int is ignored on read upstream).
const PATCH_FORMAT_VERSION: i32 = 2;

/// Native save format version 1.
pub struct SaveV1;

impl SaveVersion for SaveV1 {
    fn version(&self) -> u32 {
        1
    }

    fn write(&self, w: &mut SaveWriter, options: &SaveOptions) -> Result<(), IoError> {
        // `SaveVersion.write`: region order meta/patches/content/map/entities/
        // markers/custom. Extra tags merge under the standard set
        // (`StringMap.merge` overwrites).
        let mut tags = options.extra_tags.clone().unwrap_or_default();
        for (key, value) in &w.ctx.tags {
            tags.insert(key.clone(), value.clone());
        }
        w.write_region(REGION_META, |wire, _scratch| wire.string_map(&tags))?;

        let patches = w.ctx.patches;
        let embed = options.embed_assets;
        w.write_region(REGION_PATCHES, |wire, _scratch| {
            wire.i(PATCH_FORMAT_VERSION);
            match patches {
                Some(set) => {
                    wire.i(set.patch_count() as i32);
                    set.write_patches(wire, embed)?;
                }
                None => wire.i(0),
            }
            Ok(())
        })?;

        let content = w.ctx.content;
        w.write_region(REGION_CONTENT, |wire, _scratch| {
            write_content_header(wire, content)
        })?;

        let map = w.ctx.map;
        w.write_region(REGION_MAP, |wire, scratch| match map {
            Some(map) => write_map(wire, scratch, map),
            None => {
                // Empty world (meta-only saves).
                wire.us(0);
                wire.us(0);
                Ok(())
            }
        })?;

        let entities = w.ctx.entities;
        w.write_region(REGION_ENTITIES, |wire, scratch| match entities {
            Some(entities) => write_entities(wire, scratch, entities),
            None => {
                wire.us(0); // entity ID mapping
                wire.i(0); // team build plans
                wire.i(0); // world entities
                Ok(())
            }
        })?;

        let markers = w.ctx.markers;
        w.write_region(REGION_MARKERS, |wire, _scratch| match markers {
            Some(markers) => markers.write_markers(wire),
            None => {
                wire.i(0);
                Ok(())
            }
        })?;

        let custom = w.ctx.custom_chunks;
        w.write_region(REGION_CUSTOM, |wire, scratch| {
            let chunks: Vec<(
                &String,
                &std::sync::Arc<dyn super::super::state::CustomChunk>,
            )> = match custom {
                Some(registry) => registry
                    .iter()
                    .filter(|(_, chunk)| chunk.should_write())
                    .collect(),
                None => Vec::new(),
            };
            wire.i(chunks.len() as i32);
            for (name, chunk) in chunks {
                wire.str(name)?;
                scratch.write_chunk(wire, |chunk_writer| chunk.write(chunk_writer))?;
            }
            Ok(())
        })?;

        Ok(())
    }

    fn read(&self, r: &mut SaveReader, state: &mut SaveReadState) -> Result<(), IoError> {
        state.reset();
        let mut saw_meta = false;
        while let Some(name) = r.next_region()? {
            match name.as_str() {
                REGION_META => {
                    let expected = r.payload().len();
                    let mut wire = r.wire();
                    read_meta(&mut wire, state).map_err(|e| IoError::region_read(&name, e))?;
                    require_consumed(&name, expected, wire.pos())?;
                    saw_meta = true;
                }
                REGION_PATCHES => {
                    let expected = r.payload().len();
                    let mut wire = r.wire();
                    read_data_patches(&mut wire).map_err(|e| IoError::region_read(&name, e))?;
                    require_consumed(&name, expected, wire.pos())?;
                }
                REGION_CONTENT => {
                    let expected = r.payload().len();
                    let mut wire = r.wire();
                    let mapper = read_content_header(&mut wire, state.content.as_deref())
                        .map_err(|e| IoError::region_read(&name, e))?;
                    require_consumed(&name, expected, wire.pos())?;
                    if let (Some(mapper), Some(content)) = (mapper, state.content.as_deref_mut()) {
                        // Upstream `content.setTemporaryMapper(map)`; cleared by
                        // the `SaveIo::load` epilogue (`finally`).
                        content.set_temporary_mapper(Some(mapper));
                    }
                }
                REGION_MAP => {
                    let expected = r.payload().len();
                    let mut wire = r.wire();
                    read_map(&mut wire, state).map_err(|e| IoError::region_read(&name, e))?;
                    require_consumed(&name, expected, wire.pos())?;
                }
                REGION_ENTITIES => {
                    let expected = r.payload().len();
                    let mut wire = r.wire();
                    read_entities(&mut wire, state).map_err(|e| IoError::region_read(&name, e))?;
                    require_consumed(&name, expected, wire.pos())?;
                }
                REGION_MARKERS => {
                    let expected = r.payload().len();
                    let mut wire = r.wire();
                    match &mut state.markers {
                        Some(sink) => sink
                            .read_markers(&mut wire)
                            .map_err(|e| IoError::region_read(&name, e))?,
                        None => read_markers_stub(&mut wire)
                            .map_err(|e| IoError::region_read(&name, e))?,
                    }
                    require_consumed(&name, expected, wire.pos())?;
                }
                REGION_CUSTOM => {
                    let expected = r.payload().len();
                    let mut wire = r.wire();
                    read_custom_chunks(&mut wire, state)
                        .map_err(|e| IoError::region_read(&name, e))?;
                    require_consumed(&name, expected, wire.pos())?;
                }
                other => {
                    // Forward growth: unknown regions are skipped by length,
                    // never fatal (plan 04 §3.12.4).
                    log::warn!("skipping unknown save region `{other}`");
                }
            }
        }
        if !saw_meta {
            return Err(IoError::corrupt("save is missing the \"meta\" region"));
        }
        // `SaveVersion.readRules`: native v1 follows upstream v13+ and parses
        // the rules JSON after patches/content/map landed.
        read_rules(state)?;
        Ok(())
    }

    fn get_meta(&self, r: &mut SaveReader) -> Result<SaveMeta, IoError> {
        // Upstream reads the first chunk as the meta string map.
        let Some(name) = r.next_region()? else {
            return Err(IoError::UnexpectedEof);
        };
        if name != REGION_META {
            return Err(IoError::corrupt(format!(
                "first save region must be \"meta\", found \"{name}\""
            )));
        }
        let mut wire = r.wire();
        let tags = wire.string_map()?;
        SaveMeta::from_tags(self.version() as i32, tags)
    }
}

/// `SaveVersion.readMeta`: parses stats/locales eagerly and stashes the rules
/// JSON (parsed by [`read_rules`] after the other regions).
fn read_meta(wire: &mut WireReader, state: &mut SaveReadState) -> Result<(), IoError> {
    let map = wire.string_map()?;
    state.rule_string = Some(map.get("rules").cloned().unwrap_or_else(|| "{}".to_owned()));
    state.stats = Some(JsonIo::read(
        map.get("stats").map(String::as_str).unwrap_or("{}"),
    )?);
    state.locales = Some(JsonIo::read(
        map.get("locales").map(String::as_str).unwrap_or("{}"),
    )?);
    state.tags = map;
    Ok(())
}

/// `SaveVersion.readRules`: parse the stashed rules JSON.
///
/// Upstream then fills empty spawns from the generated wave table and applies
/// sector/planet overrides; those are plan 11/12 behavior and stay out of the
/// IO layer.
fn read_rules(state: &mut SaveReadState) -> Result<(), IoError> {
    let Some(text) = state.rule_string.as_deref() else {
        return Ok(());
    };
    state.rules = Some(JsonIo::read(text)?);
    Ok(())
}

/// `SaveVersion.readDataPatches` (stub): reads the header; patch entries are
/// plan 20 payloads, tolerated + skipped for now.
fn read_data_patches(wire: &mut WireReader) -> Result<(), IoError> {
    let _format_version = wire.i()?; // ignored upstream too
    let total = wire.i()?;
    if total < 0 {
        return Err(IoError::corrupt(format!(
            "invalid data patch count: {total}"
        )));
    }
    if total > 0 {
        log::warn!("skipping {total} data patch(es): patch payloads are plan 20");
        wire.skip_to_end();
    }
    Ok(())
}

/// `SaveVersion.writeContentHeader`: every mappable type with records, in
/// `ContentType` order, names in ID order.
fn write_content_header(
    wire: &mut WireWriter,
    content: Option<&ContentRegistry>,
) -> Result<(), IoError> {
    let Some(registry) = content else {
        wire.ub(0);
        return Ok(());
    };
    let mut mappable: Vec<(ContentType, Vec<String>)> = Vec::new();
    for type_ in ContentType::ALL {
        if registry.type_len(type_) == 0 {
            continue;
        }
        let entries = registry.entries(type_);
        // Upstream checks `arr.first() instanceof MappableContent`.
        if entries.first().and_then(|entry| entry.name).is_none() {
            continue;
        }
        let names: Vec<String> = entries
            .iter()
            .filter_map(|entry| entry.name.map(str::to_owned))
            .collect();
        mappable.push((type_, names));
    }
    if mappable.len() > u8::MAX as usize {
        return Err(IoError::corrupt("too many mapped content types"));
    }
    wire.ub(mappable.len() as u8);
    for (type_, names) in mappable {
        if names.len() > u16::MAX as usize {
            return Err(IoError::corrupt("too many content entries for one type"));
        }
        wire.ub(type_.ordinal() as u8);
        wire.us(names.len() as u16);
        for name in names {
            wire.str(&name)?;
        }
    }
    Ok(())
}

/// `SaveVersion.readContentHeader`: builds the temporary mapper. Unknown or
/// removed content maps to nothing (→ registry default, `MappedId::Default0`);
/// block names go through the [`map_fallback`] table first.
fn read_content_header(
    wire: &mut WireReader,
    content: Option<&ContentRegistry>,
) -> Result<Option<TemporaryMapper>, IoError> {
    let mapped = wire.ub()? as usize;
    let mut mapper = TemporaryMapper::new();
    for _ in 0..mapped {
        let type_ordinal = wire.ub()? as usize;
        let type_ = ContentType::ALL.get(type_ordinal).copied().ok_or_else(|| {
            IoError::corrupt(format!("invalid content type ordinal: {type_ordinal}"))
        })?;
        let total = wire.us()?;
        for id in 0..total {
            let name = wire.str()?;
            if let Some(registry) = content {
                // Fallback renames apply to blocks only (upstream).
                let resolved = if type_ == ContentType::Block {
                    map_fallback(&name)
                } else {
                    name.as_str()
                };
                let mapped_id = registry
                    .get_by_name(type_, resolved)
                    .map(|content| content.id);
                mapper.set(type_, id, mapped_id);
            }
        }
    }
    Ok(content.map(|_| mapper))
}

/// Maps a save-side block id through the temporary mapper
/// (`content.block(id)`); invalid/unknown ids become `air` (0).
fn map_block_id(registry: &ContentRegistry, raw: i32) -> u16 {
    registry
        .get_by_id(ContentType::Block, raw)
        .map(|content| content.id)
        .unwrap_or(0)
}

/// `SaveVersion.writeMap`: world size, floor/overlay RLE pass, block pass with
/// per-tile data + nested tile-entity chunks at multiblock centers.
fn write_map(
    wire: &mut WireWriter,
    scratch: &mut super::super::chunk::SaveScratch,
    map: &dyn super::super::state::MapSource,
) -> Result<(), IoError> {
    let width = map.width();
    let height = map.height();
    wire.us(width);
    wire.us(height);
    let len = width as usize * height as usize;

    // Floor + overlay pass.
    let mut i = 0;
    while i < len {
        let floor = map.floor_id(i);
        let overlay = map.overlay_id(i);
        wire.us(floor);
        wire.us(overlay);
        let mut consecutives = 0usize;
        while i + 1 + consecutives < len && consecutives < 255 {
            let j = i + 1 + consecutives;
            if map.floor_id(j) != floor || map.overlay_id(j) != overlay {
                break;
            }
            consecutives += 1;
        }
        wire.ub(consecutives as u8);
        i += consecutives + 1;
    }

    // Block pass.
    let mut i = 0;
    while i < len {
        let block = map.block_id(i);
        wire.us(block);

        let has_building = map.has_building(i);
        let save_data = map.should_save_data(i);
        // bit0: entity present; bit2: 7-byte tile data present (upstream layout).
        let packed = u8::from(has_building) | if save_data { 4 } else { 0 };
        wire.ub(packed);

        if save_data {
            let (data, floor_data, overlay_data, extra_data) = map.tile_data(i);
            wire.ub(data);
            wire.ub(floor_data);
            wire.ub(overlay_data);
            wire.i(extra_data);
        }

        if has_building {
            // Only multiblock centers carry the entity chunk.
            if map.is_center(i) {
                wire.bool(true);
                scratch.write_chunk(wire, |chunk| map.write_building(i, chunk))?;
            } else {
                wire.bool(false);
            }
        } else if !save_data {
            // Run of identical non-entity, non-data blocks (upstream compares
            // block id + shouldSaveData only; build-ness follows from the block).
            let mut consecutives = 0usize;
            while i + 1 + consecutives < len && consecutives < 255 {
                let j = i + 1 + consecutives;
                if map.block_id(j) != block || map.should_save_data(j) != save_data {
                    break;
                }
                consecutives += 1;
            }
            wire.ub(consecutives as u8);
            i += consecutives;
        }
        i += 1;
    }
    Ok(())
}

/// `SaveVersion.readMap` (ported against [`super::super::state::WorldContext`]).
fn read_map(wire: &mut WireReader, state: &mut SaveReadState) -> Result<(), IoError> {
    let width = wire.us()?;
    let height = wire.us()?;
    if width == 0 && height == 0 {
        return Ok(());
    }
    let len = width as usize * height as usize;
    let preview = state.preview;
    let all_buildings = &mut state.all_buildings;
    let Some(context) = state.context.as_deref_mut() else {
        log::warn!("skipping {width}x{height} map region: no world context");
        wire.skip_to_end();
        return Ok(());
    };
    let Some(registry) = state.content.as_deref() else {
        return Err(IoError::corrupt("map region requires a content registry"));
    };

    let generating = context.is_generating();
    if !generating {
        context.begin();
    }
    let result = read_map_body(
        wire,
        preview,
        all_buildings,
        context,
        registry,
        width,
        height,
        len,
    );
    if !generating {
        context.end();
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn read_map_body(
    wire: &mut WireReader,
    preview: bool,
    all_buildings: &mut Vec<usize>,
    context: &mut dyn super::super::state::WorldContext,
    registry: &ContentRegistry,
    width: u16,
    height: u16,
    len: usize,
) -> Result<(), IoError> {
    let stone = registry
        .block_id("stone")
        .map(|id| id.raw())
        .ok_or_else(|| IoError::corrupt("registry is missing the `stone` floor"))?;
    context.resize(width, height);
    let width = width as usize;

    // Floor pass: `air` floors become `stone` (`Blocks.stone.id`).
    let mut i = 0usize;
    while i < len {
        let floor_raw = i32::from(wire.s()?);
        let ore_raw = i32::from(wire.s()?);
        let consecutives = wire.ub()? as usize;
        if i + consecutives >= len {
            return Err(IoError::corrupt("floor RLE run exceeds the tile count"));
        }
        let mut floor = map_block_id(registry, floor_raw);
        if floor == 0 {
            floor = stone;
        }
        let ore = map_block_id(registry, ore_raw);
        for j in 0..=consecutives {
            let index = i + j;
            context.create(
                (index % width) as u16,
                (index / width) as u16,
                floor,
                ore,
                0,
            );
        }
        i += consecutives + 1;
    }

    // Block pass.
    let mut i = 0usize;
    while i < len {
        let block_raw = i32::from(wire.s()?);
        let block = map_block_id(registry, block_raw);
        let packed = wire.ub()?;
        let had_entity = packed & 1 != 0;
        let had_data = packed & 4 != 0;

        let mut data = 0u8;
        let mut floor_data = 0u8;
        let mut overlay_data = 0u8;
        let mut extra_data = 0i32;
        if had_data {
            data = wire.ub()?;
            floor_data = wire.ub()?;
            overlay_data = wire.ub()?;
            extra_data = wire.i()?;
        }

        let is_center = !had_entity || wire.bool()?;

        if is_center {
            context.set_block(i, block);
            if context.has_building(i) && !preview {
                all_buildings.push(i);
            }
        }
        if had_data {
            // Assigned after set_block (it can reset data upstream).
            context.set_tile_data(i, data, floor_data, overlay_data, extra_data);
            context.on_read_tile_data(i);
        }
        if had_entity {
            if is_center {
                if context.block_has_building_io(i) {
                    let chunk_len = wire.u()? as usize;
                    let payload = wire.bytes(chunk_len)?;
                    let mut chunk = WireReader::new(payload);
                    let version = chunk.ub()?;
                    context.read_building(i, &mut chunk, version).map_err(|e| {
                        IoError::corrupt(format!(
                            "Failed to read tile entity of block: {block}: {e}"
                        ))
                    })?;
                } else {
                    // The block lost its building IO (removed/changed): skip.
                    let chunk_len = wire.u()? as usize;
                    wire.skip(chunk_len)?;
                }
                context.on_read_building(i);
            }
        } else if !had_data {
            let consecutives = wire.ub()? as usize;
            if i + consecutives >= len {
                return Err(IoError::corrupt("block RLE run exceeds the tile count"));
            }
            for j in 1..=consecutives {
                context.set_block(i + j, block);
            }
            i += consecutives;
        }
        i += 1;
    }
    Ok(())
}

/// `SaveVersion.writeEntities`: custom ID mapping, team build plans, then the
/// entity chunks.
fn write_entities(
    wire: &mut WireWriter,
    scratch: &mut super::super::chunk::SaveScratch,
    entities: &dyn super::super::state::EntitySource,
) -> Result<(), IoError> {
    // `writeEntityMapping`.
    let id_map = entities.entity_id_map();
    wire.us(id_map.len() as u16);
    for (id, name) in id_map.iter() {
        wire.us(id);
        wire.str(name)?;
    }

    // `writeTeamBlocks`.
    let teams = entities.team_plans();
    wire.i(teams.len() as i32);
    for (team, plans) in &teams {
        wire.i(*team);
        wire.i(plans.len() as i32);
        for plan in plans {
            wire.s(plan.x);
            wire.s(plan.y);
            wire.s(plan.rotation);
            wire.us(plan.block.raw());
            typeio::write_object(wire, &plan.config)?;
        }
    }

    // `writeWorldEntities`.
    wire.i(entities.entity_count() as i32);
    entities.write_entities(wire, scratch)?;
    Ok(())
}

/// `SaveVersion.readEntities`.
fn read_entities(wire: &mut WireReader, state: &mut SaveReadState) -> Result<(), IoError> {
    // `readEntityMapping`: custom ids override the global mapping by name.
    let mut custom = EntityIdMap::new();
    let mapped = wire.us()?;
    for _ in 0..mapped {
        let id = wire.us()?;
        let name = wire.str()?;
        custom.insert(id, &name);
    }

    // `readTeamBlocks`.
    let team_count = wire.i()?;
    if team_count < 0 {
        return Err(IoError::corrupt(format!(
            "invalid team count: {team_count}"
        )));
    }
    for _ in 0..team_count {
        let team = wire.i()?;
        let blocks = wire.i()?;
        if blocks < 0 {
            return Err(IoError::corrupt(format!(
                "invalid team plan count: {blocks}"
            )));
        }
        let mut plans = Vec::with_capacity((blocks as usize).min(1000));
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..blocks {
            let x = wire.s()?;
            let y = wire.s()?;
            let rotation = wire.s()?;
            let block = wire.us()?;
            let config = typeio::read_object(wire)?;
            // Two plans cannot share a position (`IntSet.add(Point2.pack)`).
            if seen.insert(typeio::pack_point2(i32::from(x), i32::from(y))) {
                plans.push(TeamPlan {
                    x,
                    y,
                    rotation,
                    block: BlockId::new(block),
                    config,
                });
            }
        }
        state.team_plans.push((team, plans));
    }

    // `readWorldEntities`: unknown class IDs are skipped by length; the
    // `afterReadAll` pass runs at the end.
    let amount = wire.i()?;
    if amount < 0 {
        return Err(IoError::corrupt(format!("invalid entity count: {amount}")));
    }
    for _ in 0..amount {
        let chunk_len = wire.u()? as usize;
        let payload = wire.bytes(chunk_len)?;
        let mut chunk = WireReader::new(payload);
        let class_id = chunk.ub()?;
        let custom_name = custom.name_of(u16::from(class_id));
        let supported = state
            .entities
            .as_deref()
            .map(|sink| sink.supports_class(class_id, custom_name))
            .unwrap_or(false);
        if supported {
            let id = chunk.i()?;
            if let Some(sink) = state.entities.as_deref_mut() {
                sink.read_entity(class_id, custom_name, id, &mut chunk)?;
            }
        }
        // Unknown/no-sink chunks are discarded (payload already sliced).
    }
    if let Some(sink) = state.entities.as_deref_mut() {
        sink.after_read_all();
    }
    Ok(())
}

/// Markers stub for reads without a plan-12 sink (`MapMarkers` payload).
fn read_markers_stub(wire: &mut WireReader) -> Result<(), IoError> {
    let count = wire.i()?;
    if count < 0 {
        return Err(IoError::corrupt(format!("invalid marker count: {count}")));
    }
    if count > 0 {
        log::warn!("skipping {count} map marker(s): markers are plan 12");
        wire.skip_to_end();
    }
    Ok(())
}

/// `SaveVersion.readCustomChunks`: unknown names are skipped by length, never
/// fatal.
fn read_custom_chunks(wire: &mut WireReader, state: &mut SaveReadState) -> Result<(), IoError> {
    let amount = wire.i()?;
    if amount < 0 {
        return Err(IoError::corrupt(format!(
            "invalid custom chunk count: {amount}"
        )));
    }
    for _ in 0..amount {
        let name = wire.str()?;
        let len = wire.u()? as usize;
        let registered = state
            .custom_chunks
            .and_then(|registry| registry.get(&name).cloned());
        match registered {
            Some(chunk) => {
                let payload = wire.bytes(len)?;
                let mut reader = WireReader::new(payload);
                chunk.read(&mut reader, len)?;
            }
            None => {
                wire.skip(len)?;
                log::debug!("skipped custom chunk `{name}` ({len} bytes): not registered");
            }
        }
    }
    Ok(())
}

/// Shared meta tag computation for tests and hosts (plan 04 §6.2 key set).
pub fn base_meta_tags(width: u16, height: u16, wave: i32, map_name: &str) -> StringMap {
    let mut tags = StringMap::new();
    tags.insert("version".to_owned(), "1".to_owned());
    tags.insert("saved".to_owned(), "0".to_owned());
    tags.insert("playtime".to_owned(), "0".to_owned());
    tags.insert("build".to_owned(), crate::version::BUILD.to_string());
    tags.insert("mapname".to_owned(), map_name.to_owned());
    tags.insert("wave".to_owned(), wave.to_string());
    tags.insert("tick".to_owned(), "0".to_owned());
    tags.insert("wavetime".to_owned(), "0".to_owned());
    tags.insert("stats".to_owned(), "{}".to_owned());
    tags.insert("rules".to_owned(), "{}".to_owned());
    tags.insert("sectorPreset".to_owned(), String::new());
    tags.insert("locales".to_owned(), "{}".to_owned());
    tags.insert("mods".to_owned(), "[]".to_owned());
    tags.insert("controlGroups".to_owned(), "null".to_owned());
    tags.insert("width".to_owned(), width.to_string());
    tags.insert("height".to_owned(), height.to_string());
    tags.insert("viewpos".to_owned(), "(0,0)".to_owned());
    tags.insert("controlledType".to_owned(), "null".to_owned());
    tags.insert("nocores".to_owned(), "true".to_owned());
    tags.insert("playerteam".to_owned(), "1".to_owned());
    tags.insert("hasExternalAssets".to_owned(), "false".to_owned());
    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::save::version::WriteContext;
    use crate::io::save::{SaveIo, SaveOptions};

    #[test]
    fn v1_writes_all_regions_in_order() {
        let tags = base_meta_tags(0, 0, 3, "empty");
        let ctx = WriteContext::meta_only(tags);
        let bytes = SaveIo::write_to_vec(&ctx, &SaveOptions::new()).unwrap();
        assert_eq!(&bytes[..4], b"MGRS");

        // Regions appear in canonical order when listed by the raw reader.
        let mut reader = SaveIo::open_native(&bytes).unwrap().1;
        let mut names = Vec::new();
        while let Some(name) = reader.next_region().unwrap() {
            names.push(name);
        }
        assert_eq!(
            names,
            vec![
                "meta", "patches", "content", "map", "entities", "markers", "custom"
            ]
        );
    }

    #[test]
    fn content_header_roundtrip_with_mapper() {
        let registry = crate::content::test_support::test_registry();
        let ctx_tags = base_meta_tags(0, 0, 0, "empty");
        let mut ctx = WriteContext::meta_only(ctx_tags);
        ctx.content = Some(&registry);
        let bytes = SaveIo::write_to_vec(&ctx, &SaveOptions::new()).unwrap();

        let mut state = SaveReadState::default();
        let mut registry2 = crate::content::test_support::test_registry();
        state.content = Some(&mut registry2);
        SaveIo::load_bytes(&bytes, &mut state).unwrap();
        // The temporary mapper is installed and maps vanilla IDs to themselves.
        let mapped = registry2.get_by_id(ContentType::Block, 5).unwrap();
        assert_eq!(mapped.id, 5);
    }

    #[test]
    fn meta_roundtrip_preserves_tags() {
        let mut tags = base_meta_tags(16, 24, 9, "groundZero");
        tags.insert("custom".to_owned(), "value".to_owned());
        let ctx = WriteContext::meta_only(tags.clone());
        let bytes = SaveIo::write_to_vec(&ctx, &SaveOptions::new()).unwrap();
        let meta = SaveIo::get_meta_bytes(&bytes).unwrap();
        assert_eq!(meta.version, 1);
        assert_eq!(meta.map_name, "groundZero");
        assert_eq!(meta.wave, 9);
        assert_eq!(meta.width(), 16);
        assert_eq!(meta.height(), 24);
        assert_eq!(meta.tags.get("custom").unwrap(), "value");
        assert!(!meta.is_map());
    }
}
