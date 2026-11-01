# GDScript → Rust UI port status

This document tracks the native-Rust replacement of every non-addon `.gd` file
under `client/`. It is the continuation map: the API contract is in
[`PORTING.md`](PORTING.md), the scene rewiring tool is
[`tools/rewire_native_ui.py`](../../../../../tools/rewire_native_ui.py) and the
cutover is [`tools/finalize_native_ui.py`](../../../../../tools/finalize_native_ui.py).

## Cutover landed (branch `native-ui-cutover`)

- **The GDScript UI is gone.** `finalize_native_ui.py --apply` stripped the 29
  `rename = Rust*` attributes, rewired 83 scenes (they now name the bare native
  classes), verified zero non-addon script references and deleted all 122 `.gd`
  files plus their 122 `.gd.uid` sidecars. `rewire_native_ui.py --check` now
  reports 0 scenes / 0 references; `client/tools/` no longer exists.
- Every porting unit is native: 59/59 manifest dialog controllers, 14 fragments,
  19 widgets, `MindUiRoot`, `MindPlanetView`, `MindLandingCore`, the four editor
  views and four headless oracles (~73.7k lines of UI Rust before the cutover
  fixes).
- Scenario files, the manifests and the Rust class names were already the ABI;
  only the temporary `Rust*` registration names changed.
- **Hot reloading the GDExtension requires an editor restart while an editor
  session is open.** gdext re-creates Rust instances on reload without re-running
  `ready`, and the native UI tree has no reload bootstrap; the old per-dialog
  notification handler was removed because it re-entered `bind_mut` on every
  show/hide (see below).

### Bugs found and fixed during the cutover

1. **`finalize_native_ui.py` missed a trailing `rename`.** `planet.rs` declared
   `#[class(base=Control, rename = RustPlanetView)]` (rename last, no trailing
   comma) so the regex left `MindPlanetView` unregistered while its scene already
   used the bare name (`Cannot get class 'MindPlanetView'`). The regex now handles
   both positions; the attribute was removed.
2. **`#[func(rename = "lit")]` registers the quotes.** gdext stringifies the
   attribute token stream, so a string literal becomes `""name""`; 25 methods
   (the placement/minimap/block-config callbacks and `StdbBinder`'s
   `connect`/`disconnect`) were unreachable and every `callable(...)` connect
   failed. All uses are now identifier form (`rename = _select`).
3. **`MindTable::sort_now` was not a `#[func]`.** `batch_b`'s
   `call_deferred("sort_now")` failed with `Method not found`; the method is now
   exported.
4. **`MindDialog::on_notification` panicked on every visibility change.** Hiding
   or showing a dialog sets its own `visible`, which delivers
   `NOTIFICATION_VISIBILITY_CHANGED` (and propagates it to buttons) synchronously
   from inside the bound `ready`/`show_dialog`/`hide_dialog`; the notification
   virtual then re-entered `bind_mut` (gdext `already bound` panic). The handler
   is gone; controllers are static per session, which the cutover accepts.
5. **The button-row hooks must not bind the dialog from signals.** Both
   `child_entered_tree` (a controller adding buttons from `ready`) and each
   button's `visibility_changed` (propagated by the dialog's own show/hide) now
   route through a non-binding `Callable` that defers `sync_buttons_row`; the
   visibility hook is stored on the node and coalesces to one pending sync.
6. **Controller frame ticks outlived their dialog.** `batch_c`/`batch_campaign2`
   drain their deferred queues from a `SceneTree.process_frame` connection that
   kept running after an editor dialog was freed, panicking on the stored `Gd`
   (`access to instance ... after it has been freed`). The tick now checks
   `is_instance_valid` before touching the node and disconnects itself.

### Verification (cutover run)

```bash
cargo clippy -p mind-gdext --all-targets -- -D warnings   # green
cargo fmt --all -- --check                               # green
bash tools/build.sh                                      # green
bash tools/godot.sh --headless --editor --quit --path client                # exit 0, no errors
bash tools/godot.sh --headless --path client res://scenes/tools/shaders_check.tscn      # total=30 failed=0
bash tools/godot.sh --headless --path client res://scenes/tools/ui_layout_check.tscn    # failed=0
bash tools/godot.sh --headless --path client res://scenes/tools/ui_widgets_check.tscn   # failed=0
bash tools/godot.sh --headless --path client res://scenes/tools/dialog_sweep_check.tscn # failed=0 (236 checks)
bash tools/godot.sh --headless --path client --quit-after 300              # exit 0, no panics
bash tools/godot.sh --headless --path client res://scenes/ui/native_probe.tscn --quit-after 30
```

`dialog_sweep_check` (`MindDialogSweepCheck` in `checks.rs`) is new: it
instantiates every manifest dialog through its controller and asserts the button
row plus show/hide/re-show/free for all 59 (236 assertions); it is wired into
`tools/ci.sh`. A temporary GDScript version of the same sweep found bugs 4–6
before being replaced by the native oracle.

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

## Verification (pre-cutover run, `Rust*` registration)

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

## Temporary registration names (`Rust*`, removed)

Godot refuses to run while a native class and a GDScript `class_name` share a
name ("hides a native class"). During the migration the 29 colliding native
classes kept their Rust type names but registered under a `Rust` prefix
(`#[class(rename = RustMindDialog, base=Control)]`). The cutover stripped every
such attribute and deleted the GDScript classes, so the Rust type name is the
Godot class name again and the scenes name the bare classes. Do not add `rename`
to a class unless it collides with a live GDScript `class_name`.

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
- **No `on_notification` on a class that changes its own `visible` (or any
  self-notifying property).** The notification is delivered synchronously from
  `set_visible`, and gdext's notification virtual binds mutably, so it re-enters
  `bind_mut` from `ready`/`show_dialog`/`hide_dialog` (`already bound` panic).
  `MindDialog` therefore has no notification handler; the native UI tree does
  not rebuild on an extension hot reload (restart the editor instead).
- **`#[func(rename = ...)]` takes an identifier, not a string literal.** gdext
  stringifies the token stream, so `rename = "_select"` exposes the method as
  `"_select"` (quotes included) and every `callable("_select")` is invalid.
  Same shape for `#[var(rename = ...)]` and `#[class(rename = ...)]`.
- **A signal connection outlives a freed node.** Controller frame ticks
  (`SceneTree.process_frame`) and `Callable::from_fn` closures must check
  `Gd::is_instance_valid()` *before* cloning/calling into the captured node, and
  should drop the connection when it is gone; `Gd::clone()` itself panics on a
  freed instance.
- **`Gd<T>::free()` in oracles**: free dialogs after the last frame that can
  touch them, or drive the whole sweep from one `_process` state machine (the
  `MindDialogSweepCheck` pattern).
