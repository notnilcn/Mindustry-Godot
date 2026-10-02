// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! IO error type (plan 04 §3.1).
//!
//! Ported from `core/src/mindustry/io/SaveIO.java` and `SaveFileReader.java`
//! error semantics: upstream message text is kept where tools/tests match on it
//! (unknown version, region length mismatch, incorrect header).

/// Errors raised by save/map/settings IO. All untrusted-input paths return this
/// type; panics are reserved for debug invariants (plan 04 §3.12.2).
#[derive(thiserror::Error, Debug)]
pub enum IoError {
    /// Magic bytes did not match. Message mirrors upstream
    /// `SaveIO.readHeader` (`Incorrect header! Expecting: ...; Actual: ...`).
    #[error("Incorrect header! Expecting: {expected:?}; Actual: {actual:?}")]
    Header {
        /// Expected magic (native `MGRS`).
        expected: [u8; 4],
        /// Bytes found in the file.
        actual: [u8; 4],
    },

    /// The format version is not in the append-only chain. Message must match
    /// upstream exactly (`SaveIO.getMeta`), including for legacy `MSAV` files
    /// when the `msav-import` feature is disabled (OD2).
    #[error("Unknown save version: {0}. Are you trying to load a save from a newer version?")]
    UnknownVersion(u32),

    /// A region payload was not consumed exactly. Message mirrors upstream
    /// `SaveFileReader.readRegion`.
    #[error(
        "Error reading region \"{name}\": read length mismatch. Expected: {expected}; Actual: {actual}"
    )]
    RegionLengthMismatch {
        /// Region name.
        name: String,
        /// Declared payload length.
        expected: usize,
        /// Bytes actually consumed by the handler.
        actual: usize,
    },

    /// A region handler failed. Message mirrors upstream
    /// (`Error reading region "name".`).
    #[error("Error reading region \"{name}\".")]
    RegionRead {
        /// Region name.
        name: String,
        /// Underlying cause.
        #[source]
        source: Box<IoError>,
    },

    /// A region writer failed. Message mirrors upstream
    /// (`Error writing region "name".`).
    #[error("Error writing region \"{name}\".")]
    RegionWrite {
        /// Region name.
        name: String,
        /// Underlying cause.
        #[source]
        source: Box<IoError>,
    },

    /// The stream ended before a full primitive/region could be read.
    #[error("unexpected end of stream")]
    UnexpectedEof,

    /// The inflated stream exceeded the decompressed-size safety cap
    /// (zip-bomb guard, plan 04 §3.3).
    #[error("decompressed stream exceeds the {limit} byte safety cap")]
    DecompressedTooLarge {
        /// Configured cap.
        limit: u64,
    },

    /// One region payload exceeded `MAX_REGION_BYTES`.
    #[error("region payload exceeds the {limit} byte cap")]
    RegionTooLarge {
        /// Configured cap (`MAX_REGION_BYTES`).
        limit: usize,
    },

    /// Chunks nest at most two deep (region → chunk), mirroring upstream's
    /// one-level-nested static chunk buffers (`SaveFileReader.byteOutput2`).
    #[error("chunk nesting deeper than 2 levels is not supported")]
    ChunkNesting,

    /// String-map entry count was negative or exceeded the wire type.
    #[error("invalid string map size: {0}")]
    InvalidStringMapSize(i32),

    /// A length-prefixed string/array exceeded its cap.
    #[error("value exceeds the {limit} byte cap (got {actual})")]
    TooLarge {
        /// Configured cap.
        limit: usize,
        /// Actual size found.
        actual: usize,
    },

    /// UTF-8 validation failed on a wire string.
    #[error("invalid UTF-8 in wire string: {0}")]
    Utf8(#[from] std::str::Utf8Error),

    /// Structurally invalid save/map data (untrusted input).
    #[error("invalid save data: {0}")]
    Corrupt(String),

    /// JSON (de)serialization failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Filesystem/stream error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Unknown `TypeIO` object tag (plan 04 §3.12.4).
    #[error("Unknown object type: {0}")]
    UnknownTypeTag(u8),

    /// Unknown entity revision for a def (plan 04 §3.12.4).
    #[error("unknown entity revision {revision} for `{name}`")]
    UnknownRevision {
        /// Entity def name.
        name: String,
        /// Revision found in the stream.
        revision: u16,
    },
}

impl IoError {
    /// Wraps `source` as a region-read failure (`Error reading region "name".`).
    pub fn region_read(name: &str, source: IoError) -> Self {
        Self::RegionRead {
            name: name.to_owned(),
            source: Box::new(source),
        }
    }

    /// Wraps `source` as a region-write failure (`Error writing region "name".`).
    pub fn region_write(name: &str, source: IoError) -> Self {
        Self::RegionWrite {
            name: name.to_owned(),
            source: Box::new(source),
        }
    }

    /// Builds a corrupt-data error from a message.
    pub fn corrupt(message: impl Into<String>) -> Self {
        Self::Corrupt(message.into())
    }
}
