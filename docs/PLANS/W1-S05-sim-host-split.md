<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W1-S05 — Split `MindSimHost` (`mind-gdext/src/sim_host.rs`, 13,500 lines) into `sim_host/`

**Wave:** W1 (pure mechanical split, RULES §3).
**Owns:** `client/rust/mind-gdext/src/sim_host.rs` and the new `client/rust/mind-gdext/src/sim_host/**`.
**Effort:** the largest W1 package, but mechanical. A **per-method destination table** is committed at
[`data/S05-sim-host-methods.tsv`](data/S05-sim-host-methods.tsv): 327 methods with `fn_line`,
`end_line`, kind and destination file, all at `291e30a8`.
**Cut by `fn_line` and `end_line` only.** The `doc_start` column is unreliable for some rows. For
example, row 1 (`bootstrap`) has `doc_start` after `fn_line`, and row 2's `doc_start` 965 would swallow the
three `#[signal]`s at 966–980. Find each method's doc and attributes by reading upward from `fn_line`.

## What the file is

`MindSimHost` is the Godot node (`client/scenes/game.tscn:10`) that owns the `Sim`, the fixed-step
frame pump, and about 160 `#[func]`s for MCP inspection and UI. It also has about 150 Rust-only `pub`
accessors used by `render/*`, `input/*`, `campaign.rs`, `net/mod.rs` and `ui/*`.
- `sim: Sim` is not an `Option`.
- The struct is at 638–748; four fields are `#[export]` (`fx`, `camera`, `input`, `preview`).
- `INode` is at 750–883.
- The primary `#[godot_api] impl` runs 884–10426: `bootstrap` 886–964, three `#[signal]`s 966–980
  (`state_changed`, `world_changed`, `world_loaded`), then everything else.
- A second plain `impl` is at 13327–13452.
- Tests are at 12213–13326 and 13453–13500.

## Target layout

`git mv sim_host.rs sim_host/mod.rs`. The `lib.rs` lines (`mod sim_host;` at 28,
`pub use sim_host::MindSimHost;` at 46) stay as they are.

**`sim_host/mod.rs` keeps** 1–27 and 270–301 (imports and constants; move the misplaced mid-file
`use` block at 270–290 up to the top), the struct (638–748), `INode` (750–883), and the
**primary** `#[godot_api] impl MindSimHost` with **only** `bootstrap` and the three signals
(close it right after the signals). It also keeps `TILESIZE_F` (10640) and `globalize` (11091),
unchanged and private (children already see them), plus the `mod` lines and re-exports.

Every other method goes to the file named in `data/S05-sim-host-methods.tsv`:
- kind `func`/`pub func` → `#[godot_api(secondary)] impl MindSimHost { … }` in that file;
- `private`, `pub`, `pub(crate)` → a plain `impl MindSimHost { … }` in that file (private methods become `pub(super)`).

The same file may hold both blocks.

**Free items and types** (not in the TSV) go by range:

| File | Free-item ranges at `291e30a8` | Upstream / role |
|---|---|---|
| `types.rs` | 28–269 (`BeltItemSlot`, `UnitDraw`, `MechDraw`, `TreadsDraw`, `CrawlDraw`, `PayloadDraw`, `LegsDraw`, `UnitCommandRow`, `UnitAbilityDraw`, `WeaponDraw`), 303–417 (`PlaceRangeKind`, `BuilderView`, `BulletDraw`, `LaserLink`, `BeamLaserLink`, `BridgeLink`), 589–637 (`BlockStateDraw`, `FireDraw`, `PuddleDraw`), 10468–10638 (`PadDraw`, `UnitBuildDraw`, `OverlayView`, `ConfigOverlay`, `SelectOverlay`) | view-data structs for `render/*` |
| `render_light.rs` | 418–441 (`POWER_GROW`, `BRIDGE_GROW`, `LIGHT_GROW`, `LightSource`), 443–462 (`rgba_f32`, `building_center`), 474–588 (`bullet_circle_light`, `environment_light`, `floor_light_sources`) | `core/Renderer.java`, `graphics/LightRenderer.java` |
| `player.rs` | 464–473 (`editor_team_choice`), 11431–11454 (`pub fn player_near_drop_zone_predicate`) | – |
| `render_units.rs` | 10427–10467 (`factory_preview`, `reconstructor_preview`, both pub(crate)) | – |
| `overlay.rs` | 10642–10659 (`power_laser_range`, `bridge_link_range`) | – |
| `command_panel.rs` | 10660–10762 (`command_panel`) | `ui/fragments/PlacementFragment.java` (command panel) |
| `campaign_link.rs` | 10763–10798 (free `core_items_json`) | – |
| `state.rs` | 10799–11027 (`hint_state_value`) | `ui/fragments/HintsFragment.java` |
| `config.rs` | 11028–11089 (`config_to_json`, free `block_items_json`). **Allowed fix:** lines 11029–11031 hold `block_items_json`'s doc comment sitting above `config_to_json`; put it back above `block_items_json` | – |
| `world_load.rs` | 11102–11135 and 11165–11304 (`LoadedGrid`, `WorldLoadExtras`, `GeneratedLoadout`, `sector_map_name`, `load_base_catalog`, `resolve_preset_map_path`, `load_grid_file`, `load_map_grid_file`, `take_loaded_grid`, `count_spawns`) and 11136–11163 (`sector_save_path`) | `core/World.java` |
| `inspect.rs` | 11306–11364 (`GenCoords`, `count_world_gen`, `pack_coords`) | – |
| `pump.rs` | 11365–11389 (`parse_capture_args`) | – |
| `world_query.rs` | 11390–11429 (`first_core_position`) | – |
| `bar_rows.rs` | 11455–12211 (`BarRow`, `TurretBars`, `PowerBars`, `GeneratorBars`, `UnitCapBars`, `PayloadLoaderBars`, `BarExtras` + impl, `amount`, `cap_text`, `push_unit_cap_rows`, `pub fn building_bar_rows`) | `world/blocks/*` `setBars()` |
| `logic.rs` | 13327–13452 (`PING_OVERLAY_PATH`, `pub fn compose_logic_player_view`, and the existing plain `impl` block with `camera_view_bounds`, `ping_state`, `logic_player_view`, `push_logic_player_view`), 13453–13500 (`mod logic_view_tests`, kept inline) | `logic/LExecutor.java` (player view) |
| `tests.rs` | 12213–13326 (`#[cfg(test)] mod tests` body) | – |

The 23 files are `mod`, `types`, `boot`, `pump`, `commands`, `state`, `inspect`, `config`, `logic`,
`io`, `world_load`, `campaign_link`, `plans`, `player`, `world_query`, `hover`, `render_units`,
`render_blocks`, `render_light`, `bars`, `bar_rows`, `command_panel` and `overlay`, plus `tests`.
(Compared with the analysis, `sector_load` was merged into `world_load`, `logic_display` into
`logic`, and `debug` into `inspect`.) Each new file gets the two-line header and a `//! Upstream:`
line (RULES §3.7).

If `logic.rs` exceeds about 1100 lines, put the `logic_display_*`/`logic_canvas_*` group (fn lines
2584–2947) into `logic_display.rs` instead. Do the same for `world_load.rs` (fn
`load_sector_world` 3633–3863 and `sector_save_path` → `sector_load.rs`). Record the choice.

## Re-exports in `sim_host/mod.rs`

Keep each type's original visibility:
```rust
pub use types::{BeltItemSlot, UnitDraw, MechDraw, TreadsDraw, CrawlDraw, PayloadDraw, LegsDraw,
    UnitCommandRow, UnitAbilityDraw, WeaponDraw, PlaceRangeKind, BuilderView, BulletDraw,
    LaserLink, BeamLaserLink, BridgeLink, BlockStateDraw, FireDraw, PuddleDraw, PadDraw,
    UnitBuildDraw, OverlayView};                          // only those that were `pub`
pub(crate) use types::{ConfigOverlay, SelectOverlay};
pub use bar_rows::building_bar_rows;                     // if it was used outside (grep)
pub use player::player_near_drop_zone_predicate;
pub use logic::compose_logic_player_view;
pub(crate) use render_units::{factory_preview, reconstructor_preview};
```
Verify each with `grep -rn "sim_host::" client/rust/mind-gdext/src`. The known importers are
`render/{units,unit_parts,unit_overlays,unit_abilities,pods,plans,bullets,light,overlay_top,build_beam}.rs`.
Several of those files have **uncommitted user edits**. You do not edit them; the re-exports keep
their imports valid.

## Order of work

1. **Pilot:** `git mv`, then move the accelerator methods (fn lines 6836–6962 →
   `campaign_link.rs`) into a `#[godot_api(secondary)]` block. Run
   `run --cargo-check -p mind-gdext`. If the secondary block fails to compile, stop and report.
   (S01 pilots this too; whichever runs first finds any problem.)
2. `types.rs`, plus the re-exports, then check.
3. One commit per destination file, in the TSV's order of first `fn_line`. Run `--cargo-check -p mind-gdext`
   after each file.
4. `tests.rs`. It used `use super::*`, which no longer reaches helpers that moved into child files.
   Add explicit imports, e.g. `use super::bar_rows::*; use super::render_light::{…};`. Make the
   needed items `pub(super)`.
5. Final clippy. Unused imports left in `mod.rs` are errors under `-D warnings` (`MemoryBundle`/`MemoryUnlockStore`
   are used only by `bootstrap`; `DirAccess`/`FileAccess` only by the loaders). Move each import to
   the file that uses it.

## Hazards

- **Name collisions.**
  - The free `core_items_json` vs the `#[func] core_items_json` (fn 7212 calls the free fn).
  - The free `block_items_json` vs `pub fn block_items_json` (fn 6786).
  - The imported `mind_core::content::content_counts` vs the `#[func] content_counts` (1700).

  Every file that calls a free fn must import it explicitly (`use super::campaign_link::core_items_json as free_core_items_json;`
  is **not** allowed, since it is a rename). Use the module path at the call site instead:
  `super::campaign_link::core_items_json(...)`. Changing only the path is allowed.
- **`String`-dispatched names.** About 50 `#[func]`s are called via `call("name")` from other files.
  Their names must not change; `api_surface.py` proves this.
- **`load_sector_world`** (fn 3633–3863) takes the registry out (`content_snapshot.take()`, ~3646)
  and restores it. Move it whole.
- **Do not** reorder methods within a file in ways that split a doc comment from its fn.
- The `ClassDB` method order will change; that is not ABI.

## Verification (RULES §6)

```bash
python3 tools/refactor/api_surface.py | diff /tmp/S05-surface-before.txt -       # empty (158 MindSimHost funcs, 3 signals)
python3 tools/refactor/move_check.py --base $(git merge-base main HEAD) \
   --old client/rust/mind-gdext/src/sim_host.rs --new client/rust/mind-gdext/src/sim_host
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p mind-gdext --all-targets -- -D warnings
run --cargo-test -p mind-gdext --lib sim_host
wc -l client/rust/mind-gdext/src/sim_host/*.rs
```

`move_check` will report the moved `use` block and the moved doc comment as structural (comments and
`use` lines are ignored). `only-in-old` must be 0.

Orchestrator post-merge: a headless boot of `game.tscn` with no `ERROR:`, plus `dialog_sweep_check`
and `ui_widgets_check`. A short MCP playtest (place a block, spawn a unit, save and load) is advised,
because `sim_host` is the hub of the client.
