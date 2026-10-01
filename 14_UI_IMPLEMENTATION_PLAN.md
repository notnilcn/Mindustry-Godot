# 14 — UI IMPLEMENTATION PLAN

> Every source file this plan produces starts with `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.` (Rust), `## Ported from Mindustry ... — GPL-3.0` (GDScript), or `; Ported from Mindustry ... — GPL-3.0` (`.tscn`/`.tres`).
> Template: `HIGH_LEVEL_PLAN.md` §4 (nine sections, this file). Locked decisions inherited from §0: D1–D9. Do not re-litigate them here.
> Read order for the implementing agent: `HIGH_LEVEL_PLAN.md` → this file → the Mindustry `AGENTS.md` files named in §1.

## 1. Header block

| Field | Value |
|---|---|
| **Status** | Draft v1 — 2026-10-01, not started. Reconciled with the written siblings 00/02/03/04/05/07/11 (§3.11). Plans 12/13/15/16/19/21/22 are **not yet on disk**; §3.11 records the exact interfaces this plan assumes for each so the orchestrator can reconcile by filename. Three `NEEDS USER DECISION` items in §8 (OD-UI1, OD-UI2, OD-UI3); each has a working default and execution continues on it. |
| **Phase** | P5 — Logic, UI, input |
| **Depends on** | `02_CONTENT_IMPLEMENTATION_PLAN.md` (content records, icons, tech graph, teams/commands/stances — written 2026-10-01), `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (`GameState`/`State`/events, read-only sim surface — written 2026-10-01), `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` (Rules, Saves, Schematics, Universe/Planet/Sector, GameStats, tech runtime — **not yet written; interface assumed in §3.11**). Transitively 00 (spine, MCP, GDScript conventions), 01 (`mind-stdb` connector for Join/reconnect and the menu relay), 03 (atlas/`Tex`/`Icon`/`Iconc`/fonts/bundles/`IntFormat`), 04 (settings, save slots, `MindIo`), 06/07/08/09/10/11 (hover data, `ConfigUiSpec`, item/core/power/liquid/status/unit read models), 18 (`Sounds.ui*` playback). |
| **Blocks** | `15_INPUT_RTS_IMPLEMENTATION_PLAN.md` (HUD/placement/command surfaces provided here), `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md` (shared widget + dialog framework provided here; editor dialogs owned there), `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` (menu/text-input/chat relay payloads provided here), `22_PLATFORM_EXPORT_IMPLEMENTATION_PLAN.md` (native file dialogs/mobile keyboard/URI surface), `20_MODS_IMPLEMENTATION_PLAN.md` (ModsDialog/ModBrowserDialog UI consumes mod APIs), `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` (MCP UI scenario + UI goldens). |
| **Sources** | `Mindustry/core/src/mindustry/core/UI.java` (753 lines, read in full); `Mindustry/core/src/mindustry/ui/AGENTS.md`, `core/src/mindustry/core/AGENTS.md`, `core/src/mindustry/input/AGENTS.md`, `core/src/mindustry/audio/AGENTS.md`, `core/assets/AGENTS.md` (all read in full); `ui/{Styles,Fonts,Bar,BorderImage,WarningBar,GridImage,MobileButton,ReqImage,MultiReqImage,Links,IntFormat,ItemsDisplay,CoreItemsDisplay,Minimap,FileChooser,Menus,Displayable,Elems}.java` (read in full); `ui/builder/*` (all read); `ui/dialogs/*.java` (all 39 enumerated; BaseDialog read in full, SettingsMenuDialog/ContentInfoDialog read, the rest skimmed for fields/methods); `ui/fragments/*.java` (all 14; HudFragment/MenuFragment/MinimapFragment read in full, the rest skimmed); `ui/layout/*` (enumerated); `core/GameState.java`; `world/meta/StatValues.java` (display surface); `editor/MapEditorDialog.java`, `logic/LogicDialog.java`, `logic/LCanvas.java` (interface skim). No UI unit tests exist upstream (`tests/src/test/java/**` contains none — see §7a). |
| **Extends spine** | (a) Scene: adds `UiRoot` (`res://scenes/ui/ui_root.tscn`) under the plan-00 `Spine/Ui` CanvasLayer with `MenuGroup`, `HudGroup`, `DialogLayer`, `OverlayLayer`, `LoadingLayer`; keeps `/root/Spine/Ui/StateInspector`. (b) Rust: registers the `MindUi` autoload (Rust `Node`) owning the dialog registry, pause governor, margins, toasts/announce/prompts and the server-menu host; `MindHud` read-only per-frame state surface for HUD fragments. (c) `MindSimHost` (plan 00 API) is read-only for UI; no new sim methods are added here. (d) `mind-headless` gains Godot-free subcommands `ui text`, `ui dsl`, `ui menu-tree`, `ui manifest`. (e) MCP: `MindUi.*` `#[func]` API and the `ui_sweep` scenario (§7c) become part of the stable test API (plan 00 §3.10 rule 4). (f) State inspector gains a `UiState` row (dialog stack + margins) fed by `MindUi.dialog_stack()`. |
| **License** | GPL-3.0 (D6). Behavioral and data-key parity with upstream is the goal; code is a port, not a copy, except where explicitly noted. |

## 2. Scope & parity definition

### 2.1 In scope

The complete client UI for the pure-Rust/Godot client:

1. **UI root and lifecycle** (`core/UI.java`): `menuGroup`/`hudGroup` equivalents, the global dialog registry and eager construction on `ClientLoadEvent`, show/hide, pause-on-dialog semantics (`BaseDialog.shouldPause`, restoring `wasPaused` only when `state.isGame() && !net.active()`), safe-area insets + `uiEdgePadding` margins, resize event propagation, `updateScrollFocus` equivalent, `getIcon`, `loadAnd`.
2. **Widget + theme layer**: all `Styles.*` fields mapped to Godot `Theme` resources/type variations; `Fonts.def/outline/icon/iconLarge/tech/logic/monospace` plus content-icon glyph lookup (`Iconc`); the custom widget set (`Bar`, `BorderImage`, `WarningBar`, `GridImage`, `MobileButton`, `ReqImage`, `MultiReqImage`, check button (`Elems`), scroll behavior/focus, table/cell layout, tooltips); the text renderer (bundle `{0}` formatting, `IntFormat`, `:name:` tokens, color tags, `formatIcons`, `formatTime`, `formatAmount`, `roundAmount`).
3. **Dialog framework and every global dialog**: `BaseDialog` semantics (`cont`/`buttons`/`titleTable`/`titleImage`, `addCloseButton`, `closeOnBack`, `shown`/`hidden`/`update`/`onResize`, `makeButtonOverlay`); and the 39 `ui/dialogs/*` classes plus the dialogs owned elsewhere whose shells live here (`MapEditorDialog` shared shell points 19; `LogicDialog` shell 13/14).
4. **One-off prompt helpers** (`UI.java`): `showInfo`, `showInfoFade`, `showInfoToast`, `showInfoPopup`, `showLabel` (world labels), `showInfoOnHidden`, `showStartupInfo`, `showErrorMessage`, `showException`, `showText`, `showInfoText`, `showSmall`, `showConfirm`, `showCustomConfirm`, `showOkText`, `showTextInput` (desktop dialog + mobile native input), `announce`, `showUnlock`/`showToast`, `hasAnnouncement`.
5. **Fragments / in-game HUD**: `HudFragment` (waves/health/shields/ammo/payload/status bars, boss bar, objectives/wave text, core items, "core under attack", toasts/unlocks, hud text, pause/waiting/fps overlays, editor teams panel, block search/selection) — note there is **no** `ObjectivesFragment` in the pinned source; objectives render through `HudFragment`'s status text with data from plan 12's `MapObjectives`/`Objectives`, and this plan owns only the text composition, `MenuFragment` (desktop + mobile menus, `MenuButton` submenus, custom buttons), `MinimapFragment` + `Minimap` widget (click-pan, scroll-zoom, mobile tap-ping), `ChatFragment` (modes, history, chat input), `ConsoleFragment` (in-game console; JS-file injection replaced — OD1), `PlayerListFragment` (admin/kick/ban entry points), `PlacementFragment` (block/category palette, command table, hover info), `BlockConfigFragment`/`BlockInventoryFragment`/`PlanConfigFragment` (config + payload + plan UIs), `HintsFragment`, `LoadingFragment`, `FadeInFragment`, `PerformanceFragment`.
6. **Dynamic/server menus and MSUI DSL**: `Menus` equivalent (`@Remote` menus → plan 01/21 relay), `MenuBuilder`, `UiBuilder`, `UiDslParser`/`UiDslWriter`/`UiTreeBuilder`, `MenuResult`, `UiKey`, `UiStyleLookup`, `UiHotReload`.
7. **Item/core displays and requirement images**: `ItemsDisplay` (launched items), `CoreItemsDisplay`, `ReqImage`/`MultiReqImage` for block requirements, `Displayable` hover info for tiles/buildings/units (data from 06/07/11), block/research stats display (formatting side of `Stats`/`StatValues`).
8. **File chooser**: Godot native `DisplayServer.file_dialog_show` first, `FileChooserDialog` fallback scene; open/save flows consumed by `LoadDialog`/`SaveDialog` (04), map/schematic import/export (04/19), `UiHotReload`.
9. **Mobile layout branches and touch widgets**: `Vars.mobile` equivalent, `HudFragment` mobile branch (flip/collapse, portrait handling), `MenuFragment.buildMobile`, `MobileButton`, mobile text input, long-press/double-tap variants, safe-area gutters.
10. **All Godot-free UI logic that must be testable headless**: text/markup translation, `IntFormat`, stat display, HUD text composition, DSL parse/write/validate, `MenuResult` caps, dialog/style manifests, `Displayable` data model.

### 2.2 Definition of done

`cargo test -p mind-core` runs every §7a test with no Godot/network; `mind-headless ui *` scenarios pass with committed goldens; the MCP `ui_sweep` scenario opens every dialog in `client/ui/dialogs_manifest.json` and every HUD element with screenshot + visibility evidence; the §7d budgets are met. "Done" means a player can navigate the standalone menu, all in-game HUD elements, every dialog, all prompt helpers, server-sent menus and the full MSUI DSL with 1:1 behavior and bundle-key/icon parity — not that the underlying editor (19), input handling (15), rendering (16), audio (18), mods (20) or networking (21) are complete. Where a dialog depends on an unwritten plan (12/13), "done" means the shell, layout, bindings and the §3.11 interface stubs exist, flagged in the changelog.

### 2.3 Explicit boundaries (who owns what)

| Area | This plan (14) owns | Deferred to |
|---|---|---|
| Scene tree, CanvasLayer layers, widget nodes | all `Control`/`Container` layout, `MindTable`/`MindDialog`, theme resources, dialog scenes, fragment scenes | — |
| Keybinds, mouse/touch/keyboard event routing, placement state (`input.block`, build plans), camera pan/zoom, locks/focus | **displays** input state and forwards user clicks; `MindUi.has_dialog()/has_keyboard()/has_field()/locked()` predicates; no raw event logic beyond widget-local `gui_input` | `15_INPUT_RTS_IMPLEMENTATION_PLAN.md` |
| World rendering, minimap texture/entities, menu/load background renderers, world labels, screenshots | draws only inside the UI CanvasLayer; calls `MindRenderer.minimap_*`/`menu_*` APIs and paints returned textures | `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` |
| Editor dialogs (`MapEditorDialog` + info/generate/resize/load/objectives/waves/processors/locales/banned/assets), `SectorGenerateDialog`, `EditorMapsDialog` editor half | shared `MindDialog`, `MindTable`, `FileChooser`, `ColorPicker`, `PaletteDialog`, `MapListDialog` base, `NodeName`/`spawner` widgets | `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md` |
| mlog editor model (`LStatement` drag/layout, `LCanvas` internals, variable display model), `CanvasEditDialog` content | `LogicDialog` shell, `LogicCanvasHost` scene, shared widgets; plan 13 supplies `LCanvas` model + statement widgets per §3.11 | `13_LOGIC_MLOG_IMPLEMENTATION_PLAN.md` |
| SFX/music playback, UI bus filtering, pause filtering | semantic calls only (`MindAudio.play_ui("uiButton")`, `uiBack`, `uiNotify`, `uiUnlock`); no `AudioStream` ownership | `18_AUDIO_IMPLEMENTATION_PLAN.md` |
| Settings keys/defaults/persistence (`Core.settings` shape) | SettingsMenuDialog layout/categories and read/write through the 04 `SettingsStore` facade; no file I/O | `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` |
| Save slots, schematics, rules, planets/sectors, game stats, tech runtime | dialogs and displays over 04/12 APIs; this plan owns only view models and formatting | `12_CAMPAIGN_IMPLEMENTATION_PLAN.md`, `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` |
| Mod discovery/import/browser data, script engine | ModsDialog/ModBrowserDialog UI + error surfaces; no zip/HTTP loading | `20_MODS_IMPLEMENTATION_PLAN.md` |
| `Menus` `@Remote` transport, command ordering, join/host protocol, chat transport | client-side menu materialization, `MenuResult` encoding, text-input result encoding; server-side `MenuBuilder` API in `mind-core` | `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` |
| Native file dialogs, `openURI`, clipboard, file share, mobile virtual keyboard, Discord dialog integration | abstraction calls + fallback scenes | `22_PLATFORM_EXPORT_IMPLEMENTATION_PLAN.md` |
| Theme/style name ABI, `Iconc` codes, atlas regions, bundle keys, fonts | consumes read-only | `02_CONTENT_IMPLEMENTATION_PLAN.md`, `03_ASSETS_IMPLEMENTATION_PLAN.md` |
| Hover data (tiles/buildings/units), `BarSpec`, `ConfigUiSpec`, item selection data, unit/payload/status read models | renders them; defines the `HoverInfo`/`StatDisplay`/`BarModel` view structs and their formatting | `06`, `07`, `08`, `09`, `10`, `11` |
| Sim state, events, pause state transitions | listens (events) and calls `MindSimHost.set_paused`/`request_dialog_pause`; never mutates sim outside those APIs | `05_SIM_CORE_IMPLEMENTATION_PLAN.md` |

### 2.4 Deliberate deviations (reason stated)

1. **scene2d → Godot `Control`.** The UI is built from Godot `Control` nodes and `.tscn` scenes. `Table`/`Cell` behavior is reproduced by a GDScript `MindTable`/`MindCell` layout container (OD-UI1), not by Godot `GridContainer`/`VBoxContainer`, because the Mindustry dialect relies on cell semantics (`grow` + `expand` + `colspan` + `uniform` + min/pref/max + per-cell pad/align) that are load-bearing in nearly every dialog. Behavior parity, not API parity: dialog code uses a `MindTable` fluent API in GDScript, not `arc.scene` classes.
2. **No runtime font-glyph injection.** Arc injects atlas regions into `Fonts.def` as glyphs (`Fonts.registerIcon`). Godot exposes no supported runtime glyph-injection API, so content icons render through a `RichTextEffect` (`[icon name="copper"]`) and a PUA normalization pass that maps `Iconc` codepoints back to names (OD-UI3; handshake with plan 03 R8).
3. **`Styles.*` are not compile-time fields.** Upstream `Styles.load()` builds style objects in Java and generated `Tex` fields reference them. Here a single runtime-built `Theme` (`ui/theme/styles.gd`) maps every `styles_manifest.json` entry to a `StyleBox`/color/font type variation. Name lookup (`UiStyleLookup` equivalent) resolves against the same manifest, so the DSL's `style: "grayt"` behaves identically.
4. **`Menus` uses the plan-21 relay, not `Call` packets.** `@Remote menuBuilder/menu/textInput/announce/...` become relay command/event records. Because there are no Java peers (HLP §9), the payload encoding is our own versioned format (`ui_node` wire `format: 1`, §6.1) while preserving the `UiKey` ordinal ABI and the DSL text format 1:1.
5. **`ConsoleFragment` no longer evaluates JS.** Upstream injects Rhino scripts from a `.js` file. Scripting is OD1 (plan 20). The console becomes a line-based command interpreter whose commands are registered in Rust (plan 22's server console + plan 20's script hook); UI behavior (history, ping names, scroll, mobile toggle) is preserved.
6. **`EffectsDialog` catalogue comes from plan 17.** The dialog shell, paging and search live here; the effect list/preview rendering is plan 17's `EffectEntry` catalogue.
7. **`HudFragment`'s debug rate table (`if(false)`) is not ported.** It is dead code upstream; the fps/ping/tps/memory block is ported and mapped to Godot `Performance` monitors + `MindNet.ping()`.
8. **Godot `Performance` replaces Arc `PerfCounter` in `PerformanceFragment`; `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` owns the canonical metric names.**
9. **Mod emoji/content-icon registration into the font is dropped** (upstream code itself is commented out); mod UI icons resolve via `uiIcon` regions like vanilla.
10. **`Vars.mobile` is `mind_gdext::platform::is_mobile()`** (`OS.has_feature("mobile")`), overridable by `--mobile-preview` for MCP/tests; the mobile branches are otherwise ported verbatim.

## 3. Target design

### 3.1 Repository layout (additions to HLP §2.1)

```
client/
  scenes/ui/
    ui_root.tscn                      # MenuGroup/HudGroup/DialogLayer/OverlayLayer/LoadingLayer
    dialogs/  <snake_name>_dialog.tscn             # 1:1 with ui/dialogs/*; static sections only
    fragments/ hud_fragment.tscn, menu_fragment.tscn, minimap_fragment.tscn, ...
    widgets/   mind_bar.tscn, mind_req_image.tscn, mind_grid_image.tscn, mind_warning_bar.tscn,
               mind_border_image.tscn, mind_mobile_button.tscn, mind_minimap.tscn, mind_tree.tscn,
               mind_logic_canvas_host.tscn, color_picker_pad.tscn, node_name_editor.tscn
  ui/
    ui_root.gd                        # Ui root: layer wiring, margin updates, resize fan-out, dialog host
    mind_dialog.gd                    # BaseDialog equivalent (extends Window? -> Control + center container)
    mind_table.gd  mind_cell.gd       # scene2d Table/Cell layout port (OD-UI1)
    mind_stack.gd  mind_scroll.gd     # Stack / ScrollPane equivalents
    mind_widgets.gd                   # factory helpers (label/button/image/field/check/slider/table/pane/space)
    theme/ styles.gd, theme_builder.gd, icon_effect.gd, style_lookup.gd
    text/ mind_label.gd, mind_rich_label.gd   # change-cached label bindings (IntFormat parity)
    dialogs/  <snake_name>_dialog.gd            # one per ui/dialogs/*
    fragments/ hud_fragment.gd, menu_fragment.gd, minimap_fragment.gd, chat_fragment.gd,
               console_fragment.gd, player_list_fragment.gd, placement_fragment.gd,
               block_config_fragment.gd, block_inventory_fragment.gd, plan_config_fragment.gd,
               hints_fragment.gd, loading_fragment.gd, fade_in_fragment.gd, performance_fragment.gd
    layout/   tree_layout.gd, branch_tree_layout.gd, radial_tree_layout.gd, row_tree_layout.gd
    builder/  mind_dsl_view.gd, style_lookup.gd
    dialogs_manifest.json             # name -> scene path -> pause -> args -> menu_openable (§6.4)
    styles_manifest.json              # all Styles.* names by target kind (§6.5)
  rust/
    mind-core/src/ui/
      mod.rs          text.rs        # format_icons / markup -> BBCode / PUA normalization / amount/time
      int_format.rs   # thin re-export of plan 03 Bundle IntFormat helpers if not already exposed there
      hud_text.rs     # HudFragment status/objective text composition (IntFormat-driven)
      stat_display.rs # Stats formatting side (StatValue display kinds, percent/color rules)
      display.rs      # Displayable equivalent: HoverInfo/DisplayRow model + provider traits
      builder/        ui_key.rs, ui_node.rs, dsl.rs (parser), dsl_writer.rs, tree_builder.rs,
                      menu_builder.rs, menu_result.rs, style_lookup.rs
      manifest.rs     # dialogs_manifest.json / styles_manifest.json loaders + validators
    mind-gdext/src/ui/
      mod.rs, ui_host.rs     # MindUi autoload: registry, pause governor, margins, prompts, file chooser
      hud.rs                 # MindHud: typed per-frame properties + discrete signals
      menu_host.rs           # server menu materialization, MenuResult capture/upload
      dsl_factory.rs         # UiNode -> Godot Control via GDScript factory bridge
      theme_boot.rs          # builds Theme from styles_manifest + AtlasIndex at AssetsReadyEvent
      native_dialogs.rs      # DisplayServer.file_dialog_show + platform fallbacks (plan 22 hook)
```

### 3.2 UI root and lifecycle

Replaces `mindustry.core.UI` (`Vars.ui`). Two cooperating objects:

- **`UiRoot` (GDScript, scene node `/root/Spine/Ui/UiRoot`)** — layout root: `MenuGroup` (visible iff `state.is_menu()`), `HudGroup` (visible iff `state.is_game()`), `DialogLayer`, `OverlayLayer` (announce/toast/popup/world-label), `LoadingLayer`. `_ready()` loads `dialogs_manifest.json`, instantiates all global dialogs and fragments **eagerly** (parity with `UI.init()`), parenting them hidden under the correct layer, and asks `MindUi` for margins.
- **`MindUi` (Rust autoload `Node`, `/root/MindUi`)** — logic: dialog registry keyed by manifest name, show/hide (single active scene dialog, matching `Core.scene.getDialog()`), pause governor, prompt helpers, `show_text_input`, toast/announce queue, world labels, popups-by-id, `formatIcons`/text helpers exposed to GDScript, and the `Menus` host. It finds `UiRoot` and all registered dialogs via a `register_dialog(name, node)` handshake performed by the manifest instantiator.

Boot order: `ClientLoadEvent` (plan 00/03) → `MindUi.init_registry(manifest)` → `UiRoot` instantiates scenes → each `MindDialog._ready()` registers → `StyledTheme` built → `Fonts` applied → `UI` groups enabled by `StateChangeEvent`. No UI exists before assets are ready; `LoadingFragment`/`FadeInFragment` are owned by the root and available first.

Pause governor semantics (exact port of `BaseDialog`):

```text
shown:  if should_pause and state.is_game() and !net.active(): record was_paused; set_state(paused)
hidden: if should_pause and state.is_game() and !net.active() and !was_paused: set_state(playing)
        play sounds.uiBack (only on explicit hide, not on state-change teardown)
```

A dialog marked `should_pause` in the manifest declares it; runtime `should_pause` is exported on `MindDialog` so dynamic dialogs (LogicDialog) can set it. The governor is reference-counted so nested pause dialogs restore correctly. `MindUi.close_top_dialog()` backs the Android back button (`NOTIFICATION_WM_GO_BACK_REQUEST`).

Margins (`UI.updateMargins`): `safe = DisplayServer.get_display_safe_area()` converted to UI insets; `custom = round(Scl * settings.get_i32("uiEdgePadding", 0))`; portrait → top/bottom, landscape → left/right. Applied to `UiRoot` offsets and exposed as `MindUi.margin_left/right/top/bottom` for HUD gutters (`paneRight`/`paneLeft`/`paneTop` ninepatches in `HudFragment`/`MenuFragment`).

### 3.3 Widget + theme layer

**Mapping table (scene2d → Godot).** This is the contract every dialog script follows.

| scene2d | Godot target | Notes |
|---|---|---|
| `Table` + `Cell` | `MindTable` (`Container`) + `MindCell` | GDScript port of scene2d layout: `row()`, `colspan`, `uniform`, `grow/fill/expand[X/Y]`, `width/height/size/min*/max*`, `pad*`, `align/labelAlign`, `defaults()`, `margin`, `background`, `clearChildren`, `rebuild-on-show`. Float cell values interpreted in Scl-scaled pixels. |
| `Stack` | `MindStack` (`Container`) | children share the cell rect; used by ReqImage/MultiReqImage/HUD stacks. |
| `ScrollPane` | `MindScroll` (wraps `ScrollContainer` + `MindTable`) | `setFadeScrollBars`, `setScrollingDisabled(x,y)`, `setScrollYForce`, `hasScroll`, `requestScroll`, `updateScrollFocus`; mouse-focus lifecycle ported. |
| `Label` | `MindLabel` (`RichTextLabel`, `fit_content`, `bbcode_enabled`) | `labelAlign`, `wrap`, `style` pick; text set through the §3.7 renderer; `MindLabel.set_source(callable)` caches last formatted value. |
| `Image` | `TextureRect` (+`MindBorderImage` for the border variant) | `scaling=fit/bounded/none`, `imageUpColor` etc. map to `modulate`. |
| `Button`/`TextButton` | `Button` + theme type variation | `Styles.defaultt`... become custom theme types (`defaultt`, `grayt`, ...). |
| `ImageButton` | `Button` with `icon` child + variation | `resizeImage`, `getImageCell`, `imageUp/Down/Over/Disabled/CheckedColor` map to icon `modulate`. |
| `TextField` | `LineEdit` (single) / `TextEdit` (area) | `maxLength`, message text, filter (digits/hex), `setCursorPosition`, invalid state. |
| `CheckBox` | `MindCheck` (Button-based, `Elems.check` port) | drawn check images, `checked`/`update`. |
| `Slider` | `HSlider` + theme variation | `min/max/step`, `moved`, knob visuals. |
| `Tree` | `MindTree` | used by KeybindDialog groups; `TreeStyle` plus/minus. |
| `Dialog` | `MindDialog` (§3.4) | `Window`-like centered panel; title table + accent line. |
| `Tooltip` | `MindTooltip` | `allowMobile`, custom `textProvider` (black8 + margin 4), `setContainerPosition` top-left variant for `addDescTooltip`. |
| `Collapser` | `MindCollapser` | animated min-size collapse (`setCollapsed`, `setDuration`). |
| `Actions` | Godot `Tween` helpers in `mind_widgets.gd` | `fadeIn/out`, `translateBy`, `scaleTo`, `delay`, `remove`; Interp curves mapped (`fade`→`EASE_OUT`, `pow3Out`/`pow4In`→matching `TransitionType`). |

**Styles mapping.** `styles_manifest.json` (§6.5) has one entry per `Styles.java` field: drawables (`black, black9, black8, black6, black5, black3, grayPanel, grayPanelDark, none, flatDown, flatOver, accentDrawable`), button styles (`defaultb, underlineb`, `defaultt, flatt, grayt, flatTogglet, logicTogglet, flatToggleMenut, togglet, cleart, clearTogglet, fullTogglet, squareTogglet, logict, flatBordert, nonet`), image button styles (`defaulti, nodei, emptyi, emptyTogglei, selecti, logici, geni, grayi, graySquarei, flati, squarei, squareTogglei, grayTogglei, clearNonei, cleari, clearTogglei, clearNoneTogglei`), panes (`defaultPane, horizontalPane, smallPane, noBarPane`), `defaultSlider`, labels (`defaultLabel, outlineLabel, techLabel, monoLabel`), fields (`defaultField, nodeField, areaField, nodeArea`), `defaultCheck`, dialogs (`defaultDialog, fullDialog`), `defaultTree`. `theme_boot.rs` builds a single `Theme` at `AssetsReadyEvent` from `mind_assets` region/split data; `default*` become the type defaults, the rest are theme type variations keyed by field name. Dialogs must instantiate their widgets through `mind_widgets.gd` helpers or set `theme_type_variation` explicitly; `style_lookup.gd` resolves `UiStyleLookup.get(StyleKind, name)` against the manifest so MSUI `style: "grayt"` works.

Drawable composition rules: alpha tints of `whiteui`/`button` become `StyleBoxFlat` colors; ninepatch regions (`bar.9`, `pane`, `button`, `windowEmpty`, `scroll*`, `underline*`, `slider*`, `check*`, `flat-down-base`, `wavepane`, `buttonEdge4`, `inventory`, `paneRight/paneLeft/paneTop`, `discordBanner`, `infoBanner`) become `StyleBoxTexture` with margins from the plan-03 atlas `splits`/`pads`. `createFlatDown()`'s zeroed content margins are preserved (a `StyleBoxTexture` subclass in `styles.gd` overriding min sizes). `Styles.*` are never mutated at runtime (upstream rule); variants are cloned per use where a dialog needs a tweak.

**Bar** (`ui/Bar.java`): `MindBar` (`Control`) draws back/top ninepatches, blink/lerp (`blink = lerp(blink,0,0.2)`, `value = lerp(value,computed,0.15)`), outline, and centered outline-font name; `set/flash/blink/outline/snap/reset` API. HUD + block info + research use `MindBar` bound to a `Callable` returning the fraction (GDScript) or a `BarModel` id resolved by `MindHud.bar_value(id)` (Rust-computed for sim-backed bars, e.g. boss HP).

**BorderImage/WarningBar/GridImage**: pure `_draw()` ports (stroke rect around a texture; skewed quad stripes + lines; computed grid lines). GridImage `setImageSize` used by `Minimap`/`MapView`-adjacent widgets.

**ReqImage/MultiReqImage**: `MindReqImage` stacks a child and a red X overlay with `valid` predicate; `MindMultiReqImage.act` cycles visible requirement display every 1/60 s when none is valid (exact `time += delta/60`).

**Check button** (`Elems.check`): `MindCheck` (Button + icon + label, background toggles `grayPanel`, check images `checkOn/checkOff/checkOver/checkOnOver`, `left()`).

**Focus/scroll rules** (from `UI.java` and ui/AGENTS.md): after adding scroll panes call `update_scroll_focus()`; clicking outside a field clears keyboard focus; `hasField`, `hasDialog`, `hasKeyboard`, `hasMouse` predicates exposed by `MindUi` and consumed by plan 15; min-scroll-focus tracking for the selected building's flow window (07 §R9).

**Mobile**: `MindMobileButton` (icon + wrapped label), `Vars.mobile` branches ported per fragment; `tooltip.allow_mobile`; double-tap detection for favorites; larger `defaults().size(...)`; native keyboard via `MindUi.show_text_input`; `uiEdgePadding` gutters drawn with the pane ninepatches.

### 3.4 Dialog framework

`MindDialog` (GDScript, `ui/mind_dialog.gd`) is the `BaseDialog` equivalent and all dialog scripts extend it:

- Exports/fields: `title_text`, `should_pause`, `full_dialog` (style), `title_color`.
- Nodes: `titleTable` (title label centered + accent line `titleImage`), `cont` (`MindTable`, scrollable via `makeButtonOverlay()`), `buttons` (`MindTable`).
- API: `show_dialog()`, `hide_dialog()`, `set_title_color()`, `add_close_button(width=210)` (adds `@back` + `Icon.left`, then `close_on_back`), `close_on_back(extra)`, `make_button_overlay()`, `shown/hidden/update_callables`, `on_resize(fn)` (fires only while this dialog is the current scene dialog; skipped while a mobile text input is open), `key_down` handlers for Enter/Escape/Back, `is_shown()`.
- Hooks into `MindUi`: every `show`/`hide` calls `MindUi._dialog_shown(self)`/`_dialog_hidden(self)` so the pause governor and "current dialog" state stay authoritative; `MindUi` is the only place `state.set(paused)` happens.
- `titleImage` uses `Tex.whiteui` accent color (from plan 03).

Dialogs are eagerly constructed by `UiRoot` from `dialogs_manifest.json` and added hidden to `DialogLayer`; `show_dialog()` reparents to front/visible (parity with scene2d `Dialog.show()`), plays the 0.1 s fade in/out actions, and registers as the current scene dialog. Dialog scripts rebuild dynamic content in `shown`/`on_resize` exactly where upstream does; static structure lives in the `.tscn`.

**Dialog catalogue** (all from `UI.init()`; manifest names are the GDScript registry keys):

| Dialog | Manifest key | Pause | Primary data dependency |
|---|---|---|---|
| `AboutDialog` | `about` | no | `Links` entries, `Version`, contributors file (03) |
| `AdminsDialog` | `admins` | no | `NetServer.admins` (21) |
| `BansDialog` | `bans` | no | `NetServer.admins` (21) |
| `CampaignCompleteDialog` | `campaign_complete` | yes | `Universe`, planets (12), campaign completion state |
| `CampaignRulesDialog` | `campaign_rules` | no | `Planet`/`Sector` defaults (12) |
| `CanvasEditDialog` | `canvas_edit` | no | `LCanvas` model (13) |
| `ColorPicker` | `picker` | no | color pad widget; hue texture from UI page (03) |
| `ContentInfoDialog` | `content` | no | `UnlockableContent.compute_stats()` + `displayExtra` (02/07/12) |
| `CustomGameDialog` | `custom` | no | `MapListDialog` + map registry (06/19) |
| `CustomRulesDialog` | `custom_rules` | no | `Rules` (12) + `LoadoutDialog`/`IconSelectDialog` |
| `DatabaseDialog` | `database` | yes | content DB tabs/categories/search (02) |
| `DiscordDialog` | `discord` | no | link constant; `openURI` via 22 |
| `EditorMapsDialog` | `editor_maps` | no | map registry (06/19) |
| `EffectsDialog` | `effects` | no | `EffectEntry` catalogue (17) |
| `FileChooserDialog` | `file_chooser` | no | `FileHistory`, FS listing (04); native-first path (22) |
| `FullTextDialog` | `full_text` | yes | bundle text |
| `GameOverDialog` | `restart` | no | `GameStats`, winning `Team`, campaign checks (12) |
| `HostDialog` | `host` | no | host config + launch flow (21/12) |
| `IconSelectDialog` | `icon_select` | no | `Icon.all` / content unlocks |
| `JoinDialog` | `join` | no | server list (settings + HTTP list), `NetClient` connect (01/21/22) |
| `KeybindDialog` | `controls` | no | `Binding.all` (15) |
| `LanguageDialog` | `language` | no | bundle `locales` (03/04) |
| `LaunchLoadoutDialog` | `launch_loadout` | no | core/loadout/sector items (08/12) |
| `LoadDialog` | `load` | no | `SaveSlot` listing (04); map play flow (12) |
| `LoadoutDialog` | `loadout` | no | `ItemSeq`, capacity, `IconSelectDialog` |
| `MapPlayDialog` | `map_play` | no | `Gamemode` help (12), `CustomRulesDialog` |
| `ModBrowserDialog` | `mod_browser` | no | mod browser API (20) |
| `ModsDialog` | `mods` | no | `LoadedMod` list, import/releases/errors (20) |
| `PaletteDialog` | `palette` | no | color list; used by editor (19) |
| `PausedDialog` | `paused` | yes | rules/campaign save flow (12), `Control` (15) |
| `PlanetDialog` | `planet` | yes | `Universe`/`Planet`/`Sector`, launch, sector select, g3d interaction (12/16) |
| `ResearchDialog` | `research` | yes | `TechTree` runtime, `ItemSeq`, `ItemsDisplay`, tree layouts (02/12) |
| `SaveDialog` | `save` | no | `SaveSlot` create/rename/delete (04) |
| `SchematicsDialog` | `schematics` | yes | `Schematics` registry, `.msch` import/export (04/12) |
| `SectorSelectDialog` | `sector_select` | no | planet sectors (12) |
| `SettingsMenuDialog` | `settings` | yes | `SettingsStore` (04); categories game/graphics/sound/dev/main/data/planet-data |
| `TraceDialog` | `traces` | no | `TraceInfo` from server (21) |
| `LogicDialog` (13/14) | `logic` | yes | `LCanvas` model (13); shell/vars/add-dialog here |
| `MapEditorDialog` (19) | `editor` | no | editor model (19); shell host point here |

Prompt helpers (implemented in `MindUi`, GDScript-rendered): the full list in §2.1 item 4. `showTextInput` uses the desktop `MindDialog` on desktop and the platform text-input path on mobile (22); numeric filtering (digits-only), `max_length`, Enter submits, OK disabled per `allowEmpty`, `closeOnBack(closed)`. `showInfoPopup`/`showLabel` keep the by-id replacement/removal semantics and `lastAnnouncement` tracking. World labels (`showLabel`) are drawn by plan 16 and driven by `MindUi`'s id map.

`shouldPause` manifest values are ported from source: `settings`, `database`, `planet`, `research`, `schematics`, `paused`, `logic`, `campaign_complete`, `full_text` = true; all others false.

### 3.5 Fragments and HUD

`HudFragment` (GDScript `hud_fragment.gd` + `hud_fragment.tscn`) builds the same named regions; data comes from `MindHud` (Rust, read-only properties) and `MindUi` signals. Node names preserve upstream names where possible; where a name is invalid for Godot (`minimap/position`), the node gets `mind_name` metadata and a `_position` stem.

HUD elements and their bindings:

| Region (upstream name) | Target node | Binding |
|---|---|---|
| `paused` | `PausedBanner` | `state.is_paused() && !netServer.is_waiting_for_players() && !(mobile&&portrait)`; text `@paused` / `@sector.curlost` |
| `pause-disabled` | `PauseDisabledBanner` | `pauseDisableDur` fade ported (`Time.delta / duration`) |
| `waiting` | `WaitingBanner` | `@waiting.players` |
| `minimap/position` | `MinimapBox` | `MindMinimap` widget + position label (`position`/`mouseposition` settings) |
| `overlaymarker` mobile | `MobileBar` | menu/flip/schematics/pause/chat buttons; `iconLogicHideHud`, iOS always shown |
| `waves/editor` stack | `WavesStack` | `WavesMain` (`waves`, `statustable`, `skip`, `infotable`) + `EditorMain` (`teams`, `addBlockSelection`) |
| `fps/ping` | `FpsBox` | `fps`, `memory`/`memory2`, `ping`, `tps` via `MindHud` + `Performance` |
| `coreinfo` | `CoreInfo` | `CoreItemsDisplay` collapser, core-attack banner (`control.last_damaged_core`, `Trigger.teamCoreDamage`), boss bar, hud text label |
| `nearpoint` | `NearPoint` | `spawner.player_near()` |
| `saving` | `SavingLabel` | `control.saves.is_saving()` |
| toasts/unlocks | `ToastLayer` | `scheduleToast` timing (3.5 s), translate/fade actions, max 7 unlock icons w/ `+` overflow |
| `PlacementFragment` | `Placement` | category tabs, block grid (search + favorites), command table, hover info box |
| `BlockConfigFragment` | `BlockConfig` | `ConfigUiSpec` from 07 (`build_configuration`): item/liquid/block/unit selection, sliders, clear |
| `BlockInventoryFragment` | `BlockInventory` | item take/withdraw click listeners (`Call.takeItems` → plan 21 command) |
| `PlanConfigFragment` | `PlanConfig` | `BuildPlan.point_config` / plan config data (07/15) |
| `HintsFragment` | `Hints` | `Hint` catalogue (ported as `hints` data with `complete/show/order/valid`), Serpulo/Erekir branch |
| `LoadingFragment` | `Loading` | progress bar, text, button, `loadAnd` helper |
| `FadeInFragment` | `FadeIn` | black fade on game start |
| `PerformanceFragment` | `Performance` | Godot `Performance` monitors, color thresholds |
| `MinimapFragment` | `MinimapFull` | fullscreen minimap draw (16) + pan/zoom/tap-ping; keyboard focus/scroll capture |
| `ChatFragment` | `Chat` | modes (all/team/global), history, `[accent]` name colors, ping, local echo, send via 21 |
| `ConsoleFragment` | `Console` | history, scroll buttons, command registry (Rust), mobile toggle, `injectConsoleVariables` equivalent |
| `PlayerListFragment` | `PlayerList` | per-player rows, admin/vote/kick/ban actions (21), trace dialog |
| `MenuFragment` | `Menu` | desktop buttons/submenu (play, database, editor, mods, settings) + mobile layout + custom buttons + logo/version + Discord banner |

`MindHud` surface (Rust `mind-gdext/src/ui/hud.rs`) exposes typed properties recomputed at most once per frame from read-only sim state, plus signals for discrete events:
`wave, wave_time, wave_timer, enemies, win_wave, rules_waves, rules_attack_mode, mission, objectives_text, unit_activation_text, state, game_over, after_game_over, campaign, editor, paused, player_healthf, player_shieldf, player_ammof, player_payload_used, player_icon, statuses (PackedStringArray on change), core_healthf, boss_text, boss_fraction, core_attack, last_damaged_core, spawner_near, saving, fps, memory_mb, ping, tps, position_text, core_items (rebuild-on-change)`, signals `toast(text, icon)`, `unlock(content_name)`, `hud_text(text)`, `announce(text, duration)`, `wave_event`, `sector_event(kind, name)`.

GDScript labels use `MindLabel.set_source(callable)` which formats and compares before assigning `text` (IntFormat parity; no text churn when values do not change).

### 3.6 Server menus and the MSUI builder

`mind-core::ui::builder` is a Godot-free 1:1 port:

- `UiKey` enum — same 54 ordinals and same order as `UiKey.java` (node types first, then `row`, then keys); ordinals are a frozen ABI (test `ui_key_ordinals_frozen`).
- `UiNode` — typed tree (`Table/Pane/Stack/Label/Image/Button/ImageButton/Field/Check/Slider/Space/Defaults/ButtonTable`) with typed entries (`Str/F32/Bool/Node`), the same builder methods as `UiBuilder` (`grow_x`, `pad_left`, `colspan`, `width`, `style`, `clicked`, ...), `id`, `condition`.
- `dsl::parse` / `dsl_writer::write` — exact grammar port: `key: value` props, `row`, `node { ... }`, shorthand `label: "text"` / `image: "region"` (image shorthand maps the value to `region`), bare childless nodes, `//` comments, `\n` escapes, line-numbered errors, quoted-vs-bare coercion (`true/false`/float detection).
- `tree_builder` — materializes `UiNode` through a `UiTreeSink` trait with the same semantics: element `id` collection, `condition` evaluation (`portrait`, `landscape`, `width >= N`, `height >= N`), `MenuResult` capture on click, `images` collection for `TextureStreamEvent`-style overlay updates. Sinks: (a) `mind-gdext::ui::dsl_factory` (Godot), (b) headless JSON dumper for goldens.
- `menu_builder`/`menu_result` — `MenuBuilder` fields (`title, hide_on_click, hide_existing, fill_screen, token, id, ui`), `MenuResult` with caps (`maxResultLen=500`, `maxValueLen=1000`, `maxTotalStringLen=6000`, `maxTotalValues=500`) and typed getters.
- `style_lookup` — name → `StyleKind` from `styles_manifest.json`.

Client runtime (`mind-gdext/src/ui/menu_host.rs` + `ui/builder/mind_dsl_view.gd`): `Menus.menuBuilder` creates/hides a `MenuDialog` (a `MindDialog`), builds the tree, wires ids, records streamed image regions and swaps drawables when atlas overlays arrive (03 mod overlay), and re-sends `menuBuilderUpdate` by replacing the target table. `MenuBuilderChoose`/`MenuChoose`/`TextInputResult` are captured as `MenuResult`/option/text and sent through the plan-21 relay. Unreliable vs reliable variants map to STDB send modes defined by plan 21.

Hot reload: `UiHotReload.show()` (desktop console only) opens an `.msui` file, watches mtime with a 100 ms debounce, rebuilds a preview dialog, and renders parse errors with the source line.

### 3.7 Text, formatting, icons

All string rendering goes through `mind-core::ui::text` (Rust, Godot-free, fully testable) plus a thin GDScript layer:

- `bundle(format, args)` uses plan 03's `Bundle` (locale chain, `global.properties`, `{0}` formatting). `IntFormat` lives in plan 03 (`mind-core/src/assets/bundle.rs`) and is re-exported; `hud_text.rs`/`stat_display.rs` own the per-frame cached formats (`wave`, `wave.cap`, `wave.enemy(s)`, `wave.enemycore(s)`, `wave.waiting`, `fps`, `ping`, `tps`, `memory`, `memory2`).
- `format_icons(s)` — exact `UI.formatIcons` port (split on `:`, `Iconc.codes` first, then `Fonts.unicode_str`, verbatim on miss, single-pass builder).
- **Markup renderer (BBCode)**: `render_markup(s) -> String` output is Godot BBCode consumed by `RichTextLabel`. Rules:
  - Mindustry color tags map through `UI.loadColors` (`accent`, `unlaunched`, `highlight`, `stat`, `negstat`) plus the `Pal`/`Colors` registry names used in bundles (`gray`, `lightgray`, `white`, `red`, `orange`, `scarlet`, `#rrggbb`, `#rrggbbaa`). `[tag]` opens `[color=#rrggbbaa]`; `[]` closes to the *previous* color (translation maintains its own stack, since Mindustry `[]` resets to default rather than using BBCode nesting) and emits the necessary close/reopen sequence.
  - `:name:` tokens (after `format_icons`) become `[icon name="name"]`; `Iconc` PUA codepoints embedded directly in strings are mapped back to `[icon name]` via `Iconc.code_to_name` so `unit.emoji()`/team emoji work.
  - Unknown `:` tokens and unknown tags stay literal; a lone `[` that is not a known tag is escaped to `[lb]`.
  - `\n` is preserved; `wrap`/alignment set per `MindLabel`.
- `icon_effect.gd` — `RichTextEffect` with `bbcode = "icon"`, resolves `AtlasTexture` (03), draws at the run position with `font_size`-derived size, tint from `color`/current color, `scale` property; used by all labels, bars, tooltips and menus.
- Fonts: `Fonts.def` (18 px, shadow dark gray offset y+2), `outline` (18 px + border 2·Scl), `icon` (30 px), `iconLarge` (48 px + 5 px dark gray border, unscaled), `tech` (18 px, `down *= 1.5`), `logic` (16 px, ASCII only), `monospace` (16 px, fallback def). Godot `FontFile` resources loaded by plan 03; the theme applies per-style `font`/`fontColor`/`disabledFontColor`.
- `format_time(ticks)` (0:ss / m:ss / h:mm:ss) and `format_amount` (`∞`, B/M/k with `[gray]` unit words from `unit.billions/millions/thousands`) and `round_amount` are exact ports; the `billions/millions/thousands` strings come from `UI.init()` bundle reads.
- `stat_display.rs` renders `StatValue` display kinds supplied by 02/07 (`text`, `text + value`, number + unit (percent/rate/tiles/items), item/liquid lists, time, bar) with `[stat]`/`[negstat]` colors and `StatValues.fixValue`; `ContentInfoDialog`/BlockInfo/Database use it.
- `display.rs` defines `Displayable` equivalent: `HoverInfo { rows: Vec<DisplayRow> }`, `DisplayRow::{Text, IconText, Bar{label,color,fraction}, Items(Vec<(ItemId,i32)>), Status(Vec<(StatusId,f32)>) }`, produced by provider traits implemented over 06/07/11 read models (`TileInfo`, `BuildingInfo`, `UnitInfo`). `PlacementFragment` queries the hovered object at ≤ 30 Hz and shows the tooltip table.

### 3.8 File chooser

`MindUi.open_file_chooser(params)` / `save_file_chooser(params)`:
`FileChooserParams { open, allow_multiple, title, file_name, extensions[], handler, multiple_handler }` ported (default title from `open`/`save` keys, `file.` + first extension default, filename sanitization, `extEquals` filter).
Path: Godot native `DisplayServer.file_dialog_show(...)` (returns a callback with paths) when available (desktop/Android SAF), else `FileChooserDialog` scene (directory navigation, file history back/forward, text field, extensions filter, overwrite confirmation on save). iOS/local share handled by plan 22's platform hook. `FileChooser.export(name, ext, writer)` = save dialog + `ui.loadAnd` + writer + exception surface; `Control.saves`/map/schematic flows (04/12/19) call these APIs.

### 3.9 Godot / STDB surfaces touched

- **Godot nodes (stable paths):** `/root/MindUi` (autoload), `/root/Spine/Ui/UiRoot`, `/root/Spine/Ui/UiRoot/{MenuGroup,HudGroup,DialogLayer,OverlayLayer,LoadingLayer}`, dialog roots `/root/Spine/Ui/UiRoot/DialogLayer/<DialogName>`, HUD roots `/root/Spine/Ui/UiRoot/HudGroup/<FragmentName>`. Existing `/root/Spine/Ui/StateInspector` is untouched.
- **`MindUi` `#[func]` test/runtime API:** `open_dialog(name, ctx_json="")`, `close_dialog(name)`, `close_top_dialog()`, `is_dialog_shown(name)`, `dialog_stack() -> PackedStringArray`, `set_margins(l,r,t,b)`, `show_info(text)`, `show_text(title,text)`, `show_text_input(title,msg,len,def,numeric,allow_empty) -> signal`, `show_confirm(...)`, `announce(text,duration)`, `toast(text,icon="")`, `play_ui_sound(name)`, `open_file_chooser(...)`, `hud_set_visible(bool)`, `chat_toggle()`, `console_toggle()`, `has_dialog()`, `has_field()`, `has_keyboard()`, `locked()`, `menu_result_json(id)`.
- **`MindHud` properties/signals:** §3.5.
- **STDB:** no tables/reducers are defined here. Payloads carried by plan 21's relay (owned by 21, shapes defined in §6.3): `MenuBuilderShow/Update/Hide`, `MenuChoose`, `MenuBuilderChoose`, `TextInput`, `TextInputResult`, `HudText`/`HideHudText`, `Announce`, `InfoPopup`, `Label`, `InfoToast`, `WarningToast`, `OpenUri`, `CopyToClipboard`. `JoinDialog` uses plan 01's `mind-stdb` reconnect state and plan 21's server list/connection APIs; `ChatFragment`/`PlayerListFragment`/admin dialogs use plan 21 reducers/views.

### 3.10 Boundaries & invariants

1. `mind-core` gains **no** Godot/tokio dependency; `mind-core::ui` is pure data/format/parser code. CI boundary grep stays green.
2. GDScript contains **no game rules and no sim reads**: it binds to `MindHud`/`MindUi` properties and signals only; all numbers originate from Rust read models.
3. `MindUi` is the only owner of dialog visibility and pause transitions; `MindTable`/widgets never call `SimHost.set_paused`.
4. Every user-visible string goes through a bundle key; no hardcoded English except debug-flagged dev strings (mirrors ui/AGENTS.md).
5. Content names/IDs, bundle keys, `Iconc` codes, style names and `.msui` semantics are parity ABI; nothing here renames them.
6. Per-frame GDScript updates use change-cached bindings; per-frame Rust getters allocate only typed primitives; `core_items`/`statuses` arrays are rebuilt only when the underlying set changes.
7. All scenes/scripts used by tests carry stable names; MCP evals are pid-stamped per plan 00 §3.10.
8. `mind-core::ui` tests never require Godot, network or assets (fixtures inline).

### 3.11 Sibling reconciliation (by filename)

| Plan | State | Interface consumed/provided | Reconcile action |
|---|---|---|---|
| `00_FOUNDATION_IMPLEMENTATION_PLAN.md` | written | Consumes `Spine` scene, `MindSimHost` (`get_state_json`, `set_paused`, `state_changed`), state-inspector protocol, GDScript/scene conventions, MCP launch flow, `scenarios/*.json`. Provides `UiRoot` layout under `Spine/Ui` + `MindUi` autoload. | Orchestrator: confirm `Spine/Ui` insertion point and that adding `UiRoot` does not break plan 00's inspector path; `MindUi` is an autoload (new line in `project.godot`). |
| `02_CONTENT_IMPLEMENTATION_PLAN.md` | written | Consumes `Unlockable` fields (`localizedName/description/details/credit`, `ui_icon` region chain, `databaseCategory/databaseTag/databaseTabs/shownPlanets`, `unlocked*`, `researchRequirements`), `Category`, `Item`, `Liquid`, `BlockDef`/`UnitTypeDef` stat/bar data, `StatusEffect`, `Planet`/`SectorPreset`, `UnitCommand`/`UnitStance` (icons/keys), `TeamEntry` (`displayExtra` key `team.<name>.log`), `TechNode` graph, `ContentRef`. | Orchestrator: `TeamEntry` `displayExtra` rendering (PlayerList/unit tooltips) is owned here; plan 02 must expose `display_extra(key) -> rows`. |
| `03_ASSETS_IMPLEMENTATION_PLAN.md` | written | Consumes `MindAssets`/`AtlasIndex` (regions + `splits/pads`), `Tex.get`, generated `tex/icon/iconc` consts, `Fonts` (TTFs, sizes, `icon_glyph`), `Bundle` + `IntFormat`, `AssetsReadyEvent`, mod overlay hooks, `locales`. Provides the `[icon]` RichTextEffect contract (R8 handshake) and `.9.png`/ninepatch consumption. | Orchestrator: freeze the `[icon name]` protocol + `MindAssets.icon_texture(name)` API used by `icon_effect.gd`; the M7 "RichTextLabel handshake" item is this plan's `icon_effect.gd` + `render_markup`. |
| `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` | written | Consumes `SettingsStore` (all `settings.get_bool/get_i32/put` uses), `MindIo` autoload (`save_game/load_game/list_saves/list_maps`), `SaveSlot`, map registry listing, `TypeIO` for schematic/map import/export, `config/` + `saves/` paths. Provides Save/Load dialogs (04 §7c step 12 repeats save/load through these) and the file-chooser flows. | Orchestrator: confirm `MindIo` bridge calls used by `LoadDialog`/`SaveDialog` (`list_saves`, `save_game`, `load_game`, `delete`, rename) are append-only additions. |
| `05_SIM_CORE_IMPLEMENTATION_PLAN.md` | written | Consumes `GameState`/`State` (`is_menu/game/paused/editor/campaign/after_game_over`), `StateChangeEvent`, `EventBus` events (`ResetEvent`, `WaveEvent`, `UnlockEvent`, `SectorCapture/Lose/Invasion`, `ResizeEvent`, `MenuOptionChooseEvent`, `MenuBuilderOptionChooseEvent`, `TextInputEvent`, `TextureStreamEvent` equivalent), `Trigger.teamCoreDamage`, `ClientHooks` (`on_state_change`). Provides UI predicates/signals and the `MindHud` read surface. | Orchestrator: plan 05's `ClientHooks` may need an `on_ui_event` notify or `MindUi` subscribes to the Rust event bus directly (preferred: UI subscribes to `Sim.events` in `mind-gdext`, never in `mind-core`). Reconcile event-listener ownership. |
| `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` | **not written** | Assumed interface: `Rules` field access + `RuleOverrideSpec`; `Saves`/`SaveSlot` listing; `Schematic` registry (`all()`, tags, preview data, import/export); `Universe` (turns, launch, production/import, `clearLoadoutInfo`); `Planet`/`Sector` runtime (sector info, stats, `isBeingPlayed`, threat, locked state); `GameStats`; `TechTree` runtime (`roots`, `each`, `node.reset`, `finished` requirements, unlock/spend); `CampaignRules`; `Loadout`/`addStartingItems`; `MapObjectives`/`Objectives` (`qualified/text/details/hidden`); `TeamData.cores/bosses`; `AttackIndicators`; `Difficulty`. | **Orchestrator must reconcile names** when 12 is written: every dialog in the catalogue table that lists "12" needs a concrete accessor. Until then, dialogs build against stub structs owned by 14's view layer with identical names. |
| `13_LOGIC_MLOG_IMPLEMENTATION_PLAN.md` | **not written** | Assumed interface: `LogicDialog` shell gets `show(code, executor, privileged, modified)`; `LCanvas` model owns statement list/add/remove/save/load/rebuild, drag layout, jump curves, variable display model; `CanvasEditDialog` content model; `TraceInfo` source for `TraceDialog`. This plan provides `MindDialog` shell, `LogicCanvasHost` scene, variable table widgets and the add-statement dialog frame. | **Orchestrator must reconcile**: HIGH_LEVEL §3 assigns `LCanvas` to 13 and "logic editor hooks (UI in 14)"; the split must be: 13 = model + statement widgets; 14 = dialog shell + canvas host + variable list. |
| `15_INPUT_RTS_IMPLEMENTATION_PLAN.md` | **not written** | Provides to 15: `MindUi` focus predicates, placement palette surface (`placement_set_blocks/categories`, `placement_select`, `placement_rebuild`), command table (`command_bar_set`), stance bar, chat/console toggle, keybind dialog (`Binding.all` read model), `Binding.toggleMenus/skipWave/blockInfo/menu/ping` id names consumed by HUD update blocks, camera pan calls for minimap/core-attack. Consumes from 15: `input.block`, `is_building`, `command_mode`, `locked()`, `pan_camera`. | **Orchestrator must reconcile**: the HUD update blocks that call `logic.skipWave()`/`Call.adminRequest(wave)` become `MindInput`/relay calls owned by 15/21; the names above freeze the boundary. |
| `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` | **not written** | Consumes: minimap texture + entity overlay (`minimap.get_texture`, `minimap_to_screen/screen_to_world`, `zoom_by`, `set_zoom`, `draw_entities`); menu/load background renderers; world label draw; screenshots; g3d planet interaction data for `PlanetDialog`; `Draw`-equivalent color helpers. Provides: overlay CanvasLayers, world-label registration, screenshot file paths. | **Orchestrator must reconcile**: `Minimap`/`MinimapFragment` container here, painting there; `MenuRenderer`/`LoadRenderer` are 16 nodes referenced by `MenuFragment`/`LoadingFragment`. |
| `19_MAPS_EDITOR_IMPLEMENTATION_PLAN.md` | **not written** | Provides: `MindDialog`/`MindTable`/`FileChooser`/`ColorPicker`/`PaletteDialog`/`MapListDialog`/`NodeName`/`spawner` widgets, `MapPlayDialog`/`MapListDialog` shells, editor-config host hook (`BlockConfigFragment` reuse). Consumes: `MapEditorDialog` shell scene hooks, `EditorMapsDialog` subclass, `SectorGenerateDialog`, `EffectsDialog`? (17). | **Orchestrator must reconcile**: editor dialogs are 19; the shared `MapListDialog`/`MapPlayDialog` base lives here and 19 subclasses it. |
| `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` | **not written** | Provides: server-menu payload encoding (§6.3), `MenuResult` capture, text-input/announce/toast/label payloads, chat send/receive hooks, player/admin/ban/trace dialogs' data hooks, Join/Host connection state. Consumes: plan 21's relay send API and admin reducers/views. | **Orchestrator must reconcile**: `@Remote` method names → relay command variants; `Menus` is UI-side only. |
| `22_PLATFORM_EXPORT_IMPLEMENTATION_PLAN.md` | **not written** | Provides: native file dialogs, `openURI`, clipboard, share, mobile virtual keyboard, Discord dialog link opening, `Vars.mobile` source. Consumes: `FileChooser` params API and `show_text_input`. | **Orchestrator must reconcile**: platform hook names (`platform.show_file_chooser`, `platform.open_uri`, `platform.set_clipboard`, `platform.text_input`). |

## 4. Port map

Legend: **R** = Rust (`mind-core` unless noted), **G** = GDScript, **S** = Godot scene, **GX** = `mind-gdext` Rust.

| Mindustry source | Target | Notes on adaptation |
|---|---|---|
| `core/UI.java` | `GX ui/ui_host.rs` (`MindUi`) + `G ui/ui_root.gd` + `S ui_root.tscn` | Dialog registry/fragments eagerly built (parity); `WidgetGroup` groups → `Control`s; `updateMargins` → safe-area + `uiEdgePadding`; `formatIcons/formatTime/formatAmount/roundAmount` → R `ui/text.rs`; `loadColors` → R color registry; prompt helpers → `MindUi` functions + GDScript dialogs; `showLabel` → `MindUi` id map + 16 draw. |
| `ui/Styles.java` | `G ui/theme/styles.gd` + `theme_builder.gd` + `styles_manifest.json` | Runtime `Theme` built from manifest + atlas splits; `default*` = type defaults, others = variations; `Styles` mutation rule preserved. |
| `ui/Fonts.java` | plan 03 `mind-gdext/src/assets/fonts.rs` + `G ui/text/*` + `icon_effect.gd` | TTF load/scaling/outline from 03; glyph injection replaced by `[icon]` effect + PUA normalization (OD-UI3); `getGlyph` for warning toasts uses `AtlasTexture`. |
| `ui/Elems.java` | `G ui/widgets/mind_check.gd` | `Elems.check` button port (not a real checkbox). |
| `ui/Bar.java` | `G ui/widgets/mind_bar.gd` | lerp/blink/outline/scissor top draw; name via outline font; `BarModel` binding. |
| `ui/BorderImage.java` | `G ui/widgets/mind_border_image.gd` | `_draw` stroke; `drawAlpha` uses `alphaBg`; `forceNearest` sets texture filter on the fly. |
| `ui/WarningBar.java` | `G ui/widgets/mind_warning_bar.gd` | skew quads + top/bottom lines (debug/overflow usage). |
| `ui/GridImage.java` | `G ui/widgets/mind_grid_image.gd` | min-space 10 px jump logic; `set_image_size`. |
| `ui/MobileButton.java` | `G ui/widgets/mind_mobile_button.gd` | icon button + wrapped label; `row()` semantics. |
| `ui/ReqImage.java` | `G ui/widgets/mind_req_image.gd` | stack + red X overlay + `valid`. |
| `ui/MultiReqImage.java` | `G ui/widgets/mind_multi_req_image.gd` | cycle visible every 1 s (`time += delta/60`) when none valid. |
| `ui/Links.java` | `G dialogs/about_dialog.gd` + bundle | Link list data; `link.<name>.title/.description` keys; icons from `Icon.*`. |
| `ui/IntFormat.java` | plan 03 `R assets/bundle.rs` (`IntFormat`) + `R ui/hud_text.rs` | Cached per-frame formats; no duplicate implementation. |
| `ui/ItemsDisplay.java` | `G widgets/items_display.gd` | Collapser, shine flash on gain, `UI.formatAmount`. |
| `ui/CoreItemsDisplay.java` | `G widgets/core_items_display.gd` | 4-column used-item grid, tooltips, rebuild on new item use. |
| `ui/Minimap.java` | `G widgets/mind_minimap.gd` | 140 px, right-click pan via `control.input.panCamera`, scroll zoom, tap toggles fullscreen, mobile drag zoom. |
| `ui/FileChooser.java` | `GX ui/native_dialogs.rs` + `G dialogs/file_chooser_dialog.gd` | Params/validation port; native-first; fallback scene; export/loadAnd flow. |
| `ui/Menus.java` | `R ui/builder/menu_builder.rs` + `menu_result.rs` + `GX ui/menu_host.rs` | `@Remote` → plan 21 relay records; `MenuDialog`, follow-up menus, listeners, text-input routing, toasts/announce/popups/labels; `MenuResult` caps. |
| `ui/Displayable.java` | `R ui/display.rs` + `G fragments/placement_fragment.gd` hover box | `HoverInfo` rows from 06/07/11 read models; ≤ 30 Hz query. |
| `ui/builder/UiBuilder.java` | `R ui/builder/ui_node.rs` | typed nodes/entries; all cell/node props; `write/read` byte codec (§6.1). |
| `ui/builder/UiKey.java` | `R ui/builder/ui_key.rs` | ordinal-frozen enum; `first_cell_key` boundary test. |
| `ui/builder/UiDslParser.java` | `R ui/builder/dsl.rs` | exact grammar + line-number errors. |
| `ui/builder/UiDslWriter.java` | `R ui/builder/dsl_writer.rs` | `to_string`/writer parity (indentation/quoting rules). |
| `ui/builder/UiTreeBuilder.java` | `R ui/builder/tree_builder.rs` + `GX dsl_factory.rs` + `G builder/mind_dsl_view.gd` | `UiTreeSink` trait; condition eval; id collection; image map; `menuBuilderUpdate`. |
| `ui/builder/MenuBuilder.java` | `R ui/builder/menu_builder.rs` | fields + `of/dsl/show/show_all/update`. |
| `ui/builder/MenuResult.java` | `R ui/builder/menu_result.rs` | caps + typed getters + cancellation. |
| `ui/builder/UiStyleLookup.java` | `R ui/builder/style_lookup.rs` + `G theme/style_lookup.gd` | resolves against `styles_manifest.json`. |
| `ui/builder/UiHotReload.java` | `G dialogs/ui_hot_reload.gd` (desktop console command) | file picker + mtime watch + debounce + error line rendering. |
| `ui/dialogs/BaseDialog.java` | `G ui/mind_dialog.gd` | `cont`/`buttons`/title/accent/close/pause/onResize/makeButtonOverlay. |
| `ui/dialogs/AboutDialog.java` | `G dialogs/about_dialog.gd` + `S` | links grid, credits, contributors. |
| `ui/dialogs/AdminsDialog.java` | `G dialogs/admins_dialog.gd` | admin list/add/remove (21). |
| `ui/dialogs/BansDialog.java` | `G dialogs/bans_dialog.gd` | ban list/unban (21). |
| `ui/dialogs/CampaignCompleteDialog.java` | `G dialogs/campaign_complete_dialog.gd` | planet complete flow (12). |
| `ui/dialogs/CampaignRulesDialog.java` | `G dialogs/campaign_rules_dialog.gd` | planet rule toggles (12). |
| `ui/dialogs/CanvasEditDialog.java` | `G dialogs/canvas_edit_dialog.gd` | canvas size/text editor (13 model). |
| `ui/dialogs/ColorPicker.java` | `G dialogs/color_picker.gd` | hue/alpha pads; used by editor/team colors. |
| `ui/dialogs/ContentInfoDialog.java` | `G dialogs/content_info_dialog.gd` | `compute_stats()` + `stat_display.rs`; `displayExtra` hook; patched indicator; console field link. |
| `ui/dialogs/CustomGameDialog.java` | `G dialogs/custom_game_dialog.gd` | extends `MapListDialog`. |
| `ui/dialogs/CustomRulesDialog.java` | `G dialogs/custom_rules_dialog.gd` | category tabs; `check/number/numberi/text/team` builders; `LoadoutDialog`/`IconSelectDialog` hooks (12). |
| `ui/dialogs/DatabaseDialog.java` | `G dialogs/database_dialog.gd` | search, tabs, unlock filters, content info open (02). |
| `ui/dialogs/DiscordDialog.java` | `G dialogs/discord_dialog.gd` | invite link card; `openURI` (22). |
| `ui/dialogs/EditorMapsDialog.java` | `G dialogs/editor_maps_dialog.gd` | map import/export (04/19). |
| `ui/dialogs/EffectsDialog.java` | `G dialogs/effects_dialog.gd` | effect grid + preview (17 catalogue). |
| `ui/dialogs/FileChooserDialog.java` | `G dialogs/file_chooser_dialog.gd` | nav field, file list, filter, history, last-directory static. |
| `ui/dialogs/FullTextDialog.java` | `G dialogs/full_text_dialog.gd` | scrollable text. |
| `ui/dialogs/GameOverDialog.java` | `G dialogs/game_over_dialog.gd` | stat count-up animation, retry/menu (12 stats). |
| `ui/dialogs/HostDialog.java` | `G dialogs/host_dialog.gd` | host config + launch (21). |
| `ui/dialogs/IconSelectDialog.java` | `G dialogs/icon_select_dialog.gd` | icon grid + locked filter. |
| `ui/dialogs/JoinDialog.java` | `G dialogs/join_dialog.gd` | server list fetch/parse/save, version mismatch, direct connect, reconnect (01/21/22). |
| `ui/dialogs/KeybindDialog.java` | `G dialogs/keybind_dialog.gd` | grouped binds, rebind capture dialog, reset (15 bindings). |
| `ui/dialogs/LanguageDialog.java` | `G dialogs/language_dialog.gd` | locale list + display names (03/04). |
| `ui/dialogs/LaunchLoadoutDialog.java` | `G dialogs/launch_loadout_dialog.gd` | core item selection for launch (08/12). |
| `ui/dialogs/LoadDialog.java` | `G dialogs/load_dialog.gd` | slot grid, meta, rename/delete, autosave; `SaveDialog` override points (04). |
| `ui/dialogs/LoadoutDialog.java` | `G dialogs/loadout_dialog.gd` | capacity grid, item stepping, reseed (08). |
| `ui/dialogs/MapListDialog.java` | `G ui/map_list_dialog.gd` | search/sort/type toggle, custom-map row menu base (06/12). |
| `ui/dialogs/MapPlayDialog.java` | `G dialogs/map_play_dialog.gd` | mode buttons, help panel, play/playtest (12/19). |
| `ui/dialogs/ModBrowserDialog.java` | `G dialogs/mod_browser_dialog.gd` | browser list/search/install (20). |
| `ui/dialogs/ModsDialog.java` | `G dialogs/mods_dialog.gd` | imported mods, enable/disable, details, import progress, error dialog, updates (20). |
| `ui/dialogs/PaletteDialog.java` | `G dialogs/palette_dialog.gd` | palette grid (19). |
| `ui/dialogs/PausedDialog.java` | `G dialogs/paused_dialog.gd` | save/quit buttons, campaign leave, playtest check (12/15). |
| `ui/dialogs/PlanetDialog.java` | `G dialogs/planet_dialog.gd` + `S` | g3d host view, sector selection, launch/sector select modes, `PlanetInterfaceRenderer` hooks (12/16). |
| `ui/dialogs/ResearchDialog.java` | `G dialogs/research_dialog.gd` + layouts | tree roots selector, node graph, zoom/pan, item display, spend/unlock (02/12). |
| `ui/dialogs/SaveDialog.java` | `G dialogs/save_dialog.gd` | new-save row, overwrite, export (04). |
| `ui/dialogs/SchematicsDialog.java` | `G dialogs/schematics_dialog.gd` | list, tags, import/export, edit, info, preview image (04/12). |
| `ui/dialogs/SectorSelectDialog.java` | `G dialogs/sector_select_dialog.gd` | radial/row layout sector pick (12). |
| `ui/dialogs/SettingsMenuDialog.java` | `G dialogs/settings_menu_dialog.gd` | categories (game/graphics/sound/dev/main), `SettingsTable`/`Setting` row model, rebuild, data dialogs (04). |
| `ui/dialogs/TraceDialog.java` | `G dialogs/trace_dialog.gd` | trace rows + copy (21). |
| `ui/fragments/BlockConfigFragment.java` | `G fragments/block_config_fragment.gd` | scale-in panel at building position; `ConfigUiSpec` (07). |
| `ui/fragments/BlockInventoryFragment.java` | `G fragments/block_inventory_fragment.gd` | item grid, click take, mouse-wheel amounts, position near cursor. |
| `ui/fragments/ChatFragment.java` | `G fragments/chat_fragment.gd` | modes/prefix validation, history, `checkPing`, toast notification when closed. |
| `ui/fragments/ConsoleFragment.java` | `G fragments/console_fragment.gd` | line editor, history, scroll buttons, command registry hook; JS injection dropped (OD1). |
| `ui/fragments/FadeInFragment.java` | `G fragments/fade_in_fragment.gd` | alpha fade. |
| `ui/fragments/HintsFragment.java` | `G fragments/hints_fragment.gd` + `R ui/hints.rs` | hint catalogue as data (name/text/complete/show/order/valid + Serpulo branch). |
| `ui/fragments/HudFragment.java` | `G fragments/hud_fragment.gd` + `hud_fragment.tscn` + `GX ui/hud.rs` | all regions from §3.5; block search/favorites; teams; toasts/unlock; `setHudText`; `shown()`/`logicHideHud`. |
| `ui/fragments/LoadingFragment.java` | `G fragments/loading_fragment.gd` | progress/fraction/text/button, `toFront`, `loadAnd`. |
| `ui/fragments/MenuFragment.java` | `G fragments/menu_fragment.gd` | desktop/mobile builds, submenu fade, custom `MenuButton`s, version/logo, `checkPlay` mod-error guard. |
| `ui/fragments/MinimapFragment.java` | `G fragments/minimap_fragment.gd` | fullscreen draw (16), pan/zoom, keyboard/scroll capture, ping text input. |
| `ui/fragments/PerformanceFragment.java` | `G fragments/performance_fragment.gd` | monitor graph/table; Godot `Performance` + `MindPerf` counters. |
| `ui/fragments/PlacementFragment.java` | `G fragments/placement_fragment.gd` | category tabs, block grid (locked/dark states), command table, hover info; data from 02/07/11/15. |
| `ui/fragments/PlanConfigFragment.java` | `G fragments/plan_config_fragment.gd` | plan config popup (point config data 07/15). |
| `ui/fragments/PlayerListFragment.java` | `G fragments/player_list_fragment.gd` | rows, admin icon, per-player dialog (kick/ban/trace/ping), `Call` → relay (21). |
| `ui/layout/TreeLayout.java` + `Branch/Radial/Row` | `G ui/layout/*.gd` | tree positioning math for ResearchDialog/SectorSelect. |
| `editor/MapEditorDialog.java` | `G dialogs/map_editor_dialog.gd` (shell only) | editor internals 19; this plan ships the shell scene + shared widgets. |
| `logic/LogicDialog.java` | `G dialogs/logic_dialog.gd` + `G widgets/mind_logic_canvas_host.gd` | shell/vars/add dialog here; statement model 13. |
| `game/WorldLabel` (Menus.showLabel) | `GX ui/ui_host.rs` labels map + 16 renderer | id-based create/remove/update; flags/duration. |
| `graphics/MenuRenderer`, `LoadRenderer` (consumed) | 16 nodes referenced from `menu_fragment.gd`/`loading_fragment.gd` | draw-only; UI shows progress/buttons over them. |
| `core/PerfCounter` (display half) | `G fragments/performance_fragment.gd` | canonical metrics 16/23. |

## 5. Milestones & task breakdown

Each milestone ends with the §7 checks for its scope; evidence (test output, screenshot paths, dump diffs) is appended to the changelog. The first vertical slice is M0: one dialog opened from one menu button with themed widgets and formatted text.

**M0 — UI foundation + vertical slice (About dialog).**
Deliver: `UiRoot` scene + `MindUi` autoload; `MindTable`/`MindCell`/`MindStack`/`MindScroll`; theme builder + `styles_manifest.json` for the subset used by M0; `MindLabel` + `render_markup` + `icon_effect.gd`; `MindDialog`; `MenuFragment` shell with one About button; `AboutDialog` with links grid.
Verify: MCP: launch → click About → dialog visible → screenshot non-blank → close; `cargo test -p mind-core ui::text`; `mind-headless ui text` golden.

**M1 — Styles + widget set complete.**
Deliver: every `Styles.*` entry in the manifest + theme; `MindBar`, `MindBorderImage`, `MindWarningBar`, `MindGridImage`, `MindMobileButton`, `MindReqImage`, `MindMultiReqImage`, `MindCheck`, tooltips, collapser, tweens; `ItemsDisplay`/`CoreItemsDisplay`.
Verify: theme completeness test (every manifest name resolves in GDScript boot check); MCP widget gallery scene screenshot (`/root/Spine/Ui/UiRoot/DialogLayer/WidgetGallery` dev-only) with one of each widget; budget: theme build ≤ 120 ms.

**M2 — Prompts, toasts, loading, hints.**
Deliver: `MindUi` prompt helpers + `show_text_input` desktop/mobile; `menu_group` toasts/announce/popups/world-label maps; `LoadingFragment`, `FadeInFragment`, `HintsFragment`, `PerformanceFragment`; pause governor + `close_top_dialog` back handling.
Verify: headless `ui::hud_text`; MCP: eval each prompt helper, type into text input, assert signal payload; pause/release round trip.

**M3 — Menus + standalone dialogs.**
Deliver: complete `MenuFragment` desktop/mobile; dialogs: About, Settings (all categories + data dialogs via 04), Language, Keybind, Database, ContentInfo, IconSelect, Palette, ColorPicker, FullText, Discord, Mods/ModBrowser (20 stubs), Join/Host (01/21 stubs), Load/Save (04), GameOver, CustomRules (12 stubs), CampaignRules, CampaignComplete.
Verify: MCP `ui_sweep` menu-openable set; manifest completeness test.

**M4 — HUD + placement + block config.**
Deliver: full `HudFragment` + `MindHud`; `PlacementFragment` + command table hooks; `BlockConfigFragment` (`ConfigUiSpec`), `BlockInventoryFragment`, `PlanConfigFragment`; `Displayable` hover info; `Minimap`/`MinimapFragment` (renderer stub until 16); editor teams panel.
Verify: MCP HUD-vs-inspector numbers; headless `ui::display`/`ui::hud_text`; interaction: right-click block opens config, take items path via MCP relay stub.

**M5 — Campaign dialogs (12-dependent).**
Deliver: Planet, Research, Schematics, SectorSelect, LaunchLoadout, Loadout, MapList/MapPlay/CustomGame/EditorMaps shells, CampaignComplete/CampaignRules full bindings per §3.11 stubs; tree layouts.
Verify: MCP sweep for each with stub/fixture data; if 12 not landed, scenario marked `blocked-by-12` but shell still opens and renders.

**M6 — MSUI + server menu relay.**
Deliver: `mind-core::ui::builder` complete + goldens; `dsl_factory` materialization; `menu_host` show/update/hide/results; plan-21 relay handshake; `UiHotReload`; `UiStyleLookup`.
Verify: `mind-headless ui dsl|menu-tree`; two-client MCP scenario: host fixture sends `MenuBuilder` via STDB test relay → client dialog visible → click result observed server-side (plan 21 script once available).

**M7 — Chat/console/player list + file chooser + mobile.**
Deliver: Chat/Console/PlayerList fragments; command registry hook; `FileChooser` native+fallback with 04/19 flows; mobile branches and `is_mobile()` override; native keyboard path; safe-area gutters.
Verify: MCP mobile-preview run; file save/load round trip through the chooser; console command round trip.

**M8 — Full sweep, budgets, exit.**
Deliver: `ui_sweep` covering every manifest entry; perf baselines committed; docs (`client/ui/README.md` conventions), changelog evidence.
Verify: §7d budgets, §7e checklist.

## 6. Data & formats

### 6.1 `ui_node` wire encoding (server menus)

```
ui_node_message:
  u8  format = 1                     // ours; upstream NodeBuilder.write has no header
  node root                          // parent node for menuBuilder / update target

node:
  u8  node_type                      // UiKey.ordinal for node types (< first_cell_key)
  u16 entry_count
  entry[entry_count]

entry:
  u16 key_ordinal                    // UiKey.ordinal
  u8  tag                            // 0 str, 1 f32, 2 bool, 3 node
  payload: str = u16 len + utf8; f32 = little-endian IEEE754; bool = u8; node = recursive
```

Entry order is insertion order (builder order), matching upstream. `row` is an entry with `UiKey.row` and bool `true`. Unknown ordinals are a hard error (forward compatibility is a version bump, not a guess). The whole blob is stored in plan 21's relay rows as `bytes`; the relay schema is plan 21's.

### 6.2 Frozen `UiKey` ordinals

The 54 `UiKey` variants keep Java declaration order: node types `table, pane, stack, label, image, button, imageButton, field, check, slider, space, defaults, buttonTable`; then `row`; then `text, wrap, region, icon, placeholder, scaling, background, margin, id, hint, maxLength, checked, min, max, step, defaultValue, clicked, enter, style, group, condition, color, disabled`; then cell props `grow, growX, growY, fill, fillX, fillY, expand, expandX, expandY, width, height, size, minWidth, maxWidth, minHeight, maxHeight, pad, padTop, padLeft, padBottom, padRight, align, labelAlign, colspan, uniform, uniformX, uniformY`. A committed `ui_keys.txt` golden (ordinal + name per line) fails the build if reordered. Adding a key appends.

### 6.3 Relay payloads (shapes defined here; transport owned by 21)

| Payload | Fields | Direction |
|---|---|---|
| `menu_builder_show` | `id i32, token i64, title opt-str, hide_on_click, hide_existing, fill_screen, ui bytes` | server → client |
| `menu_builder_update` | `id i32, table_id str, ui bytes` | server → client |
| `menu_builder_hide` | `id i32` | server → client |
| `menu_choose` | `player, menu_id i32, option i32` | client → server |
| `menu_builder_choose` | `player, menu_id i32, result: {token i64, result opt-str, values: map<str, Str|F32|Bool>}` | client → server |
| `text_input` | `id i32, title, message, len i32, def, numeric, allow_empty` | server → client |
| `text_input_result` | `player, id i32, text opt-str` | client → server |
| `hud_text` / `hide_hud_text` | `message` | server → client |
| `announce` / `info_toast` / `warning_toast(icon_code, text)` | `message[, duration]` | server → client |
| `info_popup` | `message opt, id opt, duration, align, top, left, bottom, right` | server → client |
| `label` | `message opt, id i32, duration, worldx, worldy, flags i32` | server → client |
| `open_uri` / `copy_to_clipboard` | `value` | server → client |

Unreliable/reliable variants collapse to one payload; plan 21 chooses send mode (`hud_text`, `info_popup`, `label` unreliable; the rest reliable).

### 6.4 `client/ui/dialogs_manifest.json`

```json
{
  "format": 1,
  "dialogs": [
    {"name": "about", "scene": "res://scenes/ui/dialogs/about_dialog.tscn", "pause": false, "menu_openable": true, "args": []},
    {"name": "settings", "scene": "res://scenes/ui/dialogs/settings_menu_dialog.tscn", "pause": true, "menu_openable": true, "args": []},
    {"name": "game_over", "scene": "res://scenes/ui/dialogs/game_over_dialog.tscn", "pause": false, "menu_openable": false, "args": ["winner:team_ref"]}
  ],
  "fragments": [
    {"name": "hud", "scene": "res://scenes/ui/fragments/hud_fragment.tscn", "group": "hud"},
    {"name": "menu", "scene": "res://scenes/ui/fragments/menu_fragment.tscn", "group": "menu"}
  ],
  "prompts": ["show_info", "show_text", "show_confirm", "show_custom_confirm", "show_ok_text", "show_text_input", "show_info_text", "show_small", "show_info_fade", "show_info_toast", "show_info_popup", "show_label", "show_error", "show_exception", "announce", "toast", "unlock_toast"]
}
```

Validated by `manifest.rs` (names unique, scene paths resolvable on disk, pause flags match §3.4, fragment groups valid) and asserted at `UiRoot._ready()` (every entry registered, none missing). MCP `ui_sweep` iterates `dialogs[]`.

### 6.5 `client/ui/styles_manifest.json`

```json
{
  "format": 1,
  "drawables":  ["black","black9","black8","black6","black5","black3","grayPanel","grayPanelDark","none","flatDown","flatOver","accentDrawable"],
  "text_buttons": ["defaultt","flatt","grayt","flatTogglet","logicTogglet","flatToggleMenut","togglet","cleart","clearTogglet","fullTogglet","squareTogglet","logict","flatBordert","nonet"],
  "buttons": ["defaultb","underlineb"],
  "image_buttons": ["defaulti","nodei","emptyi","emptyTogglei","selecti","logici","geni","grayi","graySquarei","flati","squarei","squareTogglei","grayTogglei","clearNonei","cleari","clearTogglei","clearNoneTogglei"],
  "panes": ["defaultPane","horizontalPane","smallPane","noBarPane"],
  "sliders": ["defaultSlider"],
  "labels": ["defaultLabel","outlineLabel","techLabel","monoLabel"],
  "fields": ["defaultField","nodeField","areaField","nodeArea"],
  "checks": ["defaultCheck"],
  "dialogs": ["defaultDialog","fullDialog"],
  "trees": ["defaultTree"]
}
```

Generated from `Styles.java` by the oracle harness at capture time (§7a); Rust validates uniqueness/sort and the GDScript theme builder asserts each name resolves. Adding a `Styles` field requires updating this file (test fails otherwise).

### 6.6 Text goldens

`client/rust/mind-core/tests/goldens/ui/`: `format_icons.tsv`, `markup_bbc.tsv`, `format_time.tsv`, `format_amount.tsv`, `round_amount.tsv`, `ui_keys.txt`, `ui_node_wire.hex`, `hud_text.tsv`, `stats_display.tsv`. Captured once from the Java oracle harness; frozen in-repo (no JVM in CI).

### 6.7 UI settings keys (read/written by this plan; persistence 04)

`uiEdgePadding`, `fps`, `minimap`, `position`, `mouseposition`, `console`, `coreitems`, `macnotch`, `ui-hidden` (one-shot), `editor-blocks-shown`, `editor-block-favorites`, `linear`, `chatmode`. Key names are parity ABI.

## 7. Oracle & verification (REQUIRED)

### 7a. Ported tests

**Upstream has no UI unit tests.** `tests/src/test/java/**` contains only `ApplicationTests`, `DataAssetTests`, `PatcherTests`, `GenericModTest`, `ModTestAllure`, `LogicTests` and the power suite; none touch `mindustry.ui` (the UI is not constructible headless — `Vars.ui == null`). The AGENTS files confirm this ("Run gameplay changes by actually running the game"; "Don't assert on rendering"). We therefore substitute:

1. **Java-oracle capture harness** (run once, manually, in the Mindustry checkout; output committed): a throwaway `tools/src` main that instantiates the headless-free parts — `UI.formatIcons/formatTime/formatAmount/roundAmount`, the `Colors`/tag lists, `UiKey` names/ordinals, `NodeBuilder.write` bytes for a fixed tree, `IntFormat` outputs, `StatValues.fixValue` — and prints the §6.6 TSV/hex goldens. The dotnet-free repo only needs a JDK at capture time; CI consumes the frozen files. Documented command goes in the plan changelog when captured.
2. **Rust unit tests** (`cargo test -p mind-core`):
   - `ui::text::tests::{format_icons_matches_java_golden, markup_color_stack_reset, pua_icon_normalization, unknown_tokens_verbatim, format_time_golden, format_amount_golden, round_amount_golden}`
   - `ui::builder::tests::{ui_key_ordinals_frozen, dsl_parse_roundtrip_corpus, dsl_error_has_line_number, node_wire_roundtrip, node_wire_tag_errors, tree_builder_conditions, tree_builder_ids_and_images, menu_result_caps, menu_builder_fields}`
   - `ui::stat_display::tests::{fix_value_golden, stat_rows_textual, percent_and_rate_units, stat_negative_color}`
   - `ui::hud_text::tests::{wave_variants, win_wave_cap, enemy_counts, waiting_timer, objectives_compose, unit_activation_countdown}`
   - `ui::hostile::tests::{format_time_zero, amount_infinities_for_long_extremes, color_tag_unknown_is_literal}`
   - `ui::manifest::tests::{dialogs_manifest_valid, styles_manifest_matches_java, every_pause_dialog_flagged}`
3. **GDScript-side smoke assertions** (run by the Godot headless import check or a `--headless --script` fixture): every `styles_manifest` name resolves to a theme variation; every `dialogs_manifest` scene loads; `MindTable` layout golden for a 20-case cell matrix (pref/min/max/grow/colspan) rendered offscreen and compared against committed expected rects.

### 7b. Headless harness scenarios (`mind-headless ui ...`)

| Scenario | Input | Assertions |
|---|---|---|
| `ui text` | corpus of bundle-formatted strings + icon/color tags; inventory of every bundle key referenced by this plan's dialogs/fragments | `render_markup` output equals §6.6 goldens; `format_time/amount/round_amount` equal goldens; every UI key resolves through plan 03's `Bundle` chain and `{0}` formatting matches the Java oracle; zero allocations beyond the output buffer (alloc-audit flag) |
| `ui dsl` | every fixture `.msui` under `client/rust/mind-core/tests/fixtures/ui/` | parse → write → parse produces a structurally equal tree; normalized writer output equals golden; malformed fixtures fail with the Java line number |
| `ui menu-tree` | registered `MenuBuilder` fixtures (title/buttons/ids/slider/field/check/condition/nesting) | wire bytes equal `ui_node_wire.hex`; decode on a second process equals the original tree; `MenuResult` round-trips through the relay struct with caps enforced |
| `ui manifest` | `dialogs_manifest.json` + `styles_manifest.json` | schema valid, names unique, every `Styles` field present exactly once, pause flags match the §3.4 table, every scene file exists on disk |
| `ui hud-text` | fixture `GameState`/`Rules` values | status/objective text equals golden across waves 0/1/win, mission/objective overrides, waiting timer, attack mode |

These are Godot-free (the plans' `mind-core::ui` module), so they run in CI with `cargo run -p mind-headless -- ui text` etc.; exit codes follow plan 00 (0/1/2).

### 7c. MCP playtest scenario — `ui_sweep` (concrete)

Preconditions: `tools/build.sh`; `godot_health check`; if `BRIDGE_NOT_CONNECTED`, launch the editor per the repo skill (`nohup godot4 --editor --path /mnt/c/Users/Clinton/g/code_examples/mindustry-godot/client >/tmp/mind-editor.log 2>&1 &`), wait ~20 s, `godot_instance list`.

1. `godot_editor_edit open_scene res://scenes/spine.tscn`; `godot_game play` with `scene: "res://scenes/spine.tscn"`.
2. Pid-stamp: `godot_exec eval {"code": "return {\"pid\": OS.get_process_id(), \"dialog\": str(MindUi.dialog_stack())}"}` — record pid; compare with `godot_game instances`.
3. **Menu sweep (clicks).** For each `menu_openable` entry in `client/ui/dialogs_manifest.json`, in menu state: eval the button rect `get_node(<MenuFragment button path>).get_global_rect()`, `godot_input mouse_button` left click at its center, then assert `MindUi.is_dialog_shown("<name>")` and `get_node("/root/Spine/Ui/UiRoot/DialogLayer/<Node>").visible`; `godot_screenshot game` saved as `build/mcp/ui/<name>.png`; close via `Escape`/Back button and assert hidden. A capture is failed if the screenshot's dialog rect is uniform (read the PNG via `Image.load_png_from_buffer` in a helper eval; non-blank check).
4. **Full-catalogue sweep (API).** For every remaining dialog, `godot_exec call /root/MindUi open_dialog ["<name>", "<ctx_json>"]` with the manifest's `args` filled from the plan-00 inspector (e.g. `game_over: {"winner": 1}`, `content: {"content": "copper-wall"}`, `full_text: {"title":"t","text":"t"}`); assert visible + screenshot + close. This covers dialogs with no menu path and deps not yet on disk.
5. **Text input round trip.** `godot_exec eval` calls `MindUi.show_text_input("@ping.text", "@ping.text", 32, "", false, false)` capturing the returned signal; `godot_input text "hello"` into the focused field (eval focus path); press OK; assert the callback emitted `"hello"`; repeat with empty + `allow_empty=false` and assert OK disabled.
6. **Pause governor.** In game state, open `settings` (pause=true) via `open_dialog`; assert `SimHost.is_paused() == true`; close; assert `false` if it was not paused before; repeat with the game already paused and assert it stays paused.
7. **HUD numbers vs inspector.** Load `spine_place_break`, `MindSimHost.step(600)`; read `get_state_json()` → `wave`; assert `MindHud.wave == wave` and the status label `/root/Spine/Ui/UiRoot/HudGroup/HudFragment/WavesStack/WavesMain/.../status` text contains the wave formatted per `wave`/`wave.cap` keys; set `state.wavetime` via a fixture scenario and assert the waiting text matches `MindHeadless ui hud-text` golden.
8. **Toasts/announce/unlock.** `godot_exec call /root/MindUi announce ["hello", 3.0]`, `toast ["hi", "ok"]`, `unlock(["copper-wall"])`; assert the overlay nodes exist and fade within the documented duration; screenshots.
9. **Console/chat.** `godot_exec` toggles the console; type a registered command; assert the echoed result line; open chat in-game, type a message, assert local echo and (when plan 21 is present) server round trip.
10. **Minimap interaction.** Right-click the minimap widget center; assert the camera center changed toward the corresponding world position (`MindCamera2D` eval); scroll; assert zoom bounds clamp 0.25–10.
11. **Mobile preview.** Restart with `--mobile-preview` (plan 22 hook): assert `MindUi.is_mobile()`, mobile menu build (`MobileBar`), `MobileButton` layout; screenshot.
12. **Logs/teardown.** `godot_log errors` empty; `godot_game stop`.

`tools/mcp-smoke.sh` gains a `--ui` mode automating steps 1–7 and 12; the full sweep runs in plan 23's nightly catalog.

### 7d. Performance budget + measurement

| Metric | Budget | Measurement |
|---|---|---|
| UI frame CPU (HUD visible, no dialog, 1080p, after 60 s) | p95 ≤ 1.0 ms | `godot_profiler series` + `Performance.get_monitor(TIME_PROCESS)` delta against a UI-hidden baseline |
| Dialog open → visible, cached instance | p95 ≤ 8 ms | micro-bench in `MindUi.open_dialog` (`Time.get_ticks_usec` around show/sort), N=200 via eval loop |
| Dialog cold instantiate (largest: Planet/Research) | ≤ 80 ms | same bench, first open after `UiRoot` boot |
| Full catalogue sweep (open+close each of ~40, no scene churn) | stable `MEMORY_STATIC` (±2 MB) and no orphan nodes (`get_node_count`) | eval loop + profiler before/after |
| `MindTable` layout rebuild (500 cells, 3 levels) | ≤ 4 ms | GDScript bench `MindTable.bench(500)` exposed for MCP |
| `render_markup` + `format_icons` | ≤ 10 µs per 512-char string | `criterion` bench `ui_text_format` |
| DSL parse / writer / wire round-trip | ≤ 1 µs per node | `criterion` `ui_dsl_parse`, `ui_node_wire` |
| Theme build at `AssetsReadyEvent` | ≤ 120 ms | `UiRoot.theme_build_ms` property asserted by 03's assets-ready probe |
| Theme + UI textures resident | ≤ 4 MB metadata (excl. atlas pages) | `assets boot --dump` UI section + profiler |
| HUD binding churn | zero Rust allocations/frame; GDScript `text` assignments only on value change; 60 s soak with no GC spikes | alloc-audit (`mind-headless` flag) + profiler series |

Regressions: CI warns above +20%, fails above +50% (mirrors plan 00 §7d). Baselines committed under `client/rust/mind-core/tests/goldens/ui/perf.json` + `build/mcp/ui/baseline.json`.

### 7e. Exit criteria checklist

- [ ] `ui::*` Rust tests pass headless; no Godot/network needed.
- [ ] Java-oracle goldens captured and committed; tests compare exactly.
- [ ] `mind-headless ui text|dsl|menu-tree|manifest|hud-text` all green with committed goldens.
- [ ] `dialogs_manifest.json` covers all 39 `ui/dialogs/*` + editor/logic shells; `UiRoot` registers all without error; headless GDScript smoke asserts all scenes load.
- [ ] `styles_manifest.json` complete; theme builds from it; `UiStyleLookup` resolves every DSL `style:` used in fixtures.
- [ ] Every prompt helper implemented and exercised by MCP; text-input desktop + mobile path verified.
- [ ] Pause governor verified for all `pause: true` dialogs (open/close, pre-paused, network-active no-op).
- [ ] HUD parity: every HUD number/label asserted against the state inspector or a headless golden in `ui_sweep`.
- [ ] Minimap widget pan/zoom/toggle verified against `MindCamera2D`.
- [ ] MSUI: parse→write→parse round-trip + wire bytes equal Java layout; `menuBuilder` show/update/hide/result verified over the plan-21 relay (or fixture transport until 21 lands).
- [ ] File chooser: native + fallback both exercised; save/load round trip through `SaveDialog`/`LoadDialog` (04 §7c step 12).
- [ ] Mobile preview run: menu/HUD mobile branches, safe-area gutters, touch targets, virtual keyboard.
- [ ] Perf budgets §7d met; baselines committed.
- [ ] `tools/ci.sh` green including the UI headless scenarios; GPL headers on every new file; no `.cs` additions.
- [ ] Changelog updated with evidence paths (tests, dumps, screenshots).

## 8. Risks & open decisions

Each item has the default this plan proceeds with. Items marked **NEEDS USER DECISION** are load-bearing and not covered by `HIGH_LEVEL_PLAN.md`; execution continues on the stated default unless the user overrides.

| # | Decision / risk | Default being planned against | Needs user? |
|---|---|---|---|
| OD-UI1 | scene2d `Table`/`Cell` → Godot mapping strategy | **Port the layout algorithm to a GDScript `MindTable`/`MindCell`** (`Container` subclass) and keep a fluent cell API; dialogs are `.tscn` + GDScript. Alternative: remap to Godot `GridContainer`/`VBoxContainer` and restructure every dialog (weaker parity for `colspan`/`uniform`/dynamic rebuild). | **NEEDS USER DECISION** |
| OD-UI2 | Server-menu transport encoding | **Port MSUI 1:1 and ship `ui_node` bytes `format: 1`** (UiKey ordinals frozen) inside plan 21 relay rows; alternative is a JSON tree. Byte format keeps Java oracle tests and compactness; since no Java peers exist it is ours to version. | **NEEDS USER DECISION** |
| OD-UI3 | Content-icon rendering in text | **BBCode translation + `RichTextEffect` `[icon]` + PUA codepoint normalization** (no font-glyph injection); Godot has no supported runtime glyph injection. Plan 03 R8 handshake is this contract. | **NEEDS USER DECISION** |
| OD-UI4 | Dialog authoring model | **One `.tscn` + `.gd` per dialog, static layout in the scene, dynamic content in `shown`/`on_resize` code** (mirrors upstream rebuild pattern); alternative is fully code-built dialogs. | no (follows OD-UI1) |
| R1 | MindTable layout fidelity (grow/expand/pref/min/max/uniform/colspan interactions) | Port scene2d `Table.computeSize/layout` semantics with a 20-case golden matrix; fall back to scene-level explicit sizes where a dialog is simpler. | Tracked |
| R2 | Markup translation edge cases (`[]` reset, nested tags, `#` colors, `[` literals, unknown `:tokens:`, PUA collisions) | Golden corpus from the Java oracle + fuzz round-trip; every mismatch is a bug against the oracle. | Tracked |
| R3 | `RichTextLabel` per-frame cost with many HUD labels | Change-cached `MindLabel`; budget §7d; if exceeded, batch labels into a single `RichTextLabel` per region. | Tracked |
| R4 | Mobile safe areas/keyboards differ from Android/Arc | `DisplayServer.get_display_safe_area` + `uiEdgePadding`; MCP mobile-preview verifies; plan 22 owns final device bring-up. | Tracked |
| R5 | Dialogs depending on unwritten 12/13 | View-layer stubs with the §3.11 names; shells open with fixture data; milestones M5/M6 marked blocked-by-12 until they land; no silent divergence. | Tracked (orchestrator) |
| R6 | MCP click coordinates with Godot content scale/window stretch | Resolve via `get_global_rect()` at runtime (never hardcoded); pid-stamp every eval; document in the repo playtest skill. | Tracked |
| R7 | `Styles` runtime theme build cost / ninepatch margins from atlas | Build once at `AssetsReadyEvent`; budget 120 ms; margins from 03's `splits`; `.9.png` handshake test in 03 M7. | Tracked |
| R8 | Modded UI (overlay images, `dp-` bundles, DSL menus from mods) | Use plan 03 overlay hooks and 20's bundle merge; `MenuDialog` image map re-resolves on overlay events. | Tracked |
| R9 | Server menu spam / oversized trees (malicious or buggy server) | Enforce caps (nodes, depth, string lengths mirroring `MenuResult` caps) in `tree_builder`; reject with a logged warning; text input bounded by `maxLength`. | Tracked |
| R10 | In-game console no longer executes JS | Line-command interpreter registered by Rust (OD1); document behavior change in `THIRD_PARTY_NOTICES`/changelog if any bundle text references JS injection. | Tracked (OD1) |
| R11 | `Vars.mobile` fidelity on desktop touchscreens | Runtime detection + `--mobile-preview`; UI branches ported exactly; plan 22 decides shipping defaults. | no |

## 9. References

### Mindustry sources read
- `core/src/mindustry/core/UI.java` (753 lines); `core/GameState.java`.
- `ui/AGENTS.md`, `core/AGENTS.md`, `core/assets/AGENTS.md`, `input/AGENTS.md`, `audio/AGENTS.md` (read in full).
- `ui/Styles.java`, `ui/Fonts.java`, `ui/Bar.java`, `ui/BorderImage.java`, `ui/WarningBar.java`, `ui/GridImage.java`, `ui/MobileButton.java`, `ui/ReqImage.java`, `ui/MultiReqImage.java`, `ui/Links.java`, `ui/IntFormat.java`, `ui/ItemsDisplay.java`, `ui/CoreItemsDisplay.java`, `ui/Minimap.java`, `ui/FileChooser.java`, `ui/Menus.java`, `ui/Displayable.java`, `ui/Elems.java`.
- `ui/builder/{UiBuilder,MenuBuilder,MenuResult,UiDslParser,UiDslWriter,UiTreeBuilder,UiKey,UiStyleLookup,UiHotReload}.java`.
- `ui/dialogs/BaseDialog.java`, `SettingsMenuDialog.java`, `ContentInfoDialog.java` (read in full); all other `ui/dialogs/*.java` enumerated and skimmed for fields/APIs.
- `ui/fragments/{HudFragment,MenuFragment,MinimapFragment}.java` (read in full); all other `ui/fragments/*.java` skimmed.
- `ui/layout/*.java` (enumerated); `editor/MapEditorDialog.java`, `logic/LogicDialog.java`, `logic/LCanvas.java`, `world/meta/StatValues.java` (interface skim).
- `tests/src/test/java/ApplicationTests.java`, `DataAssetTests.java`, `tests/AGENTS.md` (confirm no UI tests).

### Plan set
- `HIGH_LEVEL_PLAN.md` (§0 D1–D9, §2.1–2.4, §3, §4 template, §6.5 LF, §7, §8, §9, §10).
- `PRELIMINARY_PLAN.md` (history).
- `00_FOUNDATION_IMPLEMENTATION_PLAN.md` (§3.4 project reset, §3.5 spine + node map, §3.6 events, §3.10 extension contract, §7c MCP).
- `02_CONTENT_IMPLEMENTATION_PLAN.md` (§3.6 interfaces, §3.7 integration, §3.8 gotchas).
- `03_ASSETS_IMPLEMENTATION_PLAN.md` (§3.6 fonts, §3.7 icons, §3.9 bundle/IntFormat, §7 R8 handshake).
- `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` (§3.7 settings, §3.11 Godot touchpoints, §7c save/load UI note).
- `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (§3.3 `ClientHooks`, §3.4 schedule, §3.7 events, §3.13 Godot surfaces).
- `06`/`07`/`08`/`09`/`10`/`11` (read models, `ConfigUiSpec`, hover/payload/status/command data).
- `01_PLATFORM_STDB_IMPLEMENTATION_PLAN.md` (§3.2–3.9 connector/waves/relay for Join/reconnect/menus).

### External
- Godot 4.7 docs: `RichTextLabel`/`RichTextEffect` BBcode, `Control`/`Container` layout, `Theme`/type variations, `DisplayServer.file_dialog_show`/`virtual_keyboard_show`, `DisplayServer.get_display_safe_area`.
- `client/scenes/spine.tscn`, `client/ui/state_inspector.gd` (plan-00 rig as built).

## Changelog

- 2026-10-01 — Draft v1 written (not started). No evidence yet; M0 entry to be appended when execution begins.
