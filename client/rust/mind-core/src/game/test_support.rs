// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Shared fixtures for `game::*` unit tests (campaign, saves, tech tree, play
//! flows). Adopted by W2-H01 in the `saves`, `planet`, `sector` and `universe`
//! test modules.
//! Upstream: none (port-only test support).
//!
//! There is no `own_sector` seed: the per-site bodies differ in save-name
//! construction, capacity and extra fields (see the F3/H01 reports), so W2-H01
//! left the copies in place (RULES §4).

use crate::content::{ContentRegistry, MemoryBundle, MemoryUnlockStore, create_base_content};
use crate::game::planet::EmptyNeighborhood;
use crate::game::universe::Campaign;

/// `create_base_content` **without** `init`/`post_init`, as most `game::*`
/// tests use it. Same as `game/runtime.rs:2141`, `game/play.rs:904`,
/// `game/saves.rs:701`, `game/launch_payload.rs:184`, `game/planet.rs:650`,
/// `game/schematic.rs:273`, `game/schematics.rs:1090` and
/// `game/tech_tree.rs:646`.
///
/// **Not** interchangeable with `crate::content::test_support::test_registry()`
/// (`content/mod.rs:415`), which also runs `init`, `post_init` and
/// `log_content`; a test that needs registered/linked content must use that
/// one instead.
pub(crate) fn raw_registry() -> ContentRegistry {
    create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true).expect("content")
}

/// `Campaign::from_registry` over `EmptyNeighborhood` (`game/runtime.rs:2145`,
/// `game/play.rs:908`).
pub(crate) fn campaign(registry: &ContentRegistry) -> Campaign {
    Campaign::from_registry(registry, &EmptyNeighborhood)
}
