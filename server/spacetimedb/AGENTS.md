# AGENTS.md — server/spacetimedb/ (module crate)

The `mindustry_godot` SpacetimeDB module: the match directory, membership, the
private ordered command relay and persistent state (identity, chat, moderation,
campaign, schematics, mods) for Mindustry-Godot. Reducers return no data, use
`ctx.sender()` as the only principal and perform cheap validation only; there is
no server-side simulation. Read the root [`AGENTS.md`](../../AGENTS.md) first,
then [`server/AGENTS.md`](../AGENTS.md). Publishing, database naming and generated
bindings are owned by `server/AGENTS.md`; client binders and subscription waves by
[`client/rust/mind-stdb/AGENTS.md`](../../client/rust/mind-stdb/AGENTS.md).

## Layout

| Path | Responsibility |
|---|---|
| `lib.rs` | Crate root; declares `admin`, `campaign`, `chat`, `identity`, `main`, `mods`, `relay`. |
| `admin/mod.rs` | `server_config` singleton (`ServerConfig`, id 0), `admin_identity`, `player_ban`, `whitelist_entry`, server-only `admin_rate`, append-only `admin_action_log`; `AdminActionKind`, the `admin_*` reducers and the admin-gated views. |
| `admin/actions.rs` | `ActionType` (Java parity), persisted `action_filter` policy rows, server-only `action_rate` interaction windows; `admin_add_action_filter`/`admin_remove_action_filter`, `all_action_filters`, `action_type_for`, `action_verdict`, `interact_decision`, `enforce_action_filters`. |
| `admin/history.rs` | Server-only `player_info` history (names, joins, kicks, kick-cooldown expiry), `player_trace` answers to `adminRequest(trace)`; `record_join`/`record_name`/`record_kick`/`apply_kick`, `find_by_name`/`search_names`, name-ban regex validation/enforcement, `admin_ban_name`/`admin_unban_name`/`admin_request_trace`, `all_player_info`/`my_traces`. |
| `campaign/mod.rs` | `campaign`/`sector_info`/`unlock`/`campaign_stats`/`schematic` tables with their CRUD reducers/views; `persist_from_command`. |
| `chat/mod.rs` | `ChatKind` (`All`/`Team`/`System`/`Admin`), `match_chat` (with private `target`), server-only `chat_rate`, per-match server-only `match_chat_state`, admin-managed `chat_filter` (global or match-scoped); `send_chat` command dispatch, `broadcast_system`/`private_notice`, `sanitize_text`, `filter_mutes`, `rate_limit`, `chat_visible`. |
| `chat/commands.rs` | Pure `/t`, `/a`, `/votekick`, `/vote`, `/help`, `/sync` parser + `/help` paging (Java `NetServer.registerCommands`). |
| `chat/vote.rs` | `match_vote`, `vote_ballot`, `vote_cooldown`; `start_vote`/`cast_vote`/`expire_votes`/`sweep_votes`, `votes_required`, kick-on-pass through `apply_kick`. |
| `identity/` | `tables.rs` (`player`, `player_session`, `player_profile`, `client_settings`), `methods.rs` validators, `reducers.rs` (`set_username`, `create_profile`, `update_client_settings`), `views.rs` (`local_player`, `all_players`, ...). |
| `main/` | `global.rs` tunables, `seeds.rs` code seeds, `content_seed.rs` generated `VANILLA_CONTENT`, `lifecycle.rs` (`init`/`client_connected`/`client_disconnected`), `audit.rs` server-only `audit_log` + `audit`, `tables.rs` (`protocol_info`, `relay_config`, `AuditKind`). |
| `mods/mod.rs` | `match_mod`, `content_catalog`; `check_mods` and `content_name_allowed`. |
| `relay/` | Match directory/membership, the private `match_command` log, rate/session state, rules, plan snapshots, checksums, snapshots, UI events, published `match_event` server events, server→client `@Remote` call rows (`calls.rs`), PvP team liveness/assignment (`teams.rs`), LWW player state and the maintenance sweep; every relay reducer and view. |
| `Cargo.toml` | `cdylib` crate `mindustry_godot`, its own workspace, pinned to `spacetimedb = "=2.10.1"`. |

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
- **Chat.** `send_chat` rate-gates the connection, parses `/` commands before
  the filters (Java `NetClient.sendChatMessage`), dispatches `/t`, `/a`,
  `/votekick`, `/vote`, `/help`, `/sync`, then sanitizes, applies the
  global + match-scoped `chat_filter` patterns (three infractions set the
  per-match `match_chat_state.muted_until` 60 s ahead) and the built-in
  anti-spam gate (`messageRateLimit`/`messageSpamKick`). `ChatKind::Admin` needs
  an admin, `ChatKind::System` the host/admin; every notice is a `match_chat`
  row so clients converge through `my_match_chat`.
- **Moderation.** `admin_kick`/`admin_ban`/`admin_unban`, whitelist and admin
  grant reducers, `admin_set_config`, the chat-filter reducers (global and
  match-scoped), action filters, PlayerInfo history/trace/name bans and the
  ordered wave/team/tile emitters; each committed action appends
  `admin_action_log` and an `audit_log` line. Bans are identity-only (no IP/subnet
  bans); name bans are case-insensitive regexes enforced on create/join, and a
  kick cooldown blocks rejoin until `player_info.last_kicked_at`.
- **Server→client calls (GAPS #35).** `relay/calls.rs` publishes one private
  `match_call` row per `@Remote` call the host/admin sends (`publish_call`:
  kind, optional target, the `*CallPacket.write` body); `my_match_calls`
  exposes broadcast + targeted rows and the client binder turns each row into a
  `[kind][body]` frame (`mind-stdb::calls`). Rows are pruned by
  `CALL_RETENTION_SECS`.
- **PvP teams (GAPS #33).** `relay/teams.rs` stores the host-published team
  liveness (`match_team`, `publish_match_teams`) and balances joiners/starters
  over the connected-member counts (`NetServer.assigner`); non-PvP members stay
  unassigned so the client's world default team applies.
- **Events.** `relay/events.rs` publishes one `match_event` row per
  server-observable event (match lifecycle, join/leave/kick, chat, votes,
  config/name-ban/admin changes, `/sync`); `my_match_events` is the client
  `EventType` bus source (GAPS #37).
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
  `match_ttl_hours`, chat/UI/checksum/snapshot/plan/event/trace retention,
  expired votekick sessions, idle rate rows and old player sessions.

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
- **A schema change is a bindings change.** Never hand-edit the generated bindings;
  the regeneration/publish flow is owned by `server/AGENTS.md`.
- **`content_catalog.name` is `#[unique]`**; the generated seed is deduped.

## Invariants

- **Server-only tables** (no `public`): `audit_log`, `match_call`, `relay_member`,
  `match_command`, `command_rate`, `sender_command_state`, `match_kick`,
  `match_snapshot_request`, `match_snapshot_chunk`, `snapshot_rate`,
  `checksum_rate`, `match_player_state_rate`, `chat_rate`, `match_chat_state`,
  `admin_rate`, `action_filter`, `action_rate`, `player_info`, `player_trace`,
  `match_vote`, `vote_ballot`, `vote_cooldown`, `maintenance_schedule`.
  Clients reach them through member semijoin views (`my_match_commands`,
  `my_kick`, `my_match_snapshot_chunks`, `my_sender_command_state`, ...).
- **Member semijoin views.** `all_matches` lists public non-ended matches; the
  other match views (`my_matches`, `my_match_members`, `my_match_state`,
  `my_match_commands`, `my_match_chat`, `my_match_checksums`,
  `my_match_snapshots`, `my_match_snapshot_requests`, `my_match_ui_events`,
  `my_match_player_states`, `my_match_plans`, `my_match_events`,
  `my_match_calls`) resolve the
  caller's active match through `relay_member` and never expose another match's
  rows. Admin views (`am_i_admin`, `all_bans`, `all_whitelist`, `all_admins`,
  `all_chat_filters`, `all_action_filters`, `all_player_info`, `my_traces`,
  `my_admin_actions`) gate on `admin_identity`.
- **Campaign keying.** Campaign and schematic persistence is keyed by
  `RelayMatch.campaign_id`; a match without one keeps it a no-op.
- **ABI is append-only.** Table/view accessor names, enum variants and payload
  struct fields are part of the generated bindings and are never renamed.

## Verification

```bash
cargo test --manifest-path server/spacetimedb/Cargo.toml --lib   # pure unit suites
cargo check --manifest-path server/spacetimedb/Cargo.toml --tests
spacetime describe mindustry --server local --json   # tables/reducers/views
```

The `#[cfg(test)]` suites (`admin`, `chat`, `mods`, `campaign`, `identity`,
`main`, `relay`) are pure and **do** run: `cargo test --lib` links the crate as a
test harness on the host target (verified 2026-11-07, 109 tests). The
`cdylib`-only note in older revisions was stale. Reducer behaviour is exercised
end-to-end by publishing to a local `spacetime start` node and calling reducers
(see `runs/PAR-0121/`).
