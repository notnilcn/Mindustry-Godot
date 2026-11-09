<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W0-F3 — `mind-core` test-support modules

**Wave:** W0, after F0, in parallel with F1 and F2.
**Owns:**
- `client/rust/mind-core/src/sim/test_support.rs` (new)
- `client/rust/mind-core/src/game/test_support.rs` (new)
- one `#[cfg(test)] pub(crate) mod test_support;` line each in `sim/mod.rs` and `game.rs`

**Effort:** small.

## Why

In W2, H07 (sim tests) and H01/H02 (game tests) remove a lot of copy-pasted fixture code. Their
shared fixture modules must be **declared** in `sim/mod.rs` and `game.rs`, but in W2 those two
files belong to H08 and H02. This package creates the modules and their first helpers now,
so nobody in W2 needs to edit those `mod` files.

This package changes **no existing test**. It only adds helpers that are byte-identical copies of
existing fixtures. The originals stay where they are until H07/H01/H02 adopt the new ones.

## Steps

### 1. Worktree and baseline (RULES §1, §6)

Record the mind-core test list.

### 2. `game/test_support.rs`

Add `#[cfg(test)] pub(crate) mod test_support;` to `mind-core/src/game.rs`, after the `pub mod`
block (lines 7–36). The file:

```rust
// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Shared fixtures for `game::*` unit tests (campaign, saves, tech tree, play flows).
//! Upstream: none (port-only test support).

use crate::content::{ContentRegistry, MemoryBundle /* …exact paths… */};
use crate::game::planet::EmptyNeighborhood;
use crate::game::universe::Campaign;

/// `create_base_content` **without** `init`/`post_init`, as most `game::*` tests use it.
/// Same as `game/runtime.rs:2122`, `game/play.rs:901`, `game/saves.rs:700`.
pub(crate) fn raw_registry() -> ContentRegistry {
    create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true).expect("content")
}

/// `Campaign::from_registry` over `EmptyNeighborhood` (`game/runtime.rs:2126`, `game/play.rs:907`).
pub(crate) fn campaign(registry: &ContentRegistry) -> Campaign {
    Campaign::from_registry(registry, &EmptyNeighborhood)
}
```

Fix the `use` paths so it compiles; copy the imports from `game/runtime.rs`'s test module.
Note the difference from `crate::content::test_support::test_registry()` (`content/mod.rs:415`),
which **also** runs `init`, `post_init` and `log_content`. They are not interchangeable; say so in
both doc comments.

Before you add it, check every per-module copy:
```bash
grep -n "fn registry()\|fn content()\|fn campaign(" client/rust/mind-core/src/game/*.rs client/rust/mind-core/src/game/**/*.rs
```
At `291e30a8` this grep finds 10 copies. Some return `(ContentRegistry, MemoryUnlockStore)` (`tech_tree.rs:643`),
and some use `.unwrap()` instead of `.expect`. Both are fine to unify later. List all 12 in the
report with their exact bodies.

Also add the **own-a-sector** seed. Find the repeated block that sets `save = Some("sector-{p}-{id}")`,
`has_core = true`, `storage_capacity = …`, `waves = …`, `attack = false` (in game tests and in
`mind-headless/src/campaign_scenarios.rs` ~362–381, 506–514). If one canonical body exists,
copy it as `pub(crate) fn own_sector(campaign: &mut Campaign, planet: PlanetId, id: u16, capacity: i32)`.
If the copies differ, add none, and list the variants in the report.

### 3. `sim/test_support.rs`

Add `#[cfg(test)] pub(crate) mod test_support;` to `sim/mod.rs`, next to the existing test-module
declarations (~3195–3202). The file holds **copies** of the canonical fixtures. Each helper's
doc names the original it copies.

```rust
/// AIR floor/overlay, seed `seed`, block + unit runtime installed.
/// `live_sim_seeded(1, n)` == `combat_tests::live_sim(n)` (combat_tests.rs:17).
pub(crate) fn live_sim_seeded(seed: u64, size: i32) -> Sim { … }
pub(crate) fn live_sim(size: i32) -> Sim { live_sim_seeded(1, size) }

/// Block id via the **unit runtime's** content (`combat_tests::block`, :25).
/// NOT `Sim::world_block_id`, which takes `&mut self` and lazily installs `WorldApplyRuntime`
/// (that changes `world_apply_checksum`).
pub(crate) fn unit_content_block(sim: &Sim, name: &str) -> u16 { … }

/// `combat_tests::place` (:34): SimCommand::Place, rotation 0, player None; returns `tiles.get(x,y).build`.
pub(crate) fn place_at(sim: &mut Sim, x: i16, y: i16, name: &str, team: u8) -> Entity { … }

/// `production_tests::place` (:16-36): team 1, argument order (name, x, y); returns via `grid.entity_at`.
pub(crate) fn place_team1(sim: &mut Sim, name: &str, x: i16, y: i16) -> Entity { … }

/// `production_tests::runtime_sim` (~445): seed 3, stone fill, block runtime only.
pub(crate) fn runtime_sim() -> Sim { … }

/// `for _ in 0..n { sim.tick().expect("tick"); }` (101 copies in combat_tests, 117 in production_tests).
pub(crate) fn tick_n(sim: &mut Sim, n: usize) { … }
```

**Copy each body from the original. Do not write it fresh.** The seeds, sizes, fill blocks and
lookup paths differ on purpose; the analysis found 5 different `Sim::new` setups:
- combat `live_sim`: seed 1;
- production `runtime_sim`: seed 3, stone;
- `mod.rs::ability_queue_tests::live_sim`: seed 7, 24×24;
- `builder::sim_with_player`: seed 7, 64×64;
- `unit_runtime::tests::install`: ~7054, the same as ~9203.

Add only the helpers listed above. H07 adds the rest when it adopts them.

Add two self-tests to `sim/test_support.rs`. Each builds the same sim through the new helper and
through the original fixture, ticks it 60 times, and asserts equal `sim.checksum()`:
`live_sim_matches_combat_fixture` and `runtime_sim_matches_production_fixture`. To reach the
originals, make them `pub(super)` in `combat_tests.rs`/`production_tests.rs`. Those files belong to
this package for this one-word visibility change only. `live_sim` and `place` are already `pub(super)`.

### 4. Verify (RULES §6)

- Run `run --cargo-test -p mind-core --lib sim::test_support` and
  `run --cargo-test -p mind-core --lib game::test_support`.
- The test inventory grows only by the new self-tests.
- Scoped clippy for mind-core: unused helpers are fine inside `#[cfg(test)]` only with
  `#[allow(dead_code)]` on the module (`#![allow(dead_code)] // adopted in W2`). Add that.

Report: the fixture catalogue (every variant with `file:line`), plus a CI entry with gates
`fmt,clippy,cargo-test`.
