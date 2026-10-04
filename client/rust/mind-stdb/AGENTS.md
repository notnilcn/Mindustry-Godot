# AGENTS.md — mind-stdb (Rust SpacetimeDB client)

`mind-stdb` is the SpacetimeDB client SDK for Mindustry-Godot: connection
lifecycle, token persistence, subscription waves, typed table binders, protocol
handshake, ordered match-command relay and the multiplayer session state
machine. `mind-gdext` wraps it (`StdbConnector`/`StdbBinder`); `mind-headless`
drives it for the `stdb_*` scenarios; `mind-core` never depends on it.
Read the root [`AGENTS.md`](../../../AGENTS.md) first. See also
[`client/AGENTS.md`](../../AGENTS.md) and `client/rust/AGENTS.md`.

## Layout

| Path | Responsibility |
|---|---|
| `src/lib.rs` | Crate facade and public re-exports; `MIND_VERSION`. |
| `src/config.rs` | `ConnectionConfig`, `StdbMode`, `ConnectPolicy`, `Backoff`, `DEFAULT_HOST`, `DEFAULT_DB_NAME`. |
| `src/identity.rs` | `LocalIdentity`, `is_player_arg`, `parse_player_suffix`, `parse_player_suffix_from`. |
| `src/token.rs` | `TokenStore`, `FileTokenStore`, `token_key`, `default_token_dir`, `TOKEN_SCHEMA`. |
| `src/connector.rs` | `Connector` state machine, `ConnectorEvent`, `pump`, wave issue, binder registration, typed reducers. |
| `src/waves.rs` | `WaveName`, `BASE_TABLES`/`LOBBY_TABLES`/`GAME_TABLES`/`SNAPSHOT_TABLES`, `SubscriptionWaves`, `WaveEvent`. |
| `src/binder.rs` | `TableBinder<A>`, `BinderOptions`, `RowChange<T>`, live callback registration. |
| `src/relay.rs` | `CommandStream` ordered `my_match_commands` delivery, `OrderError`. |
| `src/protocol.rs` | `PROTOCOL_VERSION`, `CLIENT_BUILD`, `check_protocol`, `ProtocolError`. |
| `src/checksum.rs` | `ChecksumMonitor`, `ChecksumReport`, `compare_at_command_id`, `host_canonical_correction`, `scope_bits`. |
| `src/command_ring.rs` | `CommandRing`, `RingEntry`, bounded tail replay (`REPLAY_TAIL`). |
| `src/commands.rs` | `CommandSender`, `PredictionQueue`, `preflight_validate`, `predict_policy`, cap constants. |
| `src/session.rs` | `MatchSession`, `SessionState`, `HostParams`, `SessionError`. |
| `src/snapshot.rs` | `SnapshotHeader`, `DynamicSnapshot`, chunk split/reassemble, `SnapshotProgress`. |
| `src/transport.rs` | `RelayTransport` seam, `StdbTransport`, `CommandDelivery`. |
| `src/rows.rs` | Hand-written `RowView::debug_json` projections for the gdext debug surface. |
| `src/module_bindings/` | **GENERATED** row/reducer/view bindings; never hand-edit. |
| `tests/it.rs` | `#[ignore]`d, `MIND_STDB_IT=1`-gated live integration tests. |
| `Cargo.toml` | Crate manifest; deps `spacetimedb-sdk`, `serde`, `serde_json`, `log`, `thiserror`. |

## Hard rules

1. **Godot-free and tokio-free public API.** No `godot` dependency and no
   `tokio` type in the public surface. The SDK owns its transport runtime
   internally; `pump()` calls `DbConnection::frame_tick()` from the main thread.
2. **`module_bindings/` is GENERATED.** Never read it as documentation and never
   hand-edit it. Regenerate with `server/build.sh`; the drift gate is
   `server/build.sh --check`.
3. **`cargo test -p mind-stdb` is network-free.** Live tests are `#[ignore]`d and
   env-gated by `MIND_STDB_IT=1`; they use database `mindustry-it`, never
   `mindustry`.
4. **Offline mode never constructs a `DbConnection`.** With `StdbMode::Offline`
   `connect()` keeps `ConnectorState::Offline` and `pump()` is a no-op, so the
   game boots and plays single-player without a server.
5. **Tokens are never logged.** Token files are per host+suffix under the
   injected data directory and are gitignored.

## Invariants

- `command_id` and other server auto-increment IDs are not sequential; numeric
  gaps are normal. Loss is detected only per sender via `sender_seq` continuity
  (`CommandStream` ⇒ `OrderError::Gap`); duplicates are ignored by `command_id`.
- The server row is the truth; mirrored state is never mutated optimistically.
- Subscription waves are explicit static lists, never reflection or SQL built at
  runtime. Every table lives in exactly one wave; binding a table absent from all
  waves logs a warning. Reconnection re-issues desired waves, re-registers every
  live binder and emits `ConnectorEvent::Resync`.
- SDK callbacks only push into a shared queue; consumers observe changes only
  after `pump()`. Nothing here blocks or touches thread-local state, and `pump()`
  never panics (SDK errors become `ConnectorEvent::ConnectError`/`Disconnected`
  and move the state to `Retrying` or `Disconnected`).
- `replay_existing` exists only for primary-key handles (`TableWithPrimaryKey`);
  event tables are insert-only at the type level.

## Rules

- `ConnectorState`: `Offline` → `Idle` → `Connecting` → `Connected`, with
  `Retrying` and `Disconnected` for teardown. `disconnect()` is deliberate and
  emits no `Disconnected` event; `reconnect()` drops and reopens the link.
- `Backoff` delay is `initial_ms * 2^attempt`, capped at `max_ms` (default
  `Backoff::new(500, 30_000)`); `ConnectPolicy` defaults to `auto_reconnect`.
- `ConnectionConfig` defaults to `DEFAULT_HOST` (`http://127.0.0.1:3000`),
  `DEFAULT_DB_NAME` (`mindustry`), `StdbMode::Offline`, and `token_append`
  carrying the on-disk `--pN` suffix (e.g. `_p1`).
- `is_player_arg` is the exact `^--p\d+$` test and rejects Godot's `--path`;
  `parse_player_suffix` maps `--p1` → `_p1`; engine args precede user args.
- `token_key` is `host.replace("://", "_").replace([':', '/'], "_")` + suffix;
  `FileTokenStore` roots at `<data-dir>/identity/` and writes schema-versioned
  `<key>.token.json` files (`TOKEN_SCHEMA = 1`).
- `WaveName` is `Base | Lobby | Game | Snapshot`; `Base` and `Lobby` are desired
  by default and re-subscribed on connect. `Base` has no applied callback, so
  late binders replay the cache; `Game`/`Snapshot` toggle explicitly.
- `CommandStream::drain` sorts by `command_id`, drops other matches and
  duplicates, and records the first `OrderError`. `CommandRing` retains the last
  `REPLAY_TAIL` (4096) commands for `tail_after`/`covers_tail` replay.
- `PROTOCOL_VERSION = 2` and `CLIENT_BUILD = 1`; `check_protocol` rejects version
  mismatches and clients older than `ProtocolInfo::min_client_build`.
- `CommandSender::send` stamps `client_tick = sim_tick + INPUT_DELAY_TICKS`,
  assigns a 1-based `sender_seq` and records a prediction; `preflight_validate`
  mirrors the server's cheap caps and `predict_policy` classifies each variant.

## The new-feature recipe

Adding a networked table/reducer/binder follows the same five steps:

1. **Table** — declare the row in `server/spacetimedb/src/<system>/tables.rs`
   with `#[table(accessor = ..., public)]` (or a `#[view]` for per-caller
   projections). Public table/view/field names are schema ABI: append-only.
2. **Reducer** — add the deterministic write path in
   `server/spacetimedb/src/<system>/reducers.rs`. `ctx.sender()` is the only
   principal; a reducer returns no data (`Result<(), String>` at most) and does
   cheap validation only.
3. **Wave entry** — add the generated accessor name to exactly one static list in
   `src/waves.rs` (`BASE_TABLES`/`LOBBY_TABLES`/`GAME_TABLES`/`SNAPSHOT_TABLES`).
   A binder for a table in no wave warns at bind time.
4. **Binder + handler** — bind through `Connector::bind::<...Accessor>()`, drain
   `RowChange<T>`s after `pump()` and handle them. Use `bind_with_replay` (and
   `replay_existing`) only on primary-key tables.
5. **Call the reducer** — expose a typed method on `Connector` that calls the
   generated reducer via `RemoteReducers` (`send_match_command` for relay
   commands). Never mutate mirrored state optimistically: the server row is the
   truth.

## Verification

```bash
server/build.sh --check                                      # bindings drift
cargo test -p mind-stdb                                      # network-free
cargo clippy -p mind-stdb --all-targets -- -D warnings       # lints
MIND_STDB_IT=1 cargo test -p mind-stdb -- --ignored          # live, db mindustry-it
```

Regenerate bindings and publish with `server/build.sh` after a schema change. The
live suite covers connect + Base/Lobby waves, replay, a two-client ping round
trip, player state and relay rejection; reducer sends flush only while that
connection is pumped (`frame_tick`), so it pumps both peers.
