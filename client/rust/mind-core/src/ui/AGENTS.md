# AGENTS.md — mind-core/src/ui (Godot-free UI logic)

`mind-core::ui` is the Godot-free data, format and model half of the UI. It owns the MSUI builder and
DSL, text/HUD/stat formatting, the campaign/chat/console/player-list/file-chooser/prompt view models,
the pause-governor state machine and the manifest validators that pin the UI ABI. It links no Godot,
tokio or network code and runs under plain `cargo test -p mind-core`. Read the root
[`AGENTS.md`](../../../../../AGENTS.md) first; the crate map is `mind-core/AGENTS.md`
(`../../AGENTS.md`). The Godot-facing widget layer lives in `mind-gdext::ui` (`MindUi`, `MindHud`) plus
GDScript under `client/ui` and `client/scenes/ui`; the Rust↔GDScript contract is
[`client/ui/README.md`](../../../../ui/README.md).

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
| `display.rs` | `DisplayRow`/`HoverInfo` hover rows and the `Displayable` provider trait. |
| `file_chooser.rs` | `FileChooserParams` title/name/extension validation and `sanitize_filename`. |
| `hud_text.rs` | `HudStatus` and `status_text` plus the fps/ping/tps/memory label helpers. |
| `manifest.rs` | `DialogsManifest`/`StylesManifest` parsers + validators and the `EXPECTED_*` ABI tables. |
| `pause.rs` | `PauseGovernor`/`PauseAction`/`should_govern` reference-counted pause state machine. |
| `player_list.rs` | `PlayerListModel`/`PlayerSummary`/`PlayerAction` rows, sort/filter and the relay seam. |
| `prompts.rs` | `PromptKind`, `TextInputSpec`, `ConfirmSpec`, `PopupRegistry`, `AnnouncementTracker`. |
| `stat_display.rs` | `StatDisplay`, `fix_value`, `ammo_stat`, `mult_stat` content-stat formatting. |
| `text.rs` | `format_icons`, `render_markup`, `format_time`, `format_amount`, `round_amount`, `color_tag`. |

## Key types

- Builder: `builder::ui_node::{UiNode, UiEntry, UiValue}`, `builder::ui_key::UiKey`,
  `builder::dsl::{parse, DslError}`, `builder::menu_host::{MenuHost, MenuSelection}`,
  `builder::menu_result::MenuResult`, `builder::ui_relay::{RelayCommand, wire_fixtures}`.
- Models: `campaign::CampaignViews`, `hud_text::HudStatus`, `display::HoverInfo`,
  `player_list::PlayerListModel`, `file_chooser::FileChooserParams`, `prompts::TextInputSpec`.
- Formatting: `text::{render_markup, format_icons, format_amount}`, `stat_display::StatDisplay`.
- ABI: `manifest::EXPECTED_STYLES`, `manifest::EXPECTED_PROMPTS`, `manifest::EXPECTED_M3_DIALOGS`,
  `manifest::EXPECTED_M5_DIALOGS`, `manifest::EXPECTED_M7_DIALOGS`, `manifest::expected_pause`.

## Invariants

- **No Godot, tokio or network.** Everything is pure data/format/parse; bundles (`assets::bundle::Bundle`),
  icon tables (`assets::icons::Iconc`) and read models are injected by callers.
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

- The Godot widget layer is `mind-gdext::ui` plus GDScript in `client/ui` and `client/scenes/ui`; this
  crate exposes read-model JSON that GDScript consumes through `MindUi` endpoints such as
  `campaign_views()`, `chat_send(text, mode)`, `console_execute(line)`, `player_list_json()` and
  `player_action(...)`; `DisplayServer.file_dialog_show` is handled natively by `mind-gdext`.
- A new dialog or fragment must be added to both `ui/dialogs_manifest.json` and the matching
  `EXPECTED_*_DIALOGS`/fragment list; a new style to `styles_manifest.json` and `EXPECTED_STYLES`; a
  new prompt to `prompts::PROMPT_HELPERS`, `EXPECTED_PROMPTS` and the manifest `prompts[]`.
- Dialogs and `.tscn` files may reference only styles that `StyleLookup` resolves against the manifest.
- Keep `use godot`/`use tokio` out of this tree and avoid `unwrap`/`expect` on runtime data.

## Verification

```bash
cd client/rust
cargo test -p mind-core ui
cargo clippy -p mind-core --all-targets -- -D warnings
cargo run -p mind-headless -- ui manifest --repo ../..
cargo run -p mind-headless -- ui campaign
cargo run -p mind-headless -- ui relay
```
