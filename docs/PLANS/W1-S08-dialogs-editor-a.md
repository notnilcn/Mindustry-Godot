<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W1-S08 — `batch_d.rs` (2177) + `batch_e.rs` (5577) → `editor/`, `pickers/`, `dev/`

**Wave:** W1. **Needs:** F2 merged. **Procedure:** [W1-DIALOGS-COMMON](W1-DIALOGS-COMMON.md).
**Owns:**
- `ui/dialogs/{batch_d,batch_e}.rs`;
- the leaves `editor/{util_d,util_e,editor_import,editor_export,editor_menu,editor_map,map_info,map_resize,map_generate,wave_info,map_objectives,map_locales}.rs` (each may become a directory, see COMMON);
- `pickers/{file_chooser,icon_select}.rs`;
- `dev/**` (single owner).

`editor/` is shared with S09: use only the leaves listed here.
**Effort:** large.

Line numbers are at `291e30a8`. Check every id against the real `dispatch`.

## `batch_d.rs`

| Lines | Item | Id | Destination | Java |
|---|---|---|---|---|
| 71–409 | shared: `defer_action` 79, `ui_call` 97, `open_named_dialog` 106, `strip_colors` 384, … | – | `editor/util_d.rs` | – |
| 410–476 | `WidgetGalleryDialog` | `widget_gallery` | `dev/widget_gallery.rs` | none (dev gallery) |
| 477–643 | editor import | `editor_import` | `editor/editor_import.rs` | `editor/MapEditorDialog.java` |
| 644–733 | editor export | `editor_export` | `editor/editor_export.rs` | `editor/MapEditorDialog.java` |
| 734–979 | `FileChooserDialog` (incl. `emit_dialog_signal` 942) | `file_chooser` | `pickers/file_chooser.rs` | `ui/dialogs/FileChooserDialog.java` |
| 980–1291 | `IconSelectDialog` | `icon_select` | `pickers/icon_select.rs` | `ui/dialogs/IconSelectDialog.java` |
| 1292–1656 | `EditorMenuDialog` | `editor_menu` | `editor/editor_menu.rs` | `editor/MapEditorDialog.java` (menu) |
| 1657–1748 | confirm wiring: `connect_confirm_prompt` 1669, `editor_maps_request_rebuild` 1741 (**sets meta `"_batch_d_maps_rebuild"`; `batch_i` reads it — keep the string**) | – | `editor/util_d.rs` | – |
| 1749–2140 | `EditorMapDialog` | `editor_map` | `editor/editor_map.rs` | `ui/dialogs/EditorMapsDialog.java` (`showMap`) |
| 2141–2177 | tests: `file_extension`, `path_join`, `simplify_path` → `file_chooser.rs`; `strip_colors` → `util_d.rs` | | | |

## `batch_e.rs`

| Lines | Item | Id | Destination | Java |
|---|---|---|---|---|
| 64–750 | shared: requests 75, Variant 102, JSON 197, meta 224, editor facade 282, UI 379, widgets 527 | – | `editor/util_e.rs` | – |
| 751–1061 | `MapInfoDialog` | `map_info` | `editor/map_info.rs` | `editor/MapInfoDialog.java` |
| 1062–1246 | `MapResizeDialog` | `map_resize` | `editor/map_resize.rs` | `editor/MapResizeDialog.java` |
| 1247–2511 | `MapGenerateDialog` (~1265 lines) | `map_generate` | `editor/map_generate.rs` | `editor/MapGenerateDialog.java` |
| 2512–3669 | `WaveInfoDialog` (~1157) | `wave_info` | `editor/wave_info.rs` | `editor/WaveInfoDialog.java` |
| 3670–5507 | `MapObjectivesDialog` (**~1838 lines**) | `map_objectives` | `editor/map_objectives/`: `git mv` the stub to `map_objectives/mod.rs` and split by section comments into 2–3 children (e.g. `list.rs`, `edit.rs`, `canvas.rs`) | `editor/MapObjectivesDialog.java` |
| 5508–5518 | `MapLocalesDialog`: an empty stub with a stray `// __APPEND__` marker. Move it verbatim, marker included; it is listed in 02-FINDINGS #6 | `map_locales` | `editor/map_locales.rs` | `editor/MapLocalesDialog.java` |
| 5519–5577 | tests: `verts_color_bits`, `editor_pos` → `map_objectives`; others → the file of the code they test | | | |

## Notes

- `util_d.rs` and `util_e.rs` both contain `defer_action`, `ui_call` and a `flat_box` (e:573, and
  its body differs from f:299). Keep both copies in W1.
- `batch_e`'s "editor facade" helpers (282) call `MindEditor` by string. The strings stay.

## Done when

Both files are shims, the checks in W1-DIALOGS-COMMON pass, and `moves/S08.tsv` and
`reports/S08.md` are written.
