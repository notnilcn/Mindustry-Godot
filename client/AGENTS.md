# AGENTS.md — client/ (Godot 4.7 project)

The Godot project for Mindustry-Godot. Gameplay behavior and the UI layer live in Rust (`client/rust/`, exported as the `mind-gdext` GDExtension); this tree owns the `.tscn` scene graph (whose nodes name the native UI classes), shaders, `project.godot`, vendored addons and the generated scenario mirror. Read the root [`AGENTS.md`](../AGENTS.md) first for global invariants and generated-file rules; the Rust workspace is [`rust/AGENTS.md`](rust/AGENTS.md) and script/gate semantics are [`tools/AGENTS.md`](../tools/AGENTS.md).

## Layout

| Path | Responsibility |
|---|---|
| `project.godot` | Project file: main scene `res://scenes/game.tscn`, `mobile` rendering method, Jolt 3D physics, the `McpRuntimeAutoload`/`MindAssets`/`MindMods`/`StdbConnector`/`MindUi`/`MindHud` autoloads, and the sole enabled editor plugin `open_godot_mcp`. |
| `mind.gdextension` | Maps `gdext_rust_init` (minimum compatibility 4.7) to `res://bin/rust/{debug,release}/…mind_gdext…`; `tools/build.sh` produces that library. |
| `shaders/` | Ported Godot shaders (`default`, `water`, `fog`, `planet`, `shield`, `buildbeam`, …), one `.gdshader` per effect. |
| `audio/bus_layout.json` | Declarative bus table (`Master`, `Music`, `Sound`, `UI`) matching the `MindAudio` buses. |
| `scenes/tools/` | Native headless UI oracles: `{shaders,ui_widgets,ui_layout,dialog_sweep}_check.tscn` root on `MindShadersCheck`/`MindUiWidgetsCheck`/`MindUiLayoutCheck`/`MindDialogSweepCheck` (`rust/mind-gdext/src/ui/checks.rs`); repo build/gate scripts live in the root `tools/` and are documented in [`tools/AGENTS.md`](../tools/AGENTS.md). `client/tools/` is gone with the GDScript port. |
| `addons/open_godot_mcp/` | Editor plugin and runtime autoload that back MCP inspection and in-engine playtesting; see its [`AGENTS.md`](addons/open_godot_mcp/AGENTS.md). |
| `addons/phantom_camera_rust/` | Phantom Camera 0.11.0.3 ported to Rust (crate `rust/phantom-camera-rust/`): the second GDExtension library, `[autoload] PhantomCameraManager`, and the ported 2D/3D demo scenes used as the parity benchmark. Guide: [`AGENTS.md`](addons/phantom_camera_rust/AGENTS.md). The original GDScript addon moved out of the project to `../parity/pcam_oracle/phantom_camera/` (it is the read-only parity oracle, and its `class_name`s would collide with the native classes). |
| `scenarios/` | Generated mirror of the repo-root `scenarios/` (see **Invariants**); gitignored. |
| `bin/`, `.godot/` | Build output and editor import cache; gitignored. |
| `rust/` | Cargo workspace behind the GDExtension; crate map, lints and Rust gates live in [`rust/AGENTS.md`](rust/AGENTS.md). Build it with `tools/build.sh`, never from inside the editor. |

## UI & scene subtree

Navigation map only; behavior and conventions are in **Responsibilities** and **Key types** below.

| Path | What lives there |
|---|---|
| `scenes/game.tscn` | Main scene (`Spine`): `SimHost`, `Input`, `MindAudio`, `World/*` render nodes, the `Mind*` Rust facades, `Planet/PlanetView`, and `Ui/{MenuBackground,EditorDialog,UiRoot,StateInspector}`. Its spine nodes carry the `node_paths`-wired `#[export]` deps (see **Invariants**). |
| `scenes/autoloads/` | One `.tscn` per scene autoload (`mind_assets`, `mind_mods`, `stdb_connector`, `mind_ui`, `mind_hud`); each root node is the matching native `Mind*` class. `McpRuntimeAutoload` is a script autoload from the addon (also injected at editor startup). |
| `scenes/editor/` | Map-editor shell and tool scenes rooted on native classes: `map_editor_dialog`, `map_view`, `wave_graph`, `map_objectives_canvas`, `wave_info_dialog`, `banned_content_dialog`, `sector_generate_dialog`, plus the map info/load/resize/locales/objectives/processors/generate dialogs and `data/map_assets_dialog`. |
| `scenes/ui/*.tscn` | `ui_root.tscn` (five layer groups plus a static instance of every manifest dialog/fragment), `state_inspector.tscn` (read-only spine overlay), `planet_view.tscn`. |
| `scenes/ui/dialogs/` | 48 manifest dialog `.tscn` files plus `mind_dialog_base.tscn`, the shared shell every dialog inherits. |
| `scenes/ui/fragments/` | 14 fragment `.tscn` files rooted on the `Mind*Fragment` classes (`rust/mind-gdext/src/ui/fragments/`). |
| `scenes/ui/widgets/` | Only `mind_minimap.tscn`; every other widget is code-built by its native class (`rust/mind-gdext/src/ui/kit/widgets/`). |
| `ui/` | The two manifests below; the former GDScript tree was deleted at the cutover. UI code lives in `rust/mind-gdext/src/ui/` (contract: its `PORTING.md`, inventory: `PORT-STATUS.md`). |
| `ui/dialogs_manifest.json` | Dialog/fragment inventory: dialogs carry `name`, `scene`, `pause`, `menu_openable`, `args`; fragments `name`, `scene`, `group`; plus top-level `prompts`. |
| `ui/styles_manifest.json` | Themed style/drawable names consumed by the theme builder. |

## Responsibilities

- `MindUiRoot` (`rust/mind-gdext/src/ui/root.rs`, scene `scenes/ui/ui_root.tscn`) owns the layer groups (`MenuGroup`, `HudGroup`, `DialogLayer`, `OverlayLayer`, `LoadingLayer`), binds every manifest dialog/fragment to the statically-instanced node of the same name, builds the theme via `MindThemeBuilder`, connects the `MindUi` prompt/toast signals, and runs the boot presentation (`set_menu_visible(true)`: show the menu, hide the HUD group, clear the loading overlay, fade the boot cover). The theme is applied on `enter_tree` so it is in place before the static children `ready`.
- `MindMenuFragment` renders the standalone menu in `MenuGroup`: the `MINDISTRY` logo, the left sidebar button tree (icons are Mindustry icon-font glyphs via `MindAssets`) with fade-in submenus, the Discord banner and quit. `MindMenuBackground` under `Ui/MenuBackground` asks `MindRender.build_menu_texture` for the procedural `MenuRenderer` world baked to a texture and darkens it behind the menu.
- `MindDialog` (`rust/mind-gdext/src/ui/dialog.rs`) is the base for all dialogs: one native root class per manifest dialog, dispatched to a `DialogController` by registry key. `MindUi` owns visibility and the active-dialog stack; the node renders and calls `show_dialog`/`hide_dialog`.
- Fragments bind to the read-only `MindHud` properties and signals or to `MindUi` JSON endpoints; they never touch sim state.
- Editor dialogs read and write `/root/Spine/MindEditor` (tool dispatch, rotation, undo/redo and palette order are resolved in Rust). They are one of the remaining path-based controller groups — deferred, unchanged by the scene-spine refactor; see [`docs/GODOT-SCENES-REFACTOR.md`](../docs/GODOT-SCENES-REFACTOR.md).
- The native nodes (`MindSimHost`, `MindWorldRenderer`, `MindFx`, `MindCampaign`, `MindLogic`, `MindNet`, `MindEditor`, `MindPlatform`, `MindAudio`, `MindAssets`, `MindMods`, `StdbConnector`) are the interface to the Rust behavior; scenes only name them.

## Key types

- Dialogs: `MindDialog` over the shell scene `scenes/ui/dialogs/mind_dialog_base.tscn`.
- Layout/widgets: `MindTable` (and its `Cell` guard), `MindStack`, `MindScroll`, `kit::widgets::factory` (the former `MindWidgets` surface), `MindUiMargins`.
- Theme/text: `MindStyles`, `MindThemeBuilder`, `MindStyleLookup`, `MindIconEffect`, `MindLabel`, `MindRichLabel`.
- Interactive widgets: `MindMinimapWidget`, `MindBar`, `MindCollapser`, `MindCheck`, `MindTooltip`, `MindGridImage`, `MindBorderImage`, `ItemsDisplay`, `CoreItemsDisplay`.
- Layout algorithms: `MindTreeLayout`, `MindRowTreeLayout`.

## Invariants

- **`.tscn`-first.** Static UI, world scaffolding, autoloads and debug views live in scenes so the project is navigable in the editor. Rust owns behavior, not static scene construction.
- **Scene-spine deps are injected.** The spine classes take collaborators as `#[export] Option<Gd<T>>` fields wired from `scenes/game.tscn` (`node_paths` + `NodePath`), adopted in `ready`/`bootstrap` through `wiring::{required, optional}`; a missing required dep warns once and degrades. Migrated: `MindTileGrid`, `MindCamera2D`, `MindFx`, `MindWorldRenderer`, `MindRender`, `MindMinimap`, `MindSimHost`, `MindNet`, `MindInput` (+ `DesktopBridge`), `MindUiRoot`, `MindAudio`, `MindCampaign`, `MindStateInspector`, `MindMenuBackground`, `MindMenuFlyers`, `MindLandingCore`. UI dialog/fragment controllers and the code-built render-pass children still use the `kit::*`/path helpers — documented follow-up in [`docs/GODOT-SCENES-REFACTOR.md`](../docs/GODOT-SCENES-REFACTOR.md).
- Any node built in code needs a `# code-instantiated: <specific reason>` comment at the site (`spawned at runtime`, `pooled/high-churn`, `count driven by server rows`).
- **The UI is native Rust.** All non-addon code is `.rs` or `.tscn`. There is no GDScript in `client/` (outside vendored addons), no `.gd`/`.gd.uid` file, and no `script = ExtResource(...)` on a non-addon scene: UI nodes are `Mind*` classes from `mind-gdext/src/ui/`, which read sim JSON/signals (`get_state_json`, `state_changed`) and never mutate sim state. Extend the native class or the scene instead.
- **Manifest ABI.** A new dialog or fragment must be added to three places: the static instance under its layer group in `scenes/ui/ui_root.tscn`, `ui/dialogs_manifest.json`, and the `EXPECTED_*_DIALOGS`/fragment lists in `mind-core::ui::manifest`. `MindUiRoot::bind_manifest`/`verify_manifest` and `MindThemeBuilder.verify()` cross-check the manifests at boot.
- **Bundle keys.** User-visible strings go through `kit::t("@key")` (`kit::tm` for markup); debug-only strings are the exception.
- `scenarios/` is an exact, generated mirror of the repo-root `scenarios/`; edit the canonical files and run `bash tools/sync_scenarios.sh`.
- File headers (UTF-8/LF, `SPDX-License-Identifier: GPL-3.0-only`, upstream citation) follow the root [`AGENTS.md`](../AGENTS.md).

## Conventions

- `kit::widgets::factory` is the way dialogs build themed widgets; text/icon lookup routes through `MindAssets`.
- Style names in `styles_manifest.json` are the parity ABI; only presentation values in `MindStyles`/`MindThemeBuilder` are local.
- `MindTable`'s fluent `add(child).grow_x_axis().pad(4)` API mirrors `Table.add(...)`; call `row()` to start a new row.
- Addon trees under `addons/` are vendored; change them only when working on the addon itself.

## Verification

Run from the repo root.

```bash
bash tools/godot.sh --headless --editor --quit --path client                # import/parse gate
godot4 --headless --path client res://scenes/tools/shaders_check.tscn     # shader compile gate
godot4 --headless --path client res://scenes/tools/ui_widgets_check.tscn  # widget/theme/atlas gate
godot4 --headless --path client res://scenes/tools/ui_layout_check.tscn   # MindTable min-size propagation gate
godot4 --headless --path client res://scenes/tools/dialog_sweep_check.tscn # every manifest dialog builds/shows/hides
```

`tools/ci.sh` runs the full local gate (composition in [`tools/AGENTS.md`](../tools/AGENTS.md)); in-engine checks run through the `open_godot_mcp` addon — load the `playtest` skill for the launch flow, node map and pid-stamped recipes.
