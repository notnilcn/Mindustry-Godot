// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Legacy upstream `MSAV` import boundary (OD2 / NUD-02, plan 04 §2.3.1).
//!
//! Upstream maps/saves use the raw `MSAV` header (`"MSAV"` + big-endian `i32`
//! format version) with the whole stream deflated, followed by the named-region
//! stream that [`crate::io::save::versions::v1`] reads. The default build reads
//! and writes the native `MGRS` chain only; with the `msav-import` feature the
//! client opts in to the one-way reader implemented here. Modified-UTF-8
//! decoding and content fallback renames are handled by the shared wire/region
//! readers (plan 04 M7 records the decision).
//!
//! Ported from `core/src/mindustry/io/SaveIO.java` (header sniffing) and
//! `core/src/mindustry/io/versions/` (reader surface).

use std::io::{Cursor, Read};

use super::IoError;
use super::save::SaveReader;

/// Whether the stream inflates to the legacy upstream `MSAV` header.
pub(crate) fn sniffs_as_legacy(bytes: &[u8]) -> bool {
    let mut decoder = flate2::read::ZlibDecoder::new(Cursor::new(bytes));
    let mut header = [0u8; 4];
    matches!(decoder.read_exact(&mut header), Ok(())) && header == *b"MSAV"
}

/// Handles a stream whose magic is the legacy upstream `MSAV`.
///
/// Without the `msav-import` feature the importer is declined: this reads the
/// legacy format version and fails with the upstream unknown-version message,
/// exactly as a newer-version save would. With the feature,
/// [`load_legacy`] handles the stream before this is reached.
pub(crate) fn open_legacy(bytes: &[u8]) -> Result<(u32, SaveReader<'_>), IoError> {
    Err(read_legacy_version(bytes))
}

/// Imports a legacy `MSAV` stream into `state` (whole-stream deflate, 8-byte
/// header, then the upstream region walk).
#[cfg(feature = "msav-import")]
pub(crate) fn load_legacy(
    bytes: &[u8],
    state: &mut super::save::SaveReadState<'_>,
) -> Result<(), IoError> {
    let (version, body) = inflate_legacy(bytes)?;
    let result = super::save::versions::v1::read_legacy_regions(&body, version, state);
    // Upstream `finally { content.setTemporaryMapper(null) }`.
    if let Some(content) = state.content.as_deref_mut() {
        content.set_temporary_mapper(None);
    }
    result
}

/// Meta-only import of a legacy `MSAV` stream (`SaveIO.getMeta` /
/// `MapIO.createMap` on `MSAV`): inflate, parse the 8-byte header, then read
/// the first (`meta`) region.
#[cfg(feature = "msav-import")]
pub(crate) fn load_legacy_meta(bytes: &[u8]) -> Result<super::save::SaveMeta, IoError> {
    let (version, body) = inflate_legacy(bytes)?;
    super::save::versions::v1::read_legacy_meta(&body, version)
}

/// Inflates the whole stream and parses the `MSAV` header, returning
/// `(format_version, region_stream)`.
#[cfg(feature = "msav-import")]
fn inflate_legacy(bytes: &[u8]) -> Result<(u32, Vec<u8>), IoError> {
    let mut decoder = flate2::read::ZlibDecoder::new(Cursor::new(bytes));
    let mut inflated = Vec::new();
    decoder
        .read_to_end(&mut inflated)
        .map_err(|_| IoError::corrupt("not a valid legacy MSAV stream"))?;
    if inflated.len() < 8 || inflated[..4] != *b"MSAV" {
        return Err(IoError::corrupt("not a valid legacy MSAV stream"));
    }
    let version = u32::from_be_bytes([inflated[4], inflated[5], inflated[6], inflated[7]]);
    inflated.drain(..8);
    Ok((version, inflated))
}

/// Reads the format version of a legacy `MSAV` stream for the error message.
fn read_legacy_version(bytes: &[u8]) -> IoError {
    let mut decoder = flate2::read::ZlibDecoder::new(Cursor::new(bytes));
    let mut header = [0u8; 8];
    match decoder.read_exact(&mut header) {
        Ok(()) => {
            let version = u32::from_be_bytes([header[4], header[5], header[6], header[7]]);
            IoError::UnknownVersion(version)
        }
        Err(_) => IoError::corrupt("not a valid legacy MSAV stream"),
    }
}
