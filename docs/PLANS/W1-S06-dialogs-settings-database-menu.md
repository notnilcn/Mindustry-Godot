<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W1-S06 — `batch_a.rs` (3996 lines) + `simple.rs` (657) → `settings/`, `database/`, `menu/`

**Wave:** W1. **Needs:** F2 merged. **Procedure:** [W1-DIALOGS-COMMON](W1-DIALOGS-COMMON.md).
**Owns:** `ui/dialogs/batch_a.rs`, `ui/dialogs/simple.rs`, `ui/dialogs/{settings,database,menu}/**`. This
package is the single W1 owner of these three groups, so it may add files to them.
**Effort:** medium.

Line numbers are at `291e30a8`. Java paths are relative to `core/src/mindustry/`.

## `batch_a.rs`

| Lines | Item | Id | Destination | Java |
|---|---|---|---|---|
| 62–305 | shared section: request/meta helpers 72–121, variant/dict/json 123–229, `load_categories`/`load_rows` 230–264, `bundle_or` 265, **`pub fn open_content_info` 274–293**, `flat_box` 300 | – | `settings/util.rs` (U) | – |
| 306–1296 | `SettingsController` | `settings` | `settings/settings_menu.rs` | `ui/dialogs/SettingsMenuDialog.java` |
| 1297–1611 | `LanguageController` | `language` | `settings/language.rs` | `ui/dialogs/LanguageDialog.java` |
| 1612–2240 | `ControlsController` | `controls` | `settings/keybind.rs` | `ui/dialogs/KeybindDialog.java` |
| 2241–2650 | `EffectsController` | `effects` | `database/effects.rs` | `ui/dialogs/EffectsDialog.java` |
| 2651–3477 | `DatabaseController` | `database` | `database/database_dialog.rs` | `ui/dialogs/DatabaseDialog.java` |
| 3478–3962 | `ContentController` | `content` | `database/content_info.rs` | `ui/dialogs/ContentInfoDialog.java` |
| 3963–3996 | tests: `effects_preview_*` → `database/effects.rs`; `settings_pane_inset` → `settings/settings_menu.rs` | | | |

Check the ids against the real `dispatch` in `batch_a.rs`; the table's ids are from the analysis.

- `open_content_info` is `pub` and is called from `batch_a` itself (~3389, ~3865, now in
  `database/database_dialog.rs` and `database/content_info.rs`) and from `batch_g.rs` ~2255. Keep it `pub` in
  `settings/util.rs`. The `batch_a.rs` shim re-exports it:
  `pub use super::settings::util::open_content_info;`.
- `settings/util.rs` helpers are used by `database/` too, so use `pub(in crate::ui::dialogs)`.

## `simple.rs`

| Lines | Item | Id | Destination | Java |
|---|---|---|---|---|
| 1–33 | header, imports, `dispatch` 19 | – | (the arms move to the leaves) | – |
| 34–223 | `AboutDialog` | `about` | `menu/about.rs` | `ui/dialogs/AboutDialog.java` |
| 224–312 | `CreditsDialog` | `credits` | `menu/credits.rs` | none (port of `client/ui/dialogs/credits_dialog.gd`) |
| 313–545 | `DiscordDialog` | `discord` | `menu/discord.rs` | `ui/dialogs/DiscordDialog.java` |
| 546–624 | `FullTextDialog` | `full_text` | `menu/full_text.rs` | `ui/dialogs/FullTextDialog.java` |
| 625–657 | tests: `capitalize`/`link_table` → `about.rs`; `discord_url` → `discord.rs` | | | |

If `simple.rs` has helpers shared by more than one menu dialog, add `menu/util.rs` and a
`pub mod util;` line in `menu/mod.rs` (allowed, single owner). `simple.rs` had no upstream
citation (02-FINDINGS #12); the new headers fix that.

## Done when

Both files are shims. Every id answers from its new leaf. The checks in W1-DIALOGS-COMMON pass.
`moves/S06.tsv` and `reports/S06.md` are written.
