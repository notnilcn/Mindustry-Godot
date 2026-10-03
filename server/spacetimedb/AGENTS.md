# AGENTS.md — `server/spacetimedb/` (module crate)

The `mindustry_godot` SpacetimeDB module. Read [`../AGENTS.md`](../AGENTS.md) and
[`../../AGENTS.md`](../../AGENTS.md) first; this file is the plan-21 module delta.

## Plan 21 additions (M0–M8)

The module is the D2 relay + persistent state. Reducers return no data, use
`ctx.sender()` as the only principal, and perform **cheap validation only** (no
server-side simulation). Failed reducers roll back, so rejections are
`log::warn!` (never `audit`).

| Module | Contents |
|---|---|
| `relay/` | Match directory/membership, the private ordered `match_command` log, `CommandRate`, `sender_command_state`, host state, player state, plans, checksums, snapshots, UI events, maintenance sweep. |
| `chat/` | `match_chat` + `chat_rate` + `chat_filter` (lowercase-substring mute). |
| `admin/` | `server_config` singleton, `admin_identity`, `player_ban`, `whitelist_entry`, `admin_rate`, `admin_action_log`; the full `admin_*` reducer set; admin-gated views (`am_i_admin`, `all_bans`, `all_whitelist`, `all_admins`, `all_chat_filters`, `my_admin_actions`). |
| `campaign/` | `campaign`, `sector_info`, `unlock`, `campaign_stats`, `schematic` + CRUD reducers/views; `persist_from_command` writes campaign rows on the ordered `ResearchUnlock`/`SectorCapture`/`SaveSector` commands (host only). |
| `mods/` | `match_mod` + `content_catalog`; join set comparison (`Missing mods:`/`Unnecessary mods:`) and `content_name_allowed`. |
| `main/` | `global.rs` tunables, `seeds.rs` (incl. `seed_content_catalog`), `content_seed.rs` (generated manifest), `lifecycle.rs`, `audit.rs`. |

## Rules (delta)

- **Views use indexes, not full scans.** Read-only view handles expose `find` +
  index accessors only. Full-table views range-unbounded over an index
  (`filter((Bound::<T>::Unbounded, Bound::<T>::Unbounded))`); the table needs a
  btree index for that (`admin_identity.by_admin_granted`, etc.). String-column
  filters must pass `&str`, never `String` (`String` is not `FilterableCopy`).
- **`content_catalog.name` is `#[unique]`**: the generated seed is deduped.
- **Generated bindings are never hand-edited.** After any schema/table/reducer
  change, regenerate with `server/build.sh` (or `spacetime generate --lang rust
  --out-dir client/rust/mind-stdb/src/module_bindings --module-path server/spacetimedb -y`).
- **Public/private:** `match_command`, `command_rate`, `sender_command_state`,
  `match_kick`, `match_snapshot_request`, `match_snapshot_chunk`, `chat_rate`,
  `admin_rate` are server-only; clients read the member semijoin views.
- **No IP/subnet bans** (OD-21-C): only STDB identity + whitelist.
- **Campaign/schematics** call `RelayMatch.campaign_id`; a match with no
  `campaign_id` makes `persist_from_command` a no-op.

## Verification

```bash
cargo check --manifest-path server/spacetimedb/Cargo.toml --tests
server/build.sh --check                 # bindings drift
GEN_CONTENT_SEED=1 server/build.sh      # publish + regenerate content_seed.rs
```

`admin::tests::*`, `mods::tests::*`, campaign and chat tests are pure and run
under `cargo check --tests` (they cannot link in place — expected).
