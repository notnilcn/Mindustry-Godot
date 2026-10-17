<!-- SPDX-License-Identifier: GPL-3.0-only -->
# L08 — Mod data host, goldens, evidence, closing walk — summary

Owner: session B (session 3 worktree, branch `parity/session-3`). Round L, wave 1.
Session A reports L08 T5 (and later T6) in its own wave reports; the coordinator merges them here.

| Task | Status | Wave |
|---|---|---|
| T1 Golden suites green with attribution (PAR-0362) | **DONE** (green except the deferred L02 T1/L06 batch, see below) | 1 |
| T2 `ModDataManager` owner + decorator (PAR-0394) | pending | 5 |
| T3 Mod content parser TODOs | pending | 5 |
| T4 Evidence debt (PAR-0408 …) | pending | 5 |
| T5 Harness text input / focus | session A | 1 |
| T6 Closing twin walk (round M) | session A | last |

## T1 — golden suites green with attribution

### Method

- Every failing assertion was traced to the commit that moved it, on golden-producing **code**
  only (no ledger history): `git log -S` / `-L` where the cause was obvious, otherwise a `git
  bisect run` over the code with the suite's own command as the predicate (scripts and logs:
  `runs/20261018-s3-L08-T1/`). Mainline merges were bisected `--first-parent`, then the merge's
  branch commit by commit; every attribution was confirmed by "old value at C^, today's value
  at C".
- Harness work: a `MIND_BLESS=1` path in every cargo-test golden harness touched
  (`blocks_golden`, `combat_golden`, `sim_core_meta`, `sim_core_determinism`,
  `logistics_golden`, `maps_editor_golden`, and the audio/input scenario writers through
  `mind_headless::paths::blessing()`); `tools/regen_goldens.sh --record <id>` re-records one
  manifest sha256 after a harness bless; `parity/golden_manifest.json` entries gained an
  optional `reasons` list (attributed regen history, parsed by `parity::golden::Golden`).
- Only the moved suite was regenerated each time; each commit body carries the reason.

### The plan's five suites

| Suite | Delta | Cause (commit, Java) | Fix | Commit |
|---|---|---|---|---|
| `blocks_golden::family_counts` | other 164→158, production 45→51 | 390b1bc3 (G02 T1): `HeatProducer` → `BlockKindData::Crafter` (`HeatProducer extends GenericCrafter`, HeatProducer.java:13); the six HeatProducer blocks (Blocks.java:1429,1452,1467,1485,1689,6792) | bless | c8d326ab |
| `audio_golden` events_sim | tick-4 `explosionArtillery` missing; `shootDuo` moved | 8e7b68b8 (H01 T1): building Pos = `tile.drawx()` (BuildingComp.java:126), bullet rows `World.toTile` = round (World.java:174, BulletComp.java:187); the scenario spawned at the half-tile centre y=68 → row 9, missing the row-8 walls. **Not** ArtilleryBulletType despawn timing: the `fuse` fixture is a Basic bullet with `hit_sound = explosionArtillery`, 40 of 100 lifetime ticks. Hit pitch 1.0→1.0638 = e414cac9 (H07 T4, BulletType.java:564) | scenario on `tile_origin`; every position = old − (4,4); bless | 0631d671 |
| `input_golden` (4 of 11 scenarios; the loop hid three) | placement table: 10 rows true→false; mobile replay + preview handoff rotation 1→0; rts_move `queue` key + unit target y=1e6 | b1e2f543 (G04 T5, World.getDarkness edge term, Build.java:229 / World.java:483-503); 4ee70402 (H02 T3, same-block canReplace, Block.java:798 / CoreBlock.java:182); 29ebd1b3 (H08 T1, InputHandler.java:1945-1946, 2467-2469); 1eaf4bed (I01 T13, InputHandler.java:340-352); 2b6d7cd1 (PAR-0157 lossless unit-id wire) | bless | 3572e1b2 |
| `logistics_golden` (2 scenarios; the loop hid one) | unloader_drain, payload_driver_throw checksums | 9c4f0e80 (H02 T14): unloader `speed = 60f/11f` (Blocks.java:2164); 4f467a42 (PAR-0139): Arc `Point2.pack` order (x high) | bless | 642dc534 |
| `maps_editor_golden` round-trip preview | 57bfde10… → 52a37ff7… | 49f0268b (PAR-0114): air's default `mapColor` is the no-draw sentinel (MapIO.java:95-103); walls on the flipped row | bless | 3b1014c8 |

### Red suites the plan did not list (all `tools/ci.sh` gates)

| Suite | Cause | Fix | Commit |
|---|---|---|---|
| `sim_core_meta::entitymeta` | b1ea8694 (Audit fix T1): transient `Building.should_consume` field (BuildingComp.java:761) | bless | 95dba852 |
| `combat_golden` (4 tests) | 8e7b68b8 (H01 T1) for basic/turret/volley (half-tile spawns), da2da9aa (G01 T3) for rail, b7d104af (G01 T4, BulletType.java:465-486 `hitTile`) for turret | tests on `tile_origin`: basic + rail reproduce the **old** goldens unchanged; volley/turret blessed (values identical at 8e7b68b8 and HEAD with the fixed test) | a25db842 |
| `sim_core_determinism` (2 tests) | ddeae35d (J08 T3): `ecs::TeamId::SHARDED` 0 → 1 (game/Team.java: `sharded = new Team(1, …)`) | bless sampled checksums + scenario `expect.checksum` | 3dc982bb |
| `run-all --tier T0` (1/4) + `run spine_many_commands` | same ddeae35d | spine_place_break a1a7b961→3e48c583, spine_determinism 57bf3097→2d61ec13, sim_core_boot eaf9472d→87ef92ca, spine_many_commands c8214320→08d1c504; `tools/mcp_smoke.py` GOLDEN_CHECKSUM updated | ee745bb3 |

No unexplained delta remained and no port behaviour was changed for T1. Every move above is
Java-correct; two were harness geometry bugs (audio, combat) fixed in the scenario/test.

### Test commands and results (end of wave 1, branch head)

```
cargo fmt --manifest-path client/rust/Cargo.toml --all -- --check                         # clean
cargo clippy --manifest-path client/rust/Cargo.toml --workspace --all-targets -- -D warnings  # clean
cargo test -p mind-core --no-fail-fast      # 2757 passed / 3 failed (deferred, below) / 4 ignored; all integration suites green
cargo test -p mind-headless --no-fail-fast  # 133 lib + every golden suite green
cargo test -p mind-gdext --lib              # 77 passed
mind-headless parity goldens                # 97 PASS / 0 FAIL
mind-headless run-all --tier T0             # 4/4
mind-headless run spine_place_break|spine_determinism|spine_many_commands --json   # pass
bash tools/godot.sh --headless --editor --quit --path client   # exit 0, no script errors
```

Red **by design** (L-INDEX speed rule 3: regenerate in one batch with L06):
`mind-core maps::planet::{serpulo,erekir,asteroid}::tests::*_generation_checksum`, moved by
L02 T1 (Floor.decoration, see the L02 summary). Not run this wave: the release bench gate and
the STDB checks of `tools/ci.sh` (no server/bench-relevant change).

### Deviations / notes

- Plan text vs findings: the tick-4 `explosionArtillery` was a scenario geometry bug, not
  artillery despawn timing; the "shootDuo muzzle moved per H01 T1" is the same geometry.
- The bisect tooling is kept as evidence only (not committed to `tools/`).

## Ledger changes to apply

| Item | Change | Evidence |
|---|---|---|
| PAR-0362 | close (T1): every golden suite green with attribution except the deferred planet-generation batch (L02 T1 + L06) | commits c8d326ab..ee745bb3; `runs/20261018-s3-L08-T1/` |
| chains.json recipes pinning `a1a7b96167c9718d` (spine_place_break in-engine checks, e.g. `c420`, `c422`, `k38`, the boot/save recipes) | update the canonical spine checksum to `3e48c583a94fbea4` (J08 T3 TeamId::SHARDED) | ee745bb3 |

## Done criteria (T1)

`tools/ci.sh` gates green except the documented host bench gate and the deferred
generation checksums, which are batched with L06 by plan.
