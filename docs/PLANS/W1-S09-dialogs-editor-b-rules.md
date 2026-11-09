<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W1-S09 — `batch_f.rs` (4682) + `batch_i.rs` (4553) → `editor/`, `rules/`, `schematics/`, `logic/`

**Wave:** W1. **Needs:** F2 merged. **Procedure:** [W1-DIALOGS-COMMON](W1-DIALOGS-COMMON.md).
**Owns:**
- `ui/dialogs/{batch_f,batch_i}.rs`;
- the leaves `editor/{util_f,map_processors,map_load,sector_generate,banned_content,editor_maps}.rs` and `editor/map_assets/**`;
- `schematics/schematic_info.rs`;
- `rules/**` (single owner);
- `logic/canvas_edit.rs`.

**Effort:** large.

Line numbers are at `291e30a8`. Check every id against the real `dispatch`.

## `batch_f.rs`

| Lines | Item | Id | Destination | Java |
|---|---|---|---|---|
| 62–366 | shared: `defer_action` 70, `ui_call` 83, `connect_prompt_once` 136, `clear_children` 158, `flat_box` 299, `square_cell` 313 | – | `editor/util_f.rs` | – |
| 367–874 | map processors | `map_processors` | `editor/map_processors.rs` | `editor/MapProcessorsDialog.java` |
| 875–1127 | map load | `map_load` | `editor/map_load.rs` | `editor/MapLoadDialog.java` |
| 1128–1493 | sector generate | `sector_generate` | `editor/sector_generate.rs` | `editor/SectorGenerateDialog.java` |
| 1494–2101 | banned content | `banned_content` | `editor/banned_content.rs` | `editor/BannedContentDialog.java` |
| 2102–4303 | map assets: controller 2102–2516, helpers 2517–3038, `MapPatchesView` 3039–3204, `MapContentView` 3205–3541, `MapBundlesView` 3542–3696, `MapImagesView` 3697–4067, `MapAudioView` 4068–4303 | `map_assets` | `editor/map_assets/{mod.rs (controller + dispatch), helpers.rs, patches.rs, content.rs, bundles.rs, images.rs, audio.rs}` | `editor/data/MapAssetsDialog.java` (+ its view classes, if ported 1:1) |
| 4304–4552 | `SchematicInfoDialog` | `schematic_info` | `schematics/schematic_info.rs` | `ui/dialogs/SchematicsDialog.java` (`SchematicInfoDialog`, ~:774–834) |
| 4553–4682 | tests: `processor_markup` → `map_processors`; `banned_keys` → `banned_content`; `asset_type`, `audio_stream`, `count_fields`, `error_codes`, `auto_fixed` → `map_assets` (whichever child); `schematics_fall_back` → `schematic_info`; `integer_validation`, `engine_available` → `util_f.rs` | | | |

## `batch_i.rs`

| Lines | Item | Id | Destination | Java |
|---|---|---|---|---|
| 58–61 | `use super::batch_campaign2::{Deferred, MapListState, install_frame_tick, maplist_*}` | – | In each moved file that needs them, write `use crate::ui::dialogs::batch_campaign2::{…};` (old full path; the S03 shim keeps it valid) | – |
| 73–427 | shared (incl. a `connect_prompt_once` at 230) | – | `rules/util.rs`. It is used by `editor_maps` and `canvas_edit` too, so use `pub(in crate::ui::dialogs)` | – |
| 428–3210 | `CustomRulesDialog` (**~2780 lines**) | `custom_rules` | `rules/custom_rules/`: `mod.rs` holds the controller and dispatch; split the rest by its section comments into about 3–5 children (for example the rule categories, team rules, the weather/env editor, the planet rules picker). Name the children after their sections | `ui/dialogs/CustomRulesDialog.java` |
| 3211–3613 | `EditorMapsDialog`; reads the `"_batch_d_maps_rebuild"` meta (~3216–3218), keep it | `editor_maps` | `editor/editor_maps.rs` | `ui/dialogs/EditorMapsDialog.java` |
| 3614–4485 | canvas edit, incl. `CanvasSurfaceState` (3630) and the GodotClass **`MindCanvasSurface`** (3645–3709) (`#[class(base=Control, init)]`; keep the struct name) | `canvas_edit` | `logic/canvas_edit.rs` | `ui/dialogs/CanvasEditDialog.java`, `world/blocks/logic/CanvasBlock.java` |
| 4486–4553 | tests (all 3) → `logic/canvas_edit.rs` | | | |

`rules/` is single-owner: adding the `custom_rules/` children and `pub mod` lines inside it is
allowed. `rules/mod.rs` already declares `custom_rules` and `util`.

## Done when

Both files are shims (`batch_i.rs` re-exports nothing). The checks in W1-DIALOGS-COMMON pass.
`api_surface.py` still lists `class MindCanvasSurface`. `moves/S09.tsv` and `reports/S09.md` are written.
