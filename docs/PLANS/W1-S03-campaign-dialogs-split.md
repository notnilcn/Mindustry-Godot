<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W1-S03 — Split `batch_campaign2.rs` (7471 lines) and `sector.rs` into named dialog files

**Wave:** W1 (pure mechanical split, RULES §3). **Priority:** first group.
**Needs:** F2 merged (the stub leaves exist).
**Owns** (paths relative to `client/rust/mind-gdext/src/ui/dialogs/`):
- `batch_campaign2.rs`, `sector.rs`;
- the leaves `campaign/{util,research,research_root_select,loadout,launch_loadout,sector_stats,sector_select}.rs`;
- `maps/{map_list,custom_game,map_play}.rs`;
- `schematics/{schematics_dialog,files,subdialogs,import_export}.rs`;
- `pickers/palette.rs`.

You never edit a group `mod.rs`; F2 owns those.

**Effort:** large, mechanical.

## Background

Read the "How dialogs are registered" section of [W0-F2](W0-F2-dialogs-scaffold.md). In short, the
dialog id → controller mapping is a string `match` in each module's `dispatch`. Ids, manifest
entries, meta-key strings and signal names are ABI.

`batch_campaign2.rs` ports seven Java dialogs; its `dispatch` (58–69) answers `research`,
`schematics`, `loadout`, `launch_loadout`, `sector_stats`, `custom`, `map_play`. `sector.rs`
(293 lines) answers `sector_select`, `research_root_select`, `palette`.

## Destination table (lines at `291e30a8`)

| Destination (under `dialogs/`) | From | Contents |
|---|---|---|
| `campaign/util.rs` (U) | bc2 71–723 | Shared helpers: `row_*` 98–156, `parse_object/array` 158–186, `text_of`/`int_of` 193–253, `ui_call`, `setting_*` 271–306, `open/close_dialog`, `dialog_node`, `spacer` 392, `content_texture` 410, `icon_rect` 420, `panel_with_box` 432, `strip_color_tags` 441, `connect_once` 477, `Deferred` 503 + `install_frame_tick` 519 (already `pub(crate)`), `bundle_or` 557, `format_millis` 663, and every other item in 71–723. Visibility: these are used from `campaign/`, `maps/`, `schematics/` and `pickers/`, so use **`pub(in crate::ui::dialogs)`** (RULES §3.2, scattered users). Keep `pub(crate)` where an item already has it. Keep the existing `#[allow(dead_code)]` on `spacer`/`panel_with_box`. |
| `campaign/research.rs` (D: `research`) | bc2 724–1604 | `ResearchState` 733, `ResearchDialog` 756, popup and requirement rows 1277–1454, `root_select_visible` 1572, `row_text` 1598 |
| `schematics/schematics_dialog.rs` (D: `schematics`) | bc2 1605–2584 | enums 1666–1706, `SchematicsDialog` 1750, tags, list, cards |
| `schematics/files.rs` (U) | bc2 2585–2879 | `.msch` persistence helpers (pure fns) |
| `schematics/subdialogs.rs` (U) | bc2 2880–3898 | all-tags, icon-tags, edit, add-tag sub-dialogs |
| `schematics/import_export.rs` (U) | bc2 3899–4287 | `showImport`/`showExport`, file dialogs |
| `campaign/loadout.rs` (D: `loadout`) | bc2 4288–4718 | `LoadoutDialog` 4313 |
| `campaign/launch_loadout.rs` (D: `launch_loadout`) | bc2 4719–5257 | `LaunchLoadoutDialog` 4741 |
| `campaign/sector_stats.rs` (D: `sector_stats`) | bc2 5258–5726 | `SectorStatsDialog` 5285 |
| `maps/map_list.rs` (U) | bc2 5727–5805 **and** 5850–6748 | MapList base: `MapListState` 5770, all `maplist_*` (`pub(crate)` stays), `MAP_SIZE`, `MAP_COLUMN_WIDTH`, `MAP_CARD_MARGIN`, `MapShowHook`, … |
| `maps/custom_game.rs` (D: `custom`) | bc2 5806–5849 | `CustomGameDialog` (it sits in the middle of the map-list block) |
| `maps/map_play.rs` (D: `map_play`) | bc2 6749–7222 | `MapPlayDialog` 6786 |
| `campaign/sector_select.rs` (D: `sector_select`) | sector.rs 24–128 | `SectorSelectDialog` |
| `campaign/research_root_select.rs` (D: `research_root_select`) | sector.rs 129–238 | `ResearchRootSelectDialog` |
| `pickers/palette.rs` (D: `palette`) | sector.rs 239–282 | `PaletteDialog` |

**Dispatch arms.** Each D leaf replaces its stub `dispatch` with a `match` holding exactly the arm(s)
for its id(s). Copy the arm *expression* verbatim from the old `dispatch` (for example
`"research" => Some(Box::new(ResearchDialog::default())),`), then `_ => None`. In the same commit,
delete that arm from the old file's `dispatch`. An id must answer from exactly one place in every
commit.

**Splitting further.** `schematics/schematics_dialog.rs` (~980 lines) and `campaign/research.rs` (~880) are
within the size target. Do not split them further in W1.

## Shims

When `batch_campaign2.rs` is empty, it becomes a shim (RULES §3). It keeps:
```rust
//! Moved to dialogs/{campaign,maps,schematics,pickers}/. Deleted in W5 (C01).
use crate::ui::dialog::DialogController;
pub(super) fn dispatch(_id: &str) -> Option<Box<dyn DialogController>> { None }
pub(crate) use super::campaign::util::{Deferred, install_frame_tick};
pub(crate) use super::maps::map_list::{MapListState, /* every maplist_* fn batch_i.rs imports */};
```
Get the exact list from `batch_i.rs` ~58–61:
`grep -n "batch_campaign2" client/rust/mind-gdext/src -r`.

`sector.rs` becomes a shim with the stub `dispatch` only, unless the grep shows an outside user.

## Tests (bc2 7223–7471, sector.rs 283–293)

Each test goes to a `#[cfg(test)] mod tests` at the end of the file that owns the code under test
(start lines):

| Tests | Go to |
|---|---|
| 7229 (`format_millis`), 7373, 7387, 7404 (markup/strip helpers) | `campaign/util.rs` |
| 7237, 7394 | `campaign/loadout.rs` |
| 7246, 7253–7372 | `schematics/files.rs`, or `schematics_dialog.rs` where the test touches dialog state |
| 7417 | `maps/map_list.rs` |
| 7458 | `campaign/research.rs` |
| sector.rs 283 | the file of the dialog it tests |
| **7441** (the dispatch test) | see below |

The dispatch test asserts that the ids resolve. Rewrite **only its call target** from
`dispatch(id)` to `crate::ui::dialogs::controller_for(id)`, and put it in `campaign/util.rs`. This
is the one allowed test edit. Note it in the report.

## Upstream citations

New file headers name the Java class, per F2's table. `sector.rs` had no citation (02-FINDINGS #12);
its three new files get one.

## Verification (RULES §6)

```bash
python3 tools/refactor/api_surface.py | diff /tmp/S03-surface-before.txt -    # empty
B=$(git merge-base main HEAD); D=client/rust/mind-gdext/src/ui/dialogs
python3 tools/refactor/move_check.py --base $B --old $D/batch_campaign2.rs --new $D/batch_campaign2.rs $D/campaign $D/maps $D/schematics $D/pickers/palette.rs
python3 tools/refactor/move_check.py --base $B --old $D/sector.rs --new $D/sector.rs $D/campaign $D/pickers/palette.rs
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p mind-gdext --all-targets -- -D warnings
run --cargo-test -p mind-gdext --lib dialogs
```

`move_check` will also count lines in leaf files that **other** packages filled, if they have merged
already. Those show up as `only-in-new` and are fine. Only `only-in-old` matters.

Orchestrator post-merge: `dialog_sweep_check`. Its pass line is `DIALOGCHECK: failed=0`.
