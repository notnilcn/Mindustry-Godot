// SPDX-License-Identifier: GPL-3.0-only

//! Registry index snapshot — Rust equivalent of `ContentLoader.copy()`.
//!
//! Ported from `core/src/mindustry/core/ContentLoader.java:43-55`. Only the
//! *index* (per-type membership, name maps, current mod, temporary mapper) is
//! snapshotted; content payload rollback is plan 20's `ResetAction` closures
//! (plan 02 §3.4).

use super::load::TemporaryMapper;
use super::names::NameMaps;
use super::{ContentType, ModId};

/// Immutable index snapshot taken before a patch/mod mutation.
///
/// `restore_index` re-syncs registry membership to the snapshot: vectors are
/// truncated to the recorded lengths, the name maps and temporary mapper are
/// restored, and the registry epoch is bumped. Content records removed by a
/// patch are not resurrected (matching upstream `ContentLoader.remove`, which
/// drops objects while `copy()` keeps the index).
#[derive(Debug, Clone)]
pub struct RegistryIndexSnapshot {
    lengths: [usize; ContentType::ALL.len()],
    names: NameMaps,
    current_mod: Option<ModId>,
    temporary_mapper: Option<TemporaryMapper>,
}

impl RegistryIndexSnapshot {
    /// Builds a snapshot from the recorded index parts.
    pub fn new(
        lengths: [usize; ContentType::ALL.len()],
        names: NameMaps,
        current_mod: Option<ModId>,
        temporary_mapper: Option<TemporaryMapper>,
    ) -> Self {
        Self {
            lengths,
            names,
            current_mod,
            temporary_mapper,
        }
    }

    /// Recorded length for `type_`.
    pub fn type_len(&self, type_: ContentType) -> usize {
        self.lengths[type_.ordinal()]
    }

    /// Name maps at snapshot time.
    pub fn names(&self) -> &NameMaps {
        &self.names
    }

    /// Current mod at snapshot time.
    pub fn current_mod(&self) -> Option<&ModId> {
        self.current_mod.as_ref()
    }

    /// Temporary save mapper at snapshot time.
    pub fn temporary_mapper(&self) -> Option<&TemporaryMapper> {
        self.temporary_mapper.as_ref()
    }

    /// Raw lengths array.
    pub fn lengths(&self) -> &[usize; ContentType::ALL.len()] {
        &self.lengths
    }
}
