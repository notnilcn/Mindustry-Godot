// SPDX-License-Identifier: GPL-3.0-only

//! Error type for `mind-atlas`.

/// Errors raised by the packing library.
#[derive(thiserror::Error, Debug)]
pub enum AtlasError {
    /// PNG decode/encode failure.
    #[error("png: {0}")]
    Png(String),
    /// IO failure.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// JSON (de)serialization failure.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// Invalid input or invariant violation (ported `IllegalArgumentException`s).
    #[error("{0}")]
    Invalid(String),
}

impl From<png::DecodingError> for AtlasError {
    fn from(error: png::DecodingError) -> Self {
        AtlasError::Png(error.to_string())
    }
}

impl From<png::EncodingError> for AtlasError {
    fn from(error: png::EncodingError) -> Self {
        AtlasError::Png(error.to_string())
    }
}

/// Crate result alias.
pub type Result<T> = std::result::Result<T, AtlasError>;
