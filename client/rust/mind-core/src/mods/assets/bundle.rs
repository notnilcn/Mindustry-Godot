// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DataBundleLoader` applier (plan 20 §3.7): locale-chain bundle merge with an
//! original-value snapshot so `unload` restores the pre-mod bundle exactly.
//!
//! Godot-free: operates on ordered key/value maps. The client decorator
//! (`mind-gdext`) maps a mod bundle file to its `Bundle` chain node and calls
//! [`BundleApplier::merge`]; headless leaves the merged maps recorded.

use indexmap::IndexMap;

/// One bundle merge target (a locale layer of the runtime `Bundle`).
pub type BundleMap = IndexMap<String, String>;

/// `DataBundleLoader`: merge + snapshot/restore of bundle layers.
#[derive(Debug, Default)]
pub struct BundleApplier {
    /// file/bundle name → pre-merge map (recorded once per file).
    originals: IndexMap<String, BundleMap>,
    /// file/bundle name → keys contributed by the mod (restore audit).
    applied: IndexMap<String, Vec<String>>,
}

impl BundleApplier {
    /// Empty applier.
    pub fn new() -> Self {
        Self::default()
    }

    /// Merges `incoming` into `target`, snapshotting `target` the first time
    /// this file is seen (upstream `DataBundleLoader.load` keeps the original
    /// bundle node). Returns the number of keys that changed value/inserted.
    pub fn merge(&mut self, file: &str, target: &mut BundleMap, incoming: &BundleMap) -> usize {
        self.originals
            .entry(file.to_owned())
            .or_insert_with(|| target.clone());
        let mut changed = 0;
        let keys = self.applied.entry(file.to_owned()).or_default();
        for (key, value) in incoming {
            if target.get(key) != Some(value) {
                changed += 1;
            }
            target.insert(key.clone(), value.clone());
            if !keys.contains(key) {
                keys.push(key.clone());
            }
        }
        changed
    }

    /// Restores `target` from the recorded snapshot (upstream
    /// `DataBundleLoader.unload`). Returns `false` when no snapshot exists.
    pub fn restore(&self, file: &str, target: &mut BundleMap) -> bool {
        match self.originals.get(file) {
            Some(original) => {
                *target = original.clone();
                true
            }
            None => false,
        }
    }

    /// Whether a snapshot exists for `file`.
    pub fn has_snapshot(&self, file: &str) -> bool {
        self.originals.contains_key(file)
    }

    /// Keys contributed for a file (restore audit).
    pub fn applied_keys(&self, file: &str) -> &[String] {
        self.applied.get(file).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Snapshot file names in insert order.
    pub fn files(&self) -> impl Iterator<Item = &str> {
        self.originals.keys().map(String::as_str)
    }

    /// Drops every snapshot/record.
    pub fn clear(&mut self) {
        self.originals.clear();
        self.applied.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> BundleMap {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    /// Plan 20 M4: `assets::bundle_merge_restore` — merge overrides, restore
    /// reproduces the exact pre-mod map.
    #[test]
    fn merge_and_restore() {
        let mut applier = BundleApplier::new();
        let mut runtime = map(&[("a", "base-a"), ("b", "base-b")]);
        let incoming = map(&[("b", "mod-b"), ("c", "mod-c")]);

        let changed = applier.merge("bundles/bundle.properties", &mut runtime, &incoming);
        assert_eq!(changed, 2, "b replaced, c inserted");
        assert_eq!(runtime.get("b").map(String::as_str), Some("mod-b"));
        assert_eq!(runtime.get("c").map(String::as_str), Some("mod-c"));
        assert!(applier.has_snapshot("bundles/bundle.properties"));
        assert_eq!(
            applier.applied_keys("bundles/bundle.properties"),
            ["b".to_owned(), "c".to_owned()]
        );

        assert!(applier.restore("bundles/bundle.properties", &mut runtime));
        assert_eq!(runtime, map(&[("a", "base-a"), ("b", "base-b")]));
    }

    /// A second merge of the same file does not overwrite the first snapshot.
    #[test]
    fn snapshot_recorded_once() {
        let mut applier = BundleApplier::new();
        let mut runtime = map(&[("k", "orig")]);
        applier.merge("bundle", &mut runtime, &map(&[("k", "one")]));
        applier.merge("bundle", &mut runtime, &map(&[("k", "two")]));
        assert!(applier.restore("bundle", &mut runtime));
        assert_eq!(runtime.get("k").map(String::as_str), Some("orig"));
    }

    #[test]
    fn restore_without_snapshot_is_false() {
        let applier = BundleApplier::new();
        let mut runtime = map(&[]);
        assert!(!applier.restore("missing", &mut runtime));
    }
}
