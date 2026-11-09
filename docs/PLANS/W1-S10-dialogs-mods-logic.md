<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W1-S10 — `batch_g.rs` (3907) + `batch_j.rs` (5516) → `mods/`, `pickers/color_picker.rs`, `logic/editor/`

**Wave:** W1. **Needs:** F2 merged. **Procedure:** [W1-DIALOGS-COMMON](W1-DIALOGS-COMMON.md).
**Owns:**
- `ui/dialogs/{batch_g,batch_j}.rs`;
- `ui/dialogs/mods/**` (single owner);
- `ui/dialogs/pickers/color_picker.rs`;
- `ui/dialogs/logic/editor/**` (F2 created only its `mod.rs` stub; you own the directory).

**Effort:** large.

Line numbers are at `291e30a8`. Check every id against the real `dispatch`.

## `batch_g.rs`

| Lines | Item | Id | Destination | Java |
|---|---|---|---|---|
| 82–340 | shared: `defer_action` 96, `ui_call` 109, `strip_colors` 259, … | – | `mods/util.rs` (used by `color_picker` too, so `pub(in crate::ui::dialogs)`) | – |
| 341–842 | `ColorPickerDialog` | `picker` (check) | `pickers/color_picker.rs` | `ui/dialogs/ColorPicker.java` |
| 843–2657 | `ModsDialog` (**~1815 lines**); calls `super::batch_a::open_content_info` ~2255, so write `crate::ui::dialogs::batch_a::open_content_info` | `mods` | `mods/mods_dialog.rs` plus siblings split by section comments (e.g. `mods_list.rs`, `mods_import.rs`, `mods_detail.rs`). Add `pub mod` lines to `mods/mod.rs` (single owner) | `ui/dialogs/ModsDialog.java` |
| 2658–3871 | `ModBrowserDialog` (~1213) | `mod_browser` | `mods/mod_browser.rs` | `ui/dialogs/ModBrowserDialog.java` |
| 3872–3907 | tests: `arc_rgba` → `color_picker`; `version_*` → `mod_browser`; `strip_colors` → `util.rs` | | | |

## `batch_j.rs`: the logic editor (`LogicDialog`) → `logic/editor/`

Upstream: `logic/LogicDialog.java`, `logic/LCanvas.java`, `logic/LStatement.java`. The file is one
dialog, laid out in sections. Map each section to a child of `logic/editor/` (section start lines;
each runs to the next one):

| Start | Section | File |
|---|---|---|
| 58 | constants | `consts.rs` |
| 123 | statement model | `model.rs` |
| 233 | dict helpers | `util.rs` (along with the 555 node helpers) |
| 256 | state (`LogicState` 299) | `state.rs` |
| 387 | text helpers | `text.rs` |
| 555 | node helpers | `util.rs` |
| 679 | state queries | `queries.rs` |
| 1273 | style | `style.rs` |
| 1334 | signal plumbing (`defer_action` 1385) | `signals.rs` |
| 1397 | controller (`LogicDialog` 1402, `dispatch` 1451) | `mod.rs` (replaces the F2 stub; the real `dispatch` lives here) |
| 1458 | shell | `shell.rs` |
| 1609 | statement column | `canvas.rs` |
| 1794 | field row | `fields.rs` |
| 2246 | select popups | `select.rs` |
| 2547 | jump nodes | `jumps.rs` |
| 2724 | drag reorder | `drag.rs` |
| 2926 | lane prediction | `lanes.rs` |
| 3108 | **`MindLogicOverlay`** (GodotClass `#[class(base=Control)]`; keep the name and every impl block) | `overlay.rs` |
| 3456 | variables | `variables.rs` |
| 3685 | edit menu | `edit_menu.rs` |
| 3866 | add menu | `add_menu.rs` |
| 4324 | load/save | `io.rs` |
| 4534 | content pickers | `pickers.rs` |
| 5431 | tests (4, covering `select`, `fields`, `pickers`) | `tests.rs` (or inline in those files) |

These sections are all one module today, so they call each other freely. Make each moved item
`pub(super)` (scope `logic::editor`) and start each child with `use super::*;`. `mod.rs` then does
`use` of the child items it needs. The simplest way: in `mod.rs`, write
`mod consts; use consts::*;` and so on for each child, so that `use super::*` in every child sees
everything. That is allowed glue.

## Done when

Both files are shims, the checks in W1-DIALOGS-COMMON pass, `api_surface.py` still lists
`class MindLogicOverlay` with the same funcs, and `moves/S10.tsv` and `reports/S10.md` are written.
