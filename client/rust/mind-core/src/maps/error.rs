// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Map registry errors (plan 06 §3.1).
//!
//! Ported from `core/src/mindustry/maps/MapException.java` plus the IO/name
//! validation paths in `Maps.java` (`loadMap`, `saveMap`, `importMap`).

use std::path::PathBuf;

/// `mindustry.maps.MapException`: carries the offending map file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapException {
    /// The file that failed to load/save.
    pub file: PathBuf,
    /// Human-readable message (upstream `RuntimeException` text).
    pub message: String,
}

impl std::fmt::Display for MapException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for MapException {}

/// Errors raised by the map registry.
#[derive(thiserror::Error, Debug)]
pub enum MapError {
    /// `Maps.loadMap`: a map must carry a non-empty `name` tag.
    #[error("Map name cannot be empty! File: {0}")]
    EmptyName(PathBuf),
    /// `FileMapGenerator`: a loaded map must contain a core.
    #[error("All maps must have a core.")]
    NoCore,
    /// `Maps.byName`: no map with that display name.
    #[error("map `{0}` does not exist")]
    NotFound(String),
    /// `MapException` carry-through.
    #[error(transparent)]
    Exception(#[from] MapException),
    /// Filesystem/map-format error from plan 04.
    #[error(transparent)]
    Io(#[from] crate::io::IoError),
}

impl MapError {
    /// Builds a [`MapException`] for a file with a message.
    pub fn exception(file: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::Exception(MapException {
            file: file.into(),
            message: message.into(),
        })
    }
}
