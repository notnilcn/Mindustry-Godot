# AGENTS.md — `mind-stdb` (Rust SpacetimeDB client)

The Rust replacement for the deleted C# `sstdbsdk`: connection lifecycle, token
persistence, subscription waves, typed table binders, protocol handshake and the
ordered match-command relay. Read the root [`AGENTS.md`](../../../AGENTS.md),
[`client/AGENTS.md`](../../AGENTS.md) and the owning plan
[`01_PLATFORM_STDB_IMPLEMENTATION_PLAN.md`](../../../01_PLATFORM_STDB_IMPLEMENTATION_PLAN.md)
(§3.2–§3.12) first.

## Hard rules

1. **Godot-free and tokio-free.** No `godot` dependency, no `tokio` type in the
   public API (OD-R4). The SDK owns its transport runtime internally; we call
   `DbConnection::frame_tick()` from the main thread once per frame.
2. **`module_bindings/` is GENERATED — never read as documentation, never
   hand-edit.** It is written only by `server/build.sh` (or `build.ps1`);
   CI's drift gate is `server/build.sh --check` (`-Check` on Windows). A manual
   edit fails CI and the file is overwritten on the next publish.
3. **`cargo test -p mind-stdb` is network-free.** Live/server tests are
   `#[ignore]`d and env-gated by `MIND_STDB_IT=1`; they use database
   `mindustry-it`, never `mindustry`.
4. **Offline mode never builds a `DbConnection`.** With `StdbMode::Offline`
   (and no server running) `pump()` is a no-op; the game must keep booting and
   playing single-player (plan §3.12 invariant 8).
5. Tokens are never logged. The token file is per host+suffix under the injected
   data directory and is gitignored.

## Layout

| Path | Contents |
|---|---|
| `src/config.rs` | `ConnectionConfig`, `StdbMode`, `ConnectPolicy`, `Backoff` |
| `src/identity.rs` | `LocalIdentity`, `parse_player_suffix` (`--pN`, exact `^--p\d+$`) |
| `src/token.rs` | `TokenStore` trait + `FileTokenStore`, `token_key` |
| `src/connector.rs` | `Connector` state machine, events, `pump`, wave subscription |
| `src/waves.rs` | `WaveName`, static wave table lists, `SubscriptionWaves` |
| `src/binder.rs` | `TableBinder<A>`, `RowChange<T>`, replay/replay-off type split |
| `src/relay.rs` | `CommandStream` (ordered `match_command` delivery) |
| `src/protocol.rs` | client `PROTOCOL_VERSION` + mismatch error |
| `src/rows.rs` | small `RowView` impls used by the gdext debug JSON only |
| `src/module_bindings/` | **GENERATED** by `server/build.sh`; never hand-edit |

`mind-gdext` wraps this crate (`StdbConnector`/`StdbBinder`); `mind-headless`
uses it for the `stdb_*` scenarios. `mind-core` never depends on it.

## The five-step recipe (from the deleted C# SDK's AGENTS.md)

Adding a new networked feature always follows the same five steps:

1. **Table** — declare the row in `server/spacetimedb/src/<system>/tables.rs`
   with `#[table(accessor = ..., public)]` (or a `#[view]` for per-caller
   projections). Public table/view/field names are schema ABI: append-only,
   never rename.
2. **Reducer** — add the deterministic write path in
   `server/spacetimedb/src/<system>/reducers.rs`. `ctx.sender()` is the only
   principal; reducers return no data (`Result<(), String>` at most) and do
   cheap validation only (D2).
3. **Wave entry** — add the generated accessor name to exactly one static list
   in `src/waves.rs` (`BASE_TABLES`/`LOBBY_TABLES`/`GAME_TABLES`). Every table
   lives in exactly one wave; a binder for a table in no wave warns.
4. **Binder + handler** — bind the table through
   `Connector::bind::<...Accessor>()`, drain `RowChange<T>`s after `pump()` and
   handle them. Use `replay_existing` only on primary-key tables; event tables
   are insert-only at the type level.
5. **Call the reducer** — expose a typed method on `Connector` (or the gdext
   `StdbConnector`) that calls the generated reducer via `RemoteReducers`.
   Never mutate mirrored state optimistically: the server row is the truth.

Then regenerate + verify:

```bash
server/build.sh                     # generate bindings + publish local db
server/build.sh --check             # drift gate (must be clean)
cargo test -p mind-stdb             # network-free suite
cargo clippy -p mind-stdb --all-targets -- -D warnings
```

## Ported rules (C# `sstdbsdk` → Rust)

1. The connection is a singleton autoload (`StdbConnector`); all access goes
   through the connector instance.
2. Row callbacks are cheap: they push an owned `RowChange<T>` into one
   `std::sync::mpsc`/mutex queue per binder and nothing else.
3. `replay_existing` is only possible for `TableWithPrimaryKey` rows — the Rust
   type system enforces what C# documented as a warning (never replay event
   tables).
4. Auto-increment IDs (command IDs, session IDs) are **not sequential**;
   numeric gaps are normal, and presence-based loss detection is wrong. Loss is
   detected per-sender via `sender_seq` continuity.
5. Never mutate the local mirror to "help" the server; the reducer call is the
   only write path.
6. Subscription waves are explicit static lists, not reflection/SQL built at
   runtime.
7. Reconnection is connector-owned: it re-subscribes active waves, re-registers
   every binder and raises `Resync`; consumers drop stale mirrors and replay
   from the client cache instead of rebuilding the scene.
8. Callbacks run inside `pump()` (main thread when driven by Godot); nothing in
   this crate may block, spawn Godot work or touch thread-local Godot state.
9. `pump()` never panics: SDK `Err` becomes `ConnectorEvent::{Disconnected,
   ConnectError}` and the connection moves to `Retrying`/`Disconnected`.
10. Token key = `host.replace("://","_").replace(":","_").replace("/","_")` +
    suffix (`--pN`). The `--pN` parser is exact (`^--p\d+$`) — it must reject
    Godot's `--path` (regression test).
