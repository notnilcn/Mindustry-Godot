// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `UnitStance` runtime bitset (plan 11 §2.4 deviation 4/§3.8).
//!
//! Upstream `CommandAI` stores active stances in an Arc `Bits` (backed by
//! `long[]`); vanilla has 30 stances, so the port uses a fixed `[u64; 1]` mask
//! ([`StanceBits`]) that preserves `set`/`get`/`clear`/`andNot` semantics without
//! allocation. Plan 20 may append a `SmallVec<u64>` variant for >64 mod stances.

use crate::content::ContentRegistry;
use crate::content::id::UnitStanceId;
use crate::content::registries::stances::UnitStanceDef;

/// Fixed 64-bit active-stance mask (`CommandAI.stances`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StanceBits(pub [u64; 1]);

impl StanceBits {
    /// An empty mask.
    pub const fn new() -> Self {
        Self([0])
    }

    /// Sets the bit for `stance` (`Bits.set`). Returns `false` when out of range.
    pub fn set(&mut self, stance: UnitStanceId) -> bool {
        let id = stance.raw() as usize;
        if id >= 64 {
            return false;
        }
        self.0[0] |= 1u64 << id;
        true
    }

    /// Clears the bit for `stance` (`Bits.clear`).
    pub fn clear(&mut self, stance: UnitStanceId) -> bool {
        let id = stance.raw() as usize;
        if id >= 64 {
            return false;
        }
        self.0[0] &= !(1u64 << id);
        true
    }

    /// Whether `stance` is active (`Bits.get`).
    pub fn get(self, stance: UnitStanceId) -> bool {
        let id = stance.raw() as usize;
        id < 64 && self.0[0] & (1u64 << id) != 0
    }

    /// Removes every bit in `mask` (`Bits.andNot`).
    pub fn and_not(&mut self, mask: u32) {
        self.0[0] &= !(mask as u64);
    }

    /// Whether no stance is active.
    pub fn is_empty(self) -> bool {
        self.0[0] == 0
    }

    /// Raw mask (codec/audit).
    pub const fn raw(self) -> u64 {
        self.0[0]
    }
}

/// `CommandAI.setStance`: drop incompatible stances, then set `stance`.
///
/// Returns `true` when the mask changed. The `stop` stance (id `0`) is ignored,
/// matching upstream (`if(stance == UnitStance.stop) return;`).
pub fn set_stance(bits: &mut StanceBits, content: &ContentRegistry, stance: UnitStanceId) -> bool {
    let Some(def) = content.unit_stance(stance) else {
        return false;
    };
    if stance.raw() == 0 {
        return false;
    }
    let before = bits.0[0];
    bits.and_not(def.incompatible_stance_bits);
    bits.set(stance);
    bits.0[0] != before
}

/// `CommandAI.setStance(stance, enabled)`.
pub fn set_stance_enabled(
    bits: &mut StanceBits,
    content: &ContentRegistry,
    stance: UnitStanceId,
    enabled: bool,
) -> bool {
    if enabled {
        set_stance(bits, content, stance)
    } else {
        disable_stance(bits, stance)
    }
}

/// `CommandAI.disableStance`: clear the bit.
pub fn disable_stance(bits: &mut StanceBits, stance: UnitStanceId) -> bool {
    let before = bits.0[0];
    bits.clear(stance);
    bits.0[0] != before
}

/// Applies a stance definition directly (used when the registry lookup is
/// already in hand).
pub fn apply_stance(bits: &mut StanceBits, def: &UnitStanceDef) -> bool {
    if def.id.raw() == 0 {
        return false;
    }
    let before = bits.0[0];
    bits.and_not(def.incompatible_stance_bits);
    bits.set(def.id);
    bits.0[0] != before
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MemoryBundle, MemoryUnlockStore};

    fn content() -> ContentRegistry {
        let mut registry = crate::content::create_base_content(
            &MemoryBundle::new(),
            &MemoryUnlockStore::new(),
            true,
        )
        .expect("content");
        registry.init().expect("init");
        registry
    }

    #[test]
    fn bits_set_get_clear_and_not() {
        let mut bits = StanceBits::new();
        let a = UnitStanceId::new(1);
        let b = UnitStanceId::new(3);
        assert!(bits.set(a));
        assert!(bits.set(b));
        assert!(bits.get(a) && bits.get(b));
        bits.and_not(1 << 1);
        assert!(!bits.get(a));
        assert!(bits.get(b));
        assert!(bits.clear(b));
        assert!(bits.is_empty());
        assert!(!bits.set(UnitStanceId::new(64)), "out of range");
    }

    #[test]
    fn set_and_disable_stance_toggle_bits() {
        let content = content();
        let patrol = content.unit_stance_by_name("patrol").expect("patrol");
        let mut bits = StanceBits::new();
        assert!(set_stance(&mut bits, &content, patrol.id));
        assert!(bits.get(patrol.id));
        // Setting again is idempotent and reports no change.
        assert!(!set_stance(&mut bits, &content, patrol.id));
        // `stop` (id 0) is ignored, matching upstream.
        assert!(!set_stance(&mut bits, &content, UnitStanceId::new(0)));
        assert!(!bits.get(UnitStanceId::new(0)));
        assert!(disable_stance(&mut bits, patrol.id));
        assert!(!bits.get(patrol.id));
    }

    #[test]
    fn enabled_wrapper_matches_set_and_disable() {
        let content = content();
        let hold_position = content
            .unit_stance_by_name("holdposition")
            .expect("holdposition");
        let mut bits = StanceBits::new();
        assert!(set_stance_enabled(
            &mut bits,
            &content,
            hold_position.id,
            true
        ));
        assert!(bits.get(hold_position.id));
        assert!(set_stance_enabled(
            &mut bits,
            &content,
            hold_position.id,
            false
        ));
        assert!(!bits.get(hold_position.id));
    }
}
