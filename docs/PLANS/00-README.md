<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Refactor programme — readable, maintainable, still 1:1

**Goal.** Split the god-files, extract the repeated boilerplate into helpers, and move business
logic that leaked into the Godot facades back into `mind-core`. The game must behave exactly as
it does today. Parity with upstream Mindustry (`/home/clintonnguyen/g/code_examples/Mindustry`)
is the product, so **a refactor package never changes behaviour**. A bug found along the way
goes into [`02-FINDINGS.md`](02-FINDINGS.md), not into the refactor commit.

**Plan base.** Every line number in these plans is at commit **`291e30a8`** (2026-11-09).
`main` moves fast (about 70 commits a week), so read a line range as a *hint*. Locate code by
the item name, and view the original with `git show 291e30a8:<path> | sed -n 'A,Bp'`.

**Who executes.** Each `W*-*.md` file is one **work package**. Hand it to one agent together with
[`01-RULES.md`](01-RULES.md), which every package assumes. The packages are written for an
agent that follows instructions literally: they name each file, item and check.

## The waves

A wave starts only after **every** package of the previous wave has merged into `main`.
Exceptions are noted per package. The packages inside one wave own **disjoint files**, so any
subset of a wave can run concurrently and merge in any order. The CPU queue
(`tools/run-cpu-intensive-task.sh`) serializes builds across worktrees anyway. Run **at most 4–6
agents at once**; more agents just queue behind the lock. Start with the campaign packages
(listed first in each wave).

| Wave | Kind | Packages | Parallel? |
|---|---|---|---|
| **W0** | Foundations: tools, baseline, scaffolds, shared helper modules | F0 → then F1, F2, F3 | F0 first, alone; then F1–F3 in parallel |
| **W1** | **Pure mechanical splits** (verbatim moves, visibility tweaks, re-exports) | S01–S19 | all parallel |
| **W2** | Helpers and dedupe inside the newly split areas; new `mind-core` campaign APIs (additive) | H01–H09 | all parallel |
| **W3** | Campaign facade goes thin (adopts the H01/H02 APIs, decomposes the giant fns); `unit_update_system` arm extraction | X01, X02 | parallel; X01 may start once H01–H03 merge, X02 once H08 merges |
| **W4** | Typed campaign view-models replace the in-crate JSON round trip | T01 → T02 | serial |
| **W5** | Cleanup: delete shims, promote helpers, update docs and ledgers, hand off to the CI fixer | C01 | alone |

### Package index

| ID | File | Owns (summary) |
|---|---|---|
| F0 | [W0-F0-baseline-tools.md](W0-F0-baseline-tools.md) | `tools/refactor/*` |
| F1 | [W0-F1-gdext-shared-helpers.md](W0-F1-gdext-shared-helpers.md) | `mind-gdext/src/gdutil/**` (new), `ui/kit/**` additions, `ui/dialog.rs` additions |
| F2 | [W0-F2-dialogs-scaffold.md](W0-F2-dialogs-scaffold.md) | `ui/dialogs/mod.rs`, every new dialog group `mod.rs` + stub leaves, `ui/dialogs/common/**`, cross-batch item moves |
| F3 | [W0-F3-core-test-support.md](W0-F3-core-test-support.md) | `mind-core/src/sim/test_support.rs`, `mind-core/src/game/test_support.rs` (new), fixture moves |
| **S01** | [W1-S01-campaign-facade-split.md](W1-S01-campaign-facade-split.md) | `mind-gdext/src/campaign.rs` → `campaign/` |
| **S02** | [W1-S02-campaign-core-split.md](W1-S02-campaign-core-split.md) | `mind-core/src/game/runtime.rs` → `game/runtime/`; `mind-core/src/ui/campaign.rs` block catalog → `ui/block_catalog.rs` |
| **S03** | [W1-S03-campaign-dialogs-split.md](W1-S03-campaign-dialogs-split.md) | `ui/dialogs/batch_campaign2.rs`, `ui/dialogs/sector.rs` |
| **S04** | [W1-S04-planet-dialog-split.md](W1-S04-planet-dialog-split.md) | `ui/dialogs/batch_c.rs`, `ui/planet.rs` |
| S05 | [W1-S05-sim-host-split.md](W1-S05-sim-host-split.md) | `mind-gdext/src/sim_host.rs` → `sim_host/` |
| S06 | [W1-S06-dialogs-settings-database-menu.md](W1-S06-dialogs-settings-database-menu.md) | `batch_a.rs`, `simple.rs` |
| S07 | [W1-S07-dialogs-game-net.md](W1-S07-dialogs-game-net.md) | `batch_b.rs`, `batch_h.rs`, `misc.rs` |
| S08 | [W1-S08-dialogs-editor-a.md](W1-S08-dialogs-editor-a.md) | `batch_d.rs`, `batch_e.rs` |
| S09 | [W1-S09-dialogs-editor-b-rules.md](W1-S09-dialogs-editor-b-rules.md) | `batch_f.rs`, `batch_i.rs` |
| S10 | [W1-S10-dialogs-mods-logic.md](W1-S10-dialogs-mods-logic.md) | `batch_g.rs`, `batch_j.rs` |
| S11 | [W1-S11-unit-runtime-split.md](W1-S11-unit-runtime-split.md) | `mind-core/src/sim/unit_runtime.rs` |
| S12 | [W1-S12-sim-spine-runtime-builder-split.md](W1-S12-sim-spine-runtime-builder-split.md) | `sim/mod.rs` (bodies), `sim/runtime.rs`, `sim/builder.rs` |
| S13 | [W1-S13-combat-tests-split.md](W1-S13-combat-tests-split.md) | `sim/combat_tests.rs` |
| S14 | [W1-S14-production-logic-tests-split.md](W1-S14-production-logic-tests-split.md) | `sim/production_tests.rs`, `sim/logic_campaign_tests.rs` |
| S15 | [W1-S15-headless-split.md](W1-S15-headless-split.md) | `mind-headless/src/{exec.rs,cli.rs,server/commands.rs}` |
| S16 | [W1-S16-mods-split.md](W1-S16-mods-split.md) | `mind-core/src/mods/{json,patch}/mod.rs` |
| S17 | [W1-S17-content-world-io-split.md](W1-S17-content-world-io-split.md) | blocks registry `mod.rs`, turrets `mod.rs`, `io/sim_io.rs` |
| S18 | [W1-S18-input-split.md](W1-S18-input-split.md) | `mind-gdext/src/input/{desktop.rs,mod.rs}` |
| S19 | [W1-S19-net-root-editor-split.md](W1-S19-net-root-editor-split.md) | `net/mod.rs`, `ui/root.rs`, `ui/editor_views.rs`, `editor/mod.rs` |
| **H01** | [W2-H01-campaign-core-api-universe.md](W2-H01-campaign-core-api-universe.md) | `game/{universe,sector,saves,campaign_rules,planet}.rs` |
| **H02** | [W2-H02-campaign-core-api-tech-play.md](W2-H02-campaign-core-api-tech-play.md) | `game/{tech_tree,play}.rs`, `game/launch.rs` (new), `game/runtime/**`, `game.rs` |
| **H03** | [W2-H03-campaign-facade-helpers.md](W2-H03-campaign-facade-helpers.md) | `mind-gdext/src/campaign/**` |
| **H04** | [W2-H04-campaign-dialogs-helpers.md](W2-H04-campaign-dialogs-helpers.md) | `dialogs/campaign/**` (minus 2 files), `dialogs/maps/**`, `dialogs/schematics/` (minus 1), `ui/planet/**` |
| H05 | [W2-H05-dialogs-helpers.md](W2-H05-dialogs-helpers.md) | four sub-packages H05a–d, one per remaining dialog group |
| H06 | [W2-H06-sim-host-helpers.md](W2-H06-sim-host-helpers.md) | `mind-gdext/src/sim_host/**` |
| H07 | [W2-H07-sim-tests-adopt-support.md](W2-H07-sim-tests-adopt-support.md) | every sim test file + `sim/test_support.rs` |
| H08 | [W2-H08-sim-safe-dedupe.md](W2-H08-sim-safe-dedupe.md) | non-test files of `sim/{unit_runtime,runtime,builder,spine}/`, `sim/mod.rs` |
| H09 | [W2-H09-other-helpers.md](W2-H09-other-helpers.md) | five sub-packages H09a–e (headless, mods, content/world, input, net/root/editor) |
| **X01** | [W3-X01-campaign-facade-thin.md](W3-X01-campaign-facade-thin.md) | `mind-gdext/src/campaign/**`, `mind-core/src/game/**` |
| X02 | [W3-X02-unit-update-dispatch.md](W3-X02-unit-update-dispatch.md) | `sim/unit_runtime/update.rs` (+ new `ai_arms/`) |
| **T01** | [W4-T01-campaign-typed-views.md](W4-T01-campaign-typed-views.md) | `mind-core/src/ui/campaign*`, `mind-gdext/src/campaign/**` |
| **T02** | [W4-T02-campaign-dialogs-typed.md](W4-T02-campaign-dialogs-typed.md) | campaign dialog consumers, `ui/planet/**`, research widgets, `ui/dialog.rs` |
| C01 | [W5-C01-cleanup.md](W5-C01-cleanup.md) | shims, docs, ledgers |

Supporting files: [`01-RULES.md`](01-RULES.md) holds the rules every agent follows.
[`W1-DIALOGS-COMMON.md`](W1-DIALOGS-COMMON.md) is the shared procedure for the dialog splits (S03, S04, S06–S10); hand it to those agents too.
[`data/S05-sim-host-methods.tsv`](data/S05-sim-host-methods.tsv) maps each `sim_host.rs` method to its destination file.
[`02-FINDINGS.md`](02-FINDINGS.md) lists the bugs found during analysis, which are out of scope
here. `moves/` and `reports/` are where the packages write their outputs (see the rules).

## File-ownership matrix (conflict guard)

A path is owned by **exactly one package per wave**. Before you edit a file, check that your
package owns it. If a fix needs a file you do not own, **stop** and write the need into your
report; do not edit it.

Paths below are relative to `client/rust/`. `dlg/` means `mind-gdext/src/ui/dialogs/`.

| Path | W0 | W1 | W2 | W3 | W4 | W5 |
|---|---|---|---|---|---|---|
| `tools/refactor/**` | F0 | – | – | – | – | C01 |
| `mind-gdext/src/gdutil/**`, `ui/kit/**`, `mind-gdext/src/lib.rs` (one `mod gdutil;` line), `mind-gdext/Cargo.toml` (one `serde` dep line) | F1 | – | – | – | T02 (`kit/widgets/research_*`) | C01 |
| `mind-gdext/src/ui/dialog.rs` | F1 | – | – | – | T02 | C01 |
| `dlg/mod.rs`, `dlg/<group>/mod.rs`, `dlg/common/**` | F2 | single-owner groups only: add `pub mod` lines (see W1-DIALOGS-COMMON) | same, per H05 sub-package | – | – | C01 |
| `mind-core/src/sim/test_support.rs`, `game/test_support.rs`, `game.rs` | F3 (new files + one `mod` line each in `sim/mod.rs`, `game.rs`) | – | H07 (sim), H01 (game test_support), H02 (`game.rs`) | X01 (`game/**`) | – | C01 |
| `mind-gdext/src/campaign*` | – | S01 | H03 | X01 | T01 | C01 |
| `mind-core/src/game/runtime*` | – | S02 | H02 | X01 | – | C01 |
| `mind-core/src/game/{universe,sector,saves,campaign_rules,planet}.rs` | – | – | H01 | X01 | – | C01 |
| `mind-core/src/game/{tech_tree,play}.rs`, `game/launch.rs`, `game.rs` | – | – | H02 | X01 | – | C01 |
| `mind-core/src/ui/campaign.rs`, `ui/block_catalog.rs`, `ui/mod.rs` | – | S02 | – | – | T01 | C01 |
| `dlg/batch_campaign2.rs`, `dlg/sector.rs` | – | S03 | – | – | – | C01 |
| `dlg/campaign/{util,research,research_root_select,loadout,launch_loadout,sector_stats,sector_select}.rs`, `dlg/maps/**`, `dlg/schematics/{schematics_dialog,files,subdialogs,import_export}.rs`, `dlg/pickers/palette.rs` | (stubs F2) | S03 | H04 | – | T02 | C01 |
| `dlg/batch_c.rs`, `dlg/campaign/planet/**`, `mind-gdext/src/ui/planet*` | (stub F2) | S04 | H04 | – | T02 | C01 |
| `mind-gdext/src/sim_host*` | – | S05 | H06 | – | – | C01 |
| `dlg/batch_a.rs`, `dlg/simple.rs`, `dlg/{settings,database,menu}/**` | (stubs F2) | S06 | H05a | – | – | C01 |
| `dlg/batch_b.rs`, `dlg/batch_h.rs`, `dlg/misc.rs`, `dlg/{game,net}/**`, `dlg/pickers/list_select.rs`, `dlg/campaign/{campaign_rules,campaign_complete}.rs` | (stubs F2) | S07 | H05b | – | – | C01 |
| `dlg/batch_d.rs`, `dlg/batch_e.rs`, `dlg/editor/{editor_import,editor_export,editor_menu,editor_map,map_info,map_resize,map_generate,wave_info,map_objectives,map_locales,util_d,util_e}.rs`, `dlg/pickers/{file_chooser,icon_select}.rs`, `dlg/dev/**` | (stubs F2) | S08 | H05c (editor + pickers), H05a (dev) | – | – | C01 |
| `dlg/batch_f.rs`, `dlg/batch_i.rs`, `dlg/editor/{map_processors,map_load,sector_generate,banned_content,editor_maps,util_f}.rs`, `dlg/editor/map_assets/**`, `dlg/schematics/schematic_info.rs`, `dlg/rules/**`, `dlg/logic/canvas_edit.rs` | (stubs F2) | S09 | H05c (editor + schematic_info), H05d (rules + logic) | – | – | C01 |
| `dlg/batch_g.rs`, `dlg/batch_j.rs`, `dlg/pickers/color_picker.rs`, `dlg/mods/**`, `dlg/logic/editor/**` | (stubs F2) | S10 | H05d | – | – | C01 |
| `mind-core/src/sim/unit_runtime*` | – | S11 | H08 (non-test), H07 (test files) | X02 (`update.rs`, `ai_arms/`, one `mod` line in `unit_runtime/mod.rs`) | – | C01 |
| `mind-core/src/sim/mod.rs`, `sim/spine/**`, `sim/spine_tests/**`, `sim/runtime*`, `sim/builder*` | F3 (one line in `mod.rs`) | S12 | H08 (non-test), H07 (tests) | – | – | C01 |
| `mind-core/src/sim/combat_tests*` | F3 (one-word visibility only) | S13 | H07 | – | – | C01 |
| `mind-core/src/sim/{production_tests,logic_campaign_tests}*` | F3 (one-word visibility only) | S14 | H07 | – | – | C01 |
| `mind-headless/src/{exec,cli}*`, `server/commands*` | – | S15 | H09a | – | – | C01 |
| `mind-core/src/mods/{json,patch}/**` | – | S16 | H09b | – | – | C01 |
| blocks registry `mod.rs`+new siblings, `turrets/` new files+`mod.rs`, `io/sim_io*` | – | S17 | H09c | – | – | C01 |
| `mind-gdext/src/input/**` | – | S18 | H09d | – | – | C01 |
| `mind-gdext/src/{net,editor}/**`, `ui/root*`, `ui/editor_views*` | – | S19 | H09e | – | – | C01 |
| any `AGENTS.md`, `PORT-STATUS.md`, `PORTING.md`, `TWIN-VERIFICATION.json`, `CAMPAIGN_GAPS.md`, `.port_audit.py`, `docs/**` (except `docs/PLANS/moves|reports/<own ID>*`) | – | – | – | – | – | **C01 only** |
| `CI_PENDING.json` | orchestrator only (see rules §9) | | | | | |

**Never touched by any package:** `mind-core/src/{render,fx}/**`, `mind-gdext/src/{render.rs,render/**,fx.rs}`
and `mind-core/src/content/registries/bullets.rs`. The user had uncommitted work there when these
plans were written. `.tscn` scenes, `client/ui/*_manifest.json` and STDB bindings are also never
touched: they are ABI.

## Orchestrator checklist (the agent that dispatches the packages)

1. **Before W0:** make sure the user's in-flight work is committed or stashed by the user. Do
   not stash it yourself. Make sure `main` builds: run
   `tools/run-cpu-intensive-task.sh --cargo-check --workspace`.
2. For each package: create its worktree (rules §1). Give the agent `01-RULES.md` plus the
   package file. Wait for its report in `docs/PLANS/reports/<ID>.md`.
3. Merge the branch into `main`: `git merge --no-ff refactor/<ID>`. On a conflict, a package
   broke the ownership matrix; send it back, do not hand-resolve. After the merge, run
   `tools/run-cpu-intensive-task.sh --cargo-check --workspace`.
4. Add the report's CI entry: `python3 tools/ci_pending.py add …`, copied from the report's
   "CI_PENDING entry" block.
5. After the **last merge of each wave**, run the wave gate in the main checkout, then remove the
   worktrees (`git worktree remove ../mg-<ID>`):
   ```bash
   tools/run-cpu-intensive-task.sh --cargo-fmt -- --check
   tools/run-cpu-intensive-task.sh --cargo-clippy -- -D warnings
   tools/run-cpu-intensive-task.sh --cargo-test -p mind-core
   tools/run-cpu-intensive-task.sh --cargo-test -p mind-headless
   tools/run-cpu-intensive-task.sh --cargo-test -p mind-gdext --lib
   tools/run-cpu-intensive-task.sh --cargo-run -q -p mind-headless -- parity goldens
   tools/run-cpu-intensive-task.sh --goldens --check
   tools/run-cpu-intensive-task.sh --build
   tools/run-cpu-intensive-task.sh --godot --headless --path client res://scenes/tools/dialog_sweep_check.tscn   # expect DIALOGCHECK: failed=0
   tools/run-cpu-intensive-task.sh --godot --headless --path client res://scenes/tools/ui_widgets_check.tscn    # expect UICHECK: failed=0
   python3 tools/refactor/api_surface.py | diff docs/PLANS/baseline/api_surface.txt -   # empty unless a package documented an addition
   ```
   The full `--ci` gate belongs to the CI fixer after W5 ([`docs/CI-WORKFLOW.md`](../CI-WORKFLOW.md)).
