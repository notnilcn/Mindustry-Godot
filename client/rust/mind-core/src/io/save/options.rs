// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Save writer options (plan 04 §3.2).
//!
//! Ported from `core/src/mindustry/io/SaveOptions.java`.

use super::super::StringMap;

/// Options controlling one save/map write.
#[derive(Debug, Clone, Default)]
pub struct SaveOptions {
    /// Embed external assets into the `patches` region (map export);
    /// plan 20 owns the asset payloads.
    pub embed_assets: bool,
    /// Extra meta tags merged under the standard keys (map tags when writing
    /// maps — `MapIO.writeMap`).
    pub extra_tags: Option<StringMap>,
}

impl SaveOptions {
    /// Default options (no assets, no extra tags).
    pub fn new() -> Self {
        Self::default()
    }
}
