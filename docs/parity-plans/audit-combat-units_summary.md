# combat-units audit — execution summary (session 2)

- **Plan:** `docs/parity-plans/audit-combat-units.md` (main checkout).
- **Session/branch:** session 2, `parity/session-2`, worktree
  `/home/clintonnguyen/g/code_examples/Mindustry-Godot-session2`, display `:21`
  (Xvfb), bridge port `6980` (per the operator's DISPLAY OVERRIDE), 2026-10-12/13.
- **Executor notes** appended to the plan file, section 8 (main checkout).
- **Tests:** baseline 1940 passed / 0 failed / 4 ignored
  (`cd client/rust && CARGO_BUILD_JOBS=6 cargo test -p mind-core --features msav-import`);
  final **1962 passed, 0 failed, 4 ignored** plus `combat_golden` 6/6 and the
  other integration suites green. `cargo clippy -p mind-core --all-targets --
  -D warnings` clean at every commit; `cargo fmt -p mind-core` applied. The six
  pre-existing `mind-headless` failures were not touched.
- **Builds/tests were serialized** behind `flock /tmp/opencode/mg-build.lock`
  with `CARGO_BUILD_JOBS=6`.

## Commit list (`git log --oneline main..HEAD`, 18 commits)

| Commit | Task | Rationale |
|---|---|---|
| `b0d194c` | T9 | `adopt_spawned_units` queries only `SpawnedByBullet` entities (PAR-0088). |
| `5c63c1d` | T1 | scathe missiles: four `MissileUnitType` spawnUnit defs + carrier bullets + missile weapon proximity fuse; goldens/counts regenerated (PAR-0088). |
| `487f79f` | T2 | `Block.underBullets` + `BulletComp.checkUnderBuild` + `MissileAI` contact rule + `AIController.target` building predicate. |
| `9d64483` | T3 | weather cooldowns re-arm per `logic_play` (`PlayEvent`, PAR-0127). |
| `0e984ce` | T6 | `canBoost` mechs lift off (`UnitComp.updateBoosting`, PAR-0137). |
| `825406f` | T10 | derelict units are not targets unless `rules.coreCapture`. *(Commit subject wrongly prefixed `Fixes PAR-0138-adjacent`; intended `Audit fix:`.)* |
| `b02dd4f` | T7 | flying units circle their target (`FlyingAI`/`circleAttack`). |
| `8652fe7` | T8 | `PhysicsProcess` unit physics (layers, mass push, random stack direction). |
| `4b7067b` | T4 | payload key path/building pickup/drop + revision-2 unit save (PAR-0138). |
| `3e9ef27` | T15 | `Bullet.owner` + lifesteal. |
| `68080c3` | T16 | dead buildings leave rebuild ghosts (`Logic` BlockDestroyEvent). |
| `a0585a1` | T17 | flying units deal crash damage on death. *(Commit trailer typo: "Claude Opus 4.5".)* |
| `afaeebb` | T5 | `Damage.dynamicExplosion` waves + building-destroy terms (PAR-0091). |
| `95fced5` | T11 | live units accept/store every command id (PAR-0140). |
| `c385699` | T12 | `Predict.intercept` aim lead in `set_mount_targets`/`face_target`. |
| `e927900` | T14a | Suicide dispatch + `BulletType.init` killShooter. |
| `9d041bb` | T14b | Builder dispatch for commanded tiles. |
| `3e50373` | T14c | Boost dispatch. |

## Per-task status

- **T9, T1, T2, T3, T6, T10, T7, T8, T4, T5, T15, T16, T17, T11, T12:** done.
- **T13** (save entity count): not reached.
- **T14:** partial — Suicide, Builder (commanded tiles only), Boost wired;
  Miner/Cargo/Defender/Hug/FlyingFollow/Logic not reached.
- **T18** (formation): not reached (low priority).
- **T19** (item-source config): reported, not fixed (not in the combat area).

Plan deviations / precondition mismatches (details in the plan's section 8):
T1 needed the missile weapon-targeting pass and explicit Explosion ctor
range overrides; T2's literal tagger test cannot target the wall past a closer
conveyor (decoy used); T2 ported `Block.init`'s *unconditional* under-priority;
T7's "wall destroyed in 1200 ticks" exceeds the flare's DPS; T12's dagger-vs-
perpendicular-flare has no intercept solution (mace used); T11's `boost` is a
stance, not a command.

## In-engine verification (required after T1/T2/T7/T8)

Editor restarted per the display override
(`parity-session.sh --session 2 --display :21 --stop` then `--no-seed-builds`;
bridge re-checked with `godot_health`; scene replayed with
`T=300 tools/session-play.sh 2 --play`, profile `{nodes:375, unlocked:282}`).

Ground Zero (sector 170) wave probe: drill (130,48) + duo (132,48) placed,
`run_wave()` at tick 2421 (4 units). Structured polls:
t=3609 `dagger(1064,1262)`, t=4590 `dagger(1047,774)`, t=5586/6026 only the
player `alpha` -> **wave dead, `hasCore` true**.
Evidence: `runs/20261012-session2-audit-combat-units/wave-probe.txt` and
`wave-after-derelict-fix.png`. Bullet counts sampled 0 between 15 s polls (the
duo's bullets are short-lived); the dagger died to the defence. T4 probe:
`Input.pickup_payload()` returns cleanly (Ground Zero player unit `alpha` has
no payload capacity). T1's scathe in-engine check is skipped per the plan
(no bridge item feed; covered by `scathe_launches_missile_and_kills`).
No other screenshots were needed; all other checks were structured evals
(`get_hud_state`, `unit_debug_positions`, `get_group_counts`, `tile_debug`).

## Blockers / coordinator follow-ups

1. **`server/spacetimedb/src/main/content_seed.rs`**: regenerated in the
   worktree (720 entries) but **not committed** (staging rule = client/parity/
   docs). Run `bash server/gen_content_seed.sh` on `main` and commit it, else
   the plan-21 seed drifts from `mods/catalog.rs`'s pinned 720.
2. **Commit message fixes**: `825406f` subject should not claim PAR-0138 (T10
   has no ledger id); `a0585a1`'s trailer says "Claude Opus 4.5" instead of
   "Claude Opus 5.5".
3. **T13/T14 remainder/T18** are unworked; T14's Miner/Cargo/Defender/Hug/
   FlyingFollow/Logic branches still fall into the generic ground/flying path.
4. Parity gaps recorded but out of task scope: `Block.baseExplosiveness`/
   `flammabilityScale`/`explosivenessScale` are not `BlockDef` fields; the
   ExplosionShield absorption check in `dynamicExplosion`'s wave loop is not
   ported; `Rule.fire` gating for explosion fires; the live stance pipeline
   (boost stance) is relay-only; the unit-death `dynamicExplosion`
   (`UnitComp.java:892`) is not ported.
5. The plan's T14/T9 dependency note holds: T9 landed first, so T1's tests run
  with the fast adopt path.
