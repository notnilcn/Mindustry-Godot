<!-- SPDX-License-Identifier: GPL-3.0-only -->
# L02 — World generation and game-state fidelity — summary

Owner: session B (session 3 worktree, branch `parity/session-3`). Round L.

| Task | Item | Status | Wave | Commit |
|---|---|---|---|---|
| T1 Explicit `Floor.decoration` | PAR-0411 (retargeted) | **DONE** (generation goldens deferred to the L06 batch) | 1 | 38dc36f1 |
| T2 Single `reset_to_menu()` path | PAR-0416 | **DONE** | 1 | 0d7c4c8f |
| T3 `ItemSelection` honours `isHidden()` | PAR-0417 | **DONE** | 1 | 44e7a863 |
| T4 Save metadata `mapname` | PAR-0377 | **DONE** | 2 | 57bab8f1 |
| T5 Diagonal proximity check | PAR-0378 | **DONE: wontfix (Java behaviour)**, pinned by tests | 2 | 14151269 |

## T1 — explicit `Floor.decoration` (PAR-0411)

Java sets `Floor.decoration` from each prop's initialiser (`Blocks.java:705-822`) and defaults
it to `Blocks.air` (`Floor.java:70`); `Floor.init`'s `<name>-boulder` lookup
(`Floor.java:201-203`) only runs for a null decoration. The port guessed `<name>-boulder`, so
stone/crater-stone/char/sand-floor/darksand/ice/salt/hotrock/magmarock/… had no decoration.

- `BlockDef.decoration` (default AIR) + `BlockSpec.decorates` on the 17 props (30 floors,
  cited per prop); `RegistrySink::push` writes the back-reference.
- `block_info::decoration(def)` returns the field; consumers: `default_filter_stack`
  (`Maps.java:333-336`), `BasicGenerator.decoration` (`BasicGenerator.java:402`, also Erekir
  and asteroid), Serpulo `dec.get(floor, floor.asFloor().decoration)`
  (`SerpuloPlanetGenerator.java:628`) — moss/spore-moss now get spore clusters too.
- Mod JSON `decoration`: block name resolved in a deferred `finish_parsing` pass after every
  mod file (`ContentParser.finishParsing`, `ContentParser.java:1005`), mod prefix first
  (`:510-515`); unknown name = content error.

Tests: `maps::filters::tests::floor_decorations_match_java_table` (30 pairs, exactly 30
decorated floors), `default_stack_scatters_boulders_on_stone_and_sand` (density within
0.009–0.017 of `ScatterFilter.chance` 0.013 on both, nothing else placed),
`mods::json::tests::floor_decoration_resolves_after_parsing`.

Goldens moved, **not regenerated** (L-INDEX speed rule 3, batch with L06):
`mind-core maps::planet::{serpulo,erekir,asteroid}::tests::*_generation_checksum`. No
mind-headless golden moved.

In-engine (fresh build, real clicks; `runs/20261018-s3-L02-T1/`): Ancient Caldera survival,
same pose as Java `42-survival` — `diff/42-survival-side.png`: Java 5 sand boulders + 3
boulders, Godot 3 + 3 (re-rolled per load in both; before: 1–2 boulders, no sand boulders).
**MATCH.**

Serpulo spot-check (Java editor Sector Generate, Serpulo sector 0, seed 0, no save writes;
Godot `MindEditor.sector_generate("serpulo", 0, 0)` — staging, the dialog's text field could
not be driven before L01 T4): the generated **layout differs** from Java (land/water/ice
placement) and Java places derelict ruins that the Godot editor generation lacks. Editor
display orientation matches Java for loaded maps, so this is a generator gap → new finding
below; the decoration comparison at a shared landmark was not possible.

## T2 — single `reset_to_menu()` path (PAR-0416)

Java ends every return to the menu in `logic.reset()` (`Logic.java:299-311`). Found in-engine
(fresh build): Save & Quit already reset the facade (database locks were right), but the sim
kept the world loaded in `paused`; the non-campaign game-over `@menu`
(`GameOverDialog.java:134-138`), editor exit (`MapEditorDialog.java:789-795`), abandon and
campaign-complete paths only paused the sim. (The round-L frame was captured against a stale
`.so`, L-environment rule 4.)

- `MindSimHost.reset_to_menu` (`Sim::reset` + per-match host state) and
  `MindCampaign.reset_to_menu` (#[func], the single path; `run_exit_save`/`check_playtest` end
  in it); `UiRoot.return_to_menu()` used by pause quit/abandon, game-over `@menu`, campaign
  complete and editor exit; the trailing `set_paused(true)` calls are gone.
- Pause key ignored outside `paused`/`playing` (`Control.java:735`).
- Phase-consumer audit (mind-gdext): `database_json` uses
  `ui::database::custom_game_unlock_override` = `!isCampaign && !isMenu`
  (`DatabaseDialog.java:244`); banned marks, content stats, planet rules and `save_slot`
  read the facade phase (Menu after reset); music already treats the menu background as menu;
  HUD `state_name` reads the sim (`menu`).

Test: `ui::database::tests::database_locks_return_after_reset_to_menu`.

In-engine (`runs/20261018-s3-L02-T2/`, real clicks except the staged core kill): Save & Quit
→ sim `menu`, 0 entities, IO idle, database 250/71 (= boot baseline; 321/0 in game); second
game loads after the reset; game over (core killed via `debug_kill_building` — staging) →
`Menu` → sim `menu`, database 250/71, Space ignored; Core Database lock pattern = Java
`java/28-coredb.png`. **MATCH** (spacing = L01). Not walked: editor exit, campaign complete.

## T3 — `ItemSelection` honours `isHidden()` (PAR-0417)

`config_spec` options carry `hidden` (`Item.hidden`, `Liquid.hidden`, `UnitType.hidden`,
`Block.isHidden` under the live rules — `Item.java:67`, `Liquid.java:102`,
`UnitType.java:785`, `Block.java:1553`); items fold `!hidden` into `on_planet`
(`Item.java:61-64`); constructor/payload block lists pre-filter with `Block.isVisible()`
(`Constructor.java:55`, `PayloadSource.java:89`); `mind_item_selection.gd` drops `hidden`.
Hidden units stay in factory/payload lists and are dropped by the widget (Java shape).

Test: `desktop::tests::content_options_carry_the_java_icon_size_and_filters` (16 Serpulo items
survive; fissile matter/dormant cyst, gallium, the six hidden units, item-source in survival).

In-engine (`runs/20261018-s3-L02-T3/`): sandbox item source (block selected via
`select_block_by_name` and deselected via `clear_building` — staging; placement and the config
tap are real clicks): **16 items**, 4×4 grid = Java `22-srccfg`. **MATCH.**

## T4 — save meta `mapname` (PAR-0377)

Java writes `"mapname", state.map.name()` (`SaveVersion.java:133`); the port wrote the `sim`
placeholder, so custom-game Load cards read "Map: Unknown".

- `Sim::map_name`/`set_map_name` (Java `GameState.map`; empty = `emptyMap` -> `Map.tag`'s
  "Unknown", `Map.java:151-153`), cleared by `Sim::reset` (`logic.reset()`).
- Set on every world load: custom map = the map's `name` tag (`World.java:369`,
  `Map.java:127-129`); sector = `preset.localizedName` or "<planet>; Sector <id>"
  (`World.setSectorRules`, `World.java:300`, also on resume); Load Game = the save's
  `mapname` (`SaveVersion.java:186-191`). `SimIoHandler::save` writes it.
- Deviation: the editor playtest saves carry the map's name, not Java's
  "Editor Playtesting" (`MapEditorDialog.java:338`) - the port playtests through map play.

Test: `io::sim_io::tests::save_meta_mapname_is_the_loaded_map`. No golden moved.

In-engine (`runs/20261018-s3-L02-T4/`, fresh build, real clicks): the new Caldera autosave
card reads **Map: Ancient Caldera**; this session's pre-T4 Caldera saves read
**Map: Unknown**. No Java capture (saving would change the oracle profile); citation only.

## T5 — PAR-0378 diagonal proximity (wontfix)

Java has no diagonal proximity: `BuildingComp.updateProximity` (`BuildingComp.java:1886-1914`)
links through `Edges.getEdges(block.size)` only, and the edge table (`Edges.java:22-36`) is
`size * 4` side points, never a corner (size 1 = the four orthogonal neighbours); there is
no `checkDiagonal` in the Java tree. The J04 T1 observation (a diagonally adjacent item
source does not feed a turret) is therefore Java behaviour. Pinned by
`world::edges::tests::edges_match_java_table_without_corners` and
`world::proximity::tests::diagonal_neighbours_do_not_link`. No Java in-engine capture was
taken (the code citation is exact); recommend `wontfix`.

## Test commands (end of wave 2)

```
cargo fmt --all -- --check; cargo clippy --workspace --all-targets -- -D warnings   # clean
cargo test -p mind-core --no-fail-fast    # 2770 passed, 3 failed = the deferred generation checksums
cargo test -p mind-headless --no-fail-fast  # green
cargo test -p mind-gdext                  # 77 passed
mind-headless parity goldens              # 97 PASS; run-all --tier T0 4/4
godot --headless --path client --script res://tools/shaders_check.gd   # exit 0
bash tools/godot.sh --headless --editor --quit --path client   # clean
```

## Findings for the coordinator (new items)

1. Serpulo generated sector layout differs from Java (sector 0 via editor Sector Generate:
   different land/water/ice placement; no derelict ruins in the Godot editor generation).
   Evidence `runs/20261018-s3-L02-T1/{java/08-gen-serpulo0.png,godot/12-gen-serpulo0.png}`.
   Candidate for L06 (PAR-0391 generator work) or a new item.
2. Clicking the highlighted placement-palette button does not deselect the block (Java
   toggles off). UI owner: session A. `runs/20261018-s3-L02-T3/notes.md`.
3. (Corrected in wave 2) custom-game autosaves ARE written; wave 1 looked in the wrong
   directory. New wave-2 findings instead: Save Game -> New Save with a typed name writes no
   file, and that dialog shows the raw key `save.name` (`runs/20261018-s3-L02-T4/notes.md`).
4. `SimHost.place_block` (API) places team-0 (derelict) blocks — harness note only.
5. Godot editor view: an unrendered grey chunk at the top-right after a sector generate
   (`godot/09-gen-serpulo100.png`).

## Ledger changes to apply

| Item | Change | Evidence |
|---|---|---|
| PAR-0411 | fixed by L02 T1 (38dc36f1); Caldera frame MATCH; generation goldens pending the L06 batch | `runs/20261018-s3-L02-T1/diff/42-survival-side.png` |
| PAR-0416 | fixed by L02 T2 (0d7c4c8f); Core Database after quit/game-over MATCH | `runs/20261018-s3-L02-T2/` |
| PAR-0417 | fixed by L02 T3 (44e7a863); 16-item grid MATCH | `runs/20261018-s3-L02-T3/godot/03-srccfg.png` |
| PAR-0424 | Serpulo generated sector layout/ruins differ from Java (finding 1; L06 T6, wave 3) | as above |
| new (UI) | palette button click does not deselect (finding 2) - seeded as PAR-0425 | T3 notes |
| PAR-0377 | fixed by L02 T4 (57bab8f1); Load card "Map: Ancient Caldera" | `runs/20261018-s3-L02-T4/godot/06-load.png` |
| PAR-0378 | wontfix (Java behaviour: `Edges.getEdges` has no corners), tests 14151269 | `Edges.java:22-36`, `BuildingComp.java:1886-1914` |
| new | Save Game -> New Save with a typed name writes no file; dialog shows raw `save.name` | `runs/20261018-s3-L02-T4/notes.md` |

## Done criteria

T1–T5 resolved: T1–T4 landed with tests and matching frames, T5 recorded as Java behaviour
with pinning tests. Goldens: L02 T1's three planet generation checksums wait for the batched
regen after L06 T4–T7 (wave 3, by plan). Plan complete.
