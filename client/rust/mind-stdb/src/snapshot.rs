// SPDX-License-Identifier: GPL-3.0-only

//! Snapshot header/chunk helpers (plan 21 §3.8/§6.4).
//!
//! Plan 21 M1 only needs snapshot **metadata** for the late-join decision; the
//! full dynamic-snapshot restore path (04's entity codec) lands in M4/M5. This
//! module owns the wire header and chunk reassembly so the metadata check and
//! chunk plumbing are testable without Godot.

/// Dynamic snapshot magic (`MGSN`, plan §6.4).
pub const SNAPSHOT_MAGIC: [u8; 4] = *b"MGSN";

/// Current dynamic snapshot format.
pub const SNAPSHOT_FORMAT: u32 = 1;

/// Snapshot chunk size in bytes (plan §3.8).
pub const SNAPSHOT_CHUNK_BYTES: usize = 16 * 1024;

/// Snapshot kind (mirrors the server enum names; append-only ABI).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotKind {
    /// `command_id = 0` world reset (rules/map identity).
    WorldReset,
    /// Full dynamic state blob.
    Dynamic,
    /// Checksum/digest only (no blob).
    Digest,
}

/// Decoded dynamic-snapshot header (plan §6.4).
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotHeader {
    /// Header format version.
    pub format: u32,
    /// Plan 05 checksum version.
    pub checksum_version: u32,
    /// Command watermark: all commands ≤ this are included.
    pub command_id: u64,
    /// Diagnostic sim tick.
    pub sim_tick: u64,
    /// Map content id.
    pub map_id: String,
    /// Map seed.
    pub map_seed: u64,
    /// Map generator hash.
    pub map_hash: u64,
    /// Client build id.
    pub build_id: String,
    /// Content manifest hash.
    pub content_hash: u64,
    /// `EntityIds.next_id` (plan 05).
    pub next_entity_id: i32,
    /// Rules JSON at snapshot time.
    pub rules_json: String,
    /// Wave number.
    pub wave: i32,
    /// Seconds until the next wave.
    pub wavetime: f32,
}

/// Snapshot header decode error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SnapshotError {
    /// Missing/wrong magic.
    #[error("not a dynamic snapshot (bad magic)")]
    BadMagic,
    /// Unsupported format version.
    #[error("unsupported snapshot format {found} (expected {SNAPSHOT_FORMAT})")]
    UnsupportedFormat {
        /// Version found in the blob.
        found: u32,
    },
    /// Header ended before all fields were read.
    #[error("truncated snapshot header")]
    Truncated,
    /// Length-prefixed string was not valid UTF-8.
    #[error("invalid UTF-8 in snapshot header")]
    InvalidString,
    /// Header map/build identity did not match the local match.
    #[error("snapshot identity mismatch: {field}")]
    IdentityMismatch {
        /// Mismatching field name.
        field: &'static str,
    },
}

fn take<'a>(bytes: &'a [u8], pos: &mut usize, len: usize) -> Result<&'a [u8], SnapshotError> {
    let end = pos.checked_add(len).ok_or(SnapshotError::Truncated)?;
    if end > bytes.len() {
        return Err(SnapshotError::Truncated);
    }
    let slice = &bytes[*pos..end];
    *pos = end;
    Ok(slice)
}

fn read_u32(bytes: &[u8], pos: &mut usize) -> Result<u32, SnapshotError> {
    Ok(u32::from_le_bytes(
        take(bytes, pos, 4)?.try_into().unwrap_or([0; 4]),
    ))
}

fn read_u64(bytes: &[u8], pos: &mut usize) -> Result<u64, SnapshotError> {
    Ok(u64::from_le_bytes(
        take(bytes, pos, 8)?.try_into().unwrap_or([0; 8]),
    ))
}

fn read_i32(bytes: &[u8], pos: &mut usize) -> Result<i32, SnapshotError> {
    Ok(i32::from_le_bytes(
        take(bytes, pos, 4)?.try_into().unwrap_or([0; 4]),
    ))
}

fn read_f32(bytes: &[u8], pos: &mut usize) -> Result<f32, SnapshotError> {
    Ok(f32::from_bits(read_u32(bytes, pos)?))
}

fn read_string(bytes: &[u8], pos: &mut usize) -> Result<String, SnapshotError> {
    let len = read_u32(bytes, pos)? as usize;
    let raw = take(bytes, pos, len)?;
    String::from_utf8(raw.to_vec()).map_err(|_| SnapshotError::InvalidString)
}

impl SnapshotHeader {
    /// Decodes the fixed header from the start of a dynamic blob.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SnapshotError> {
        if bytes.len() < 4 || bytes[..4] != SNAPSHOT_MAGIC {
            return Err(SnapshotError::BadMagic);
        }
        let mut pos = 4;
        let format = read_u32(bytes, &mut pos)?;
        if format != SNAPSHOT_FORMAT {
            return Err(SnapshotError::UnsupportedFormat { found: format });
        }
        Ok(Self {
            format,
            checksum_version: read_u32(bytes, &mut pos)?,
            command_id: read_u64(bytes, &mut pos)?,
            sim_tick: read_u64(bytes, &mut pos)?,
            map_id: read_string(bytes, &mut pos)?,
            map_seed: read_u64(bytes, &mut pos)?,
            map_hash: read_u64(bytes, &mut pos)?,
            build_id: read_string(bytes, &mut pos)?,
            content_hash: read_u64(bytes, &mut pos)?,
            next_entity_id: read_i32(bytes, &mut pos)?,
            rules_json: read_string(bytes, &mut pos)?,
            wave: read_i32(bytes, &mut pos)?,
            wavetime: read_f32(bytes, &mut pos)?,
        })
    }

    /// Encodes the header (symmetrical with [`SnapshotHeader::from_bytes`]).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&SNAPSHOT_MAGIC);
        out.extend_from_slice(&self.format.to_le_bytes());
        out.extend_from_slice(&self.checksum_version.to_le_bytes());
        out.extend_from_slice(&self.command_id.to_le_bytes());
        out.extend_from_slice(&self.sim_tick.to_le_bytes());
        put_string(&mut out, &self.map_id);
        out.extend_from_slice(&self.map_seed.to_le_bytes());
        out.extend_from_slice(&self.map_hash.to_le_bytes());
        put_string(&mut out, &self.build_id);
        out.extend_from_slice(&self.content_hash.to_le_bytes());
        out.extend_from_slice(&self.next_entity_id.to_le_bytes());
        put_string(&mut out, &self.rules_json);
        out.extend_from_slice(&self.wave.to_le_bytes());
        out.extend_from_slice(&self.wavetime.to_bits().to_le_bytes());
        out
    }

    /// Validates the header against the local match identity (plan §3.8).
    pub fn validate_identity(
        &self,
        map_id: &str,
        map_seed: u64,
        build_id: &str,
        content_hash: u64,
    ) -> Result<(), SnapshotError> {
        if self.map_id != map_id {
            return Err(SnapshotError::IdentityMismatch { field: "map_id" });
        }
        if self.map_seed != map_seed {
            return Err(SnapshotError::IdentityMismatch { field: "map_seed" });
        }
        if !build_id.is_empty() && !self.build_id.is_empty() && self.build_id != build_id {
            return Err(SnapshotError::IdentityMismatch { field: "build_id" });
        }
        if content_hash != 0 && self.content_hash != 0 && self.content_hash != content_hash {
            return Err(SnapshotError::IdentityMismatch {
                field: "content_hash",
            });
        }
        Ok(())
    }
}

fn put_string(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u32).to_le_bytes());
    out.extend_from_slice(value.as_bytes());
}

/// Number of chunks for a blob of `len` bytes.
pub fn chunk_count(len: usize) -> usize {
    if len == 0 {
        0
    } else {
        len.div_ceil(SNAPSHOT_CHUNK_BYTES)
    }
}

/// Splits a blob into `SNAPSHOT_CHUNK_BYTES` chunks (last chunk may be short).
pub fn split_chunks(blob: &[u8]) -> Vec<Vec<u8>> {
    blob.chunks(SNAPSHOT_CHUNK_BYTES)
        .map(<[u8]>::to_vec)
        .collect()
}

/// Reassembles `expected` chunks given as `(chunk_index, bytes)`; `None` when
/// the set is incomplete or contains duplicates/out-of-range indices.
pub fn reassemble(chunks: &[(u16, Vec<u8>)], expected: usize) -> Option<Vec<u8>> {
    if expected == 0 {
        return Some(Vec::new());
    }
    let mut ordered: Vec<Option<&[u8]>> = vec![None; expected];
    for (index, bytes) in chunks {
        let slot = ordered.get_mut(*index as usize)?;
        if slot.is_some() {
            return None;
        }
        *slot = Some(bytes);
    }
    let mut out = Vec::new();
    for slot in ordered {
        out.extend_from_slice(slot?);
    }
    Some(out)
}

/// Late-join snapshot download progress (plan §3.8; `MindNet.snapshot_progress`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SnapshotProgress {
    /// Chunks received so far.
    pub received: u16,
    /// Total chunks expected (0 before metadata is known).
    pub total: u16,
}

impl SnapshotProgress {
    /// Fraction complete in `0.0..=1.0` (1.0 when complete/empty).
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            1.0
        } else {
            (f32::from(self.received) / f32::from(self.total)).clamp(0.0, 1.0)
        }
    }

    /// Whether every chunk arrived.
    pub fn is_complete(&self) -> bool {
        self.total == 0 || self.received >= self.total
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    fn header() -> SnapshotHeader {
        SnapshotHeader {
            format: SNAPSHOT_FORMAT,
            checksum_version: 1,
            command_id: 1024,
            sim_tick: 2048,
            map_id: "demo_flat".to_string(),
            map_seed: 42,
            map_hash: 7,
            build_id: "build-1".to_string(),
            content_hash: 9,
            next_entity_id: 55,
            rules_json: "{\"waves\":true}".to_string(),
            wave: 3,
            wavetime: 12.5,
        }
    }

    #[test]
    fn header_roundtrip_and_identity_validation() {
        let header = header();
        let decoded = SnapshotHeader::from_bytes(&header.to_bytes()).expect("decode");
        assert_eq!(decoded, header);
        assert!(
            decoded
                .validate_identity("demo_flat", 42, "build-1", 9)
                .is_ok()
        );
        assert_eq!(
            decoded
                .validate_identity("other", 42, "build-1", 9)
                .unwrap_err(),
            SnapshotError::IdentityMismatch { field: "map_id" }
        );
    }

    #[test]
    fn header_validation_rejects_mismatch_and_bad_input() {
        assert_eq!(
            SnapshotHeader::from_bytes(b"nope").unwrap_err(),
            SnapshotError::BadMagic
        );
        let mut bytes = header().to_bytes();
        bytes[4] = 9;
        assert_eq!(
            SnapshotHeader::from_bytes(&bytes).unwrap_err(),
            SnapshotError::UnsupportedFormat { found: 9 }
        );
        let mut truncated = header().to_bytes();
        truncated.truncate(truncated.len() - 1);
        assert_eq!(
            SnapshotHeader::from_bytes(&truncated).unwrap_err(),
            SnapshotError::Truncated
        );
    }

    #[test]
    fn chunk_reassembly() {
        let blob: Vec<u8> = (0..(SNAPSHOT_CHUNK_BYTES * 2 + 7))
            .map(|i| i as u8)
            .collect();
        let chunks = split_chunks(&blob);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunk_count(blob.len()), 3);
        let indexed: Vec<(u16, Vec<u8>)> = chunks
            .iter()
            .enumerate()
            .map(|(i, c)| (i as u16, c.clone()))
            .collect();
        assert_eq!(reassemble(&indexed, chunks.len()), Some(blob));
        // Missing chunk -> None.
        let missing: Vec<(u16, Vec<u8>)> = indexed.iter().take(2).cloned().collect();
        assert_eq!(reassemble(&missing, chunks.len()), None);
    }

    #[test]
    fn progress_fraction() {
        assert_eq!(
            SnapshotProgress {
                received: 0,
                total: 0
            }
            .fraction(),
            1.0
        );
        assert_eq!(
            SnapshotProgress {
                received: 1,
                total: 4
            }
            .fraction(),
            0.25
        );
        assert!(
            SnapshotProgress {
                received: 4,
                total: 4
            }
            .is_complete()
        );
    }
}
