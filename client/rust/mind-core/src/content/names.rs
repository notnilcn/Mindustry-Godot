// SPDX-License-Identifier: GPL-3.0-only

//! Content name maps and mod name prefixing.
//!
//! Ported from `core/src/mindustry/core/ContentLoader.java` (per-type and global
//! name maps, `transformName`, duplicate semantics) and the
//! `SaveFileReader.modContentNameMap` fallback table (plan 04 owns the reader;
//! the table contents are duplicated here per plan 02 §3.7).

use indexmap::IndexMap;

use super::{ContentRef, ContentType, ModId};

/// `ContentLoader.transformName`: prefixes mod content with `<modname>-`.
pub fn transform_name(current_mod: Option<&ModId>, name: &str) -> String {
    match current_mod {
        None => name.to_owned(),
        Some(mod_id) => format!("{}-{}", mod_id.name(), name),
    }
}

/// Legacy save fallbacks applied by `getByName(ContentType.block, ..)`
/// (`SaveFileReader.modContentNameMap`, plan 04 §6).
const MOD_CONTENT_NAME_MAP: &[(&str, &str)] = &[
    ("craters", "crater-stone"),
    ("deepwater", "deep-water"),
    ("water", "shallow-water"),
    ("slag", "molten-slag"),
];

/// Looks up a legacy block name fallback (linear scan; 4 entries).
pub fn mod_content_name_map(name: &str) -> Option<&'static str> {
    MOD_CONTENT_NAME_MAP
        .iter()
        .find(|(old, _)| *old == name)
        .map(|(_, new)| *new)
}

/// Per-type name maps plus the global `nameMap` (`ContentLoader`).
///
/// Iteration is insertion-ordered (`IndexMap`) — no `HashMap` iteration anywhere
/// in the registry (HIGH_LEVEL_PLAN §2.4).
#[derive(Debug, Clone)]
pub struct NameMaps {
    per_type: [IndexMap<String, u16>; ContentType::ALL.len()],
    global: IndexMap<String, ContentRef>,
}

impl Default for NameMaps {
    fn default() -> Self {
        Self::new()
    }
}

impl NameMaps {
    /// Empty maps for all 18 content types.
    pub fn new() -> Self {
        Self {
            per_type: std::array::from_fn(|_| IndexMap::new()),
            global: IndexMap::new(),
        }
    }

    /// Per-type map for `type_`.
    pub fn per_type(&self, type_: ContentType) -> &IndexMap<String, u16> {
        &self.per_type[type_.ordinal()]
    }

    /// Global name map (`byName`).
    pub fn global(&self) -> &IndexMap<String, ContentRef> {
        &self.global
    }

    /// Raw id for a name inside `type_`.
    pub fn get(&self, type_: ContentType, name: &str) -> Option<u16> {
        self.per_type[type_.ordinal()].get(name).copied()
    }

    /// Global reference for a name (last registration wins, like upstream).
    pub fn get_global(&self, name: &str) -> Option<ContentRef> {
        self.global.get(name).copied()
    }

    /// Registers a name/id pair in the per-type and global maps.
    pub fn insert(&mut self, type_: ContentType, name: &str, id: u16) {
        self.per_type[type_.ordinal()].insert(name.to_owned(), id);
        self.global
            .insert(name.to_owned(), ContentRef::new(type_, id));
    }

    /// Removes a per-type name; the global entry is only dropped when it points
    /// at the same type/id (`ContentLoader.remove`).
    pub fn remove(&mut self, type_: ContentType, name: &str, id: u16) {
        self.per_type[type_.ordinal()].shift_remove(name);
        if self.global.get(name) == Some(&ContentRef::new(type_, id)) {
            self.global.shift_remove(name);
        }
    }

    /// Drops all names of one type.
    pub fn clear_type(&mut self, type_: ContentType) {
        self.per_type[type_.ordinal()].clear();
    }

    /// Replaces the global map (used when ids shift after a `remove`).
    pub fn set_global(&mut self, global: IndexMap<String, ContentRef>) {
        self.global = global;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_name_prefixes_only_mods() {
        assert_eq!(transform_name(None, "copper"), "copper");
        let mod_id = ModId(String::from("mymod"));
        assert_eq!(transform_name(Some(&mod_id), "copper"), "mymod-copper");
    }

    /// `names::mod_content_name_map` — the four legacy block fallbacks.
    #[test]
    fn legacy_block_fallbacks() {
        assert_eq!(mod_content_name_map("craters"), Some("crater-stone"));
        assert_eq!(mod_content_name_map("deepwater"), Some("deep-water"));
        assert_eq!(mod_content_name_map("water"), Some("shallow-water"));
        assert_eq!(mod_content_name_map("slag"), Some("molten-slag"));
        assert_eq!(mod_content_name_map("copper"), None);
    }

    #[test]
    fn duplicate_names_per_type_are_tracked() {
        let mut maps = NameMaps::new();
        maps.insert(ContentType::Item, "copper", 0);
        maps.insert(ContentType::Block, "copper", 0);
        assert_eq!(maps.get(ContentType::Item, "copper"), Some(0));
        assert_eq!(maps.get(ContentType::Block, "copper"), Some(0));
        assert_eq!(
            maps.get_global("copper"),
            Some(ContentRef::new(ContentType::Block, 0))
        );
    }
}
