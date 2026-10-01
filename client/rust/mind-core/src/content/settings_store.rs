// SPDX-License-Identifier: GPL-3.0-only

//! Unlock-state persistence interface (`Core.settings` subset).
//!
//! Ported from the settings calls in `ctype/UnlockableContent.java`
//! (`getBool(name + "-unlocked", false)` / `put`). The file-backed store is
//! plan 04; the headless harness and tests use [`MemoryUnlockStore`].

use indexmap::IndexMap;

/// Minimal settings store used for unlock persistence keys (plan 04 implements
/// the persistent version; plan 02 only needs booleans and `i32`s).
pub trait UnlockStore {
    /// Boolean value for `key` (missing = false).
    fn get_bool(&self, key: &str) -> bool;

    /// Stores a boolean value.
    fn set_bool(&mut self, key: &str, value: bool);

    /// Integer value for `key` (missing = 0).
    fn get_i32(&self, key: &str) -> i32;

    /// Stores an integer value.
    fn set_i32(&mut self, key: &str, value: i32);
}

/// In-memory unlock store (insertion-ordered; deterministic iteration).
#[derive(Debug, Clone, Default)]
pub struct MemoryUnlockStore {
    bools: IndexMap<String, bool>,
    ints: IndexMap<String, i32>,
}

impl MemoryUnlockStore {
    /// Empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of stored keys.
    pub fn len(&self) -> usize {
        self.bools.len() + self.ints.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.bools.is_empty() && self.ints.is_empty()
    }
}

impl UnlockStore for MemoryUnlockStore {
    fn get_bool(&self, key: &str) -> bool {
        self.bools.get(key).copied().unwrap_or(false)
    }

    fn set_bool(&mut self, key: &str, value: bool) {
        self.bools.insert(key.to_owned(), value);
    }

    fn get_i32(&self, key: &str) -> i32 {
        self.ints.get(key).copied().unwrap_or(0)
    }

    fn set_i32(&mut self, key: &str, value: i32) {
        self.ints.insert(key.to_owned(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_roundtrip() {
        let mut store = MemoryUnlockStore::new();
        assert!(!store.get_bool("x-unlocked"));
        assert_eq!(store.get_i32("y"), 0);
        store.set_bool("x-unlocked", true);
        store.set_i32("y", 7);
        assert!(store.get_bool("x-unlocked"));
        assert_eq!(store.get_i32("y"), 7);
    }
}
