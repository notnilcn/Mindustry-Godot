<!-- SPDX-License-Identifier: GPL-3.0-only -->
# L01 — UI layout engine: minimum-size propagation and dialog centring (wave 1: T4, T1, T2)

Session A (session 2: worktree `Mindustry-Godot-session2`, branch `parity/session-2`, display `:11`,
bridge 6980). Pure GDScript + harness, so no cargo changes. A fresh `tools/build.sh` (flock) and
`tools/session-play.sh 2` ran before the first walk (L-environment rule 4); scripts were reloaded with
`tools/session-play.sh 2 --play`.

## Per-task status

| Task | Status | Notes |
|---|---|---|
| T4 text injection | **done** | `godot_input text` repaired in the runtime autoload; `tools/gmcp.py --text`, twin `GT`. Proven on the Custom Game search. |
| T1 min-size propagation | **done** | Root cause was bigger than the plan's diagnosis (see below). `ui_layout_check.gd` added and wired into `tools/ci.sh`. PAR-0410 fixed in-engine. |
| T2 fill-dialog centring | **done** | Pause 3 px, editor menu 2 px, Map Info 3 px, Game Over ≤ 4 px vs Java. |
| T3 re-verify workaround dialogs | **pending (wave 2)** | Not started, by schedule. |

## Commits

```
005e203e L01 T4: godot_input text types into the focused LineEdit (gmcp --text, GT)
57c2d00b L01 T1: MindTable lays itself out and propagates its minimum size
52c0b27f L01 T2: fill BaseDialog bodies centre like Arc Dialog cont
```
(`766e9cb9 L08 T5` landed in the same wave; its record is in the wave report, as L08 is session B's summary.)

## T4 — text entry for tests

### What changed
- `client/addons/open_godot_mcp/runtime/runtime_autoload.gd`. `_input_text` built one `InputEventKey`, parsed it
  pressed, then mutated it to released and parsed it again. `Input.parse_input_event` buffers events until the
  frame flush, so both queued references were releases, and `LineEdit` (which inserts only on a pressed key with
  `unicode >= 32`) stayed empty. The installed MCP server's own tool description calls this path "broken".
  The fix sends a **fresh** event per edge (`_make_text_key`) with `unicode` set, plus keycode/physical
  keycode/`key_label`/shift for ASCII letters, digits and space. It flushes, and waits `frame_delay` process
  frames (default 1) between characters. It returns `focus_before`, `focus_after` and `field_text`.
- `tools/gmcp.py --text 'sand'` (repeatable, `--text-frame-delay N`); `tools/twin.sh` `GT 'sand'`.

### In-engine evidence (`runs/20261018-s2-L01-T4/`, real input)
`GC` Play → Custom Game, `GC` into the search field, then `GT sand` gives
`{"typed":4,"focus_before":".../Search/@LineEdit@3843","field_text":"sand"}`. The eval readback is
`["sand", true, 4]` (`godot/custom-typed-sand.png`). `GK Backspace` ×4 empties the field
(`godot/custom-cleared.png`). Learnings: `open-godot-mcp-learnings.md` 2026-10-18 (L01 T4).

## T1 — `MindTable`/`MindCell` minimum-size propagation

### Root cause
The plan diagnosed a missing `update_minimum_size()`. The deeper bug: `Container.queue_sort()` defers to the
native `Container::_sort_children`, which only sends `NOTIFICATION_SORT_CHILDREN`. `MindTable`'s script
`_sort_children()` is not a virtual, and the native path never calls it. A probe container recorded 0 script
`_sort_children` calls against 2 notifications. So a `MindTable` laid out **only** when a dialog called
`sort_now()`, and every stale rect followed from that. PAR-0410 is the clearest case: after the `sand` filter, the
nested map grid kept the unfiltered 1080x1608 rect (its minimum was 216x268) and centred its one tile at y=678,
below the visible pane.

### What changed (Arc `WidgetGroup.invalidateHierarchy`, `WidgetGroup.java:98-106`; `Table.invalidate` `Table.java:174-177`; `Table.row` `:647-650`)
- `mind_table.gd`: handles `NOTIFICATION_SORT_CHILDREN`. `invalidate()` (= `update_minimum_size()` +
  `queue_sort()`) runs from `add`, `row`, `clear`, `clear_children` and `margin`. Child `minimum_size_changed`,
  `visibility_changed` and `size_flags_changed` already arrive through `Container.add_child_notify`, so they are
  not connected twice. `_resort_after_sort` and the `_sorting` latch are gone. `sort_now()` stays for headless
  checks and same-frame rect reads.
- `mind_cell.gd`: `table` back-reference; every layout setter (grow/fill/expand/uniform/align/width/height/size/
  min/max/pad/colspan) invalidates through `_changed()`. Without that, the chained
  `add(x).width(...)` kept a stale cached minimum.
- Workarounds removed: `MindDialog.show_dialog`'s immediate + deferred `_resort_tables`; `map_list_dialog.gd` and
  `load_dialog.gd` `sort_now()` + deferred `sort_now`; `map_play_dialog.gd` deferred `_resort_tables`.
  `logic_dialog.gd` had no such call any more. Other dialogs' `sort_now`/`_resort_tables` calls (join, keybind,
  mods, planet, schematics, custom rules, …) are now redundant but harmless; T3 re-verifies them.
- Found during the walk: editor **Map Info** stacked a second body under the first on every reopen. Its `shown()`
  appends a new `content_table()`, while Java builds a fresh dialog per `showMap` (`EditorMapsDialog.java:101`).
  The same pattern exists in editor menu/import/export and the research root select. New
  `MindDialog.clear_content()` is now called before those rebuilds. The `content_table()` doc no longer claims it
  clears.

### Tests (exact commands)
```bash
bash tools/godot.sh --headless --path client --script res://tools/ui_layout_check.gd
# LAYOUTCHECK: failed=0 (22 assertions: CenterContainer refill, MindScroll refill,
# the PAR-0410 nesting scroll>vbox>table>grid with a filter rebuild, visibility + margin)
# With the pre-fix mind_table.gd/mind_cell.gd stashed: failed=8, including
#   "filtered grid shrinks to its new minimum (got (1080, 1608), min (216, 268))"
```
`tools/ci.sh` runs it right after the headless import gate. It fails the build unless `LAYOUTCHECK: failed=0`.

### In-engine evidence (`runs/20261018-s2-L01-T1/`, real input: GC/GT/GK)
- `godot/09-search-sand.png`: the filtered grid shows the sandbox tile (grid 216x268 at x=468, tile at its pad).
  Round L showed an empty grid.
- `godot/41-caldera-1.png`, `41-anthill.png`, `41-caldera-2.png`: Map Play inside its floating panel on the
  first open, then portrait anthill, then Caldera reusing the panel. No clip.
- `godot/editor-mapclick.png`, `editor-mapinfo-2.png` (before `clear_content`: stacked copy),
  `editor-mapinfo-reopen.png` (after: one body).
- `godot/sweep-*.png`: Database submenu, Settings, Load Game lay out without the removed calls.

## T2 — fill `BaseDialog` centring

### What changed
- Arc `Dialog` lays out `add(cont).expand().fill(); row(); add(buttons).fillX()` (`Dialog.java:209-211`); `cont`
  is a centre-aligned `Table`. The shell's `Content` VBox (alignment centre) sat in the `Body` scroll without
  vertical EXPAND, so the ScrollContainer gave it only its minimum height and every body stuck to the top.
  `MindDialog._apply_cont_alignment()` expands it. The new `cont_top` (`set_cont_top()`) maps Java
  `cont.top()`, which is set in `schematics_dialog` (`SchematicsDialog.java:78`), `database_dialog` (`:57`),
  `sector_select_dialog` (`:27`), `campaign_rules_dialog` (`:42`) and editor `map_processors_dialog`
  (`MapProcessorsDialog.java:58`).
- Title geometry (`BaseDialog.java:23-25`). Java text sits at y=10–22 with the rule at y=30–32; the port had text
  at 3–15 and the rule at 16–18, overlapping the label. Now there is a 7 px top inset on the
  `defaultDialog`/`fullDialog` panel (`theme_builder.gd`, the `windowEmpty` background inset, which also places
  the untitled Game Over headline), a 6 px label bottom inset, and the rule 4 px under the text.
- An empty button row is hidden (an empty Arc `buttons` table has no height).
- `paused_dialog.gd`: removed the 14 px "bias" spacer. The campaign row is now hidden unless one of its buttons
  shows, because Java's `tpad = -60` makes that row zero-height (`PausedDialog.java:61,80-82`).
- `editor_menu_dialog.gd`: Java pads `padBottom/Left/Right(5)` (`MapEditorDialog.java:71`) and the wide cont rows
  (`:181-184`); the old `pad(1)` gave a 62 px pitch against Java's 65.
- `game_over_dialog.gd`: the block grows to `maxSize(600, 550)`, the pane grows, stats centre in it, Time Played
  sits at its bottom, and `buttons.margin(10)` applies (`GameOverDialog.java:47`, `:62-102`).

### Acceptance (±4 px) — `runs/20261018-s2-L01-T2/notes.md`
| Frame | Java | Godot | Δ |
|---|---|---|---|
| Pause (`26-pause`) | rows 218–467, rule 30 | rows 215–464, rule 30 | 3 |
| Editor menu (`32-edmenu`) | rows 110–493, x 398–753 | rows 112–495, x 398–753 | 2 |
| Map Info (fresh Java `java/mapinfo.png`) | preview top 122, buttons 442–471 | 125, 439–469 | 3 |
| Game Over (`44b`) | headline 19, rows 104, Time Played 501, Menu 594 | 16, 105, 505, 593 | ≤ 4 |

Java frames: round L `runs/20261017-L-twin/java/{26-pause,32-edmenu,44b}.png` and a fresh `:11` Map Info capture
(seeded `parity-java-home`, `twin_launch`/`twin_stop`). **Staging:** the Game Over was reached with
`SimHost.debug_kill_building(130, 32)` on the Caldera core after a real-input Play. Every other step used real
input.

## Deviations (with Java locations)
- Paused dialog, campaign branch: Java draws `@abandon`/`@objective` 60 px above a zero-height cell
  (`PausedDialog.java:80-82`); the port gives the row normal height when shown. This is not in an acceptance frame.
- Title rule: Java's rule sits 3 px higher in editor dialogs (y=27 in `32-edmenu`/`mapinfo`) than in the in-game
  and menu dialogs (y=30); the port uses 30 everywhere (within tolerance).
- The plan says Custom Game and Load Game use `top()`. They don't: `MapListDialog.java:137`
  `cont.add(pane)…grow()` and `LoadDialog.java:82` `growY()`, with Arc `ScrollPane` stretching the widget to the
  viewport (`ScrollPane.java:459-460`). A short filtered list therefore centres in Java. The port's map grid sits
  at the top of the pane; T3 compares the search frame against Java.

## Blockers
None for T1/T2/T4.

## Pre-existing failures (verified, not caused by this work)
`ui_widgets_check.gd` (not run by `tools/ci.sh`) reports 65 ok and 3 FAILs: "keybind dialog renders the reset-all
button", "axis binding renders min / max (A / D)" and "key label renders with a real width". It then stops on
`SCRIPT ERROR: Invalid call to function '_build_prompt' … Expected 4 argument(s)`. The output is identical before
and after the T2 changes. The drift comes from J06 T4 (keybind axis markup) and J07 T5 (prompt arity).

## Notes for T3 (wave 2)
- The Java editor maps grid has taller cards and its search row sits lower than Godot's
  (`runs/20261018-s2-L01-T2/java/editor-maps.png` vs `godot/editor-maps.png`).
- `GT` (text) and `GC` (with the real pointer) are the T3 input path.

## Ledger changes to apply
| Item | Change | Evidence |
|---|---|---|
| PAR-0410 (F-K2 search empties grid) | fixed in code (T1); close after T3's Java pair | `runs/20261018-s2-L01-T1/godot/09-search-sand.png`, `ui_layout_check.gd` |
| J06 T7 "Map Play portrait previews clip" | fixed (T1); close after T3's Java pair | `runs/20261018-s2-L01-T1/godot/41-*.png` |
| new "Editor Map Info at the bottom on first open" | fixed (T1 + T2): centred on every open, 3 px from Java | `runs/20261018-s2-L01-T2/{java,godot}/mapinfo.png` |
| new "Fill dialogs top-align their content (pause, editor menu, Game Over)" | fixed (T2), ±4 px | `runs/20261018-s2-L01-T2/notes.md` |
| new | Reused dialogs (editor Map Info/menu/import/export, research root select) stacked a second body on reopen → fixed (T1) | `runs/20261018-s2-L01-T1/godot/editor-mapinfo-2.png` vs `-reopen.png` |
| new (harness) | `ui_widgets_check.gd` red on main (keybind ×3 + `_build_prompt` arity) and absent from `ci.sh` | `/tmp` log quoted above |

## Done criteria (wave 1 scope)
T4 landed first. T1 and T2 landed, with `ui_layout_check.gd` in the import gate (`tools/ci.sh`). T2 acceptance
frames are within ±4 px of Java. Still open (wave 2): T3 acceptance frames, then PAR-0410/J06 T7 closure with Java
pairs.
