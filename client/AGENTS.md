# AGENTS.md — client/ (Godot 4.7 project)

The Godot project for Mindustry-Godot. Gameplay behavior lives in Rust (`client/rust/`, exported as the `mind-gdext` GDExtension); this tree owns the `.tscn` scene graph, the GDScript UI layer, shaders, `project.godot`, vendored addons and the generated scenario mirror. Read the root [`AGENTS.md`](../AGENTS.md) first for global invariants and generated-file rules; the Rust workspace is [`rust/AGENTS.md`](rust/AGENTS.md) and script/gate semantics are [`tools/AGENTS.md`](../tools/AGENTS.md).

## Layout

| Path | Responsibility |
|---|---|
| `project.godot` | Project file: main scene `res://scenes/game.tscn`, `mobile` rendering method, Jolt 3D physics, the `McpRuntimeAutoload`/`MindAssets`/`MindMods`/`StdbConnector`/`MindUi`/`MindHud` autoloads, and the sole enabled editor plugin `open_godot_mcp`. |
| `mind.gdextension` | Maps `gdext_rust_init` (minimum compatibility 4.7) to `res://bin/rust/{debug,release}/…mind_gdext…`; `tools/build.sh` produces that library. |
| `shaders/` | Ported Godot shaders (`default`, `water`, `fog`, `planet`, `shield`, `buildbeam`, …), one `.gdshader` per effect. |
| `audio/bus_layout.json` | Declarative bus table (`Master`, `Music`, `Sound`, `UI`) matching the `MindAudio` buses. |
| `tools/` | Godot-side `SceneTree` check scripts (`shaders_check.gd`, `ui_widgets_check.gd`, `ui_layout_check.gd`); repo build/gate scripts live in the root `tools/` and are documented in [`tools/AGENTS.md`](../tools/AGENTS.md). |
| `addons/open_godot_mcp/` | Editor plugin and runtime autoload that back MCP inspection and in-engine playtesting; see its [`AGENTS.md`](addons/open_godot_mcp/AGENTS.md). |
| `addons/blastbullets2d/`, `addons/phantom_camera/` | Vendored third-party addons (bullet engine; camera tweening). They are present in the tree but **not** listed in `project.godot` `[editor_plugins]`, so they do not load. |
| `scenarios/` | Generated mirror of the repo-root `scenarios/` (see **Invariants**); gitignored. |
| `bin/`, `.godot/` | Build output and editor import cache; gitignored. |
| `rust/` | Cargo workspace behind the GDExtension; crate map, lints and Rust gates live in [`rust/AGENTS.md`](rust/AGENTS.md). Build it with `tools/build.sh`, never from inside the editor. |

## UI & scene subtree

Navigation map only; behavior and conventions are in **Responsibilities** and **Key types** below.

| Path | What lives there |
|---|---|
| `scenes/game.tscn` | Main scene (`Spine`): `SimHost`, `Input`, `MindAudio`, `World/*` render nodes, the `Mind*` Rust facades, `Planet/PlanetView`, and `Ui/{MenuBackground,EditorDialog,UiRoot,StateInspector}`. |
| `scenes/autoloads/` | One `.tscn` per scene autoload (`mind_assets`, `mind_mods`, `stdb_connector`, `mind_ui`, `mind_hud`); each root node is the matching native `Mind*` class. `McpRuntimeAutoload` is a script autoload from the addon (also injected at editor startup). |
| `scenes/editor/` | Map-editor shell and tools as scene+script pairs: `map_editor_dialog`, `map_view`, `wave_graph`, `map_objectives_canvas`, `wave_info_dialog`, `banned_content_dialog`, `sector_generate_dialog`, plus the map info/load/resize/locales/objectives/processors/generate dialogs and `data/map_assets_dialog`. |
| `scenes/ui/*.tscn` | `ui_root.tscn` (five layer groups plus a static instance of every manifest dialog/fragment), `state_inspector.tscn` (read-only spine overlay), `planet_view.tscn`. |
| `scenes/ui/dialogs/` | 48 manifest dialog `.tscn` files plus `mind_dialog_base.tscn`, the shared shell every dialog inherits. |
| `scenes/ui/fragments/` | 14 fragment `.tscn` files mirroring `ui/fragments/`. |
| `scenes/ui/widgets/` | Only `mind_minimap.tscn`; every other widget is code-built by its `ui/widgets/*.gd` script. |
| `ui/*.gd` | Root modules: `ui_root.gd`, `mind_dialog.gd`, `mind_table/cell/stack/scroll`, `mind_widgets.gd`, `margins.gd`, menu background/flyers, `planet_view.gd`, `state_inspector.gd`. |
| `ui/dialogs/` | One script per dialog scene (48) plus the `map_list_dialog.gd` helper; scene owns fixed structure, code owns data-driven rows. |
| `ui/fragments/` | 14 controllers bound to `MindHud`/`MindUi` read models (menu, HUD, chat, minimap, placement, …). |
| `ui/widgets/` | One script per interactive widget (see **Key types**). |
| `ui/layout/` | `MindTreeLayout` (`tree_layout.gd`) and `MindRowTreeLayout` (`row_tree_layout.gd`). |
| `ui/theme/` | `MindStyles`, `MindThemeBuilder`, `MindStyleLookup`, `MindIconEffect`, `mind_icons.gd` (`MindIcons`). |
| `ui/text/` | `MindLabel` and `MindRichLabel`, the wrapping/localized label nodes. |
| `ui/dialogs_manifest.json` | Dialog/fragment inventory: dialogs carry `name`, `scene`, `pause`, `menu_openable`, `args`; fragments `name`, `scene`, `group`; plus top-level `prompts`. |
| `ui/styles_manifest.json` | Themed style/drawable names consumed by the theme builder. |

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
- Layout algorithms: `MindTreeLayout`, `MindRowTreeLayout`.

## Invariants

- **`.tscn`-first.** Static UI, world scaffolding, autoloads and debug views live in scenes so the project is navigable in the editor. Rust owns behavior, not static scene construction.
- Any node built in code needs a `# code-instantiated: <specific reason>` comment at the site (`spawned at runtime`, `pooled/high-churn`, `count driven by server rows`).
- **GDScript is UI-only.** It reads sim JSON and signals (`get_state_json`, `state_changed`) and never mutates sim state.
- **Manifest ABI.** A new dialog or fragment must be added to three places: the static instance under its layer group in `scenes/ui/ui_root.tscn`, `ui/dialogs_manifest.json`, and the `EXPECTED_*_DIALOGS`/fragment lists in `mind-core::ui::manifest`. `MindUiRoot._bind_manifest`/`_verify_manifest` and `MindThemeBuilder.verify()` cross-check the manifests at boot.
- **Bundle keys.** User-visible strings go through `_t("@key")`; debug-only strings are the exception.
- `scenarios/` is an exact, generated mirror of the repo-root `scenarios/`; edit the canonical files and run `bash tools/sync_scenarios.sh`.
- File headers (UTF-8/LF, `SPDX-License-Identifier: GPL-3.0-only`, upstream citation) follow the root [`AGENTS.md`](../AGENTS.md).

## Conventions

- `MindWidgets` factories are the way dialogs build themed widgets; text/icon lookup routes through `MindAssets`.
- Style names in `styles_manifest.json` are the parity ABI; only presentation values in `MindStyles`/`MindThemeBuilder` are local.
- `MindTable`'s fluent `add(child).grow_x_axis().pad(4)` API mirrors `Table.add(...)`; call `row()` to start a new row.
- Addon trees under `addons/` are vendored; change them only when working on the addon itself.

## Verification

Run from the repo root.

```bash
bash tools/godot.sh --headless --editor --quit --path client                # import/parse gate
godot4 --headless --path client --script res://tools/shaders_check.gd       # shader compile gate
godot4 --headless --path client --script res://tools/ui_widgets_check.gd    # widget atlas/min-size gate
godot4 --headless --path client --script res://tools/ui_layout_check.gd     # MindTable min-size propagation gate
```

`tools/ci.sh` runs the full local gate (composition in [`tools/AGENTS.md`](../tools/AGENTS.md)); in-engine checks run through the `open_godot_mcp` addon — load the `playtest` skill for the launch flow, node map and pid-stamped recipes.
