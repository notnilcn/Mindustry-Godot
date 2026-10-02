// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Dense integer set (`arc/src/arc/struct/IntSet.java`) used by the plan-09
//! power-graph BFS closed set and the heat recursion guard.
//!
//! Deviation §2.3.3: Java's per-graph static scratch object becomes a reusable
//! [`IdSet`] carried by `PowerScratch`/`HeatScratch`. Membership is a bitset
//! keyed by `u32` (building `SimId`/entity index) so `add` returns whether the
//! value was newly inserted, exactly like `IntSet.add`.

/// A growable bitset over `u32` ids.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IdSet {
    words: Vec<u64>,
    len: usize,
}

impl IdSet {
    /// Creates an empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Removes every element, retaining capacity (`IntSet.clear`).
    pub fn clear(&mut self) {
        self.words.fill(0);
        self.len = 0;
    }

    /// Number of elements.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Whether `id` is present.
    pub fn contains(&self, id: u32) -> bool {
        let word = (id / 64) as usize;
        match self.words.get(word) {
            Some(bits) => bits & (1u64 << (id % 64)) != 0,
            None => false,
        }
    }

    /// Adds `id`; returns `true` when it was not already present
    /// (`IntSet.add`).
    pub fn add(&mut self, id: u32) -> bool {
        let word = (id / 64) as usize;
        if word >= self.words.len() {
            self.words.resize(word + 1, 0);
        }
        let bit = 1u64 << (id % 64);
        if self.words[word] & bit != 0 {
            return false;
        }
        self.words[word] |= bit;
        self.len += 1;
        true
    }

    /// Removes `id`; returns whether it was present.
    pub fn remove(&mut self, id: u32) -> bool {
        let word = (id / 64) as usize;
        let Some(bits) = self.words.get_mut(word) else {
            return false;
        };
        let bit = 1u64 << (id % 64);
        if *bits & bit == 0 {
            return false;
        }
        *bits &= !bit;
        self.len -= 1;
        true
    }

    /// Unions `other` into this set (`IntSet.addAll`).
    pub fn union_with(&mut self, other: &IdSet) {
        if other.words.len() > self.words.len() {
            self.words.resize(other.words.len(), 0);
        }
        self.len = 0;
        for (index, bits) in self.words.iter_mut().enumerate() {
            if let Some(other_bits) = other.words.get(index) {
                *bits |= other_bits;
            }
            self.len += bits.count_ones() as usize;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_returns_newly_inserted() {
        let mut set = IdSet::new();
        assert!(set.add(0));
        assert!(!set.add(0));
        assert!(set.add(70));
        assert!(set.add(128));
        assert_eq!(set.len(), 3);
        assert!(set.contains(70));
        assert!(!set.contains(71));
        assert!(set.remove(70));
        assert!(!set.remove(70));
        assert!(!set.contains(70));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn clear_resets_membership() {
        let mut set = IdSet::new();
        set.add(5);
        set.add(5_000);
        set.clear();
        assert!(set.is_empty());
        assert!(!set.contains(5));
        assert!(!set.contains(5_000));
    }
}
