<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Plan verification round 3, session 3 (2026-10-14): H05 campaign UI, H08 leftovers

Worktree `Mindustry-Godot-session3` @ `ffbe84dd` (main; H00 not landed), bridge 6990, display `:12`.
Godot profile from a clean `--import` (`{"nodes": 375, "unlocked": 282}`), re-imported once mid-run
(sector 0/1 starts and a custom-game attempt had mutated it). Evidence: `runs/20261014-s3-verify3/`
(`H05-*`, `H08-*`, `java-*` PNGs). Temporary, uncommitted worktree edit: H00 T1 Change 1
(`planet.rs::env_bit` -> Java bits) so the alpha spawns; reverted with `git checkout -- client/`,
rebuilt; worktree clean except the ledger symlink. 0 panics in every run log, 0 crash reports.

Java client: pid 377755 on `:12` with the worktree `parity-java-home` (log
`runs/java-console-s3.log`), quit gracefully from the main menu. Campaign state of that copy changed
only by a sandbox custom-game start/quit (no sector launched). Session 2's processes were not touched.

## H05

| Task | Status | Evidence | Plan edits |
|---|---|---|---|
| T1 | PARTLY | all 272 view rows `threat` 0.0; sector 1 `winWave` 15 | Changes 1-2 are a no-op (the 45 attack ids are already submission presets); test `serpulo_attack_sectors_marked` corrected (no preset-less marked cell); verify expectation (sector 0 threat 0.8) |
| T2 | CONFIRMED | `H05-T2-sector1.png`, `-sector2.png`; floor fingerprint 601855000 for sectors 1/2/5; 128x128 | recipe dry-run notes (out-of-bounds `tile_debug` = `{}`) |
| T3 | CONFIRMED (code + visual) | `H05-T2-sector1.png` | verify pair was non-discriminating (corner tiles are walls; `place_block` bypasses the gate) -> rewritten; Arc `Noise` oracle values added (JDK + Arc jar exist) |
| T4 | CONFIRMED | `H05-T4-sector1-start.png`: `gameOver` true, `hasCore` false, `restart` dialog | UI-path note (no selectable numbered sector on this profile; Java "Locked") |
| T5 | SYMPTOM NOT IN VANILLA | `H05-T5-sector0-hidden-preset.png`: sector 0 = attack, 5 enemy cores | coordinator decision (below) |
| T6 | CONFIRMED | 16384-tile scan of sector 1: no building at all | — |
| T7 | CONFIRMED (code) + fact fix | `H05-T7-toast-in-menu.png` | the menu toast guard only mutes the sound (label still shown) -> Change 2 rewritten; log line per event for the verify |
| T8 | PARTLY | `H05-T8-tree*.png`, `-root-selector.png`, `-researched.png`; Java `java-11/12` | (1) dim and (3) empty items NOT reproduced; (2),(4),(5) confirmed; blank node = Ground Zero sector icon, Java falls back to `Icon.terrain` (Change 5 reduced); live spend works today (Scrap Wall: scrap 300 -> 100, unlocked) |
| T9 | CONFIRMED (code) | JDK run against the Arc jar | test value `-1.25 (max 1)` is `"-1.3"`, not `"-1.2"` |
| T10a | CONFIRMED | `H05-T10a-duo-info.png` vs `java-26-duo-info.png` | extra symptom: raw `[accent]Duo` header; Java reference rows recorded |
| T10b-d | NOT CHECKABLE | — | — |
| T10e | NOT CHECKABLE | Java `java-26-duo-info.png` (copper 9 damage, 2 ammo/item) | — |
| T11a | CONFIRMED | `H05-T11a-rules.png` vs `step-316-rules.png` | — |
| T11b | CONFIRMED (code); verify BLOCKED | `H05-T11b-pause.png`, `H05-T11b-sandbox.png`, `java-23-escape-inventory.png` | the port's Custom Game never loads the map -> verify note + headless fallback |
| T12 | CONFIRMED (code) | — | factorycontrol check depends on H08 T6b |
| T13 | CONFIRMED + 2 new | `H05-T13-cards-ingame.png` vs `java-02/03/03b` | Change 5 added: vanilla loadouts listed as user schematics (Java keeps them out of `all()`); note 6: Java `Tags:` row missing |
| T14 | CONFIRMED (code) | `java-09-rotated.png` (Java drag) | PlanetView path verified |
| T15 | CONFIRMED | `H05-T15-gz-panel.png` vs `java-06-gz-panel.png` | Port-today: the title row shows the preset art where Java shows the `Icon.noneSmall` button |
| T16 | CONFIRMED | `H05-T16-stats.png` vs `java-07-gz-stats.png` | reference path fixed (`step-142-stats.png` does not show the dialog) |
| T17 | CONFIRMED | `H05-T17-database-onset.png` vs `java-04-database.png` | reference path fixed (`menu-03-database.png` is the planet page) |
| T18 | withdrawn (marker present) | — | H04 T11 not on main; acceptance not run |
| T19 | CONFIRMED (code) | — | — |
| T20a-c | CONFIRMED (visual) | `H05-T20-planet.png`, `H05-T15-gz-panel.png` vs `java-05/06` | — |
| T21 | PARTLY (code) | — | layout-stub bullet fixed (`MindRowTreeLayout` is used) |
| T22 | CONFIRMED | `H05-T22-host.png` vs `step-375-host.png` | — |
| **T23 (new)** | CONFIRMED | `H05-T23-onset-palette.png` (fresh: ducts), `H05-T23-onset-after-gz.png` (after GZ: Serpulo conveyors) | new task: palette rebuild on world load / block unlock (session 2's report; owner H05) |

## H08 leftovers

| Item | Status | Evidence | Plan edits |
|---|---|---|---|
| T6b | CONFIRMED | `H08-T6b-wall-rightclick.png`; right-click on the enemy wall -> `attack:false` move to (1124,500); middle-click -> no order | recipe: hold Shift through select + right-click, `world_to_screen`, numeric block id 216 + `rotation`; `set_unit_command` absent until H03 T3/T6a |
| V1.2 | PARTLY | paste at (139..141,64) rot 0, copper charged, built instantly | coords fixed; re-run after H00 T2 |
| V1.3 | PARTLY | ghost -> rebuilt, ghost list empty, instantly | re-run after H00 T2 |
| V1.4 | PASS | objective false -> true -> false | — |
| V1.5 | PASS | stack 30 -> 15, duo `turret_total_ammo` 30 | — |
| Escape over config panel | CONFIRMED | `H08-NEW-config-escape.png` vs `java-23-escape-inventory.png`, `java-24-after-back.png` | **new T14**: panels live in `OverlayLayer` above `DialogLayer`; Java keeps them in `hudGroup` under dialogs (still shown after Back) |

## Java leg (≈45 min)

Captured: schematics dialog (`java-02/03/03b`), Core Database (`java-04`), planet page + Ground Zero
panel + stats (`java-05/06/07`), numbered-sector hover "Locked" (`java-08`, `java-10-*`), globe drag
(`java-09`), tech tree + root selector (`java-11/12`), sandbox custom game: core inventory, Escape ->
pause menu with the book button, after Back (`java-17..24`), duo info (`java-26`). Not observed:
a numbered-sector launch (profile has no launchable numbered sector), Java sorter config panel
(the sandbox stayed paused on Xvfb so the sorter never built — the inventory fragment shares the
same group), invasion toasts, rename/icon picker.

## Hiccups (logged in `open-godot-mcp-learnings.md` / `java-leg-learnings.md`)

- `dialog_stack()` called on UiRoot (it is on MindUi) and `.size()` on the numeric
  `select_plans` field each wedged the game; `--play` recovered (second time cost a GZ re-entry).
- The process-kill loop matched its own shell (exit 144, harmless).
- Java sandbox would not unpause with Space on Xvfb.

## For the coordinator

1. **H05 T5 has no vanilla trigger.** All 45 Serpulo `attackSectors` are hidden-map submission
   presets (`SectorSubmissions.java`), `generateSector` is a no-op, no other generator uses
   `basegen`; attack sector 0 already works through its map. Decide: drop T5 to low priority
   (mod/debug parity) or keep it headless-only. T6 (ruins) still matters.
2. **Custom games never load their map** (`map_play_dialog._play` -> missing `start_map_mode` ->
   `start_map` without a world load; `play_requested` has no listener). Not owned by any round-H
   plan; it blocks H05 T11b's in-engine check.
3. **Loaded player units lose CommandAI** (save/reload -> `GroundAI`, `commandable:false`; Java
   `UnitComp.afterRead` recreates the controller). Owner: H04 T9 (unit chunk) or H01.
4. New tasks: H05 T23 (palette rebuild; session 2's finding, owner H05 since the fix is the
   fragment + a `world_loaded` signal; H08 T5 also edits `placement_fragment.gd`), H08 T14 (config
   panels under dialogs; it changes the `block_config` node path used by H-environment §4.2,
   H02 T6, H08 T4 recipes).
5. PAR-0145 live spend already works in engine (H05 T8): only the visual points remain.
6. H05 T1's real fix is the boot `updateBaseCoverage` (threat 0 everywhere today; numbered
   sectors get `winWave` 15 instead of >= 25).
