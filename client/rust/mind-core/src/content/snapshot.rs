// SPDX-License-Identifier: GPL-3.0-only

//! Registry index snapshot — Rust equivalent of `ContentLoader.copy()`.
//!
//! Ported from `core/src/mindustry/core/ContentLoader.java:43-55`. Only the
//! *index* (per-type membership, name maps, current mod) is snapshotted; content
//! payload rollback is plan 20's `ResetAction` closures (plan 02 §3.4).

use super::{ContentType, ModId};

/// Immutable index snapshot taken before a patch/mod mutation.
///
/// `restore_index` re-syncs registry membership to the snapshot: vectors are
/// truncated to the recorded lengths and the name maps are rebuilt. Content
/// records removed by a patch are not resurrected (matching upstream
/// `ContentLoader.remove`, which drops objects while `copy()` keeps the index).
#[derive(Debug, Clone, PartialEq)]
pub struct RegistryIndexSnapshot {
    lengths: [usize; ContentType::ALL.len()],
    current_mod: Option<ModId>,
}

impl RegistryIndexSnapshot {
    /// Builds a snapshot from per-type lengths and the current mod.
    pub fn new(lengths: [usize; ContentType::ALL.len()], current_mod: Option<ModId>) -> Self {
        Self {
            lengths,
            current_mod,
        }
    }

    /// Recorded length for `type_`.
    pub fn type_len(&self, type_: ContentType) -> usize {
        self.lengths[type_.ordinal()]
    }

    /// Current mod at snapshot time.
    pub fn current_mod(&self) -> Option<&ModId> {
        self.current_mod.as_ref()
    }

    /// Raw lengths array.
    pub fn lengths(&self) -> &[usize; ContentType::ALL.len()] {
        &self.lengths
    }
}
