// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Entity ID mapping + duplicate handling (plan 04 §3.5).
//!
//! Ported from `SaveVersion.writeEntityMapping`/`readEntityMapping`
//! (`EntityMapping.customIdMap`: save-side custom class IDs → def names) and
//! `SaveVersion.readWorldEntities` duplicate-ID reassignment semantics.

use indexmap::IndexMap;

/// Save-side custom entity-ID mapping (`u16` class id → def name).
///
/// Written at the top of the `entities` region; on read, ids present in the
/// map resolve their def by name instead of by the global class-ID table
/// (mod save compat).
#[derive(Debug, Clone, Default)]
pub struct EntityIdMap {
    by_id: IndexMap<u16, String>,
}

impl EntityIdMap {
    /// Empty mapping.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a custom id for a def name.
    pub fn insert(&mut self, id: u16, name: &str) {
        self.by_id.insert(id, name.to_owned());
    }

    /// Def name for a custom id.
    pub fn name_of(&self, id: u16) -> Option<&str> {
        self.by_id.get(&id).map(String::as_str)
    }

    /// Custom id for a def name.
    pub fn id_of(&self, name: &str) -> Option<u16> {
        self.by_id
            .iter()
            .find_map(|(id, other)| (other == name).then_some(*id))
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Whether the map is empty.
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// Entries in insertion order (deterministic serialization).
    pub fn iter(&self) -> impl Iterator<Item = (u16, &str)> {
        self.by_id.iter().map(|(id, name)| (*id, name.as_str()))
    }
}

/// Duplicate entity-ID tracker (`SaveVersion.readWorldEntities`: `used` set +
/// `reassign` queue). First use of an id wins; duplicates are queued for
/// reassignment to `EntityGroup::nextId` (plan 05) with a warning.
#[derive(Debug, Clone, Default)]
pub struct DuplicateIdTracker {
    used: std::collections::BTreeSet<i32>,
    reassign: Vec<i32>,
}

impl DuplicateIdTracker {
    /// Empty tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records an id read from the save. Returns `true` when the id is fresh
    /// (entity is added normally), `false` for a duplicate (entity must be
    /// reassigned a new id before being added).
    pub fn claim(&mut self, id: i32) -> bool {
        if self.used.insert(id) {
            true
        } else {
            self.reassign.push(id);
            false
        }
    }

    /// Ids queued for reassignment, in read order.
    pub fn reassign_queue(&self) -> &[i32] {
        &self.reassign
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_map_roundtrip_lookup() {
        let mut map = EntityIdMap::new();
        map.insert(50, "mymod-turret");
        map.insert(51, "mymod-unit");
        assert_eq!(map.name_of(50), Some("mymod-turret"));
        assert_eq!(map.id_of("mymod-unit"), Some(51));
        assert_eq!(map.name_of(6), None);
        assert_eq!(map.len(), 2);
        let names: Vec<&str> = map.iter().map(|(_, name)| name).collect();
        assert_eq!(names, vec!["mymod-turret", "mymod-unit"]);
    }

    /// `io::entity::tests::duplicate_id_reassign` (plan 04 §7a).
    #[test]
    fn duplicate_id_reassign() {
        let mut tracker = DuplicateIdTracker::new();
        assert!(tracker.claim(7));
        assert!(tracker.claim(9));
        assert!(!tracker.claim(7));
        assert!(!tracker.claim(9));
        assert!(tracker.claim(10));
        assert_eq!(tracker.reassign_queue(), &[7, 9]);
    }
}
