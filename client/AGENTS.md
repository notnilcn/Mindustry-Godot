# AGENTS.md — client/ (Godot 4.7 project)

The Godot project for Mindustry-Godot. Gameplay behavior lives in Rust (`client/rust/`, exported as the `mind-gdext` GDExtension); this tree owns the `.tscn` scene graph, the GDScript UI layer, shaders, `project.godot`, third-party addons and the generated scenario mirror. Read the root [`AGENTS.md`](../AGENTS.md) first.

## Layout

| Path | Responsibility |
|---|---|
| `project.godot` | Project file: main scene `res://scenes/game.tscn`, `mobile` rendering method, Jolt 3D physics, the `McpRuntimeAutoload`/`MindAssets`/`MindMods`/`StdbConnector`/`MindUi`/`MindHud` autoloads, and the `open_godot_mcp` editor plugin. |
| `mind.gdextension` | Maps `gdext_rust_init` (minimum compatibility 4.7) to `res://bin/rust/{debug,release}/…mind_gdext…`; `tools/build.sh` produces that library. |
| `scenes/game.tscn` | Main scene rig: `SimHost`, `Input`, `MindAudio`, `World/{TileGrid,Camera2D,Renderer,Bloom,LowRes,MinimapProvider}`, the `MindRender`/`MindFx`/`MindCampaign`/`MindLogic`/`MindNet`/`MindEditor`/`MindPlatform` facades, `Planet`, and `Ui/{MenuBackground,UiRoot,EditorDialog,StateInspector}`. |
| `scenes/autoloads/` | One `.tscn` per autoload; each root node is the native `mind-gdext` class (`MindAssets`, `MindMods`, `StdbConnector`, `MindUi`, `MindHud`). |
| `scenes/editor/` | Map-editor shell and tools: `map_editor_dialog`, `map_view`, `wave_graph`, `map_objectives_canvas`, `wave_info_dialog`, `banned_content_dialog`, `sector_generate_dialog`, plus the map info/load/save/resize/locales/processors/generate/data dialogs. |
| `scenes/ui/dialogs/`, `scenes/ui/fragments/`, `scenes/ui/widgets/` | `.tscn` files mirroring `ui/`; `mind_dialog_base.tscn` is the shared dialog shell and `mind_minimap.tscn` hosts the minimap widget. |
| `scenes/ui/ui_root.tscn` | UI root: the five layer groups plus a static instance of every `ui/dialogs_manifest.json` dialog/fragment under its layer group, so the tree is navigable in-editor. |
| `scenes/ui/state_inspector.tscn` | Read-only spine overlay reading sim/net/audio/editor JSON. |
| `ui/` | GDScript UI layer: `ui_root.gd`, the dialog base, table/widget factories, theme/text/layout helpers, dialogs, fragments and the two manifests. |
| `ui/dialogs_manifest.json`, `ui/styles_manifest.json` | The UI ABI: dialog/fragment inventory and the themed style names. |
| `shaders/` | Ported Godot shaders (`default`, `water`, `fog`, `planet`, `shield`, `buildbeam`, …) plus `tools/shaders_check.gd`. |
| `addons/open_godot_mcp/` | Editor plugin and runtime autoload that back MCP inspection and in-engine playtesting. |
| `addons/blastbullets2d/`, `addons/phantom_camera/` | Vendored third-party addons (bullet engine; camera tweening), loaded as plugins. |
| `audio/bus_layout.json` | Declarative bus table (`Master`, `Music`, `Sound`, `UI`) matching the `MindAudio` buses. |
| `tools/shaders_check.gd` | `SceneTree` script that force-compiles every `res://shaders/*.gdshader`. |
| `tools/ui_widgets_check.gd` | `SceneTree` script that boots `MindAssets` headlessly and asserts the check widget's atlas regions and minimum row size (GDScript-only widget oracle). |
| `scenarios/` | Generated mirror of the repo-root `scenarios/`; synced by `tools/sync_scenarios.sh`, never hand-edited. |
| `bin/`, `.godot/` | Build output and editor import cache; gitignored. |
| `rust/` | Cargo workspace (`mind-core`, `mind-gdext`, `mind-headless`, `mind-stdb`, `mind-atlas`, `mind-derive`, `mind-macros`, `mind-tools`); build it with `tools/build.sh`, not from inside the editor. |

## Responsibilities

- `ui_root.gd` (`MindUiRoot`) owns the layer groups (`MenuGroup`, `HudGroup`, `DialogLayer`, `OverlayLayer`, `LoadingLayer`), binds every manifest dialog/fragment to the statically-instanced node of the same name under `scenes/ui/ui_root.tscn`, builds the theme via `MindThemeBuilder`, connects the `MindUi` prompt/toast signals, and runs the boot presentation (`set_menu_visible(true)`: show the menu, hide the HUD group, clear the loading overlay, fade the boot cover). `_enter_tree` applies the theme so it is in place before the static children `_ready`.
- `menu_fragment.gd` renders the standalone menu in `MenuGroup`: the `MINDISTRY` logo, the left sidebar button tree (icons are Mindustry icon-font glyphs via `MindAssets.icon_font`) with fade-in submenus, the Discord banner and quit. `menu_background.gd` under `Ui/MenuBackground` asks the Rust `MindRender.build_menu_texture` for the procedural `MenuRenderer` world baked to a texture and darkens it behind the menu.
- `mind_dialog.gd` (`MindDialog`) is the base for all dialogs; `MindUi` (Rust) owns visibility and the active-dialog stack, while the node renders and calls `show_dialog`/`hide_dialog`.
- Fragments bind to the read-only `MindHud` properties and signals or to `MindUi` JSON endpoints; they never touch sim state.
- Editor dialogs read and write `/root/Spine/MindEditor` (tool dispatch, rotation, undo/redo and palette order are resolved in Rust).
- The native nodes (`MindSimHost`, `MindWorldRenderer`, `MindFx`, `MindCampaign`, `MindLogic`, `MindNet`, `MindEditor`, `MindPlatform`, `MindAudio`, `MindAssets`, `MindMods`, `StdbConnector`) are the interface to the Rust behavior; GDScript only calls them.

## Key types

- Dialogs: `MindDialog` over the shell scene `scenes/ui/dialogs/mind_dialog_base.tscn`.
- Layout/widgets: `MindTable`, `MindCell`, `MindStack`, `MindScroll`, `MindWidgets`, `MindUiMargins`.
- Theme/text: `MindStyles`, `MindThemeBuilder`, `MindStyleLookup`, `MindIconEffect`, `MindLabel`, `MindRichLabel`.
- Interactive widgets: `MindMinimapWidget`, `MindBar`, `MindCollapser`, `MindCheck`, `MindTooltip`, `MindGridImage`, `MindBorderImage`, `ItemsDisplay`, `CoreItemsDisplay`.
- Layout algorithms: `MindTreeLayout`, `MindRowTreeLayout`, `MindBranchTreeLayout`, `MindRadialTreeLayout`.

## Invariants

- **`.tscn`-first.** Static UI, world scaffolding, autoloads and debug views live in scenes so the project is navigable in the editor. Rust owns behavior, not static scene construction.
- Any node built in code needs a `# code-instantiated: <specific reason>` comment at the site (`spawned at runtime`, `pooled/high-churn`, `count driven by server rows`).
- **GDScript is UI-only.** It reads sim JSON and signals (`get_state_json`, `state_changed`) and never mutates sim state.
- **Manifest ABI.** A new dialog or fragment must be added to three places: the static instance under its layer group in `scenes/ui/ui_root.tscn`, `ui/dialogs_manifest.json`, and the `EXPECTED_*_DIALOGS`/fragment lists in `mind-core::ui::manifest`. `MindUiRoot._bind_manifest`/`_verify_manifest` and `MindThemeBuilder.verify()` cross-check the manifests at boot.
- **Bundle keys.** User-visible strings go through `_t("@key")`; debug-only strings are the exception.
- `scenarios/` is an exact, generated mirror of the repo-root `scenarios/`; edit the canonical files and run `tools/sync_scenarios.sh`.
- Generated STDB bindings under `rust/mind-stdb/src/module_bindings/` are never hand-edited; regenerate with `server/build.sh` (drift gate `--check`).
- Source files are UTF-8/LF with `SPDX-License-Identifier: GPL-3.0-only` headers citing the ported Mindustry source.

## Conventions

- `MindWidgets` factories are the way dialogs build themed widgets; text/icon lookup routes through `MindAssets`.
- Style names in `styles_manifest.json` are the parity ABI; only presentation values in `MindStyles`/`MindThemeBuilder` are local.
- `MindTable`'s fluent `add(child).grow_x_axis().pad(4)` API mirrors `Table.add(...)`; call `row()` to start a new row.
- Addon trees under `addons/` are vendored; change them only when working on the addon itself.

## Verification

```bash
tools/build.sh                                                  # mind-gdext + mind-headless -> client/bin/rust, then sync scenarios
godot4 --path client                                            # open res://scenes/game.tscn
bash tools/godot.sh --headless --editor --quit --path client    # import/parse gate
godot4 --headless --path client --script res://tools/shaders_check.gd   # shader compile gate
godot4 --headless --path client --script res://tools/ui_widgets_check.gd  # widget region/size gate
tools/ci.sh                                                     # full local gate (Rust, goldens, mirror, Godot import, STDB)
tools/mcp-smoke.sh                                              # in-engine spine smoke (needs the editor running)
```

In-engine checks run through the `open_godot_mcp` addon; the repo skill [`.opencode/skills/playtest/SKILL.md`](../.opencode/skills/playtest/SKILL.md) holds the launch flow, node map and pid-stamped recipes.
