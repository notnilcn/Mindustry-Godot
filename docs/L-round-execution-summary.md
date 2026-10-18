<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Round L — coordinator execution summary (paused after wave 4 / session-B wave 3)

Round L ran as two parallel sessions per `docs/parity-plans/L-INDEX.md` §2:
- session A = UI + harness, worktree `../Mindustry-Godot-session2`, display :11, bridge 6980;
- session B = Rust, worktree `../Mindustry-Godot-session3`, display :12, bridge 6990.

The coordinator merged each wave into `main` and applied the ledger changes to `chains.json`. The operator
paused the round after session A's wave 4 and session B's wave 3. Everything below is on `main`; both session
branches are fast-forwarded to `main` and both sessions are stopped.

## 1. Status per plan

| Plan | Done | Remaining |
|---|---|---|
| L01 layout engine | T1–T4 (final summary) | exhaustive double-open/orientation sweep → L08 T6 |
| L02 world/state | T1–T5 (final summary) | — |
| L03 skin | T1–T5 (final summary) | PAR-0420 orbit like-for-like frame → L08 T6 |
| L04 unit/FX draw | — | **all: T0 Java staging batch, T1–T6** (no summary yet) |
| L05 block UI/info | T2 partial, T3, T4, T5 | **T1 hover bars (session B)**; T2 TargetDummy + CoreBlock toggles (need session-B sim support, see §3) |
| L06 sim/campaign | T1–T7 (final summary) | T7 residue in PAR-0296 (§3) |
| L07 editor/logic | T1–T3 | **T4 (session B: sim_host.rs), T5, T6 (+PAR-0433)** |
| L08 mods/goldens/closeout | T1 (incl. the batched regen), T5 | **T2 ModDataManager (PAR-0394), T3 mod parser TODOs, T4 evidence debt, T6 round-M closing walk** |

Per-plan detail: `docs/L01-…`, `L02-…`, `L03-…`, `L05-…`, `L06-…`, `L07-…`, `L08-…_summary.md`.

## 2. What the next session should run (remaining schedule)

The waves that were not run, in L-INDEX order (ownership rules of L-INDEX §2 still hold):

| Wave | Session A — UI + harness | Session B — Rust |
|---|---|---|
| 3 (rest) | — | L04 T0 (stage every L04 Java capture in one batch), L04 T5 (`ExplosionSink` host, PAR-0399) |
| 4 (rest) | — | L04 T1–T2 → L05 T1 (hover bars, PAR-0379) + the L05 T2 sim support (§3) |
| 5 | L07 T5–T6 (+PAR-0433), the accelerator tap hook (§3), L05 T2 CoreBlock/TargetDummy UI once B's sim half lands | L08 T2–T4, L07 T4, L04 T3/T4/T6 |
| last | L08 T6 (round-M closing twin walk → `docs/M-round-twin-verification.md`) | — |

## 3. Open cross-session hand-offs

- **PAR-0296 accelerator tap (session A, `input/desktop.rs` `config_spec`):** an accelerator tap still opens the
  generic item panel. Hook: `if def.kind == BlockKind::Accelerator { drop(guard); host.bind_mut().accelerator_tapped(x, y); return None; }`.
  Also open on PAR-0296: Java capture needs a throwaway campaign profile (L-environment rule 7), and the unported
  launch visuals (lightning, dust rings, drawLaunch rings/clouds, white fade, info text).
- **PAR-0380 CoreBlock team toggles (session B, `sim_host.rs`):** with `rules.core_build_and_config`, configuring a
  core with an Int sets the player team (`CoreBlock.java:321-330`); then session A adds the six buttons to `config_spec`.
- **PAR-0380 TargetDummy (session B):** an int-array determinism config type (`mind-core/src/determinism/command.rs`),
  `sim_host.rs configure_building` parsing, `TargetDummyState` `boosting`/`dummy_size`/`unit_team` + the 5-int handler,
  the tethered dummy unit; then session A builds the panel.
- **PAR-0426 / PAR-0435:** Arc shrinks rows toward their minimum when a full-screen dialog overflows; `MindTable`
  has no min/pref interpolation (layout engine, session A).
- **PAR-0447 (session B, `maps/filters/mod.rs`):** `block_info::is_overlay`/`is_floor` omit SpawnBlock and
  RemoveOre (Java: OverlayFloor/Floor). The editor palette no longer uses them; check the map filters.
- **PAR-0446 (session B):** unit-cargo-unload-point and the Erekir assemblers lack `solid = true`
  (`UnitCargoUnloadPoint.java:26`, `UnitAssembler.java:62`); a sim/pathing change, so it was left for B.
- **PAR-0445 (session A):** the Save Game dialog should use the Load-dialog layout (`runs/20261018-s2-L07-java/java/savedialog.png`).

## 4. Ledger (applied to `chains.json` by the coordinator)

Coordinator commits on `main` carry each batch (`Coordinator: round-L …`); the op files are in
`runs/20261018-L-coordinator/ledger-*.json`, applied with `runs/20261018-L-coordinator/ledger.py` (line-level
edits that keep `chains.json`'s hand layout, then re-validate the JSON).

- Closed (verified): PAR-0301, 0362, 0377, 0383, 0386, 0391, 0402, 0410, 0411, 0413, 0414, 0416, 0417, 0418,
  0419, 0421, 0422, 0423, 0424, 0425, 0427, 0428, 0429, 0430, 0437, 0440, 0441; wave 4: 0110, 0302, 0431, 0432, 0434, 0438, 0444.
- Closed (wontfix): PAR-0378 (Java behaviour), 0409 (harness artefact), 0412, 0415 (not reproduced).
- Fixing: PAR-0296 (accelerator residue), 0380 (CoreBlock/TargetDummy), 0420 (orbit like-for-like), plus the
  pre-round PAR-0110, 0126, 0363, 0408.
- New this round and still open: PAR-0426, 0431–0436, 0438, 0439, 0442, 0443, 0445, 0446, 0447; PAR-0407 (L07 umbrella) stays seeded with T4–T6 open.

## 5. Gates on `main` at the pause

Run by the coordinator on the merged tree (b8b251cf + the rustfmt fix 37b7e4af):

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | clean (after 37b7e4af) |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo test -p mind-core` | 2798 passed, 0 failed, 4 ignored (planet checksums regenerated in the wave-3 batch) |
| `cargo test -p mind-headless` / `-p mind-gdext` | all pass / 77 pass |
| `mind-headless parity goldens` | 0 FAIL |
| `tools/regen_goldens.sh --check` | 0 FAIL |
| Godot import gate | exit 0 |
| `ui_layout_check.gd` / `ui_widgets_check.gd` | failed=0 / failed=0 (both in `tools/ci.sh`) |
| `shaders_check.gd` | total=29 failed=0 |

Not run: the full `tools/ci.sh` (release bench, STDB typecheck). Merge conflict resolved by the coordinator:
`parity/golden_manifest.json` `content_ids` (both sessions changed `golden_content.json`; the merged file's hash was
re-recorded with `regen_goldens.sh --record content_ids` and both reasons kept).

## 6. Process notes for the next coordinator

- The session brief is `runs/20261018-L-coordinator/brief-common.md`. It is binding for session agents: ledger
  rules, commit format, Java access, build flock, summaries, wave-report format, context budget. `J-environment.md`
  is copied to `runs/20261018-L-coordinator/J-environment.md`. Only the main checkout's working-tree
  `archived_plans_do_not_read.zip` has it; the committed zip in the worktrees does not.
- Session agents that carried context across waves ran out of context (session B in wave 3). Start a fresh agent
  per wave with a compact handoff (worktree, branch head, commits so far, ledger ids) instead of resuming one.
- Harness learned this round (learnings files, 2026-10-18): Godot text `tools/gmcp.py --text` / twin `GT`; Java
  text `tools/java_xkeys.py` / twin `JT` (XTEST keys never reach Java on the session Xvfb); `JW` hovers 0.4 s before
  clicking; `GC`/`GW` move the real pointer; in-game Java frames need `backgroundpause` flipped offline in the
  session `parity-java-home` with the original `settings.bin` restored afterwards.
- The session Godot profiles carry walk state: session 3's has a staged accelerator in Ground Zero, a bogus Frozen
  Forest save (holds Ground Zero's world) and launch-founded sectors. Normalise before campaign comparisons.
- `pkill -f "<path>"` from a shell whose own command line contains the path kills that shell; use `pgrep` + `kill`.
- Spine golden checksum is now `3e48c583a94fbea4` (ddeae35d, `TeamId::SHARDED` = 1). The copy of the playtest skill
  at `~/g/.opencode/skills/playtest/SKILL.md` still says `a1a7b96167c9718d` (outside this repo; not edited).
- Uncommitted in the main checkout on purpose: the operator's archival deletions of the round-I/J/K summaries and
  plans plus the modified `archived_plans_do_not_read.zip` (left for the operator to commit).


---

## 7. Continuation after the pause (later 2026-10-18 session)

Start point for the next session: `docs/L-round-continuation-handoff.md`.

The paused backlog was continued on `main` in the coordinator's working tree.
No new worktrees or Java captures were created; verification used `mind-core`
tests, `mind-gdext` tests, the Godot import gate and live state probes on
display `:10`. The operator asked to stop rebuilding mid-way, so the last
source-only delta (the generator base-power-row gate) is tests/check clean but
was not re-smoked in the running `.so`.

### Landed

| Area | Result | Evidence |
|---|---|---|
| PAR-0446 | `solid = true` for `unit-cargo-unload-point` + tank/ship/mech assemblers (`UnitCargoUnloadPoint.java:26`, `UnitAssembler.java:62`); content golden re-recorded with reason | content test + `content::parity` |
| PAR-0447 | map-filter `is_overlay`/`is_floor` include `SpawnBlock`/`RemoveOre` (`SpawnBlock.java:6`, `RemoveOre.java:11`) | `spawn_and_remove_ore_are_floor_overlays` |
| L05 T1 (PAR-0379) | all remaining hover-bar families + multi-arg labels; base power row only for consumers | `bar_rows_for_the_remaining_families`, 80 gdext tests |
| L05 T2 (PAR-0380) | `int[]` config wire (determinism/relay/TypeIO), TargetDummy state/config/tether unit, CoreBlock team config + spec, both GDScript panels | two new mind-core tests; live `building_config_json`/`config_spec_json` probes |
| L04 T5 (PAR-0399) | `ExplosionSink` host (effects/shake/scorch/sound) installed on every world load; reactor effect/sound fields filled; `Fx.generatespark` seam | explosion + sink tests |
| L07 T4 | logic font glyph rasteriser + `unicodeToName`/`getLargeIcon` icon tag | code; live `logic_font`/`icon_name_for_code` probes |
| L07 T5 partial | 480 px canvas, PNG import/export, 2 s auto-save, canvas blending mask | canvas test + GDScript parse gate |
| L07 T6 partial | permanent-status `seconds` hiding, unit `@id` NaN, boss radar | code/tests |
| L08 T3 partial | `drillMultipliers` parsing | code only |
| PAR-0296 | accelerator tap now opens the planet launch page instead of the item panel (`Accelerator.java:258-261`) | code |

### Still open

- L04 T0–T4/T6 (unit parts, abilities, crawl/payload/layerOffset, turret trees,
  live leg pairs).
- L05 T2: the live core team change did not move in the sandbox because the
  rule mirror was not applied to the running `BuildRules`; the Sim test covers
  the apply path.
- L07 T5 off-screen display pre-pass + canvas edge rendering; L07 T6 pickers.
- L08 T2 (`ModDataManager`), T4 (Java evidence), T6 (round-M walk).
- PAR-0445 Save Game dialog layout.
- PAR-0420 orbit like-for-like frame (code was already fixed in L03).

### Gates run

```
cargo test -p mind-core --lib                 # 2803 passed, 0 failed, 4 ignored
cargo test -p mind-gdext --lib                # 80 passed, 0 failed
cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings  # clean
cargo fmt --all -- --check                    # clean
bash tools/godot.sh --headless --editor --quit --path client          # no script errors
ui_layout_check.gd / ui_widgets_check.gd      # failed=0 / failed=0
```

### Ledger changes to apply

| Item | Change |
|---|---|
| PAR-0446 | close (solid flags; content golden re-recorded) |
| PAR-0447 | close (filters include SpawnBlock/RemoveOre) |
| PAR-0379 | close for the enumerated families; record the payload-loader progress approximation and the unpaired Java hover frames |
| PAR-0380 | complete TargetDummy + CoreBlock halves; keep the live core-rule mirror check |
| PAR-0399 | close (host installed + tests); keep the Java meltdown pair as evidence debt |
| PAR-0407 | T4 landed; T5/T6 partial as listed |
| PAR-0296 | tap hook landed; launch visuals and the Java capture remain |
| PAR-0126 | still fixing (live leg pair) |

---

## 8. Continuation — L04 draw and L07/PAR-0445 padding (2026-10-18, later session)

One build, one editor session (display :10); no Java captures (the Java reference
staging remains the blocker for the pairs).

### Landed

| Area | Result | Evidence |
|---|---|---|
| L04 T1 (PAR-0365) | `UnitDraw` gained `parts`/`fin`/`draw_body`/`draw_cell`/`ground_layer` and `WeaponDraw` the resolved def + mount; new `render/unit_parts.rs` (three programs per band: under / over / additive) draws unit `UnitType.parts` and weapon under/over parts through the shared `PartEmit`; `units.rs` gates body/cell | code + live sandbox frames (quasar/quell/tecta/merui) |
| L04 T2 (PAR-0366) | ForceField/ShieldArc in `shields.rs`, EnergyField/SuppressionField in new `render/unit_abilities.rs`; `AbilityState` view mirrors (`cur_stroke`, `radius_scale`, `alpha`) advanced in the sim; `AbilitySpec` draw knobs; `ability_view_state_lerps` test | code + tests + live frames |
| L04 T3 (partial) | weapon-part `layerOffset` through the parts pass; unit-part polygons filtered for degenerate triangulation | code |
| L04 T4 | `HaloPart` kind/fields + `SinTime` progress; `lustre`/`scathe` (+ammo missiles)/`smite`/`malign` turret trees ported | `turret_parts_cover_the_vanilla_drawers`; live frames still need T0 |
| L07 T5 (edge/pre-pass) | canvas `side1`/`side2`/`corner1`/`corner2` edge sprites incl. y-flipped corners; recursive `draw image` source pre-pass in `process_display` | code |
| PAR-0445 | `SaveDialog` now extends `LoadDialog` (search, filters, card grid, per-card actions) and puts `@save.new` in the button row | live `runs/20261018-L-continue/live/savedialog2.png`; `ui_widgets_check` |

### Still open in L04/L07

- L04 T0 Java staging; T1/T2 Java pairs; T3 crawl segments + payload composition
  + hand-drawn weapon z; T4 turret part trees (HaloPart); T6 live leg pairs.
- L07 T5 canvas off-screen image pre-pass is landed; the remaining T6 icon
  pickers/enum columns/drag-follow are unchanged.
- L08 T2/T4/T6 were not started in this session.

### Gates run

```
cargo test -p mind-core --lib        # 2805 passed, 0 failed, 4 ignored
cargo test -p mind-gdext --lib       # 80 passed
cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings  # clean
cargo fmt --all -- --check           # clean
mind-headless parity goldens         # 0 FAIL
godot --headless --editor --quit     # exit 0
ui_layout_check.gd                   # failed=0
ui_widgets_check.gd                  # failed=0
```

Live probes also confirmed no `Invalid polygon data` after the degenerate-fill
filter; an MCP `open_dialog` eval produced one pre-existing
`MindWorldRenderer` bind_mut panic (no repro on the next call) recorded as
harness interaction, not a render-pass failure.
