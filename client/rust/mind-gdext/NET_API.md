# `MindNet` API (plans 14 / 22)

`/root/MindNet` is the plan-21 root node (`mind-gdext/src/net/mod.rs`,
`#[class(base=Node)]`). It owns the `MatchSession`, the `CommandSender`, the
relay runtime, the snapshot driver and the remote-player-state mirror. Plans 14
(Join/Host/Chat/Admin UI) and 22 (dedicated server console) call these `#[func]`s;
the surface is append-only. All calls are no-ops returning `false`/`""`/`0` when
`StdbMode::Offline`.

## Match lifecycle

| Method | Returns | Notes |
|---|---|---|
| `create_match(map_id, seed, mode, visibility, max_players, rules_json)` | `i64` match id (0 = sent) | Host; id arrives via `my_match`. |
| `join_match(match_id, password)` | `bool` | Sends the reducer; readiness follows `my_match_members`. |
| `leave_match()` | `bool` | Host leaving ends the match. |
| `start_match(force)` | `bool` | Host only; `force` skips the all-ready gate. |
| `set_ready(bool)` | `bool` | Lobby readiness. |
| `session_state()` | `String` | `offline`/`browsing`/`in_lobby`/`loading`/`in_game`/`snapshot_sync`/`reconnecting`. |

## Relay / commands

| Method | Returns | Notes |
|---|---|---|
| `send_command_json(kind_json)` | `bool` | Dev/test: decode a `CommandKind` JSON and emit it. |
| `last_applied_command_id()` | `i64` | Applied watermark. |
| `relay_queue_depth()` | `i64` | Pending foreign rows. |
| `relay_order_error()` | `String` | Last `CommandStream` gap/dup (empty = none). |
| `get_match_state_json()` | `String` | Host `match_state` mirror. |
| `get_members_json()` | `String` | `my_match_members` mirror. |

## Player state / checksums / snapshots

| Method | Returns | Notes |
|---|---|---|
| `get_remote_player_state(identity_hex)` | `Dictionary` | LWW puppet target for 16. |
| `get_remote_players_json()` | `String` | All remote rows. |
| `checksum_report()` | `Dictionary` | Own + peer checksums at the current watermark. |
| `request_snapshot()` | `bool` | Asks the host for a `Dynamic` snapshot. |
| `snapshot_progress()` | `f32` | `0.0..=1.0` chunk download progress. |

## Chat / UI / admin / dev

| Method | Returns | Notes |
|---|---|---|
| `send_chat(text)` | `bool` | `All` channel; server applies filters/rate. |
| `send_ui_result(kind, payload)` | `bool` | Opaque `Custom` relay command (14 menus); ≤ 1 KiB. |
| `authority()` | `String` | `relay` (D2 default). |
| `set_authority_mode(mode)` | `bool` | `relay` accepted; `authoritative` returns `false` (deferred sim). |
| `dev_inject_divergence()` | `bool` | Debug-only desync hook (currently no-op in release). |

## Plan-22 dedicated server contract

The dedicated server boots via `mind-headless serve --stdb-host <uri> --db <name>
--admin-token-file <path> --match-config <json>` (see
`client/rust/mind-headless/src/server/dedicated.rs`). It authenticates with the
service token (never a player token), parses the match config (`map_id`,
`map_seed`, `mode`, `visibility`, `max_players`, `map_rotation`, `auto_pause`),
and hosts the first map; map rotation is driven by `next_map`. The admin console
commands map onto the `admin_*` reducers (see plan 21 §4).

## Surfaces owned by other plans

- 14 owns all Join/Host/Chat/Admin dialog data and the `ui_node` payload bytes.
- 22 owns the process entry, console/socket and export preset; it calls the
  reducers/`MindNet` methods above.
- 16/17 own remote-state interpolation and FX; this plan only publishes the
  `match_player_state` mirror and never relays effects.
