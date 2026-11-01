# GDScript → Rust UI port status

This document tracks the native-Rust replacement of every non-addon `.gd` file
under `client/`. It is the continuation map: the API contract is in
[`PORTING.md`](PORTING.md), the scene rewiring tool is
[`tools/rewire_native_ui.py`](../../../../../tools/rewire_native_ui.py) and the
cutover is [`tools/finalize_native_ui.py`](../../../../../tools/finalize_native_ui.py).

## Snapshot (current)

- **Every porting unit exists in Rust.** All 59 manifest dialog controllers, all
  14 fragments, all 19 widgets, `MindUiRoot`, `MindPlanetView`, `MindLandingCore`,
  the four editor views and the three headless oracles are native; ~73.7k lines
  of UI Rust.
- `cargo check`, `cargo clippy --all-targets -- -D warnings` and
  `cargo fmt --check` are **green**; `bash tools/build.sh` is green.
- The headless editor import gate, the three native oracles, a `scenes/game.tscn`
  boot and the native probe all pass (see **Verification**).
- All **122** non-addon `.gd` files are still present and the running project
  still uses them (the native classes are registered as `Rust*`). **The cutover
  (`tools/finalize_native_ui.py`) has not been run** — by design, until the
  project owner takes the cutover decision. `rewire_native_ui.py --check`
  reports 90 scenes still holding non-addon script `ext_resource`s; finalize
  rewires and deletes them in one step.

## What the cutover still needs (and only that)

1. `python3 tools/finalize_native_ui.py --apply` — removes the `rename = Rust*`
   attributes, rewires the 90 scenes, verifies no script reference remains and
   deletes the 122 `.gd` files. Nothing else is pending: no controller, fragment,
   widget, root node, editor view or oracle is missing.
2. Rebuild + rerun the gates below (the class names change from `Rust*` to the
   bare names, so the first run after `--apply` is the real proof).
3. Optional follow-ups owned by the cutover commit: `mind-core::ui::manifest`
   scene checks (they only read manifests, no `.gd` coupling),
   `mind-core/src/ui/builder/style_lookup.rs` (a dev token scan over `.gd` text,
   harmless when the files are gone) and `client/AGENTS.md`'s UI-layer section
   (it still describes the GDScript layer — correct until the cutover lands).

## Done

Foundation (`client/rust/mind-gdext/src/ui/`):

- `kit/` — `MindTable`/`MindStack`/`Cell`, `CellData`/`Align`,
  `MindRichLabel`/`MindLabel`/`MindIconEffect`, `MindScroll` (wheel easing),
  `styles`, `theme` (+`verify`), `icons`, `layout`, `margins`, and
  `widgets/` (all 19 interactive widgets plus `factory`, the `MindWidgets`
  surface).
- `dialog.rs` — `MindDialog` base (title/underline/full-dialog/button row,
  show/hide fade, context, campaign helpers, per-dialog signal union,
  `DialogController` trait).
- `dialogs/` — **59/59 manifest controllers**: `simple` (about, credits,
  discord, full_text), `misc` (list_select, bans, admins, traces,
  campaign_complete), `sector` (sector_select, research_root_select, palette),
  `batch_a` (settings, language, controls, effects, database, content),
  `batch_b` (paused, restart, join), `batch_c` (planet, onplanet),
  `batch_campaign2` (research, schematics, loadout, launch_loadout,
  sector_stats, custom/map_play + the shared `MapListDialog` base),
  `batch_d` (widget_gallery, editor_import/export, file_chooser, icon_select,
  editor_menu, editor_map), `batch_e` (map_info, map_resize,
  map_generate, wave_info, map_objectives, map_locales), `batch_f`
  (map_processors, map_load, sector_generate, banned_content, map_assets,
  schematic_info), `batch_g` (picker, mods, mod_browser), `batch_h` (host, load,
  save, campaign_rules), `batch_i` (custom_rules, editor_maps, canvas_edit),
  `batch_j` (logic, the full mlog editor + `MindLogicOverlay`).
- `fragments/` — all 14 (performance, fade_in, loading, menu, plan_config, hud,
  minimap, placement, block_config, block_inventory, hints, chat, console,
  player_list).
- Roots and views: `root.rs` (`MindUiRoot`), `planet.rs` (`MindPlanetView`),
  `landing.rs` (`MindLandingCore`), `state_inspector.rs`,
  `menu_background.rs`, `menu_flyers.rs`, and `editor_views.rs`
  (`MindMapEditorDialog`, `MindMapView`, `MindWaveGraph` on a `PanelContainer`
  base, `MindMapObjectivesCanvas`).
- `checks.rs` — the `client/tools/*.gd` oracles as native scene roots:
  `MindShadersCheck`, `MindUiLayoutCheck`, `MindUiWidgetsCheck`, run from
  `client/scenes/tools/*.tscn` (wired into `tools/ci.sh`).

## Verification (this run)

```bash
cargo check     --manifest-path client/rust/Cargo.toml -p mind-gdext --target-dir client/bin/rust --all-targets   # green
cargo clippy    --manifest-path client/rust/Cargo.toml -p mind-gdext --target-dir client/bin/rust --all-targets -- -D warnings   # green
cargo fmt       --manifest-path client/rust/Cargo.toml --all -- --check    # green
bash tools/build.sh                                                        # green
bash tools/godot.sh --headless --editor --quit --path client               # exit 0, no script/parse errors, no panics
bash tools/godot.sh --headless --path client res://scenes/tools/shaders_check.tscn     # total=30 failed=0
bash tools/godot.sh --headless --path client res://scenes/tools/ui_layout_check.tscn   # failed=0 (33 assertions)
bash tools/godot.sh --headless --path client res://scenes/tools/ui_widgets_check.tscn  # failed=0 (17 assertions)
bash tools/godot.sh --headless --path client --quit-after 300              # boots all facades, exit 0
bash tools/godot.sh --headless --path client res://scenes/ui/native_probe.tscn --quit-after 30  # `[ui] MindDialog ready`
```

Last unit, found and fixed in this run:

- **`save` had no controller.** `controller_for("save")` returned `None` (the old
  `batch_d` list above named a `save` entry that did not exist), so the save
  dialog would have opened as an empty shell after the cutover. It is now
  `LoadDialogController::save()` in `batch_h`: one controller with a `LoadKind`
  mode that mirrors `SaveDialog extends LoadDialog` — `@save.new` replaces
  `@save.import` (prompt `@save.newslot` → `UiRoot.load_and("@saving")` →
  `MindCampaign.save_slot`, dialog stays open), and a card body asks
  `@save.overwrite` before emitting `save_requested(slot)` and closing for the
  `save` registry entry.
- **Controller dialogs panicked in `ready` (shell bug).** The first in-engine
  instantiation of a controller-driven dialog crashed in
  `MindDialog::on_buttons_child_entered`: `controller.ready` →
  `add_close_button` → `add_child` fired `child_entered_tree` synchronously while
  `ready` held the mutable borrow, so the `#[func]` receiver re-entered
  `bind_mut` (gdext `already bound` panic). The button-row watcher is now a
  non-binding `Callable` that defers `sync_buttons_row`, which binds each
  button's `visibility_changed` hook itself. Every controller dialog was
  affected; it stayed latent because the pre-cutover scenes still run the
  GDScript dialogs and the editor gate instantiates `MindDialog` without a
  controller.

A temporary native probe (the `save` and `load` scenes swapped to
`RustMindDialog` shells at their manifest paths, both restored afterwards)
passed 21 in-engine assertions: save title/`@save.new`/card listing, card click
→ `@save.overwrite` with no write before the answer, confirm →
`save_requested("probe")`, `@save.new` prompt → `save_requested("probe2")` with
the dialog staying open, prompt cancel, and the load view's `@save.import`
button + `slot_selected(name)` + close.

Fixes made while integrating the last batches:

- `init` must not touch the base object (**confirmed**): `MindResearchView`,
  `MindResearchItemsDisplay` and `MindSchematicImage` set base properties in
  `init` and panicked with `downcast from Control to RustMind… failed` — the
  editor instantiation gate caught it. All three now configure the base in
  `ready()`.
- `block_config` (`fragments/block_config.rs`) and `map_objectives`
  (`dialogs/batch_e.rs`) called a non-existent `picker.show_picker` /
  `picker.set_color`; both now open `picker` through `MindUi.open_dialog` with
  the documented context (`{"color": "#rrggbbaa", "alpha": …}`) and consume
  `color_selected`.
- The `batch_b` duplicate `HostDialog`/`LoadDialog` tail (superseded by
  `batch_h`) was removed; remaining ported-but-unreached helpers carry
  `#[allow(dead_code)]` with a reason (the same convention `batch_c.rs` already
  used).
- `batch_b`'s `sort_by` → `sort_by_key`, `planet.rs`'s repeated `push`, and the
  test-target lints (`unused` imports, struct-literal initialisation, `expect`
  in the styles test) were cleaned so `clippy -D warnings` covers `--all-targets`.

Not part of the gate: `cargo test -p mind-gdext` runs 193 unit tests; 17 of them
construct Godot builtins (`VarDictionary`, `GString`, …) outside a Godot host and
panic in `godot-ffi` — a pre-existing pattern in this crate, which is why
`tools/ci.sh` gates on `cargo test -p mind-core` only. Pure-data tests pass.

## Temporary registration names (`Rust*`)

Godot refuses to run while a native class and a GDScript `class_name` share a
name ("hides a native class"). During the migration the 29 native classes that
collide keep their Rust type names but register under a `Rust` prefix:

```rust
#[class(rename = RustMindDialog, base=Control)]
pub struct MindDialog { ... }
```

The running project therefore still uses the GDScript UI; the native classes
are verified by `scenes/ui/native_probe.tscn` (`RustMindDialog`,
`RustMindScroll`, `RustMindRichLabel`) and by the editor instantiation gate,
which instantiates every registered extension class. At cutover:

1. delete the remaining `.gd` files,
2. remove every `rename = Rust*` attribute (the Rust type name becomes the Godot
   class name again),
3. `python3 tools/rewire_native_ui.py --apply`, rebuild and playtest.

Do not add `rename` to a new class unless its name collides with a live
GDScript `class_name`.

## Porting deviations worth knowing

Behaviour is 1:1 with the `.gd` except where the native object model has no
equivalent; each site carries an in-source comment:

- **Prompt/consumer callables** (`ColorPicker.show(color, alpha, consumer)`,
  `showTextInput`-style prompts): the native seam is the `MindUi` prompt signal
  plus dialog context, answered asynchronously through `call_deferred`
  (`PAR-0066` re-entrancy).
- **`DialogController` has no per-frame hook**: dialogs that polled in
  `_process` (canvas auto-save, mod download progress, logic vars refresh) use a
  `SceneTree.process_frame` tick or a `Timer` instead.
- **`@tool`-style draw/input on controllers**: the logic editor, canvas editor
  and wave graph need a `Node` for `_draw`/`_gui_input`, so they register small
  helper classes (`MindLogicOverlay`, `MindCanvasSurface`) or keep the draw in
  the scene-root class (`MindWaveGraph`).
- **Cross-dialog node calls** (`mods.set_updates`, `mod_browser.install_repos`)
  became node metadata handoffs + a deferred `refresh_controller`, since one
  shared `MindDialog` class cannot grow per-dialog methods without widening the
  ABI for every dialog.

## gdext pitfalls found

- A custom `init` must NOT touch the base object (`self.base_mut()`); the
  extension class is not bound yet and gdext panics on the downcast. Configure
  such classes in `ready()`.
- `Gd<UserClass>` has no `Deref`: use `.bind()`/`.bind_mut()`; engine methods
  need an upcast.
- Typed signals (`node.signals().pressed().connect(...)`) take closures; to
  connect a `Callable`, use `node.connect("pressed", &callable)`.
- Optional object arguments take `&Gd<T>` (not `Some(gd)`); optional scalars
  take `Some(x)`.
- `#[func(default = ...)]` parameters use `#[opt(default = EXPR)]`.
- Enum constants live in class sidecar modules (`godot::classes::control::SizeFlags`,
  `godot::classes::texture_rect::StretchMode`, …).
- `StyleBoxFlat`/`StyleBoxEmpty` are RefCounted (`new_gd()`); per-side setters
  are `set_border_width(Side, i32)` and `set_corner_radius(Corner, i32)`.
- `SceneTree::quit()` is generated without parameters; pass the exit code
  through `tree.call("quit", &[code.to_variant()])` (the oracles rely on it).
- A signal that fires **synchronously inside `ready`** must not reach a
  `#[func]` on the same node: `child_entered_tree` (emitted by `add_child` from
  a controller's `ready`) re-enters `bind_mut` and gdext panics with
  `already bound`. Connect a non-binding `Callable::from_fn` that
  `call_deferred`s the binding work instead (`MindDialog::watch_buttons_row`).
