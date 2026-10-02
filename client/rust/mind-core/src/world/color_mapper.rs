// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Map-color to block lookup (`world/ColorMapper.java`, plan 06 §3.7).
//!
//! Rebuilt by plan 02's loader (`ContentLoader.loadColors`) and consumed by the
//! plan-19 image-map importer. Preserves the upstream `0,0,0,1` → `air` entry.

use indexmap::IndexMap;

use crate::content::{BlockId, ContentRegistry, Rgba};

/// Maps packed `rgba8888` colors to block ids (`ColorMapper`).
#[derive(Debug, Clone, Default)]
pub struct ColorMapper {
    map: IndexMap<u32, BlockId>,
}

impl ColorMapper {
    /// An empty mapper.
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuilds the table from every block with a map color (`load`).
    ///
    /// Iteration is content-id order (deterministic; no hash-order surprise),
    /// and the first registration of a color wins (upstream `putIfAbsent`).
    pub fn load(&mut self, content: &ContentRegistry) {
        self.map.clear();
        for def in content.blocks() {
            if let Some(color) = def.map_color {
                let key = rgba8888(&color);
                self.map.entry(key).or_insert(def.id);
            }
        }
        // Upstream seeds transparent black as `air`.
        self.map
            .entry(rgba8888(&Rgba::new(0.0, 0.0, 0.0, 1.0)))
            .or_insert(BlockId::AIR);
    }

    /// Block for a packed color, defaulting to `air` (`get`).
    pub fn get(&self, color: u32) -> BlockId {
        self.map.get(&color).copied().unwrap_or(BlockId::AIR)
    }

    /// Number of mapped colors.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether the mapper has no colors.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// Packs a float color into `rgba8888` (`Color.rgba8888`).
pub fn rgba8888(color: &Rgba) -> u32 {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    (channel(color.r) << 24) | (channel(color.g) << 16) | (channel(color.b) << 8) | channel(color.a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_packing_matches_arc_layout() {
        assert_eq!(rgba8888(&Rgba::new(1.0, 0.0, 0.0, 1.0)), 0xFF00_00FF);
        assert_eq!(rgba8888(&Rgba::new(0.0, 0.0, 0.0, 1.0)), 0x0000_00FF);
    }

    #[test]
    fn empty_mapper_defaults_to_air() {
        let mapper = ColorMapper::new();
        assert_eq!(mapper.get(0x1234_5678), BlockId::AIR);
    }
}
