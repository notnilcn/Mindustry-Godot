// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Minimap attack indicators (plan 12 M7).
//!
//! Ported from `core/src/mindustry/game/AttackIndicators.java`. Upstream stores
//! a packed `(pos,time)` long; this port uses an explicit [`Indicator`] and
//! advances time in fixed ticks (`ATTACK_INDICATOR_LIFETIME_TICKS`) instead of
//! `Time.delta` (HLP §2.4). The insertion-ordered `IndexMap` keeps the
//! relocation fixup deterministic.

use indexmap::IndexMap;

use super::rules::ATTACK_INDICATOR_LIFETIME_TICKS;
use crate::io::typeio::pack_point2;

/// One active attack indicator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Indicator {
    /// Packed tile position (`Point2.pack`).
    pub pos: i32,
    /// Elapsed lifetime in ticks.
    pub time: f32,
}

/// Attack indicators for the minimap (`AttackIndicators`).
#[derive(Debug, Clone, Default)]
pub struct AttackIndicators {
    /// Indicators in insertion order.
    pub indicators: Vec<Indicator>,
    /// Packed position -> index in [`Self::indicators`].
    pub pos_to_index: IndexMap<i32, usize>,
}

impl AttackIndicators {
    /// Empty indicator set.
    pub fn new() -> Self {
        Self::default()
    }

    /// `AttackIndicators.add`: resets an existing position or appends a new one.
    pub fn add(&mut self, x: i32, y: i32) {
        let pos = pack_point2(x, y);
        if let Some(&index) = self.pos_to_index.get(&pos) {
            if let Some(indicator) = self.indicators.get_mut(index) {
                indicator.time = 0.0;
            }
        } else {
            self.indicators.push(Indicator { pos, time: 0.0 });
            self.pos_to_index.insert(pos, self.indicators.len() - 1);
        }
    }

    /// `AttackIndicators.update`: advances and compacts expired indicators.
    pub fn update(&mut self, delta_ticks: f32) {
        let mut i = 0;
        while i < self.indicators.len() {
            self.indicators[i].time += delta_ticks;
            if self.indicators[i].time >= ATTACK_INDICATOR_LIFETIME_TICKS as f32 {
                let removed = self.indicators.remove(i);
                self.pos_to_index.shift_remove(&removed.pos);
                // Relocate the head that shifted into this index.
                if let Some(next) = self.indicators.get(i) {
                    self.pos_to_index.insert(next.pos, i);
                }
            } else {
                i += 1;
            }
        }
    }

    /// `AttackIndicators.clear`.
    pub fn clear(&mut self) {
        self.indicators.clear();
        self.pos_to_index.clear();
    }

    /// `AttackIndicators.list`.
    pub fn list(&self) -> &[Indicator] {
        &self.indicators
    }

    /// Number of active indicators.
    pub fn len(&self) -> usize {
        self.indicators.len()
    }

    /// Whether no indicator is active.
    pub fn is_empty(&self) -> bool {
        self.indicators.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_resets_and_dedupes() {
        let mut indicators = AttackIndicators::new();
        indicators.add(3, 4);
        indicators.add(5, 6);
        assert_eq!(indicators.len(), 2);
        indicators.update(500.0);
        // Re-adding an existing position resets its timer, not a new entry.
        indicators.add(3, 4);
        assert_eq!(indicators.len(), 2);
        assert_eq!(indicators.indicators[0].time, 0.0);
    }

    #[test]
    fn update_expires_and_reindexes() {
        let mut indicators = AttackIndicators::new();
        indicators.add(1, 1);
        // Age the first by 450 ticks.
        indicators.update(450.0);
        assert_eq!(indicators.len(), 1);
        indicators.add(2, 2);
        indicators.add(3, 3);
        // Second update expires only the first (450 + 450 = 900).
        indicators.update(450.0);
        assert_eq!(indicators.len(), 2);
        assert_eq!(indicators.indicators[0].pos, pack_point2(2, 2));
        assert_eq!(indicators.pos_to_index.get(&pack_point2(2, 2)), Some(&0));
        // Upstream only relocates the head after a removal (the tail index goes
        // stale); the port keeps that exact behavior, so no assertion on 3,3.
        // A final update expires the remaining two.
        indicators.update(901.0);
        assert!(indicators.is_empty());
    }

    #[test]
    fn clear_empties() {
        let mut indicators = AttackIndicators::new();
        indicators.add(9, 9);
        indicators.clear();
        assert!(indicators.is_empty());
        assert!(indicators.pos_to_index.is_empty());
    }
}
