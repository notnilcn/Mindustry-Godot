# UI layer conventions (plan 14)

This directory (`client/ui/`) and `client/scenes/ui/` hold the Godot-facing UI.
The Rust behaviour/format layer lives in `mind-core::ui` (Godot-free) and the
`MindUi`/`MindHud` autoloads in `mind-gdext::ui`.

## Hard rules

- **Static layout in scenes, dynamic content in code.** Every dialog is a
  `.tscn` (inheriting `scenes/ui/dialogs/mind_dialog_base.tscn`) plus a `.gd`
  script extending `MindDialog`. Fixed structure/sections live in the scene;
  rows built from data (lists, grids, per-entry cards) are created in code and
  carry a `# code-instantiated: <specific reason>` comment (HLP §6.6).
- **No game rules and no sim reads in GDScript.** Dialogs bind to `MindUi`/
  `MindHud` properties, signals and JSON endpoints only. Numbers originate from
  Rust read models.
- **User-visible strings go through bundle keys** (`_t("@key")`); debug-only
  strings are the exception.
- **Manifests are the ABI.** `dialogs_manifest.json` and `styles_manifest.json`
  are validated by `mind-core::ui::manifest` and by `UiRoot._ready()`. New
  dialogs/fragments must be added to `dialogs_manifest.json` and to the
  `EXPECTED_M*_DIALOGS`/fragment lists in `manifest.rs`.

## Layout

```
ui/
  ui_root.gd               # layer groups, manifest instantiation, prompt overlays
  mind_dialog.gd           # BaseDialog equivalent + campaign read-model helpers
  mind_table.gd mind_cell.gd mind_stack.gd mind_scroll.gd
  mind_widgets.gd          # themed widget factories + tween helpers
  margins.gd               # MindUiMargins (safe-area gutters)
  theme/ text/ layout/ widgets/ dialogs/ fragments/
  dialogs_manifest.json styles_manifest.json
scenes/ui/                 # mirrors ui/ with .tscn files
```

## Campaign read models (M5)

`MindUi.campaign_views()` returns the JSON projection built by
`mind-core::ui::campaign::CampaignViews` from the plan-12 `Campaign`/`Planet`/
`Sector`/`Schematics`/`TechStore` runtime. `MindDialog.campaign_section(name)`
returns one section (`planets`, `sectors`, `research`, `schematics`, `loadouts`,
`rules`, `complete`, `maps`). A live campaign binding replaces the deterministic
fixture snapshot without changing the dialog scripts (plan 12/21 seam).

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
- `DisplayServer.file_dialog_show` (plan 22) is native-first;
  `FileChooserDialog` is the fallback browser.
