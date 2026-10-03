// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Per-floor environment attributes (`world/blocks/Attributes.java`, plan 06 §3.7).
//!
//! The `Attribute` id registry itself lives in plan 02 (`content::Attribute`);
//! this is the float-array value container. Deviation: plan 02 currently defines
//! four attributes (`heat`, `spores`, `water`, `light`), so the count is fixed
//! here and documented; mod-added attributes append in `Attribute`.

use serde::{Deserialize, Serialize};

use crate::content::Attribute;

/// Number of attribute slots (plan 02 `Attribute` variants).
pub const ATTRIBUTE_COUNT: usize = 5;

/// Attribute-name order used for JSON serialization (plan 06 §3.7).
pub const ATTRIBUTE_NAMES: [&str; ATTRIBUTE_COUNT] = ["heat", "spores", "water", "light", "steam"];

/// Index of an [`Attribute`] in the value array.
pub const fn attribute_index(attribute: Attribute) -> usize {
    match attribute {
        Attribute::Heat => 0,
        Attribute::Spores => 1,
        Attribute::Water => 2,
        Attribute::Light => 3,
        Attribute::Steam => 4,
    }
}

/// Attribute from its JSON name.
pub fn attribute_from_name(name: &str) -> Option<Attribute> {
    match name {
        "heat" => Some(Attribute::Heat),
        "spores" => Some(Attribute::Spores),
        "water" => Some(Attribute::Water),
        "light" => Some(Attribute::Light),
        "steam" => Some(Attribute::Steam),
        _ => None,
    }
}

/// A float attribute vector (`Attributes`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Attributes {
    values: [f32; ATTRIBUTE_COUNT],
}

impl Attributes {
    /// All-zero attributes.
    pub const fn new() -> Self {
        Self {
            values: [0.0; ATTRIBUTE_COUNT],
        }
    }

    /// Clears every attribute to `0`.
    pub fn clear(&mut self) {
        self.values = [0.0; ATTRIBUTE_COUNT];
    }

    /// Reads one attribute.
    pub fn get(&self, attribute: Attribute) -> f32 {
        self.values[attribute_index(attribute)]
    }

    /// Sets one attribute.
    pub fn set(&mut self, attribute: Attribute, value: f32) {
        self.values[attribute_index(attribute)] = value;
    }

    /// Raw values.
    pub fn values(&self) -> &[f32; ATTRIBUTE_COUNT] {
        &self.values
    }

    /// Adds another attribute vector element-wise (`Attributes.add`).
    pub fn add(&mut self, other: &Attributes) {
        for (dst, src) in self.values.iter_mut().zip(other.values.iter()) {
            *dst += *src;
        }
    }

    /// Adds another vector scaled by `scale` (`Attributes.add(other, scale)`).
    pub fn add_scaled(&mut self, other: &Attributes, scale: f32) {
        for (dst, src) in self.values.iter_mut().zip(other.values.iter()) {
            *dst += *src * scale;
        }
    }

    /// Whether every value is zero.
    pub fn is_zero(&self) -> bool {
        self.values.iter().all(|value| *value == 0.0)
    }
}

impl Serialize for Attributes {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        for (index, name) in ATTRIBUTE_NAMES.iter().enumerate() {
            if self.values[index] != 0.0 {
                map.serialize_entry(name, &self.values[index])?;
            }
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for Attributes {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use std::collections::BTreeMap;
        let map: BTreeMap<String, f32> = BTreeMap::deserialize(deserializer)?;
        let mut attributes = Attributes::new();
        for (name, value) in map {
            if let Some(attribute) = attribute_from_name(&name) {
                attributes.set(attribute, value);
            }
        }
        Ok(attributes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_scaled() {
        let mut a = Attributes::new();
        a.set(Attribute::Water, 1.0);
        let mut b = Attributes::new();
        b.set(Attribute::Water, 2.0);
        b.set(Attribute::Heat, 4.0);
        a.add(&b);
        assert_eq!(a.get(Attribute::Water), 3.0);
        assert_eq!(a.get(Attribute::Heat), 4.0);
        a.add_scaled(&b, 0.5);
        assert_eq!(a.get(Attribute::Water), 4.0);
        assert_eq!(a.get(Attribute::Heat), 6.0);
    }

    #[test]
    fn steam_roundtrips_and_keeps_legacy_indices() {
        // Append-only: `Heat..Light` keep their original indices.
        assert_eq!(attribute_index(Attribute::Heat), 0);
        assert_eq!(attribute_index(Attribute::Light), 3);
        assert_eq!(attribute_index(Attribute::Steam), 4);
        assert_eq!(Attribute::Steam.name(), "steam");
        let mut a = Attributes::new();
        a.set(Attribute::Steam, 1.0);
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"steam\":1.0"));
        let decoded: Attributes = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.get(Attribute::Steam), 1.0);
    }

    #[test]
    fn json_roundtrip_names() {
        let mut a = Attributes::new();
        a.set(Attribute::Heat, 0.5);
        a.set(Attribute::Light, 2.0);
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"heat\":0.5"));
        assert!(json.contains("\"light\":2.0"));
        assert!(!json.contains("water"));
        let decoded: Attributes = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, a);
    }
}
