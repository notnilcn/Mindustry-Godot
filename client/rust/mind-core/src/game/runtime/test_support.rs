// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Shared fixtures for the campaign runtime tests.
//! Upstream: none (port-only test support).

use crate::content::{ContentRegistry, MemoryBundle, MemoryUnlockStore, create_base_content};
use crate::game::planet::EmptyNeighborhood;
use crate::game::universe::Campaign;

pub(super) fn content() -> ContentRegistry {
    create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true).expect("content")
}

pub(super) fn campaign(registry: &ContentRegistry) -> Campaign {
    Campaign::from_registry(registry, &EmptyNeighborhood)
}
