// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Region/chunk primitives for the native `MGRS` container (plan 04 §3.2/§6.1).
//!
//! Ported from `core/src/mindustry/io/SaveFileReader.java`: length-prefixed
//! chunks, one-level nesting via two reusable scratch buffers, string maps, the
//! legacy block-name fallback tables, and the exact region length-mismatch
//! error text. Native regions additionally carry their name (`u8` length + UTF-8)
//! so readers can skip unknown regions for forward growth (plan 04 §3.3).
//!
//! Also ported: `core/src/mindustry/io/SaveIO.java` deflate framing — raw zlib
//! (`flate2` rust backend, R5) wraps everything after the 4-byte magic, with a
//! decompressed-size guard.

use std::io::Read;

use super::super::wire::{WireReader, WireWriter};
use super::super::{IoError, StringMap};
use super::version::WriteContext;

/// Region payload cap (`MAX_REGION_BYTES`, plan 04 §3.2): 128 MiB.
pub const MAX_REGION_BYTES: usize = 128 * 1024 * 1024;

/// Whole-stream decompressed cap (zip-bomb guard, plan 04 §3.3): 512 MiB.
pub const MAX_DECOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;

/// Region names in write order (plan 04 §6.1). Append-only; readers tolerate
/// unknown names and missing trailing regions.
pub const REGION_META: &str = "meta";
/// `patches` region (data patches; payload owned by plan 20).
pub const REGION_PATCHES: &str = "patches";
/// `content` region (mappable content name header).
pub const REGION_CONTENT: &str = "content";
/// `map` region (tile data).
pub const REGION_MAP: &str = "map";
/// `entities` region (entity ID mapping, team plans, entity chunks).
pub const REGION_ENTITIES: &str = "entities";
/// `markers` region (map markers; payload owned by plan 12).
pub const REGION_MARKERS: &str = "markers";
/// `custom` region (mod custom chunks; registry owned by plan 20).
pub const REGION_CUSTOM: &str = "custom";

/// All native v1 regions in canonical write order.
pub const REGION_ORDER: [&str; 7] = [
    REGION_META,
    REGION_PATCHES,
    REGION_CONTENT,
    REGION_MAP,
    REGION_ENTITIES,
    REGION_MARKERS,
    REGION_CUSTOM,
];

/// Legacy block-name fallback table (`SaveFileReader.fallback`).
///
/// Applied to block names read from a save's content header before registry
/// lookup; unknown names still resolve to nothing (→ default content).
/// Append-only, parity ABI.
pub const SAVE_FALLBACK: &[(&str, &str)] = &[
    ("dart-mech-pad", "legacy-mech-pad"),
    ("dart-ship-pad", "legacy-mech-pad"),
    ("javelin-ship-pad", "legacy-mech-pad"),
    ("trident-ship-pad", "legacy-mech-pad"),
    ("glaive-ship-pad", "legacy-mech-pad"),
    ("alpha-mech-pad", "legacy-mech-pad"),
    ("tau-mech-pad", "legacy-mech-pad"),
    ("omega-mech-pad", "legacy-mech-pad"),
    ("delta-mech-pad", "legacy-mech-pad"),
    ("draug-factory", "legacy-unit-factory"),
    ("spirit-factory", "legacy-unit-factory"),
    ("phantom-factory", "legacy-unit-factory"),
    ("wraith-factory", "legacy-unit-factory"),
    ("ghoul-factory", "legacy-unit-factory-air"),
    ("revenant-factory", "legacy-unit-factory-air"),
    ("dagger-factory", "legacy-unit-factory"),
    ("crawler-factory", "legacy-unit-factory"),
    ("titan-factory", "legacy-unit-factory-ground"),
    ("fortress-factory", "legacy-unit-factory-ground"),
    ("mass-conveyor", "payload-conveyor"),
    ("vestige", "scepter"),
    ("turbine-generator", "steam-generator"),
    ("rocks", "stone-wall"),
    ("sporerocks", "spore-wall"),
    ("icerocks", "ice-wall"),
    ("dunerocks", "dune-wall"),
    ("sandrocks", "sand-wall"),
    ("shalerocks", "shale-wall"),
    ("snowrocks", "snow-wall"),
    ("saltrocks", "salt-wall"),
    ("dirtwall", "dirt-wall"),
    ("ignarock", "basalt"),
    ("holostone", "dacite"),
    ("holostone-wall", "dacite-wall"),
    ("rock", "boulder"),
    ("snowrock", "snow-boulder"),
    ("cliffs", "stone-wall"),
    ("craters", "crater-stone"),
    ("deepwater", "deep-water"),
    ("water", "shallow-water"),
    ("sand", "sand-floor"),
    ("slag", "molten-slag"),
    ("cryofluidmixer", "cryofluid-mixer"),
    ("block-forge", "constructor"),
    ("block-unloader", "payload-unloader"),
    ("block-loader", "payload-loader"),
    ("thermal-pump", "impulse-pump"),
    ("alloy-smelter", "surge-smelter"),
    ("steam-vent", "rhyolite-vent"),
    ("fabricator", "tank-fabricator"),
    ("basic-reconstructor", "refabricator"),
];

/// `SaveFileReader.mapFallback`: the fallback rename for `name`, else `name`.
pub fn map_fallback(name: &str) -> &str {
    SAVE_FALLBACK
        .iter()
        .find_map(|(from, to)| (*from == name).then_some(*to))
        .unwrap_or(name)
}

/// Streaming reader over the inflated container body with a decompressed-size
/// guard (plan 04 §3.3). All reads are exact-length and bounds-checked.
pub struct InflateReader<R: Read> {
    inner: R,
    count: u64,
    limit: u64,
}

impl<R: Read> InflateReader<R> {
    /// Wraps an inflated source with a byte cap.
    pub fn new(inner: R, limit: u64) -> Self {
        Self {
            inner,
            count: 0,
            limit,
        }
    }

    /// Total inflated bytes consumed so far.
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Reads exactly `buf.len()` bytes.
    pub fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), IoError> {
        self.inner.read_exact(buf).map_err(|error| {
            if error.kind() == std::io::ErrorKind::UnexpectedEof {
                IoError::UnexpectedEof
            } else {
                IoError::Io(error)
            }
        })?;
        self.count = self.count.saturating_add(buf.len() as u64);
        if self.count > self.limit {
            return Err(IoError::DecompressedTooLarge { limit: self.limit });
        }
        Ok(())
    }

    /// Reads one byte; `Ok(None)` on a clean stream end (region boundary).
    pub fn try_u8(&mut self) -> Result<Option<u8>, IoError> {
        let mut byte = [0u8; 1];
        match self.inner.read(&mut byte) {
            Ok(0) => Ok(None),
            Ok(_) => {
                self.count = self.count.saturating_add(1);
                Ok(Some(byte[0]))
            }
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
            Err(error) => Err(IoError::Io(error)),
        }
    }

    /// Big-endian `u32` (container format version).
    pub fn u32(&mut self) -> Result<u32, IoError> {
        let mut bytes = [0u8; 4];
        self.read_exact(&mut bytes)?;
        Ok(u32::from_be_bytes(bytes))
    }
}

/// Reader for one open `MGRS` container, positioned after the format version.
///
/// Region payloads are buffered into a reusable scratch buffer (capped at
/// [`MAX_REGION_BYTES`]); handlers read them through [`WireReader`] and must
/// consume them exactly (checked by [`require_consumed`]).
pub struct SaveReader<'a> {
    input: InflateReader<Box<dyn Read + 'a>>,
    scratch: Vec<u8>,
}

impl<'a> SaveReader<'a> {
    /// Prepares a reader positioned at the first region.
    pub(crate) fn new(input: InflateReader<Box<dyn Read + 'a>>) -> Self {
        Self {
            input,
            scratch: Vec::new(),
        }
    }

    /// Reads the next region header and payload into scratch.
    ///
    /// Returns the region name, or `None` at a clean stream end. Unknown region
    /// names are the forward-growth path: callers skip their payloads.
    pub fn next_region(&mut self) -> Result<Option<String>, IoError> {
        let Some(name_len) = self.input.try_u8()? else {
            return Ok(None);
        };
        let mut name_bytes = vec![0u8; name_len as usize];
        self.input.read_exact(&mut name_bytes)?;
        let name = std::str::from_utf8(&name_bytes)?.to_owned();
        let len = self.input.u32()? as usize;
        if len > MAX_REGION_BYTES {
            return Err(IoError::RegionTooLarge {
                limit: MAX_REGION_BYTES,
            });
        }
        self.scratch.clear();
        self.scratch.resize(len, 0);
        self.input.read_exact(&mut self.scratch)?;
        Ok(Some(name))
    }

    /// The current region payload.
    pub fn payload(&self) -> &[u8] {
        &self.scratch
    }

    /// A [`WireReader`] over the current region payload.
    pub fn wire(&self) -> WireReader<'_> {
        WireReader::new(&self.scratch)
    }
}

/// Verifies a region handler consumed its payload exactly
/// (`SaveFileReader.readRegion` length check; upstream message text kept).
pub fn require_consumed(name: &str, expected: usize, actual: usize) -> Result<(), IoError> {
    if actual != expected {
        return Err(IoError::RegionLengthMismatch {
            name: name.to_owned(),
            expected,
            actual,
        });
    }
    Ok(())
}

/// Writer for one open `MGRS` container: owns the downstream stream, the
/// scratch buffers and the write context (data source).
pub struct SaveWriter<'a> {
    /// Downstream (deflating) stream, after magic + format version.
    pub out: &'a mut dyn std::io::Write,
    /// Region/chunk scratch buffers.
    pub scratch: SaveScratch,
    /// Data source for region writers.
    pub ctx: &'a WriteContext<'a>,
}

impl SaveWriter<'_> {
    /// Writes one named region (`name` + `u32` length + payload).
    ///
    /// The closure receives the region payload writer and a scratch handle for
    /// nested chunks (`SaveFileReader.writeRegion`).
    pub fn write_region(
        &mut self,
        name: &str,
        f: impl FnOnce(&mut WireWriter, &mut SaveScratch) -> Result<(), IoError>,
    ) -> Result<(), IoError> {
        self.scratch.write_region(self.out, name, f)
    }
}

/// Reusable scratch buffers for chunk serialization
/// (`SaveFileReader.byteOutput`/`byteOutput2` + `chunkNested`).
///
/// Regions are written at depth 0 into the outer buffer; nested chunks at
/// depth 1 use the inner buffer. Deeper nesting is an error, matching
/// upstream's one-level-nested static buffers.
#[derive(Debug, Default)]
pub struct SaveScratch {
    outer: Vec<u8>,
    inner: Vec<u8>,
    depth: usize,
}

impl SaveScratch {
    /// Empty scratch.
    pub fn new() -> Self {
        Self::default()
    }

    /// Writes a named region: `u8` name length + name + `u32` payload length +
    /// payload. Region-level only (depth 0).
    pub fn write_region(
        &mut self,
        out: &mut dyn std::io::Write,
        name: &str,
        f: impl FnOnce(&mut WireWriter, &mut SaveScratch) -> Result<(), IoError>,
    ) -> Result<(), IoError> {
        if self.depth != 0 {
            return Err(IoError::ChunkNesting);
        }
        let mut buf = std::mem::take(&mut self.outer);
        self.depth = 1;
        let result = {
            let mut writer = WireWriter::new(&mut buf);
            f(&mut writer, self)
        };
        self.depth = 0;
        let final_result = match result {
            Ok(()) => {
                let name_bytes = name.as_bytes();
                if name_bytes.len() > u8::MAX as usize {
                    Err(IoError::TooLarge {
                        limit: u8::MAX as usize,
                        actual: name_bytes.len(),
                    })
                } else {
                    out.write_all(&[name_bytes.len() as u8])
                        .and_then(|()| out.write_all(name_bytes))
                        .and_then(|()| out.write_all(&(buf.len() as u32).to_be_bytes()))
                        .and_then(|()| out.write_all(&buf))
                        .map_err(IoError::Io)
                }
            }
            Err(error) => Err(error),
        };
        buf.clear();
        self.outer = buf;
        final_result.map_err(|error| IoError::region_write(name, error))
    }

    /// Writes a nested length-prefixed chunk (`u32` length + payload) into a
    /// parent region stream (`SaveFileReader.writeChunk`). Depth 1 only.
    pub fn write_chunk(
        &mut self,
        out: &mut WireWriter,
        f: impl FnOnce(&mut WireWriter) -> Result<(), IoError>,
    ) -> Result<(), IoError> {
        if self.depth != 1 {
            return Err(IoError::ChunkNesting);
        }
        let mut buf = std::mem::take(&mut self.inner);
        self.depth = 2;
        let result = {
            let mut writer = WireWriter::new(&mut buf);
            f(&mut writer)
        };
        self.depth = 1;
        let final_result = result.map(|()| {
            out.u(buf.len() as u32);
            out.bytes(&buf);
        });
        buf.clear();
        self.inner = buf;
        final_result
    }
}

/// Writes a string map into a wire stream (`SaveFileReader.writeStringMap`).
pub fn write_string_map(wire: &mut WireWriter, map: &StringMap) -> Result<(), IoError> {
    wire.string_map(map)
}

/// Reads a string map from a wire stream (`SaveFileReader.readStringMap`).
pub fn read_string_map(wire: &mut WireReader) -> Result<StringMap, IoError> {
    wire.string_map()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region_bytes(name: &str, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(name.len() as u8);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        out.extend_from_slice(payload);
        out
    }

    fn reader_for(bytes: &'static [u8]) -> SaveReader<'static> {
        // Leak is acceptable in tests: SaveReader owns a boxed dyn Read.
        let cursor: Box<dyn Read> = Box::new(std::io::Cursor::new(bytes));
        SaveReader::new(InflateReader::new(cursor, MAX_DECOMPRESSED_BYTES))
    }

    static EMPTY: [u8; 0] = [];

    #[test]
    fn region_roundtrip_with_nested_chunk() {
        // Write: one "meta" region containing a string map, then a "custom"
        // region with two nested chunks.
        let mut out = Vec::new();
        let mut scratch = SaveScratch::new();
        scratch
            .write_region(&mut out, REGION_META, |wire, _scratch| {
                let mut map = StringMap::new();
                map.insert("wave".to_owned(), "7".to_owned());
                map.insert("mapname".to_owned(), "groundZero".to_owned());
                wire.string_map(&map)
            })
            .unwrap();
        scratch
            .write_region(&mut out, REGION_CUSTOM, |wire, scratch| {
                wire.i(2);
                scratch.write_chunk(wire, |chunk| {
                    chunk.str("first")?;
                    chunk.i(42);
                    Ok(())
                })?;
                scratch.write_chunk(wire, |chunk| {
                    chunk.bytes(&[1, 2, 3]);
                    Ok(())
                })?;
                Ok(())
            })
            .unwrap();

        // Read back.
        let leaked: &'static [u8] = Box::leak(out.into_boxed_slice());
        let mut reader = reader_for(leaked);

        let name = reader.next_region().unwrap().unwrap();
        assert_eq!(name, REGION_META);
        let mut wire = reader.wire();
        let map = wire.string_map().unwrap();
        assert_eq!(map.get("wave").unwrap(), "7");
        assert_eq!(map.get("mapname").unwrap(), "groundZero");
        require_consumed(&name, reader.payload().len(), wire.pos()).unwrap();

        let name = reader.next_region().unwrap().unwrap();
        assert_eq!(name, REGION_CUSTOM);
        let mut wire = reader.wire();
        assert_eq!(wire.i().unwrap(), 2);
        // Nested chunk 1.
        let len = wire.u().unwrap() as usize;
        let mut chunk = WireReader::new(wire.bytes(len).unwrap());
        assert_eq!(chunk.str().unwrap(), "first");
        assert_eq!(chunk.i().unwrap(), 42);
        assert_eq!(chunk.remaining(), 0);
        // Nested chunk 2.
        let len = wire.u().unwrap() as usize;
        let mut chunk = WireReader::new(wire.bytes(len).unwrap());
        assert_eq!(chunk.bytes(3).unwrap(), &[1, 2, 3]);
        assert_eq!(chunk.remaining(), 0);
        require_consumed(&name, reader.payload().len(), wire.pos()).unwrap();

        assert!(reader.next_region().unwrap().is_none());
    }

    #[test]
    fn region_length_mismatch_is_detected() {
        // Payload declares 4 bytes but the handler consumes only 2.
        let payload = [0u8, 0, 0, 0];
        let bytes = region_bytes("meta", &payload);
        let leaked: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        let mut reader = reader_for(leaked);
        let name = reader.next_region().unwrap().unwrap();
        let mut wire = reader.wire();
        let _ = wire.us().unwrap();
        let error = require_consumed(&name, reader.payload().len(), wire.pos()).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Error reading region \"meta\": read length mismatch. Expected: 4; Actual: 2"
        );
    }

    #[test]
    fn unknown_region_is_skipped_by_name() {
        let mut bytes = region_bytes("future-region", &[9, 9, 9]);
        bytes.extend_from_slice(&region_bytes("meta", &EMPTY));
        let leaked: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        let mut reader = reader_for(leaked);
        assert_eq!(reader.next_region().unwrap().unwrap(), "future-region");
        assert_eq!(reader.payload(), &[9, 9, 9]);
        assert_eq!(reader.next_region().unwrap().unwrap(), "meta");
        assert!(reader.next_region().unwrap().is_none());
    }

    #[test]
    fn nested_chunk_depth_is_capped() {
        let mut scratch = SaveScratch::new();
        let mut out = Vec::new();
        let error = scratch
            .write_region(&mut out, "custom", |wire, scratch| {
                scratch.write_chunk(wire, |_chunk| {
                    // A third level must fail (upstream has only two buffers).
                    Err(IoError::ChunkNesting)
                })
            })
            .unwrap_err();
        assert!(matches!(error, IoError::RegionWrite { .. }));

        // Direct misuse: chunk outside a region.
        let mut scratch = SaveScratch::new();
        let mut buf = Vec::new();
        let mut wire = WireWriter::new(&mut buf);
        assert!(matches!(
            scratch.write_chunk(&mut wire, |_| Ok(())),
            Err(IoError::ChunkNesting)
        ));
    }

    #[test]
    fn oversized_region_is_rejected() {
        let mut bytes = Vec::new();
        bytes.push(4u8);
        bytes.extend_from_slice(b"meta");
        bytes.extend_from_slice(&((MAX_REGION_BYTES as u32 + 1).to_be_bytes()));
        let leaked: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        let mut reader = reader_for(leaked);
        assert!(matches!(
            reader.next_region(),
            Err(IoError::RegionTooLarge { .. })
        ));
    }

    #[test]
    fn fallback_table_lookup() {
        assert_eq!(map_fallback("cryofluidmixer"), "cryofluid-mixer");
        assert_eq!(map_fallback("water"), "shallow-water");
        assert_eq!(map_fallback("copper-wall"), "copper-wall");
        // Table stays append-only and sorted-ish; verify no duplicate keys.
        let mut seen = std::collections::BTreeSet::new();
        for (from, _) in SAVE_FALLBACK {
            assert!(seen.insert(*from), "duplicate fallback key {from}");
        }
    }
}
