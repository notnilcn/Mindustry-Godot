<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W1-S13 — Split `sim/combat_tests.rs` (7438 lines, 147 tests) by topic

**Wave:** W1 (pure mechanical split, RULES §3). Test-only, so the risk is low.
**Owns:** `client/rust/mind-core/src/sim/combat_tests.rs` → `client/rust/mind-core/src/sim/combat_tests/**`.
**Effort:** medium (mechanical).

## Layout

`git mv sim/combat_tests.rs sim/combat_tests/mod.rs`. `sim/mod.rs` keeps `#[cfg(test)] mod combat_tests;`.

`mod.rs` keeps the imports and the fixtures (17–69: `live_sim`, `block`, `place` are `pub(super)`;
`feed`, `health`), `shots_in` 105, `spawn_unit_weapon` 234 and `campaign_sim` 2174. Fixtures that
topic files use become `pub(super)`. Do **not** replace them with the F3 `sim::test_support`
copies; H07 does that in W2.

Topic files (lines at `291e30a8`). Each starts with `use super::*;` and holds the `#[test]` fns of
its range verbatim:

| File | Lines |
|---|---|
| `turrets_bullets.rs` | 70–531 |
| `motion_payload.rs` | 532–840 |
| `deaths.rs` | 841–1031 |
| `commands_ai.rs` | 1032–1604 |
| `fire_weather.rs` | 1605–1843 |
| `payload_weapons.rs` | 1844–2173 |
| `multipliers_player.rs` | 2174–2664 (`campaign_sim` 2174 stays in `mod.rs`) |
| `pathing_mining.rs` | 2665–3410 |
| `environment.rs` | 3411–3652 |
| `command_explosion.rs` | 3653–4004 |
| `defense_blocks.rs` | 4005–4391 |
| `targeting.rs` | 4392–4940 |
| `fog.rs` | 4941–5347 |
| `building_explosions.rs` | 5348–5544 |
| `player_campaign.rs` | 5545–5723 |
| `physics.rs` | 5724–6177 |
| `ai_kinds.rs` | 6178–6427 |
| `walls.rs` | 6428–6599 |
| `assist_cargo_control.rs` | 6600–7438 |

Topic ranges may contain local helper fns between the tests (for example `shots_in` at 105 inside
`turrets_bullets`). A helper used by **one** topic moves with it. A helper used by several stays in
`mod.rs` as `pub(super)`.

## Paths other files depend on

`sim/fog.rs:581/593` imports `combat_tests::live_sim`, and `sim/tests.rs:1398/1423/1452/1493` imports
`combat_tests::{live_sim, place}`. They stay valid because the fixtures stay in `combat_tests/mod.rs`
with `pub(super)` visibility, which means visible in `sim`. Do not edit `fog.rs` or `tests.rs`.

## Verification

```bash
python3 tools/refactor/move_check.py --base $(git merge-base main HEAD) \
  --old client/rust/mind-core/src/sim/combat_tests.rs --new client/rust/mind-core/src/sim/combat_tests
run --cargo-test -p mind-core --lib sim::combat_tests
run --cargo-test -p mind-core -- --list 2>/dev/null | grep -E ': test$' | sort > /tmp/S13-tests-after.txt
# the count must equal the baseline; names differ only by the inserted topic module segment
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p mind-core --all-targets -- -D warnings
```

Grep `TWIN-VERIFICATION.json` and `parity/` for `combat_tests::`. List every cited test path in
the report for C01, because its path now has a topic segment.
