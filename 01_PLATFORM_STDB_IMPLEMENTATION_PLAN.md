# 01 — Platform: Rust `sstdbsdk` replacement (`mind-stdb`) + SpacetimeDB skeleton + command relay

## 1. Header block

| Field | Value |
|---|---|
| **Status** | Draft, not started. Authored 2026-10-01 against `HIGH_LEVEL_PLAN.md` §0/§2/§4/§6–§9. |
| **Phase** | P1 — Platform & content. |
| **Depends on** | `00_FOUNDATION_IMPLEMENTATION_PLAN.md` (workspace, gdext bring-up, `mind-headless` harness, build scripts, MCP bridge). This plan must not start before plan 00's workspace + headless + MCP gates are green. |
| **Blocks** | `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` (full schema, relay authority, snapshots); the lobby/identity parts of `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` (profiles, campaign identity) and `14_UI_IMPLEMENTATION_PLAN.md` (settings dialog, join/lobby UI, connection-lost UI). |
| **Sources** | C# being replaced: `client/sstdbsdk/AGENTS.md`, `DatabaseConnector.cs`, `TableSubscriber.cs`, `TableBinderComponent.cs`, `DatabaseConnector.tscn` (584 LOC total). Conventions: `/mnt/c/Users/Clinton/g/main/AGENTS.md`, `/mnt/c/Users/Clinton/g/main/server/AGENTS.md`, and a real 2.x module read for shape: `/mnt/c/Users/Clinton/g/main/server/spacetimedb/{Cargo.toml,src/lib.rs,src/main/{mod,lifecycle,seeds,global}.rs,src/item/{reducers,views}.rs,src/chat/mod.rs}`. Tooling: local `spacetime` CLI 2.10.1 (WSL; 2.10.2 upgrade available), `spacetimedb-sdk` 2.10.1 (docs.rs), `spacetimedb` server crate 2.10.1 (crates.io). Test rig: `/mnt/c/Users/Clinton/g/.opencode/skills/playtest/SKILL.md` (main-project recipes; the mindustry-godot analog is created by plan 00). |
| **Extends spine** | Plan 00's rig is untouched. Adds: `StdbConnector` (gdext autoload, one `Node`) + optional `StdbBinder` (gdext `Node`) to the project; a `mind-headless` scenario family (`stdb_*`); a `net` page on the plan-00 state inspector (connector state, wave status, relay counters); `server/spacetime.json` + `build.sh`/`build.ps1` publish rig; generated Rust bindings. The plan-00 in-engine spine must keep working **offline** with the STDB layer absent. |

This plan replaces the C# `sstdbsdk` semantics 1:1 (D1) and lays the STDB foundation: identity/session/profile/settings/audit skeleton, subscription waves, typed binders, and an ordered per-match command relay with cheap server-side validation (D2). Catalog/save/campaign tables are explicitly **not** here — plan 21 owns the full schema.

## 2. Scope & parity definition

### 2.1 In scope

1. **`mind-stdb` crate** (pure Rust, Godot-free, tokio-free in its public API): connection lifecycle equivalent to `DatabaseConnector.cs`; token persistence per host + `--pN` suffix; subscription waves equivalent to `TableSubscriber.cs` base/lobby/game; typed per-table binder equivalent to `TableBinderComponent.cs`; frame pump bridge.
2. **Generated Rust bindings workflow**: `spacetime generate --lang rust` into `client/rust/mind-stdb/src/module_bindings/`; never hand-edited; `server/build.sh` (WSL/Linux primary) + `build.ps1` (Windows parity) regenerate + publish locally.
3. **`server/spacetimedb/` skeleton** (2.x conventions, `#[table(accessor, public)]`, views, indexes, `ctx.sender()`, spread updates): `player`/`player_session`/`player_profile`, `client_settings`, `protocol_info`, `audit_log`, `relay_config`, lifecycle reducers (`init`/`client_connected`/`client_disconnected`), seeds in code.
4. **Command-relay foundation**: `relay_match`/`relay_member` + append-only ordered `match_command` (private table exposed through a per-caller view), client reducers with cheap validation only (identity/ownership/membership, rate limit, enum, coarse range/existence/payload caps), and a client `CommandStream` that delivers every command to every peer in transaction order with per-sender sequence checks.
5. **gdext layer**: `StdbConnector` autoload (`_process` pump, Godot signals) and `StdbBinder` node (no-arg signals + debug JSON row) so GDScript UI layout can consume rows without C#. Typed Rust consumers use `mind-stdb` directly.
6. **Local dev workflow** documented and scripted: `spacetime start`, publish/wipe, `logs`/`sql`/`call`, test isolation.
7. **`mind-stdb/AGENTS.md`** — the Rust rewrite of the C# SDK `AGENTS.md` recipe (five steps: table → reducer → wave → binder → reducer call; the 10 rules adapted).

### 2.2 Done means

- `cargo test -p mind-stdb` is green network-free (offline path included); `cargo check --manifest-path server/spacetimedb/Cargo.toml --tests` is green.
- A local `spacetime start` + `server/build.sh` publish yields a running module; two Godot instances (`--p1`, `--p2`) connect, subscribe base/lobby waves, join the same match, and observe a `Ping` command round-trip through the relay (MCP scenario in §7.3).
- Port map rows all have a named Rust replacement; generated bindings reproduce byte-identically after regeneration.
- Perf budgets in §7.4 are met or the plan is fixed first.

### 2.3 Deferred (explicit ownership)

| Item | Owner |
|---|---|
| Full STDB schema (players/profiles/matches/sectors/schematics/tech unlocks/full command set) | `21_MULTIPLAYER_IMPLEMENTATION_PLAN.md` |
| Authoritative server match sim (D2 later phase), snapshots, checksums, desync correction, late-join world streaming | 21 |
| Host/join/ban/whitelist/chat/admin reducers and UI | 21, 14 |
| Lobby/join UI, settings dialog, connection-lost panel | 14 |
| Device-side client settings surface (all keybinds/audio/video) | 14 (this plan stores the opaque settings row) |
| Campaign profiles/saves/sector state | `12_CAMPAIGN_IMPLEMENTATION_PLAN.md`, `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` |
| Content IDs used inside commands (`block_id: u16`, etc.) | `02_CONTENT_IMPLEMENTATION_PLAN.md` (ID spaces) + 21 (payload binding) |
| Transport beyond STDB relaying (OD3), direct UDP/ENet if measured too slow | 21 |

### 2.4 Deliberate deviations (with reason)

- **No `Username` hook inside the connector.** The C# `DatabaseConnector.OnConnected` hardcoded `LocalLobbyPlayer`; here profile/username state is a normal bound view (`local_player_profile`) and consumers own it.
- **No scene rebuild after reconnect.** C# rule 10 ("rebuild the scene") is replaced by `Connector::reconnect()` re-registering all binders and waves itself, with a `Resync` event so consumers can drop stale state. Reason: typed binders are connector-owned data, not scene signal wiring.
- **Signals carry a reason.** `disconnected(reason: GString)` adds a diagnostic arg the C# signal lacked.
- **Command table is persistent + append-only, not an `event` table.** Event tables cannot replay history for late joiners and cannot be scanned in order by a future server sim; persistent rows with a monotonic key can do both (details §3.8–§3.10).
- **Own version scheme.** Protocol version handshake is ours; no cross-build with Java clients (HIGH_LEVEL_PLAN §9).

## 3. Target design

### 3.1 Repository additions (per HIGH_LEVEL_PLAN §2.1)

```
client/rust/
  Cargo.toml                          # workspace (plan 00 owns)
  mind-stdb/
    Cargo.toml
    AGENTS.md                         # Rust rewrite of client/sstdbsdk/AGENTS.md
    src/
      lib.rs
      config.rs                       # ConnectionConfig, StdbMode, ConnectPolicy
      identity.rs                     # LocalIdentity, parse_player_suffix, IsLocal
      token.rs                        # TokenStore trait, FileTokenStore, token_key()
      connector.rs                    # Connector, ConnectorState, ConnectorEvent, pump()
      waves.rs                        # WaveName, SubscriptionWaves, WaveEvent
      binder.rs                       # TableBinder<A>, RowChange<T>, BinderOptions
      relay.rs                        # CommandStream, CommandOrder, OrderError
      protocol.rs                     # PROTOCOL_VERSION, version check helpers
      rows.rs                         # small RowView impls for gdext debug JSON
      module_bindings/                # GENERATED — never hand-edit
client/scenes/autoloads/
  stdb_connector.tscn                 # root node is the gdext StdbConnector class
server/
  spacetime.json                      # rust generate entry
  build.sh  build.ps1                 # generate + publish (bash primary; .ps1 Windows parity)
  spacetimedb/                        # module skeleton crate
```

`client/Scripts/` and `client/sstdbsdk/` stay as reference until plan 00's Rust equivalents land; after that they are deleted (plan 00).

### 3.2 Crate boundaries and dependencies

- `mind-stdb` depends only on: `spacetimedb-sdk = "=2.10.1"`, `log`, `serde`/`serde_json` (token file only). **No `godot`, no `tokio` direct dependency, no `mind-core`.** `cargo test -p mind-stdb` must not require Godot, network, or a running server (integration tests are `#[ignore]`/env-gated).
- `mind-gdext` depends on `mind-stdb` and wraps it: `StdbConnector`/`StdbBinder`. No game rules, no direct generated-binding use outside the wrapper.
- `mind-headless` depends on `mind-stdb` for `stdb_*` scenarios and benches.
- `server/spacetimedb` is a **separate Cargo workspace** (same as `main/server/spacetimedb/`): own `target/`, `crate-type = ["cdylib"]`, checked with `--manifest-path`. Reason: wasm build profile isolation and no accidental `spacetimedb-sdk`/`spacetimedb` feature unification.
- Command application to the sim is **not** wired in this plan: `mind-stdb` exposes `CommandStream`; plan 21 maps rows to `mind-core` commands at fixed-tick boundaries. `mind-core` never depends on `mind-stdb`.

### 3.3 `mind-stdb` module map (public API)

| Module | Public items | Role |
|---|---|---|
| `config` | `ConnectionConfig { host: String, db_name: String, token_append: Option<String>, token_store_path: Option<PathBuf>, mode: StdbMode, policy: ConnectPolicy }`, `StdbMode::{Online, Offline}`, `ConnectPolicy { auto_reconnect: bool, backoff: Backoff }` | C# `[Export] Host/DbName/TokenAppend` + explicit offline mode. |
| `identity` | `LocalIdentity { identity, suffix }`, `parse_player_suffix(args: impl Iterator<Item=String>) -> Option<String>`, `is_local(&self, other)` | Port of the `--pN` parser (exact `^--p\d+$`) and `IsLocal`. |
| `token` | `trait TokenStore { load(&self, key) -> Option<String>; save(&self, key, token) }`, `FileTokenStore`, `token_key(host: &str, suffix: Option<&str>) -> String` | Port of per-host `AuthToken` behavior; file JSON. |
| `connector` | `Connector`, `ConnectorState::{Offline, Idle, Connecting, Connected, Retrying, Disconnected}`, `ConnectorEvent::{Connected{identity}, Disconnected{reason}, ConnectError{message}, WaveApplied(WaveName), WaveError{wave,message}, Resync}`, `pump()`, `connect()`, `reconnect()`, `disconnect()`, `local_identity()`, `subscribe_base()/subscribe_lobby()/subscribe_game()/unsubscribe_lobby()/unsubscribe_game()` | C# `DatabaseConnector` + `TableSubscriber` lifecycle semantics. |
| `waves` | `WaveName::{Base, Lobby, Game}`, `SubscriptionWaves` (state of each wave, queries per wave), `WaveEvent::Applied/Error`, `is_applied(WaveName)` | C# static wave lists + `LobbyApplied`/`LobbySubActive`. |
| `binder` | `TableBinder<A: TableAccessor<RemoteTables>>` (and `TableWithPrimaryKey` specialization for updates/replay), `RowChange<T> { Insert(T), Update{old,new}, Delete(T) }`, `BinderOptions { replay_existing: bool, verbose: bool }`, `BinderHandle` (drain), `bind(...)` on `Connector` | C# `TableBinderComponent` signals, Rust-typed. |
| `relay` | `CommandStream { subscribe(conn, match_id), drain() -> Vec<MatchCommandRow>, applied_count(), last_command_id(), order_error() }`, `CommandSender::send(match_id, client_tick, kind)` | New: ordered delivery of the match command log. |
| `protocol` | `PROTOCOL_VERSION: u32`, `check_protocol(server_row) -> Result<(), ProtocolError>` | Own version scheme (HIGH_LEVEL_PLAN §9). |
| `module_bindings` | `DbConnection`, `RemoteTables`, `RemoteReducers`, `Reducer`, row types (`MatchCommand`, `RelayMatch`, ...), view accessors | `spacetime generate` output only. |

### 3.4 Runtime bridge (explicit design)

The C# SDK pump is `DbConnection.FrameTick()` once per `_Process`. The Rust SDK offers exactly the same primitive: `DbConnection::frame_tick()` — "advance the connection until no work remains, then return rather than blocking". The bridge:

- **Primary (format-ago parity, chosen default): main-thread `frame_tick`.** `mind-gdext::StdbConnector::_process` calls `mind_stdb::Connector::pump()` exactly once per frame; `pump()` calls `conn.frame_tick()`, converts `Err` into a `ConnectorEvent::Disconnected`, and never panics or unwraps. All SDK callbacks run synchronously on the Godot main thread inside `pump()`. No tokio runtime is created by our code — the SDK owns its transport runtime internally; `frame_tick` is a non-blocking poll of it.
- **Callback rule:** SDK table callbacks must be cheap and Godot-free at the point they run. They do exactly one thing: push a typed `RowChange<T>` into the binder's `std::sync::mpsc` queue (one per binder). Consumers drain queues after `pump()` in the same frame. This mirrors C# rule 9 ("handlers run inside `_Process`, keep them cheap") but moves work to an explicit queue.
- **Why the queue design makes the pump swappable:** because callbacks never touch Godot or `Connector`, the fallback mode is a one-line change. **Fallback (feature `threaded-pump`, off by default):** call `conn.run_threaded()` once; callbacks run on the SDK's worker thread and push to the same queues; `pump()` becomes "drain internal channel" and still runs on the main thread. Use only if measured `frame_tick` p99 exceeds budget (§7.4); the rest of the API is unchanged.
- **Thread rule:** Godot objects are main-thread only. The binder-queue contract enforces this for both pump modes; any future direct-callback API must be marked main-thread-only.
- **Frame order in `mind-gdext` (interface with plans 00/05/21):** `pump stdb` → `drain binder queues` → `drain CommandStream into pending (do not apply mid-frame)` → `sim accumulator steps` (plan 05) → `view sync` → `UI`. Commands are applied only at fixed-tick boundaries, never on arrival; this is what keeps relay replay deterministic on every peer. Plan 21 consumes `CommandStream` here.
- **Reconnect:** on `frame_tick` error, state → `Disconnected`, emit event; with `auto_reconnect` the connector re-`Connect()`s after backoff using the saved token. On `on_connect`, the connector re-subscribes all active waves and re-registers all binders, then emits `Resync` so consumers discard stale mirrors and replay from the cache.
- **Offline mode:** `StdbMode::Offline` never builds a `DbConnection`; `pump()` is a no-op; state is `Offline`. This is the path that must keep the single-player game fully functional and is the headless test default.

### 3.5 Typed binder API

Rust analog of `TableBinderComponent` (one binder per table per consumer):

```text
TableBinder<A: TableAccessor<RemoteTables>>          // A = generated marker `MatchCommandTableAccessor`
  .drain() -> impl Iterator<Item = RowChange<A::Row>>
  .replay_existing(bool)                            // only where A: TableWithPrimaryKey
```

- Callbacks are registered through the generated table handle (`Table::on_insert`/`on_delete`, `TableWithPrimaryKey::on_update`) and push into the binder's queue; `LastRow`/`LastOldRow`/`LastDeletedRow` and the "rows cannot cross the signal boundary" workaround disappear — `RowChange<T>` carries owned rows.
- **Replay**: `replay_existing` iterates the client cache once **after** registering live callbacks and enqueues each row as `Insert`. It is only callable on primary-key/persistent tables (type state). Event tables get a binder without the method, making C# rule 3 ("replay false on event tables") a compile time property instead of a convention.
- **Event tables**: binders for `A: EventTable` expose `Insert` only (`on_insert`); `remove_on_insert`/`remove_on_delete` IDs are stored inside the binder so `_exit_tree`/drop unregisters cleanly (no leak, C# `_ExitTree` parity). **M3 implementation note (recorded 2026-10-01):** callback IDs are not stored — they are table-typed and a dropped binder has no connection handle. Callbacks capture `Weak<BinderCore>` instead (dropped binder ⇒ inert no-op, queue freed) and registrations die with the connection; same no-leak/no-stale-delivery contract, see `binder.rs` docs and the M3 changelog entry.
- **GDScript bridge**: `StdbBinder` (gdext) owns one `TableBinder` for a table named via `#[export] table_name`, emits arg-less `row_inserted`/`row_updated`/`row_deleted` plus `last_row_json: String` (via the small hand-written `RowView` impls in `rows.rs`, never by editing generated code). Typed gameplay/UI code should consume the Rust binder; the node exists for layout-time wiring and debugging (plan 14 owns the real UI data paths).

### 3.6 Subscription waves

Port of `TableSubscriber.BaseTables/LobbyTables/GameTables`, with one intentional structural change: static waves are built with the **typed query builder** (`subscription_builder().add_query(|q| q.from.<table>())...`) rather than C# reflection over `From`; that keeps rename-safety without SQL strings. If generated view accessors turn out not to exist for view-only queries (SDK uncertainty), fall back to `&'static [&'static str]` SQL lists in the same module (single place to change).

| Wave | Semantics | Contents (plan 01 skeleton) |
|---|---|---|
| Base | Subscribed automatically on connect; no applied callback by design (late binders replay the cache) | `protocol_info`, `relay_config`, `relay_match` (directory), `local_client_settings` |
| Lobby | Subscribed automatically on connect; raises `WaveApplied(Lobby)`; `unsubscribe_lobby()`/`subscribe_lobby()` manual toggles | `local_player`, `local_player_profile`, `all_players`, `my_matches` |
| Game | Explicit `subscribe_game()` on join / `unsubscribe_game()` on leave; raises `WaveApplied(Game)` | `my_match`, `my_match_commands` (the relay stream; §3.8) |

`WaveName::Game` subscription and the `CommandStream` are separate objects: the wave covers scene-level match state; `CommandStream` binds `my_match_commands` for the live match and owns ordering (§3.8). Both are re-issued on reconnect.

### 3.7 Server module skeleton layout (2.x conventions mirroring `main/server/spacetimedb`)

```
server/spacetimedb/
  Cargo.toml                 # package mindustry_godot, edition 2024, crate-type cdylib, spacetimedb = "=2.10.1"
  src/
    lib.rs                   # #![allow(special_module_name)] + mod declarations
    main/
      mod.rs
      global.rs              # constants: PROTOCOL_VERSION, rate window/cap, map bounds defaults
      lifecycle.rs           # #[reducer(init)], #[reducer(client_connected)], #[reducer(client_disconnected)]
      seeds.rs               # Seed trait + seed_protocol_info/seed_relay_config
      audit.rs               # AuditLog table + internal audit() helper (server-only)
    identity/
      mod.rs
      tables.rs              # Player, PlayerSession, PlayerProfile, ClientSettings
      methods.rs             # username/profile validation helpers
      reducers.rs            # set_username, create_profile, update_client_settings
      views.rs               # local_player, local_player_profile, all_players, local_client_settings
    relay/
      mod.rs
      tables.rs              # RelayMatch, RelayMember, MatchCommand, CommandRate (server-only)
      methods.rs             # require_member, rate_allow, validate_kind
      reducers.rs            # create_match, join_match, leave_match, start_match, send_match_command
      views.rs               # my_matches, my_match, my_match_commands
```

Rules carried from `main/server/AGENTS.md`: reducers deterministic and return no data (`Result<(), String>` at most); `ctx.sender()` is the only principal; auto-inc IDs are not sequential; spread-update (`..row`) never `Default`; index names unique module-wide; failed reducers roll back — rejection auditing goes to `log::warn!`, not `AuditLog` (see §3.9).

### 3.8 Relay schema and ordering

Illustrative 2.x shapes (plan 21 extends variants/tables; envelope stays):

```rust
// relay/tables.rs
#[derive(SpacetimeType, Clone, Copy, PartialEq, Eq)]
pub enum MatchStatus { Lobby, Running, Ended }

#[derive(SpacetimeType, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityMode { Relay, Authoritative }        // D2: Relay today, authoritative later

#[table(accessor = relay_match, public)]
pub struct RelayMatch {
    #[primary_key] #[auto_inc] pub match_id: u64,
    pub map_id: String, pub map_seed: u64,
    pub map_width_tiles: i32, pub map_height_tiles: i32,
    pub status: MatchStatus,
    pub authority: AuthorityMode,
    pub protocol_version: u32,
    pub created_by: Identity, pub created_at: Timestamp,
    pub started_at: Option<Timestamp>, pub ended_at: Option<Timestamp>,
}

#[table(accessor = relay_member,
        index(accessor = by_match_identity, btree(columns = [match_id, identity])),
        index(accessor = by_identity, btree(columns = [identity, match_id])))] // M4: needed for the per-caller views
pub struct RelayMember {
    #[primary_key] #[auto_inc] pub member_id: u64,
    #[index(btree)] pub match_id: u64,
    pub identity: Identity, pub joined_at: Timestamp,
}

// Append-only command log. PRIVATE table: only ever exposed through `my_match_commands`.
#[table(accessor = match_command, index(accessor = by_match_command, btree(columns = [match_id, command_id])))]
pub struct MatchCommand {
    #[primary_key] #[auto_inc] pub command_id: u64,   // global commit order; gaps are normal
    #[index(btree)] pub match_id: u64,
    pub sender: Identity,
    pub sender_seq: u64,                              // per-sender continuity, starts at 1
    pub client_tick: u64,                             // sender's 60 Hz sim tick
    pub kind: CommandKind,
    pub sent_at: Timestamp,
}

#[derive(SpacetimeType, Clone)]
pub enum CommandKind {
    Noop,
    Ping { nonce: u64 },
    PlaceBlock { x: i32, y: i32, block_id: u16, rotation: u8, config: u32 },
    BreakBlock { x: i32, y: i32 },
    ConfigBlock { x: i32, y: i32, config: u32 },
    // plan 21 owns the full parity set (unit orders, objectives, payloads, ...)
}
// M4 implementation note (recorded 2026-10-01): STDB 2.10.1's `SpacetimeType`
// derive only accepts unit/newtype variants, so the payloads are product
// structs (`PlaceBlock`/`BreakBlock`/`ConfigBlock`) and the variants are
// newtypes (`Ping(u64)`, `PlaceBlock(PlaceBlock)`, ...). The contract above is
// unchanged in meaning; plan 21 owns the schema hard-cut.

#[table(accessor = command_rate)]                    // server-only
pub struct CommandRate {
    #[primary_key] pub identity: Identity,
    pub window_start: Timestamp, pub count: u32,
    pub last_sender_seq: u64,                        // persists across window rolls (gap/dup gate)
}
```

Ordering contract:

- STDB serializes transactions; `command_id` (auto-inc) is monotonic in commit order. Clients apply commands sorted by `command_id` ascending. **Numeric gaps in `command_id` are normal and are not loss** (main/server rule 4); loss detection uses `sender_seq` continuity per sender (`OrderError::Gap { sender, expected, got }`). Duplicates (`command_id` already applied) are ignored — they occur on binder replay/reconnect.
- Every peer subscribed to `my_match_commands` receives every command in transaction order; the client asserts monotonicity and surfaces protocol violations instead of guessing.
- `my_match_commands` view (right-semijoin of `relay_member` → `match_command` on `match_id`) is the only exposure of the private log; nobody can subscribe to another match's commands.
- Retention/compaction is **not** implemented here: plan 21 adds snapshot rows + a prune pass. Row growth budget is in §7.4.

### 3.9 Cheap validation only (D2)

`send_match_command(ctx, match_id, client_tick, kind) -> Result<(), String>` performs, in order:

1. **Existence/ownership:** match exists and is `Running`; caller is the sender (`ctx.sender()`, never an identity argument); caller has a `relay_member` row for `match_id` (`by_match_identity` index).
2. **Rate limit:** `CommandRate` 1 s window per identity (same shape as `main`'s `ChatRate`), cap `RelayConfig.commands_per_second` (default 600; plan 21 tunes against real build spam).
3. **Protocol:** match `protocol_version` equals module `PROTOCOL_VERSION` and client is on the same build (client sends nothing; the check is on match creation and on the client handshake view).
4. **Enum/payload shape:** `CommandKind` is exhaustive; no opaque bytes.
5. **Coarse range/existence:** for placement/config commands, `0 <= x < map_width_tiles`, `0 <= y < map_height_tiles`, `rotation <= 3`, `block_id != 0`. No tile occupancy, no resources, no rule legality — those are sim-side and stay client-local until D2's sim arrives server-side.
6. **Per-sender sequence:** `sender_seq` must be strictly increasing per sender (`CommandRate.last_sender_seq` persists across rate-window rolls, so replayed old sequences are rejected). **M4 implementation note:** the reducer signature has no `sender_seq` argument, so the server assigns it atomically from `CommandRate.last_sender_seq` (+1, overflow-checked); the client still verifies continuity (gap detection) as a protocol-violation detector.

Rejections return `Err(String)` (rolled back) and log via `log::warn!`; successful lifecycle events (`create_match`, `join_match`, `start_match`) write `AuditLog` rows. This split is deliberate: a failed reducer's writes roll back, so a rejection cannot persist an audit row in the same transaction.

### 3.10 How this leaves room for an authoritative STDB sim (D2), and what is deferred

- The **command envelope is authority-neutral**: today each client inserts intents into `match_command`; tomorrow a scheduled server sim consumes the exact same rows in `command_id` order and writes authoritative state tables that do not exist yet (`match_snapshot`, `match_entity`, `match_checksum`, ...). Adding tables is additive; no command field needs to change.
- `RelayMatch.authority` lets clients switch policy (run deterministic sim from relayed commands vs mirror authoritative rows) without a schema migration in the client.
- `sender_seq` + `client_tick` are already lockstep-compatible (per-sender total order + intended tick); server sim can additionally stamp `exec_tick` on its own outputs.
- Cheap validation becomes the front gate the sim re-checks; it is deliberately a pure helper (`validate_kind`) so it can be reused by the sim.
- **Deferred:** the sim itself, `exec_tick`, snapshots/checksums/desync, late-join streaming, admin/ban/whitelist, chat, full command variants, content-hash verification, retention. All plan 21. Adding command variants is a schema change and a hard cut (dev wipes expected); plan 21 owns the cutover.

### 3.11 gdext layer (thin pump)

- `mind-gdext::stdb::StdbConnector` — `#[derive(GodotClass)] #[class(base=Node)]`. Exports: default host `http://127.0.0.1:3000`, db `mindustry`, mode online/offline. `_ready`: build `Connector`, parse `--pN` via `OS::get_cmdline_args()`/`get_cmdline_user_args()` (same order as C#), `connect()` unless offline. `_process(delta)`: `connector.pump()` exactly once, then drain connector events and emit Godot signals. `_exit_tree`: `disconnect()`. Signals: `connected`, `disconnected(reason)`, `wave_applied(wave: String)`, `resync`. Methods (GDScript-callable): `connect()`, `reconnect()`, `disconnect()`, `state()`, `local_identity_hex()`, `subscribe_game()`, `unsubscribe_game()`, plus plan-01 verification helpers `dev_create_match(map_id, seed)`, `dev_join_match(match_id)`, `dev_send_ping(nonce)`, `dev_relay_applied_count()`, `dev_last_command_id()` (kept as diagnostics; plan 21 may move them behind a debug console in plan 14).
- `mind-gdext::stdb::StdbBinder` — child node; `#[export] table_name: String`, `#[export] replay_existing: bool`, `#[export] verbose: bool`; signals `row_inserted`/`row_updated`/`row_deleted` (arg-less) + `last_row_json`/`last_deleted_row_json` properties.
- `client/scenes/autoloads/stdb_connector.tscn` — single `StdbConnector` node; registered as autoload `StdbConnector` in `project.godot` (register the scene, not a script — C# parity).
- **No game rules in this layer** (HIGH_LEVEL_PLAN §2.2).

### 3.12 Invariants (must hold at every milestone)

1. `mind-core` has no dependency on `mind-stdb`; `mind-stdb` has no dependency on `godot`.
2. Reducers are the only write path; the client never mutates relay/mirror state optimistically.
3. Command rows are append-only: no reducer updates or deletes `match_command`.
4. Commands are applied only at fixed-tick boundaries in `command_id` order; arrival time never influences sim order.
5. Every static wave membership is explicit in `waves.rs`; a binder for a table in no wave warns (C# convention kept).
6. Generated bindings are never hand-edited; `server/build.sh` is the only writer.
7. Exactly one `frame_tick` pump per process.
8. The game boots and plays single-player with `StdbMode::Offline` and no server running.

## 4. Port map

| C# (this repo, `client/sstdbsdk/`) | Rust replacement | Semantic changes |
|---|---|---|
| `DatabaseConnector.cs` (138 LOC) | `mind-stdb::connector::Connector`, `config::ConnectionConfig`, `token::{TokenStore, FileTokenStore}`, `identity::parse_player_suffix`; gdext `stdb::StdbConnector` | No hardcoded `LocalLobbyPlayer`/`Username` hook; `Result` pump instead of try/catch; explicit `Offline` mode and state machine; reconnect re-registers waves/binders (no scene rebuild) and emits `Resync`; token store path injected (Godot passes `user://`, headless passes temp/XDG data dir); C# `AuthToken` key algorithm ported exactly (`host.replace("://","_").replace(":","_").replace("/","_")` + suffix); the `--path` regression guard is preserved as a unit test. |
| `TableSubscriber.cs` (218 LOC) | `mind-stdb::waves::SubscriptionWaves` + `relay::CommandStream` | Typed query-builder waves replace reflection-built SQL; `LobbyApplied` → `WaveEvent::Applied(Lobby)`; `LobbySubActive` → `is_applied(WaveName)`; `MapConfig`/`LapQ`/`LapR` torus logic dropped (origin game, not Mindustry); dynamic per-match command subscription is a `CommandStream`, not a static wave; `_ExitTree` statics reset disappears (connector-scoped state). |
| `TableBinderComponent.cs` (228 LOC) | `mind-stdb::binder::{TableBinder<A>, RowChange<T>, BinderOptions}`; gdext `stdb::StdbBinder` | Typed owned rows instead of arg-less signals + `LastRow` casts; queues instead of direct Godot calls; `replay_existing` only exists on primary-key tables (compile-time rule instead of a warning); no `IEntity` ancestor/`ComponentRegistration` dependency (binder is given the connector); editor inspector dropdown → export enum/string validated against `SubscriptionWaves::all_tables()`. |
| `DatabaseConnector.tscn` | `client/scenes/autoloads/stdb_connector.tscn` | Root node is the native `StdbConnector` class; host/db defaults set in the scene export (local dev) — no C# script resource. |
| `client/sstdbsdk/AGENTS.md` (668 lines) | `client/rust/mind-stdb/AGENTS.md` | Rewritten for Rust: same five-step recipe (table → reducer → wave entry → binder + handler → call the reducer), rules 1–10 translated (rule 3 becomes a type error; rule 8 deleted with the `IEntity` framework; rule 10 replaced by connector-owned rebind), generated bindings path/build script, `stdb_*` test commands. |
| `README.md`, `pointers.md` (docs) | not ported | Repo-local scratch docs; superseded by `mind-stdb/AGENTS.md`. |

## 5. Milestones & task breakdown

Ordered; each milestone ends with its verification commands. **Smallest vertical slice first** (M1–M2): a headless `Connector` in offline mode that boots, pumps, and reports `Offline`, with `mind-headless run stdb_offline_boot` green — before any network or schema work.

### M0 — Workspace, crate skeletons, bindings pipeline
- [x] Add `client/rust/mind-stdb` to the plan-00 workspace; `Cargo.toml` with pins (`spacetimedb-sdk = "=2.10.1"`, `serde`, `serde_json`, `log`).
- [x] Create `server/spacetimedb/` crate (`mindustry_godot`, edition 2024, cdylib, `spacetimedb = "=2.10.1"`), `src/lib.rs` module skeleton (§3.7).
- [x] `server/spacetime.json` with a `rust` generate entry into `../client/rust/mind-stdb/src/module_bindings`; `server/build.sh` + `build.ps1` (`spacetime generate --lang rust ...`; `spacetime publish mindustry -y --delete-data` with `--server`/`--db` overrides; `--check` drift mode).
- [x] Commit generated bindings; add the never-hand-edit header note to `mind-stdb/AGENTS.md`.
- [x] `client/scenes/autoloads/stdb_connector.tscn` committed (native `StdbConnector` root; `.tscn`-first).
- [ ] `project.godot`: autoload `StdbConnector="*res://scenes/autoloads/stdb_connector.tscn"` — **deferred to M5** so the project never boots with a missing native class (the scene alone is inert).
- **Verify:** `cargo check -p mind-stdb`; `cargo check --manifest-path server/spacetimedb/Cargo.toml`; `server/build.sh --check` reports no drift; `spacetime --version` reports 2.10.1.

### M1 — Server skeleton (identity/session/profile/settings/audit)
- [x] `identity/tables.rs` + views + reducers: `set_username`, `create_profile`, `update_client_settings` (length/range checks, `ctx.sender()` ownership).
- [x] `main/lifecycle.rs`: `init` seeds protocol/config; `client_connected` upserts `Player` + inserts open `PlayerSession`; `client_disconnected` closes the session and updates `last_seen_at`.
- [x] `main/audit.rs`: `AuditLog` + server-only `audit()` helper; use on successful lifecycle/lobby writes.
- [x] `main/global.rs`: `PROTOCOL_VERSION`, rate window/cap, default map bounds.
- [x] `#[cfg(test)]` pure helpers in `identity/methods.rs` (validation) — typecheck gate only.
- **Verify:** `cargo check --manifest-path server/spacetimedb/Cargo.toml --tests`; local publish; `spacetime sql "SELECT * FROM protocol_info"` / `player` / `player_session` show seeded/connected rows; `spacetime call` a reducer and read it back.

### M2 — `mind-stdb` connector core (offline first)
- [x] `config.rs`, `token.rs`, `identity.rs` (pure `--pN` parser port), `connector.rs` state machine + offline mode + `pump()`.
- [x] `protocol.rs` (`PROTOCOL_VERSION` mirror + mismatch error type); `connect()` path builds the generated `DbConnection` with builder callbacks; token load/save; events queue.
- [x] Unit tests: suffix parser (including `--path` rejection), host-scoped token key, file-store round-trip, offline state, pump-without-connection no-op.
- [x] `mind-headless run stdb_offline_boot` scenario (offline connector boots, N pumps, expected state dump).
- **Verify:** `cargo test -p mind-stdb` (all network-free); `cargo run -p mind-headless -- run stdb_offline_boot`.

### M3 — Waves + typed binders
- [x] `waves.rs`: `WaveName`, static base/lobby/game query sets, applied/error events, manual toggles.
- [x] `binder.rs`: `TableBinder<A>`, queue, `RowChange`, replay on `TableWithPrimaryKey`, event-table specialization, drop cleanup.
- [x] Connector `bind()` + rebind-on-reconnect + `Resync`.
- [x] Headless scenario `stdb_binder_replay` (synthetic source); `stdb_command_order` moved to M4 (needs the `match_command` rows).
- [x] Env-gated integration test `local_connect_applies_base_and_lobby_waves` (`MIND_STDB_IT=1`).
- **Verify:** `cargo test -p mind-stdb`; with local server: `MIND_STDB_IT=1 cargo test -p mind-stdb -- --ignored local_connect_applies_base_and_lobby_waves`.

### M4 — Relay foundation (match/member/command + validation + CommandStream)
- [x] `relay/tables.rs`, `methods.rs` (`require_member`, `rate_allow`, `validate_kind`), `reducers.rs` (`create_match`, `join_match`, `leave_match`, `start_match`, `send_match_command`), `views.rs` (`my_matches`, `my_match`, `my_match_commands`).
- [x] `relay.rs` client: `CommandStream` binding `my_match_commands`, ordering by `command_id`, dedup, per-sender gap detection, `applied_count`/`last_command_id`/`order_error`.
- [x] Unit tests: order-not-arrival, duplicate ignore, per-sender gap flag, auto-inc numeric gaps are not loss; server `#[cfg(test)]`: bounds rejection, rate window roll.
- [x] Integration tests (env-gated): `two_clients_relay_ping_round_trip`, `relay_rejects_non_member_and_rate_limit`.
- **Verify:** `cargo test -p mind-stdb`; `cargo check --manifest-path server/spacetimedb/Cargo.toml --tests`; `MIND_STDB_IT=1 cargo test -p mind-stdb -- --ignored two_clients_relay_ping_round_trip`.

### M5 — gdext autoload + binder node + MCP playtest
- [x] `StdbConnector`/`StdbBinder` classes; scene + autoload; signals/methods per §3.11.
- [x] `net` page on the plan-00 inspector (state, wave flags, relay counters).
- [ ] Run the §7.3 MCP scenario end-to-end and record evidence (eval outputs, log lines, SQL rows, screenshot path). **Deferred to the orchestrator after merge**: the Godot editor points at the main worktree; lane 01 only builds/checks here.
- **Verify:** MCP checklist §7.3 passes; `godot_log errors` clean; offline launcher still boots with the server stopped.

### M6 — Perf, docs, handoff
- [ ] `mind-headless bench stdb_pump` + env-gated `bench_relay_throughput`; record p50/p95.
- [ ] Finish `mind-stdb/AGENTS.md`; add handoff notes for 21 (schema growth points, envelope freeze, `AuthorityMode` switch, retention deferred).
- [ ] Exit checklist §7.5 complete; append Changelog to this plan when execution starts.
- **Verify:** budgets §7.4 met; docs reviewed against `HIGH_LEVEL_PLAN.md` §4.

## 6. Data & formats

### 6.1 Identity/session/profile/settings/audit (server skeleton)

| Table (accessor) | Public | Key/index | Fields (plan 01) |
|---|---|---|---|
| `player` | yes | PK `identity: Identity` | `username: String`, `created_at`, `last_seen_at: Timestamp`, `protocol_version: u32` |
| `player_session` | yes | PK `session_id: u64` auto-inc; index `by_identity_started(identity, started_at)` | `identity`, `connection_id: Option<ConnectionId>` (2.10.1 `ReducerContext::connection_id()` is `Option`), `started_at`, `ended_at: Option<Timestamp>` |
| `player_profile` | yes | PK `profile_id` auto-inc; index `by_owner(identity)` | `identity`, `name: String`, `created_at` |
| `client_settings` | yes (view `local_client_settings`) | PK `identity` | `ui_scale: f32`, `language: String`, `music_volume: f32`, `sfx_volume: f32`, `keybinds_json: String`, `revision: u32`, `updated_at` |
| `protocol_info` | yes | PK `id: u8` (singleton 0) | `protocol_version: u32`, `min_client_build: u32`, `save_format_version: u32` |
| `relay_config` | yes | PK `id: u8` (singleton 0) | `commands_per_second: u32`, `command_rate_window_ms: u32`, `max_commit_commands_per_transaction: u32`, `default_map_width_tiles/height_tiles: i32` |
| `audit_log` | no (server-only) | PK `log_id` auto-inc | `at: Timestamp`, `actor: Option<Identity>`, `kind: AuditKind`, `message: String` |

Views: `local_player`, `local_player_profile`, `all_players`, `local_client_settings` (ViewContext, sentinel-query idiom from `main/server`), plus relay views in §6.2. `AuditKind` is a `SpacetimeType` enum (`Connect`, `Disconnect`, `ProfileCreate`, `MatchCreate`, `MatchJoin`, `MatchStart`, `ConfigChange`).

### 6.2 Relay tables/views

Defined in §3.8. Views: `my_matches` (member semijoin), `my_match` (single active match row per caller — highest `match_id` where Running/Lobby), `my_match_commands` (member semijoin over the private log). `CommandRate` is server-only.

### 6.3 Command payload contract (plan 01 subset)

`CommandKind::{Noop, Ping{nonce}, PlaceBlock{x,y,block_id,rotation,config}, BreakBlock{x,y}, ConfigBlock{x,y,config}}`. `block_id: u16` is an opaque content ID until plan 02 defines ID spaces and plan 21 binds real payloads; all command kinds carry no map/content data, so the envelope survives content growth. `Ping` is the round-trip probe used by tests and dev helpers; it is intentionally cheap/no-op in the sim.

### 6.4 Version pairing & pin strategy

- Installed CLI: `spacetime` 2.10.1 (`spacetimedb-cli`/`spacetimedb-lib` 2.10.1). The CLI is the generator; its version determines the generated bindings' `spacetimedb-sdk` requirement.
- Server crate: `spacetimedb = "=2.10.1"`; client SDK: `spacetimedb-sdk = "=2.10.1"`. Exact pins, lockstep. Generated bindings bring their own exact pins; do not override them.
- `PROTOCOL_VERSION` (ours) is separate from the crate version: module `main/global.rs` and `mind-stdb/src/protocol.rs` both define it; plan 23 checks the live row against the client constant. Bump both together when the envelope changes.
- `main/` pairs server crate 2.2.0 with C# 2.7.1 and CLI 2.10.1, proving cross-minor tolerance, but Rust generated bindings are pinned to the generating CLI's SDK line, so this plan pins everything to one line to remove skew.
- CI gates: `spacetime --version` == 2.10.1 (WSL dev + CI); `server/build.sh --check` (regenerate to temp dir, `git diff --exit-code module_bindings`); `cargo tree -p mind-stdb` shows a single `spacetimedb-sdk` version.

### 6.5 Generated bindings workflow

- Output path: `client/rust/mind-stdb/src/module_bindings/` (checked in, never hand-edited). Header comment generated by the CLI is authoritative; add a repo-level note in `mind-stdb/AGENTS.md`, not in the files.
- `server/build.sh` (primary, WSL/Linux): `spacetime generate --lang rust --out-dir ../client/rust/mind-stdb/src/module_bindings --module-path ./spacetimedb -y`; then `spacetime publish mindustry --delete-data -y` (default local; `--server <url>` sets project server). `build.ps1` mirrors for Windows.
- `server/spacetime.json` holds the same generate entry so `spacetime generate` from `server/` matches the script.

### 6.6 Token file format

`FileTokenStore` writes JSON: `{"schema":1,"hosts":{"http://127.0.0.1:3000__p1":"<jwt>"}}` — keys are `token_key(host, suffix)`. Path is injected: gdext resolves `user://stdb_tokens.json` via `ProjectSettings::globalize_path`; headless/tests pass a temp path. Tokens are never logged; the file is gitignored (HIGH_LEVEL_PLAN §6.5).

### 6.7 Local dev workflow & test isolation

- Start: `spacetime start` (login shell; background) → `server/build.sh` (generate + publish db `mindustry` with `--delete-data`) → run game / MCP.
- Inspect: `spacetime logs mindustry`, `spacetime sql mindustry "SELECT ..."` (small SQL subset: plain `SELECT … WHERE … LIMIT`, no `BETWEEN`/`ORDER BY`), `spacetime call` for server-side setup only; gameplay flows are driven through the client (playtest skill rule).
- Wipe semantics: every publish is a hard cut; seeds re-run from `main/seeds.rs`. Tests and demos must never assume persisted rows across publishes.
- Test isolation: unit tests are network-free. Integration tests (`MIND_STDB_IT=1`) use db `mindustry-it`; each test creates its own `RelayMatch` (random `map_seed`) and only asserts on rows it created; matches are left `Ended` for hygiene (no cross-test table resets needed). `bench_relay_throughput` is `#[ignore]`d and manual.

## 7. Oracle & verification (REQUIRED)

### 7.1 Ported tests (the C# SDK has no test suite — tests port behavior, named here)

`cargo test -p mind-stdb` (network-free):

- `identity::tests::parses_p_only_args` — `--p1`/`--p2` accepted; `--path` rejected (C# regression guard).
- `identity::tests::engine_args_win_over_user_args` — C# argument-source order preserved.
- `token::tests::key_scopes_host_and_appends_suffix` — exact C# `Replace` chain + suffix.
- `token::tests::file_store_roundtrip_is_schema_versioned`.
- `connector::tests::offline_never_builds_and_reports_offline`.
- `connector::tests::pump_without_connection_is_noop`.
- `connector::tests::disconnect_then_reconnect_rebinds` (events order: `Disconnected` → `Connected` → `Resync`).
- `waves::tests::wave_tables_are_unique_and_all_known`.
- `binder::tests::replay_emits_rows_then_live_inserts`.
- `binder::tests::event_table_binder_has_no_replay` (doc-test `compile_fail`).
- `relay::tests::orders_by_command_id_not_arrival`.
- `relay::tests::duplicate_command_id_is_ignored`.
- `relay::tests::per_sender_gap_is_flagged`.
- `relay::tests::autoincrement_numeric_gaps_are_not_loss`.
- `protocol::tests::version_mismatch_is_rejected`.

Server-side `#[cfg(test)]` helpers, typechecked via `cargo check --manifest-path server/spacetimedb/Cargo.toml --tests` (cannot link in place — same constraint as `main/`):

- `relay::methods::tests::validate_kind_rejects_out_of_bounds_and_zero_block`.
- `relay::methods::tests::rate_window_rolls_after_window`.
- `relay::methods::tests::sender_sequence_must_increase`.
- `identity::methods::tests::username_rules`.

Integration tests (`#[ignore]`, `MIND_STDB_IT=1`) and benchmarks:

- `it::local_connect_applies_base_and_lobby_waves`
- `it::two_clients_relay_ping_round_trip`
- `it::relay_rejects_non_member_and_rate_limit`
- `bench::bench_relay_throughput` (manual, §7.4)
- `bench::bench_pump_idle` (manual, §7.4)

### 7.2 Headless harness scenarios (`mind-headless`)

| Scenario | Network | What it proves | Expected assertions |
|---|---|---|---|
| `stdb_offline_boot` | none | Offline path is fully testable headless (default in CI) | state `Offline` after N pumps; no socket; pump p99 < 0.2 ms; JSON dump matches golden |
| `stdb_binder_replay` | none | Binder replay/order/cleanup against a synthetic row source | inserted rows in iteration order; no duplicate live events; drop removes callbacks |
| `stdb_command_order` | none | Ordering/dedup/gap contract with canned `MatchCommand` rows | apply order by `command_id`; dup ignored; per-sender gap flagged; auto-inc gap not flagged |
| `stdb_relay_live` (opt-in) | local STDB | Real match + ping round-trip without Godot | commands observed in order; `applied_count == sent_count`; no `order_error` |

### 7.3 MCP playtest scenario — `stdb_relay_roundtrip_2p` (concrete)

Preconditions: `spacetime start`; `server/build.sh` published to local; plan-00 MCP bridge connected; editor launched per the playtest skill and **two game instances** started with `--p1`/`--p2` (distinct identities). Run instance calls **sequentially** (playtest skill rule), pid-stamp every eval, compare to `godot_game instances`.

1. `godot_health check`, then `godot_game play` (plan-00 spine scene; instances auto-run).
2. Instance 1 eval: `{"code":"var c=get_node(\"/root/StdbConnector\"); return {\"pid\":OS.get_process_id(),\"state\":c.state(),\"id\":c.local_identity_hex()}"}` → expect `state == "connected"` and a 64-hex identity.
3. Instance 1 eval: `{"code":"return get_node(\"/root/StdbConnector\").dev_create_match(\"demo\", 1234)"}` → capture `match_id`; then `dev_join_match` is implicit for the creator; call `spacetime sql mindustry "SELECT * FROM relay_match"` to confirm the row.
4. Instance 2 eval: `... dev_join_match(<match_id>)`; confirm `my_matches` membership via `spacetime sql "SELECT * FROM relay_member"`.
5. Instance 1 eval: `... dev_send_ping(42)`.
6. Instance 2 eval (after a short wait): `{"code":"var c=get_node(\"/root/StdbConnector\"); return {\"count\":c.dev_relay_applied_count(),\"last\":c.dev_last_command_id(),\"err\":c.relay_order_error()}"}` → expect `count >= 1`, `last` non-zero, `err` empty. Same eval on instance 1 (sender sees its own row).
7. `godot_log get` on both instances → `relay command applied` lines with matching `command_id`; `spacetime sql "SELECT command_id, match_id, kind FROM match_command"` shows the ordered row(s).
8. Optional screenshot of the inspector's `net` page (evidence path recorded in the plan Changelog when execution starts).

Also run the negative path once: stop the local SpacetimeDB process (Ctrl-C on `spacetime start` or kill it), launch an instance in offline mode → game boots single-player, `disconnected`/`Offline` shown, no panic (supports §2.2 and invariant 8).

### 7.4 Performance budgets + measurement method

Baseline: 60 tps ⇒ 16.6 ms/frame; network must stay under ~1.5% of a frame.

| Metric | Budget | Measurement |
|---|---|---|
| `Connector::pump()` idle overhead | p50 ≤ 50 µs, p99 ≤ 200 µs | `mind-headless bench stdb_pump` 10k iterations after connect; report p50/p99 |
| Base wave apply (≤ 20 rows) | ≤ 3 ms one-shot | `on_applied` timestamp delta logged at debug; integration `local_connect_applies_base_and_lobby_waves` asserts |
| Lobby wave apply (< 100 rows) | ≤ 10 ms one-shot | same |
| Binder delivery | ≥ 10k rows/s sustained to queues; 100k-row replay < 100 ms | `bench_relay_apply` (ignored) |
| Relay end-to-end (local) | ≥ 200 committed commands/s for 30 s with 10 clients; p95 sender→remote-apply ≤ 150 ms; 0 gap/dup errors | `bench_relay_throughput` (ignored, `MIND_STDB_IT=1`) |
| Command row cost | ≤ 2 KB/row encoded; queue cap 20k rows (overflow → drop + `Resync` flag) | integration assert + row size from `spacetime sql` |

Regressions block the P1 gate (HIGH_LEVEL_PLAN §7.4/§5).

### 7.5 Exit criteria checklist

- [x] `cargo fmt --check`, `cargo clippy -p mind-stdb -p mind-gdext -p mind-headless` clean; `cargo test -p mind-stdb` green network-free.
- [x] `cargo check --manifest-path server/spacetimedb/Cargo.toml --tests` green.
- [x] `server/build.sh --check` shows no generated-binding drift; no hand edits.
- [x] Version pins lockstep at 2.10.1 (`spacetime --version` = 2.10.1; bindings header 2.10.1).
- [x] Module publishes locally; seeds verified via `spacetime sql`.
- [x] Base+lobby waves apply; `WaveApplied(Lobby)` observed in the integration test.
- [ ] `stdb_relay_roundtrip_2p` MCP scenario passes with evidence; negative offline path passes. **Deferred to the orchestrator after merge** (editor points at the main worktree); code + helpers are in M5.
- [ ] Perf budgets §7.4 met and recorded. **Partial:** `bench stdb_pump` p50 191 ns / p99 611 ns (budgets 50 µs / 200 µs); base/lobby apply < 3/10 ms observed in the IT (≤ 20 s budget timeouts, no timing assert); relay throughput bench deferred.
- [x] Invariants §3.12 verified by test (1–4, 6–8) or review (5).
- [x] `mind-stdb/AGENTS.md` complete; handoff notes for 21 written; plan Changelog started.

## 8. Risks & open decisions

| # | Risk / decision | Default taken | Status |
|---|---|---|---|
| R1 | **Rust SDK API uncertainty.** Exact generated names (`DbConnection::frame_tick`, typed `add_query` on view accessors, table accessor marker types, `credentials` module) verified only against 2.10.1 docs, not compiled yet. | Use the documented 2.10.1 API; isolate every SDK touch point in `connector.rs`/`waves.rs`/`binder.rs`; keep a raw-SQL `subscribe(&[...])` fallback inside `waves.rs`. First M2 task is a spike compiling `frame_tick` + one table subscription. | Monitor; fix per 2.10.1 release notes |
| R2 | **Pump mode** (main-thread `frame_tick` vs `run_threaded` + channel). | Main-thread `frame_tick` (C# parity), queue-based binders make the swap a feature flag if budget fails. | Default; fallback pre-designed |
| R3 | **Server module name / db name.** Not covered by HIGH_LEVEL_PLAN. | **Locked 2026-10-01 (NUD-08; final user decision): crate `mindustry_godot`, local db `mindustry`, integration db `mindustry-it`.** Scripts take `--db`/`--server` overrides (`.ps1`: `-Db`/`-Server`). | locked |
| R4 | **Command envelope representation** (typed `CommandKind` enum vs generic `type:u16 + bytes`). | Typed enum with a small fixed envelope; plan 21 owns the full variant set and the schema hard-cut. | **NEEDS USER DECISION** (affects 21's schema-freeze point) |
| R5 | **Generated bindings are checked in.** | Yes, with a `--check` drift gate (`.ps1` twin: `-Check`); plan 00's CI applies it. | Reconcile with `00_FOUNDATION_IMPLEMENTATION_PLAN.md` CI design |
| R6 | **Version pin =2.10.1 vs main's server 2.2.0/C# 2.7.1.** | Lockstep 2.10.1 for Rust (generated bindings pin it). | Reconcile with plan 21; flag if it prefers tracking main |
| R7 | **Rejection auditing.** A reducer returning `Err` rolls back its writes, so failures cannot write `AuditLog`. | Failures go to `log::warn!`; only committed actions audit. Plan 21 may add a staged rejection path (needs a second transaction trigger). | Documented limitation |
| R8 | **Command-log retention.** No pruning in plan 01; 200 cmd/s ≈ 17 MB/day worst case (2 KB/row). | Plan 21 adds snapshot + prune; queue cap 20k rows protects clients. | Deferred to 21 |
| R9 | **Admin/bootstrap model.** Plan 01 has no admin slot; `audit_log` is server-only. | Plan 21 owns admin/ban/whitelist; seeds need no admin. | Deferred to 21 |
| R10 | **Map bounds/coords in validation.** Mindustry map limits not yet defined in this repo. | `RelayMatch.map_width_tiles`/`height` stored at creation (default 1000×1000 from `relay_config`); plan 06/21 pass real values. | Default |
| R11 | **Token store location/format.** | Injected path; JSON with schema version; `user://` in Godot, temp dir in tests; SDK's `credentials` helper not reused. | Default |
| R12 | **GDScript row access.** No `Variant` conversion for generated rows. | `StdbBinder` exposes debug JSON only; typed UI consumers live in Rust (D1). Plan 14 may add per-table conversions. | Default; note for 14 |
| R13 | **`my_match_commands` view semijoin correctness/perf on large logs.** A caller in many matches gets all their commands. | Skeleton enforces one active match per client in practice; plan 21 may add a parameterized per-match subscription (OD3 transport). | Monitor in M4 integration tests |
| R14 | **Two-instance MCP scenario depends on plan 00's launch-arg support** (`--p1`/`--p2` as user args reaching gdext). | Match C# parsing order; add a smoke test at M5; plan 00's runbooks carry the launch args. | Reconcile with `00_FOUNDATION_IMPLEMENTATION_PLAN.md` |

## 9. References

Exact files/dirs read for this plan (2026-10-01):

- `C:\Users\Clinton\g\code_examples\mindustry-godot\HIGH_LEVEL_PLAN.md` (§0 locked decisions, §2 architecture, §3 plan set, §4 template, §6–§9 conventions/verification/parity ledger).
- `C:\Users\Clinton\g\code_examples\mindustry-godot\PRELIMINARY_PLAN.md` (historical intent).
- `C:\Users\Clinton\g\code_examples\mindustry-godot\client\sstdbsdk\AGENTS.md`, `DatabaseConnector.cs`, `TableSubscriber.cs`, `TableBinderComponent.cs`, `DatabaseConnector.tscn`, `README.md`, `pointers.md`.
- `C:\Users\Clinton\g\main\AGENTS.md` (project-wide client/server map, publish workflow) and `C:\Users\Clinton\g\main\server\AGENTS.md` (SpacetimeDB Rust rules, canonical shapes, views, indexes, commands).
- `C:\Users\Clinton\g\main\server\spacetimedb\Cargo.toml`, `src\lib.rs`, `src\main\{mod,lifecycle,seeds,global}.rs`, `src\item\{reducers,views}.rs`, `src\chat\mod.rs` (2.x layout, seed trait, rate window, view/semijoin idioms).
- `/mnt/c/Users/Clinton/g/main/server/{build.sh,spacetime.json}` (publish/generate pattern being adapted to Rust).
- `/mnt/c/Users/Clinton/g/.opencode/skills/playtest/SKILL.md` (MCP + `spacetime` CLI verification rules).
- `spacetime generate --help` / `spacetime --help` (CLI 2.10.1 confirms `--lang rust`).
- crates.io API: `spacetimedb-sdk` 2.10.1 and `spacetimedb` 2.10.1 version/pin data; docs.rs `spacetimedb_sdk` 2.10.1 crate page and the SpacetimeDB Rust client reference (`DbConnection`, `DbContext`, `frame_tick`/`run_threaded`/`run_async`, `SubscriptionHandle`, `Table`/`TableWithPrimaryKey` callbacks, query builder).

## Legacy C# migration notes (captured at plan-00 M6, 2026-10-01 — source tree deleted)

Deleted at M6 (NUD-05=C, no archive): `client/Scripts/Components/` (`IComponent.cs`, `Component.cs`, `AreaComponent.cs`, `Node2DComponent.cs`, `Node3DComponent.cs`, `ControlComponent.cs`, `VisualComponent.cs`, `ComponentRegistration.cs`, `NodeExtensions.cs`, `README.md`; the `client/Scripts/Entities/` files the README references — `IEntity`, `Entity`, `EntityRegistry` — were already absent from this copy) and `client/sstdbsdk/` (`DatabaseConnector.cs` + `.tscn`, `TableSubscriber.cs`, `TableBinderComponent.cs`, `AGENTS.md`, `README.md`, `pointers.md`).

- **Token key + persistence** (`DatabaseConnector.Connect()`, `_authTokenKey`): `Host.Replace("://","_").Replace(":","_").Replace("/","_")` + `TokenAppend`; `--pN` parsed from `OS.GetCmdlineArgs()` then `GetCmdlineUserArgs()` with exact `^--p\d+$` (`IsPlayerArg`, rejects Godot's `--path`). Replace chain/parser already covered by 01 §4 (`DatabaseConnector.cs` row) + §3.3 (`identity`, `token`). **Delta vs 01 §6.6:** the M5 lane found the 2.10.1 token layout is `<data-dir>/identity/<key>.token.json` per host+suffix (00 §3.7/M5), not 01 §6.6's single `stdb_tokens.json` map — reconcile in 01 M2. Rust owner: `mind-stdb/src/{token,identity}.rs` (01 §3.3).
- **Subscription waves** (`TableSubscriber`): legacy lists were origin-bullethell content — Base `{AllTextures, AllItems, AllEnchantments, AllAnimations}`; Lobby `{LocalLobbyPlayer, LocalPlayerProfiles}`; Game ~30 names (`LocalPlayer*`, `Nearby*`, `EnemyTemplates`, `BulletPatternEvent`, `BulletControlEvent`, `AbilityVisualEvent`, `MapConfig`, `ActiveStatusEffect`, `ActiveEnemyStatusEffect`, `GroundZone`, `AbilityCharge`, `ChanneledAction`, `LocalPlayerKnowledge`, `LocalTradeSession`, `AllTerritories`, `AllChatChannels`, `LocalChatMessages`; per-entry comments tagged persistent-replay-OK vs event-replay-OFF). Timing (base+lobby auto on connect; base has no applied callback; lobby raises `LobbyApplied` + `LobbySubActive`; game explicit via `SubscribeGame()`/`UnsubscribeGame()`; names resolved by reflection over the generated `From` API) already covered by 01 §3.6; origin-only tables/`MapConfig`/`LapQ`/`LapR` dropped per 01 §4. Rust owner: `mind-stdb/src/waves.rs` (01 §3.6).
- **Binder row semantics** (`TableBinderComponent`): insert/update/delete set `LastRow`/`LastOldRow`/`LastDeletedRow` and emit arg-less `RowInserted`/`RowUpdated`/`RowDeleted`; event tables lack `OnUpdate`/`OnDelete` and the hook-up skips them; `ReplayExistingRows` iterates the cache after live hooks are installed; `_ExitTree` unhooks every event; connection reached via `DatabaseConnector.Instance` with the already-connected guard; inspector dropdown from `AllSubscribedTables`. Rust `RowChange<T>`, primary-key-only `replay_existing`, event-table insert-only specialization and drop cleanup already covered by 01 §3.5 + §4 (no `IEntity` dependency). Rust owner: `mind-stdb/src/binder.rs`.
- **Frame pump** (`DatabaseConnector._Process`): `Conn?.FrameTick()` exactly once per frame in try/catch — a throwing handler is logged but pumping continues; without it nothing dispatches. Already covered by 01 §3.4 (main-thread `pump()` → `frame_tick()`; `Err` → `ConnectorEvent::Disconnected`; never panics/unwraps). Rust owner: `mind-stdb/src/connector.rs`.
- **Reconnect/backoff**: C# had no retry loop or backoff — `OnDisconnected` emitted `Disconnected` unless `_shuttingDown` (set by `_ExitTree`); consumers called `Connect()` again and rebuilt the scene (C# rule 10) because binders stayed attached to the dead `DbConnection`. New behavior (`ConnectPolicy { auto_reconnect, backoff }`, connector-owned rebind + `Resync`, no scene rebuild) already covered by 01 §2.4 + §3.4; preserve the "no `Disconnected` event on deliberate `disconnect()`" suppression in `connector.rs`.
- **Component framework → Bevy ECS**: `IComponent`/`IEntity` interfaces; `EntityRegistry` (component list + Type→component cache, first match wins, order-dependent); `ComponentRegistration.Register` (nearest-ancestor walk, `PushError` when none) / `ValidateRequired`; six bases (`Component`:Node, `AreaComponent`:Area2D, `Node2DComponent`:Node2D, `Node3DComponent`:Node3D, `ControlComponent`:Control, `VisualComponent`:AnimatedSprite2D) enforcing `_Ready` register + deferred `OnEntityReady` (after all siblings — the guarantee `ReplayExistingRows` fires after parent signal wiring) + `_ExitTree` unregister, with virtual `OnRegistered`/`GetRequiredComponents`/`GetSibling<T>`; `NodeExtensions.GetAncestor<T>` for cross-scene lookup. Bevy ECS (plan 00) replaces this with entity IDs + component data in `mind-core`: no node-ancestor coupling, no type-cache registry, no per-native-root base duplication, no sibling lookup; only GDScript-authored static scene structure remains in Godot. `sstdbsdk` drops `: Component`/`IEntity` per 01 §4 + §3.5; Rust owners `mind-stdb/src/{connector,waves,binder}.rs` (networking) and `mind-core` ECS (gameplay).
- **STDB 2.10.1 deltas recorded by the M5 lane (not yet in 01):** `ctx.db.<table>()` accessors require their accessor-trait imports; publish wipe is `--delete-data=always`; schema inspection is `spacetime describe <db> --json`; `spacetime generate` requires the `wasm32-unknown-unknown` target; database names reject underscores (`^[a-z0-9]+(-[a-z0-9]+)*$`), so the final names are local db **`mindustry`** and integration db **`mindustry-it`** while crate/module stays `mindustry_godot` — supersedes 01 §3.11/§6.5/§6.7/R3 literals; token file layout per the token bullet above. Apply when executing 01 M0/M2.

## Changelog

### 2026-10-01 — lane 01 (worktree `mindustry-godot-lane01`, branch `lane/01-stdb`)

#### M0 — Workspace, crate skeletons, bindings pipeline (commit `01-M0`)

- Verified the plan-00 skeleton against §3.1/§3.2: workspace member `mind-stdb` with `spacetimedb-sdk = "=2.10.1"` pin, separate `server/spacetimedb` crate (`mindustry_godot`, cdylib, `spacetimedb = "=2.10.1"`), `server/spacetime.json` generate entry, `build.sh`/`build.ps1` (`--check`/`-Check` drift mode), checked-in `module_bindings/`.
- Added `client/rust/mind-stdb/AGENTS.md`: generated-bindings never-hand-edit rule + drift gate, the five-step recipe (table → reducer → wave entry → binder + handler → reducer call), the ported C# rules and `stdb_*` verify commands.
- Added `client/scenes/autoloads/stdb_connector.tscn` (native `StdbConnector` root node; scene is inert until the class lands in M5). The `project.godot` autoload registration deliberately moves to M5: registering a scene whose native class does not exist yet would break boot before mind-gdext M5 lands.
- Removed the unused direct `tokio` dependency from `mind-stdb` (§3.2: no tokio in the public API; the SDK owns its runtime).
- Evidence:
  - `cargo check --manifest-path client/rust/Cargo.toml -p mind-stdb` → `Finished dev profile`
  - `cargo check --manifest-path server/spacetimedb/Cargo.toml` → `Finished dev profile`
  - `server/build.sh --check` → `== check: bindings are drift-clean ==`
  - `spacetime --version` → `spacetimedb tool version 2.10.1; spacetimedb-lib version 2.10.1`; `wasm32-unknown-unknown` target installed.

#### M1 — Server skeleton (commit `01-M1`)

- Module layout per §3.7: `main/{global,seeds,audit,tables,lifecycle}.rs`, `identity/{tables,methods,reducers,views}.rs`; the P0 `main/{tables,reducers}.rs` were replaced.
- Tables: `player` (`last_seen_at`, `protocol_version`), `player_session` (open/closed with `by_identity_started`), `player_profile` (`by_owner`), `client_settings`, `protocol_info`, `relay_config` (both singleton `id = 0`), server-only `audit_log`.
- Views: `local_player`, `local_player_profile` (latest profile), `all_players` (query-style), `local_client_settings`.
- Reducers: `set_username`, `create_profile`, `update_client_settings` — all `ctx.sender()`-owned, cheap validation, committed actions audited (`ProfileCreate`, `ConfigChange`); lifecycle connect/disconnect audited.
- Plan deltas recorded: §6.1 `player_session.connection_id` is `Option<ConnectionId>` (2.10.1 context API); 2.10.1 view accessor traits are `<accessor>__view` / query trait `<accessor>__query` and `AnonymousViewContext` returns `impl Query<T>` for full-table views (documented in code comments).
- Evidence:
  - `cargo check --manifest-path server/spacetimedb/Cargo.toml --tests` → `Finished dev profile` (validation `#[cfg(test)]` helpers typecheck).
  - `server/build.sh` → published local `mindustry`; `== done: bindings in .../module_bindings ==`.
  - `spacetime sql mindustry --server local 'SELECT * FROM protocol_info'` → `0 | 1 | 1 | 1`; `relay_config` → `0 | 600 | 1000 | 256 | 1000 | 1000`.
  - `audit_log` shows `init: seeded protocol_info and relay_config`, then connect/disconnect/connect rows with the CLI identity; `player`/`player_session` show the created row and closed sessions.
  - `spacetime call --server local mindustry update_client_settings 1.0 '"en"' 0.5 0.25 '"{}"'` then `SELECT … FROM client_settings` → `1 | "en" | 0.5 | 0.25 | 1`.
  - `SELECT identity, username FROM all_players` works (view query); `server/build.sh --check` → drift-clean; `cargo check -p mind-stdb` still green against the regenerated bindings.
- Divergence note: 2.10.1 *table* accessor traits need `use spacetimedb::Table` for `insert`/`iter`; *view* code needs `<accessor>__view` (and `<accessor>__query` for query-style views). Recorded in `identity/views.rs` comments; no plan text change beyond the §6.1 cell above.

#### M2 — `mind-stdb` connector core (commit `01-M2`)

- Replaced the P0 `conn.rs`/`tokens.rs` facade with the plan §3.3 modules: `config` (`StdbMode`, `ConnectPolicy`, `Backoff`, injected `token_store_path`), `identity` (`LocalIdentity`, exact `^--p\d+$` parser with engine-args-first order), `token` (`TokenStore` trait + `FileTokenStore` per-host+suffix schema-versioned files, C#-exact key chain), `connector` (`Connector` + `ConnectorState::{Offline,Idle,Connecting,Connected,Retrying,Disconnected}` + `ConnectorEvent` incl. `Resync`), `protocol` (`PROTOCOL_VERSION`/`CLIENT_BUILD`/`check_protocol`). `waves.rs` renamed `SubscriptionWave` → `WaveName` and now lists the real Base/Lobby accessors.
- Pump contract: `pump()` calls `frame_tick()` once, drains the callback queue, then retries/backoff; callbacks only push into an `Arc<Mutex<VecDeque>>`; deliberate `disconnect()` suppresses the `Disconnected` event; SDK errors never panic.
- Token layout reconciled with the legacy-notes finding (`<data-dir>/identity/<key>.token.json`, schema 1) instead of §6.6's single-map file; the §6.6 text is superseded by the legacy-notes bullet (code carries the reconciliation comment).
- `mind-gdext::sim_host` `--db` facade migrated to `Connector::pump()` (no structural change; M5 hands the connection to the autoload).
- New `mind-headless` `stdb_scenarios` runner + `scenarios/stdb_offline_boot.json`; `list` shows it.
- Evidence:
  - `cargo test --manifest-path client/rust/Cargo.toml -p mind-stdb` → `13 passed; 0 failed` (identity/token/offline/pump state tests listed in §7.1 minus the M3+ ones).
  - `cargo run -p mind-headless -- run stdb_offline_boot --json` → `{"state":"offline","frames":64,"pumps":64,"pump_p50_ns":50,"pump_p99_ns":1784,"pass":true,...}` (p99 budget 200 µs met).
  - `cargo fmt -p mind-stdb -p mind-headless -- --check` clean; `cargo clippy -p mind-stdb -p mind-headless --all-targets -- -D warnings` clean; `cargo check -p mind-gdext` green.
  - Environment note (machine-local, not committed): WSL's `~/.cargo/config.toml` OpenSSL workaround had broken `libcrypto.so.3`/`libssl.so.3` symlinks, so rust-lld fell back to static `libcrypto.a` and failed on zstd symbols. Repaired the two symlinks to the system `libcrypto.so.3`/`libssl.so.3`; `cargo test` (which links the SDK's native-tls) now works. CI runners with `libssl-dev` are unaffected.

#### M3 — Waves + typed binders (commit `01-M3`)

- `waves.rs`: `SubscriptionWaves` (desired survives disconnects, applied cleared; Base+Lobby desired by default) + `WaveEvent`; connector issues the typed query-builder waves (`add_query(|q| q.from.<accessor>())`), Base without an applied callback, Lobby/Game with `on_applied`/`on_error` queueing internal events; `is_applied`/`subscribe_*`/`unsubscribe_*` surface.
- `binder.rs`: `TableBinder<A>` owns an `Arc<BinderCore>` queue fed by callbacks capturing only `Weak`; `drain()`, `replay()`, `inject()` (doc-hidden scenario hook). `replay_existing(bool)` exists **only** when the generated handle implements `TableWithPrimaryKey` (doc-test `compile_fail` proves the property on the `local_player` view). `Connector::bind` (live insert/delete) and `Connector::bind_with_replay` (adds update callbacks + cache replay after registration, C# order).
- Connector `on_connected` now re-issues desired waves and re-registers every live binder before emitting `Connected` + `Resync`; `binder_tables()` exposes diagnostics; wave members not in any static list warn at bind time (§3.12 invariant 5).
- **Implementation divergence recorded (plan §3.5):** callback IDs are not stored for `_ExitTree`-style unhooking. SDK callback IDs are table-typed, and a dropped binder has no connection handle; instead callbacks capture `Weak<BinderCore>` (dropped binder ⇒ inert no-op, queue freed) and registrations die with the connection on disconnect/reconnect. Same observable contract (no leak, no stale delivery), simpler ownership. Documented in the `binder.rs` module docs.
- Evidence:
  - `cargo test -p mind-stdb` → `18 passed; 0 failed` + `1 passed` doc-test (the `compile_fail` event-table/replay property).
  - `cargo run -p mind-headless -- run stdb_binder_replay --json` → `{"pass":true,"replay_order":[1,2],"live_order":[3],...}`; `stdb_offline_boot` still `pass: true`.
  - `server/build.sh --db mindustry-it` created the integration DB; `MIND_STDB_IT=1 cargo test -p mind-stdb -- --ignored local_connect_applies_base_and_lobby_waves` → `1 passed` (base+lobby waves applied, `WaveApplied(Lobby)` observed, `local_player` insert delivered, `relay_config` cache replay delivered).
  - `cargo fmt -p mind-stdb -p mind-headless -- --check` clean; `cargo clippy -p mind-stdb -p mind-headless --all-targets -- -D warnings` clean.
- Deferred: `stdb_command_order` (M4, needs `match_command`).

#### M4 — Relay foundation (commit `01-M4`)

- Server: `relay/{tables,methods,reducers,views}.rs` per §3.7 — `relay_match`/`relay_member`/private `match_command`/server-only `command_rate`; `create_match` (creator auto-joins, bounds from `relay_config`), `join_match`, `leave_match`, `start_match`, `send_match_command`; `my_matches`/`my_match`/`my_match_commands` per-caller views; rejection paths log via `log::warn!` (R7).
- Client: `relay.rs::CommandStream` (sort by `command_id`, dedup by `command_id`, per-sender `sender_seq` gap detection, `applied_count`/`last_command_id`/`order_error`, synthetic `inject` hook); connector reducer methods (`create_match`/`join_match`/`leave_match`/`start_match`/`send_match_command`/`send_ping`); Game wave now subscribes `my_match` + `my_match_commands`; Lobby wave gained `my_matches` (plan §3.6 listed it; missed in the M2/M3 list, fixed here).
- Divergences recorded in §3.8/§3.9 above: 2.10.1 newtype `CommandKind` payloads; `RelayMember.by_identity` index for the per-caller views; server-assigned `sender_seq`.
- New headless scenario `stdb_command_order` (canned rows): order-not-arrival, duplicate ignored, per-sender gap flagged, auto-inc gaps not loss.
- Evidence:
  - `cargo test -p mind-stdb` → `23 passed; 0 failed` + doc-test `1 passed`; the 3 integration tests stay `ignored`.
  - `cargo run -p mind-headless -- run stdb_command_order --json` → `{"pass":true,"applied_order":[7,8,9],"duplicate_ignored":true,"order_error":"gap: expected 2, got 3"}`.
  - `server/build.sh` + `server/build.sh --db mindustry-it` published; `server/build.sh --check` → drift-clean; `spacetime describe mindustry --server local --json` lists 11 tables (incl. `match_command`, `command_rate`) and 7 views (incl. `my_match`, `my_matches`, `my_match_commands`).
  - `MIND_STDB_IT=1 cargo test -p mind-stdb -- --ignored` → `3 passed`: base/lobby waves, two-client ping round trip (both peers apply the command, no order error), non-member rejection + rate limit (700 sent in one burst, 600 accepted; 100 `rate limit exceeded` warnings in `spacetime logs`).
  - `cargo check --manifest-path server/spacetimedb/Cargo.toml --tests` clean; `cargo fmt --check` and `cargo clippy -p mind-stdb -p mind-headless --all-targets -- -D warnings` clean.
- Test-rig note: SDK reducer sends flush while their own connection is pumped (`frame_tick`), so the ITs pump both peers; documented in `tests/it.rs`.

#### M5 — gdext autoload + binder node + net page (commit `01-M5`; in-engine run deferred)

- `mind-gdext/src/stdb.rs`: `StdbConnector` (autoload Node: exports host/db/offline/token-suffix, `_process` pumps exactly once and emits `connected`/`disconnected(reason)`/`connect_error`/`wave_applied`/`wave_error`/`resync`, GDScript methods `state`/`local_identity_hex`/`connect`/`reconnect`/`disconnect`/`subscribe_game`/`unsubscribe_game`/`wave_applied_state`, dev helpers `dev_create_match`/`dev_match_id`/`dev_join_match`/`dev_start_match`/`dev_send_ping`/`dev_relay_applied_count`/`dev_last_command_id`/`relay_order_error`); `StdbBinder` (attach/drain protocol with the autoload, arg-less `row_inserted`/`row_updated`/`row_deleted`, `last_row_json`/`last_deleted_row_json`).
- `mind-stdb/src/rows.rs`: `RowView` debug JSON for `ProtocolInfo`/`RelayConfig`/`Player`/`RelayMatch` (subset; generated code untouched).
- `client/scenes/autoloads/stdb_connector.tscn` now sets the local-dev exports; `project.godot` registers `StdbConnector="*res://scenes/autoloads/stdb_connector.tscn"` (M0 item closed). Online mode = `--db` or a parsed `--pN` **or** the scene `offline=false`; the default stays offline so boot never requires a server (§3.12 invariant 8).
- Inspector `net` page added to `client/scenes/ui/state_inspector.tscn` + `client/ui/state_inspector.gd` (state, identity prefix, wave flags, relay counters). Plan 14 owns the real UI surfaces.
- `sim_host.rs`: the P0 `--db` facade was removed — the autoload is now the single pump per process (§3.12 invariant 7).
- **Deviation from §7.3:** `dev_create_match` cannot return the server-assigned `match_id` synchronously (reducers return no data); the MCP flow is `dev_create_match` → poll `dev_match_id()` → `dev_start_match` → `dev_send_ping`. Recorded here for the orchestrator's §7.3 run.
- Evidence (in this worktree):
  - `cargo check -p mind-gdext` green; `cargo build -p mind-gdext` links `libmind_gdext.so` (247 MB debug).
  - `cargo fmt -p mind-gdext -p mind-stdb -p mind-headless -- --check` and `cargo clippy … --all-targets -- -D warnings` clean.
  - In-engine/MCP run not performed here (editor points at the main worktree); see handoff notes.

#### M6 — Perf, docs, handoff (commit `01-M6`, partial)

- `mind-headless bench --scenario stdb_pump --ticks 10000` implemented
  (`stdb_scenarios::bench_pump`, pure pump path, budget p99 ≤ 200 µs):
  `{"scenario":"stdb_pump","ticks":10000,"p50_ns":191,"p99_ns":611,"p50_us":1,"p99_us":1,"baseline_status":"ok"}`.
- `mind-stdb/AGENTS.md` gained the plan-21 handoff section (envelope freeze,
  schema growth points, `AuthorityMode` switch, retention/`applied_ids` cut,
  per-match subscription, one-pump rule, Game-wave bind timing, IT rig notes).
- Exit checklist §7.5 updated with what is done and the two deferrals (MCP run,
  relay throughput bench) that need the merged worktree/editor.
- Remaining for the integrator: run the §7.3 MCP scenario (`dev_create_match` →
  poll `dev_match_id` → `dev_start_match` → `dev_send_ping` → counters on both
  instances) and the ignored `bench_relay_throughput` once a load harness
  exists (plan 21 owns the 10-client generator).


