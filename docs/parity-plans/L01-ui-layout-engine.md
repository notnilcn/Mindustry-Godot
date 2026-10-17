<!-- SPDX-License-Identifier: GPL-3.0-only -->
# L01 — UI layout engine: minimum-size propagation and dialog centring

**Goal.** Dialogs lay out correctly on the first open, every time, without per-dialog
`sort_now()` patches. Fill dialogs centre their content like Arc's `BaseDialog`.

**Order.** T4 first (it unblocks the search-field checks in T3), then T1 → T2 → T3. This plan
is pure GDScript (`client/ui/`, `client/tools/`): no cargo build, no `.so` reload — verify in
the running editor.

**Evidence.** Round-L report G1. Frames:
- `runs/20261017-L-twin/diff/41-caldera-side.png` and `godot/41-caldera-2.png` (Map Play
  clipped, two opens);
- `godot/t4.png` / `godot/t5.png` (editor Map Info: first open at the bottom, second at the top);
- `godot/09-search-pair.png` (filtered Custom Game grid renders nothing);
- `diff/26-pause-side.png`, `diff/32-edmenu-side.png`, `diff/44b-side.png` (content top-aligned
  vs Java centred).

**Why round I/J fixes did not hold.** `client/ui/mind_table.gd` overrides
`_get_minimum_size()` (`:117`), but nothing in `add()` (`:41-48`), `row()`, `clear()`
(`:59-67`), `clear_children()` (`:98-106`), `margin()` or the cell setters calls
`update_minimum_size()`. Godot only re-queries a Control's minimum size after
`update_minimum_size()` / `minimum_size_changed`. Parents (`MindScroll`, the dialog panel,
`CenterContainer`) therefore keep the *previous* content's size, and the table sorts into a stale
rect. The round-J patch (`37ef2a8d`, `_resort_after_sort` + `call_deferred("_resort_tables")`)
re-sorts the child but never tells the parent its minimum changed. `mind_dialog.gd` and
`mind_scroll.gd` have the same gap (no `update_minimum_size` anywhere in the three files).

## T1 — Propagate minimum size from `MindTable`/`MindCell` — HIGH

- Call `update_minimum_size()` whenever the grid's min can change:
  - in `add`, `row`, `clear`, `clear_children` and `margin`;
  - in every `MindCell` setter that affects size (`width`/`height`/`size`/`pad`/`grow*`/
    `colspan`; the cell can hold a back-reference to its table, or the table can connect the
    cell's changed signal);
  - when a child emits `minimum_size_changed` or `visibility_changed`. Connect these in `add`,
    disconnect in `clear`.
- Arc reference: `Table.invalidateHierarchy()` on every `add`/`row`/`clear`/cell change
  (Arc `scene/ui/layout/Table.java`, `WidgetGroup.invalidateHierarchy`).
- Then remove the per-dialog workarounds that only existed to paper over this, and confirm each
  still lays out:
  - `sort_now()` + `call_deferred("sort_now")` in `map_list_dialog.gd:510-511`, `load_dialog.gd`
    and `logic_dialog.gd`;
  - `_resort_tables` in `map_play_dialog.gd`;
  - `_resort_after_sort`.
- Test: add `client/tools/ui_layout_check.gd` (a `SceneTree` script like
  `ui_widgets_check.gd`) that:
  1. builds a `MindTable` in a `CenterContainer`, fills it with 6 cells, measures it, clears
     it, refills it with 1 wide cell, waits one frame, and asserts the parent rect equals the
     new minimum;
  2. does the same inside `MindScroll`.

## T2 — `BaseDialog` content centring in fill mode — HIGH

- Java: a fill `BaseDialog` is a `Dialog` table with `cont` and `buttons` rows. `cont` is a
  centred `Table` that takes the remaining height, and its content is centred both ways unless
  the dialog sets `top()`. Examples:
  - `PausedDialog`: Java buttons at y≈190–405 of 648;
  - the editor menu: y≈95–425;
  - Map Info: y≈105–415;
  - Game Over: a `black3` pane with `.center().minWidth(370).maxSize(600, 550)`.
- Port: `mind_dialog.gd` fill mode anchors `cont` at the top.
- Change: centre `cont` vertically in the space between the title bar and the `buttons` row.
  Respect explicit `top()` alignment where a Java dialog sets it (Custom Game/Load Game/Core
  Database use `top()` on their panes; check each `cont.top()` call in the Java dialog).
- Accept: the pause menu, editor menu, Map Info and Game Over frames line up with the Java frames
  above (±4 px).

## T3 — Re-verify every dialog that used a workaround — HIGH

With real clicks and the real pointer (`tools/twin.sh` `GC`/`GP`), open each of these **once**
per shape and capture the acceptance frame only:
- Map Play;
- editor Map Info;
- Custom Game (search `sand`, then clear — uses T4's `text` path);
- editor maps;
- Load Game (search);
- Schematics;
- Core Database;
- Content info;
- Research;
- Logic editor;
- every editor sub-dialog.

Compare each capture with a Java frame and record MATCH or a new item. The exhaustive
double-open / orientation-switch sweep belongs to the round-M closing walk (L08 T6), not here.

## T4 — Search field text injection for tests — MEDIUM (harness, land first)

Injected `godot_input key` events carry no `unicode`, so a focused `LineEdit` gets no text. Add
a `text` action to `tools/gmcp.py` that sends one `InputEventKey` per character with
`unicode` set (`pressed` then released), via a small runtime helper method in the
open-godot-mcp runtime autoload. Without that, the search tests in T3 have to fall back to
`text_changed.emit`, which bypasses the field. Document the result in
`open-godot-mcp-learnings.md`.

## Done criteria

T4 landed first. T1 and T2 landed with `ui_layout_check.gd` in the import gate. T3's
acceptance frames are captured with real input and every frame is MATCH or a recorded item.
Ledger: PAR-0410 closed; the "Map Play portrait clipping" item (J06 T7) closed with fresh
evidence; new items for any T3 residue. Summary in `docs/L01-ui-layout-engine_summary.md`.
