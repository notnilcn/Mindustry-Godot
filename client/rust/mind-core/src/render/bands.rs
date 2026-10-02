// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Append-only band allocator (plan 16 §3.4 / D16-1/D16-2).
//!
//! Godot `CanvasItem` has no fractional z, so each `(Layer, sub, Blend)` key is
//! mapped to an ordered integer `z_index`. The table is generated once from an
//! ordered list and is **append-only**: new keys append, existing band numbers
//! never change. `bands.json` is dumped at startup for audit and golden diffing.

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::render::commands::Blend;
use crate::render::layer::{CacheLayerId, Layer};

/// A band key: a logical layer plus a sub-order (cache layer / building cache)
/// and blend mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BandKey {
    /// Logical layer.
    pub layer: Layer,
    /// Sub-order within the layer (`CacheLayer.id` for floor, `0` otherwise).
    pub sub: u8,
    /// Blend mode.
    pub blend: Blend,
}

impl BandKey {
    /// A plain key at the given layer.
    pub const fn new(layer: Layer) -> Self {
        Self {
            layer,
            sub: 0,
            blend: Blend::Normal,
        }
    }

    /// A floor key for a cache layer.
    pub const fn floor(cache: CacheLayerId) -> Self {
        Self {
            layer: Layer::Floor,
            sub: cache.id(),
            blend: Blend::Normal,
        }
    }

    /// The layer's base key (`sub = 0`).
    pub const fn base(layer: Layer) -> Self {
        Self::new(layer)
    }
}

/// One allocated band.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BandEntry {
    /// Logical layer.
    pub layer: Layer,
    /// Layer float value (`Layer.z`).
    pub z: f32,
    /// Sub-order.
    pub sub: u8,
    /// Blend mode.
    pub blend: Blend,
    /// Integer `z_index` band.
    pub band: i32,
}

/// The band table.
#[derive(Clone, Debug, Default)]
pub struct BandPlan {
    entries: Vec<BandEntry>,
    index: HashMap<BandKey, i32>,
}

impl BandPlan {
    /// Builds the fixed band table. Floor gets `CacheLayerId::ALL.len()`
    /// sub-bands; every other layer has a single (`sub = 0`) band.
    pub fn new() -> Self {
        let mut plan = Self::default();
        let mut band = 0i32;
        for layer in Layer::ALL {
            if layer == Layer::Floor {
                for cache in CacheLayerId::ALL {
                    plan.insert(
                        BandKey {
                            layer,
                            sub: cache.id(),
                            blend: Blend::Normal,
                        },
                        band,
                    );
                    band += 1;
                }
            } else {
                plan.insert(BandKey::new(layer), band);
                band += 1;
            }
        }
        plan
    }

    fn insert(&mut self, key: BandKey, band: i32) {
        self.entries.push(BandEntry {
            layer: key.layer,
            z: key.layer.z(),
            sub: key.sub,
            blend: key.blend,
            band,
        });
        self.index.insert(key, band);
    }

    /// Resolves the band for a key, falling back to the layer's base band.
    pub fn band(&self, key: BandKey) -> i32 {
        if let Some(band) = self.index.get(&key) {
            return *band;
        }
        self.index
            .get(&BandKey::base(key.layer))
            .copied()
            .unwrap_or(0)
    }

    /// Allocated entries in band order.
    pub fn entries(&self) -> &[BandEntry] {
        &self.entries
    }

    /// Serializes the band table as JSON (`build/render/bands.json`).
    pub fn to_json(&self) -> String {
        let mut out = String::from("[\n");
        for (i, entry) in self.entries.iter().enumerate() {
            let comma = if i + 1 == self.entries.len() { "" } else { "," };
            let _ = writeln!(
                out,
                "  {{\"layer\":\"{}\",\"value\":{},\"sub\":{},\"blend\":\"{:?}\",\"band\":{}}}{}",
                entry.layer.name(),
                format_f32(entry.z),
                entry.sub,
                entry.blend,
                entry.band,
                comma
            );
        }
        out.push_str("]\n");
        out
    }
}

/// Minimal fixed-precision float formatting for the audit JSON.
fn format_f32(value: f32) -> String {
    if value.fract() == 0.0 {
        format!("{:.1}", value)
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bands_strictly_increase_in_layer_order() {
        let plan = BandPlan::new();
        let mut last = -1i32;
        for entry in plan.entries() {
            assert!(entry.band > last, "band order at {:?}", entry.layer);
            last = entry.band;
        }
    }

    #[test]
    fn floor_subbrands_follow_frozen_order() {
        let plan = BandPlan::new();
        let water = plan.band(BandKey::floor(CacheLayerId::Water));
        let normal = plan.band(BandKey::floor(CacheLayerId::Normal));
        let walls = plan.band(BandKey::floor(CacheLayerId::Walls));
        assert!(water < normal && normal < walls);
        // Scorch follows the floor sub-bands.
        assert!(walls < plan.band(BandKey::new(Layer::Scorch)));
    }

    #[test]
    fn unknown_sub_falls_back_to_base() {
        let plan = BandPlan::new();
        let key = BandKey {
            layer: Layer::Block,
            sub: 200,
            blend: Blend::Additive,
        };
        assert_eq!(plan.band(key), plan.band(BandKey::new(Layer::Block)));
    }

    #[test]
    fn json_is_deterministic() {
        let a = BandPlan::new().to_json();
        let b = BandPlan::new().to_json();
        assert_eq!(a, b);
        assert!(a.starts_with("[\n"));
        assert!(a.contains("\"layer\":\"min\""));
    }
}
