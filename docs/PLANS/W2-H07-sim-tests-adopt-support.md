<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W2-H07 — Sim tests: one shared fixture module, no copy-pasted setup

**Wave:** W2 (helper package, RULES §4). Test-only. **Needs:** W1 (S11–S14) and W0 F3 merged.
**Owns:**
- `client/rust/mind-core/src/sim/test_support.rs`;
- `sim/combat_tests/**`, `sim/production_tests/**`, `sim/logic_campaign_tests/**`, `sim/spine_tests/**`;
- the test files `sim/unit_runtime/{tests,msav_tests,markers_region_tests,unit_payload_tests}.rs`,
  `sim/runtime/tests.rs`, `sim/builder/tests.rs`;
- `sim/tests.rs`, `sim/fog.rs`'s test module only, and any other `#[cfg(test)]`-only file under `sim/`.

You never edit non-test code: H08 owns that.
**Effort:** medium-large (many sites, low risk).

## Goal

Every sim test builds its world through `sim::test_support`, and the boilerplate disappears:
- `for _ in 0..N { sim.tick().expect("tick"); }`: 101 in combat, 117 in production, 23 in builder, 16 in unit_runtime;
- `Sim::new` plus install pairs: 5 copy-pasted variants;
- `get_resource::<CampaignRuntime>().expect(..)`: 18 + 12 + 6.

**The tests' behaviour must not change.** The same seeds, sizes, fills and lookups must stay, so
each test checks exactly what it checked before.

## Steps

1. **Grow `test_support.rs`** with the helpers below. Each body is **copied** from the fixture it
   replaces; F3 added the first ones and their self-tests.
   ```rust
   pub(crate) fn live_sim_seeded(seed: u64, size: i32) -> Sim;   // F3
   pub(crate) fn live_sim(size: i32) -> Sim;                      // F3, == combat live_sim
   pub(crate) fn stone_sim(size: i32) -> Sim;                     // production runtime_sim/wide_sim body, parameterised
   pub(crate) fn runtime_sim() -> Sim;                            // F3, == stone_sim(32)
   pub(crate) fn runtime_sim_with_content(c: ContentRegistry) -> Sim;
   pub(crate) fn tick_n(sim: &mut Sim, n: usize);                 // F3
   pub(crate) fn unit_content_block(sim: &Sim, name: &str) -> u16; // F3 (via unit_runtime().content, NOT world_block_id)
   pub(crate) fn place_at(sim: &mut Sim, x: i16, y: i16, name: &str, team: u8) -> Entity;  // F3 (combat `place`)
   pub(crate) fn place_team1(sim: &mut Sim, name: &str, x: i16, y: i16) -> Entity;         // F3 (production `place`)
   pub(crate) fn item(sim: &Sim, name: &str) -> ItemId;
   pub(crate) fn liquid(sim: &Sim, name: &str) -> LiquidId;
   pub(crate) fn configure(sim: &mut Sim, x: i16, y: i16, v: ConfigValue);
   pub(crate) fn health(sim: &Sim, e: Entity) -> Option<f32>;
   pub(crate) fn stored_item(sim: &Sim, e: Entity, i: ItemId) -> i32;
   pub(crate) fn stored_liquid(sim: &Sim, e: Entity, l: LiquidId) -> f32;
   pub(crate) fn feed_turret(sim: &mut Sim, turret: Entity, item: &str, n: usize);
   pub(crate) fn spawn(sim: &mut Sim, name: &str, team: u8, x: f32, y: f32) -> Entity;    // spawn_live_unit(..,0.0).expect
   pub(crate) fn campaign_runtime(sim: &Sim) -> &CampaignRuntime;                        // get_resource::<CampaignRuntime>().expect("campaign runtime")
   pub(crate) fn campaign_runtime_mut(sim: &mut Sim) -> Mut<'_, CampaignRuntime>;
   // moved from production_tests (3025–3086 at 291e30a8):
   pub(crate) fn install_campaign_session(..); pub(crate) fn install_logic_world(..);
   pub(crate) fn place_world_processor(..); pub(crate) fn test_rules() -> Rules;
   // from logic_campaign_tests helpers:
   pub(crate) fn finish_sim(..); pub(crate) fn campaign_sim() -> Sim; pub(crate) fn wide_sim(n: i32) -> Sim;
   pub(crate) fn assert_events_drained(sim: &Sim);
   ```
   Where two fixtures share a name but differ (the two `place` fns, the `live_sim` in
   `spine_tests/ability_queue` with seed 7 at 24×24, `builder::tests::sim_with_player` with seed 7 at
   64×64, `unit_runtime::tests::install`), give them **different names** that state the difference.
   Never "fix" a seed.
2. **Replace the fixture definitions** in `combat_tests/mod.rs`, `production_tests/mod.rs`,
   `logic_campaign_tests/mod.rs` with `pub(super) use crate::sim::test_support::{… as old_name};`
   aliases, so test bodies compile unchanged. Then update call sites file by file to the new names
   and remove the aliases. The cross-file importers (`sim/fog.rs:581/593`, `sim/tests.rs:550,1398–1493`)
   switch to `test_support` directly.
   When the aliases are gone, F3's self-tests (`live_sim_matches_combat_fixture`,
   `runtime_sim_matches_production_fixture`) would compare a helper with itself. Delete them, and
   record the deletion in the report (an expected change to the test inventory).
3. **`tick_n`.** Replace `for _ in 0..N { sim.tick().expect("tick"); }` loops whose body is exactly
   that. Loops with extra statements stay.
4. **`campaign_runtime`.** Replace `sim.ecs.0.get_resource::<CampaignRuntime>().expect(..)` where the
   expect message does not matter to the test; messages only show up on failure.
5. **Optional `LogicFixture` builder** for `logic_campaign_tests`:
   `LogicFixture::new().rules(|r| ..).size(300).processor(x, y, code).build() -> Sim`. It is
   justified by 59× `campaign_sim()`, 48× `place_world_processor`, and 11× `finish_sim(...)` with rule
   tweaks. Add it only if it makes the tests shorter **and** each converted test still builds the same
   world. Convert at most one file first and show the diff in the report.

## Verification

```bash
run --cargo-test -p mind-core --lib sim::
run --cargo-test -p mind-core --features msav-import sim::unit_runtime
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p mind-core --all-targets -- -D warnings
run --cargo-test -p mind-core -- --list 2>/dev/null | grep -E ': test$' | sort > /tmp/H07-after.txt
diff <(sort docs/PLANS/baseline/tests-mind-core.txt | sed 's/.*:://' ) <(sed 's/.*:://' /tmp/H07-after.txt) | head  # test names are the same set (modulo module paths); F3's self-tests are added
git diff --stat main...HEAD     # expect a large net deletion
```

Every test that passed at the branch point must pass now. A test that newly fails means a fixture
changed its world; revert that site.
