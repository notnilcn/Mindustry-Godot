// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `EntityMapping` — class-id ↔ def-name parity with `classids.properties`.
//!
//! Ported from `annotations/src/main/resources/classids.properties` and
//! `annotations/.../entity/EntityProcess.java` (name registration). The committed
//! copy lives at `mind-core/assets/classids.properties`; ids are append-only.

use std::collections::BTreeMap;

/// One parsed `classids.properties` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassIdEntry {
    /// Full key as written upstream (`mindustry.entities.comp.BuildingComp`).
    pub key: String,
    /// Simple def name (`BuildingComp`).
    pub simple: String,
    /// Class id (save/network ABI).
    pub id: u16,
}

/// The class-id registry.
#[derive(Debug, Clone, Default)]
pub struct EntityMapping {
    by_name: BTreeMap<String, u16>,
    by_id: BTreeMap<u16, String>,
    entries: Vec<ClassIdEntry>,
}

/// The embedded upstream copy (GPL header + `classids.properties`).
pub const CLASSIDS_PROPERTIES: &str = include_str!("../../assets/classids.properties");

impl EntityMapping {
    /// Parses the embedded upstream copy.
    pub fn load_default() -> Self {
        Self::parse(CLASSIDS_PROPERTIES)
    }

    /// Parses a `key=id` properties file; comments/blank lines are ignored and
    /// duplicate ids keep the first (upstream) name.
    pub fn parse(text: &str) -> Self {
        let mut mapping = Self::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let Ok(id) = value.trim().parse::<u16>() else {
                continue;
            };
            let key = key.trim().to_owned();
            let simple = key.rsplit('.').next().unwrap_or(key.as_str()).to_owned();
            mapping.by_name.entry(simple.clone()).or_insert(id);
            mapping.by_id.entry(id).or_insert(simple.clone());
            mapping.entries.push(ClassIdEntry { key, simple, id });
        }
        mapping
    }

    /// Class id for a simple def name.
    pub fn by_name(&self, name: &str) -> Option<u16> {
        self.by_name.get(name).copied()
    }

    /// Simple def name for a class id.
    pub fn name_of(&self, id: u16) -> Option<&str> {
        self.by_id.get(&id).map(String::as_str)
    }

    /// Number of parsed rows.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no rows were parsed.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Parsed rows in file order.
    pub fn entries(&self) -> &[ClassIdEntry] {
        &self.entries
    }

    /// Simple-name → id map for [`super::meta::EntityRegistry::build_from_specs`].
    pub fn class_id_map(&self) -> BTreeMap<String, u16> {
        self.by_name
            .iter()
            .map(|(name, id)| (name.clone(), *id))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classids_parity() {
        let mapping = EntityMapping::load_default();
        assert!(mapping.len() >= 50, "expected the full upstream table");
        assert_eq!(mapping.by_name("BuildingComp"), Some(6));
        assert_eq!(mapping.by_name("mace"), Some(4));
        assert_eq!(mapping.by_name("alpha"), Some(0));
        assert_eq!(mapping.name_of(6), Some("BuildingComp"));
        assert_eq!(mapping.class_id_map().get("BulletComp"), Some(&7));
    }

    #[test]
    fn parse_ignores_comments_and_bad_lines() {
        let mapping = EntityMapping::parse("# c\n\nfoo=3\nbar=notanumber\nbaz=2\nfoo=9\n");
        // Three parseable rows; the first wins for a duplicate name.
        assert_eq!(mapping.len(), 3);
        assert_eq!(mapping.by_name("foo"), Some(3));
        assert_eq!(mapping.name_of(2), Some("baz"));
        assert_eq!(mapping.by_name("bar"), None);
    }
}
