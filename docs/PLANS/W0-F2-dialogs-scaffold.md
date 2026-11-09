<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W0-F2 — Dialog directory scaffold + `dialogs/common`

**Wave:** W0, after F0, in parallel with F1 and F3.
**Owns:** `client/rust/mind-gdext/src/ui/dialogs/mod.rs`, plus every **new** file listed below
(group `mod.rs` files, stub leaves, `common/**`). It does **not** touch any `batch_*.rs`,
`misc.rs`, `simple.rs` or `sector.rs`.
**Effort:** medium (mostly typing; the risk is low).

## Why this package exists

Five W1 packages (S03, S04, S06–S10) move dialogs out of the eleven `batch_*.rs` files into
directories named after the Java classes. Several of them put files into the **same** group
directory: `editor/` gets files from S08 and S09, `campaign/` from S03, S04 and S07, `pickers/`
from S03, S07, S08 and S10. If each package created its group's `mod.rs`, they would conflict.

This package creates **all** group `mod.rs` files and **all** leaf files up front, as empty stubs.
The W1 packages then only *fill* leaf files they own, and never edit a `mod.rs` they do not own.

## How dialogs are registered (do not change any of this)

- `MindDialog::ready` (`ui/dialog.rs` ~1004) resolves the dialog id from
  `res://ui/dialogs_manifest.json` (`manifest_dialog_id`, ~1046), then calls
  `super::dialogs::controller_for(&id)` (~1020).
- `controller_for` in `dialogs/mod.rs` tries each module's `dispatch(id)` in turn. Each
  `dispatch` is a `match` on the id string. Ids are unique across modules.
- No `.tscn` names a Rust module, so module paths are free to change. The ids, the manifest, the
  `EXPECTED_*_DIALOGS` lists in `mind-core/src/ui/manifest.rs` ~357–410, the signal union in
  `dialog.rs` ~808–859 and every node meta-key string (`"_batch_d_maps_rebuild"`,
  `"_batch_c_frame_"`, `"_campaign2_frame_"`, `"mind_dialog_request"`) are runtime ABI. Never rename them.

## Steps

### 1. Worktree and baseline (RULES §1, §6)

### 2. Create the tree

Every new file gets this header (exception to RULES §3.7: the stub has no port yet):
```rust
// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! <one line: what will live here>. Filled by <package ID> (docs/PLANS).
//! Upstream: <Java file, from the table below>
```

There are two kinds of leaf file:
- **D** (*dispatch leaf*). It will hold dialog controllers. Its stub body is:
  ```rust
  use crate::ui::dialog::DialogController;

  pub(super) fn dispatch(_id: &str) -> Option<Box<dyn DialogController>> {
      None
  }
  ```
- **U** (*util leaf*). It holds helpers only. Its body is just the header.

**Subdir** entries create `X/mod.rs` as a dispatch leaf; the filling package owns the whole directory.

A group's `mod.rs` declares `pub mod <leaf>;` for every leaf. For the D leaves it has:
```rust
use crate::ui::dialog::DialogController;

pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    first::dispatch(id)
        .or_else(|| second::dispatch(id))
        // … every D leaf, in the table's order
}
```

Paths are relative to `client/rust/mind-gdext/src/ui/dialogs/`. Java paths are relative to `core/src/mindustry/`.

| Group dir | Leaf | Kind | Filled by | Upstream |
|---|---|---|---|---|
| `menu/` | `about.rs` | D | S06 | `ui/dialogs/AboutDialog.java` |
| | `credits.rs` | D | S06 | none (port-only; was `client/ui/dialogs/credits_dialog.gd`) |
| | `discord.rs` | D | S06 | `ui/dialogs/DiscordDialog.java` |
| | `full_text.rs` | D | S06 | `ui/dialogs/FullTextDialog.java` |
| `settings/` | `settings_menu.rs` | D | S06 | `ui/dialogs/SettingsMenuDialog.java` |
| | `language.rs` | D | S06 | `ui/dialogs/LanguageDialog.java` |
| | `keybind.rs` | D | S06 | `ui/dialogs/KeybindDialog.java` |
| | `util.rs` | U | S06 | none (shared helpers of the old batch_a) |
| `database/` | `effects.rs` | D | S06 | `ui/dialogs/EffectsDialog.java` |
| | `database_dialog.rs` | D | S06 | `ui/dialogs/DatabaseDialog.java` |
| | `content_info.rs` | D | S06 | `ui/dialogs/ContentInfoDialog.java` |
| `game/` | `paused.rs` | D | S07 | `ui/dialogs/PausedDialog.java` |
| | `game_over.rs` | D | S07 | `ui/dialogs/GameOverDialog.java` |
| | `load.rs` | D | S07 | `ui/dialogs/LoadDialog.java`, `SaveDialog.java` |
| `net/` | `util.rs` | U | S07 | none (join/host plumbing) |
| | `host.rs` | D | S07 | `ui/dialogs/HostDialog.java` |
| | `join/` | subdir | S07 | `ui/dialogs/JoinDialog.java` |
| | `roster.rs` | U | S07 | none (shared bans/admins state) |
| | `bans.rs` | D | S07 | `ui/dialogs/BansDialog.java` |
| | `admins.rs` | D | S07 | `ui/dialogs/AdminsDialog.java` |
| | `trace.rs` | D | S07 | `ui/dialogs/TraceDialog.java` |
| `pickers/` | `list_select.rs` | D | S07 | none (generic select picker) |
| | `file_chooser.rs` | D | S08 | `ui/dialogs/FileChooserDialog.java` |
| | `icon_select.rs` | D | S08 | `ui/dialogs/IconSelectDialog.java` |
| | `color_picker.rs` | D | S10 | `ui/dialogs/ColorPicker.java` |
| | `palette.rs` | D | S03 | `ui/dialogs/PaletteDialog.java` |
| `campaign/` | `util.rs` | U | S03 | none (shared helpers of the old batch_campaign2) |
| | `research.rs` | D | S03 | `ui/dialogs/ResearchDialog.java` |
| | `research_root_select.rs` | D | S03 | `ui/dialogs/ResearchDialog.java` (inline root-select `BaseDialog`, ~:86) |
| | `loadout.rs` | D | S03 | `ui/dialogs/LoadoutDialog.java` |
| | `launch_loadout.rs` | D | S03 | `ui/dialogs/LaunchLoadoutDialog.java` |
| | `sector_stats.rs` | D | S03 | `ui/dialogs/PlanetDialog.java` (`showStats`) |
| | `sector_select.rs` | D | S03 | `ui/dialogs/SectorSelectDialog.java` |
| | `campaign_rules.rs` | D | S07 | `ui/dialogs/CampaignRulesDialog.java` |
| | `campaign_complete.rs` | D | S07 | `ui/dialogs/CampaignCompleteDialog.java` |
| | `planet/` | subdir | S04 | `ui/dialogs/PlanetDialog.java` |
| `maps/` | `map_list.rs` | U | S03 | `ui/dialogs/MapListDialog.java` |
| | `custom_game.rs` | D | S03 | `ui/dialogs/CustomGameDialog.java` |
| | `map_play.rs` | D | S03 | `ui/dialogs/MapPlayDialog.java` |
| `schematics/` | `schematics_dialog.rs` | D | S03 | `ui/dialogs/SchematicsDialog.java` |
| | `files.rs` | U | S03 | `ui/dialogs/SchematicsDialog.java` (.msch persistence helpers) |
| | `subdialogs.rs` | U | S03 | `ui/dialogs/SchematicsDialog.java` (tag/edit sub-dialogs) |
| | `import_export.rs` | U | S03 | `ui/dialogs/SchematicsDialog.java` (`showImport`/`showExport`) |
| | `schematic_info.rs` | D | S09 | `ui/dialogs/SchematicsDialog.java` (`SchematicInfoDialog`, ~:774–834) |
| `editor/` | `util_d.rs`, `util_e.rs`, `util_f.rs` | U | S08, S08, S09 | none (shared helpers of the old batch_d/e/f) |
| | `editor_import.rs`, `editor_export.rs`, `editor_menu.rs` | D | S08 | `editor/MapEditorDialog.java` |
| | `editor_map.rs` | D | S08 | `ui/dialogs/EditorMapsDialog.java` (`showMap`) |
| | `map_info.rs` | D | S08 | `editor/MapInfoDialog.java` |
| | `map_resize.rs` | D | S08 | `editor/MapResizeDialog.java` |
| | `map_generate.rs` | D | S08 | `editor/MapGenerateDialog.java` |
| | `wave_info.rs` | D | S08 | `editor/WaveInfoDialog.java` |
| | `map_objectives.rs` | D | S08 | `editor/MapObjectivesDialog.java` |
| | `map_locales.rs` | D | S08 | `editor/MapLocalesDialog.java` |
| | `map_processors.rs` | D | S09 | `editor/MapProcessorsDialog.java` |
| | `map_load.rs` | D | S09 | `editor/MapLoadDialog.java` |
| | `sector_generate.rs` | D | S09 | `editor/SectorGenerateDialog.java` |
| | `banned_content.rs` | D | S09 | `editor/BannedContentDialog.java` |
| | `editor_maps.rs` | D | S09 | `ui/dialogs/EditorMapsDialog.java` |
| | `map_assets/` | subdir | S09 | `editor/data/MapAssetsDialog.java` |
| `dev/` | `widget_gallery.rs` | D | S08 | none (developer widget gallery) |
| `rules/` | `custom_rules.rs` | D | S09 | `ui/dialogs/CustomRulesDialog.java` |
| | `util.rs` | U | S09 | none (shared helpers of the old batch_i) |
| `logic/` | `canvas_edit.rs` | D | S09 | `ui/dialogs/CanvasEditDialog.java`, `world/blocks/logic/CanvasBlock.java` |
| | `editor/` | subdir | S10 | `logic/LogicDialog.java`, `LCanvas.java`, `LStatement.java` |
| `mods/` | `mods_dialog.rs` | D | S10 | `ui/dialogs/ModsDialog.java` |
| | `mod_browser.rs` | D | S10 | `ui/dialogs/ModBrowserDialog.java` |
| | `util.rs` | U | S10 | none (shared helpers of the old batch_g) |

**Naming rule:** a leaf must not have the same name as its group (`database/database.rs`), because
`clippy::module_inception` is denied (`clippy::all = "deny"`). Such leaves get a `_dialog` suffix:
`database/database_dialog.rs`, `mods/mods_dialog.rs`, `schematics/schematics_dialog.rs`.

Check the Java paths as you go (`ls /home/clintonnguyen/g/code_examples/Mindustry/core/src/mindustry/ui/dialogs/`).
Fix any path that does not exist and note it in the report.

### 3. `common/` (real code, small)

- **`common/mod.rs`**: `pub mod defer; pub mod facade;`.
- **`common/defer.rs`**:
  - `type DeferredAction = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;`. This is the per-file alias every copy
    declares, e.g. `batch_b.rs:59`.
  - `pub(crate) fn defer_action(name: &'static str, action: impl FnOnce() + 'static)`. Copy the
    body byte-identically from `batch_b.rs` ~67–77 (the `DeferredAction` slot, `Callable::from_fn`,
    `call_deferred`). First `diff` the eight copies (`misc.rs` 608, `batch_b` 67, `d` 79, `f` 70, `g` 96,
    `h` 93, `j` 1385, plus `ui/fragments/player_list.rs` ~914 and `ui/editor_views.rs` ~65). They are
    expected to be identical. If one differs, note it; do not merge it.
  - `Deferred` + `install_frame_tick`. Copy them from `batch_campaign2.rs` ~500–554:
    - `pub(crate) struct Deferred(Rc<RefCell<Vec<Box<dyn FnMut()>>>>)`, keeping its `#[allow(clippy::type_complexity)]`
      and `#[derive(Clone, Default)]`;
    - `push`/`drain` become **`pub(crate)`**; in the original they are private.
    - `batch_c.rs` ~405–466 has a copy that differs only in the meta-key prefix
      (`"_campaign2_frame_"` vs `"_batch_c_frame_"`) and the callable name (`"campaign2_frame"` vs
      batch_c's). Make those two strings **parameters**, so each caller keeps its exact strings:
      ```rust
      pub(crate) fn install_frame_tick(
          dialog: &mut MindDialog,          // NOT &Gd<MindDialog>: callers run inside bind_mut; a Gd would re-bind and panic
          meta_prefix: &str,                // "_campaign2_frame_" / "_batch_c_frame_"
          callable_name: &'static str,      // "campaign2_frame" / batch_c's name
          key: &str,
          deferred: Deferred,
          tick: impl FnMut() + 'static,
      )
      ```
      The body is the original with `format!("{meta_prefix}{key}")` and `Callable::from_fn(callable_name, …)`.
      If the two bodies differ in anything besides those two strings, stop and record it.
- **`common/facade.rs`**:
  - `pub(crate) fn ui_call(method: &str, args: &[Variant]) -> Option<Variant>`. The body is
    identical in 8 copies: `let mut ui = kit::ui()?; if !ui.has_method(method) { return None; } Some(ui.call(method, args))`.
  - `pub(crate) fn rich(label: &Gd<MindLabel>) -> Gd<RichTextLabel> { label.clone().upcast() }`.

No existing file calls these yet. Add `#[allow(dead_code)] // adopted in W2 (docs/PLANS)` where clippy needs it.

### 4. Rewrite `dialogs/mod.rs`

Keep the header and doc; update the doc to describe the groups. Declare `pub mod common;` and
every group (alphabetical), **then** the existing `pub mod batch_*; misc; sector; simple;` lines
unchanged. `controller_for` tries the **new groups first**, then the old modules in their current
order. Ids are unique, so the order cannot change which controller answers. This is still true
mid-wave, because each W1 package moves an id from a batch file to a group leaf in a single commit.

### 5. Test (optional but wanted)

`batch_campaign2.rs` ~7441 has a dispatch test that builds controllers without the engine. If that
works, add `#[cfg(test)] mod tests` to `dialogs/mod.rs` with
`every_manifest_id_resolves_exactly_once`. It reads `../../ui/dialogs_manifest.json` relative to
`env!("CARGO_MANIFEST_DIR")` (check the real path; the manifest is `client/ui/dialogs_manifest.json`).
For each entry `name`, it counts how many group or batch `dispatch` fns return `Some`, and asserts
1. If entries legitimately have no controller, assert ≤ 1 and list those ids in the report.

### 6. Verify (RULES §6) and report

- `api_surface.py` diff is empty.
- Scoped clippy is clean.
- `run --cargo-test -p mind-gdext --lib dialogs`.

Report the final tree (`find ui/dialogs -name '*.rs' | sort`), and the CI entry with gates
`fmt,clippy,cargo-check,cargo-test,dialog-sweep` (risk: "controller_for order; a stub leaf
shadowing an id"). The orchestrator runs `dialog_sweep_check` after the merge.

## Note for the W1 packages that fill these files

When you move a batch file's helpers that **other** batch files import by path, the old file
keeps `pub(crate) use` re-exports (RULES §3, shim). At `291e30a8` these are:
- `batch_a::open_content_info` (used by `batch_g.rs` ~2255);
- `misc::trace_context_json` (used by `ui/fragments/player_list.rs:34`);
- `batch_campaign2::{Deferred, install_frame_tick, MapListState, maplist_*}` (used by `batch_i.rs` ~58–61).

Code you move that **uses** such an item must import it by its full old path, e.g.
`crate::ui::dialogs::batch_campaign2::MapListState`. That path stays valid in every merge order;
C01 repoints the imports to the new homes.
