# AGENTS.md — server/ (SpacetimeDB)

The SpacetimeDB side of Mindustry-Godot: the persistent-state module plus the tooling that publishes it and regenerates the Rust client bindings. The server owns authoritative persistent rows, relays ordered client commands, and performs cheap validation only; the determinism-critical simulation runs on the clients. Read the root [`AGENTS.md`](../AGENTS.md) first. Module details live in [`server/spacetimedb/AGENTS.md`](spacetimedb/AGENTS.md).

## Layout

| Path | Responsibility |
|---|---|
| `spacetimedb/` | The `mindustry_godot` module crate (`cdylib`, `edition = "2024"`, `spacetimedb = "=2.10.1"`). Declares its own `[workspace]`, so it is NOT a member of `client/rust/`. |
| `spacetimedb/src/main/` | Module-wide home: `global.rs` tunables (`PROTOCOL_VERSION`, rate/cap constants), `seeds.rs` code seeds, `content_seed.rs` generated manifest, `lifecycle.rs` reducers, `audit.rs` server-only log, `tables.rs` shared types. |
| `spacetimedb/src/identity/` | `player`, `player_session`, `player_profile`, `client_settings` tables with their reducers and views. |
| `spacetimedb/src/relay/` | Match directory/membership, private ordered `match_command` log, rate gates, player state, plans, checksums, snapshots, UI events, `sweep.rs` maintenance. |
| `spacetimedb/src/chat/` | `match_chat`, `chat_rate`, `chat_filter` and `send_chat`. |
| `spacetimedb/src/admin/` | `server_config`, `admin_identity`, `player_ban`, `whitelist_entry`, `admin_rate`, `admin_action_log`; the `admin_*` reducers and admin-gated views. |
| `spacetimedb/src/campaign/` | `campaign`, `sector_info`, `unlock`, `campaign_stats`, `schematic` persistence and the `my_campaign_*` views. |
| `spacetimedb/src/mods/` | `match_mod` + `content_catalog`, mod-set comparison and `content_name_allowed`. |
| `spacetime.json` | Local defaults (`server: local`, `module-path: ./spacetimedb`) and the generate target `../client/rust/mind-stdb/src/module_bindings`. |
| `build.sh` | Canonical script: `spacetime generate` into the bindings dir, then `spacetime publish --delete-data=always`. |
| `gen_content_seed.sh` | Opt-in regeneration of `spacetimedb/src/main/content_seed.rs`. |

## Responsibilities

- Own persistent state: identities/sessions, match directory and membership, ordered command log, chat, admin, campaign/schematics, mods.
- Relay commands: append accepted `match_command` rows so every peer simulates deterministically.
- Validate cheaply: bounds, rate windows, roles and membership, with no server-side simulation.

## Rules

- **Reducers return no data.** They use `ctx.sender()` as the only principal and write rows; clients read state through views, never through reducer return values.
- **Cheap validation only.** Failed reducers roll back, so failure paths use `log::warn!`; `audit` records committed actions only.
- **Generated bindings are checked in and never hand-edited.** Regenerate them with `server/build.sh`; the drift gate verifies they match the module.
- **Publishing wipes data.** Every publish passes `--delete-data=always`, so local dev data is discarded and connected clients drop; seeds must re-run from code.
- **Database names** match `^[a-z0-9]+(-[a-z0-9]+)*$` (no underscores): the crate/module is `mindustry_godot`, the local DB is `mindustry`, and integration tests use `mindustry-it`. `--db`/`-Db` override.
- `spacetime generate` requires the `wasm32-unknown-unknown` Rust target. CLI, crate and bindings are pinned to `2.10.1`.
- No secrets or tokens are committed; STDB token/identity files are gitignored.

## Conventions

- The module is organized as `main/`, `identity/`, `relay/`, `chat/`, `admin/`, `campaign/`, `mods/`; each subsystem owns its tables, reducers and views.
- Public accessor names and enum variant names are schema ABI: append-only, never rename.
- Tunables live in `main/global.rs` and are mirrored by `client/rust/mind-stdb/src/protocol.rs`; keep them in lockstep and bump `PROTOCOL_VERSION` on envelope or view-shape changes.
- Views use indexes, never full-table scans; a full-table view range-unbounds over a btree index.
- Seeds live in `main/seeds.rs` and re-run on every publish.

## Commands

```bash
server/build.sh                              # publish mindustry + regenerate bindings
server/build.sh --check                      # bindings drift gate (no publish)
cargo check --manifest-path server/spacetimedb/Cargo.toml --tests
GEN_CONTENT_SEED=1 server/build.sh           # also regenerate content_seed.rs
spacetime describe mindustry --server local  # tables/reducers
spacetime logs mindustry --server local      # module logs
spacetime sql mindustry --server local "SELECT * FROM player LIMIT 10"
```

`--check` regenerates the bindings into a temp directory and diffs them against the checked-in copy without publishing, exiting non-zero on drift. Runtime client/server verification runs through the Godot MCP tools rather than CLI reducer calls; see [`.opencode/skills/playtest/SKILL.md`](../.opencode/skills/playtest/SKILL.md).
