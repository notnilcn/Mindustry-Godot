<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Execution summary — audit-campaign-import (session 5)

- Plan: `/home/clintonnguyen/g/code_examples/Mindustry-Godot/docs/parity-plans/audit-campaign-import.md`
- Session/branch: session 5, `parity/session-5`, worktree `/home/clintonnguyen/g/code_examples/Mindustry-Godot-session5`; date 2026-10-12.
- Bootstrap: `tools/parity-session.sh --session 5` on display `:14`, bridge 7010; `git merge --ff-only main` (clean; branch tip was already an ancestor). No merge conflicts; audit fixes `a77e7b9`/`09ba3d0` present. Assets present, `pack.sh` not needed.
- Working state at end: branch clean (only the shared `parity-ledger.json` symlink untracked); 0 panics in the current run log; clean import yields `{"nodes": 375.0, "unlocked": 282.0}` and the T4 eval `[223, 174]`.

## Per-task status

| Task | Status | Notes |
|---|---|---|
| T4 research requirements | **done** | Committed `2fe5800`. |
| T1 Configure Loadout (PAR-0085/0025/0026/0027) | **done** | Committed `44b0f7b`. |
| T2 launch schematic core (PAR-0085 step 5) | **done** | Committed `fe9381b`. |
| T3 landing cutscene remainder (PAR-0131) | **done** | Committed `659146d`. Thrusters + hiding the real core building skipped (plan allows; recorded in the commit). |
| T5 remap notice styling | **done** | Committed `22f800d` (needed a small `MindUi.open_dialog` `keep_previous` overlay flag). |
| T9 lastplanet write-on-change | **done** | Committed `56aac12`. |
| T6 generated sectors (PAR-0112) | **not reached** | Budget. Step 1 needs a generator refactor: `SerpuloPlanetGenerator`/`ErekirPlanetGenerator` hold a concrete `FlatSectorView` field, so a real `PlanetGridSectorView` needs a boxed-trait/downcast seam plus a `Sector.makeRect` port (Arc Vec3/Plane math over `PlanetGrid` corners); `generate_sector` must then pass the view and `seed_offset=0` + `getSectorSize`. Nothing was changed. |
| T7 enemy AI residuals (PAR-0156) | **not reached** | Budget. Recon done in code: `TurretState.config` (reload/shots/range/target_air/target_ground/total_ammo) and `turrets::peek_ammo`/`has_ammo` exist, but turret ammo `estimateDPS` has no public seam (the bullet DPS recurrence lives inline in `units::link`); T7a needs it extracted, plus `TeamAiBuildingRow` turret fields and `team_ai_enemy_stats` appending them; T7b needs the `prebuild_ai` spawn branch. Nothing was changed. |
| T8 shared settings hub | **not reached** | Budget (architectural; plan sequences it last before T10+). |
| T10–T16 | **not reached** | Budget. |

## Commits (`git log --oneline main..HEAD`)

- `2fe5800` Research: materialize tech-node requirements and imported payments (T4) — fills `TechNode.requirements`/`finished_requirements` from `content.researchRequirements()` + `req-*` keys at `create_base_content` end (post-init is store-less), fixes `ResearchView.root` so the dialog lists all nodes, regenerates both goldens.
- `44b0f7b` Configure Loadout: validity, Max toggle, capacity and item rows (T1) — `mind-core` `launch_plan` + tests, `MindCampaign.launch_dialog_json`, `settings_get_flag_default`, `Schematics.load` of `schematics/*.msch` with post-sort reindex, dialog rewrite, planet dialog reads plan resources.
- `fe9381b` Fixes PAR-0085: place the launch loadout schematic's core (T2) — `Sim::place_loadout`/`SimRuntime::place_schematic`, `MindSimHost.place_launch_loadout`, campaign payload `schematic`, launcher-core loadout rows, `best_core_position` exposed to GDScript.
- `659146d` Fixes PAR-0131: landing core sprite, white fade, announce, timing and locks (T3) — delta clamp, `landing_draw_json`, `landing_sprite` + test, `World/LandingCore` fragment, `start_white`, announce probe-verified, pause/hints cutscene gates, `start_map` trigger.
- `56aac12` lastplanet: write settings only when the planet changes (T9).
- `22f800d` Campaign Changes Notice styling: OK button, 500px body, planet page behind (T5) — plus `MindUi.open_dialog` `keep_previous` support.

## Tests (exact commands)

Run from `client/rust` (serialized behind `flock /tmp/opencode/mg-build.lock`):

- `cargo test -p mind-core --lib` → **1940 passed; 0 failed; 4 ignored** (at the last full run; includes the new `materialized_requirements_match_block_research_cost`, `finished_requirements_load_req_keys`, `launch_plan_*` (3), `place_loadout_replaces_core_block`, `load_reindexes_loadouts_after_sort`, `landing_sprite_formula_matches_java`).
- `cargo test -p mind-headless --lib ui_goldens_match` → **1 passed; 0 failed**.
- `cargo run -q -p mind-headless -- parity goldens` → all PASS after the T4 golden regeneration (both `content_ids` and `harness_ui_campaign` sha256 updated in `parity/golden_manifest.json`).
- `cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings` → clean.
- `cargo fmt -p mind-core -p mind-gdext` applied each task.
- GDScript parse gate `bash tools/godot.sh --headless --editor --quit --path client` → no `SCRIPT ERROR`/`Parse Error` after every task (only the pre-existing `MindMods::get` shadow warning).

## In-engine verification (screenshots in `runs/20261012-session5-campaign-import/`)

Clean re-imports were used before/after fixture mutations (282/375 preserved; the T5 verification re-imported again).

- **T4**: baseline `[223, 1]` → after fix `[223, 174]`; `lancer` row shows `copper 1900 / lead 2200 / silicon 1900 / titanium 900` with the identical imported `finished` amounts; tooltip text `copper 1900/1900, lead 2200/2200`. `t4-research-dialog.png` (before: single root row) vs `t4-research-dialog2.png` (after: full node list). Java evidence: `TechTree.java:160-176` cost formula + `req-*` keys (the profile's `req-lancer-*` values).
- **T1**: launcher fixture (Ground Zero captured with 3000 copper / 2000 lead, marker `[100,50]`): plan JSON capacity 2000, max true, resources `{copper 2000, lead 1200}`, stored/total `{3000, 2000}`, valid true; `t1-launch-loadout.png` shows "Launching From: Ground Zero", "Launching Item Capacity: 2000", rows "3000 (2000 + 1000)" and "2000 (1200 + 800)", Back/Max/Resources/Launch. Max toggle flipped `maxresources` and rebuilt; emptied launcher → valid false, `t1-missingresources.png` shows scarlet `0/1000 (0 + 1000)` rows + "Insufficient Core Resources". Launch left the new core `{"copper":2000,"lead":1200}` and the launcher empty. Java evidence: `LaunchLoadoutDialog.java:58-117,125-152`.
- **T2 (also the T1 title-block check)**: after the launcher was given a foundation core (via the new seam) holding 6000/4000/2000, the dialog offers basicBastion/basicFoundation/basicShard, defaults to basicFoundation, capacity 4500, resources `{copper 3000, lead 1000}`, Launch enabled (`t2-launch-foundation.png`, rows "6000 (3000 + 3000)" etc.). Confirming launched into frozenForest: `best_core_position`/`hover_info` report `core-foundation` at the core tile and `core_items_json()` is `{"copper":3000,"lead":1000}`; the launcher stores nothing. Unit test pins shard→foundation replacement. Java evidence: `FileMapGenerator.java:95-130`, `Schematics.java:465-500`.
- **T3**: `landing_draw_json` mid-cutscene `{"active":true,"block":"core-foundation","fin":0.0,...}`; `t3-landing-1.png` shows the white fade with the dropping core, `t3-landing-2.png` the mid zoom, `t3-landing-3.png` the settled view; zoom reaches 4.0 and `is_cutscene()` falls; announce probe captured `[color=#ffd37fff]Impact 0078[/color]` (no resources line: fresh sector stores none, matching `resources.any()`); hints fragment `visible=false` during the cutscene; Space injected during the cutscene leaves `is_paused` false and the same press after the landing pauses (`[false, true]`). Java evidence: `CoreBlock.java:369-418,480-538`, `Control.java:415,730-734`, `HintsFragment.java:59,112`.
- **T5**: after a clean re-import, `t5-remap-notice2.png` shows the notice (title, centered ~500px body, single "OK" button) **over the still-visible planet page** (globe, Ground Zero panel, Back/Go/Tech Tree dimmed behind). Java evidence: `PlanetDialog.java:216-222`.
- **T9**: `stat -c %Y $U/config/settings.bin` from the menu: 1791810660 before and after three `_show_planet("serpulo")` sector-click calls.

Checks I did not perform: screenshots are the only visual evidence; all other state checks above are structured evals. The T3 thruster flames and hiding the real core building under the landing sprite are not implemented (recorded in the T3 commit as remaining).

## §6 ledger corrections to report (coordinator follow-ups; I did not edit chains.json/parity-ledger.json)

From the plan §6, still to apply:

- PAR-0154 → `verified` (fixed by `588a8c8`; evidence `runs/20261012-session5/planet-grid.png`).
- PAR-0085 → `fixing` in the plan; **update: now fully fixed by T1 (`44b0f7b`) + T2 (`fe9381b`)** — suggest `verified` after review (validity/Max/capacity/rows + schematic core placement).
- PAR-0025 / PAR-0026 / PAR-0027 → plan says `fixing`; **update: the Max toggle, `valid` gate, item rows, capacity and header landed in T1 (`44b0f7b`)**; only the cosmetic 200 px `SchematicImage` toggles remain (T15) — suggest `fixing` with that note, or `verified` once T15's cosmetic item is accepted as out of those items' scope.
- PAR-0131 note: resume must not land (`Control.java:415,589`); the plan's remainder (T3) landed in `659146d` — the note should be updated to "landing only on NewGame; sprite/fade/announce/timing/locks done; thrusters + real-core hiding remain".
- Remap notice: real import flow fixed in `09ba3d0`; **styling/overlay finished in `22f800d` (T5)**.
- New item: research requirements not materialized (T4) — **fixed in `2fe5800`; the item can be filed as verified**.
- New item: generated sectors ignore the sector id (T6 step 1) — **still open** (see T6 status above).
- PAR-0156 (T7) — still open (see T7 status above).
- Consider filing the `ResearchView.root` grouping bug fixed inside T4 (`2fe5800`): before, only 1 of 223 rows carried a root name, so the dialog listed the root row only; it now carries the tree name per row.

## Blockers / notes for the coordinator

- `godot_exec` break-on-error pauses the game: any eval that raises a GDScript error freezes the process loop and every later tool call times out (the plan's §1.3 trap). Recovery is `tools/session-play.sh 5 --play`. This accounted for the long verification times.
- `godot_input` key injection works through the MCP bridge (verified with Space), but `Input.parse_input_event` from inside an eval does **not** reach the port's binding dispatch — use `tools/gmcp.py godot_input` for key tests.
- The landing cutscene lasts tens of seconds on big maps (the sim tick rate drops on 300x300+ maps), so timing-sensitive checks need generous windows.
- `best_core_position` is now a `#[func]` returning `Vector2` (`(-1,-1)` when coreless); the Rust-internal tuple accessor was renamed `best_core_position_px` (camera/campaign updated).
- `parity/golden_content.json` and the UI campaign golden were regenerated for T4 (both sha256s updated in `parity/golden_manifest.json`) — merge carefully if another session also regenerated them.
- Shared-file churn to watch on merge: `campaign.rs`, `sim_host.rs`, `camera.rs`, `input/desktop.rs`, `ui_host.rs`, `launch_loadout_dialog.gd`, `planet_dialog.gd`, `game.tscn`.
