# AGENTS.md — mind-core/src/ui (Godot-free UI logic)

`mind-core::ui` is the Rust-side data/model half of the UI: the MSUI builder and DSL, text/HUD/stat
formatting, the campaign/chat/console/player-list/file-chooser/prompt view models, settings rows,
tree layout, the pause-governor state machine and the manifest validators that pin the UI ABI. It
links no Godot, tokio or network code and runs under plain `cargo test -p mind-core`. Crate-wide
rules are in [`mind-core/AGENTS.md`](../../AGENTS.md); the repo map is the root
[`AGENTS.md`](../../../../../AGENTS.md).

## Layout

| Path | Responsibility |
|---|---|
| `mod.rs` | Module list and the view-only boundary note. |
| `builder/` | Godot-free MSUI builder: typed tree + wire codec, DSL parser/writer, materialization, style lookup, server-menu host and relay payloads. |
| `builder/ui_key.rs` | Frozen `UiKey` ordinals and Java names; declaration order is the wire/DSL ABI. |
| `builder/ui_node.rs` | `UiNode`/`UiEntry`/`UiValue` and the versioned `UI_NODE_FORMAT = 1` byte codec. |
| `builder/dsl.rs`, `builder/dsl_writer.rs` | `parse`/`write` for the MSUI DSL (`key: value`, `row`, `node { }`, shorthand, `//` comments, `\n` escapes). |
| `builder/dsl_factory.rs` | Godot-free materialization (`FactoryTree`/`FactoryNode`) with resolved styles; `dump_json` headless golden. |
| `builder/tree_builder.rs` | `BuildContext` condition eval, id/image collection, `Materialized::fire_result` and the R9 tree caps. |
| `builder/menu_builder.rs`, `builder/menu_result.rs`, `builder/menu_host.rs` | `MenuBuilder`, `MenuResult`/`MenuValue`, and the `MenuHost` show/update/hide/choose state machine. |
| `builder/ui_relay.rs` | `format: 1` relay codecs (`MenuBuilderShow/Update`, `TextInput`, `InfoPopup`, `WorldLabel`, `WarningToast`, `PingMarker`) and `wire_fixtures`. |
| `builder/style_lookup.rs` | `StyleLookup`/`StyleKind` resolution plus `scan_style_references` over GDScript/`.tscn`. |
| `builder/hot_reload.rs` | `.msui` mtime debounce and error-line/source extraction. |
| `campaign.rs` | Campaign read models (`CampaignViews` and `PlanetView`/`SectorView`/`ResearchView`/`SchematicView`/`LoadoutView`/`CampaignRulesView`/`CampaignCompleteView`/`MapEntryView`) with `vanilla_fixture`/`empty`. |
| `chat.rs` | `ChatState`, `ChatMode`, `ChatSend`, `Ping` and `check_ping`. |
| `console.rs` | `ConsoleRegistry`/`ConsoleCommand`/`ConsoleEntry` line dispatch and `help_text`. |
| `content_stats/` | Content-info (`?`) stat model: `Stats`/`Stat`/`StatCat`/`StatUnit`/`Cell`, base block/item/liquid stats and the `logistics`/`production`/`turret_ammo`/`unit_blocks`/`unit_stats`/`values` bodies. |
| `database.rs` | Core Database view model (`DatabaseView` planet tabs + `databaseCategory`/`databaseTag` grid) with `empty`. |
| `display.rs` | `DisplayRow`/`HoverInfo` hover rows and the `Displayable` provider trait. |
| `file_chooser.rs` | `FileChooserParams` title/name/extension validation and `sanitize_filename`. |
| `hud_text.rs` | `HudStatus` and `status_text` plus the fps/ping/tps/memory label helpers. |
| `manifest.rs` | `DialogsManifest`/`StylesManifest` parsers + validators and the `EXPECTED_*` ABI tables. |
| `pause.rs` | `PauseGovernor`/`PauseAction`/`should_govern` reference-counted pause state machine. |
| `player_list.rs` | `PlayerListModel`/`PlayerSummary`/`PlayerAction` rows, sort/filter and the relay seam. |
| `prompts.rs` | `PromptKind`, `TextInputSpec`, `ConfirmSpec`, `PopupRegistry`, `AnnouncementTracker` and `PROMPT_HELPERS`. |
| `settings.rs` | Ordered `SettingRow`/`SettingsCategory` model (kind, default, range, formatting tag); values live in `io::settings`. |
| `stat_display.rs` | `StatDisplay`, `fix_value`, `ammo_stat`, `mult_stat` content-stat formatting. |
| `text.rs` | `format_icons`, `render_markup`, `format_time`, `format_amount`, `round_amount`, `color_tag`. |
| `tree_layout.rs` | Arena `LayoutNode` tree layout and research-tree placement (exact abego walker; do not simplify). |

## Boundaries

- This module owns Rust-side UI *data*: read models, MSUI builder/DSL, formatting, pause state,
  settings rows, tree layout, manifest JSON parsing and the `EXPECTED_*` validators.
- [`client/AGENTS.md`](../../../../AGENTS.md) owns the Godot layer: static `.tscn` instances
  (`ui_root.tscn`, dialogs/fragments/widgets), theme construction/verification in
  `MindThemeBuilder`, GDScript bindings and the `# code-instantiated:` rule. `mind-gdext::ui`
  (`MindUi`, `MindHud`) is the runtime bridge; its read-model JSON contract is
  [`client/ui/README.md`](../../../../ui/README.md).

## Key types

- Builder: `builder::ui_node::{UiNode, UiEntry, UiValue}`, `builder::ui_key::UiKey`,
  `builder::dsl::{parse, DslError}`, `builder::menu_host::{MenuHost, MenuSelection}`,
  `builder::menu_result::MenuResult`, `builder::ui_relay::{RelayCommand, wire_fixtures}`.
- Models: `campaign::CampaignViews`, `content_stats::Stats`, `hud_text::HudStatus`,
  `display::HoverInfo`, `player_list::PlayerListModel`, `file_chooser::FileChooserParams`,
  `prompts::TextInputSpec`, `settings::SettingRow`, `tree_layout::LayoutNode`.
- Formatting: `text::{render_markup, format_icons, format_amount}`, `stat_display::StatDisplay`.
- ABI: `manifest::EXPECTED_STYLES`, `manifest::EXPECTED_PROMPTS`, `manifest::EXPECTED_*_DIALOGS`
  (M3/M5/M7/logic/editor), `manifest::expected_pause`.

## Invariants

- **View-only.** The module computes view data and never feeds the simulation, saves, sync or checksum.
- **Manifests are the ABI.** `client/ui/dialogs_manifest.json` and `client/ui/styles_manifest.json`
  carry `format: 1`; `manifest.rs` rejects bad format, duplicate names, missing scenes/fragments,
  pause-flag mismatches and missing `Styles.*`/prompt entries.
- **Frozen encodings.** `UiKey` ordinals, `ui_node`/`MenuResult`/relay `format: 1` bytes and the
  golden `tests/goldens/ui/*` (`ui_keys.txt`, `relay_wire.hex`, `*.json`) are stable ABI.
- **Caps on server input.** `tree_builder::{MAX_NODES, MAX_DEPTH, MAX_STRING_LEN}` and
  `menu_result::{MAX_RESULT_LEN, MAX_VALUE_LEN, MAX_TOTAL_STRING_LEN, MAX_TOTAL_VALUES}` are enforced
  on construction/validation.
- **Strings stay localizable.** User-visible text is composed from bundle keys (`Bundle::get`/`format`)
  and rendered through `render_markup`/`format_icons`; no game rules live here.

## Rules

- A new dialog/fragment/style/prompt is not done until the manifest validators pass; the three-place
  checklist (static scene, manifest, `EXPECTED_*`) is in [`client/AGENTS.md`](../../../../AGENTS.md).
  `prompts::PROMPT_HELPERS` is the single ordered prompt list and must match the manifest `prompts[]`.
- Dialogs and `.tscn` files may reference only styles that `StyleLookup` resolves against the manifest.
- Keep user-visible strings in bundles; this tree composes text, it does not decide game rules.

## Verification

Run from `client/rust/`.

```bash
cargo test -p mind-core ui
cargo run -p mind-headless -- ui manifest --repo ../..
cargo run -p mind-headless -- ui campaign
cargo run -p mind-headless -- ui relay
```
