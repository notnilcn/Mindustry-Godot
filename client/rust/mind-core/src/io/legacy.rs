// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Legacy upstream `MSAV` import boundary (OD2 / NUD-02, plan 04 §2.3.1).
//!
//! The default build reads/writes the native `MGRS` chain only; upstream files
//! fail with the standard unknown-version message. The one-way importer
//! (readers for upstream versions 1–13, modified-UTF-8 decoding, content
//! fallback renames) is **opt-in and currently declined**: the `msav-import`
//! Cargo feature is declared so the boundary stays testable, but enabling it
//! changes nothing until an explicit opt-in lands `legacy::java` readers
//! (plan 04 M7 records the decision).
//!
//! Ported from `core/src/mindustry/io/SaveIO.java` (header sniffing) and
//! `core/src/mindustry/io/versions/` (reader surface only).

use std::io::{Cursor, Read};

use super::IoError;
use super::save::SaveReader;

/// Handles a stream whose magic is the legacy upstream `MSAV`.
///
/// Without an implemented importer this reads the legacy format version (the
/// whole upstream stream is deflated, header included) and fails with the
/// upstream unknown-version message, exactly as a newer-version save would.
pub(crate) fn open_legacy(bytes: &[u8]) -> Result<(u32, SaveReader<'_>), IoError> {
    Err(read_legacy_version(bytes))
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
