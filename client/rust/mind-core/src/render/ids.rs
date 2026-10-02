// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Region-id interning over plan 03's `AtlasIndex` (plan 16 §3.11).
//!
//! `RegionId` is a dense `u32` assigned once at content load; the map is
//! append-only and never carries a string per frame. The name↔id table is
//! dumped as an audit artifact by the headless harness.

use std::collections::HashMap;

/// A dense atlas-region id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct RegionId(pub u32);

/// Append-only region-name interner.
#[derive(Debug, Default, Clone)]
pub struct RegionIdTable {
    names: Vec<String>,
    by_name: HashMap<String, u32>,
}

impl RegionIdTable {
    /// Empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Interns `name`, returning a stable id. Ids are assigned in first-call
    /// order and never reused.
    pub fn intern(&mut self, name: impl Into<String>) -> RegionId {
        let name = name.into();
        if let Some(id) = self.by_name.get(&name) {
            return RegionId(*id);
        }
        let id = self.names.len() as u32;
        self.names.push(name.clone());
        self.by_name.insert(name, id);
        RegionId(id)
    }

    /// Interns a borrowed name without allocating on a cache hit (the render
    /// build path uses this to avoid per-tile `String` clones; plan 16 §7.4).
    pub fn intern_str(&mut self, name: &str) -> RegionId {
        if let Some(id) = self.by_name.get(name) {
            return RegionId(*id);
        }
        self.intern(name)
    }

    /// Resolves an id back to its name.
    pub fn name(&self, id: RegionId) -> Option<&str> {
        self.names.get(id.0 as usize).map(String::as_str)
    }

    /// Looks up an existing id without interning.
    pub fn get(&self, name: &str) -> Option<RegionId> {
        self.by_name.get(name).map(|id| RegionId(*id))
    }

    /// Number of interned regions.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// All interned names in id order.
    pub fn names(&self) -> &[String] {
        &self.names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_ids_stable_and_dense() {
        let mut table = RegionIdTable::new();
        let a = table.intern("grass");
        let b = table.intern("copper-wall");
        let a2 = table.intern("grass");
        assert_eq!(a, RegionId(0));
        assert_eq!(b, RegionId(1));
        assert_eq!(a, a2);
        assert_eq!(table.len(), 2);
        assert_eq!(table.name(b), Some("copper-wall"));
        assert_eq!(table.get("grass"), Some(a));
        assert_eq!(table.get("missing"), None);
    }

    #[test]
    fn intern_str_matches_intern_without_alloc() {
        let mut table = RegionIdTable::new();
        let a = table.intern("grass");
        assert_eq!(table.intern_str("grass"), a);
        assert_eq!(table.intern_str("new"), RegionId(1));
        assert_eq!(table.len(), 2);
    }
}
