# UI layer conventions (plan 14, native-Rust cutover 2026-11-02)

The UI is native Rust. `client/ui/` now holds only the two UI manifests; the
former GDScript tree was deleted at the cutover. Behaviour, layout code and the
`Mind*` classes live in `client/rust/mind-gdext/src/ui/` (contract:
[`PORTING.md`](../rust/mind-gdext/src/ui/PORTING.md), inventory:
[`PORT-STATUS.md`](../rust/mind-gdext/src/ui/PORT-STATUS.md)); the Godot-facing
scene graph is `client/scenes/ui/`.

**All non-addon code is `.rs` or `.tscn`.** There is no hand-written GDScript
outside `client/addons/`: no `.gd` file, no `.gd.uid` sidecar, and no
`script = ExtResource(...)` on a UI scene. Do not add one; extend the native
class or the scene instead.

## Hard rules

- **Static layout in scenes, dynamic content in Rust.** Every dialog is a
  `.tscn` (inheriting `scenes/ui/dialogs/mind_dialog_base.tscn`) whose root
  names the native `MindDialog` class for that manifest key. Fixed
  structure/sections live in the scene; rows built from data (lists, grids,
  per-entry cards) are created by the Rust controller and carry a
  `# code-instantiated: <specific reason>` comment (HLP §6.6).
- **No game rules and no sim mutation in the UI layer.** Dialogs and fragments
  bind to `MindUi`/`MindHud` properties, signals and JSON endpoints only.
  Numbers originate from Rust read models, and scenes never touch sim state.
- **User-visible strings go through bundle keys** (`kit::t("@key")`); debug-only
  strings are the exception.
- **Manifests are the ABI.** `dialogs_manifest.json` and `styles_manifest.json`
  are validated by `mind-core::ui::manifest` and by `MindUiRoot`. New
  dialogs/fragments must be added to `dialogs_manifest.json`, to the static
  instance in `scenes/ui/ui_root.tscn`, and to the
  `EXPECTED_M*_DIALOGS`/fragment lists in `manifest.rs`.

## Layout

```
ui/
  dialogs_manifest.json      # dialog/fragment inventory + prompt keys
  styles_manifest.json       # themed style/drawable names (parity ABI)

scenes/ui/
  ui_root.tscn               # layer groups + static instance of every manifest node
  dialogs/                   # 48 manifest dialogs + mind_dialog_base.tscn
  fragments/                 # 14 Mind*Fragment scenes
  widgets/mind_minimap.tscn  # the only scene-backed widget; the rest are Rust-built

rust/mind-gdext/src/ui/      # every Mind* UI class and DialogController
rust/mind-core/src/ui/       # Godot-free read models + manifest validation
```

## Campaign read models (M5)

`MindUi.campaign_views()` returns the JSON projection built by
`mind-core::ui::campaign::CampaignViews` from the plan-12 `Campaign`/`Planet`/
`Sector`/`Schematics`/`TechStore` runtime. The dialog controllers receive one
section (`planets`, `sectors`, `research`, `schematics`, `loadouts`, `rules`,
`complete`, `maps`). A live campaign binding replaces the deterministic fixture
snapshot without changing the controllers (plan 12/21 seam).

`MindUi.block_catalog_json()` returns the placement-palette projection built by
`mind-core::ui::campaign::BlockCatalogView`. In a campaign it prefers
`MindCampaign.block_catalog_json()`, which filters by the settings-backed unlock
store (`PlacementFragment.getUnlockedByCategory`: locked tech-gated blocks and
empty categories are hidden); the campaign-less fallback filters through the
persisted settings file instead of serving the unfiltered inventory.

## Chat / console / player list / file chooser (M7)

- `MindUi.chat_send(text, mode)` validates and emits `chat_message`; transport
  is plan 21's relay.
- `MindUi.console_execute(line)` dispatches through the Rust
  `mind-core::ui::console::ConsoleRegistry` (Rhino JS dropped, OD1).
- `MindUi.player_list_json()` / `MindUi.player_action(...)` are the documented
  plan-21 relay seam; the shell never fakes a server action.
- `DisplayServer.file_dialog_show` (plan 22) is native-first; the native
  `FileChooserDialog` controller is the fallback browser.
