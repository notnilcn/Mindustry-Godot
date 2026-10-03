# 23 — PARITY, VERIFICATION & PERFORMANCE IMPLEMENTATION PLAN

> The cross-cutting oracle. This plan owns the harness, the master matrix, the golden-artifact policy, the determinism program, the scenario/MCP catalogs, the performance program, CI, the parity ledger and the aggregated risk register.
> It inherits `HIGH_LEVEL_PLAN.md` §0 (locked decisions), §2 (architecture), §4 (template), §6–§9. Section numbers follow §4 exactly.

## 1. Header block

| Field | Value |
|---|---|
| **Status** | Draft v1 — continuous program. 40 aggregated `NEEDS USER DECISION` items are owned by sibling plans (none block this plan's harness milestones). Authored 2026-10-01 against plans 00–17; plans 18–22 are **not on disk** (matrix/catalog rows marked `TBD by {plan}`). **F17 lane (`lane/f17-23`, 2026-10-03):** matrix planned rows for plans 12/13 filled in — **100 rows, 91 landed / 9 no-equivalent / 0 planned**; `scenario_catalog.json` **107** entries with `tier`+`plan`; `golden_manifest.json` **64** (`oracle`+`scenario`+`harness` sha256); `bench_budgets.json` **40** rows; `checksum_registry` reviewed; `mind-headless list --json`/`run-all` landed; `parity check` 6/6 PASS. |
| **Phase** | Continuous (cont.) — starts at plan 00 M3 (harness exists) and runs through every phase gate P0–P8. |
| **Depends on** | All plans: `00`–`17` on disk (read in full); `18`–`22` by filename from `HIGH_LEVEL_PLAN.md` §3 (not yet on disk); `HIGH_LEVEL_PLAN.md`; `PRELIMINARY_PLAN.md`. Continuous: picks up each plan's §7 as it lands. |
| **Blocks** | Every phase gate (`HIGH_LEVEL_PLAN.md` §5): P0…P8 each require a green `parity gate --phase Pn` report. No implementation plan is considered complete without a matrix row and a gate entry here. |
| **Sources** | `mindustry-godot/HIGH_LEVEL_PLAN.md`, `PRELIMINARY_PLAN.md`; all sibling plans `00`–`17` (header, §7, §8 read for every one); `Mindustry/tests/AGENTS.md` + `Mindustry/tests/src/test/java/{ApplicationTests,DataAssetTests,PatcherTests,LogicTests,GenericModTest,ModTestAllure}.java` + `power/{PowerTestFixture,PowerTests,DirectConsumerTests,ConsumeGeneratorTests}.java` + `tests/src/test/resources/{77,85,108,114,152,152_be}.msav`; `Mindustry/AGENTS.md`; skills `playtest` (`/mnt/c/Users/Clinton/g/.opencode/skills/playtest/SKILL.md`) and `godot-compositor-testing` (`/mnt/c/Users/Clinton/g/.opencode/skills/godot-compositor-testing/SKILL.md`). |
| **Extends spine** | Adds to the plan-00 rig: the `parity/` tree; `mind-headless` subcommands (`list/run/run-all/parity/checksums/gate/bench-gate/soak/desync-inject/report`); scenario tiers; the master matrix; the golden manifest + JVM dump scripts; cross-platform/cross-process determinism jobs; baseline + regression gates; the MCP scenario catalog and smoke orchestrator; `tools/parity.sh|.ps1`; nightly CI. It never changes sim state or the plan-00 extension contract except by a documented policy entry (§6). |

**Boundary (hard).** Individual plans own their own tests, scenarios and budgets; they must keep the names they published in their §7. **This plan owns**: (a) the harness that runs them, (b) the master matrix and its completeness check, (c) the golden/artifact format + storage + drift gates, (d) determinism policy and cross-platform checks, (e) the scenario and MCP catalogs, (f) performance baselines and phase-gate thresholds, (g) CI shape, (h) the parity ledger, (i) the aggregated risk register. When a sibling's §7 is silent or wrong, this plan records the gap in §8 and writes `TBD by {plan}` in the matrix; it never invents a sibling's test name.

## 2. Scope & parity definition

### 2.1 In scope

1. **Master test matrix** — every upstream JUnit test/file mapped to the owning plan's Rust test name, with status and substitute-oracle columns (§7a; machine-readable `parity/matrix.toml`).
2. **Golden parity artifacts** — the JVM one-off dump approach (`parity/java/*.java`), the Rust self-recorded harness goldens, screenshot oracles, generation/versioning/storage policy and CI drift gates (§6.2, §7a, M2).
3. **Determinism program** — seed + command-log replay → checksum equality across processes, worker counts, OSes and hosts (headless vs in-engine); `CHECKSUM_VERSION` registry and joint ownership with 05/06/12/13; desync injection with 21 (§3.3, §6.4, §7b, M1/M5).
4. **Scenario catalog** — the `{system}_{case}` naming convention, tiers, the master index of every scenario promised by plans 00–17 (plus `TBD by 18–22`), run-all/run-one commands (§7b).
5. **MCP scenario catalog** — every in-engine scenario promised by each plan, grouped by phase, with shared harness rules (launch flow, pid stamps, fixed camera poses, screenshot naming, inspector assertions) and the repo playtest skill created by plan 00 (§7c).
6. **Performance program** — `mind-headless bench` suite, the collected per-system budgets, baseline machine, regression gates, alloc/memory checks, soak tests (§7d).
7. **CI** — local scripts + GitHub Actions shape, caching, artifacts, drift jobs, optional MCP smoke (§3.6, §6.8, M0/M6).
8. **Parity ledger** — per-system completion checklists pulled from the plans, the definition of "parity complete" per system, tracked deviations (§7e, §8.3).
9. **Risk register aggregation** — one numbered table merging every plan's `NEEDS USER DECISION` and open risks with owner and current default (§8.1/§8.2).

### 2.2 Done means

- `parity matrix check` is green: every upstream test in `Mindustry/tests/src/test/java/**` has exactly one primary owner row (or an explicit `no-equivalent` row with a substitute oracle); every row's `rust` name either resolves via `cargo test -- --list` or carries `status = "planned-ignored"` with an owning plan/milestone.
- `parity run --suite smoke` (T0) and `--suite gate` (T1) pass with committed goldens; every phase gate P0–P8 has a committed `parity/reports/gate_Pn.json` with `pass: true`.
- Determinism holds: 2 in-process + 2 fresh-process runs, `--workers 1` vs `--workers 4`, Windows vs Linux CI, and the in-engine MCP checksum check all agree for every registered deterministic scenario.
- `parity bench-gate` passes on the baseline machine: no canonical tick regression > 10%, no absolute budget exceeded, zero steady-state allocations.
- Soak profiles (`mid` 60 min, `stress` 10 min, `multiplayer` 30 min when 21 lands) pass with bounded memory.
- The parity ledger (§7e) lists every system as `complete | deviation | deferred(owner)`; every `deviation` matches an entry in §8.3.

### 2.3 Deliberate deviations

This plan intentionally adds no game behavior. Its only "deviations" are policy fixes for conflicts it found between siblings (all recorded in §8.2 with defaults). No deviation from `HIGH_LEVEL_PLAN.md` §9 is introduced.

### 2.4 Deferred (explicit ownership)

| Item | Owner |
|---|---|
| Per-system tests, scenarios, budgets, milestones | plans 00–22 (their §7/§7e) |
| The `mind-headless` runner and scenario format internals | 00 (`{system}_{case}` contract); extended here read-only |
| `CHECKSUM_VERSION` constant and `ChecksumPart` implementation | 05 (joint bumps with 06/12/13) |
| Desync detector and correction | 21 |
| MCP addon (`open_godot_mcp`) and the repo playtest skill | 00 M7 |
| JVM golden dump source files | maintained here; provenance of each golden owned by its plan |
| Nightly GPU/self-hosted runners | user/infra (see NUD-40) |

## 3. Target design

### 3.1 Repository additions (this plan's files)

```
mindustry-godot/
  parity/                                # NEW — this plan owns
    README.md                            # how to add a matrix row / golden / scenario
    upstream.lock                        # { mindustry_commit, jdk, dump_args } for JVM goldens
    matrix.toml                          # master test matrix (§6.1), checked in
    checksum_registry.json               # CHECKSUM_VERSION + contributors (§6.4)
    mcp_catalog.json                     # MCP scenario catalog (§6.6)
    soak.toml                            # soak profiles (§6.7)
    java/                                # one-off JVM dump sources (never linked into the port)
      DumpContent.java  DumpLogicIO.java  DumpWaves.java
      DumpUi.java  DumpJsonIO.java  DumpFx.java
      README.md
    golden/                              # committed oracle artifacts (JVM-derived)
      manifest.json
      content/  logic/  waves/  ui/  io/  fx/  assets/
    screenshots/
      baseline/                          # fixed-pose MCP oracles (PNG + .meta.json)
    reports/                             # generated; gitignored
  bench/
    baselines.json                       # committed baseline + budgets (05 declares 23 owns it)
  client/rust/mind-headless/src/parity/  # subcommands: matrix, checksums, gate, soak, desync, bench_gate, report
  client/rust/mind-core/tests/
    golden/<system>/...                  # Rust self-recorded harness goldens (per plan)
    fixtures/legacy/{77,85,108,114,152,152_be}.msav   # copied at 04
  tools/
    parity.sh  parity.ps1                # suite driver (smoke/gate/full/soak)
    regen_goldens.sh  regen_goldens.ps1  # JVM golden regeneration (opt-in, JDK 17)
  .github/workflows/
    ci.yml                               # extended with parity jobs
    parity-nightly.yml                   # full catalog + bench + soak + mcp (schedule/dispatch)
  .opencode/skills/playtest/SKILL.md     # created by plan 00; this plan feeds the catalog + naming rules
```

### 3.2 Harness contracts

**Scenario naming.** Every `mind-headless` scenario is a Rust-registered fixture named `{system}_{case}` (HLP §7.1), `system` = the owning plan's domain (`spine`, `content`, `assets`, `io`, `stdb`, `sim_core`, `world`, `blocks`, `logistics`, `power`, `liquid`, `heat`, `combat`, `units`, `campaign`, `logic`, `ui`, `input`, `render`, `fx`, `parity`). Where a plan published a command with spaces (`io roundtrip`, `world gen`, `trace order`), the registered name is the underscore form (`io_roundtrip`, `world_gen`, `sim_core_schedule_order`); the catalog records both. `parity::scenarios::tests::all_registered_names_follow_system_case` enforces this.

**Tiers.** T0 smoke ≤ 2 min (one representative scenario per landed system); T1 gate ≤ 20 min (all acceptance + determinism scenarios for landed systems); T2 nightly ≤ 3 h (full catalog, all golden programs, bench suite); T3 soak (hours). Every scenario declares its tier in the registry; `mind-headless list --tier T1 --json` is the source of truth.

**Run-all/run-one.** `mind-headless run <scenario>` (one), `mind-headless run-all [--tier T0|T1|T2]`, `mind-headless parity run --suite smoke|gate|full --json out/report.json`. Exit codes 0 pass / 1 assertion or golden mismatch / 2 usage-IO, matching plan 00 §3.5.

**Result JSON (`format: 1`).**

```json
{ "format": 1, "suite": "smoke", "tier": "T0",
  "results": [ { "scenario": "spine_place_break", "plan": "00", "phase": "P0",
                 "status": "pass", "checksum": "…", "golden_match": true,
                 "duration_ms": 12, "error": null } ],
  "counts": { "pass": 1, "fail": 0, "skipped": 0 }, "pass": true }
```

### 3.3 Determinism architecture

```
scenario/seed ─► Sim (mind-core) ─► command-log .jsonl (tick, op)
      │                                  │
      ▼                                  ▼
per-tick CHECKSUM_VERSION stream ──► checksums.json ──► compare across:
  run×2 (same process) · process×2 · workers 1 vs 4 · Windows vs Linux
  · release (canonical) · headless vs in-engine (MCP SimHost)
```

- **Canonical unit** is `mind_core::checksum::Checksum` (owner 05): versioned (`CHECKSUM_VERSION`), fixed contributor order, `ChecksumPart` per component. Plan 00's `Sim::checksum()` (`DUMP_FORMAT` byte stream, xxh3-64) exists only until plan 05 lands; at 05 M8 it is re-implemented on the canonical unit and **all P0 goldens are re-recorded once in the same commit**. From that commit on, any contributor change is a breaking change handled by the registry (§6.4).
- **Contributors** (each owns its `checksum_part`, joint version bumps): 05 `GameState`/clock/RNG/group iteration; 06 `WorldGrid`/tiles (replaces the 05 placeholder); 12 `Rules` + team/campaign scalars; 13 `RngStream::Logic` (reserved until the stream exists); 02 content-ID table hash is included as a header field (build/content identity, not per-tick state); 16/17 render/FX state is **excluded** (view-only, HLP §2.4).
- **Replay semantics.** `mind-headless replay file.jsonl --through-tick N --checksum-every 60 [--workers n]` applies commands at the tick they are stamped with, before that tick executes, stable by file order for equal ticks (plan 00 §6.1/§6.2) — identical to plan 21's ordered application. The command log is append-only; variants are append-only.
- **Headless ↔ windowed equality.** Every deterministic scenario that the MCP catalog loads asserts `SimHost.get_checksum()` equals the scenario's committed golden after the same number of `step()` calls (plan 00 §7c step 4 precedent). `parity mcp-parity --suite T0` automates the subset.
- **Desync injection (21).** `parity desync-inject --case <case>` feeds a deliberately corrupted stream to the plan-21 detector (or to a detector trait double before 21 lands): reserved cases `wrong_checksum`, `reorder`, `duplicate`, `gap`, `skipped_command`, `content_hash_mismatch`, `late_command`, `speedhack_tick_stamp`. Each case names the expected detector outcome; plan 21 owns the detector, this plan owns the cases and the report.

### 3.4 Golden artifact architecture

Two categories, one manifest:

| Category | Where | Who records | Verified by |
|---|---|---|---|
| **Oracle goldens** (JVM-derived: content IDs/names, mlog field order, wave tables, UI bytes, JsonIO rules, Arc vectors) | `parity/golden/<domain>/` | `parity/java/*.java`, run manually with JDK 17 against `parity/upstream.lock` | `parity goldens verify` (sha256 vs manifest) + optional `MIND_JAVA_PARITY=1` re-dump diff |
| **Harness goldens** (Rust self-recorded: checksums, dumps, render lists, entitymeta, FX programs, bench numbers) | `client/rust/mind-core/tests/golden/<system>/` and `bench/baselines.json` | the owning plan's scenario at first accepted run | `parity run --suite gate` byte-diff + `parity goldens verify` |

Policy: goldens are committed; a golden change is a **deliberate PR** that updates the manifest hash and names the plan changelog entry. CI drift fails `golden-drift`. JVM goldens never require a JVM in CI (frozen files); regeneration is local and opt-in (aggregated NUD-10).

### 3.5 MCP harness contract

**Node-path authority.** Plan 00 §3.5 is authoritative: node root `/root/Spine`, nodes `/root/Spine/SimHost`, `/root/Spine/World/TileGrid`, `/root/Spine/World/Camera2D`, `/root/Spine/Ui/StateInspector/Label`; autoloads `/root/StdbConnector`, `/root/MindUi`, `/root/MindIo`, `/root/Spine/Input`, `/root/Spine/MindUnits`, `/root/Spine/MindCampaign`, `/root/Spine/MindRender`, `/root/Spine/MindFx`. Plans that wrote `/root/Main/*` or `res://scenes/Main.tscn` must be corrected (R-01). New autoload/class names are registered in the plan-00 extension contract before use.

**Launch flow.** `godot_health check` → if `BRIDGE_NOT_CONNECTED`, launch the editor per the playtest skill → `godot_instance list` → `godot_editor_edit open_scene res://scenes/spine.tscn` → `godot_game play` with `scene` passed explicitly. Multiplayer instances use `--pN` launch args; calls to multiple instances are sequential.

**Pid stamps.** Every `godot_exec` eval returns `"pid": OS.get_process_id()`; compare against `godot_game instances`; re-establish assumed state (camera, pause, loaded scenario) if the pid changes.

**Fixed camera poses.** Use `MindCamera2D.center_on_tile(tx,ty)` + zoom, or plan 16's `MindRender.set_camera_pose(tx,ty,zoom)`; poses are recorded in the catalog entry so screenshots are comparable. Screenshots are captured with `godot_screenshot game` (or `region`) and saved under `parity/screenshots/<plan>_<scenario>_<step>.png`; every capture is non-blank-checked and (where a baseline exists) diffed at the same pose only.

**State assertions.** Prefer `godot_exec call` on typed APIs plus `get_state_json()` / `get_checksum()` over pixel assertions; `godot_runtime_state inspect/watch` for live values; `godot_log errors` must be empty at teardown; toggled flags are restored.

**Catalog.** `parity/mcp_catalog.json` (§6.6) lists every promised MCP scenario with phase, plan, concrete steps, expected assertions and screenshot names. `tools/mcp-smoke.sh --suite T0|<phase>` automates a subset; `parity mcp-parity` supplements in-engine checksums. The repo skill (`mindustry-godot/.opencode/skills/playtest/SKILL.md`, plan 00 M7) is the human-readable companion; this plan keeps the catalog in sync with it.

### 3.6 CI architecture

| Layer | Trigger | Jobs | Gate |
|---|---|---|---|
| Local `tools/ci.sh`/`.ps1` | dev/pre-commit | fmt, clippy, check, `cargo test -p mind-core`, boundary greps, `parity matrix check`, T0 headless suite, Godot `--headless --editor --quit`, `cargo check -p mind-stdb`, `cargo check --manifest-path server/spacetimedb/Cargo.toml --tests` | blocks commit |
| `ci.yml` | push/PR | `rust-lint` (ubuntu: fmt/clippy), `rust-test` (windows+ubuntu: check+test), `determinism-compare` (downloads both OS checksum artifacts, diffs), `headless-golden` (T1 + `parity goldens verify`), `godot-import` (pinned 4.7.2 standard, `--headless --import --path client`, parse check), `spacetime-check`, `bindings-drift`, `matrix-check` | blocks merge |
| `parity-nightly.yml` | schedule + `workflow_dispatch` | full T2 catalog, `bench-gate` on self-hosted `perf` runner, soak profiles, `mcp-smoke` on self-hosted `windows-gpu`, `golden-drift`, screenshot diff report | blocks phase gate |
| Phase gate | manual command | `parity gate --phase Pn` (T1 + phase MCP subset + budget gate + ledger check) | blocks next phase (D9) |

Caching: `Swatinem/rust-cache` keyed on `client/rust` and `server/spacetimedb` (separate keys); cached Godot 4.7.2 archive keyed by version; cached `client/.godot/` import tree keyed on `project.godot` + assets. Artifacts: cargo test logs, `out/checksums-<os>.json`, `parity/reports/*.json`, screenshots, bench JSON. MCP smoke is **not** on shared CI (GPU/editor required); it runs locally (WSLg) or on the self-hosted `windows-gpu` runner.

### 3.7 Parity ledger, phase gates and completion definitions

The ledger (§7e) is a table: `System | Owner plan | Upstream tests | Rust tests | Scenarios | Goldens | Budgets | MCP | Status | Evidence`. Status is `not-started | in-progress | parity-complete | deviation | deferred(owner)`. **Parity complete** per system means: every §7a row of the owning plan is implemented and green (or explicitly `#[ignore]` with an owner and a re-enable milestone); every promised scenario and MCP scenario exists and passes; every golden is committed and verified; the owning plan's §7d budgets are recorded and the phase gate passed; the owning plan's §7e checklist is fully ticked. This plan re-derives the table and fails `parity gate` if a row for the phase is `not-started`.

**Gate protocol.** `mind-headless parity gate --phase Pn [--json out/parity/reports/gate_Pn.json]`:
1. matrix check for rows whose `phase ≤ Pn`;
2. T1 scenario suite for plans with `phase ≤ Pn`;
3. `parity checksums --suite gate` (same process, fresh process, workers 1/4);
4. `parity mcp-parity` for the phase's MCP subset (windowed runner);
5. `parity bench-gate` (canonical tick + phase budgets) on the baseline machine;
6. `parity goldens verify`;
7. ledger check (no `not-started` row for `phase ≤ Pn`).

A failed step fails the gate; the report path is required evidence in the next phase's kickoff.

## 4. Port map

The upstream `tests/` module is JUnit infrastructure; this plan ports its **semantics**, not its code.

| Mindustry tests artifact | Port / harness equivalent | Notes |
|---|---|---|
| `tests/build.gradle` (`forkEvery = 1`, workingDir `core/assets`, `showStandardStreams`, heap dump) | Rust test isolation by construction: `Sim` has no global statics; `cargo test` process model; temp data dir per test | `forkEvery=1` exposes the same static-leak hazard; `mind-core` is written static-free (05). |
| `ApplicationTests.launchApplication(boolean clear)` full-app bootstrap | `mind_headless::bootstrap::launch(LaunchOptions)` + `TestRig` (05) | Same order: `content.create_base_content()` → mods/scripts stub → `create_mod_content()` → `content.init()`; throws on content errors. |
| `ApplicationTests.testDataFolder` (`tests/build/test_data`) | `target/parity-data/<test>/` temp dir (cleaned per run), `--data-dir` override | Never touches the user's real data dir (`~/.local/share/Mindustry-Godot/`). |
| `Time.setDeltaProvider(() -> 1f)`; `Time.delta = 0.5f` (`PowerTestFixture`) | fixed `delta = 1.0` from `FixedStepRunner`; graph APIs take an explicit `delta` arg (09) | Deviation: Java power tests used 0.5; expected numbers are Rust-golden (09 R6). |
| `@ParameterizedTest`/`@TestFactory` dynamic cases | `#[test]` with an in-test case table (`for case in CASES`), case description in the panic message | No new test-framework dependency; case counts preserved in the matrix. |
| `@BeforeEach resetWorld()` (`logic.reset()`, `state.set(menu)`) | `TestRig::reset()`; `Sim::reset()` semantics from 05 | Data/patcher tests use `TestRig::reset_content()` instead (02/20). |
| `PowerTestFixture` (`createFakeTile`, fake blocks, `System.nanoTime()` names) | `mind_core::fixtures::power::{PowerHarness, FakeProducer, FakeBattery, FakeDirectConsumer, create_tile}` (05 seed, 09 full) | Deterministic counter for names. |
| `.msav` resources (`77,85,108,114,152,152_be`) | `client/rust/mind-core/tests/fixtures/legacy/*.msav` (copied verbatim; provenance in `parity/golden/assets/`) | Loaded only under `msav-import` (HLP OD2); no writer. |
| `tests/AGENTS.md` conventions (headless, no rendering, fake time, fork isolation, cleanup) | this plan's §3.2/§3.5 rules + `parity/README.md` | The single "how to write a ported test" doc; per-plan test notes stay in the plans. |
| JUnit `Assertions.*` | `assert!`/`assert_eq!` + `pretty_assertions` where helpful (dev-dep only) | Test deps never enter `mind-core`'s runtime tree. |

## 5. Milestones & task breakdown

Continuous plan: milestones overlap sibling execution; M0 may start once plan 00 M3 is green, and later milestones attach to the earliest plan that needs them. Each milestone ends with evidence in this file's Changelog. Smallest vertical slice first: **M0 → M1** (ingest the plan-00 scenarios into a matrix/catalog, prove determinism across processes).

### M0 — Program bootstrap (after plan 00 M3)

- [x] Create the `parity/` tree; `parity/README.md`; `parity/upstream.lock` (`mindustry_commit` = `2cd7aeecf1378b3db456be9bfde8691b3cdc1bcc`, JDK 17.0.20).
- [x] Ingest `parity/matrix.toml` from plans 00–22 §7a tables (this file's §7a is the rendered view); every row has `phase`, `owner`, `rust`, `status`, `oracle`. Landed rows carry the **actual** resolving `rust` name; the plan's §7a name is preserved in `oracle` when it differed.
- [x] Ingest `parity/mcp_catalog.json` from plans 00–22 §7c (this file's §7c is the rendered view).
- [x] Add `parity check`, `parity matrix`, `parity registry`, `parity goldens`, `parity budgets`, `parity scenarios`, `parity mcp`, `parity soak`, `parity report`, `parity gate`, `parity bench-gate` (`mind-headless parity …`, plan §3.7). `list --json` / `run-all` / catalog `tier`+`plan` fields landed at the F17 lane (below); `parity run --suite` remains.
- [x] Scenario registry (`parity/scenario_catalog.json`) carries `tier` + `plan` for every entry; `mind-headless list [--json] [--tier T0|T1|T2]` and `run-all [--tier T]` landed (`list --json` is the tier source of truth; `run-all` runs the file-backed tier). `parity run --suite smoke|gate|full` remains outstanding (embedded reporters print; a clean suite JSON needs the nightly runner / child-process capture).
- [x] `tools/ci.sh` gains the parity block (`parity check --tests`, `parity goldens`, `run-all --tier T0`) — **applied by the orchestrator 2026-10-03 (F20 post-join)**; `ci.yml` `matrix-check` job remains orchestrator-owned/proposed in §7f (not applied; `.github` unverifiable locally).
- **Verify:** `cargo run -p mind-headless -- parity check --tests out/tests.txt` exit 0 (matrix **100 rows, 91 landed resolved / 9 no-equivalent**); `parity report` roll-up green; `list --tier T0 --json` (18) and `run-all --tier T0` (4/4) green.

### M1 — Determinism harness

- [ ] `parity checksums --suite sm` runs each scenario twice in-process + twice via `--replay`; emits `checksums.json` (`format: 1`).
- [ ] `--workers 1` vs `--workers 4` compare flag; cross-process comparison in the same command (spawns a child `mind-headless`).
- [ ] `parity/checksum_registry.json` + `parity::checksum::tests::{registry_matches_core, golden_versions_equal}`; publish the joint-bump rule to 05/06/12/13.
- [ ] `ci.yml` `determinism-compare` job: windows+ubuntu upload `checksums-<os>.json`, a third job diffs them.
- [ ] Replay fuzz: `parity replay-fuzz --seed N --mutations reorder,dup,truncate` proves ordering/dedup/gap semantics against the plan-05 `CommandLog` contract.
- **Verify:** `spine_determinism` identical in 4 runs + 2 workers + 2 OSes; injecting a reorder changes the checksum (negative control); `checksum_registry` test green.

### M2 — Golden artifact program

- [ ] Write `parity/java/DumpContent.java`, `DumpLogicIO.java`, `DumpWaves.java`, `DumpUi.java`, `DumpJsonIO.java`, `DumpFx.java` against the upstream checkout; capture `parity/upstream.lock` (commit + JDK + args); commit outputs + `golden/manifest.json`.
- [ ] `tools/regen_goldens.sh --check` verifies sha256 vs manifest; `--only <id>` regenerates.
- [ ] CI `golden-drift` job validates manifest hashes (no JVM); `MIND_JAVA_PARITY=1` opt-in job re-dumps when a local checkout is present.
- [ ] Owner handshakes: 02 (`content`), 11 (`waves`), 13 (`logic`), 14 (`ui`), 04 (`io`), 17 (`fx`), 03 (`assets` region list); each plan's `field_order_matches_golden`-style test reads the committed file.
- **Verify:** `parity goldens verify` exit 0; each owning plan's test green against the committed golden; `regen_goldens.sh --check` green.

### M3 — Scenario catalog complete for landed plans; gate command

- [x] Every landed plan's scenarios registered with tier/plan metadata (`scenario_catalog.json`, 107 entries; file entries cross-checked by `parity::scenario::tests::every_registered_file_scenario_is_catalogued`); `parity::scenario::tests::committed_catalog_is_consistent` green.
- [~] `parity gate --phase Pn` implemented (seven steps + report JSON, runnable steps green, T1/checksum/MCP/bench steps reported `deferred` per NUD-40/A); `parity run --suite gate` outstanding.
- [ ] Scenario mirror check: `tools/sync_scenarios` output matches repo `scenarios/` (CI asserts).
- **Verify:** `parity gate --phase P0` passes and writes `reports/gate_P0.json`; a deliberately broken golden fails the gate with exit 1.

### M4 — Performance program

- [ ] `parity bench-gate --baseline bench/baselines.json --canonical --json`; `bench` subcommand grows `--profile`, `--assert-alloc`, `--checksum`.
- [ ] Record the baseline on the baseline machine (first run with plan 05's `sim_core mid`); commit `bench/baselines.json` including every plan's budget rows (§7d).
- [ ] `parity::budgets::tests::{every_plan_has_budget_rows, baseline_covers_all_bench_profiles}`; nightly `bench-gate` job.
- **Verify:** gate reports no >10% regression and all absolute budgets recorded; deliberately slowing a stub system fails the gate.

### M5 — Soak + desync

- [ ] `parity soak --profile mid|stress [--minutes N]` with RSS/alloc/checksum tracking; commit `parity/soak.toml`.
- [ ] `parity desync-inject` cases (detector double until 21 lands; then real).
- [ ] Windowed soak via MCP (`parity mcp-soak --minutes 30`, T0 script, no leak, clean logs).
- **Verify:** 60-min mid soak passes; every injection case yields the documented detector outcome; windowed soak log clean.

### M6 — MCP program + nightly

- [ ] `parity mcp-parity` (headless-checksum vs in-engine), `tools/mcp-smoke.sh --suite T0|<phase>`; screenshot baselines for plan-16/17 oracles under `parity/screenshots/baseline/`.
- [ ] `parity-nightly.yml` (full catalog + bench + soak + mcp + golden-drift); `tools/parity.sh --suite` mirrors it locally.
- [ ] Feed the repo playtest skill with the catalog table + naming/pid/pose rules.
- **Verify:** nightly green on the self-hosted runners; a stale screenshot baseline fails the diff step; skill contains every catalog scenario.

### M7 — Phase-gate integration and continuous maintenance

- [ ] Run `parity gate` for P0…P8 as phases land; commit each report; update §7e ledger after each gate.
- [ ] On every new sibling plan: add its §7a rows to the matrix, its §7b scenarios to the catalog, its §7c steps to the MCP catalog, its §7d budgets to the baseline, its §8 rows to §8.1/§8.2 — inside the same PR that adds the plan file (review checklist in `parity/README.md`).
- [ ] Final: `parity report --all --json` gives the whole-program pass/fail with per-system status; archive in `parity/reports/`.
- **Verify:** all P0–P8 gate reports `pass: true`; ledger has no `not-started`; risk register current.

## 6. Data & formats

### 6.1 `parity/matrix.toml` (`format: 1`)

```toml
format = 1
# one row per (upstream test, owner) pair; an upstream test may have several
# subcase rows (different owners) — exactly one has primary = true.
[[row]]
upstream = "ApplicationTests.sorterOutputCorrect"
owner    = "08_LOGISTICS_IMPLEMENTATION_PLAN.md"
phase    = "P3"
rust     = "world::blocks::distribution::sorter::tests::sorter_output_correct"
primary  = true
status   = "planned"            # planned | planned-ignored | no-equivalent | tbd
oracle   = "exact vault delivery counts on the Java tile layout; golden dump"
[[row]]
upstream = "ApplicationTests.testSectorValidity"
owner    = "12_CAMPAIGN_IMPLEMENTATION_PLAN.md"
phase    = "P4"
rust     = "campaign::tests::presets_rule_validity"
primary  = true
status   = "planned"
oracle   = "rule-validity assertions; generator/placement joint with 06/11"
```

Rules: `status = "tbd"` requires `tbd_by = "<plan file>"`; `rust` names resolve via `cargo test -- --list` when `status = "planned"` and the owning plan is marked landed, otherwise the row is skipped with a warning. Matrix check fails on: missing upstream test, duplicate `primary`, unknown plan filename, `tbd` without `tbd_by`, or `status = "planned"` for a plan whose header says "not started" but the name does not resolve (stale name).

### 6.2 `parity/golden/manifest.json` (`format: 1`)

```json
{ "format": 1,
  "upstream": { "repo": "https://github.com/Anuken/Mindustry", "commit": "<40-hex>" },
  "goldens": [
    { "id": "content_ids", "kind": "oracle",
      "path": "parity/golden/content/golden_content.json",
      "owner_plan": "02", "sha256": "<64-hex>",
      "regen": "tools/regen_goldens.sh --only content_ids" },
    { "id": "harness_campaign_play", "kind": "harness",
      "path": "client/rust/mind-headless/tests/golden/campaign/play.json",
      "owner_plan": "12", "sha256": "<64-hex>" }
  ] }
```

Rules: every oracle golden has an entry (`kind = "oracle"`); every committed `scenarios/*.json` has an entry (`kind = "scenario"`); every Rust self-recorded golden under `client/rust/*/tests/golden(s)/**` is pinned with `kind = "harness"` for sha256 drift **in addition to** being verified by its owning scenario. `sha256` is verified by `parity goldens verify`; `upstream.commit` is the checkout the JVM ran against (harness goldens inherit it). A golden edit must touch both the file and its manifest hash in one commit.

### 6.3 `bench/baselines.json` (`format: 1`)

```json
{ "format": 1,
  "machine": { "id": "dev-wsl-amd860m", "os": "Ubuntu 26.04 in WSL2 (Windows 11 host, WSLg)", "cpu": "…",
               "gpu": "llvmpipe (software Vulkan; GPU budgets use dev-win-amd860m on Windows)",
               "rust": "1.98.1", "godot": "4.7.2.stable", "profile": "release" },
  "canonical": { "scenario": "sim_core_mid", "metric": "tick_p95_ms",
                 "value": 0.0, "budget": 4.0, "warn_pct": 5, "fail_pct": 10 },
  "entries": [
    { "id": "core_sim_mid_p99", "plan": "05", "measure": "mind-headless bench sim_core --profile mid --ticks 3600 --json",
      "metric": "p99_ms", "value": 0.0, "budget": 4.0 },
    { "id": "combat_bullets_p95", "plan": "10", "measure": "mind-headless bench combat --bullets 2000 --turrets 400 --ticks 3600 --json",
      "metric": "p95_ms", "value": 0.0, "budget": 1.2 }
  ],
  "alloc": { "assert_zero_profiles": ["sim_core", "blocks", "logistics", "power", "combat", "units", "campaign", "ui"] },
  "soak": { "mid": { "minutes": 60, "max_rss_growth_pct": 5, "max_alloc_delta": 0 },
            "stress": { "minutes": 10, "max_rss_growth_pct": 5, "max_alloc_delta": 0 } } }
```

Rules: `value` is filled only by a run on the baseline machine (or the self-hosted `perf` runner with the same `machine.id`); CI runners record, never gate. `canonical` is the one metric that blocks phase gates on >10% regression (assignment default; ratified in NUD-39); all other entries gate against their absolute `budget` plus plan 00's ±20% warn / +50% fail until the absolute budget is met.

### 6.4 `parity/checksum_registry.json` (`format: 1`) and joint ownership

```json
{ "format": 1, "checksum_version": 1, "algorithm": "fnv1a64",
  "owner": "05_SIM_CORE_IMPLEMENTATION_PLAN.md",
  "contributors": [
    { "id": "game_state", "owner": "05", "status": "active" },
    { "id": "clock_rng", "owner": "05", "status": "active" },
    { "id": "groups", "owner": "05", "status": "active" },
    { "id": "world_grid", "owner": "06", "status": "planned", "note": "replaces 05 placeholder" },
    { "id": "rules", "owner": "12", "status": "planned", "note": "Rules::checksum_part" },
    { "id": "logic_rng", "owner": "13", "status": "planned", "note": "reserved stream" },
    { "id": "content_hash", "owner": "02", "status": "header" }
  ],
  "excluded": [ { "id": "render", "owner": "16", "reason": "view-only" },
                { "id": "fx", "owner": "17", "reason": "view-only" } ] }
```

Rules: `checksum_version` mirrors `mind_core::checksum::CHECKSUM_VERSION`; `parity::checksum::tests::registry_matches_core` fails if they differ. Any contributor change bumps the version and re-records every golden in the same commit (plan 00 §6.4 invariant). Plan 21 compares versions before judging a desync (mismatched version = `IncompatibleBuild`, not desync).

### 6.5 Suite/report JSON

`parity run`, `gate`, `checksums`, `soak`, `desync-inject` all emit one `format: 1` JSON documented in §3.2 plus per-suite payloads:

```json
{ "checksums": { "spine_determinism": { "seed": 2, "ticks": 600, "checksum_version": 1,
    "final": "…", "every60": ["…"], "runs": 4, "workers": [1, 4], "match": true } } }
```

Exit codes: 0 all pass; 1 assertion/golden/regression mismatch; 2 usage/IO. `reports/` is gitignored except the committed gate reports (`parity/reports/gate_Pn.json`, force-added) — the phase-gate evidence of record.

### 6.6 `parity/mcp_catalog.json` (`format: 1`)

```json
{ "format": 1,
  "entries": [
    { "id": "mcp_spine_place_break", "phase": "P0", "plan": "00",
      "scene": "res://scenes/spine.tscn",
      "scenario": "spine_place_break",
      "steps": ["load_scenario", "set_paused true", "step 60", "eval checksum",
                "place_block 3 5 stone-wall", "break_block 3 5", "mouse place 7 7",
                "inspector label", "screenshot", "godot_log errors"],
      "asserts": ["checksum == golden", "tile (7,7) == stone-wall", "errors empty"],
      "screenshots": ["parity/screenshots/00_spine_place_break_01.png"] } ] }
```

Every entry names a real `scenario` (cross-checked by `parity matrix check`); `steps` reference either `godot_exec call` methods or the shared recipe vocabulary in `parity/README.md` so `tools/mcp-smoke.sh` can execute a subset mechanically.

### 6.7 `parity/soak.toml` (`format: 1`)

```toml
format = 1
[profiles.mid]
minutes = 60
world = 256
buildings = 600
units = 300
bullets = 3000
belt_items = 4000
checksum_every_ticks = 600
max_rss_growth_pct = 5
max_alloc_delta = 0
[profiles.stress]
minutes = 10
world = 512
buildings = 2000
units = 1000
bullets = 10000
checksum_every_ticks = 600
[profiles.multiplayer]           # plan 21, 2 headless clients + 1 windowed
minutes = 30
clients = 3
checksum_every_ticks = 600
desync_tolerance = 0
```

### 6.8 CI files and caching keys

| File | Purpose |
|---|---|
| `.github/workflows/ci.yml` | PR/push: lint, cross-OS test, determinism compare, T1 goldens, Godot import, STDB check, bindings drift, matrix check |
| `.github/workflows/parity-nightly.yml` | schedule 03:00 + dispatch: T2, bench, soak, MCP, golden drift |
| `tools/ci.sh` / `tools/ci.ps1` | local mirror of `ci.yml` (bash primary) |
| `tools/parity.sh` / `tools/parity.ps1` | `--suite smoke|gate|full|soak`, `--phase Pn`, `--mcp` |
| `tools/regen_goldens.sh` / `.ps1` | JVM golden regeneration, requires `MIND_JAVA_PARITY=1` and a checkout path |
| Cache keys | `rust-client-<os>-<Cargo.lock hash>`, `rust-stdb-<os>-<hash>`, `godot-4.7.2-standard-<os>`, `godot-import-<project.godot+assets hash>` |

## 7. Oracle & verification (REQUIRED)

### 7a. Ported tests — the master matrix

Rendered view of `parity/matrix.toml`; **status is "planned" for every row at authoring** (no sibling implementation is started). Primary owners are bold in prose only; the machine-readable file carries `primary`. Names are quoted exactly from the owning plan's §7; `TBD by {plan}` means the sibling did not publish a name (e.g. plan not on disk). Upstream method names from `ApplicationTests.java`, `DataAssetTests.java`, `PatcherTests.java`, `LogicTests.java`, `power/*.java`, `GenericModTest.java`, `ModTestAllure.java`.

**Table 7a-1 — `ApplicationTests.java` (owner plan in the `Rust test` cell).**

| Upstream test | Rust test (owner) |
|---|---|
| `writeStringTest` (parameterized: null/ASCII/CJK/emoji) | `io::typeio::tests::write_string_roundtrip` (04) |
| `writeRules` | `io::typeio::tests::write_rules_binary` (04); `game::rules::tests::typeio_roundtrip` (12) |
| `writeRules2` | `io::json::tests::rules_json_roundtrip` (04); `game::rules::tests::json_roundtrip_and_field_tolerance` (12) |
| `initialization` | `content::tests::spine_registry_non_empty` (00, P0 seed) → `content::tests::initialization` (02); `mind_core::tests::initialization` (05); `units::tests::unit_content_registered` (11) |
| `playMap` | `io::map::tests::create_map_reads_meta_only` (04); `tests::play_sets_playing_and_wavetime` (05); `world::tests::load_internal_map_ground_zero` (06); `game::play::tests::play_map_loads_and_sets_playing` (12) |
| `spawnWaves` | `tests::run_wave_fires_event` (05); `waves::tests::spawn_waves_headless` (11); `game::universe::tests::run_wave_campaign_difficulty` (12) |
| `createMap` | `world::tests::resize_and_fill` (00, P0 seed) → `world::tests::create_map_resize_fill` (06); `io::map::tests::create_map_reads_meta_only` (04) |
| `multiblock` | `world::tests::multiblock_linkage` (06); `world::block::tests::multiblock_building_identities` (07); `input::placement::tests::multiblock_line_suppression` (15) |
| `blockInventories` | `world::tests::multiblock_shared_modules` (06, world half); `world::modules::tests::item_module_arithmetic` (07); `storage::tests::item_module_totals` (08) |
| `timers` | `time::tests::timer_runs_after_two_updates` (00, P0) → `sim::time::tests::timers` (05) |
| `manyTimers` | `time::tests::many_timers_same_update` (00, P0) → `sim::time::tests::many_timers` (05) |
| `longTimers` | `time::tests::long_timers_catch_up` (00, P0) → `sim::time::tests::long_timers` (05) |
| `save` | `io::save::tests::save_writes_valid_file` (04); `world::tests::save_then_resize_clears_buildings` (06); `game::saves::tests::slot_save_load_checksum` (12) |
| `saveLoad` | `io::save::tests::save_then_load_preserves_unit_and_map` (04); `world::tests::save_load_preserves_world_size` (06); `game::saves::tests::slot_save_load_checksum` (12) |
| `liquidOutput` | `liquid::tests::transfer_flow_formula` (09) |
| `liquidJunctionOutput` | `liquid::tests::junction_destination_recursion` (09) |
| `liquidRouterOutputAll` | **TBD by 09** (no router test named in 09 §7a; `liquid_conduit_transfer` covers single-chain flow only) |
| `sorterOutputCorrect` | `world::blocks::distribution::sorter::tests::sorter_output_correct` (08) |
| `routerOutputAll` | `distribution::router::tests::router_output_all` (08) |
| `junctionOutputCorrect` | `distribution::junction::tests::junction_output_correct` (08) |
| `blockOverlapRemoved` | `world::tests::multiblock_overlap_removed` (06); `world::place::tests::multiblock_overlap_replaces_buildings` (07); `input::plan::tests::overlapping_plan_replaced` (15) |
| `conveyorCrash` | `distribution::conveyor::tests::conveyor_crash` (08) |
| `conveyorBench` | `distribution::conveyor::tests::conveyor_bench` (`#[ignore]` bench, 08) + `mind-headless bench logistics` |
| `load77Save` | `io::legacy::tests::load_77` (`msav-import` only, 04) |
| `load85Save` | `io::legacy::tests::load_85` (04) |
| `load108Save` | `io::legacy::tests::load_108` (04) |
| `load114Save` | `io::legacy::tests::load_114` (04) |
| `load152BESave` | `io::legacy::tests::load_152_be` (04) |
| `load152Save` | `io::legacy::tests::load_152` (04) |
| `arrayIterators` | `entities::groups::tests::iteration_no_alloc` (05); `world::tiles::tests::iteration_order_is_row_major` (06) |
| `inventoryDeposit` | `world::place::tests::crafter_accepts_deposit` (07, ignored until 08/09); `storage::tests::deposit_all_blocks` (08); `input::desktop::tests::deposit_cooldown_gate` (15) |
| `edges` | `world::edges::tests::edge_order` (06) |
| `buildingOverlap` | `world::construct::tests::builder_overlap_deterministic` (07); `world::tests::edge_tile_proxy_cleanup` (06); (15 shares placement table) |
| `buildingDestruction` | `world::construct::tests::builder_construct_then_deconstruct` (07); `combat::tests::entity_order_bullets_after_buildings` (10) |
| `allBlockTest` | `blocks::tests::all_metadata_valid` (02, metadata); `world::behavior::tests::all_blocks_update_without_panic` (07, update); `world::blocks::defense::turrets::tests::turret_metadata_valid` (10, turret metadata) |
| `checkPayloads` | `payloads::tests::all_payload_blocks_update_and_roundtrip` (08); `units::tests::payload_blocks_update_and_save` (11, with 08/04) — **duplicate ownership; primary 08** |
| `allPayloadBlockTest` | same rows as `checkPayloads` (08 primary; 11 partial) |
| `initBuilding` | `storage::core_block::tests::team_core_registration` (08); `game::teams::tests::register_core_and_inventory` (12) |
| `testSectorValidity` | `sectors::tests::presets_resolve` (02); `world::tests::sector_rules_hooks_called` (06); `waves::tests::sector_spawns_nonempty_to_boss` + `ai::tests::sector_indexer_no_sandbox_sources` (11); `campaign::tests::presets_rule_validity` (12) |

Counts: `ApplicationTests` = 37 annotated test methods (35 `@Test`, 1 `@ParameterizedTest`, 1 `@TestFactory`) plus helpers. `launchApplication(boolean)` is infrastructure (row in §4); `updateBlocks`, `depositTest`, `handleItem`, `resetWorld` are helpers. `checkPayloads()` and `initBuilding()` are unannotated helper methods invoked by `allPayloadBlockTest`/`depositTest`; they are tracked as sub-rows here because plans 08/11/12 cite them by name.

**Table 7a-2 — `DataAssetTests.java`, `PatcherTests.java`, `LogicTests.java`, `power/*.java`.**

| Upstream test | Rust test (owner) |
|---|---|
| `DataAssetTests.basicItem` | `parser_hooks::tests::provider_contract` (02, fake provider until 20) |
| `DataAssetTests.basicUnit` | `parser_hooks::tests::provider_contract` (02) + `combat::bullet::tests::laser_def_parses_fields` (10) |
| `DataAssetTests.noContentAddedWithError` | `parser_hooks::tests::provider_contract` (02); end-to-end `TBD by 20` |
| `DataAssetTests.noNullFieldsAllowed` | `parser_hooks::tests::provider_contract` (02); end-to-end `TBD by 20` |
| `PatcherTests.unitWeapons` / `uUnitWeaponReassign` / `addWeapon` / `ammoReassign` | `weapons::tests::patcher_kind_and_field_resolution` (10); end-to-end `TBD by 20` |
| `PatcherTests.unitFactoryPlans` / `reconstructorPlans` / `reconstructorPlansEditSpecific` / `reconstructorPlansAdd` | `blocks::tests::{factory_plan_requirements_scaled, reconstructor_upgrade_matrix}` (11, fixtures); end-to-end `TBD by 20` |
| `PatcherTests.specificArrayRequirements` (reset semantics) | `parser_hooks::tests::index_snapshot_restore` (02) |
| `PatcherTests.{consumeApply,unitAbilities,unitAbilitiesArray,unitTypeObject,unitFlagsArray,unitFlags,unitType,cannotPatch,assignStringToObject,gibberish,noIdAssign,unknownFieldWarn,objectFloatMap,attributes,singleValue,singleValue2,noResolution,setMultiAdd,indexAccess,nestedArrays,nestedArrays2,arrayMulti,customAttribute,bigPatch}` | `TBD by 20` (`mind_core::mods::*::tests` group; plan 20 not on disk). Negative cases (`cannotPatch`, `gibberish`, `noIdAssign`) are substitute-oracle rows: DataPatcher must reject + warn, never panic. |
| `LogicTests.parsesStringEscapes` | `logic::parser::tests::string_escapes` (13) |
| `LogicTests.plainNumberIsNotTreatedAsAString` | `logic::parser::tests::plain_number_not_a_string` (13) |
| `LogicTests.sanitizesInput` | `logic::statement::tests::sanitize` (13) |
| `LogicTests.sanitizedQuotedValuesRoundTripThroughTheParser` | `logic::statement::tests::sanitize_roundtrip` (13) |
| `LogicTests.parseVarValues` | `logic::value::tests::parse_var_values` (13) |
| `LogicTests.parseColorValues` | `logic::value::tests::parse_colors` (13) |
| `LogicTests.parseInvalidNumbers` | `logic::value::tests::parse_invalid_numbers` (13) |
| `LogicTests.unterminatedStringsThrow` | `logic::parser::tests::unterminated_strings_throw` (13) |
| `LogicTests.varWithProperlyQuotedEmptyString` | `logic::value::tests::empty_string_const` (13) |
| `LogicTests.quoteInVariableNameThrows` / `quoteAtEndOfVariableNameThrows` | `logic::parser::tests::quote_in_token_throws` (13) |
| `LogicTests.crlfAfterUnquotedTokenDoesNotCorruptTheToken` | `logic::parser::tests::crlf_token_identity` (13) |
| `LogicTests.crlfAfterQuotedStringParsesCleanly` | `logic::parser::tests::crlf_quoted_string` (13) |
| `LogicTests.loneCarriageReturnActsAsALineEnding` | `logic::parser::tests::lone_cr` (13) |
| `LogicTests.crlfLabelsResolveToTheSameJumpLocationAsLfLabels` | `logic::parser::tests::crlf_labels` (13) |
| `PowerTestFixture` (`initializeDependencies`, fake blocks/tile) | `fixtures::power::{PowerHarness, FakeProducer, FakeBattery, FakeDirectConsumer, create_tile}` (05 seed, 09 full) |
| `PowerTests.PowerGraphTests.simulateDirectConsumption` (7 dynamic cases) | `world::blocks::power::graph::tests::direct_consumer_satisfaction_is_as_expected` (09) |
| `PowerTests.PowerGraphTests.simulateDirectConsumptionWithBattery` (9 cases) | `battery_capacity_is_as_expected` (09) |
| `PowerTests.directConsumptionStopsWithNoPower` | `direct_consumption_stops_without_power` (09) |
| `DirectConsumerTests.noPowerRequestedWithNoItems` | `world::blocks::power::tests::direct_consumer::no_items_no_power_request` (09) |
| `DirectConsumerTests.noPowerRequestedWithInsufficientItems` | `...::insufficient_items_no_power_request` (09) |
| `DirectConsumerTests.powerRequestedWithSufficientItems` | `...::sufficient_items_power_request` (09) |
| `ConsumeGeneratorTests.simulateLiquidConsumption` (deltas 2/1/0.5 × 4 amounts) | `world::blocks::power::consume_generator::tests::liquid_input_deltas` (09) |
| `ConsumeGeneratorTests.simulateItemConsumption` (coal/blast/spore/pyratite) | `...::item_flammability_inputs` (09) |
| `ConsumeGeneratorTests.efficiencyRemainsConstantWithinItemDuration_ItemsOnly` | `...::efficiency_constant_within_item_duration` (09) |

**Table 7a-3 — network-gated mod tests.**

| Upstream test | Rust test (owner) |
|---|---|
| `ModTestAllure.begin` (downloads Allure over HTTP, spawns a mod unit) | `mods::tests::allure_unit_loads_and_spawns` (20, `#[ignore]`, `MIND_MOD_NET=1`); offline substitute: a checked-in minimal JSON mod fixture loaded by plan 20's `mods::tests::fixture_mod_loads` |
| `GenericModTest.grabMod/checkExistence` (helpers, no `@Test`) | helpers on 20's integration harness; same network gate |

**Table 7a-4 — upstream tests with no direct equivalent and their substitute oracle.**

| Area | Substitute oracle |
|---|---|
| Bullets/turrets/damage/fires/puddles/lightning/defense (no upstream tests; 10) | hand-computed math tests + `combat_*` headless scenarios + `parity/combat/duo_dummy_worksheet.json` + fixed-frame MCP screenshots at SSIM ≥ 0.995 |
| UI (no upstream tests; 14) | one-off `parity/java/DumpUi.java` goldens (`format_icons`, `format_time/amount`, `UiKey` ordinals, `NodeBuilder` wire hex) + `dialogs_manifest.json`/`styles_manifest.json` + `ui_sweep` screenshots + `MindTable` 20-case rect matrix |
| Input (no upstream tests; 15) | deterministic input-log replay `.events.jsonl` + `placement_validation_table` golden + MCP drag/click recipes |
| Render (no upstream tests; 16) | render-list goldens (`render_*`) + per-layer fixed-pose screenshot A/B (compositor skill rule) + cache-invalidation unit tests |
| FX/parts/trails/weather (no upstream tests; 17) | `parity/java/DumpFx.java` (`fx_order.txt`, Arc-Rand `fx_vectors.json`) + `fx audit` (267/267) + per-effect program goldens + fixed-frame screenshot tolerance |
| AI/pathfinding/waves (no upstream tests; 11) | `parity/java/DumpWaves.java` golden table + deterministic per-tick path/route dumps + `units_*` scenarios |
| mlog executor semantics (upstream only parses; 13) | new executor tests (`ops_table`, `budget_clamping`, `wait_and_stop`, `print_format`, `draw_pack_vectors`) + `logic_*` scenarios |
| Campaign runtime (upstream only bootstrap assertions; 12) | new `game::*` tests + `campaign_*` scenarios + golden turn JSON |
| Content metadata beyond `initialization` (02) | `DumpContent.java` golden IDs/names/counts + `content audit` + per-type ledgers |
| Assets (03) | golden region-name inventory + pack determinism sha256 + MCP region/icon/bundle probes |
| C# `sstdbsdk` semantics (01) | named behavior tests (01 §7.1) + 2-client MCP relay scenario; no upstream suite existed |
| Network/@Remote packets (21) | relay ordering tests + desync injection cases + multiplayer soak; no upstream unit suite exists |

**Table 7a-5 — this plan's own meta-tests (Rust, in `mind-headless`).**

| Test | Assertion |
|---|---|
| `parity::matrix::tests::{every_upstream_test_has_owner, every_owner_plan_exists, no_duplicate_primary_owner, tbd_rows_name_a_plan}` | matrix completeness (§6.1 rules) |
| `parity::scenarios::tests::{all_registered_names_follow_system_case, every_plan_scenario_present, scenario_has_golden_or_reason}` | catalog integrity |
| `parity::goldens::tests::{manifest_hash_matches, every_golden_has_upstream_commit, harness_golden_files_tracked}` | golden integrity |
| `parity::budgets::tests::{every_plan_has_budget_rows, baseline_covers_all_bench_profiles, canonical_metric_defined}` | budget coverage |
| `parity::checksum::tests::{registry_matches_core, golden_versions_equal, contributor_order_stable}` | joint checksum ownership |
| `parity::mcp::tests::{catalog_covers_every_plan_with_mcp, every_catalog_scenario_exists, screenshot_paths_unique}` | MCP catalog integrity |
| `parity::reports::tests::gate_report_schema_valid` | committed phase-gate evidence parses |

### 7b. Headless harness scenarios — master index

Naming/runner rules are §3.2/§3.5. Registered names are the underscore forms of the plans' commands; where a plan gave only a command form, the name shown is the normalized registration (marked `*`). Tiers are assigned by this plan at M3 (defaults shown; `T0` = smoke, `T1` = gate, `T2` = nightly). **One run:** `mind-headless run <name> [--dump <path>]`; **all:** `mind-headless run-all --tier T0|T1|T2`; **suite:** `parity run --suite smoke|gate|full`.

**Plan 00 (P0) — spine.**

| Scenario | Tier | Assertions |
|---|---|---|
| `spine_place_break` | T0 | golden checksum; sparse tiles `(5,4)=stone-wall`, `(4,4)=air`; dump schema |
| `spine_determinism` | T0/T1 | 200 seeded commands; 2+2 runs and replay identical |
| `spine_many_commands` | T1 | 10 000 ops < 1 s release; checksum stable |
| `bench_baseline` | T2 | p50/p99 vs committed baseline |

**Plan 01 (P1) — STDB.**

| Scenario | Tier | Assertions |
|---|---|---|
| `stdb_offline_boot` | T0 | `Offline` after N pumps; no socket; pump p99 < 0.2 ms |
| `stdb_binder_replay` | T1 | replay order; no duplicate live events; drop unregisters |
| `stdb_command_order` | T1 | order by `command_id`; dup ignored; per-sender gap flagged; auto-inc gap not loss |
| `stdb_relay_live` | T2 (opt-in, local STDB) | applied == sent; no order error |

**Plan 02 (P1) — content.**

| Scenario | Tier | Assertions |
|---|---|---|
| `content_load`* | T0 | base+fake mod init; per-type counts == golden |
| `content_dump`* (`content dump --out parity/out_content_ids.json`) | T1 | diff vs golden empty |
| `content_load_order_bad`* | T1 | fails with `RegistryEpoch` load-order error naming the missing `ItemId` |
| `content_audit`* | T1 (CI gate) | names/IDs/bundles/regions/tech/field-diff/dangling refs all clean |
| `content_bench`* | T2 | median base load ≤ 200 ms release |

**Plan 03 (P1) — assets.**

| Scenario | Tier | Assertions |
|---|---|---|
| `assets_migrate`* | T1 | migrated tree matches manifest; forbidden generated files absent |
| `assets_pack`* | T2 | every output hash present; `inputsHash` matches fresh staging |
| `assets_boot`* | T0 | pages/regions (`len ≥ 18000`), error region, bundle `en`, sound index; golden JSON |
| `assets_regions`* | T1 | every content region resolves; `fallback=error` miss fails |
| `assets_bundles`* | T1 | locale chain + `global.properties` merge; no key absent from English |
| `assets_sounds`* | T1 | registry == file tree; dense append-only ids; duplicates rejected |
| `assets_determinism`* | T2 | two full packs → identical sha256 per output |

**Plan 04 (P1) — IO.**

| Scenario | Tier | Assertions |
|---|---|---|
| `io_roundtrip`* | T1 | load → 600 ticks → save → reset → load; canonical checksum `C0 == C1` |
| `io_dump_meta`* | T1 | width/height/wave/build match save-time |
| `io_map_list`* | T1 | parallel listing; corrupt file skipped with warning |
| `io_settings`* | T1 | set/flush/reload persists; corrupt → defaults |
| `io_check_revisions`* | T1 | drift exits non-zero; `--update` appends `N.json`, never edits old |
| `bench_io_save`* | T2 | §7d budgets |
| `cargo test --features msav-import` | T2 (opt-in) | 77/85/108/114/152/152_be load; expected JSON matches |

**Plan 05 (P2) — sim core.**

| Scenario | Tier | Assertions |
|---|---|---|
| `sim_core_boot` | T0 | `playing`, `tick == 600`, group counts stable, golden checksum |
| `sim_core_determinism` | T1 | 61 checkpoints × (2 in-proc + 2 cross-proc) × workers 1/4 identical |
| `sim_core_schedule_order` (`trace order`) | T1 | exact set trace == golden incl. menu/paused/editor/`!client` skips |
| `sim_core_reset_play_cycle` | T1 | 20 cycles; no entity/group/time leak; alloc delta 0 after warmup |
| `meta_entities`* | T1 | stable JSON == golden; `classids.properties` parity |
| `bench_sim_core`* | T2 | empty/mid/stress §7d budgets |

**Plan 06 (P3) — world/terrain.**

| Scenario | Tier | Assertions |
|---|---|---|
| `world_gen`* | T1 | 2 in-proc + 1 fresh process identical; histogram golden; 250×250 no panic |
| `world_gen_erekir` / `world_gen_tantros` / `world_gen_asteroid`* | T2 | cross-process identical; planet-specific features present |
| `world_tile_ops` | T1 | 10k seeded ops; histogram; change counters == event counts; < 1 s release |
| `world_multiblock` | T1 | 3×3 shared entity; overlap clears; break → 9 air |
| `world_filters` | T1 | forward golden; forward != reverse; buffered/unbuffered semantics |
| `maps_list`* | T1 | sorted; corrupt skipped; tags/width/height correct |
| `world_load_roundtrip`* | T1 | save → reset → load tile checksum equal (extends `io_roundtrip`) |

**Plan 07 (P3) — blocks/build.**

| Scenario | Tier | Assertions |
|---|---|---|
| `blocks_place_construct_destroy` | T0 | construct → real → air transitions; event order; golden checksum |
| `blocks_multiblock_cover_clear` | T1 | 9 tiles share center; one End event; overlap removal |
| `blocks_consumer_efficiency` | T1 | exact progress/item counts at fixed ticks; fuel removal stops consumption |
| `blocks_config_roundtrip` | T1 | config survives save/load; checksum equal |
| `blocks_proximity_multiblock` | T1 | proximity lengths match `Edges`; `onProximityRemoved` effects |
| `blocks_limits_darkness` | T1 | `valid_place` false for each limit; indestructible `valid_break` false |
| `bench_blocks`* | T2 | §7d budgets |

**Plan 08 (P3) — logistics.**

| Scenario | Tier | Assertions |
|---|---|---|
| `logistics_smoke` | T0 | one source→conveyor→vault; exact per-tick belt motion; golden checksum |
| `logistics_conveyor_lane` | T1 | delivered ≈ formula ±2; monotonic positions; titanium 2.29× ±1% |
| `logistics_router_fairness` | T1 | every vault > 0; `max−min <= 1`; cycle order golden |
| `logistics_sorter_config` | T1 | exact port of `sorterOutputCorrect` layout |
| `logistics_bridge_latency` | T1 | first-arrival tick golden; phase transport = 2; link config round-trip |
| `logistics_mass_driver_roundtrip` | T1 | conservation sent − in-flight == received; pool empty |
| `logistics_payload_move` / `logistics_payload_load_unload` | T1 | `moveTime = 45` per tile; item conservation loader+payload+unloader |
| `logistics_core_inventory` | T1 | capacity unification; over-capacity rejection; save/load totals |
| `logistics_unloader_drain` | T1 | comparator order; no item loss |
| `bench_logistics`* | T2 | ≥ 20M updates/s; lane items/tick ±1%; 0 alloc |

**Plan 09 (P3) — power/liquids/heat.**

| Scenario | Tier | Assertions |
|---|---|---|
| `power_battery_cycle` | T0 | charge/drain trace == golden, monotone status |
| `power_graph_split_merge` | T1 | 2 graphs after break; satisfaction per side; 1 graph after re-place; updaters == graphs |
| `liquid_conduit_transfer` | T1 | transfer within ε; monotone tank; broken segment stops flow; flow cache ≈ expected |
| `heat_network_equilibrium` | T1 | conductor == producer after ramp; overheat formula; decay; nuclear coolant case |
| `power_network_determinism` | T1 | 61 checkpoints identical across processes and workers |
| `bench_power` / `bench_liquid` / `bench_heat`* | T2 | §7d budgets; alloc 0 |

**Plan 10 (P4) — combat.**

| Scenario | Tier | Assertions |
|---|---|---|
| `combat_basic` | T0 | exact death tile; HP reduced by `damage`; golden checksum |
| `combat_bullet_pierce_frag` | T1 | 3 walls in order; frag count seeded; pierce factor |
| `combat_turret_ammo` | T1 | exact shot cadence/accounting; liquid/power ammo rules |
| `combat_point_defense` | T1 | intercept after retarget; non-lethal subtraction; no friendly fire |
| `combat_shield_absorb` | T1 | buildup/break/regen; shield-wall split; explosion absorb |
| `combat_fire_puddle_tick` | T1 | spread schedule; damage ticks; water multiplier; cap/boil |
| `combat_determinism` | T1 | 61 checkpoints identical, workers 1/4 |
| `combat_dump` / `combat_trace`* | T1 | inspector feeds for MCP |
| `bench_combat`* | T2 | ≤ 1.2 ms p95 bullets+turrets; 0 alloc |

**Plan 11 (P4) — units/AI/waves.**

| Scenario | Tier | Assertions |
|---|---|---|
| `units_spawn_path_arrive` | T0 | arrival ≤ `hitSize`; tick budget; golden checksum |
| `units_flowfield_costs` | T1 | passability per kind; route dumps golden |
| `units_rts_command_queue` | T1 | queue drains; stances/formation match; relay command crosses |
| `units_waves_difficulty` | T1 | per-wave counts/boss/shields == `wave_generate.json` |
| `units_cargo_pickup_deliver` | T1 | items loader→unit→point; stale flips at 360; no loss |
| `units_factory_output` | T1 | unit after `plan.time` with command; save/load mid-build |
| `units_legs_ik` / `units_segment_chain` | T1 | clamps/no NaN; chain despawns on head death |
| `bench-path` / `bench-air`* | T2 | §7d budgets |

**Plan 12 (P4) — campaign.**

| Scenario | Tier | Assertions |
|---|---|---|
| `rules_roundtrip` | T1 | JSON→Rules→TypeIO→JSON equal; `RulesLoadEvent` once; checksum stable |
| `sector_save_load_turn` | T1 | save→reset→load checksum golden; 10 turns import/export golden |
| `schematic_place` | T1 | base64 prefix; loadout validated; rotate + point config |
| `tech_unlock_gating` | T1 | locked rejected; deps enforced; auto-unlocks; settings keys |
| `fog_reveal` | T1 | discovered != visible; edge clipping; custom chunk round-trip |
| `campaign_sector_cycle` | T1 | capture events; waves off; save written; lose path clears |
| `campaign_turn`* / `campaign_bench`* | T2 | turn means; §7d budgets |

**Plan 13 (P5) — logic/mlog.**

| Scenario | Tier | Assertions |
|---|---|---|
| `logic_arith` | T0 | final vars; golden checksum |
| `logic_strings` | T1 | text buffer + yield ticks exact |
| `logic_link_sensor` | T1 | memory/switch/link counts |
| `logic_draw` / `logic_draw_headless` | T1 | `DisplayCmd` vector golden / empty buffer parity |
| `logic_budget` | T1 | `5*ipt` obey; `@ipt` synced |
| `logic_save_load` | T1 | wait/vars/code/tag/transform preserved |
| `logic_unit_control_gating` | T1 | gating rules | 
| `logic_privileged_world` | T1 | side effects via dump; non-privileged noop |
| `logic_sync_event` / `logic_globals` / `logic_script_filter` | T1 | exact event cadence/global math/cap |
| `logic_assemble`* / `logic_bench`* | T2 | roundtrip bytes; §7d budgets |

**Plan 14 (P5) — UI.**

| Scenario | Tier | Assertions |
|---|---|---|
| `ui_text` | T0 | markup/icons/time/amount goldens; bundle keys resolve |
| `ui_dsl` | T1 | parse→write→parse; Java line numbers on malformed |
| `ui_menu_tree`* | T1 | wire bytes hex; second-process decode; result caps |
| `ui_manifest`* | T1 | manifests valid; all scenes exist; pause flags |
| `ui_hud_text`* | T1 | wave/objective/timer strings golden |

**Plan 15 (P5) — input/RTS.**

| Scenario | Tier | Assertions |
|---|---|---|
| `placement_validation_table` | T0 | ~120-row table exact |
| `input_place_line_headless` | T1 | plan counts/rot/bridges; post-construct checksum |
| `input_replay_desktop` / `input_replay_mobile` | T1 | event.jsonl → state + commands golden |
| `input_rts_move` | T1 | selection/formation/targets; 600-tick checksum |
| `input_schematic_transform` | T1 | rotate/flip/shift coords golden |
| `input_plan_snapshot` | T1 | truncation/chunking/empty behavior |
| `bench input_*`* | T2 | §7d budgets |

**Plan 16 (P6) — render.**

| Scenario | Tier | Assertions |
|---|---|---|
| `render_flat_floor` | T0 | render-list golden; 9 chunks; no dirty |
| `render_block_change` | T1 | dirty minimality; cold rebuild byte-equal |
| `render_layer_order` | T1 | `(z, seq)` ordering; `--no-sort` emission order |
| `render_darkness_radius` | T1 | dark radius predicate counts |
| `render_menu_world` | T1 | deterministic pinned seed |
| `render_bench` | T2 | build p50/p99; no alloc growth |

**Plan 17 (P6) — FX.**

| Scenario | Tier | Assertions |
|---|---|---|
| `fx_audit`* | T1 | 267/267 ledger; order hash; regions; data cases |
| `fx_lifecycle` | T0 | spawn/expiry ticks; pool reuse; golden JSON |
| `fx_noop`* | T1 | zero states; checksum == baseline (excluded) |
| `fx_program`* | T1 | per-catalogue sorted program golden (all 267 in nightly) |
| `fx_trail`* | T1 | trail vertices/lifetime golden |
| `bench_fx`* | T2 | light/mid/stress budgets; 0 alloc |

**Plans 18–22 (not on disk).** `18_AUDIO_IMPLEMENTATION_PLAN.md` (audio routing/loops), `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md` (editor round-trips), `20_MODS_IMPLEMENTATION_PLAN.md` (data-mod/patch scenarios), `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` (relay/late-join/desync), `22_PLATFORM_EXPORT_IMPLEMENTATION_PLAN.md` (export/headless-server). Catalog rows are `TBD by 18|19|20|21|22`; each plan must add its rows to `parity/matrix.toml` + catalog in the PR that lands the plan file (M7).

**Program suites (`mind-headless`).**

| Command | Behavior |
|---|---|
| `parity run --suite smoke` | T0 across landed systems; CI + local default |
| `parity run --suite gate --phase Pn` | T1 for plans with `phase ≤ Pn` |
| `parity run --suite full` | T2; nightly |
| `parity checksums --suite gate` | determinism matrix; artifacts per OS |
| `parity gate --phase Pn` | seven-step gate protocol (§3.7) |
| `parity bench-gate --baseline bench/baselines.json --canonical` | regression gate |
| `parity soak --profile mid|stress|multiplayer --minutes N` | soak |
| `parity desync-inject --case <case>` | detector cases |
| `parity mcp-parity --suite T0` | headless↔windowed checksum equality |
| `parity report --all --json` | program status roll-up |

### 7c. MCP playtest catalog

Shared harness rules are §3.5. `parity/mcp_catalog.json` is the machine-readable source; the table below is the phase-grouped index of the concrete scenarios each plan promised. Every entry loads/reuses a headless scenario where one exists (`scenario` column) so the in-engine checksum is compared with the committed golden (`parity mcp-parity`). The repo skill created by plan 00 (`mindustry-godot/.opencode/skills/playtest/SKILL.md`, mirrored to `.kimi-code/skills/playtest/`) carries the launch flow, node map, pid-stamp rule and the recipe vocabulary; this plan keeps the catalog and the skill in sync (M6).

**Phase P0 — foundation.**

| Plan | Scenario id | Scene | Key assertions | Screenshots |
|---|---|---|---|---|
| 00 | `mcp_spine` | `res://scenes/spine.tscn` | load `spine_place_break`; `step 60`; checksum == golden; API + mouse place/break; inspector label; errors empty | `00_spine_place_break_01.png` |

Plan 00 also ships `tools/mcp-smoke.sh` automating steps 1–6 + log check.

**Phase P1 — platform/content.**

| Plan | Scenario id | Scene | Key assertions | Screenshots |
|---|---|---|---|---|
| 01 | `mcp_stdb_relay_2p` | spine, instances `--p1`/`--p2` | both `connected`; create/join match; `dev_send_ping(42)` seen by both in `command_id` order; SQL cross-check; offline negative path | `01_stdb_net_page.png` |
| 02 | `mcp_content` | spine | `content_counts()` golden (item 22, block 418, unit 65, …); `content_list("item")[0] == "copper"`; inspector list; errors empty | `02_content_inspector.png` |
| 03 | `mcp_assets` | spine | `MindAssets.probe("copper-wall")` found 32×32; negative lookup false; transparency alpha probe; icon glyph; bundle key + missing-key echo; duplicates empty | `03_assets_region.png`, `03_assets_icon.png` |
| 04 | `mcp_io_roundtrip` | spine | place probe + wave 5; digest before; save; reset; load; digest after equal; runtime watch wave == 5; backup file check | `04_io_wave5.png` |

**Phase P2 — sim core.**

| Plan | Scenario id | Scene | Key assertions | Screenshots |
|---|---|---|---|---|
| 05 | `mcp_sim_pause` | spine | inspect `tick/state/group_counts/checksum`; tick advances ≈60/s; pause freezes; resume advances; errors empty | `05_sim_inspector.png` |

**Phase P3 — world/systems.**

| Plan | Scenario id | Scene | Key assertions | Screenshots |
|---|---|---|---|---|
| 06 | `mcp_world_gen` | spine | load `world_gen_serpulo`; width > 32; floor/wall counts; paused checksum golden; tile probe place/break; camera pan + round-trip | `06_world_terrain.png` |
| 07 | `mcp_blocks` | spine | place → `build2`; `dev_construct`; `copper-wall` health 360; configure/clear; multiblock shared `build_id`; break clears 9; checksum equal after save/load | `07_blocks_place.png` |
| 08 | `mcp_logistics` | spine | drill on ore → belt chain → core; step 400; core count > 0; belt `len > 0` and `ys` advance ≈ 0.035×30; payload spot check | `08_logistics_before.png`, `08_logistics_after.png` |
| 09 | `mcp_power_split` | spine | place generator/node/battery; satisfaction 1.0, stored > 0, same `graph_id`; break node → different graph ids, generator side satisfied; re-place → merged; errors empty | `09_power_before_split.png`, `09_power_after_merge.png` |

**Phase P4 — combat/units/campaign.**

| Plan | Scenario id | Scene | Key assertions | Screenshots |
|---|---|---|---|---|
| 10 | `mcp_combat_duo` | spine + `combat_duo_dummy.json` | dummy `{hp:1000}`; ammo 30; `step 240` = 8 shots; `hp == 1000 − 8·dmg` (worksheet); counters shots/created/damage/live; paused step → HP unchanged | `10_combat_preshot.png`, `10_combat_postshot.png` |
| 11 | `mcp_units_move` | spine | spawn dagger×N + mace + risso; unit_state positions; `command_move` toward (800,800) (relay row when 21 live); positions strictly closer; arrival within `max(hitSize,6)`; negative stance/banned spawn | `11_units_move.png` |
| 12 | `mcp_campaign_cycle` | spine | `start_sector("serpulo",0)`; core/wave/rules state; research node → unlocked + `req-` persisted; capture → `was_captured`, waves off, objectives empty; save/load slot checksum equal | `12_campaign_sector.png` |

**Phase P5 — logic/UI/input.**

| Plan | Scenario id | Scene | Key assertions | Screenshots |
|---|---|---|---|---|
| 13 | `mcp_logic_display` | spine | place processor/memory/display; set code; `code_lines == 6`; display commands non-empty; vars.result == 123; memory[0]==123; drawflush grows commands; errors clean | `13_logic_display.png` |
| 14 | `mcp_ui_sweep` | spine | dialog sweep (manifest-driven) each visible + non-blank; full-catalogue API sweep; text-input round trip; pause governor; HUD wave == state; toasts; console/chat; minimap pan/zoom; mobile preview | per-dialog `14_ui_<name>.png`, `14_ui_mobile.png` |
| 15 | `mcp_input_place` | spine | conveyor line drag (11 plans, valid); release → plans ≥ 11 → constructed; rotation/wheel gating; right-drag break; RTS select/move + control group; camera/minimap/shake; errors clean | `15_place_line_preview.png`, `15_place_line_done.png`, `15_rts_select.png`, `15_rts_move.png`, `15_break_rect.png` |

**Phase P6 — render/FX/audio.**

| Plan | Scenario id | Scene | Key assertions | Screenshots |
|---|---|---|---|---|
| 16 | `mcp_render_layers` | spine | fixed pose; per-layer captures + one-at-a-time toggles change only expected regions; chunk rebuild dirty counts; minimap pixel vs `color_for`; fog band | per-layer `16_layer_<name>.png` |
| 17 | `mcp_fx_impact` | spine + `fx_turret_impact.json` | live counts per tick 1..12; kinds include shoot/smoke/eject/hit; draws ≤ 200; part warmup/recoil/heat/charge samples; `dump_effects` == headless golden; paused no tick | `17_fx_pre.png`, `17_fx_shot_t2.png`, `17_fx_hit.png` |
| 18 | audio | — | `TBD by 18` | — |

**Phases P7/P8.**

| Plan | Scenario id | Key assertions |
|---|---|---|
| 19 | `TBD by 19` | editor open/save/playtest round-trip, previews, map import/export |
| 20 | `TBD by 20` | data-mod load/patch, sprite override, bundle merge, mod browser |
| 21 | `TBD by 21` | two clients see builds/units; late-join stream; desync detector clean |
| 22 | `TBD by 22` | export presets, headless server, file associations, mobile preview |

**Rendering note.** Plan 16's OD16-A–E and plan 17's OD-17-A can change capture mechanics (ArrayMesh vs MultiMesh, bloom, tolerance); the catalog keeps assertions behavioral and pins only the fixed pose + screenshot path, so option changes do not invalidate the catalog.

### 7d. Performance program

**Baseline machine.** WSL2 Ubuntu 26.04 dev host (Windows 11 + WSLg), Godot 4.7.2 standard, Rust 1.98.1, release profile, 60 s warmup, 3 runs median. **WSL Vulkan enumerates llvmpipe (software) only — no hardware ICD (dzn absent, checked 2026-10-01)**: sim/CPU budgets use `bench/baselines.json.machine.id = "dev-wsl-amd860m"`, while GPU-rendering budgets (16/17) and screenshot perf are measured on the Windows 11 host with the standard Godot 4.7.2 build at `C:\Users\Clinton\g\Godot\Godot-stable_win64.exe` (`machine.id = "dev-win-amd860m"`, AMD Radeon 860M). The self-hosted nightly runner must report the matching `machine.id` to gate; shared GitHub runners record only; CI asserts presence, not values.

**Global budgets (HLP §7.4).** 60 tps ⇒ 16.6 ms/frame; mid-game sim tick ≤ 4 ms (p99); frame headroom ≥ 25%.

**Gate policy (this plan ratifies; see NUD-39).**
1. **Determinism/checksum mismatch** → hard fail, all suites.
2. **Canonical tick** (`sim_core mid` p95) regression **> 10%** vs `bench/baselines.json.canonical` → blocks the phase gate.
3. **Subsystem benches**: absolute budget exceeded → hard fail; otherwise plan 00's ±20% warn / +50% fail policy applies until the owning plan's absolute budget is met, after which the absolute budget is the gate.
4. **Allocations**: `--assert-alloc 0` profiles must report zero steady-state allocations after warmup; any growth fails.
5. **Memory**: soak RSS growth ≤ 5% after warmup; `size_of` budgets where a plan declared one (06 Tile ≤ 40 B).
6. **Recording**: every `mind-headless bench ... --json` writes a `bench/reports/<id>-<machine>-<commit>.json` artifact; nightly appends to a trend file (not committed).

**Budget roll-up (all numbers copied from the owning plan; `pNN` = percentile).**

| Plan | Measurement | Budget |
|---|---|---|
| 00 | `Sim::tick` 64×64/256 blocks | p50 ≤ 100 µs, p99 ≤ 250 µs |
| 00 | `spine_determinism` 600 ticks | ≤ 300 ms release, ≤ 3 s debug |
| 00 | checksum + full dump 64×64 | ≤ 5 ms |
| 00 | Godot `_process` sim+sync idle | ≤ 2 ms CPU |
| 00 | cold start first frame | ≤ 3 s |
| 00 | binaries | gdext ≤ 20 MB, headless ≤ 10 MB |
| 01 | `Connector::pump()` idle | p50 ≤ 50 µs, p99 ≤ 200 µs |
| 01 | base wave ≤ 20 rows / lobby < 100 rows | ≤ 3 ms / ≤ 10 ms |
| 01 | binder delivery / 100k replay | ≥ 10k rows/s / < 100 ms |
| 01 | relay 10 clients 30 s | ≥ 200 cmd/s, p95 ≤ 150 ms, 0 gap/dup |
| 01 | command row / queue cap | ≤ 2 KB / 20k rows |
| 02 | base content load | ≤ 200 ms median of 20 |
| 02 | audit / lookup / registry mem | ≤ 2 s / ≤ 50 ns / ≤ 4 MiB |
| 03 | full pack / incremental | ≤ 90 s (+25% gate) / ≤ 20 s |
| 03 | assets ready / atlas find / bundle | ≤ 1500 ms / ≤ 200 ns / get ≤ 500 ns, format ≤ 5 µs |
| 03 | atlas metadata / GPU pages | ≤ 12 MB / ≤ 350 MB worst (expected ≤ 140 MB) |
| 04 | save / load / meta / 100-slot list / settings flush | ≤ 50 / ≤ 100 / ≤ 5 / ≤ 150 / ≤ 2 ms (p95) |
| 04 | save size groundZero / TypeIO / peak extra | ≤ 500 KB / ≤ 1 µs / ≤ 128 MiB cap (expected ≤ 2 MiB) |
| 05 | empty / mid / stress tick | ≤ 0.5 / ≤ 4.0 / ≤ 10 ms (p99); alloc 0 |
| 06 | planet gen 256² / asteroid 500² | p50 ≤ 120, p95 ≤ 250 ms / p50 ≤ 80, p95 ≤ 180 ms |
| 06 | unbuffered filter / median r2 / darkness | p95 ≤ 3 / ≤ 20 / ≤ 5 ms |
| 06 | `set_floor` / `set_block` / full-grid write / Tile size | ≤ 100 ns / ≤ 250 ns / ≤ 15 ms / ≤ 40 B |
| 07 | 2000 buildings idle / active / place / construct-1000 | p50 ≤ 0.6, p99 ≤ 1.5 / p99 ≤ 2.0 / p99 ≤ 250 µs / ≤ 300 µs; alloc 0 |
| 07 | building base IO | ≤ 200 ns/building |
| 08 | conveyor bench / lane items/tick | ≥ 20M updates/s / formula ±1% (titanium 2.29× ±1%) |
| 08 | mid logistics slice / buffer ops / save-load / payload | ≤ 0.35 ms p95 / ≥ 50M ops/s / ≤ 10 ms / ≤ 5 µs; alloc 0 |
| 09 | power / split-merge / liquid / heat | ≤ 0.30 / ≤ 1.0 (merge ≤ 0.1) / ≤ 0.20 / ≤ 0.50 ms; alloc 0 |
| 10 | 2000 bullets+400 turrets / 10k bullets / 200 idle turrets | ≤ 1.2 ms p95 / ≤ 2.5 ms / ≤ 0.15 ms |
| 10 | tileDamage r32 / explosion / spawn churn / lightning | ≤ 0.3 ms / ≤ 0.5 ms / ≤ 10 µs / ≤ 5 µs; alloc 0 |
| 11 | units mid / stress / pathfinder / rts-200 / wave burst | ≤ 1.5 / ≤ 5.0 / avg ≤ 0.6 worst ≤ 1.5 / ≤ 1.2 ms / ≤ 4 ms |
| 11 | memory path tiles / caches | ≤ 1 MiB / ≤ 8 MiB with eviction; alloc 0 |
| 12 | teams / fog / turn / objectives / schematic | ≤ 0.35 / ≤ 0.60 / ≤ 0.50 / ≤ 0.10 / ≤ 1.0 ms; alloc 0 |
| 13 | mixed / arith / world / idle-100 / hyper-100 | ≤ 25 / ≤ 10 ns/instr / ≤ 30 µs / ≤ 0.2 ms / ≤ 1.5 ms |
| 13 | assemble 1000 lines / roundtrip / draw / print / per-proc mem | ≤ 2 ms / ≤ 1 µs / ≤ 40 ns/cmd / ≤ 60 ns/glyph / ≤ 16 KiB |
| 14 | UI frame / dialog open / cold / table-500 / text / DSL / theme / UI tex | ≤ 1.0 / ≤ 8 / ≤ 80 / ≤ 4 / ≤ 10 µs/512ch / ≤ 1 µs/node / ≤ 120 ms / ≤ 4 MB; alloc 0 |
| 15 | preview / astar / bridges / select-1000 / command-200 / idle / in-engine | ≤ 0.5 / ≤ 2.0 / ≤ 0.5 / ≤ 1.5 / ≤ 1.0 / ≤ 0.2 / ≤ 2.0 ms (p95); alloc 0 |
| 16 | world build p50/p99 / full frame / GPU / floor bake / recache | ≤ 2.5/5 ms / ≤ 16.6 ms (25% headroom) / ≤ 8 ms / ≤ 250 ms / ≤ 2 ms |
| 16 | draw calls / triangles / alloc / texture mem | ≤ 4000 (worst 8000) / ≤ 80k / 0 growth / ≤ 512 MB |
| 17 | light / mid / stress program+submit | ≤ 0.25+0.4 / ≤ 1.5+3.0 / ≤ 3.0+5.0 ms; ≤ 40/200 draws; ≥ 55 fps; alloc 0 |
| 17 | pool churn 1000/s | ≤ 2 µs/effect |

**Soak tests** (`parity soak`; `parity/soak.toml`): `mid` 60 min (256², 600 buildings, 300 units, 3000 bullets, 4000 belt items; checksum checkpoints every 600 ticks must match a same-seed short replay prefix; RSS growth ≤ 5%; alloc delta 0), `stress` 10 min (512², 2000/1000/10000), `multiplayer` 30 min when 21 lands (2 headless clients + 1 windowed via MCP; desync clean), `windowed` 30 min MCP-driven (no leak, error log clean, screenshots at 0/15/30 min diff-stable modulo animated content).

**Alloc/memory mechanics.** `mind-core` feature `alloc-audit` (05) counts allocations; `bench --assert-alloc 0` fails any profile marked in `baselines.json.alloc.assert_zero_profiles`; `dhat`-style heap summaries are nightly artifacts, not gates. `render_alloc_events == 0` (16) and `fx` pool growth 0 (17) are in-engine counterparts.

### 7e. Parity ledger, exit criteria and phase gates

**Ledger (maintained by `parity report`, rendered here per gate).** Status at authoring: every system is `not-started`. "Parity complete" definition is §3.7; evidence is the gate report + plan Changelog paths.

| System | Plan | Phase | Upstream test rows (primary) | Scenarios promised | MCP promised | Budgets | Status / evidence |
|---|---|---|---|---|---|---|---|
| Foundation | 00 | P0 | 5 (timers×3, createMap, initialization) | 4 | 1 | 6 | not-started |
| Platform/STDB | 01 | P1 | 0 upstream (behavior tests) | 4 | 1 | 6 | not-started |
| Content | 02 | P1 | 1 (`initialization`) + new parity tests | 5 | 1 | 5 | not-started |
| Assets | 03 | P1 | 0 upstream (asset aspects cross-ref 20) | 7 | 1 | 7 | not-started |
| IO/serialization | 04 | P1 | 10 (`writeStringTest`…`load152`, `save`, `saveLoad`) | 6 | 1 | 8 | not-started |
| Sim core | 05 | P2 | 10 (timers, groups, reset, wave event, order) | 6 | 1 | 3 | not-started |
| World/terrain | 06 | P3 | 10 (`createMap`…`load` world halves) | 7 | 1 | 8 | not-started |
| Blocks/build | 07 | P3 | 8 (multiblock, inventories, overlap, destruction, limits) | 6 | 1 | 7 | not-started |
| Logistics | 08 | P3 | 5 (`sorter/router/junction/conveyor`, inventories, deposit) | 10 | 1 | 7 | not-started |
| Power/liquids/heat | 09 | P3 | 3 power classes (≈17 cases) + liquids | 6 | 1 | 5 | not-started |
| Combat/bullets | 10 | P4 | 0 upstream (substitute oracles) | 9 | 1 | 8 | not-started |
| Units/AI/waves | 11 | P4 | 4 (`spawnWaves`×3 halves, payload, init anchors) | 10 | 1 | 7 | not-started |
| Campaign | 12 | P4 | 6 (`writeRules`…`testSectorValidity` halves) | 8 | 1 | 6 | not-started |
| Logic/mlog | 13 | P5 | 15 `LogicTests` groups | 14 | 1 | 9 | not-started |
| UI | 14 | P5 | 0 upstream (Java-oracle harness) | 5 | 1 full sweep | 9 | not-started |
| Input/RTS | 15 | P5 | 0 upstream (input-log replay; 3 placement-adjacent) | 7 | 1×5 flows | 7 | not-started |
| Render/world | 16 | P6 | 0 upstream (render-list goldens) | 6 | 1×3 poses | 9 | not-started |
| FX/parts | 17 | P6 | 0 upstream (267-effect ledger + goldens) | 6 | 1 | 5 | not-started |
| Audio | 18 | P6 | TBD by 18 | TBD | TBD | TBD | not on disk |
| Maps/editor | 19 | P7 | TBD by 19 | TBD | TBD | TBD | not on disk |
| Mods | 20 | P7 | `DataAssetTests`/`PatcherTests` (≈39) + `ModTestAllure` | TBD by 20 | TBD by 20 | TBD by 20 | not on disk |
| Multiplayer | 21 | P8 | 0 upstream (relay/desync) | TBD by 21 | TBD by 21 | TBD by 21 | not on disk |
| Platform/export | 22 | P7 | TBD by 22 | TBD by 22 | TBD by 22 | TBD by 22 | not on disk |

**Program exit criteria checklist.**

- [ ] `parity/matrix.toml` covers every upstream test with a primary owner; `parity matrix check` green in CI.
- [ ] `parity goldens verify` green; `golden-drift` job green; every oracle golden has `upstream.commit` + sha256.
- [ ] Determinism green: same-process ×2, fresh-process ×2, workers 1/4, Windows/Linux artifacts equal, MCP in-engine checksums equal.
- [ ] `bench/baselines.json` records all plan budgets on the baseline machine; `parity bench-gate` enforces the >10% canonical rule + absolute budgets; alloc audits zero.
- [ ] Soak profiles pass (`mid` 60 min, `stress` 10 min; `multiplayer` when 21 lands).
- [ ] Every promised scenario exists (no `TBD` for a landed plan); naming check green; `run-all` covers the catalog.
- [ ] MCP catalog matches the repo playtest skill; `tools/mcp-smoke.sh` passes for T0 + each landed phase; screenshot baselines committed where plans declared pixel oracles.
- [ ] Phase gates P0–P8 each have `parity/reports/gate_Pn.json` with `pass: true` and a §7e ledger row updated to `parity-complete|deviation|deferred`.
- [ ] Risk register (§8) current: every resolved NUD has a resolution note; every open item has an owner; every accepted deviation in §8.3 is reflected in the owning plan's §2/§9.
- [ ] `parity report --all --json` green at the final ledger review.

### 7f. Proposed CI patch (orchestrator-owned; NOT applied by lane 23)

> **Update 2026-10-03 (F20 post-join, orchestrator):** the `tools/ci.sh` half of
> this patch is now **APPLIED** (the `parity check --tests` / `parity goldens` /
> `run-all --tier T0` block after `cargo test -p mind-core`; `.ps1` is a thin WSL
> wrapper so it needs no change). The `.github/workflows/ci.yml` `matrix-check`
> job below is still proposed/not applied (no local CI runner).

The plan-23 harness is already runnable; wiring it into the shared CI is
orchestrator-owned per the lane brief, so the patch is recorded here rather than
applied.

`tools/ci.sh` (after the `cargo test -p mind-core` step; `.ps1` twin mirrors it):

```sh
mkdir -p out
cargo test -p mind-core    -- --list > out/tests-core.txt
cargo test -p mind-headless -- --list > out/tests-headless.txt
cat out/tests-core.txt out/tests-headless.txt > out/tests.txt
cargo run -q -p mind-headless -- parity check --tests out/tests.txt   # matrix + all registries (harness goldens pinned)
cargo run -q -p mind-headless -- parity goldens                       # sha256 drift, no JVM
cargo run -q -p mind-headless -- run-all --tier T0                    # T0 smoke scenarios (catalog-driven)
```

`tools/ci.ps1`:

```powershell
New-Item -ItemType Directory -Force -Path out | Out-Null
cargo test -p mind-core     -- --list | Out-File out/tests-core.txt
cargo test -p mind-headless -- --list | Out-File out/tests-headless.txt
Get-Content out/tests-core.txt, out/tests-headless.txt | Set-Content out/tests.txt
cargo run -q -p mind-headless -- parity check --tests out/tests.txt
cargo run -q -p mind-headless -- parity goldens
cargo run -q -p mind-headless -- run-all --tier T0
```

`.github/workflows/ci.yml` — a `matrix-check` job (needs the rust toolchain + cache
already defined for `rust-lint`):

```yaml
  matrix-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with: { workspaces: client/rust }
      - run: |
          cargo test -p mind-core     -- --list > tests-core.txt
          cargo test -p mind-headless -- --list > tests-headless.txt
          cat tests-core.txt tests-headless.txt > tests.txt
          cargo run -q -p mind-headless -- parity check --tests tests.txt
          cargo run -q -p mind-headless -- parity goldens
          cargo run -q -p mind-headless -- run-all --tier T0
        working-directory: client/rust
```

Notes:
- `parity check` is read-only and needs no Godot/GPU; safe on shared runners.
- `--tests` is what turns on the "every `status = \"landed\"` row resolves" gate;
  without it, the check is structural only. At the F17 lane that is **91/91 landed**
  rows resolved (9 `no-equivalent` rows carry substitute oracles).
- `parity goldens` now verifies **oracle + scenario + harness** sha256 (64 entries):
  a re-recorded harness golden must update `parity/golden_manifest.json` in the same
  commit.
- `run-all --tier T0` is the catalog-driven smoke suite (4 file-backed T0 scenarios);
  the full T1/T2 suite stays on the nightly runner.
- `parity gate Pn --out parity/reports/gate_Pn.json` is the phase-gate entry point;
  steps 2–5 are nightly-owned and reported as `deferred` until the perf/GPU runners
  are wired (NUD-40/A).

## 8. Risks & open decisions

### 8.1 Aggregated `NEEDS USER DECISION` register

Every sibling item marked `NEEDS USER DECISION` (or equivalent), deduplicated and numbered; the "Default" column is the choice current plans proceed with. Execution continues on defaults; a user override lands in the owning plan first, then here. Items HLP `OD5`–`OD9` are already user-decided (`no`/locked) and are not repeated.

| ID | Owner plan | Decision | Default being planned against | Blocks |
|---|---|---|---|---|
| NUD-01 | 20 (HLP OD1) | Mod scripting engine replacing Rhino/JS | JSON-only mods; script hook behind a trait; candidate engines evaluated at 20 | plan 20 execution |
| NUD-02 | 04 (HLP OD2) | Upstream `.msav` compatibility | Native `MGRS` v1; one-way importer behind default-off `msav-import` | 04 M7 (importer only) |
| NUD-03 | 21 (HLP OD3) | Multiplayer transport beyond STDB relay | STDB relay; direct ENet/UDP only if measured too slow | 21 execution |
| NUD-04 | 22 (HLP OD4) | Steam/Discord/Workshop/achievements | Deferred; optional plan-22 work | 22 |
| NUD-05 | 00 OD-R1 | Archive C# tree via git history vs a `legacy/` folder | `git init` + tag `legacy-csharp-reference`; markdown preserved under `docs/port-history/` | 00 M6 |
| NUD-06 | 00 OD-R2 / 16 OD16-B | Renderer method (`forward_plus` vs `gl_compatibility`/mobile) | `forward_plus` (Vulkan) | 00 M1, 16 |
| NUD-07 | 00 OD-R3 | How the standard (non-mono) Godot 4.7 build is obtained | Standard `godot4` on PATH in WSL locally; CI downloads pinned standard build; `tools/godot.sh` resolves | 00 M1/CI |
| NUD-08 | 01 R3 | Server module/db names | Crate `mindustry_godot`; local db `mindustry` (integration `mindustry-it`) | 01 M0, 21 |
| NUD-09 | 01 R4 | Command envelope representation | Typed `CommandKind` enum with fixed envelope; plan 21 owns the full variant set | 01 M4, 21 schema |
| NUD-10 | 02 R1 / 11 / 13 R13 / 14 | One-off JVM parity dumps acceptable | Yes: `parity/java/*` run manually with JDK 17; outputs committed; CI never needs a JVM | 02 M4, 11 M6, 13 M2, 14 M0 |
| NUD-11 | 03 R10 | Vendor assets into the repo vs reference upstream checkout | Vendor via `mind-tools migrate`; `MIND_UPSTREAM` override for re-diff | 03 M1 + CI size |
| NUD-12 | 03 R11 | GLSL→Godot shader translation | Hand-port to `.gdshader` + `shaders check` drift guard | 16 shaders |
| NUD-13 | 03 R12 | Icon-font regeneration | Reuse upstream `icon.ttf`/`logic.ttf`/`tech.ttf`; generate code tables only | 03 M4 |
| NUD-14 | 04 R3 | Data-root location (`./data` vs `user://`) | Portable `./data` when writable, else `user://`; `--data-dir` override | 04 M1, 22 |
| NUD-15 | 05 R2 (OD-05-A) | Entity-group removal order | Insertion-stable slab (if float tie-breaks demand Java order, switch to swap-removal) | 05 M4, 08/10/11 goldens |
| NUD-16 | 05 R3 (OD-05-B) | Worker threads in authoritative replay | Keep workers; prove `workers 1 == 4` checksums | 05 M7, 21 |
| NUD-17 | 06 OD6-A | Generation RNG determinism | Named seeded `SimRng::MapGen` streams; structural parity only (HLP §9) | 06 M5 |
| NUD-18 | 07 R2 | `BlockDef.kind_data` additive field (cross-plan API) | Add `BlockKindData` enum in 07; registration waves construct it | 07 M0, 02/08 |
| NUD-19 | 07 R3 | Manual `BuildingCodec` registration in 04 | Yes (derive-only would break byte-layout parity) | 07 M0, 04 |
| NUD-20 | 08 R2 / 12 R2 | Core inventory sharing model | `TeamInventory` resource in `Teams`; accessors for cores/storage | 08 M1, 04/12/14/21 |
| NUD-21 | 08 R3 | Carried payload entity model | Real ECS entities + `PayloadCarried` marker + group removal + `EMPTY` tile | 08 M4, 04/11/21 |
| NUD-22 | 09 R4 | Exact Arc `QuadTree` port vs simpler scan | Exact port in `world::spatial::quad_tree`; 11/12 consume | 09 M1, 11/12 |
| NUD-23 | 09 R5 (UD-09-2) | `LiquidModule` single-watched flow cache | Parity (single watched building); per-building is a UX deviation | 09 M3, 14 |
| NUD-24 | 10 OD-10-A | Cross-platform float determinism for sim math | Route sim math through pure-Rust `libm` wrappers; view may use `std` | 10 M1, CI cross-OS |
| NUD-25 | 11 OD-11-A | Pathfinder/`UnitGroup` work model | Deterministic fixed node budgets on the sim clock; single worker joined at tick boundary | 11 M2 |
| NUD-26 | 11 OD-11-B | `UnitGroup` formation thread | Synchronous in the triggering tick (squads ≤ 50); async only if >200-unit squads become a target | 11 (conditional) |
| NUD-27 | 13 R1 | Ship logic stubs while plan 12 is absent vs wait | Ship `LogicRulesApi`/`MarkerApi` stubs with upstream defaults; replace at 12 kickoff | 13 M1 |
| NUD-28 | 14 OD-UI1 | scene2d `Table` mapping | Port `MindTable`/`MindCell` layout to GDScript; dialogs are `.tscn` + GDScript | 14 M1 |
| NUD-29 | 14 OD-UI2 | Server-menu transport encoding | Port MSUI 1:1 (`ui_node` bytes format 1, `UiKey` ordinals frozen) inside relay rows | 14 M3, 21 |
| NUD-30 | 14 OD-UI3 | Content-icon rendering in text | BBCode + `RichTextEffect [icon]` + PUA normalization (no glyph injection) | 14 M2, 03 |
| NUD-31 | 15 U1 | `UiFocus` delivery contract | UI pushes focus flags to `MindInput`; Rust probe only as fallback | before 14 lands |
| NUD-32 | 15 U2 | `SimCommand` action-variant ownership | Extend 05's enum at M4; 21 translates to its wire type | before 21 schema freeze |
| NUD-33 | 16 OD16-A | Floor/block chunk representation | `ArrayMesh` per `(chunk, CacheLayer)` with band items | 16 M2 |
| NUD-34 | 16 OD16-C | Bloom implementation | Band-capture `SubViewport`; whole-frame glow as fallback | 16 M7 |
| NUD-35 | 16 OD16-D | Light composite blend fidelity | `CanvasItemMaterial.BLEND_MODE_ADD`; RD per-channel `max` shader if diff visible | 16 M5 |
| NUD-36 | 16 OD16-E | Map screenshot capture path | Main-thread `SubViewport` + worker `save_png`; render-thread save if required | 16 M8 |
| NUD-37 | 17 OD-17-A | Visual parity acceptance tolerance | SSIM ≥ 0.995 / ≤ 0.5% changed pixels in the FX rect; geometry compared exactly | 17 M3 |
| NUD-38 | 17 OD-17-B | FX custom-body location | `mind-core` + `DrawPrim` for all bodies; only noise/shader/MultiMesh submission in gdext | 17 M4 |
| NUD-39 | 23 | Regression threshold: >10% canonical tick regression blocks a phase gate (siblings had ±20% warn/+50% fail) | Adopt 10% for the canonical tick; keep 20/50 for subsystem micro-benches until absolute budgets are met | every phase gate |
| NUD-40 | 23 | Where MCP smoke runs | Local + self-hosted `windows-gpu` nightly; shared CI never requires a GPU/editor | nightly setup |
| NUD-41 | 18 OD-18-A | Ambient-source polling model | Main-thread 20 Hz poller over a pre-built `AmbientSnapshot`; worker behind the same trait only if measured too expensive | 18 M6 |
| NUD-42 | 18 OD-18-B | Lowpass "wet" fidelity (Godot has no Arc filter wet/dry param) | Cutoff sweep 20500→500 Hz (0.4 s, `FILTER_12DB`, resonance 1.0) on the Sound bus | 18 M4 |
| NUD-43 | 19 OD19-A | Editor rendering strategy | Reuse 16's chunk meshes/bands via `MindRender.mount_editor_view` + `SubViewport`; private 60×60 editor layer only as fallback | 19 M3 |
| NUD-44 | 19 OD19-B | Editor model location | `mind-core::editor` (Godot-free, headless-testable op-log oracle); `mind-gdext` is the view facade | 19 M1 |
| NUD-45 | 21 OD-21-A | Player-possessed unit authority under D2 | Client-owned kinematics/health + `checksum_scoped` mask; authoritative sim removes the mask later | before 21 M4 |
| NUD-46 | 21 OD-21-B | Snapshot authority / host migration | Host = `created_by`, no migration (host leaving ends the match); candidate-host handover only if wanted | before 21 M6 |
| NUD-47 | 21 OD-21-C | Ban key | STDB identity + optional `banned_name_patterns`; subnet bans rejected with a message | before 21 M6 |
| NUD-48 | 21 OD-21-D | Admin bootstrap | Seeded `ADMIN_IDENTITIES` const (`build.sh --admin`), empty by default; no self-claim reducer | before 21 M1 |
| NUD-49 | 21 OD-21-E | Transport beyond STDB relay | STDB-only until §3.13 thresholds trip; user sign-off before adding a direct ENet/UDP data plane | 21 execution |
| NUD-50 | 22 OD-P1 | Dedicated server binary form | Pure Rust `mind-headless server` (not a Godot headless export) | 22 M2 |
| NUD-51 | 22 OD-P2 | Steam/Discord/Workshop/achievements scope | `GameService` trait + empty workshop stubs + disabled Discord mapping; no Steam code | 22 |
| NUD-52 | 22 OD-P3 | iOS shipping target this phase | Preset + plugin design + store notes authored; all iOS verification NOT-RUN (WSL/Linux host; macOS required) | 22 M5 |
| NUD-53 | 22 OD-P4 | Mobile renderer | `mobile` everywhere via NUD-06=B; no per-platform override needed (resolved 2026-10-01) | locked |

### 8.1.1 Locked user decisions (2026-10-01)

The user answered the register on 2026-10-01. Most items accepted the recommended default; the overrides are called out in bold. Execution records the chosen option in the owning plan before its milestone; this table is the authoritative summary of the answers.

| ID | Decision | Note |
|---|---|---|
| NUD-05 | **C — delete the legacy C# tree outright** | No git tag/archive, no `docs/port-history/` copy; keep it only until plans 00/01 stop referencing it, delete at plan 00 M6. |
| NUD-06 | **B — `mobile` renderer everywhere** | Desktop and Android both use Godot's `mobile` renderer; `gl_compatibility` is the emergency fallback. Plan 16's RenderingDevice-only paths (OD16-D max-blend) fall away; additive light blending is final. |
| NUD-07 | **Standard Godot host** | `godot4` on PATH in WSL Ubuntu (4.7.2 standard); `godot4-mono` fallback. Pin the same standard build in CI. |
| NUD-08 | **crate `mindustry_godot`, local db `mindustry`, integration db `mindustry-it`** (final naming, 2026-10-01; STDB 2.10.1 rejects underscores in *database* names, so the db keeps no snake_case). | Verified against `spacetimedb-client-api-messages-2.10.1` name parsing; see `01_PLATFORM_STDB_IMPLEMENTATION_PLAN.md` legacy-notes section and R3. |
| NUD-11 | A — vendor assets | `mind-tools migrate`; `MIND_UPSTREAM` override to re-diff. |
| NUD-12 | **C — hybrid shader translation** | Hand-port complex shaders; transpile simple ones into committed, reviewed `.gdshader` output; `shaders check` gates all drift. |
| NUD-13 | A — reuse upstream icon fonts | `mind-tools` generates code tables only. |
| NUD-14 | A — app-data root + portable opt-in | Plan 22 §6.1 precedence. |
| NUD-17 | A — seeded map-gen RNG streams | |
| NUD-15 | A — insertion-stable slab removal order | |
| NUD-16 | A — keep workers, prove `workers 1 == 4` checksums | |
| NUD-18 | A — `BlockKindData` additive field | |
| NUD-22 | A — exact `QuadTree` port | |
| NUD-24 | A — `libm` wrappers for sim math | |
| NUD-26 | A — synchronous formation (squads ≤ 50) | |
| NUD-19 | A — manual `BuildingCodec` registration | |
| NUD-20 | A — `TeamInventory` core sharing | |
| NUD-21 | A — live ECS payload entities | |
| NUD-01 | A — JSON-only now; WASM later | `ScriptHost` trait + `NoScriptHost`; engine chosen at plan 20 M8 if scripting is greenlit. |
| NUD-02 | A — native `MGRS` + opt-in one-way map importer | |
| NUD-03 / NUD-49 | A — STDB-only relay, §3.13 thresholds tripwire | Stop and ask before adding a direct ENet/UDP data plane. |
| NUD-04 / NUD-51 | A — Steam/Discord/Workshop deferred | `GameService` trait + stubs + documented mapping only. |
| NUD-09 | A — typed `CommandKind` enum envelope | |
| NUD-10 | A — one-off JVM goldens, outputs committed | CI never needs a JVM. |
| NUD-27 | A — direct integration (stubs moot; plan 12 exists) | |
| NUD-28 | A — `MindTable`/`MindCell` port | |
| NUD-29 | A — MSUI `ui_node` bytes 1:1 | |
| NUD-30 | A — BBCode + `RichTextEffect [icon]` | |
| NUD-31 | A — UI pushes focus flags to `MindInput` | |
| NUD-32 | A — plan 05 owns the base `SimCommand` enum; module plans append variants | |
| NUD-33 | A — `ArrayMesh` per `(chunk, CacheLayer)` | |
| NUD-34 | A — band-capture bloom | |
| NUD-35 | A — additive light blend first | Now final because NUD-06=B removes the RD path. |
| NUD-36 | A — `SubViewport` capture + worker save | |
| NUD-37 | A — SSIM tolerance for FX | |
| NUD-38 | A — FX bodies in `mind-core` + `DrawPrim` | |
| NUD-39 | A — 10% canonical-tick regression gate; 20/50% for micro-benches | |
| NUD-40 | A — local + self-hosted `windows-gpu` nightly MCP smoke | |
| NUD-41 | A — main-thread 20 Hz ambient poll | |
| NUD-42 | A — lowpass cutoff sweep | |
| NUD-43 | A — adapt plan-16 chunk meshes for the editor view | |
| NUD-44 | A — `mind-core::editor` | |
| NUD-45 | A — client-owned possessed units + `checksum_scoped` mask | |
| NUD-46 | A — no host migration | |
| NUD-47 | A — identity + name-pattern bans | |
| NUD-48 | A — seeded admin const | |
| NUD-50 | A — pure-Rust `mind-headless server` | |
| NUD-52 | **B — iOS cut from this phase** | No preset/plugin/store work; tracked in HIGH_LEVEL_PLAN §9. |
| NUD-53 | A — `mobile` everywhere via NUD-06=B | No per-platform override needed. |
| NUD-23 | *Not explicitly answered — default continues* | Single watched-building liquid flow cache (upstream parity). |
| NUD-25 | *Not explicitly answered — default continues* | Fixed per-tick pathfinder node budgets. |

**Workflow rule (user, 2026-10-01):** Godot conventions first — prefer `.tscn`-declared nodes/scenes over code construction; keep the game navigable in the editor; any node instantiated in code carries a `// code-instantiated: <specific reason>` comment. See `HIGH_LEVEL_PLAN.md` §6.6 and `AGENTS.md`.

### 8.2 Cross-plan reconciliation register (orchestrator actions; not user decisions)

### 8.2 Cross-plan reconciliation register (orchestrator actions; not user decisions)

Every open risk from a sibling that requires another plan to act, consolidated by seam. "Default" is the resolution this plan records and monitors; the orchestrator fixes the plans/code at the named milestone.

| ID | Seam | Flagged by | Default resolution |
|---|---|---|---|
| R-01 | **Node paths and entry scene.** 05/08/09/11/12/14 wrote `/root/Main/SimBridge`, `/root/Main/MindCampaign`, `res://scenes/Main.tscn`, `MindSim.dev_*`; 00/07/10/13/15/16/17 use `/root/Spine/SimHost`, `res://scenes/spine.tscn`, `SimHost.*`. | 06 R5, 05 R10, 16 OD16-J | Plan 00 §3.5 authoritative: `/root/Spine/*` + `res://scenes/spine.tscn`; update 05/08/09/11/12/14 text and their MCP steps before their milestones; add a path table to the plan-00 extension contract. |
| R-02 | **Two checksum definitions.** 00 ships xxh3-64 over a canonical `DUMP_FORMAT` stream; 05 defines FNV-1a-64 `Sim::checksum()` with `CHECKSUM_VERSION`; 06/12/13 mutate it; 04 references "plan-23 canonical dump checksum". | 00 §6.4, 05 §6.5, 06 R10, 12 R5, 13 R3 | One canonical sim checksum (05 owner). 00's stream becomes the **dump** hash only; at 05 M8 all goldens are re-recorded once; `parity/checksum_registry.json` is the contract; 04's scenario uses the canonical checksum. |
| R-03 | **Generated STDB bindings: checked in vs gitignored.** 01 M0/§6.5/R5 commits them with a `--check` drift gate; 00 §3.3/§6.5/OD-R7 gitignores them and feature-gates `pub mod generated`. | 01 R5 | Checked in (01) + `server/build.sh --check` drift job; update plan 00 `.gitignore` and OD-R7; `cargo check -p mind-stdb` then works on a clean clone without the CLI. |
| R-04 | **Duplicate test ownership** (payloads 08/11; blockInventories 06/07/08; inventoryDeposit 07/08/15; save/saveLoad 04/06/12; createMap/playMap 00/04/05/06/12; timers 00/05; power fixture 05/09; allBlockTest 02/07/10; `writeRules` 04/12). | this plan | Matrix rows keep one `primary` per upstream test + composition rows; primary owners as rendered in §7a-1/7a-2. |
| R-05 | **`SimCommand` variant ownership.** 11 R3 and 15 U2 both need additive variants (unit commands, stances, queue, non-placement actions). | 11 R3, 15 U2 | Extend 05's enum at M4 (base set in 05 §6.4); 21 owns the wire mapping. |
| R-06 | **Payload entity model** affects entity IDs in saves/replays. | 08 R3, 11 (payload carriers) | NUD-21 default; 04 serializes ECS payload entities; 21 snapshot rules consume them. |
| R-07 | **Core inventory sharing** affects save shape and sync. | 08 R2, 12 R2 | NUD-20 default; 04 records `TeamInventory` in `Teams` state; 14 reads via accessors; 21 sync policy on the resource. |
| R-08 | **`Rules`/`Teams` merge ownership.** 05 ships boundary stubs (R4); 12 takes merge ownership; 07 appends placement-limit fields (R6). | 05 R4, 07 R6, 12 R1 | 12 M0/M1 replace the stubs wholesale; 07's additive fields land with 12's merge; 05 changelog records the hand-off; `sim_core_schedule_order` golden updated once. |
| R-09 | **`WindowedMean` ownership.** 08 uses it (flow windows), 09 ports it, 12 consumes `ExportStat`. | 09 R9, 12 R10 | Single port in `math::windowed_mean` (09); 08/12 consume; no duplicates. |
| R-10 | **Arc `QuadTree` ownership.** 09 R4 wants 09 to port it; 11/12 need queries; 12 R3 offers a fallback. | 09 R4, 12 R3, 11 (TeamData) | 09 ports exact semantics into `world::spatial::quad_tree`; 11/12 consume; 12's fallback only if 09 slips. |
| R-11 | **`BlockDef.kind_data` / `BlockKindData` ownership.** 02 R3/R4, 07 R2, 08 §3.2 freeze. | 02 R3/R4, 07 R2 | NUD-18 default: 07 defines, 02 constructs at registration; 08 consumes; no re-registration. |
| R-12 | **Controller codec ownership** (`writeController`/`readController`). | 11 R4 (no 04 owner) | 11 specifies codec + fixtures; 04 implements `io/entity/controller_codec.rs`; 21 consumes for snapshots. |
| R-13 | **`WorldContext` trait ownership** (04 defines; 06 implements). | 04 R6, 06 R1 | 04 §3.1/M4 signature authoritative; 06 adapts and records the exact signature; no duplicate trait. |
| R-14 | **Draw naming/paths.** 16 OD16-I says real path is `world/draw/*`; 16/17 split `DrawBlockParts`/part bodies; `DrawPrim` vs `DrawCmd`. | 16 OD16-I/K, 17 R-17-1 | 16 owns `render/{layer,commands}.rs`; 17's `DrawPrim` maps onto `DrawCmd`; constants use Java identifiers; port maps corrected. |
| R-15 | **Effect registry.** 02 ships a seed `fx_meta`; 17 owns `EffectRegistry`. | 02 R6, 17 R-17-2 | 17 owns; 02 keeps a seed accessor; names/ids unchanged; close 02 R6 in its changelog. |
| R-16 | **Weather draw ownership** (02 port map says 16; 17 claims particle visuals). | 17 R-17-3 | 17 owns draw bodies + prim builders; 16 owns registration/pipeline; both port maps note it. |
| R-17 | **Astar ownership** (06 generation vs 11 unit pathing). | 06 R4 | 06 ships a deterministic grid search for generation; 11 wraps/reuses without breaking the API. |
| R-18 | **`basepartnames` generation** (03 generates; 11 consumes; 12 `.msch` decode). | 11 R6 | 03 generates from `baseparts/`; 11 ships a committed fallback list; 12 uses it for schematics. |
| R-19 | **`EntityDefs!`/`FieldMeta` seam.** 11 R2 vs 05 §3.5; 02 R5 vocabulary freeze. | 05 R6, 11 R2, 02 R5 | Vocabulary frozen from `entities/comp/*` names; 05 consumes `FieldMeta`; plans may append, never rename. |
| R-20 | **MCP API naming.** `MindSim.dev_*` vs `SimHost.*` vs `MindUnits.*` vs domain autoloads. | R-01 set | One table in the plan-00 extension contract: `MindSimHost` (sim), `MindCamera2D`, `MindUnits`, `MindCampaign`, `MindIo`, `MindUi`, `MindRender`, `MindFx`, `MindInput`; methods are append-only. |
| R-21 | **Scenario mirror/paths.** MCP steps reference `res://scenarios/*.json`; only `tools/sync_scenarios` writes `client/scenarios/`; 00 requires CI sync assert. | 00 §6.5, 02/06/10/13/17 MCP steps | Every scenario JSON lives in repo `scenarios/`; sync assert in `ci.yml`; MCP loads `res://scenarios/<name>.json` only. |
| R-22 | **Atlas size vs render texture budget.** 03 asserts `AtlasIndex::len ≥ 18000`; 16 caps texture memory ≤ 512 MB. | 03 §7, 16 §7.4 | Gate P6 cross-check: page count/dimensions from `assets boot --dump` must satisfy 16's budget; add the check to `parity gate --phase P6`. |
| R-23 | **Shader manifest** handshake 03↔16 (`shaders check --reverse`). | 03 R11, 16 §7.5 | 03 owns the manifest; 16 fails its checklist if a shader is missing/untranslated. |
| R-24 | **`logicids.dat` absent behavior** vs an optional committed name table. | 13 R2 | Locked absent-file behavior; optional `logicids.toml` only via a future flagged decision; matrix row asserts null/-1. |
| R-25 | **Headless UI smoke.** 14 wants GDScript smoke assertions under a headless Godot run; `godot-compositor-testing` warns headless has no RenderingDevice/window specifics. | 14 §7a(3) | Smoke only scene-load + manifest resolution + `MindTable` rect math under `--headless --script`; pixel work stays in `mcp_ui_sweep` on the GPU host. |
| R-26 | **09 bridges depend on 08 bases** (`ItemBridge`/`DirectionBridge`) outside 09's declared deps. | 09 R2 | Either extend 09's `Depends on` to 08 or move the bases to 07; decide at P3 kickoff; 09 M5 gated. |
| R-27 | **10/09/11 `BlockIndexer`/`TargetQueries` ownership.** 10 R-10-3/R-10-8, 11 §3. | 10 R-10-3/R-10-8 | First lander implements a minimal trait-backed index; 11 provides the ECS implementation; 10 consumes by name. |
| R-28 | **07 R11 `PATCH_DENIED` handshake with 20** and `@NoPatch` enforcement. | 07 R11, 02 R3 | 07 exports the field-name list; 20's DataPatcher rejects; a cross-plan test lands when 20 lands. |
| R-29 | **15 R4 local-apply vs relay rejection.** D2 has no authoritative sim; dropped commands can diverge. | 15 R4 | Clients apply immediately (upstream parity); 21 owns rejection events + desync correction; 15 shows toasts and never retries silently. |
| R-30 | **Plans 18–22 absent.** Plan 16 already consumes plan-18 audio hooks; 02/04 have trait stubs for 19/20/21. | this plan | No work blocked before P6; each missing plan must add matrix/catalog/budget rows in its own PR; `parity matrix check` fails a row that names a non-existent plan file (`TBD by 18` rows point at the expected filename). |
| R-31 | **02 sector/planet traits** (`SectorRemapProvider`, `SectorView`, `SchematicHooks`, `BaseRegistryView`, `MapGenHooks`). | 02 R7, 06 R3 | Traits with identity/no-op defaults ship in 02/06; 11/12 replace at their milestones; no duplicate trait. |
| R-32 | **13 font metrics fallback** (`LogicFontMetrics`) until 16 ships real atlas metrics. | 13 R4 | Committed fallback constants; 16 replaces; `logic_draw` golden re-recorded once at replacement (checksum-neutral; draw buffer only). |
| R-33 | **16 preview call sites** (`MapPreviewLoader.checkPreviews`) split 04/06/16/19. | 16 OD16-L, 06 R8 | 16 owns the two frame call sites; 06 owns queue/paths; 19 pixels/PNG; 04 meta hooks. |
| R-34 | **Binder/plugin API stability:** plans 06/07/10/11/12/13/21 each consume `mind-core` internals; 05 §7e requires a documented plugin API. | 05 R1, §7e | 05 publishes the plugin API at M6; later plans consume without editing core files; review at each kickoff. |
| R-35 | **04 `mind-derive` fifth workspace crate** beyond HLP §2.1's four. | 04 R1 | Accepted: proc-macro-only crate, no runtime dependency; update HLP layout note when created. |

### 8.3 Known deviations tracked by this plan

`HIGH_LEVEL_PLAN.md` §9 is the authority; this table adds the plan-level deviations accepted during authoring. Any new deviation must be added to the owning plan's §2/§9 **and** here in the same PR, or `parity gate` ledger review rejects it.

| Area | Deviation | Owner | Tracking |
|---|---|---|---|
| Java JAR mods / Rhino scripts | Cannot run JVM code; JSON data mods + patches are 1:1 | HLP §9, 20 | OD1 |
| Steam Workshop / Discord RPC / Steam networking / achievements | Deferred | HLP §9, 22 | OD4 |
| Android/iOS launchers | Godot export replaces Arc backends; touch parity kept | HLP §9, 22 | OD5 |
| `logicids.dat` | Skipped; lookups by name; null/-1 behavior asserted | HLP §9, 13 R2 | locked |
| Version/build checking | Own scheme; no cross-build with Java clients | HLP §9, 01 | locked |
| `.msav` byte compatibility | Own `MGRS` format by default; importer opt-in | HLP §9, 04 | OD2 |
| Exact float determinism vs Java | Rust↔Rust only; pure-Rust `libm` wrappers for sim math | HLP §9, 10 OD-10-A | NUD-24 |
| Terrain vs Java | Structural parity + Rust-recorded goldens only | HLP §9, 06 OD6-B | accepted |
| Fixed `Time.delta = 1.0` | Java power tests used 0.5; graph APIs take explicit delta; Rust goldens | 05 R7, 09 R6 | accepted |
| Fog background threads | Tick-queued static drain + 40-tick dynamic cadence | 12 R4 | accepted |
| `LogicScript` timeout | Instruction cap replaces wall-clock timeout | 13 R12 | accepted |
| `MessageState` `@wait` blocking | Local client divergence (upstream also client-timed); headless absent | 13 R6 | accepted |
| Logic font metrics | Fallback constants until plan 16 ships real metrics | 13 R4 | R-32 |
| Decal cap | 1024 oldest-drop (Java unbounded) | 17 R-17-8 | accepted |
| Liquid flow cache | Single watched building (Java global static) | 09 R5 | NUD-23 |
| Hash-iteration tie-breaks | First-touch/sorted order replaces Java hash order (Rust↔Rust stable) | 10 R-10-6, 06 R12 | accepted |
| P0 checksum algorithm | xxh3-64 `DUMP_FORMAT` stream superseded by 05's canonical checksum at 05 M8 | 00 §6.4, 05 §6.5 | R-02 |
| Visual FX parity | SSIM-based tolerance; geometry exact | 17 OD-17-A | NUD-37 |

## 9. References

### Project plans and docs read (all required first reads)

- `mindustry-godot/HIGH_LEVEL_PLAN.md` — §0 (D1–D9), §2 (architecture/boundaries/determinism), §3 (plan table incl. 18–22), §4 (template), §5 (phase gates), §6–§9 (conventions, verification tooling, addons, parity ledger), §10 (OD1–OD9).
- `mindustry-godot/PRELIMINARY_PLAN.md`.
- Sibling plans read in full (header, §2, §3, §7, §8; §7/§9 for every one): `00_FOUNDATION`, `01_PLATFORM_STDB`, `02_CONTENT`, `03_ASSETS`, `04_IO_SERIALIZATION`, `05_SIM_CORE`, `06_WORLD_TERRAIN`, `07_BLOCKS_BUILD`, `08_LOGISTICS`, `09_POWER_LIQUIDS_HEAT`, `10_COMBAT_BULLETS`, `11_UNITS_AI_WAVES`, `12_CAMPAIGN`, `13_LOGIC_MLOG`, `14_UI`, `15_INPUT_RTS`, `16_RENDER_WORLD`, `17_FX_PARTS` (all `*_IMPLEMENTATION_PLAN.md` in `C:\Users\Clinton\g\code_examples\mindustry-godot\`). Plans `18`–`22` do not exist on disk; their rows are `TBD by 18…22` and their HIGH_LEVEL scope rows are the only source.
- `mindustry-godot/client/` (plan-00 target layout referenced by every plan), `mindustry-godot/HIGH_LEVEL_PLAN.md` §2.1.

### Mindustry tests and sources

- `Mindustry/tests/AGENTS.md` (read in full): `forkEvery=1`, headless bootstrap, fake time, data dir, power fixture, `.msav` resources, CI command.
- `Mindustry/tests/src/test/java/ApplicationTests.java` (37 annotated test methods + `launchApplication`/`resetWorld`/`updateBlocks`/`depositTest`/helpers); `DataAssetTests.java` (4); `PatcherTests.java` (33); `LogicTests.java` (15); `GenericModTest.java` (helpers only); `ModTestAllure.java` (`begin`); `power/PowerTestFixture.java`, `power/PowerTests.java` (3 entries / 17 dynamic cases), `power/DirectConsumerTests.java` (3), `power/ConsumeGeneratorTests.java` (3 entries).
- `Mindustry/tests/src/test/resources/{77,85,108,114,152,152_be}.msav` (sizes/roles; fixtures for 04).
- `Mindustry/AGENTS.md` (module map, conventions, gotchas).

### Tooling and skills

- `C:\Users\Clinton\g\.opencode\skills\playtest\SKILL.md` — Godot MCP + `spacetime` CLI recipes; launch flow, pid stamps, node-path/coordinate rules; explicitly noted as tuned for `main/` (plan 00 creates the mindustry-godot analog).
- `C:\Users\Clinton\g\.opencode\skills\godot-compositor-testing\SKILL.md` — headless `--import` parse checks, windowed `--capture` pattern, RenderingDevice/render-thread rules, stuck-process handling.
- open-godot-mcp tool surface: `godot_health`, `godot_game`, `godot_input`, `godot_exec`, `godot_runtime_state`, `godot_screenshot`, `godot_log`, `godot_game_time`, `godot_profiler`, `godot_instance`, `godot_editor_edit`.

## Changelog

> Append entries here when execution starts. Every gate claim carries the `parity/reports/gate_Pn.json` path, the T1 report path and screenshot paths.

- 2026-10-01 — Plan authored (v1). Matrix ingested from plans 00–17 §7a; scenario catalog from §7b; MCP catalog from §7c; budgets from §7d; 40 aggregated `NEEDS USER DECISION` rows; 35 cross-plan reconciliation rows; 18 tracked deviations. No milestones executed.
- 2026-10-03 — **lane 23 F17 (`lane/f17-23`) advanced M0/M3 after plans 12/13/14/17 landed.** (1) **Matrix:** moved the 16 `planned` rows to `landed` with the real resolving names — the 15 `LogicTests` groups now point at the ported `logic::tests`/`logic::statement::tests::sanitize_table`/`logic::assembler::tests::empty_string_const` (the CRLF/quoted/lone-CR/label tests share `logic::tests::crlf_does_not_corrupt_tokens`), and `ApplicationTests.testSectorValidity` points at `game::play::tests::sector_capture_flags` (plan 12's aspirational `campaign::tests::presets_rule_validity` was never ported; oracle records the 02/06/11 composition). Result: **100 rows, 91 landed / 9 no-equivalent / 0 planned**, `resolved 91/91 landed` against 1404 test names. (2) **Catalogs:** `scenario_catalog.json` **60 → 107** entries (moved `rules_roundtrip`/`campaign_sector_cycle`/`logic_arith`/`logic_draw`/`ui_text`/`fx_lifecycle` from `planned` to `embedded`, added campaign `objectives_completion`/`play`/`tech`/`turn`/`schematic`/`fog`/`bench`, all M5 logic scenarios (`logic_sensor_access`/`logic_radar_filters`/`logic_unit_control_gating`/`logic_link_sensor`/`logic_save_load`/`logic_draw_headless`), `world_gen_{serpulo,erekir,asteroid,tantros}`, all `ui/*`, `render_darkness_radius`, `audio_policy`, `mods_*`); `golden_manifest.json` **15 → 64** (oracle + scenario + **49 harness** goldens pinned by sha256, including `campaign/*`, `logic`/`ui` goldens, `world_gen_*.checksum`, `fx/*`, `render/*`, `units/*`, `network/*`, `audio/*`); `bench_budgets.json` **30 → 40** rows (added plan 12 campaign p99s, plan 17 fx, plan 06 serpulo/erekir/asteroid world-gen); `checksum_registry.json` reviewed (no contributor change → v1 stays; `rules`/`logic_rng` still unfolded, notes updated). (3) **CLI:** `mind-headless list [--json] [--tier T]` (catalog-driven; 18 T0 entries) and `run-all [--tier T]` (T0: 4/4) landed; new `parity::scenario::tests::every_registered_file_scenario_is_catalogued`. `bench-gate`/gate nightly steps unchanged (deferred per NUD-40/A). **Evidence:** `parity check --tests /tmp/tests.txt` → 6/6 PASS; `cargo test -p mind-headless` **75 lib** + 4 integration (1 logistics + 2 fx + 1 audio) = 79 passed; workspace clippy `-D warnings` + fmt clean. CI wiring proposal refreshed in §7f. **Deferred:** `parity run --suite`, nightly T1/checksum/MCP/bench jobs, MCP editor runs (single-editor mutex), plan-15 matrix/catalog rows.

- 2026-10-03 — **lane 23 M0 harness landed (branch `lane/23-parity`).** Added the `parity/` registry set: `matrix.toml` (**100 upstream-test rows** for `ApplicationTests`/`DataAssetTests`/`PatcherTests`/`LogicTests`/`power/*`/network-gated mod tests; **75 landed** resolved against the 966 `mind-core` lib tests, **9 `no-equivalent`** with substitute oracles, **16 `planned`** for unlanded systems 12/13), `checksum_registry.json` (v1, mirrors `CHECKSUM_VERSION=1`), `scenario_catalog.json` (**60 entries**: 11 file + 38 embedded + 11 planned), `golden_manifest.json` (**15 sha256-pinned** oracle/scenario goldens), `bench_budgets.json` (**30 rows** across 15 plans, canonical `sim_core_mid.tick_p95_ms ≤ 4.0 ms`, NUD-39/A), `mcp_catalog.json` (**22 phase-grouped** entries), `soak.toml` (mid/stress/windowed/multiplayer), `upstream.lock` (commit `2cd7aeec…`, JDK 17.0.20), `README.md` + `system_checklist.md`, `reports/.gitignore`. Harness: `mind-headless parity {check,matrix,registry,goldens,budgets,scenarios,mcp,soak,report,gate,bench-gate}` (`client/rust/mind-headless/src/parity/**`, 23 unit tests). **Resolved the plan-09 `liquidRouterOutputAll` TBD** → `world::blocks::liquid::tests::router_dumps_current`. Additive shared edits: `mind-headless/Cargo.toml` (`sha2.workspace = true`), `mind-headless/src/lib.rs` (`pub mod parity;`), `src/cli.rs` (+`ParityCommand`), `src/exec.rs` (+dispatch). CI wiring is proposed (not applied) in §7f. **Evidence:** `parity check --tests out/tests.txt` → 6/6 checks PASS; `cargo test -p mind-headless` 59 passed; full `cargo clippy --workspace --all-targets -- -D warnings` + `cargo fmt --all -- --check` clean. **Deferred:** `list --json`/`run-all`/registry tier+plan fields; nightly T1/checksum/MCP/bench gate steps; MCP editor runs (single-editor mutex); porting LogicTests/DataAsset end-to-end rows to real tests (systems 13/12 not landed).

- 2026-10-03 — **lane 23 F19 (`lane/f19-23`, base `main` @ `e1fd7bc`; commits `e29f5f2` + docs) finished the M5/M6 headless halves.** (1) **M5 soak:** `parity soak --profile mid|stress [--minutes N|--ticks N] [--seed S] [--json]` executes a real `Sim` (flat grid + placed walls) with RSS/alloc/checksum tracking against `parity/soak.toml`, replaying the first checkpoint from a fresh same-seed build (prefix stability); RSS growth/alloc deltas gate against the profile's `max_rss_growth_pct`/`max_alloc_delta`. Bounded evidence: `mid --ticks 600` → 11 ms loop / checksum `840324407d42c529` / RSS growth 3.09% ≤ 5%; `mid --ticks 3600` → 59 ms / `3521103619818dcc`. Bounded + seed-determinism + deferred-profile + zero-budget tests added (~0.1 s). (2) **M5 desync:** the reference detector double now seeds expected checksums/content hash from the **host** stream before observing the peer stream (fixing `content_hash_mismatch`); all **8** cases yield their documented verdicts (`parity desync-inject --case all` PASS). The production plan-21 detector swaps in behind `DesyncDetector` with no case change. (3) **M6 MCP:** `parity mcp-parity --suite T0|all` resolves every catalog entry against its committed golden, reports `planned` scenarios `deferred` (not failed) and embedded entries through their command — **22/22 PASS** (in-engine half deferred). New tolerance diff `parity screenshots --diff-a/--diff-b` (`DiffTolerance` ≤12/255/channel, ≤0.5% changed pixels; plan-17 OD-17-A / NUD-37), tested on synthetic Pixmaps and a lossless PNG round-trip. (4) **M6 suite:** `parity run --suite smoke|gate|full [--phase Pn] [--json]` runs the file-backed catalog tier in-process and emits one clean `format:1` report (quiet per-scenario pass) — smoke **4/4**, gate **6/6**, unknown suite exit 2. (5) **Registries:** plan-15 `input_place_line_headless`/`placement_validation_table` promoted `planned → embedded`; `golden_manifest.json` **64 → 73** (4 plan-14 UI + 5 plan-15 input harness goldens pinned by sha256; no plan-13 logic golden files exist — its goldens are in-memory checksums). **Evidence:** `parity check --tests out/tests.txt` 6/6 (matrix 100 / 91 landed / 9 no-equivalent, registry 7, scenarios 107, goldens 73, budgets 40, mcp 22); `cargo fmt --all -- --check` clean; workspace `clippy --all-targets -D warnings` clean; `cargo test -p mind-headless` **101 lib + 4 integration = 105 passed / 0 failed**. **Deferred:** nightly bench/cross-OS checksum/editor MCP runs (single-editor mutex, NUD-40/A).

- 2026-10-03 — **lane 23 M7 (`lane/f21-23`, base `main` @ `d85e49f`): continuous maintenance + F20 registrations.** (1) **Registrations (F20 wave):** `scenario_catalog.json` **107 → 133** entries — plan 15 promoted its 9 landed `input scenario` goldens to `embedded` (`input_focus_guards`, `input_replay_mobile`, `input_mobile_parity`, `input_rts_move`, `input_camera_{pan_zoom,clamp,shake}`, `input_plan_snapshot`, `input_preview_handoff`); plan 19 replaced the lone `planned` `editor_roundtrip` with **7 embedded** scenarios (`editor_ops` with its committed fixture path, `editor_roundtrip`, `editor_resize_shift`, `maps_{save_load_save,preview_tiles,image_roundtrip,registry_shuffle}`); plan 21 added the 3 landed `mp_*` scenarios (`mp_lobby_join`, `mp_command_log_replay`, `mp_validation_matrix`); plan 13 added the 4 remaining `logic_*` scenarios (`logic_globals_live`, `logic_markers_smoke`, `logic_privileged_world`, `logic_sync_event`); plan 14 added `ui_{campaign,chat_console,display,file_chooser}`. `golden_manifest.json` **73 → 82** — 8 plan-15 input harness goldens (`input_camera_*`, `input_plan_snapshot`, `input_preview_handoff`, `input_replay_mobile`, `input_mobile_parity`, `input_rts_move`) + plan-11 `units_base_build.json` pinned by sha256 (closes the last unmanifested harness files). `mcp_catalog.json` **22 → 26** — plan 19/20 entries and the five plan-21 §7c `mp_*` scenarios filled from their plans (in-engine half still `deferred`, NUD-40/A). (2) **Matrix:** the 100-row upstream JUnit baseline is unchanged — plans 15/19/21 publish no upstream tests and the plan-20 `DataAssetTests`/`PatcherTests` rows (37) already resolve; `parity check --tests` **resolves 91/91 landed rows against 1639 test names** (100 rows: 91 landed / 9 no-equivalent / 0 planned/tbd). (3) **Phase gates:** `parity gate --phase Pn --out parity/reports/gate_Pn.json` run for **P0–P8**; every report is `pass: true` with the T1 suite, checksum matrix, in-engine MCP subset, bench recording and ledger check reported `deferred` (owners `tools/ci.sh`/nightly, never faked). (4) **Wrapper:** new `tools/parity.sh` (+ `.ps1` WSL twin) local suite driver (`--check`, `--suite smoke|gate|full`, `--phase Pn`, `--gate Pn`, `--mcp`, `--bench`, `--soak <profile>`, `--json`). **Evidence:** `parity check --tests` 6/6 PASS (matrix 100, scenario_catalog 133, golden_manifest 82, bench_budgets 40, mcp_catalog 26); `parity goldens` 82/82 PASS; `parity mcp` 26/26; `parity mcp-parity --suite all` 26/26 PASS; `parity run --suite smoke` 4/4; `parity bench-gate` PASS; `cargo fmt --all -- --check` clean; workspace `cargo clippy --all-targets -- -D warnings` clean; `cargo test -p mind-headless` **106 passed** (101 lib + 5 golden). **Deferred (unchanged owners):** nightly T1/T2 suite, checksum matrix, in-engine MCP capture and bench recording (NUD-40/A); `parity report --all` final roll-up.
