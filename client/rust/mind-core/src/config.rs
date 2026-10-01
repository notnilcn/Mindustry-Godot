// SPDX-License-Identifier: GPL-3.0-only

//! Minimal P0 configuration.
//!
//! Ported from `core/src/mindustry/Vars.java` (tilesize/maxBlockSize and the
//! data-directory layout). The full `Settings` port lands in plan 04.

use std::path::PathBuf;

use crate::version::APP_NAME;

/// Size of one tile in world units (pixels).
/// Ported from `core/src/mindustry/Vars.java` `tilesize`.
pub const TILESIZE: i32 = 8;

/// Maximum edge length of a multiblock.
/// Ported from `core/src/mindustry/Vars.java` `maxBlockSize`.
pub const MAX_BLOCK_SIZE: i32 = 16;

/// Environment override for the data directory (`Vars` parity, OD-R9).
pub const DATA_DIR_ENV: &str = "MINDUSTRY_GODOT_DATA_DIR";

/// Minimal configuration carried by P0 hosts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MindConfig {
    /// Directory that holds `last_log.txt` and (later) saves/maps/schemes/mods.
    pub data_dir: PathBuf,
}

impl Default for MindConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl MindConfig {
    /// Builds the config with the platform default data directory.
    pub fn new() -> Self {
        Self {
            data_dir: default_data_dir(),
        }
    }

    /// Builds the config with an explicit data directory.
    pub fn with_data_dir(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    /// The default log file path: `<data-dir>/last_log.txt`.
    pub fn log_file(&self) -> PathBuf {
        self.data_dir.join("last_log.txt")
    }
}

/// Resolves the default data directory.
///
/// Order: `MINDUSTRY_GODOT_DATA_DIR`, then `$XDG_DATA_HOME/Mindustry-Godot`,
/// then `$HOME/.local/share/Mindustry-Godot` (Linux/WSL), then the current
/// directory as a last resort.
pub fn default_data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os(DATA_DIR_ENV) {
        return PathBuf::from(dir);
    }
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join(APP_NAME);
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join(APP_NAME);
    }
    PathBuf::from(".")
}
