// SPDX-License-Identifier: GPL-3.0-only

//! Bundle read interface used to localize content at construction.
//!
//! Ported from `UnlockableContent`'s `Core.bundle.get/getOrNull` calls. The real
//! bundle loader (`.properties` merge/fallback chain) is plan 03; this crate only
//! needs the read view so `create_base_content` captures localized names.

use indexmap::IndexMap;

/// Read-only bundle view (plan 03 implements the file-backed version).
pub trait BundleView {
    /// Raw bundle value for `key`, if present.
    fn get(&self, key: &str) -> Option<&str>;

    /// Value for `key`, falling back to `default` (`Core.bundle.get`).
    fn get_or(&self, key: &str, default: &str) -> String {
        self.get(key)
            .map(str::to_owned)
            .unwrap_or_else(|| default.to_owned())
    }
}

/// In-memory bundle for tests and the headless harness (empty by default, so
/// localized names fall back to internal names exactly like a missing key).
#[derive(Debug, Clone, Default)]
pub struct MemoryBundle {
    entries: IndexMap<String, String>,
}

impl MemoryBundle {
    /// Empty bundle.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts/replaces a key.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.entries.insert(key.into(), value.into());
    }

    /// Builds a bundle from key/value pairs.
    pub fn with_pairs<K, V>(pairs: impl IntoIterator<Item = (K, V)>) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        let mut bundle = Self::new();
        for (key, value) in pairs {
            bundle.insert(key, value);
        }
        bundle
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the bundle is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl BundleView for MemoryBundle {
    fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_or_falls_back() {
        let mut bundle = MemoryBundle::new();
        bundle.insert("item.copper.name", "Copper");
        assert_eq!(bundle.get("item.copper.name"), Some("Copper"));
        assert_eq!(bundle.get("item.lead.name"), None);
        assert_eq!(bundle.get_or("item.lead.name", "lead"), "lead");
    }
}
