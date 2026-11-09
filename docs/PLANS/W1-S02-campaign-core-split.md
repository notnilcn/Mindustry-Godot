<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W1-S02 — Split `mind-core` campaign runtime and the campaign UI view module

**Wave:** W1 (pure mechanical split, RULES §3). **Priority:** first group.
**Owns:**
- `client/rust/mind-core/src/game/runtime.rs` (becomes `game/runtime/**`);
- `client/rust/mind-core/src/ui/campaign.rs`;
- the new `client/rust/mind-core/src/ui/block_catalog.rs`;
- `client/rust/mind-core/src/ui/mod.rs` (one `pub mod` line; check the file name, it may be `ui.rs`).

**Effort:** medium. **Determinism-sensitive:** RULES §5 applies.

## Part A — `game/runtime.rs` (3414 lines) → `game/runtime/`

`CampaignRuntime` (`#[derive(Resource)]`, 261–310) is the ECS resource for a loaded sector. Its
own impl is small (315–371). Most of the file is free ECS systems, hook types and the team-AI
base builder, plus about 1300 lines of tests.

`git mv client/rust/mind-core/src/game/runtime.rs client/rust/mind-core/src/game/runtime/mod.rs`, then
move items out of `mod.rs` into:

| File | Lines at `291e30a8` (doc comments included) | Upstream (`core/src/mindustry/…`) |
|---|---|---|
| `runtime/mod.rs` (stays) | 1–53 (header, imports, the three tick constants `TICKS_PER_SECOND`, `GLOBAL_UPDATE_TICKS`, `SERVICE_CHECK_TICKS`; they **stay here** and children see them), 255–377 (`CampaignRuntime`, `ABANDON_KILL_DELAY_TICKS`, its impl, `rules_of`), plus the `mod` lines and the `pub use` block below | `core/Logic.java`, `core/Control.java` (campaign half) |
| `runtime/hooks.rs` | 67–221: `UnlockRef`, `CampaignHookQueues` + `Default`, `CampaignCoreHooks` + `impl CoreCampaignHooks`, `CampaignCoreQueues`, `install_campaign_core_hooks`, `reset_campaign_core_hooks`, `unlock_sort_key`, `event_content`. The last two become `pub(super)`; `tick.rs` uses them (~642/654) | `world/blocks/storage/CoreBlock.java` (campaign hooks), `game/EventType.java` |
| `runtime/session.rs` | 55–65 (`SessionSync`), 379–544: `apply_played_invasions`, `sync_session_with_sim`, `run_wave_now` | `core/Logic.java` (`runWave`), `game/Universe.java` (invasions) |
| `runtime/tick.rs` | 545–768: `campaign_tick_system`, `apply_sector_lighting`, `sector_light` | `core/Logic.java` (`update`), `game/Universe.java` (`update`) |
| `runtime/objectives.rs` | 769–971: `objectives_tick_system`, `ObjectiveInputState`, the `impl crate::sim::Sim { set_command_mode_satisfied }` block (~828–837), `RuntimeObjectiveEnv` + impl | `game/MapObjectives.java` |
| `runtime/game_state.rs` | 972–1048: `game_state_tick_system`, `current_sector_captured` | `core/Logic.java` (`checkGameState`) |
| `runtime/weather.rs` | 1296–1489: `weather_system`, `weather_water_attr` | `type/Weather.java`, `core/Logic.java` (`updateWeather`) |
| `runtime/team_ai/mod.rs` | 222–256 (`BaseCatalog`, `AiPlaceRequest`, `CampaignAiQueues`, `install_campaign_ai_queues`), 1049–1295 (`TARGET_FLAGS`, `TeamAiUnitRow`, `TeamAiBuildingRow`, `team_ai_system`), 1936–2115 (core/enemy tiles, `collect_team_ai_rows`, `team_ai_targets`/`damaged`/`enemy_stats`) | `ai/RtsAI.java`, `ai/BaseBuilderAI.java` (use whichever the doc comments cite) |
| `runtime/team_ai/builder.rs` | 1490–1820: `run_team_base_builder`, `core_unit_of`, `core_unit_type`, `live_type_count`, `fill_first_core_items`, `spawn_*`, `run_prebuild_ai_defenders` | `ai/BaseBuilderAI.java`, `ai/types/PrebuildAI.java` |
| `runtime/team_ai/stats.rs` | 1821–1935: `enemy_stats_for`, `turret_enemy_stats` | same as builder |
| `runtime/test_support.rs` (`#[cfg(test)]`) | 2122–2129: test helpers `content()` and `campaign()` (`pub(super)`) | – |
| `runtime/tests/*.rs` | 2116–3414, split by topic (below) | – |

### Re-exports (critical)

Other crates and modules use `crate::game::runtime::X` / `mind_core::game::runtime::X`. Keep every
such path valid with `pub use` lines in `runtime/mod.rs`:

- **mind-gdext / mind-headless:** `CampaignRuntime`, `rules_of`, `BaseCatalog`,
  `apply_played_invasions`, `reset_campaign_core_hooks`, `ObjectiveInputState`,
  `install_campaign_core_hooks`, `install_campaign_ai_queues`, `UnlockRef`, `SessionSync`,
  `sync_session_with_sim`, `run_wave_now`, `CampaignAiQueues`, `AiPlaceRequest`.
- **mind-core:**
  - `CampaignCoreQueues`;
  - the systems `campaign_tick_system`, `objectives_tick_system`, `game_state_tick_system`,
    `team_ai_system`, `weather_system` (wired in `sim/schedule.rs`; **do not touch** `schedule.rs`);
  - `weather_water_attr`, `current_sector_captured`.

Grep to be sure nothing else is named:
```bash
grep -rhn "game::runtime::{\?[A-Za-z_]*" client/rust --include=*.rs | grep -o "runtime::[A-Za-z_{}, ]*" | sort | uniq -c
```
Each re-export uses the item's **original visibility**: `pub use` for `pub` items,
`pub(crate) use` for `pub(crate)` items.

### Tests

Make the test modules children of the code they test, or keep them together in
`runtime/tests/mod.rs` with topic files. Either way, the test **count must not change**. Line =
each test fn's start; its `#[test]` and doc sit 1–3 lines above.

| Topic file | Tests |
|---|---|
| `tests/game_state.rs` | 2131 (noop without resource), 2143, 2172, 2897 |
| `tests/team_ai.rs` | 2199, 2486; helpers 2571–2637 (`t10_*`, `alpha_id`); tests 2638, 2668, 2694, 2711, 2742 |
| `tests/weather.rs` | 2291, 2361 |
| `tests/session.rs` | 2421, 3252, 3288 |
| `tests/tick.rs` | 2804, 2859, 2943, 2976, 3015, 3207 |
| `tests/hooks.rs` | 3102, 3147 |
| `tests/objectives.rs` | 3044, 3338–3414 |

## Part B — `ui/campaign.rs` (3445 lines): extract the block catalog

Lines 457–803 (block catalog types plus `BlockVisibilityContext`) and their tests 2558–2888 are
`PlacementFragment` data, not campaign data. Move them to `mind-core/src/ui/block_catalog.rs`
(upstream `ui/fragments/PlacementFragment.java`). In `ui/campaign.rs`, add
`pub use crate::ui::block_catalog::*;` (or the explicit list) so that
`mind_core::ui::campaign::{block_catalog…, BlockVisibilityContext, …}` still resolves. Consumers:
`mind-gdext/src/input/desktop.rs` (~669, 3965–3968, 4356, 4408, 5325, 5399, 5457),
`input/mod.rs:441`, `campaign.rs` ~3807–3855.

**Leave the rest of `ui/campaign.rs` in place.** T01 (W4) restructures it. Do not add `Deserialize`
or change field order: the serialized `CampaignViews` is sha256-pinned
(`mind-core/tests/goldens/ui/campaign.json`, `parity/golden_manifest.json` ~536).

## Verification (RULES §6)

```bash
python3 tools/refactor/move_check.py --base $(git merge-base main HEAD) \
    --old client/rust/mind-core/src/game/runtime.rs --new client/rust/mind-core/src/game/runtime
python3 tools/refactor/move_check.py --base $(git merge-base main HEAD) \
    --old client/rust/mind-core/src/ui/campaign.rs \
    --new client/rust/mind-core/src/ui/campaign.rs client/rust/mind-core/src/ui/block_catalog.rs
run --cargo-check --workspace --all-targets          # mind-gdext and mind-headless import these paths
run --cargo-test -p mind-core --lib game::runtime
run --cargo-test -p mind-core --lib ui::
run --cargo-test -p mind-headless
run --cargo-run -q -p mind-headless -- parity goldens
run --goldens --check
```

The test inventory must be the same count, with only the `game::runtime::tests::…` paths changed.

In the report, under "Docs to update in C01", list the `runtime.rs` row of
`client/rust/mind-core/src/game/AGENTS.md`.
