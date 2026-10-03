# AGENTS.md — server/spacetimedb/ (module crate)

The `mindustry_godot` SpacetimeDB module: the match directory, membership, the
private ordered command relay and persistent state (identity, chat, moderation,
campaign, schematics, mods) for Mindustry-Godot. Reducers return no data, use
`ctx.sender()` as the only principal and perform cheap validation only; there is
no server-side simulation. Read the root [`AGENTS.md`](../../AGENTS.md) first,
then [`server/AGENTS.md`](../AGENTS.md).

## Layout

| Path | Responsibility |
|---|---|
| `lib.rs` | Crate root; declares `admin`, `campaign`, `chat`, `identity`, `main`, `mods`, `relay`. |
| `admin/mod.rs` | `server_config` singleton (`ServerConfig`, id 0), `admin_identity`, `player_ban`, `whitelist_entry`, server-only `admin_rate`, append-only `admin_action_log`; `AdminActionKind`, the `admin_*` reducers and the admin-gated views. |
| `campaign/mod.rs` | `campaign`/`sector_info`/`unlock`/`campaign_stats`/`schematic` tables with their CRUD reducers/views; `persist_from_command`. |
| `chat/mod.rs` | `ChatKind`, `match_chat`, server-only `chat_rate`, admin-managed `chat_filter`; `send_chat`, `sanitize_text`, `filter_mutes`, `rate_limit`. |
| `identity/` | `tables.rs` (`player`, `player_session`, `player_profile`, `client_settings`), `methods.rs` validators, `reducers.rs` (`set_username`, `create_profile`, `update_client_settings`), `views.rs` (`local_player`, `all_players`, ...). |
| `main/` | `global.rs` tunables, `seeds.rs` code seeds, `content_seed.rs` generated `VANILLA_CONTENT`, `lifecycle.rs` (`init`/`client_connected`/`client_disconnected`), `audit.rs` server-only `audit_log` + `audit`, `tables.rs` (`protocol_info`, `relay_config`, `AuditKind`). |
| `mods/mod.rs` | `match_mod`, `content_catalog`; `check_mods` and `content_name_allowed`. |
| `relay/` | Match directory/membership, the private `match_command` log, rate/session state, rules, plan snapshots, checksums, snapshots, UI events, LWW player state and the maintenance sweep; every relay reducer and view. |
| `Cargo.toml` | `cdylib` crate `mindustry_godot`, its own workspace, pinned to `spacetimedb = "=2.10.1"`. |
| `../spacetime.json` | Server/db defaults and the Rust generate target (`../client/rust/mind-stdb/src/module_bindings`). |

## Responsibilities

- **Directory and membership.** `create_match` inserts `relay_match`
  (`AuthorityMode::Relay`, `MatchStatus::Lobby`, host = creator) plus a host
  `relay_member` row; `join_match` runs the build/content/mod and
  `JoinGate` (ban -> whitelist -> cap -> password) checks; `leave_match` (host
  exit ends the match), `set_ready` and `start_match` move it to `Running`.
- **Command relay.** `send_match_command` and the `admin_*` emitters share
  `send_match_command_impl`: status/member/protocol checks, spectator and role
  gates from `command_role`/`spectator_forbidden`, `validate_kind` coarse caps,
  `persist_from_command`, `rules_after_command`, then `rate_allow` assigns a
  monotonic `sender_seq` and appends the `CommandKind` row to the private
  `match_command` log. `sender_command_state` records per-sender acceptance.
- **Chat.** `send_chat` sanitizes text, applies `chat_filter` mute patterns
  (three infractions set `chat_rate.muted_until` 60 s ahead), then the
  `server_config` rate window; `ChatKind::System` requires host/admin.
- **Moderation.** `admin_kick`/`admin_ban`/`admin_unban`, whitelist and admin
  grant reducers, `admin_set_config`, the chat-filter reducers and the ordered
  wave/team/tile emitters; each committed action appends `admin_action_log` and
  an `audit_log` line. Bans are identity-only (no IP/subnet bans).
- **Campaign/schematics.** Campaign writes require the campaign host or a global
  admin; schematics are owner-scoped. `RelayMatch.campaign_id` keys campaign
  rows, and `persist_from_command` writes `ResearchUnlock`, `SectorCapture` and
  `SaveSector` effects on the same ordered command; a match without
  `campaign_id` is a no-op.
- **Checksums/snapshots/plans/UI.** `publish_checksum` stores scoped versioned
  votes compared by `compare_at_command_id`; `publish_snapshot` and
  `request_snapshot` stream chunked host snapshots (newest per kind retained by
  `snapshots_to_delete`); `report_plan_snapshot` reassembles chunked plan groups
  by monotonic `group_id`; `publish_ui_event` stores host/admin UI events.
- **Maintenance.** `tick_maintenance` runs on the `maintenance_schedule`
  interval and prunes the command log behind the snapshot watermark, expired
  bans, disconnected members past `MEMBER_GRACE_SECS`, ended matches past
  `match_ttl_hours`, chat/UI/checksum/snapshot/plan retention, idle rate rows
  and old player sessions.

## Rules

- **No return data, single principal.** Reducers return `Result<(), String>` (or
  `()`); clients read rows/views. `ctx.sender()` is the only principal.
- **Cheap validation only.** Bounds, charset, length, rotation, finite floats,
  sequence monotonicity and rate windows - no tile occupancy, resources, unit
  ownership or collision (sim-side concerns).
- **Failures log, they do not audit.** A failed reducer rolls back its writes, so
  rejection paths use `log::warn!`; `audit` records committed actions.
- **Views use indexes.** Read-only view handles expose `find` + index accessors
  only; full-table views range-unbounded over a btree index with
  `(Bound::<T>::Unbounded, Bound::<T>::Unbounded)`. String-column filters pass
  `&str`, never `String`.
- **Generated bindings are never hand-edited.** Regenerate with `server/build.sh`
  (or `spacetime generate --lang rust --out-dir
  client/rust/mind-stdb/src/module_bindings --module-path server/spacetimedb -y`).
- **`content_catalog.name` is `#[unique]`**; the generated seed is deduped.

## Invariants

- **Server-only tables** (no `public`): `audit_log`, `match_command`,
  `command_rate`, `sender_command_state`, `match_kick`, `match_snapshot_request`,
  `match_snapshot_chunk`, `snapshot_rate`, `checksum_rate`,
  `match_player_state_rate`, `chat_rate`, `admin_rate`, `maintenance_schedule`.
  Clients reach them through member semijoin views (`my_match_commands`,
  `my_kick`, `my_match_snapshot_chunks`, `my_sender_command_state`, ...).
- **Member semijoin views.** `all_matches` lists public non-ended matches; the
  other match views (`my_matches`, `my_match_members`, `my_match_state`,
  `my_match_commands`, `my_match_chat`, `my_match_checksums`,
  `my_match_snapshots`, `my_match_snapshot_requests`, `my_match_ui_events`,
  `my_match_player_states`, `my_match_plans`) resolve the caller's active match
  through `relay_member` and never expose another match's rows. Admin views
  (`am_i_admin`, `all_bans`, `all_whitelist`, `all_admins`, `all_chat_filters`,
  `my_admin_actions`) gate on `admin_identity`.
- **Campaign keying.** Campaign and schematic persistence is keyed by
  `RelayMatch.campaign_id`; a match without one keeps it a no-op.
- **ABI is append-only.** Table/view accessor names, enum variants and payload
  struct fields are part of the generated bindings and are never renamed.

## Verification

```bash
cargo check --manifest-path server/spacetimedb/Cargo.toml --tests
server/build.sh --check                 # bindings drift gate
GEN_CONTENT_SEED=1 server/build.sh      # publish + regenerate content_seed.rs
spacetime describe mindustry --server local   # tables/reducers/views
```

The `#[cfg(test)]` suites (`admin`, `mods`, `chat`, `campaign`, `identity`,
`main`, `relay`) are pure and typecheck under `cargo check --tests`; the
`cdylib` cannot be linked in place, so they are not run as Rust tests.
