// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Map headers (plan 04 §3.9).
//!
//! Ported from `mindustry.maps.Map` (the meta-only view used by map lists)
//! and `io/MapIO.createMap`. Pixel/preview state is collected separately
//! (`Map.teams`/`Map.spawns` fill during preview generation).

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::super::StringMap;
use super::super::save::SaveMeta;

/// Meta-only map descriptor (`Map` for map lists).
#[derive(Debug, Clone)]
pub struct MapHeader {
    /// Display name (`name` tag; maps always carry it).
    pub name: String,
    /// Width in tiles.
    pub width: i32,
    /// Height in tiles.
    pub height: i32,
    /// Save format version of the file.
    pub version: i32,
    /// Build that wrote the file.
    pub build: i32,
    /// All map tags.
    pub tags: StringMap,
    /// Teams with core buildings (filled by preview generation).
    pub teams: BTreeSet<u8>,
    /// Spawn-overlay count (filled by preview generation).
    pub spawns: u32,
    /// Whether this is a custom (user) map.
    pub custom: bool,
    /// Source file, when known.
    pub file: Option<PathBuf>,
}

impl MapHeader {
    /// Builds a header from save meta (`MapIO.createMap(file, custom)`).
    pub fn from_meta(meta: &SaveMeta, custom: bool, file: Option<PathBuf>) -> Self {
        Self {
            name: meta
                .tags
                .get("name")
                .cloned()
                .unwrap_or_else(|| meta.map_name.clone()),
            width: meta.width(),
            height: meta.height(),
            version: meta.version,
            build: meta.build,
            tags: meta.tags.clone(),
            teams: BTreeSet::new(),
            spawns: 0,
            custom,
            file,
        }
    }

    /// One tag value with default.
    pub fn tag(&self, key: &str) -> &str {
        self.tags.get(key).map(String::as_str).unwrap_or("")
    }

    /// `SaveMeta.isMap` parity: the tags carry a `name` key.
    pub fn is_map_header(&self) -> bool {
        self.tags.contains_key("name")
    }
}
