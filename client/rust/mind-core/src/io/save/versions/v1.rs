// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Native save format v1 (plan 04 §3.3, current writer).
//!
//! Ported from `core/src/mindustry/io/SaveVersion.java` (region layout and
//! read/write logic). Native v1 always writes all seven regions; readers
//! tolerate unknown region names and missing trailing regions for forward
//! growth. M0 writes an empty world (`0×0` map, no entities); the tile/entity
//! payload writers land in M4 against [`super::super::state::WorldContext`].
//!
//! Framing (plan 04 §6.1):
//! - `meta`: `i16` count + (`u16`-len UTF-8 key, value) pairs.
//! - `patches`: `i32` patch format version + `i32` asset count (empty until 20).
//! - `content`: `u8` mapped types + per type (`u8` ordinal, `u16` count, names).
//! - `map`: `u16` width, `u16` height, then tile passes (M4).
//! - `entities`: `u16` id-map count, `i32` team count, `i32` entity count (M4).
//! - `markers`: `i32` count (payload owned by plan 12).
//! - `custom`: `i32` chunk count + (name, chunk) pairs (registry owned by 20).

use super::super::chunk::{
    REGION_CONTENT, REGION_CUSTOM, REGION_ENTITIES, REGION_MAP, REGION_MARKERS, REGION_META,
    REGION_PATCHES, SaveReader, SaveWriter, map_fallback, require_consumed,
};
use super::super::meta::SaveMeta;
use super::super::options::SaveOptions;
use super::super::state::SaveReadState;
use super::super::version::SaveVersion;
use crate::content::load::TemporaryMapper;
use crate::content::{ContentRegistry, ContentType};
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

        w.write_region(REGION_PATCHES, |wire, _scratch| {
            wire.i(PATCH_FORMAT_VERSION);
            wire.i(0);
            Ok(())
        })?;

        let content = w.ctx.content;
        w.write_region(REGION_CONTENT, |wire, _scratch| {
            write_content_header(wire, content)
        })?;

        // M0 empty world: 0x0 map, no entities/markers/custom chunks.
        w.write_region(REGION_MAP, |wire, _scratch| {
            wire.us(0);
            wire.us(0);
            Ok(())
        })?;

        w.write_region(REGION_ENTITIES, |wire, _scratch| {
            wire.us(0); // entity ID mapping
            wire.i(0); // team build plans
            wire.i(0); // world entities
            Ok(())
        })?;

        w.write_region(REGION_MARKERS, |wire, _scratch| {
            wire.i(0);
            Ok(())
        })?;

        w.write_region(REGION_CUSTOM, |wire, _scratch| {
            wire.i(0);
            Ok(())
        })?;

        Ok(())
    }

    fn read(&self, r: &mut SaveReader<'_>, state: &mut SaveReadState) -> Result<(), IoError> {
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
                        // the load epilogue (`SaveIO.load` finally block).
                        content.set_temporary_mapper(Some(mapper));
                    }
                }
                REGION_MAP => {
                    let expected = r.payload().len();
                    let mut wire = r.wire();
                    read_map_stub(&mut wire).map_err(|e| IoError::region_read(&name, e))?;
                    require_consumed(&name, expected, wire.pos())?;
                }
                REGION_ENTITIES => {
                    let expected = r.payload().len();
                    let mut wire = r.wire();
                    read_entities_stub(&mut wire).map_err(|e| IoError::region_read(&name, e))?;
                    require_consumed(&name, expected, wire.pos())?;
                }
                REGION_MARKERS => {
                    let expected = r.payload().len();
                    let mut wire = r.wire();
                    read_markers_stub(&mut wire).map_err(|e| IoError::region_read(&name, e))?;
                    require_consumed(&name, expected, wire.pos())?;
                }
                REGION_CUSTOM => {
                    let expected = r.payload().len();
                    let mut wire = r.wire();
                    read_custom_chunks(&mut wire).map_err(|e| IoError::region_read(&name, e))?;
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
        Ok(())
    }

    fn get_meta(&self, r: &mut SaveReader<'_>) -> Result<SaveMeta, IoError> {
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

/// `SaveVersion.writeMeta`: the caller computes the standard tag set (plan 04
/// §6.2); the writer only merges + serializes.
fn read_meta(wire: &mut WireReader, state: &mut SaveReadState) -> Result<(), IoError> {
    let map = wire.string_map()?;
    // `SaveVersion.readMeta`: the rules JSON is stashed and parsed after data
    // patches land (plan 04 M6 wires the `JsonIo` parse).
    state.rule_string = Some(map.get("rules").cloned().unwrap_or_else(|| "{}".to_owned()));
    state.tags = map;
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

/// M0 map stub: reads the size; tile passes land in M4.
fn read_map_stub(wire: &mut WireReader) -> Result<(), IoError> {
    let width = wire.us()?;
    let height = wire.us()?;
    if width > 0 || height > 0 {
        log::warn!("skipping {width}x{height} map region: tile data lands in M4");
        wire.skip_to_end();
    }
    Ok(())
}

/// M0 entities stub: reads the three section counts; payload lands in M4.
fn read_entities_stub(wire: &mut WireReader) -> Result<(), IoError> {
    let id_map = wire.us()?;
    let teams = wire.i()?;
    let entities = wire.i()?;
    if id_map > 0 || teams > 0 || entities > 0 {
        log::warn!(
            "skipping entities region ({id_map} mapped, {teams} teams, {entities} entities): entity IO lands in M4"
        );
        wire.skip_to_end();
    }
    Ok(())
}

/// M0 markers stub (`MapMarkers.write` payload is plan 12's).
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
/// fatal. No chunks are registered before plan 20.
fn read_custom_chunks(wire: &mut WireReader) -> Result<(), IoError> {
    let amount = wire.i()?;
    if amount < 0 {
        return Err(IoError::corrupt(format!(
            "invalid custom chunk count: {amount}"
        )));
    }
    for _ in 0..amount {
        let name = wire.str()?;
        let len = wire.u()? as usize;
        wire.skip(len)?;
        log::debug!("skipped custom chunk `{name}` ({len} bytes): none registered");
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
