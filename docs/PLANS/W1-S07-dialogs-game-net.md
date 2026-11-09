<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W1-S07 — `batch_b.rs` (3337), `batch_h.rs` (2763), `misc.rs` (1152) → `game/`, `net/`, and three shared leaves

**Wave:** W1. **Needs:** F2 merged. **Procedure:** [W1-DIALOGS-COMMON](W1-DIALOGS-COMMON.md).
**Owns:**
- `ui/dialogs/{batch_b,batch_h,misc}.rs`;
- `ui/dialogs/{game,net}/**` (single W1 owner, so files may be added);
- the leaves `pickers/list_select.rs`, `campaign/campaign_rules.rs`, `campaign/campaign_complete.rs`.

**Effort:** medium-large.

Line numbers are at `291e30a8`. Check every id against the real `dispatch` fns before you move.

## `batch_b.rs`

| Lines | Item | Id | Destination | Java |
|---|---|---|---|---|
| 54–404 | shared: `defer_action` 67, `defer_dialog`/`with_dialog`/`dialog_press`/`press`, `open_named_dialog` 123, `ui_show_info`/`ui_show_confirm` 137/148, `campaign_call` 159, `dialog_node` 220, variant/dict/json 234–330, `format_millis` 384 | – | **new** `game/util_b.rs` (add `pub mod util_b;` to `game/mod.rs`) | – |
| 405–944 | `PausedDialog` | `paused` | `game/paused.rs` | `ui/dialogs/PausedDialog.java` |
| 945–1624 | `GameOverDialog` (incl. `difficulty_guide_should_show` 1425) | `restart` (**sic**: keep the id) | `game/game_over.rs` | `ui/dialogs/GameOverDialog.java` |
| 1625–1824 | join/host plumbing: `ui_call` 1657, `settings_int` 1694, `stored_player_name` 1717, `push_player_identity` 1751, `close_named_dialog` 1768, `emit_dialog_signal` 1790 | – | `net/util.rs` (U) | – |
| 1825–2588 | `JoinDialogController` | `join` | `net/join/mod.rs` (replace F2's stub) | `ui/dialogs/JoinDialog.java` |
| 2589–3239 | version gate: `JoinVersion` 2599, `JoinSafeConnect` 2702 | – | `net/join/version_gate.rs` (new, inside your subdir) | `ui/dialogs/JoinDialog.java` (version check), `net/BeControl.java` if cited |
| 3240–3337 | tests: `build_compare`, `version_gate`, `safe_connect` → `version_gate.rs`; `difficulty_guide…` → `game_over.rs` | | | |

The `batch_b.rs` header cites `HostDialog`/`LoadDialog`/`SaveDialog`, which live in `batch_h`
(stale). The new headers cite correctly.

## `batch_h.rs`

| Lines | Item | Id | Destination | Java |
|---|---|---|---|---|
| 81–465 | shared: `defer_action` 93, `ui_call` 106, `install_ui_signal` 413, `clear_table_nodes` 436, … | – | **new** `net/util_h.rs` (add `pub mod util_h;` to `net/mod.rs`) | – |
| 466–895 | `HostDialogController` | `host` | `net/host.rs` | `ui/dialogs/HostDialog.java` |
| 896–2280 | `LoadDialogController` + `LoadKind` 924 | `load`, `save` (`save` → `LoadDialogController::save()`) | `game/load.rs` (about 1385 lines, within the ceiling) | `ui/dialogs/LoadDialog.java`, `SaveDialog.java` |
| 2281–2725 | campaign rules dialog | `campaign_rules` | `campaign/campaign_rules.rs` | `ui/dialogs/CampaignRulesDialog.java` |
| 2726–2763 | test `save_file_check` → `game/load.rs` | | | |

## `misc.rs`

| Lines | Item | Id | Destination | Java |
|---|---|---|---|---|
| 28–92 | preamble helpers (`rich` 77; `dispatch` 66 holds the arms) | – | **new** `net/util_misc.rs` (add `pub mod util_misc;` to `net/mod.rs`) | – |
| 93–208 | `ListSelectDialog` | `list_select` | `pickers/list_select.rs` | none (generic picker, like `ui.showSelect`) |
| 209–377, 411–607 | `RosterEntry`, `RosterKind`, `RosterState` (231–377), and the roster fns shared by Bans and Admins (411–607) | – | `net/roster.rs` (U) | – |
| 378–393 | `BansDialog` | `bans` | `net/bans.rs` | `ui/dialogs/BansDialog.java` |
| 394–410 | `AdminsDialog` | `admins` | `net/admins.rs` | `ui/dialogs/AdminsDialog.java` |
| 605–619 | `type DeferredAction` + `defer_action` 608 | – | `net/util_misc.rs` | – |
| 620–887 | `TraceFields` 633–707, `pub(crate) fn trace_context_json` 708, `TraceDialog` 715 | `traces` | `net/trace.rs` | `ui/dialogs/TraceDialog.java` |
| 888–1003 | `CampaignCompleteDialog` | `campaign_complete` | `campaign/campaign_complete.rs` | `ui/dialogs/CampaignCompleteDialog.java` |
| 1004–1152 | tests: `string_array…` → `list_select.rs`; `roster_*` → `roster.rs`; `trace_*`, `trace_context_*`, `roster_trace_maps…` → `trace.rs`; `playtime…` → `campaign_complete.rs`; `pane_colors…` → the file of its owner | | | |

The `misc.rs` shim keeps `pub(crate) use super::net::trace::trace_context_json;`, because
`ui/fragments/player_list.rs:34` imports `crate::ui::dialogs::misc::trace_context_json`.
`misc.rs` had no upstream citation; the new headers fix that.

## Notes

- `batch_b` and `batch_h` duplicate about 15 helpers (`settings_int`, `stored_player_name`,
  `push_player_identity`, `close_named_dialog`, `emit_dialog_signal`, `format_millis`, `ui_show_*`).
  **Do not merge them in W1**; H05b does it.
- Join and host are networking UI. They call into `MindNet` by string. Every `call("…")` string
  stays as it is.

## Done when

The three files are shims, the checks in W1-DIALOGS-COMMON pass, and `moves/S07.tsv` and
`reports/S07.md` are written.
