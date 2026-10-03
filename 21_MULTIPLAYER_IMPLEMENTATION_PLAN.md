# 21 — MULTIPLAYER IMPLEMENTATION PLAN

> Every source file this plan produces starts with `// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.`
> Template: `HIGH_LEVEL_PLAN.md` §4 (nine sections, this file). Locked decisions inherited from §0: D1–D9. The multiplayer authority model is **D2** and is not up for debate: STDB is persistent state + command relay + cheap validation; each client runs the full deterministic simulation; an authoritative STDB match sim is deferred but the schema keeps room for it.
> Read order for the implementing agent: `HIGH_LEVEL_PLAN.md` → this file → the Mindustry `AGENTS.md` files named in §1.

## 1. Header block

| Field | Value |
|---|---|
| **Status** | Draft v1 — 2026-10-01, not started |
| **Phase** | P8 — Multiplayer |
| **Depends on** | `01_PLATFORM_STDB_IMPLEMENTATION_PLAN.md` (connector, waves, binders, `RelayMatch`/`RelayMember`/`MatchCommand`/`CommandKind`, `CommandStream`, rate table), `05_SIM_CORE_IMPLEMENTATION_PLAN.md` (`SimCommand`, `CommandLog`, `Sim::checksum`, `Sim::snapshot`/restore, `FieldMeta`, `SyncFloat`/`SyncLocal`), `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` (deterministic map generation from `map_id`+`map_seed`, `WorldContext`/`load_map`, tile ops), `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` (unit variants `UnitCommand`/`UnitCommandQueue`/`UnitStance`, spawn/death determinism, controller codec), `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` (`SetRules`/`ResearchUnlock`/`CompleteObjective`/`SectorCapture`/`SaveSector` intents, `RulesBlob`, campaign row shapes), `15_INPUT_RTS_IMPLEMENTATION_PLAN.md` (`RemoteAction`/`ActionBatch`, plan-snapshot payload, prediction contract), `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` (interpolation of remote state), `17_FX_PARTS_IMPLEMENTATION_PLAN.md` (view-only effects; no sync). |
| **Blocks** | `22_PLATFORM_EXPORT_IMPLEMENTATION_PLAN.md` (dedicated server binary, console, Steam/Discord; consumes §3.11/§3.13), `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` (multiplayer scenarios/goldens/benches registered here). |
| **Sources** | `Mindustry/core/src/mindustry/core/NetClient.java`, `NetServer.java`; `net/{Net,Packet,Packets,NetworkIO,Streamable,NetConnection,Administration,Host,WorldReloader,ValidateException,ArcNetProvider}.java`; `server/src/mindustry/server/ServerControl.java` (command surface only); `annotations/src/main/java/mindustry/annotations/remote/{RemoteProcess,CallGenerator}.java` (semantics only); `entities/comp/*` sync markers; `tests/src/test/java/ApplicationTests.java`. Reference stack: `C:\Users\Clinton\g\main\client\sstdbsdk\{DatabaseConnector.cs,TableSubscriber.cs,TableBinderComponent.cs}` + `main\server\spacetimedb\src\player\{tables,views}.rs`, `main\server\spacetimedb\src\main\{lifecycle,global,seeds}.rs`, `chat/mod.rs`. |
| **AGENTS read** | `Mindustry/core/src/mindustry/net/AGENTS.md`, `core/src/mindustry/core/AGENTS.md`, `core/src/mindustry/entities/AGENTS.md` (Syncc/`@SyncField`), `server/AGENTS.md` (headless hosting/admin commands); `C:\Users\Clinton\g\main\AGENTS.md`, `C:\Users\Clinton\g\main\server\AGENTS.md` (SpacetimeDB Rust rules); `C:\Users\Clinton\g\.opencode\skills\playtest\SKILL.md` (multi-instance MCP recipe). |
| **Extends spine** | New `MindNet` autoload (`/root/MindNet`) beside `/root/StdbConnector` and `/root/Spine/SimHost`; `MindSimHost` gains `pending_commands` drain + relay/checksum/snapshot `#[func]`s; state inspector gains a `net` page (`01` reserved it) extended with match/session/checksum fields; `mind-headless` gains the `mp_*` scenario family; `scenarios/*.json` gains an append-only `"relay"` section; MCP two-instance (`--p1`/`--p2`) flows become part of the repo playtest skill (plan 00's analog). Single-player remains fully functional with `StdbMode::Offline` and never touches this plan's code paths. |

This plan owns the multiplayer system on the D2 model: the full STDB schema, the command relay and cheap validation, ordered command application, player-state/plan sync, snapshots and desync correction, late join/reconnect, lobby/host/join/spectator flows, admin/moderation, chat, campaign/schematic persistence, the dedicated-server shape, and the OD3 transport seam.

---

## 2. Scope & parity definition

### 2.1 In scope

1. **Full STDB module schema** (extends plan 01's skeleton): match lifecycle + membership, per-match ordered command log, match state, player-state LWW stream, plan snapshots, checksums/digests, snapshot chunks, chat, UI/server-menu events, campaign/sector/unlock/stats persistence, schematics, admin/ban/whitelist/filters, mod requirements, content catalog, per-sender command state, sweeps.
2. **Command envelope and relay**: extend plan 01's `RelayMatch`/`RelayMember`/`MatchCommand`/`CommandKind` (do not redefine the envelope); implement the full variant set; ordering (`command_id` global, `sender_seq` per sender) and the client `CommandStream` consumer; cheap validation matrix; echo-skip and prediction rules; rejection detection/correction.
3. **Client pipeline**: `RemoteAction` → `SimCommand` (plan 15) and `CommandKind` → `SimCommand` (this plan) adapters; apply only at fixed-tick boundaries in `command_id` order; own-echo skip; player input/state sync; plan snapshots; `Call.setRules` equivalent.
4. **Snapshots, checksums, desync**: checksum publication/comparison keyed on `command_id`; host digests and dynamic snapshots; deterministic cohort + authority mask; desync detection, full-resync correction, and late-join streaming (terrain by seed regeneration + dynamic snapshot + tail replay); reconnect.
5. **Lobby/host/join**: host/join/leave/start/kick/ban/spectate flows, readiness gating, spawn, server browser via a public STDB view, dedicated headless server shape.
6. **Admin/moderation**: global admin table, kick/ban/whitelist/trace/wave/switch-team actions, chat filters, rate limits, audit, server-console mapping.
7. **Campaign/schematics/settings**: MP campaign persistence (host-owned, STDB), personal schematics, reuse of plan 01's settings row; mod-requirement handshake rows.
8. **Transport decision (OD3)** and the seams that keep direct ENet/UDP possible for bulk traffic without moving authority out of STDB.
9. **Room for the deferred authoritative sim**: additive `exec_tick`, the scheduled tick reducer contract, client authority switch, and exactly what changes.

### 2.2 “Done” means

- `cargo test -p mind-stdb` and the server `cargo check --manifest-path server/spacetimedb/Cargo.toml --tests` gates in §7a are green (network-free unit tests; env-gated integration tests).
- `server/build.sh` publishes the full module; `server/build.sh --check` shows no binding drift; all tables in §6.1 exist with the listed indexes.
- Two clients (`--p1`/`--p2`) create/join the same match through STDB, both spawn, and a block placed on one appears on the other within the §7d budget; movement, chat, plan snapshots, and admin kick/ban are observable — evidence per §7c.
- `mind-headless run mp_command_log_replay`, `mp_validation_matrix`, `mp_desync_injection`, `mp_late_join`, `mp_reconnect_gap` pass (§7b).
- A 30-minute soak with two clients + 200 relayed commands/s + one forced desync ends with identical host checksums and no growing divergence (§7e).
- Single-player with the STDB layer absent still boots and plays (invariant §3.17-1).

### 2.3 Deliberate deviations (all rooted in D2; reason stated)

| # | Deviation from upstream Mindustry | Reason |
|---|---|---|
| 1 | **No authoritative server simulation.** Clients run the sim; the relay only carries ordered intents and LWW player state. | HLP D2. Upstream `NetServer.clientSnapshot` clamps/corrects; here that job belongs to the future STDB sim (§3.14). |
| 2 | **Player-possessed units are client-authoritative** (`SyncLocal`-equivalent): kinematics, health/shield of the unit a player controls are published as state, not compared in relay checksums. | Without a server sim there is no authority to own them; matches Mindustry's client-predicted movement. The authority mask is removed when the sim lands (§3.2, §6.5). Flagged for user (§8 OD-21-A). |
| 3 | **No IP/subnet bans.** Only STDB identity bans + whitelist. | SpacetimeDB exposes no peer IP to reducers; `Administration`'s IP/subnet machinery has no data source. Identity is stronger for account bans, weaker for evading. |
| 4 | **No UDP entity snapshots.** Entity interpolation sources are (a) the local deterministic sim for the world cohort and (b) `match_player_state` LWW rows for remote players. | D2: every peer already computes the world; sending entity snapshots would duplicate it. Upstream `entitySnapshot`/`blockSnapshot`/`hiddenSnapshot` collapse onto the deterministic replay + snapshot correction path. |
| 5 | **Fog of war is client-side concealment, not data withholding.** | All peers subscribe to the same match state under D2; `FogControl` (12) decides what the local renderer shows. Upstream `Syncc.isSyncHidden(team)` per-team snapshots return with the authoritative sim. |
| 6 | **Packet ids / `Call` codegen replaced by typed `CommandKind` + reducer names.** | HLP §1 (no `mindustry.gen`). Variant order is not ABI; the variant *names* are the ABI and append-only. |
| 7 | **Server menus / chat / admin travel as STDB rows**, not `@Remote` packets. Join flow uses the server browser view, not ping discovery. | HLP D1/D2, `main/` stack. LAN/ping discovery (`ArcNetProvider.discoverServers`) is Steam/22 deferred. |
| 8 | **Command log is durable and pruned by snapshot watermark**, not ephemeral UDP. | Plan 01 §3.8 chose persistent ordered rows; this plan adds snapshot-anchored pruning (§6.8). |
| 9 | **Plan snapshots are LWW per player**, not `clientPlanSnapshot` unreliable packets. | Plan 15 §6.5 defines the payload; durability costs nothing and simplifies late join. |
| 10 | **`stateSnapshot` becomes `match_state` (host-published)**, including RNG state in digests; `setCameraPosition`/`sound`/`effect` remotes are not relayed at all. | Under D2 every peer derives exactly these from the ordered log; only host-owned UI/menu payloads are relayed. |

### 2.4 Ownership boundaries (explicit; do not implement in this plan)

| Area | Owner | This plan consumes |
|---|---|---|
| Connector/token/waves/binders/`frame_tick` pump; `RelayMatch`/`RelayMember`/`MatchCommand` base tables; `CommandStream`; `CommandRate` | `01_PLATFORM_STDB_IMPLEMENTATION_PLAN.md` | Extends tables/variants; never forks the pump or `CommandStream` ordering. |
| `SimCommand`, `CommandLog`, `Sim::checksum`, `Sim::snapshot`, RNG streams, entity framework | `05_SIM_CORE_IMPLEMENTATION_PLAN.md` | Maps relay rows → `SimCommand`; requests `checksum_scoped` + snapshot restore hooks (additive; §3.2/§6.5). |
| Terrain generation, `WorldContext`, `load_map`, deterministic seed → world | `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` | Late join regenerates terrain; never ships terrain bytes. |
| Bullet/Turret relay payload (`BulletSpawn`) | `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md` | `BulletRelay` encoding only (§6.3). |
| Unit command variants, spawn/death determinism, controller codec | `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` | Relays `UnitCommand`/`UnitCommandQueue`/`UnitStance`/`PlayerSpawn`. |
| Campaign intents, `Rules`, `RulesBlob`, fog, objectives | `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` | STDB tables `sector_info`/`unlock`/`campaign_stats`/`schematic`/`campaign`; host-only validation. |
| `LogicSyncEvent`/`ClientLogicDataEvent`/`Configure` blobs | `13_LOGIC_MLOG_IMPLEMENTATION_PLAN.md` | Ordered relay variants + rate limits. |
| Join/Host/Chat/PlayerList/Admin/Trace/Server-Menu dialogs; UI payload capture | `14_UI_IMPLEMENTATION_PLAN.md` | `match_ui_event` transport + `MindNet` APIs. |
| `RemoteAction`/`ActionBatch`, prediction contract, plan-snapshot payload | `15_INPUT_RTS_IMPLEMENTATION_PLAN.md` | Echo/prediction rules; no re-implementation. |
| Remote-state interpolation, fog/minimap rendering | `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md`, `17_FX_PARTS_IMPLEMENTATION_PLAN.md` | Publish interpolation targets; never relay effects. |
| Mod discovery/metadata, content JSON, asset delivery, mod handshake data | `20_MODS_IMPLEMENTATION_PLAN.md` (not on disk at authoring) | `match_mod` rows + `content_catalog` seed; join compatibility check (§3.12.4, §8 R-21-6). |
| Dedicated export preset, console, file dialogs, Steam/Discord | `22_PLATFORM_EXPORT_IMPLEMENTATION_PLAN.md` (not on disk) | `MindNet` host/serve API + admin reducer/console contract (§3.11). |
| Golden suite/perf CI | `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` (not on disk) | Scenario/bench names and golden paths in §7b/§7d. |
| Saves/settings/TypeIO/entity IO | `04_IO_SERIALIZATION_IMPLEMENTATION_PLAN.md` | Snapshot blob codec + compression; no new save format. |

---

## 3. Target design

All names below are final unless marked otherwise. `mind-core` stays Godot-free and network-free; `mind-stdb` stays Godot-free; the server module is a separate workspace crate (plan 01 §3.2). No `HashMap` iteration in sim/apply paths; ordered containers only (HLP §2.4). GPL-3.0 headers on every file (HLP §6.4).

### 3.1 Repository additions

```
client/rust/
  mind-stdb/src/
    relay.rs                 # EXISTS (01): CommandStream, CommandOrder, OrderError — extended with
                             #   `command_row_to_bytes`/`RowView` impls only; ordering logic untouched
    commands.rs              # this plan: CommandKind helpers, per-variant caps, local preflight_validate,
                             #   sender envelope stamping (client_tick, sender_seq)
    session.rs               # this plan: MatchSession state machine (Offline|Browsing|InLobby|Loading|InGame|
                             #   SnapshotSync|Reconnecting), host/join/leave/start/kick APIs over reducers
    snapshot.rs              # this plan: chunk downloader/uploader, StreamBuilder-equivalent progress,
                             #   dynamic snapshot codec wrapper (delegates entity bytes to 04)
    transport.rs             # this plan (OD3 seam): trait RelayTransport + StdbTransport; send_command,
                             #   subscribe_commands, send_player_state, publish_snapshot_chunk, request_snapshot
  mind-gdext/src/
    net/mod.rs               # `MindNet` autoload (Rust #[class(base=Node)]) — session + relay pump ownership
    net/relay.rs             # CommandKind -> SimCommand adapter + apply policy + echo skip
    net/session.rs           # join/lobby/host flows called by 14; readiness/spawn
    net/snapshot.rs          # apply snapshot into Sim; tail replay driver
    net/inspector.rs         # `net` inspector page data (match, members, checksum, queue depth)
  mind-headless/src/mp/      # mp_* scenarios + benches (registration only; logic in mind-core/mind-stdb)
server/spacetimedb/src/
  relay/                     # EXISTS (01): mod.rs tables.rs methods.rs reducers.rs views.rs
    state.rs                 # this plan: match_state (host state snapshot equivalent)
    snapshot.rs              # this plan: match_snapshot + match_snapshot_chunk + request/publish/prune
    checksum.rs              # this plan: match_checksum + comparison helpers + sweep
    player_state.rs          # this plan: match_player_state + report_player_state
    plans.rs                 # this plan: match_plan_chunk/match_plan_state + report_plan_snapshot
    ui_events.rs             # this plan: match_ui_event + publish/consume + ping markers
    sweep.rs                 # this plan: tick_maintenance scheduled reducer + retention policies
  chat/mod.rs                # this plan (main/server chat/mod.rs pattern): match_chat + rate/filter
  admin/
    mod.rs tables.rs methods.rs reducers.rs views.rs
  campaign/mod.rs            # campaign, sector_info, unlock, campaign_stats, schematic (+ reducers/views)
  mods/mod.rs                # match_mod, content_catalog (+ join compatibility helpers)
server/spacetimedb/src/main/global.rs   # constants: caps, intervals, retention, cohort/mask policy
client/scenarios/            # mirrored; new mp_* scenario files come from repo-root scenarios/
```

`client/Scripts/` and `client/sstdbsdk/` remain reference-only until plan 00 deletes them.

### 3.2 Authority model and the deterministic cohort (D2)

Every peer runs `mind-core`'s sim. Authority is split three ways:

1. **Ordered world intents** (`match_command` rows) — place/break/configure, rotate/delete plans, unit commands/stances, inventory/payload/control, bullets from host-only sources, `SetRules`/`SetRule`, objectives/research/campaign intents, `LogicSync`/`LogicClientData`, admin tile ops. Applied on **every** peer at a fixed-tick boundary in `command_id` order. These mutations are part of the shared deterministic state.
2. **Client-owned player state** (`match_player_state`, LWW per `(match_id, identity)`) — the possessed unit's kinematics/health plus input flags (the `NetClient.sync()` clientSnapshot fields). Applied to remote players as puppet/interpolation targets; the owner is authoritative. Excluded from cross-peer checksum comparison via the **authority mask** (§6.5).
3. **Host-owned derived state** (`match_state`, `match_snapshot`, `match_checksum`) — wave/wavetime/paused/game-over/rules and periodic state digests. The host (`RelayMatch.host`) is the D2 canonical checkpoint source; a dedicated server is simply a host that never plays.

**Deterministic cohort** (hashed and compared): `GameState` scalars, `Rules`, `Teams`/`TeamData` caches, world tiles/buildings/logistics, power/liquid/heat graphs, all non-possessed units (incl. their AI, commands, stances), bullets/fires/puddles, waves/rng `Sim` stream, `MapObjectives`/fog, `SimClock`/`Time` runs. **Masked** (computed locally, not compared): `SyncLocal` fields per plan 05, and — while `AuthorityMode::Relay` — the possessed-unit fields listed in §6.5 (`Pos`, `Vel`, `Rot`, `Health`, `Shield`, `Aim`, `MineTile`, input flags). The mask is a comparison-time projection, not a change to 05's hasher: plan 05 adds `Sim::checksum_scoped(&ChecksumScope) -> u64` that walks the identical field order but replaces masked fields with their declared defaults before hashing. `AuthorityMode::Authoritative` later removes the mask (invariant, §3.14).

Why not strict lockstep gating: D2 has no server tick to gate on; a slow client would stall everyone (unlike upstream, where the server is authoritative and clients merely interpolate). Why not rollback: plan 05/15 provide no input history; snapshot correction is cheaper and already needed for late join.

### 3.3 Match lifecycle, host, join, spawn, leave

`MatchStatus` stays plan 01's `{ Lobby, Running, Ended }` (assignment's “closed” = `Ended`; there is no separate closed state). `AuthorityMode` stays `{ Relay, Authoritative }`.

1. **Host** (any client; UI in 14) calls `create_match` (plan 01 reducer, extended parameters): `map_id`, `map_seed` (or `map_hash` when hosting a custom map), `mode`, `mode_name`, `visibility`, `password: Option<String>`, `max_players`, `rules_epoch = 1`, `rules_json`, `is_dedicated=false`, `campaign_id: Option<u64>`, `sector`. The reducer writes `relay_match` + a `RelayMember` row for the host with `role=Player, ready=true` and inserts `match_mod` rows for the host's content manifest (§3.12.4). Host then loads the world locally (06) and remains in `Lobby`.
2. **Join** (14's JoinDialog; `MindNet.join_match(match_id, password)`): `join_match` reducer checks status (`Lobby` or `Running`), ban/whitelist, protocol/build, password hash (salted FNV of the supplied string), player cap, and mod compatibility (client sends `mods` + `content_hash` as reducer args; server compares against `match_mod`; mismatch → `Err`, client shows the reason). On success the reducer inserts `RelayMember` (`connected=true`, `ready=false`, `team=None`) and increments `relay_match.player_count`.
3. **Loading/ready**: the joiner subscribes the Game wave (01) + on-demand Snapshot wave (§3.8), loads/regenerates the world, requests the latest snapshot when `Running`, then calls `set_ready(true)`. Host sees `my_match_members.ready`; `start_match` requires every connected member ready or `force=true` (host only).
4. **Start** (`start_match`, host): status → `Running`, `started_at` set, host publishes a `WorldReset` snapshot (`command_id = 0`, `snapshot_id` stored in `match_state`), `match_state` populated. Host emits `CommandKind::SetRules` if the lobby edited rules, then each client emits `CommandKind::PlayerSpawn` (itself) once it observes `Running`; spawn parameters come from `rules.spawns`/core placement resolved deterministically by the sim (all peers resolve identically from `rules_json`; the command carries only `unit: Option<String>`, `team: u8`).
5. **Late join while `Running`**: §3.8.
6. **Leave/kick/ban**: `leave_match` (01) removes the member; host leaving ends the match for everyone (`leave_match` sets `Ended` when `ctx.sender() == host`; default — no host migration, upstream parity). Non-host disconnect: `client_disconnected` sets `RelayMember.connected=false` (grace), `tick_maintenance` removes after `MEMBER_GRACE_SECS` (120 s). Kick (`admin_kick`) sets `RelayMember.kicked_reason`; the target's `my_kick` view drives disconnection + UI. Ban (`admin_ban`) additionally writes `player_ban`; the target cannot rejoin.
7. **Spectator**: `RelayMember.role = Spectator` (join dialog choice). Spectators receive the Game wave and may chat/read, but `send_match_command` rejects sim-affecting variants for them; they never emit `PlayerSpawn`.
8. **Player cap / player_count**: maintained by the join/leave/kick reducers on `relay_match.player_count` (LWW counter, no aggregate view needed).

### 3.4 Relay schema extension (plan 01 delta; envelope unchanged)

Plan 01's envelope stays exactly: `MatchCommand { command_id, match_id, sender, sender_seq, client_tick, kind, sent_at }`. `RelayMember` gains columns; `RelayMatch` gains columns; `CommandRate` gains per-class counters. The full final table catalog is §6.1.

```text
RelayMatch (01) + :
  host: Identity,                        // == created_by; authoritative checkpoint source (D2)
  mode: Gamemode, mode_name: String,
  visibility: Visibility,                // Public | Unlisted
  password_hash: Option<u64>,            // salted FNV-1a of the password; never the plaintext
  rules_json: String, rules_epoch: u32,
  build_id: String, content_hash: u64,   // join compatibility (20)
  is_dedicated: bool,
  player_count: u16, max_players: u16,
  campaign_id: Option<u64>, sector_planet: Option<String>, sector_id: Option<u32>,
  last_command_id: u64, last_snapshot_id: Option<u64>,
  closed_at: Option<Timestamp>,

RelayMember (01) + :
  role: MemberRole,                      // Player | Spectator
  team: Option<u8>,
  connected: bool, ready: bool,
  last_seen_at: Timestamp,
  kicked_reason: Option<String>,

CommandRate (01) + :
  count_input: u32, count_plan: u32, count_ui: u32, count_logic: u32, count_admin: u32,

CommandKind — 01's variants stay by name; the two payloads below widen (hard cut delegated to 21 by
01 §2.3/§3.10 because block config is 04's TypeIO blob, not u32):
  PlaceBlock   { x: i32, y: i32, block: String, rotation: u8, config: Vec<u8> }
  ConfigBlock  { x: i32, y: i32, value: Vec<u8> }
```

New variants (full list + caps + `SimCommand` mapping in §6.3): `Rotate`, `DeletePlans`, `CommandBuilding`, `Inventory`, `Payload`, `UnitControl`, `UnitClear`, `BuildingControlSelect`, `UnitCommand`, `UnitCommandQueue`, `UnitStance`, `PlayerSpawn`, `Bullet`, `SetRules`, `SetRule`, `ResearchUnlock`, `CompleteObjective`, `ClearObjectives`, `SectorCapture`, `SaveSector`, `SkipWave`, `RunWave`, `AdminSwitchTeam`, `AdminTileOp`, `LogicSync`, `LogicClientData`, `MenuChoose`, `MenuBuilderChoose`, `TextInputResult`, `Custom`.

New envelope-adjacent tables: `sender_command_state` (per-sender echo/rejection bookkeeping, exposed to the caller), `match_state`, `match_player_state`, `match_plan_chunk`/`match_plan_state`, `match_checksum`, `match_snapshot`/`match_snapshot_chunk`, `match_ui_event`, `match_chat`, `match_kick`, `chat_rate`, `admin_rate`, `server_config`, `admin_identity`, `player_ban`, `whitelist_entry`, `chat_filter`, `admin_action_log`, `campaign`, `sector_info`, `unlock`, `campaign_stats`, `schematic`, `match_mod`, `content_catalog`, `maintenance_schedule`.

### 3.5 Command envelope, ordering, echo, prediction, rejection

Ordering contract is plan 01's and is not re-specified: apply by ascending `command_id`; gaps in `command_id` are normal; loss = broken `sender_seq` continuity; duplicates ignored. This plan adds the **client behavior** around it:

- **Emit** (`mind-stdb::commands`): `CommandSender::send(match_id, kind)` stamps `sender_seq = next_seq()` (strictly increasing, starts at 1) and `client_tick = sim_tick + INPUT_DELAY_TICKS` (default 6 = 100 ms; `relay_config.input_delay_ticks`), inserts via reducer, and records `(sender_seq, kind)` in the in-flight table `PredictionQueue` (bounded 512; overflow → forced resync).
- **Local prediction**: the client runs a local mirror of the cheap validator (`preflight_validate`) before emitting; if it passes, the action is applied immediately to the local sim (`Sim::command`) and queued as in-flight. This preserves plan 15's `called = Loc.both` feel. `PredictPolicy` per variant:
  - `Immediate` (default, upstream `called=Loc.both`): all `RemoteAction`-backed variants.
  - `AwaitEcho`: `SetRules`/`SetRule` (host config), `AdminTileOp`, `SkipWave`/`RunWave` (admin), `SaveSector`, and any `Custom` variant. These either cannot be locally validated or are infrequent enough that 1 RTT is invisible.
  - `LocalOnly`: `PlayerSpawn` self and `PlayerInput`-class state are owned locally and never predicted from the log.
- **Echo skip**: `RelayRuntime` drains `CommandStream` rows, and if `row.sender == local_identity && row.sender_seq` matches an in-flight prediction, it removes the in-flight entry and **does not re-apply**; the row still advances `applied_command_id` watermark (needed for checkpoints).
- **Foreign apply**: rows from other senders are applied at the next fixed-tick boundary (plan 01's frame order: `pump → drain binder queues → drain CommandStream into pending → sim accumulator`), before `TickSet::Frame`. Plan 15's `MindSimHost.pending_commands` is the same queue.
- **Rejection detection**: on success, `send_match_command` updates `sender_command_state.last_accepted_seq` for the caller (server-only table with a per-caller view). `RelayRuntime` watches: if the oldest in-flight seq is absent and `last_accepted_seq < seq` after `ECHO_TIMEOUT_MS` (500), it emits `PredictionRejected { seq, reason_unknown }`, drops the prediction, and triggers §3.7 correction. If `last_accepted_seq >= seq` the row was accepted but pruned/subscription-missed → normal ordering wait.
- **Rejection audit**: failed reducers roll back all writes, so failures are `log::warn!` only (plan 01 R7). `sender_command_state.reject_count` increments on the *next* successful command when a gap is detected (best-effort; documented limitation).
- **Command stream hygiene**: `match_command` rows are pruned behind the snapshot watermark (§6.8); clients tolerate `RowChange::Delete` for already-applied ids and treat a delete for a never-seen id as a gap → snapshot path.
- **Own clock**: `client_tick` is advisory (used by the clock-skew validation in §6.3 and future `exec_tick`), never for ordering.

### 3.6 Player state, plan snapshots, chat, UI events, `setRules`

- **Player state** (`report_player_state`, 15 Hz default = upstream 66 ms `playerSyncTime`): reducer inserts/updates `match_player_state` for `ctx.sender()` after cheap checks (member, role, monotonic `seq > stored.seq`, finite floats, `client_tick` skew, rate 25/s). The view `my_match_player_states` is member-scoped; the client applies remote rows as puppet targets: position/rotation are snapped (or interpolated by 16 at render time), input flags feed the remote unit's controller mirror, `health`/`shield` are applied to the puppet (owner-authoritative under D2). `MindNet.get_remote_player_state(identity)` exposes it for 16 and tests.
- **Plan snapshots** (plan 15 §6.5: 0.5 s cadence, ≤1000 plans, 75/chunk): `report_plan_snapshot(group_id, chunk_index, chunk_count, plans_blob)` writes `match_plan_chunk` rows; when all `chunk_count` chunks of a group exist, the same reducer sets `match_plan_state.active_group_id` and deletes the previous group. Views: `my_match_plans` (the active group per member + own group). `plans_blob` uses 04's `TypeIO` plan encoding; caps 8 KB/chunk, 128 KB/player.
- **Chat** (`send_chat(kind, text)`): `kind ∈ {All, Team, System}`; server-side rate 1/2 s window + cap (Mindustry `chatSpamLimit` 20/2 s → mute), length `MAX_TEXT = 150`, newline strip, `chat_filter` substring pass (case-insensitive; `mute=true` drops + increments infractions), team requires `RelayMember.team = Some(t)` and broadcasts to members of `t` via the view filter (`my_match_chat` includes All + own Team). `System` is host/admin only.
- **UI events** (`publish_ui_event(kind, target, payload)`): host/admin → client(s) for 14's payloads (`menu_builder_show/update/hide`, `hud_text`, `announce`, `info_popup`, `label`, `info_toast`, `warning_toast`, `open_uri`, `copy_to_clipboard`, `ping_marker`). `payload` is 14's `ui_node` bytes (`format: 1`), capped 64 KB. Views: `my_match_ui_events` (member; `target` filter when set). Client-side results (`MenuChoose`, `MenuBuilderChoose`, `TextInputResult`) are `CommandKind` rows so the host receives them in its own relay stream.
- **`Call.setRules` equivalent**: `SetRules { rules_json, rules_epoch }`, host/admin only, `len(rules_json) ≤ 100_000` and epoch must equal `relay_match.rules_epoch`; the reducer writes the command row **and** updates `relay_match.rules_json` + `rules_epoch += 1` + `match_state.rules_json/epoch` in the same transaction, so late joiners always read the latest rules. `SetRule { rule, json }` is the single-field variant (`NetClient.setRule`), host/admin only. Clients apply on echo (`AwaitEcho`), fire `RulesLoadEvent`, and reject non-host senders locally as well.

### 3.7 Checksums, state digests, desync detection & correction

- **Publication**: every member publishes `match_checksum { command_id, sim_tick, checksum, checksum_version, scope }` every `CHECKSUM_INTERVAL_TICKS = 120` (2 s) **and** whenever `command_id` crosses `CHECKPOINT_INTERVAL_COMMANDS = 128` since the last row. The row is keyed on the `command_id` watermark (state after all commands ≤ C applied); `sim_tick` is diagnostic only. `checksum = checksum_scoped(&relay_scope)` (§6.5). Rate: 1/2 s per identity.
- **Detection**: `ChecksumMonitor` reads `my_match_checksums`, groups by `command_id`; rows with different `checksum_version`s are ignored (05 requirement). When ≥2 members report the same `command_id` with different hashes → `DesyncDetected { command_id, ours, host_checksum, others }`. The **host's value is canonical** under D2 (no server truth); if the host itself is the outlier the majority value is still not actionable without authority — clients log and correct to host, then escalate to the human admin (upstream has no equivalent; this is the D2 cost).
- **Correction policy: full resync** (chosen over rollback):
  1. Client sets `MatchSession::SnapshotSync`, pauses sim ticks (view keeps running).
  2. Calls `request_snapshot` (rate 1/10 s); host sees `my_match_snapshot_requests` and publishes a fresh `Dynamic` snapshot at its current `command_id` (`publish_snapshot` chunks payload).
  3. Client downloads chunks (on-demand Snapshot wave), validates the header (magic/format/map/content/checksum_version), loads: terrain from seed (06), dynamic state via 04's restore path into `Sim` (`Sim::restore_snapshot`, a 05 hook), then applies every `match_command` with `command_id > snapshot.command_id` from its local command ring buffer (`REPLAY_TAIL = 4096` rows; if any are missing, it asks the host for a snapshot at the *current* `last_command_id` so the tail is empty).
  4. Resume; publish a checksum at the next checkpoint; if it mismatches again within `RESYNC_RETRY_LIMIT = 3`, transition to `SnapshotSync` with a full host re-snapshot and surface a retry toast (14).
  - Justification: no rollback machinery exists (05 has no input history), snapshots are already required for late join, and correction frequency should be low; bounded cost (≤4 MB + tail replay) beats new deterministic-replay complexity. Rollback is revisited only with the authoritative sim (§3.14).
- **Digest cadence**: host publishes a `Digest` (no blob) every `DIGEST_INTERVAL = 600 ticks` (10 s) with `command_id`, `sim_tick`, `checksum`, `rng_sim`, `next_entity_id` into `match_state`/`match_checksum`; members compare. Dynamic snapshots every `SNAPSHOT_INTERVAL = 3600 ticks` (60 s) and on request.

### 3.8 Snapshots and late-join world streaming

**Terrain is never streamed; it is regenerated.** Late join always starts from `relay_match.map_id/map_seed/map_hash` (06's generators are deterministic by build; `map_hash` catches generator changes/custom maps, and a mismatch rejects the match with a clear error). This keeps the byte budget oriented to dynamic state only.

Snapshot kinds: `WorldReset { command_id=0, rules, map identity, blob=empty }`, `Dynamic { blob = header + 04 entity/teams/markers payload }`, `Digest { no blob, checksum/rng }`.

Dynamic blob format (§6.4): `MGSN` magic + `format` + `checksum_version` + `command_id` + `sim_tick` + `map_id`/`map_seed`/`map_hash` + `content_hash`/`build_id` + `next_entity_id` + `rules_json` + wave/wavetime + teams/objects + 04's entity chunk (buildings, items, power/fluid, non-possessed units, markers; possessed units are excluded — the joiner gets them from `match_player_state`). Compression: 04's deflate codec; blob split into `SNAPSHOT_CHUNK_BYTES = 16 KiB` rows.

Byte budget: `Dynamic` p95 ≤ 512 KB compressed, hard cap 4 MB; `chunk_count ≤ 256`; per-match retained snapshots: latest `Dynamic` + latest `Digest` + the `WorldReset` (3 max; sweep deletes older). Late join sequence: join → terrain gen → fetch latest snapshot metadata → if `Running`, subscribe Snapshot wave and download chunks (progress via `MindNet.snapshot_progress`) → restore → tail-replay (usually zero; host's latest dynamic snapshot is ≤60 s old) → `set_ready` → spawn.

### 3.9 Reconnect

Plan 01 owns reconnection (token, `Resync` event). This plan's rules:
- On disconnect, `MindNet` moves to `MatchSession::Reconnecting`, pauses sim ticks (upstream `state.set(State.paused)` on snapshot timeout), keeps the view live, and shows 14's connection-lost panel.
- On reconnect (`Connector::reconnect` → base/lobby re-subscribe → `Resync`): drop stale mirrors (binder queues), re-subscribe the Game wave, re-read `my_match`. If the match is still `Running` and the member row survives (`connected=false` → `join_match` flips it back), do the late-join snapshot path (§3.8); if `Ended` or member gone, return to menu.
- Command continuity: STDB re-delivers the full `my_match_commands` view on re-subscribe, so `CommandStream` resumes from the start of the retained log; the applied-watermark dedup makes replay harmless. A pruned gap → snapshot path.
- Dedicated-server matches never pause on any single client disconnect.

### 3.10 Admin, bans, whitelist, filters, rate limits

Global admin is an `admin_identity` table (bootstrap in §6.8), per-match authority is `RelayMatch.host`. Admin reducers:

| Action | Reducer | Cheap checks | Effect |
|---|---|---|---|
| kick | `admin_kick(target, reason)` | actor admin or match host; target is member; target != actor; rate 5/10 s | `RelayMember.kicked_reason = reason`; target's `my_kick` drives disconnect; `admin_action_log` |
| ban | `admin_ban(target, reason, duration)` | admin only; target not admin; duration cap 30 d | `player_ban` row + kick |
| unban | `admin_unban(target)` | admin | delete row |
| whitelist on/off | `admin_set_whitelist(bool)` | admin | `server_config.whitelist_enabled` |
| whitelist add/remove | `admin_whitelist(target, on)` | admin | `whitelist_entry` |
| trace | view `admin_trace(target)` | admin (view checks `admin_identity`) | reads `player`/`player_session`/`admin_action_log` (no IPs exist; includes identity hex, usernames, join/kick counts) |
| wave | `admin_skip_wave()` / `admin_run_wave(count)` | admin or host | inserts a `SkipWave`/`RunWave` `match_command` row (sender actor) so all peers apply in order |
| switch team | `admin_switch_team(target, team)` | admin or host | `AdminSwitchTeam` command row + `RelayMember.team` update |
| set config | `admin_set_config(field, value)` | admin | `server_config` typed fields (no free-form keys) |
| set tile | `admin_tile_op(...)` | admin or host | `AdminTileOp` command row (06's remote ops) |

Rate limits: `CommandRate` windows (plan 01 shape) per class — `ALL=40/s`, `INPUT=25/s` (separate reducer), `PLAN=16 chunks/s`, `UI=10/s`, `LOGIC=30/s`; chat `chat_rate` 1/2 s + 20/2 s spam threshold (mute 60 s, then kick); `admin_rate` 5/10 s/action. Rejections are `Err` + `log::warn!` (plan 01 R7); moderation effects are committed rows.

Chat filters: `chat_filter { pattern (lowercase substring), mute, added_by }`; applied on insert; default empty; mods/20 may append at match creation (host-published `chat_filter` rows are global, admin-managed only under D2 — documented limitation, §8 R-21-10).

Server console: plan 22 owns the console/binary; the contract is (a) local stdin commands call the same `MindNet`/`mind-stdb` admin APIs, (b) a dedicated server's admin identity is its token identity seeded into `admin_identity`, (c) `spacetime logs` + `admin_action_log`/`audit_log` are the persistent record. Mapping table `ServerControl` command → reducer/API is in §4.

### 3.11 Server browser and dedicated headless server

- **Browser**: `all_matches` public view: `match_id, host_name, map_id, mode/mode_name, player_count, max_players, status, wave, ping_hint, is_dedicated, mod_count`. 14's JoinDialog renders it; refresh is a subscription (no polling). `Unlisted` matches are excluded (join requires a match id, shared out of band).
- **Dedicated server** (shape finalized with 22): a `mind-headless` mode (`mind-headless serve --stdb-host --db --admin-token-file --match-config <path>`) or a thin `mind-server` binary that: boots `mind-core` content + `mind-stdb` online, authenticates as its service identity (a token file, never a player token), calls `create_match(is_dedicated=true, visibility=…)` or joins an existing match as host, runs the `Sim` (it may have zero player-owned units; it is the snapshot/digest authority), publishes `match_state`, hosts admin console commands, and re-hosts the next map on game over (upstream `ServerControl.gameOverListener`). Plan 22 owns the export preset, process entry and console; this plan owns the match/reducer contract.
- **Auto-pause**: `match_state.paused` published by host; `server_config.auto_pause` parity (upstream `Config.autoPause`): dedicated host pauses when `player_count == 0` and resumes on join (PvP waiting-room behavior preserved via `paused && player_count < 2` when `rules.pvp`).

### 3.12 Campaign persistence, schematics, settings, mod handshake

1. **Campaign (MP)**: a `campaign` row (`campaign_id`, `host`, `planet`, `turn`, `seconds`, `rules_json`, `rules_epoch`) owns `sector_info`, `unlock`, `campaign_stats` rows. `RelayMatch.campaign_id`/`sector_*` link a match to the campaign. Only host/campaign members write (`save_sector_info`, `set_unlock`, `add_campaign_stat`, `advance_turn` reducers). Plan 12's local file saves remain the single-player format; MP uses STDB (12 §6.7 shapes, schema owned here). `SetRules`/`ResearchUnlock`/`CompleteObjective`/`SectorCapture`/`SaveSector` commands are the sim-visible intents; the campaign reducer writes the persistent row on the same command (host only), so a late joiner reads campaign state from tables.
2. **Schematics**: `schematic { owner, name, base64, tags_json }` with per-owner view `my_schematics`; CRUD reducers (`save_schematic`, `delete_schematic`, `import_schematic`) validated by size (`MAX_SCHEMATIC_BASE64 = 128 KB`) and name length. 12/04 keep local files; STDB is the sync/share layer. Schematic sharing inside a match is out of scope (users paste base64/chat).
3. **Settings**: plan 01's `client_settings` row is unchanged; this plan adds match-scoped preferences into it only if needed by 14 (documented as an additive JSON field later).
4. **Mod handshake** (with 20): at match creation the host publishes `match_mod { name, version, content_hash }` for every loaded mod (vanilla = implicit). Join sends `mods` + `content_hash`; the module compares sets and rejects with `Missing mods: …` / `Unnecessary mods: …` (upstream `NetServer` semantics). `content_catalog` (seeded from the vanilla content manifest at publish time, §6.8) is used for cheap content-name validation in commands when the match is vanilla; when mods are present, existence checks are skipped (names must still match charset/length) because per-mod catalogs are not replicated. Flagged §8 R-21-6.

### 3.13 Transport (OD3) and the seam

**Decision**: STDB event tables carry **everything** — commands, player state, plans, chat, UI events, checksums, snapshots (chunked rows), campaign writes. No direct transport ships in plan 21.

**Layered seams** (keep possible, do not build):
- `mind-stdb::transport::RelayTransport` trait: `send_command`, `subscribe_commands`, `send_player_state`, `report_plan`, `publish_snapshot_chunk`, `request_snapshot`, `drain`. `StdbTransport` is the only implementation now.
- Authority stays in STDB: a future `DirectTransport` (ENet/UDP) would be a **data plane only** — a designated bridge (host/dedicated server) still writes the canonical rows, or the direct path carries only bulk snapshot chunks addressed by `(match_id, snapshot_id, chunk_index)` with STDB as the manifest. Client command emission stays on reducers or is mirrored into rows by the server; the log remains the ordering source.

**Measurement that would trigger direct transport** (record in §7d; needs user sign-off at execution per HLP OD3):
1. `bench_relay_roundtrip` p95 sender→remote-apply > 250 ms sustained (10 clients, local + remote server), or
2. sustained committed command throughput saturates `RelayConfig.commands_per_second` while commands are legitimate (build spam) and p99 apply latency > 400 ms, or
3. a single `Dynamic` snapshot > 4 MB or snapshot push saturates a subscriber > 2 MB/s for > 5 s, or
4. STDB subscription change-rate (rows/s) in a 16-player match exceeds `RELAY_ROW_BUDGET = 1500 rows/s` p95.
Below all four, STDB remains the transport. The seam is tested by a `transport::tests::stdb_transport_contract` test so a swap is mechanical.

### 3.14 Room for the deferred authoritative STDB match sim

Kept compatible (no changes needed when the sim lands):
- `MatchCommand` envelope, `sender_seq`, `client_tick`, `RelayMatch.authority`, `match_state`, `match_snapshot`/`match_snapshot_chunk` shapes, `match_checksum`, all views, `SetRules`/campaign commands, admin reducers.
- Client mirror pipeline: `RelayRuntime` already consumes views; on `authority = Authoritative` it runs `AuthorityRuntime` (mirror snapshots + interpolate) instead of local command application. `SyncLocal` mask lifted; `match_player_state` becomes server-written (input rows become validated intents rather than authoritative state).
- Cheap validation becomes the sim's front gate (plan 01 kept `validate_kind` pure).

Additive columns/tables the future tick reducer would use:
- `MatchCommand.exec_tick: Option<u64>` (stamped by the sim; clients ignore it under Relay).
- `MatchCommand.sim_applied_at: Option<Timestamp>` (debug/audit).
- `match_sim_status` (singleton per match: `tick`, `lag_ticks`, `last_command_id`, `hosting_generation`), scheduled `sim_tick` reducer consuming `relay_match.authority == Authoritative` matches in `command_id` order, writing authoritative `match_state`, entity rows and checksums. Client applies `exec_tick` deterministically if it still runs a shadow sim, or mirrors snapshots.

What changes: authority flips in one row; no client schema migration; no command field rewrite. This is the D2 promise from plan 01 §3.10, carried forward.

### 3.15 Godot / gdext surfaces

- `mind-gdext::net::MindNet` — autoload `Node` at `/root/MindNet` (registered in `project.godot`). It owns `MatchSession`, the `CommandSender`, the `RelayRuntime` mapping, the snapshot driver, and exposes `#[func]`s (14 and tests consume; additions are append-only):
  `create_match(map_id, seed, mode, visibility, max_players, rules_json) -> i64`, `join_match(match_id, password) -> bool`, `leave_match() -> bool`, `start_match(force) -> bool`, `set_ready(bool) -> bool`, `send_command_json(kind_json) -> bool` (dev/test), `get_match_state_json() -> GString`, `get_members_json() -> GString`, `get_remote_player_state(identity_hex) -> Dictionary`, `send_chat(text) -> bool`, `send_ui_result(kind, payload) -> bool`, `request_snapshot() -> bool`, `snapshot_progress() -> f32`, `last_applied_command_id() -> i64`, `checksum_report() -> Dictionary`, `relay_queue_depth() -> i64`, `set_authority_mode(mode) -> bool` (dev), `dev_inject_divergence() -> bool` (test only, debug builds), `session_state() -> String`.
- `MindSimHost` (plan 00) gains: `pending_commands` drain hook, `apply_sim_command_json(cmd_json) -> bool`, `get_checksum_scoped() -> GString`, `restore_snapshot(path) -> bool`, `save_snapshot(path) -> bool`, `get_rng_state_json() -> GString`. The relay layer calls these; no sim rules in `mind-gdext`.
- Inspector: plan 01's `net` page gains `session_state`, `match_id`, `applied_command_id`, `queue_depth`, `last_checksum`, `checksum_peers`, `snapshot_progress`, `authority`, `resync_count`. GDScript reads only.
- Scenes: no new game scenes; 14 owns join/host/chat/admin UI which drives `MindNet`.

### 3.16 Sibling reconciliation (by filename)

| Sibling | Frozen interface this plan consumes | This plan provides / reconcile action |
|---|---|---|
| `01_PLATFORM_STDB_IMPLEMENTATION_PLAN.md` | Envelope, `CommandStream`, wave mechanics, `frame_tick` frame order, cheap-validation split, `RelayMatch`/`RelayMember`/`MatchCommand`/`CommandRate`. | Extends tables/variants (hard cut); owns the full schema and `AuthorityMode::Authoritative` switch. Orchestrator: regenerate bindings once at M0; plan 01's dev helpers (`dev_create_match` etc.) are superseded by `MindNet` (remove at 21 M1 — update 01's §3.11 note). |
| `05_SIM_CORE_IMPLEMENTATION_PLAN.md` | `SimCommand`, `CommandLog`, `Sim::command`, `Sim::checksum`, `FieldMeta`, snapshot/restore hooks. | Requests additive `Sim::checksum_scoped(&ChecksumScope)` and `Sim::restore_snapshot`; owns the `CommandKind`→`SimCommand` map; owns the relay checksum cadence. |
| `06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` | Deterministic generation from `map_id`+`map_seed`, `load_map`, `WorldGrid`, `WorldContext`. | Late-join terrain regeneration; `map_hash` verification; `AdminTileOp` maps its remote helpers. |
| `10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md` | `BulletSpawn` field order; host-only `create_net` sources; bullets never snapshotted/checksummed per 10 §6.4. | Owns `CommandKind::Bullet` payload (`BulletRelay`) and its mapping into `BulletSpawn`; no snapshot entity send. |
| `11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` | `UnitCommandQueue`/`UnitStance` additions, controller codec, spawn/death determinism. | Owns relay variants + validation + mapping; `match_player_state` carries possessed-unit state only. |
| `12_CAMPAIGN_IMPLEMENTATION_PLAN.md` | `SetRules`/`ResearchUnlock`/`CompleteObjective`/`SectorCapture`/`SaveSector` intents, `RulesBlob`, row shapes (`SectorInfoRow`/`UnlockRow`/`CampaignStatsRow`/`SchematicRow`). | Owns the STDB tables/reducers (`campaign`, `sector_info`, `unlock`, `campaign_stats`, `schematic`); `rules_json` is a String (12's shape says blob — reconcile at M6). |
| `13_LOGIC_MLOG_IMPLEMENTATION_PLAN.md` | `LogicSyncEvent`/`ClientLogicDataEvent` shapes, `Configure` blobs, host-only rule ops. | Owns ordered `LogicSync`/`LogicClientData` relay + rate limits + caps. |
| `14_UI_IMPLEMENTATION_PLAN.md` | `ui_node` bytes, payload table, `MenuResult` capture, Join/Host/Chat/Admin dialog data needs. | Owns `match_ui_event` transport + `MindNet` APIs; `MenuChoose`/`TextInputResult` command variants. |
| `15_INPUT_RTS_IMPLEMENTATION_PLAN.md` | `RemoteAction`/`ActionBatch`, prediction contract, plan payload/caps. | Owns echo/rejection/desync rules around the contract; consumes (does not duplicate) `RemoteAction`. |
| `16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` | Interpolation read APIs. | Provides `match_player_state` mirror as the remote-state source; remote units' interpolation targets come from both the sim and LWW state. |
| `17_FX_PARTS_IMPLEMENTATION_PLAN.md` | View-only contract; no sim effect state. | Nothing relayed for FX; all peers derive FX from the same ordered sim. |
| `20_MODS_IMPLEMENTATION_PLAN.md` (**not on disk**) | Mod list/hash surface, content catalog manifest. | `match_mod` + `content_catalog` + join compatibility; orchestrator must reconcile the manifest generation at 20's M-plan. |
| `22_PLATFORM_EXPORT_IMPLEMENTATION_PLAN.md` (**not on disk**) | Export presets, console, server binary, Steam. | Dedicated server shape + admin reducer contract; orchestrator reconciles the binary name/entry point and `server_config` mapping. |
| `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` (**not on disk**) | Scenario/bench registry, goldens. | Registers `mp_*` scenarios/benches; orchestrator adds the golden files to 23's suite. |
| `02_CONTENT`, `04_IO`, `07_BLOCKS`, `08_LOGISTICS`, `09_POWER` | Content names as ABI; TypeIO/ConfigValue blobs; building/IO formats. | Consumes only; no edits requested (config blobs go through 04 codecs; `AdminTileOp` calls 06 ops). |

### 3.17 Invariants

1. The game boots and plays **single-player** with `StdbMode::Offline` and no server; `MindNet` is a no-op in offline mode.
2. `mind-core` never depends on `mind-stdb` or Godot; `mind-stdb` never depends on `mind-core`/Godot.
3. Commands are applied only at fixed-tick boundaries in `command_id` order; arrival wall-clock time never influences sim order.
4. `match_command` is append-only from clients; only the sweep deletes rows, and only behind the snapshot watermark.
5. Reducers return no data; `ctx.sender()` is the only principal; failed reducers roll back (no partial writes).
6. Every command variant has a row in the §6.3 mapping table and a validation entry in §6.3's caps table; adding a variant without both fails the `command_kind_matrix` test.
7. A client never re-applies its own echoed command; duplicates by `(sender, sender_seq)` are ignored.
8. The authority mask is applied **only** to comparisons; `Sim::checksum()` semantics (05) are unchanged.
9. Access control is per-view: a caller can never read another match's commands/chat/snapshots (member semijoins only).
10. Generated bindings are never hand-edited; `server/build.sh` is the only writer.
11. Every GPL header/notice rule from HLP §6.4 holds; content names and keys are ABI.

---

## 4. Port map

Legend: **S** = `server/spacetimedb/src`; **MS** = `mind-stdb`; **GX** = `mind-gdext`; **MH** = `mind-headless`; **MC** = `mind-core`. “Apply path” = how the action reaches the local sim.

| Mindustry source | Target | Notes / apply path |
|---|---|---|
| `core/NetClient.java` (`Connect`/`Disconnect` handlers) | `MS::session::MatchSession`, `GX::net::session`; `GX::net::MindNet` signs | Join/leave state machine; token/reconnect owned by 01; UI panels by 14. |
| `NetClient.sync()` (`clientSnapshot`, 66 ms) | `MS::commands::report_player_state`, `S::relay::player_state` | LWW row; puppet-apply on peers; `PredictPolicy::LocalOnly`. |
| `NetClient.sync()` plan snapshots (0.5 s, chunk 75) | `MS::commands::report_plan_snapshot`, `S::relay::plans` | LWW active group; view `my_match_plans`. |
| `NetClient.sendChatMessage` / `sendMessage` | `S::chat::{send_chat, match_chat}`, view `my_match_chat` | Relay row; filters/rate; local echo via view. |
| `NetClient.setRules` | `CommandKind::SetRules` (host) | Ordered apply; `match_state`/`relay_match.rules_json` updated in the same tx. |
| `NetClient.setRule` | `CommandKind::SetRule` (host) | Single-field host edit. |
| `NetClient.setObjectives`/`completeObjective`/`clearObjectives` | 12 intents → `CommandKind::CompleteObjective`/`ClearObjectives` | Objectives blob is capped by 04; `CompleteObjective{index, rules_epoch}` ordered. |
| `NetClient.kick`/`KickReason` | `S::admin::admin_kick` + `match_kick`/`RelayMember.kicked_reason`; `KickReason` enum in the module | No direct client targeting; target watches `my_kick`. |
| `NetClient.setPosition`/`setCameraPosition` | `setPosition` → future `AdminTeleport` (documented, not shipped); `setCameraPosition` dropped | D2 has no server correction; camera is local (15). |
| `NetClient.playerDisconnect`/`readSyncEntity`/`entitySnapshot`/`hiddenSnapshot`/`blockSnapshot`/`stateSnapshot` | Local sim + `match_state`/`match_checksum`/snapshots | World cohort is deterministic; possessed units via LWW; fog is client-side. |
| `core/NetServer.java` connect/`ConnectPacket` checks | `S::relay::reducers::join_match` + `S::mods` + `S::admin` | Ban/whitelist/version/mod/cap/password checks; error strings mirror `KickReason`. |
| `NetServer.clientSnapshot` | `S::relay::player_state::report_player_state` | Position/aim/input flags; no server integration under D2 (owner-authoritative). |
| `NetServer.clientPlanSnapshot` | `S::relay::plans::report_plan_snapshot` | Chunked LWW. |
| `NetServer.adminRequest` (`AdminAction`) | `S::admin` reducers | kick/ban/trace/wave/switchTeam mapped; wave/team emit ordered commands. |
| `NetServer.sync()`/`writeStateSnapshot` | `S::relay::state::publish_match_state` (host) | wave/wavetime/enemies/paused/gameOver + rules; RNG in digests. |
| `NetServer.writeEntitySnapshots*`/`buildHealthUpdate` | `S::relay::snapshot::publish_snapshot` (host) | One dynamic blob instead of UDP snapshots; not per-tick. |
| `NetServer.requestWorld`/`requestAssets`/`sendWorldAndAssets` | `MS::snapshot` + 20 asset handshake | Terrain by seed; mod assets via 20/03; snapshot chunks via STDB. |
| `NetServer.connectConfirm`/`WorldReloader` | `S::relay::reducers::{set_ready,start_match}` + `CommandKind::AdminTileOp`/`WorldReset` snapshot | Host reloads a new map via a new `match_id` (upstream reload keeps players; documented deviation). |
| `net/Net.java` registry/dispatch/`NetProvider` | `MS::connector` (01) + `MS::transport::RelayTransport` | Packet ids do not exist; generated reducer names replace `packetToId`. |
| `net/Packet.java` (`priority`, `allow`, `handled`) | Subscription waves + `PredictPolicy` + reducer guards | Priority → wave/pending order; `allow(server)` → role/membership checks. |
| `net/Packets.java` (`ConnectPacket`, streams, `KickReason`, `AdminAction`) | `join_match` args, `match_snapshot_chunk`, `match_kick.reason`, admin actions | No base64 uuid/usid; STDB Identity + `player.usid` from 01. |
| `net/NetworkIO.writeWorld/loadWorld` | `MS::snapshot` + 04 entity codec + 06 seed regen | Dynamic-only blob; terrain regenerated. |
| `net/NetworkIO.writeServerData/readServerData`/`Host` | `all_matches` view + 14 browser | No ping/discovery; `Host` → view row struct. |
| `net/Streamable.java`/`StreamBuilder` | `MS::snapshot` chunk download/upload + progress events | `IncrementalStream` semantics not needed (chunk rows). |
| `net/NetConnection.java` | `RelayMember` + `match_player_state.seq` + `CommandRate`/`chat_rate`/`admin_rate` | `localEntities` deferred to authoritative sim. |
| `net/Administration.java` (`playerInfo`, bans, whitelist, filters, `Config`) | `S::admin`, `S::chat`, `server_config`, `player`/`player_session` (01) | No IPs/subnets; `PlayerInfo` → player/session/admin_action_log. |
| `net/WorldReloader.java` | `CommandKind::AdminTileOp` + host `WorldReset` snapshot/re-host | 12's `WorldReloader` trait impl lives there; relay bodies here. |
| `net/ValidateException.java` | `Err(String)` + `log::warn!` cheap validation | Never panic; no `ValidateException` type. |
| `annotations/.../RemoteProcess.java`, `CallGenerator`, `Call` | `CommandKind` + reducer names + §6.3 mapping; 14 payload table | No codegen; a table-driven test (invariant §3.17-6) replaces generator-time exhaustiveness. `Loc` targets → `PredictPolicy`/roles. |
| `Tile.java` `@Remote` helpers | 06 pub fns; relayed as `CommandKind::AdminTileOp` (host/admin) or normal Place/Break/Configure | 06 §3.4 owns the op list. |
| `Units.unitDeath`/`notifyUnitSpawn` | 11 deterministic lifecycle; no per-event relay | Applies identically on every peer from the sim; authoritative sim later adds rows. |
| `Units.unitCommand`/`InputHandler` remotes | 15 `RemoteAction` → `CommandKind::{UnitCommand,UnitCommandQueue,UnitStance,Rotate,DeletePlans,CommandBuilding,Inventory,Payload,UnitControl,UnitClear,BuildingControlSelect}` | §6.3 map; ordered apply. |
| `WaveSpawner.java` `@Remote` | 11 waves deterministic; `admin_run_wave`/`SkipWave`/`RunWave` | No per-spawn relay under D2. |
| `Logic.java` `@Remote` (`setrule`, `setflag`, `setblock`, `spawnwave`, sync/clientdata) | `CommandKind::{SetRule,AdminTileOp,SkipWave,LogicSync,LogicClientData}` | 13's shapes; host-only where upstream is host-only. |
| `Menus.java` `@Remote` | `match_ui_event` + `MenuChoose`/`MenuBuilderChoose`/`TextInputResult` | 14 owns payload bytes. |
| `NetClient.effect`/`sound`/`playMusic` remotes | not relayed | All peers derive from sim; music is local/14/18. |
| `server/src/mindustry/server/ServerControl.java` | `S::admin` + `MS::session`; binary/console by 22 | Command → reducer/API table below. |
| `client/sstdbsdk/*` (C#) | `MS` (01) + this plan's `session.rs` | 01 already ports the SDK; this plan adds match semantics. |

`ServerControl` mapping (console parity; 22 wires the stdin/socket):

| Upstream command | Port |
|---|---|
| `host [map] [mode]`, `stop`, `status` | `create_match`/`start_match`/`leave_match`/`MindNet.serve_status` |
| `config`, `rules`, `pause`, `gameover`, `runwave`, `fillitems` | `admin_set_config`, `SetRules`/`SetRule` command, `publish_match_state(paused)`, `RunWave`, `SkipWave`, future `AdminFillItems` (documented, not shipped) |
| `kick`, `ban`, `bans`, `unban`, `pardon`, `admin`, `admins`, `whitelist`, `name-ban`, `subnet-ban` | `admin_*` reducers (name-ban = `server_config.banned_name_patterns`; subnet-ban has no data source → rejected with a clear message) |
| `players`, `info`, `search` | `my_match_members`/`all_players` (admin-filtered) + `admin_action_log` |
| `save`, `load`, `saves`, `loadautosave` | campaign `save_sector_info`/12 local files (dedicated server keeps local saves too; documented) |
| `say` | `send_chat(kind=System)` |
| `maps`, `reloadmaps`, `shuffle`, `nextmap` | `all_matches`/Map registry (06) + `create_match` rotation |
| `mods`, `mod`, `js` | 20; `js` is OD1-deferred |

---

## 5. Milestones & task breakdown

Ordered; each milestone ends with its verification commands, evidence in the Changelog. Smallest vertical slice first: **M0–M1** must give a working two-client lobby with no commands.

### M0 — Schema cutover + bindings + publish
- [ ] Extend `relay/tables.rs` (`RelayMatch`/`RelayMember`/`CommandRate` columns; `CommandKind` full variant set with the two widened payloads); add `main/global.rs` constants; regenerate bindings (`server/build.sh`); fix plan 01's M4 tests.
- [ ] Spike (timeboxed): `Vec<u8>` row of 16 KiB + `rules_json` 100 KB insert/subscribe on `spacetime start` (de-risks R-21-2).
- [ ] `MS` mirrors: `commands.rs` caps/preflight, `session.rs` skeleton state machine.
- **Verify:** `server/build.sh --check`; `cargo check --manifest-path server/spacetimedb/Cargo.toml --tests`; `cargo test -p mind-stdb` (01's tests still green); `spacetime sql` shows the new columns/tables.

### M1 — Match lifecycle, lobby, browser, readiness (no sim commands)
- [ ] `S::relay::reducers::{create_match, join_match, leave_match, start_match, set_ready}`, `state.rs` (`publish_match_state`), views (`all_matches`, `my_match_members`, `my_match_state`, `my_kick`); `S::mods::join` compatibility; admin bootstrap seed.
- [ ] `MS::session` host/join/leave/start over reducers; `MS::snapshot` metadata fetch; `GX::net::MindNet` autoload + inspector page.
- [ ] Player cap, password, visibility, ban/whitelist checks; `player_count` maintenance.
- **Verify:** §7b `mp_lobby_join`; §7c MCP steps 1–2 (two instances join, host starts); server tests `admin::tests::{ban_blocks_join, whitelist_blocks_join}`; bindings drift check.

### M2 — Command relay end-to-end (place/break/config)
- [ ] `MS::commands::CommandSender` + `GX::net::relay::RelayRuntime` mapping, echo skip, `PredictPolicy`; `MindSimHost.pending_commands` drain at tick start; `sender_command_state` view.
- [ ] Validation matrix implementation + shared fixture (`tests/fixtures/relay/validation_matrix.json`) used by module tests and headless.
- [ ] Bounds/rate/team/host gates; `match_command` pruning deferred to M5.
- **Verify:** §7b `mp_command_log_replay`, `mp_validation_matrix`; §7c MCP `mp_two_client_build_visible`; `it::two_clients_create_join_place_break`.

### M3 — Player state, plans, chat, UI events, `SetRules`
- [ ] `S::relay::{player_state, plans, ui_events}`, `S::chat`; `MS` report APIs; `GX` puppet application + `get_remote_player_state`; 14 payload hooks.
- [ ] `SetRules`/`SetRule` host-only path + `match_state.rules_json` update; campaign hooks stubbed.
- **Verify:** §7c MCP `mp_movement_sync`, `mp_chat_admin_kick`; `it::player_state_roundtrip`; server tests `chat::tests::{rate_limit,length_cap,filter_mutes}`.

### M4 — Checksums, digests, desync detection/correction
- [ ] `Sim::checksum_scoped` (05 coordination) + `ChecksumScope` mask; `S::relay::checksum`; `MS::checksum_monitor`; `GX` resync state.
- [ ] `request_snapshot`/`publish_snapshot`; dynamic snapshot encode/restore path; tail replay from the local command ring buffer.
- **Verify:** §7b `mp_desync_injection`; tests `checksum::tests::{compare_at_command_id, host_canonical_correction, version_mismatch_ignored}`; `it::checksum_mismatch_triggers_resync`.

### M5 — Late join, reconnect, retention sweeps
- [ ] Snapshot on-demand wave + `MindNet.snapshot_progress`; late-join sequence; reconnect path.
- [ ] `S::relay::sweep::{tick_maintenance}`: command prune by watermark, checksum/chat/ui retention, member grace, ended-match TTL, snapshot keep-3.
- **Verify:** §7b `mp_late_join`, `mp_reconnect_gap`; `it::late_join_snapshot_tail`; `sweep::tests` green.

### M6 — Admin, moderation, campaign/schematics, mod handshake
- [ ] `S::admin` full set + views + audit; `admin_tile_op`; `server_config`.
- [ ] `S::campaign` tables/reducers + `MS::session` campaign calls + 12 command wiring; `schematic` CRUD; `content_catalog` seed generation from the build.
- [ ] Join mod compatibility end-to-end (20 handshake).
- **Verify:** server tests `admin::tests::*`, `mods::tests::*`, campaign round-trips; §7c MCP `mp_admin_kick_ban`; `spacetime call admin_*` console path.

### M7 — Dedicated server shape + transport seam + authority switch stub
- [ ] `mind-headless serve`/22 binary contract; service-token identity; auto-pause; map rotation.
- [ ] `RelayTransport` trait + `StdbTransport`; `set_authority_mode` dev path (authoritative mode returns “not implemented” until the sim lands).
- **Verify:** dedicated server hosts, two clients join via browser, game-over rotation works; `transport::tests::stdb_transport_contract`.

### M8 — Perf, docs, exit gate
- [ ] Benches §7d; soak run 30 min + forced desync; `server/spacetimedb/AGENTS.md` delta section; `MindNet` API doc for 14/22; reconciliation notes to 20/22/23; exit checklist.
- **Verify:** §7d budgets recorded; §7e green; plan 23 registration list handed over.

---

## 6. Data & formats

### 6.1 Table catalog (full; server module)

Plan 01 tables unchanged unless listed. All new tables live in the modules of §3.1; `table` syntax follows `main/server/AGENTS.md`, e.g. `#[table(accessor = a, public, index(accessor = b, btree(columns = [c])))]`. “SO” = server-only (no `public`).

**Match core (`relay/`)**

```rust
#[table(accessor = relay_match, public, index(accessor = by_relay_status, btree(columns = [status, match_id])))]
pub struct RelayMatch {
    #[primary_key] #[auto_inc] pub match_id: u64,
    pub map_id: String, pub map_seed: u64, pub map_hash: u64,
    pub map_width_tiles: i32, pub map_height_tiles: i32,
    pub status: MatchStatus, pub authority: AuthorityMode,
    pub protocol_version: u32,
    pub created_by: Identity, pub created_at: Timestamp,
    pub started_at: Option<Timestamp>, pub ended_at: Option<Timestamp>,
    // 21 extensions:
    pub host: Identity, pub mode: Gamemode, pub mode_name: String,
    pub visibility: Visibility, pub password_hash: Option<u64>,
    pub rules_json: String, pub rules_epoch: u32,
    pub build_id: String, pub content_hash: u64,
    pub is_dedicated: bool,
    pub player_count: u16, pub max_players: u16,
    pub campaign_id: Option<u64>, pub sector_planet: Option<String>, pub sector_id: Option<u32>,
    pub last_command_id: u64, pub last_snapshot_id: Option<u64>,
    pub closed_at: Option<Timestamp>,
}

#[table(accessor = relay_member, public,
  index(accessor = by_match_identity, btree(columns = [match_id, identity])),
  index(accessor = by_identity_match, btree(columns = [identity, match_id])))]
pub struct RelayMember {
    #[primary_key] #[auto_inc] pub member_id: u64,
    #[index(btree)] pub match_id: u64,
    pub identity: Identity, pub joined_at: Timestamp,
    pub role: MemberRole, pub team: Option<u8>,
    pub connected: bool, pub ready: bool,
    pub last_seen_at: Timestamp,
    pub kicked_reason: Option<String>,
}
```

**Ordered command log** stays exactly plan 01's `MatchCommand`/`match_command` (private; exposed by `my_match_commands`). Additions:

```rust
#[table(accessor = sender_command_state)]   // SO; per-caller view my_sender_command_state
pub struct SenderCommandState {
    #[primary_key] pub identity: Identity,
    pub last_accepted_seq: u64, pub last_accepted_command_id: u64,
    pub reject_count: u32, pub last_gap_at: Option<Timestamp>,
    pub updated_at: Timestamp,
}
```

**Match state (`relay/state.rs`)** — host-published state snapshot equivalent:

```rust
#[table(accessor = match_state, public, index(accessor = by_match_state, btree(columns = [match_id])))]
pub struct MatchState {
    #[primary_key] #[auto_inc] pub state_id: u64,
    #[index(btree)] pub match_id: u64,
    pub rules_json: String, pub rules_epoch: u32,
    pub wave: i32, pub wavetime: f32, pub enemies: i32,
    pub paused: bool, pub game_over: bool,
    pub sim_tick: u64, pub last_command_id: u64,
    pub rng_sim: Vec<u8>, pub next_entity_id: i32,
    pub snapshot_id: Option<u64>, pub updated_by: Identity, pub updated_at: Timestamp,
}
```

**Player state (`relay/player_state.rs`)** — LWW per `(match_id, identity)`:

```rust
#[table(accessor = match_player_state, public,
  index(accessor = by_match_player, btree(columns = [match_id, identity])))]
pub struct MatchPlayerState {
    #[primary_key] #[auto_inc] pub state_id: u64,
    #[index(btree)] pub match_id: u64,
    pub identity: Identity,
    pub seq: u32, pub unit_id: i32, pub dead: bool,
    pub x: f32, pub y: f32, pub vx: f32, pub vy: f32,
    pub pointer_x: f32, pub pointer_y: f32,
    pub rotation: f32, pub base_rotation: f32,
    pub mining_x: i16, pub mining_y: i16,          // -1 = none
    pub boosting: bool, pub shooting: bool, pub chatting: bool, pub building: bool,
    pub selected_block: Option<String>, pub selected_rotation: u8,
    pub view_x: f32, pub view_y: f32, pub view_width: f32, pub view_height: f32,
    pub health: f32, pub shield: f32, pub team: u8,
    pub updated_at: Timestamp,
}
```

**Plan snapshots (`relay/plans.rs`)**:

```rust
#[table(accessor = match_plan_chunk, public, index(accessor = by_plan_group, btree(columns = [match_id, identity, group_id, chunk_index])))]
pub struct MatchPlanChunk {
    #[primary_key] #[auto_inc] pub chunk_id: u64,
    #[index(btree)] pub match_id: u64,
    pub identity: Identity, pub group_id: u32,
    pub chunk_index: u16, pub chunk_count: u16,
    pub plans_blob: Vec<u8>,                   // 04 TypeIO plan encoding; ≤ 8 KiB
    pub received_at: Timestamp,
}

#[table(accessor = match_plan_state, public, index(accessor = by_plan_state_match, btree(columns = [match_id, identity])))]
pub struct MatchPlanState {
    #[primary_key] #[auto_inc] pub plan_state_id: u64,
    #[index(btree)] pub match_id: u64,
    pub identity: Identity, pub active_group_id: u32,
    pub plan_count: u16, pub updated_at: Timestamp,
}
```

**Checksums (`relay/checksum.rs`)**:

```rust
#[table(accessor = match_checksum, public, index(accessor = by_checksum_match, btree(columns = [match_id, command_id])))]
pub struct MatchChecksum {
    #[primary_key] #[auto_inc] pub checksum_id: u64,
    #[index(btree)] pub match_id: u64,
    pub sender: Identity, pub command_id: u64, pub sim_tick: u64,
    pub checksum: u64, pub checksum_version: u32,
    pub scope: u8,                             // ChecksumScope bitset (cohort|mask)
    pub created_at: Timestamp,
}
```

**Snapshots (`relay/snapshot.rs`)**:

```rust
#[table(accessor = match_snapshot, public, index(accessor = by_snapshot_match, btree(columns = [match_id, created_at])))]
pub struct MatchSnapshot {
    #[primary_key] #[auto_inc] pub snapshot_id: u64,
    #[index(btree)] pub match_id: u64,
    pub author: Identity, pub kind: SnapshotKind,       // WorldReset | Dynamic | Digest
    pub format: u32, pub checksum_version: u32,
    pub command_id: u64, pub sim_tick: u64, pub checksum: u64,
    pub map_id: String, pub map_seed: u64, pub map_hash: u64,
    pub build_id: String, pub content_hash: u64,
    pub bytes_len: u32, pub chunk_count: u16,
    pub created_at: Timestamp,
}

#[table(accessor = match_snapshot_chunk, index(accessor = by_snapshot_chunk, btree(columns = [snapshot_id, chunk_index])))]
pub struct MatchSnapshotChunk {
    #[primary_key] #[auto_inc] pub chunk_id: u64,
    #[index(btree)] pub snapshot_id: u64,
    pub chunk_index: u16, pub data: Vec<u8>,            // ≤ 16 KiB
}

#[table(accessor = match_snapshot_request)]      // SO; host watches via my_match_snapshot_requests
pub struct MatchSnapshotRequest {
    #[primary_key] #[auto_inc] pub request_id: u64,
    #[index(btree)] pub match_id: u64,
    pub identity: Identity, pub requested_at: Timestamp,
}
```

**UI events (`relay/ui_events.rs`)**:

```rust
#[table(accessor = match_ui_event, public, index(accessor = by_ui_event_match, btree(columns = [match_id, event_id])))]
pub struct MatchUiEvent {
    #[primary_key] #[auto_inc] pub event_id: u64,
    #[index(btree)] pub match_id: u64,
    pub sender: Identity, pub kind: UiEventKind,
    pub target: Option<Identity>,
    pub payload: Vec<u8>,                       // 14 formats; ≤ 64 KiB
    pub sent_at: Timestamp,
}
```

**Chat (`chat/mod.rs`)**:

```rust
#[table(accessor = match_chat, public, index(accessor = by_chat_match, btree(columns = [match_id, chat_id])))]
pub struct MatchChat {
    #[primary_key] #[auto_inc] pub chat_id: u64,
    #[index(btree)] pub match_id: u64,
    pub sender: Identity, pub kind: ChatKind,   // All | Team | System
    pub team: Option<u8>, pub text: String,
    pub sent_at: Timestamp,
}

#[table(accessor = chat_rate)]                  // SO
pub struct ChatRate {
    #[primary_key] pub identity: Identity,
    pub window_start: Timestamp, pub posts: u32,
    pub infractions: u32, pub muted_until: Option<Timestamp>,
}

#[table(accessor = chat_filter, public)]
pub struct ChatFilter {
    #[primary_key] #[auto_inc] pub filter_id: u64,
    pub pattern: String, pub mute: bool,        // lowercase substring
    pub added_by: Identity, pub added_at: Timestamp,
}
```

**Kick (`relay/state.rs`)**:

```rust
#[table(accessor = match_kick)]                 // SO; view my_kick
pub struct MatchKick {
    #[primary_key] #[auto_inc] pub kick_id: u64,
    #[index(btree)] pub match_id: u64,
    pub target: Identity, pub reason: String,
    pub by: Identity, pub at: Timestamp,
}
```

**Admin (`admin/`)**:

```rust
#[table(accessor = server_config, public)]      // singleton id 0
pub struct ServerConfig {
    #[primary_key] pub id: u8,
    pub server_name: String, pub description: String, pub motd: String,
    pub whitelist_enabled: bool, pub allow_custom_clients: bool,
    pub max_players_default: u16, pub auto_pause: bool,
    pub chat_rate_window_ms: u32, pub chat_rate_max: u32,
    pub snapshot_interval_ticks: u32, pub digest_interval_ticks: u32,
    pub checksum_interval_ticks: u32, pub command_retention_commands: u64,
    pub chat_retention_rows: u32, pub match_ttl_hours: u32,
    pub banned_name_patterns: Vec<String>,
}

#[table(accessor = admin_identity, public)]
pub struct AdminIdentity {
    #[primary_key] pub identity: Identity,
    pub granted_by: Identity, pub granted_at: Timestamp,
}

#[table(accessor = player_ban, public, index(accessor = by_ban_identity, btree(columns = [identity])))]
pub struct PlayerBan {
    #[primary_key] #[auto_inc] pub ban_id: u64,
    #[index(btree)] pub identity: Identity, pub reason: String,
    pub banned_by: Identity, pub created_at: Timestamp,
    pub expires_at: Option<Timestamp>,
}

#[table(accessor = whitelist_entry, public)]
pub struct WhitelistEntry {
    #[primary_key] pub identity: Identity,
    pub added_by: Identity, pub added_at: Timestamp,
}

#[table(accessor = admin_rate)]                 // SO
pub struct AdminRate {
    #[primary_key] pub identity: Identity,
    pub window_start: Timestamp, pub actions: u32,
}

#[table(accessor = admin_action_log, public, index(accessor = by_admin_action, btree(columns = [at])))]
pub struct AdminActionLog {
    #[primary_key] #[auto_inc] pub action_id: u64,
    pub actor: Identity, pub kind: AdminActionKind,
    pub target: Option<Identity>, pub match_id: Option<u64>,
    pub detail: String, pub at: Timestamp,
}
```

**Campaign (`campaign/mod.rs`)** — 12's `*Row` shapes:

```rust
#[table(accessor = campaign, public)]
pub struct Campaign {
    #[primary_key] #[auto_inc] pub campaign_id: u64,
    pub host: Identity, pub planet: String,
    pub turn: u32, pub seconds: u64, pub turn_counter: u32,
    pub rules_json: String, pub rules_epoch: u32,
    pub created_at: Timestamp, pub updated_at: Timestamp,
}

#[table(accessor = sector_info, public, index(accessor = by_sector_campaign, btree(columns = [campaign_id, sector_id])))]
pub struct SectorInfo {
    #[primary_key] #[auto_inc] pub sector_row_id: u64,
    #[index(btree)] pub campaign_id: u64,
    pub planet: String, pub sector_id: u32,
    pub info_json: String,                      // 12 SectorInfoRow payload (items/production/import/export)
    pub wave: i32, pub win_wave: i32, pub waves: bool, pub attack: bool,
    pub minutes_captured: f32, pub playtime: u64,
    pub spawn_position: i32, pub last_preset_name: String, pub was_captured: bool,
    pub updated_at: Timestamp,
}

#[table(accessor = unlock, public, index(accessor = by_unlock_campaign, btree(columns = [campaign_id, content_name])))]
pub struct Unlock {
    #[primary_key] #[auto_inc] pub unlock_id: u64,
    #[index(btree)] pub campaign_id: u64,
    pub content_name: String, pub unlocked: bool,
    pub requirements_json: String,              // Vec<(item, delta)>
    pub updated_at: Timestamp,
}

#[table(accessor = campaign_stats, public, index(accessor = by_stats_campaign, btree(columns = [campaign_id, kind, key])))]
pub struct CampaignStat {
    #[primary_key] #[auto_inc] pub stat_id: u64,
    #[index(btree)] pub campaign_id: u64,
    pub kind: u8, pub key: String, pub value: i64,
    pub updated_at: Timestamp,
}

#[table(accessor = schematic, public, index(accessor = by_schematic_owner, btree(columns = [owner, updated_at])))]
pub struct Schematic {
    #[primary_key] #[auto_inc] pub schematic_id: u64,
    #[index(btree)] pub owner: Identity,
    pub name: String, pub base64: String,
    pub tags_json: String,
    pub created_at: Timestamp, pub updated_at: Timestamp,
}
```

**Mods (`mods/mod.rs`)**:

```rust
#[table(accessor = match_mod, public, index(accessor = by_match_mod, btree(columns = [match_id, name])))]
pub struct MatchMod {
    #[primary_key] #[auto_inc] pub match_mod_id: u64,
    #[index(btree)] pub match_id: u64,
    pub name: String, pub version: String, pub content_hash: u64,
}

#[table(accessor = content_catalog, public)]
pub struct ContentCatalog {
    #[primary_key] #[auto_inc] pub content_id: u64,
    #[unique] pub name: String,
    pub content_type: u8, pub source_mod: String,   // "" = vanilla
}
```

**Sweep (`relay/sweep.rs`)**:

```rust
#[table(accessor = maintenance_schedule, scheduled(tick_maintenance))]
pub struct MaintenanceSchedule {
    #[primary_key] #[auto_inc] pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,               // Interval 60 s
}
```

### 6.2 Views and subscription waves

| View | Wave | Caller scope | Contents |
|---|---|---|---|
| `all_matches` | Base or Browser (client chooses) | public | `relay_match` rows where `visibility = Public && status != Ended` (host name resolved per row) |
| `my_matches` (01) | Lobby | member | matches the caller belongs to |
| `my_match` (01) | Game | caller's active match (highest Running/Lobby `match_id`) | `relay_match` |
| `my_match_members` | Game | member | `RelayMember` semijoin over `my_match` |
| `my_match_state` | Game | member | latest `MatchState` for the match |
| `my_match_commands` (01) | Game | member | private ordered `match_command` semijoin |
| `my_match_checksums` | Game | member | `MatchChecksum` rows for the match (retention-bounded) |
| `my_match_player_states` | Game | member | `MatchPlayerState` for the match |
| `my_match_plans` | Game | member | `MatchPlanState` + chunks for the match |
| `my_match_chat` | Game | member | `MatchChat` (All + own Team + System), retention-bounded |
| `my_match_ui_events` | Game | member/target | `MatchUiEvent` for the match, `target` filter |
| `my_kick` | Game | member | `MatchKick` rows targeting the caller |
| `my_match_snapshots` | Game | member | `MatchSnapshot` metadata for the match (no chunks) |
| `my_match_snapshot_chunks` | **Snapshot (on demand)** | member | `MatchSnapshotChunk` rows for `my_match`; subscribed only while downloading |
| `my_match_snapshot_requests` | Game (host-only filter) | host | pending `MatchSnapshotRequest` rows for the match |
| `my_sender_command_state` | Game | caller | own `SenderCommandState` row |
| `my_schematics` | Lobby | owner | `Schematic` rows owned by the caller |
| `my_campaigns` / `my_campaign_sectors` / `my_campaign_unlocks` / `my_campaign_stats` | Lobby/Game | campaign host or relay member of a match linked to the campaign | campaign rows |
| `am_i_admin` | Lobby | caller | own `AdminIdentity` row or none |
| `admin_trace` (per target view impossible; use a reducer-free view with an argument is not supported in 2.x) | Base | admin | **Implementation**: a `trace` request table is overkill — the Admin dialog reads `all_players` (01) + `admin_action_log` + `my_match_members`, filtered client-side. `admin_trace` is therefore not a view; the data comes from `all_players`/`player_session` (admin-only exposure is a documented follow-up if 14 needs it). |
| `all_bans` / `all_whitelist` / `all_admins` / `all_chat_filters` | Lobby | admin-gated per-caller filter in the view body | admin lists |

Waves: plan 01's Base/Lobby/Game names stay. This plan adds **`WaveName::Snapshot`** (manual subscribe/unsubscribe managed by `MS::snapshot`, only ever active during a download). The Game wave grows to the list above; the browser uses a **`WaveName::Browser`** toggle (optional; `all_matches` may ride the Lobby wave if bundle size is acceptable — decision in M1 by measured row counts; default: Lobby wave, because the match table is small).

### 6.3 `CommandKind` and `SimCommand` mapping

Caps/validation (cheap; enforced server-side and mirrored client-side in `preflight_validate`). “ROLE” = who may send: any member (`M`), host-or-admin (`HA`), admin (`A`), server/host only (`H`), spectator-forbidden (`¬S`). `LEN` = list caps.

| `CommandKind` variant | Payload (STDB) | Caps | ROLE | `SimCommand` target | Predict |
|---|---|---|---|---|---|
| `Noop` | `{}` | — | M | none (watermark only) | Immediate |
| `Ping{nonce}` | `nonce: u64` | — | M | none | Immediate |
| `PlaceBlock` | `x,y: i32; block: String; rotation: u8; config: Vec<u8>` | name ≤ 100 chars, config ≤ 4 KB | M,¬S | `Place{x,y,block,rotation,team}` (05; team = member.team) | Immediate |
| `BreakBlock` | `x,y: i32` | — | M,¬S | `Break{x,y,team}` | Immediate |
| `ConfigBlock` | `x,y: i32; value: Vec<u8>` | value ≤ 16 KB | M,¬S | `Configure{x,y,ConfigValue}` (07) | Immediate |
| `Rotate` | `x,y: i32; direction: bool` | — | M,¬S | `Rotate{x,y,direction}` (15 request) | Immediate |
| `DeletePlans` | `positions: Vec<i32>` | ≤ 256 | M,¬S | `DeletePlans{positions}` (15) | Immediate |
| `CommandBuilding` | `positions: Vec<i32>; x,y: f32` | ≤ 256 | M,¬S | `CommandBuilding{positions,x,y}` (15) | Immediate |
| `Inventory` | `kind: u8; x,y: i32; item: Option<String>; amount: i32; angle: f32` | item ≤ 100; amount ≤ 10 000 | M,¬S | `Inventory{...}` (15) | Immediate |
| `Payload` | `kind: u8; x,y: f32; target: Option<i32>` | — | M,¬S | `Payload{...}` (15) | Immediate |
| `UnitControl` | `unit: Option<i32>` | — | M,¬S | `UnitControl{unit}` (15) | Immediate |
| `UnitClear` | `{}` | — | M,¬S | `UnitClear` (15) | Immediate |
| `BuildingControlSelect` | `x,y: i32` | — | M,¬S | `BuildingControlSelect{x,y}` (15) | Immediate |
| `UnitCommand` | `units: Vec<i32>; command: u16; x,y: f32` | units ≤ 200, command < 256 | M,¬S | `UnitCommand{units,command,x,y}` (05/11) | Immediate |
| `UnitCommandQueue` | same | ≤ 200 | M,¬S | `UnitCommandQueue{...}` (11 §6.5) | Immediate |
| `UnitStance` | `units: Vec<i32>; stance: u16; enabled: bool` | ≤ 200 | M,¬S | `UnitStance{...}` (11 §6.5) | Immediate |
| `PlayerSpawn` | `unit: Option<String>; team: u8` | name ≤ 100 | M,¬S (self only) | `SpawnUnit{unit,x,y,team}` resolved by sim spawn rules (05/11) | LocalOnly |
| `Bullet` | `def: u16; team: u8; x,y,angle,damage,velocity_scl,lifetime_scl,aim_x,aim_y: f32; data: Vec<u8>` | data ≤ 4 KB; aim = NaN when absent | H (host-owned `createNet` sources) | `SimCommand::Bullet(BulletSpawn{owner:None,shooter:None,..})` (10/05) | AwaitEcho |
| `SetRules` | `rules_json: String; rules_epoch: u32` | ≤ 100 000 chars; epoch == current | HA | `SetRules(Box<Rules>)` (05/12) | AwaitEcho |
| `SetRule` | `rule: String; json: String` | rule ≤ 64, json ≤ 16 KB | HA | `SetRule{rule,json}` (12/13) | AwaitEcho |
| `ResearchUnlock` | `content: String` | name ≤ 100 | HA | `ResearchUnlock{content}` (12) | AwaitEcho |
| `CompleteObjective` | `index: u32; rules_epoch: u32` | epoch match | HA | `CompleteObjective{index}` (12) | AwaitEcho |
| `ClearObjectives` | `{}` | — | HA | `ClearObjectives` (12) | AwaitEcho |
| `SectorCapture` | `{}` | — | HA | `SectorCapture` (12) | AwaitEcho |
| `SaveSector` | `{}` | — | HA | `SaveSector` (12; also writes campaign row) | AwaitEcho |
| `SkipWave` | `{}` | — | HA | `RunWave`-adjacent host fn (11/12) | AwaitEcho |
| `RunWave` | `count: u8` | count ≤ 10 | HA | `SpawnWave{count}` | AwaitEcho |
| `AdminSwitchTeam` | `target: Identity; team: u8` | — | HA | `SwitchTeam{player,team}` (05/12) | AwaitEcho |
| `AdminTileOp` | `op: u8; points: Vec<i32>; arg0..2: i32; name0/1: Option<String>` | points ≤ 1024, names ≤ 100 | HA | 06 pub ops (`set_tile_blocks`, `set_floors`, …) | AwaitEcho |
| `LogicSync` | `x,y: i32; var_name: String; var_id: i32; value: Vec<u8>` | var ≤ 64, value ≤ 1 KB | H | `LogicSync{...}` (13) | Immediate |
| `LogicClientData` | `channel: String; value: Vec<u8>; reliable: bool` | channel ≤ 64, value ≤ 8 KB | M (gated by `rules.allow_logic_data`) | `ClientLogicData{...}` (13) | Immediate |
| `MenuChoose` | `menu_id: i32; option: i32` | — | M | UI intent (14 consumes; no sim) | LocalOnly |
| `MenuBuilderChoose` | `menu_id: i32; result: Vec<u8>` | ≤ 8 KB | M | 14 | LocalOnly |
| `TextInputResult` | `id: i32; text: Option<String>` | ≤ 256 chars | M | 14 | LocalOnly |
| `Custom` | `kind: u16; data: Vec<u8>` | ≤ 1 KB | M | `SimCommand::Custom{kind,data}` (05; mod/OD1 hook) | AwaitEcho |

Validation rules not expressible as caps (server-side, all variants): match `Running` (except `PlayerSpawn`/player-state, which also work pre-start), sender is a member (`by_match_identity`), role rules, rate class, `sender_seq > CommandRate.last_sender_seq`, `client_tick` within `[match_state.sim_tick − 1800, match_state.sim_tick + 600]` (10 s lag / 3 s lead), coordinate bounds against `relay_match.map_width_tiles/height_tiles` for variants with tile coords, `content_catalog` name existence when `match_mod` is empty, `rules_epoch` equality, and host/admin gates (`relay_match.host == sender` or `admin_identity` exists). Explicitly **not** checkable cheaply (documented; sim/client re-checks): tile occupancy, resources, unit ownership/team, rule legality, collision, plan validity, spawn-point occupancy, per-building range.

### 6.4 Snapshot and digest formats

Dynamic blob (`match_snapshot_chunk.data` sequence, compressed with 04's deflate codec):

```
MGSN                        u32 magic
format                      u32 = 1
checksum_version            u32
command_id                  u64        watermark: all commands <= this are included
sim_tick                    u64
map_id                      UTF str
map_seed                    u64
map_hash                    u64
build_id                    UTF str
content_hash                u64
next_entity_id              i32        EntityIds next_id (05)
rules_json                  u32 len + UTF-8
wave                        i32
wavetime                    f32
rng_sim                     u32 len + bytes   RngStream::Sim state (05)
teams                       u32 len + 04 team/blob payload
entities                    u32 len + 04 entity chunk (buildings, items, non-possessed units, markers)
```

- Possessed units are excluded (their state lives in `match_player_state`); fires/puddles are included via the 04 entity chunk per 10 §6.3; bullets are never included (10 §6.4).
- Chunks are `SNAPSHOT_CHUNK_BYTES = 16 KiB` of the compressed blob; `chunk_count = ceil(len/16 KiB)`; `chunk_index` is stable.
- Digest rows carry no blob; `checksum`/`rng_sim` are copied into `match_state` for late readers.
- Restore path: `MS::snapshot` assembles chunks in order, verifies magic/format/map/content/checksum version, writes the blob to a temp file under `user://` (gdext) or the harness temp dir, then `MindSimHost.restore_snapshot(path)` → 04 read path with `generate_terrain=true` (06) → replay tail.

### 6.5 Checksum and cohort rules

`ChecksumScope` (bitset, sent in `scope`): bit 0 = `world_cohort` (always 1 in D2 relay), bit 1 = `include_possessed_unit_kinematics` (0 under Relay, 1 under Authoritative and in headless replay), bit 2 = `include_fxless_view_state` (always 0). `Sim::checksum_scoped` walks plan 05's exact hasher order; masked fields are replaced by their type defaults. Masked under Relay (bit 1 = 0): for each `Player`-controlled unit — `Pos`, `Vel`, `Rot`, `Health`, `Shield`, `Aim`, `MineTile`, and the input-flag set; the unit entity itself, its `SimId`/`DefId`, its type/team and all other components remain hashed, so spawn/despawn still diverges visibly. `checksum_version` = plan 05's `CHECKSUM_VERSION`; rows with different versions are ignored by comparison.

Checkpoint rule: choose the highest `command_id` C such that `C % CHECKPOINT_INTERVAL_COMMANDS == 0` (or the latest command id at the 2 s boundary) and every client has applied ≤ C; the detector compares rows at the same C. `sim_tick` is advisory; a mismatch in `sim_tick` for equal C is ignored.

### 6.6 Chat, UI-event, and plan formats

- `ChatKind`: `All`, `Team(u8)`, `System`. `MAX_TEXT = 150` (upstream `maxTextLength`); newlines stripped; `chat_filter` applied before insert; `muted_until` blocks with a silent drop + `infractions` increment; 3 infractions in one window → automatic `admin_kick` (upstream `messageSpamKick`).
- `UiEventKind`: `MenuBuilderShow`, `MenuBuilderUpdate`, `MenuBuilderHide`, `HudText`, `HideHudText`, `Announce`, `InfoPopup`, `Label`, `InfoToast`, `WarningToast`, `OpenUri`, `CopyToClipboard`, `PingMarker`; `payload` = 14's encoding (menus = `ui_node` bytes with `format: 1`).
- Plans: chunks carry 04's `TypeIO.writePlans` bytes; `plan_count` on `MatchPlanState`; active group = highest fully-uploaded `group_id` (monotonic by sender); incomplete groups are ignored and swept after `PLAN_GROUP_TIMEOUT = 30 s`.

### 6.7 Join handshake sequence

```
client                                    STDB module                              other peers
------                                    -----------                              -----------
1. subscribe Base+Lobby waves  ────────►  (views pushed)
2. read all_matches / my_matches
3. join_match(match_id, password,        validate: status/ban/whitelist/cap/
   build_id, content_hash, mods)   ────►  protocol/password/mod set
                                          insert relay_member, player_count++
                                    ◄──── view push: my_match, my_match_members
4. subscribe Game wave           ────────► (my_match_commands replay from command 1)
5. if Running: read my_match_snapshots, subscribe Snapshot wave, download chunks,
   generate terrain (map_seed), restore snapshot, replay tail
6. set_ready(true)               ────────► relay_member.ready = true
                                          host sees; when all ready → start_match
7. observe Running + match_state ───────► host emits SetRules/WorldReset snapshot
8. emit PlayerSpawn (self)  ────────────► ordered command  ──────────────────────► all peers spawn
                                                                                    the player unit
9. report_player_state @15 Hz   ────────► LWW row          ──────────────────────► puppet updates
```

### 6.8 Seeds, retention, and sweeps

Seeds (`main/seeds.rs`, run on every publish — dev wipes are expected):
- `seed_relay_config` (01) + `seed_server_config` (singleton; defaults from `main/global.rs`: `whitelist_enabled=false`, `auto_pause=false`, intervals above, retention below).
- `seed_admin_identities`: `ADMIN_IDENTITIES: &[&str]` const in `main/global.rs` (empty by default); `server/build.sh --admin <hex>` publishes with the dev identity injected (plan 22 exposes the same flag). **NEEDS USER DECISION** (§8 OD-21-D): seeded const vs first-claim; default seeded const, no self-claim reducer.
- `seed_content_catalog`: generated at build time from the client's vanilla content manifest (`server/build.sh` step writes `server/spacetimedb/src/main/content_seed.rs`; format `(name, content_type, source_mod="")`). If generation is unavailable (20 not landed), the seed is empty and existence validation degrades to charset/length — logged at publish.
- `seed_chat_filters`: empty; admins add.

Retention/sweep (`tick_maintenance`, 60 s interval):
- `match_command`: delete rows with `command_id < relay_match.last_snapshot_id`'s watermark minus `server_config.command_retention_commands` (default 4096); keeps late join + a tail-replay window. Never prune past a watermark that a snapshot row does not yet exist for.
- `match_checksum`: keep the newest row per `(sender, command_id)` for 24 h and at most 2000 rows/match (oldest first).
- `match_chat`: keep the newest `server_config.chat_retention_rows` (default 500) per match.
- `match_ui_event`: delete older than 5 min.
- `match_plan_chunk`: delete groups older than `PLAN_GROUP_TIMEOUT`; `match_plan_state` pruned with the member.
- `match_snapshot`/`match_snapshot_chunk`: keep `WorldReset` + newest `Dynamic` + newest `Digest`; delete older rows and their chunks.
- `relay_member`: delete rows with `connected=false && now-last_seen_at > MEMBER_GRACE_SECS`; `player_count` recomputed in the same pass.
- `relay_match`: `Ended` with `closed_at > server_config.match_ttl_hours` (24 h) → delete match + all child rows.
- `player_session` (01): delete older than 30 d.
- `player_ban`: expired rows deleted.
- `chat_rate`/`admin_rate`/`sender_command_state`: idle > 30 d rows deleted.

### 6.9 Versioning and IDs

- `PROTOCOL_VERSION` (01) covers the whole envelope + schema generation; bump on any command/schema change. Generated bindings are the compile-time mirror.
- `RelayMatch.build_id`/`content_hash` + `protocol_info` (`01`) gate join. `match_mod` gates mod sets.
- `map_hash` = FNV-1a-64 over (map id, generator version, seed, generator type, content manifest hash) computed by 06; a mismatch on late join aborts with a clear UI error.
- `content_hash` = FNV-1a-64 over the sorted `(content name, content type)` pairs of loaded content (vanilla + mods); computed by 20/02 at load.
- IDs: `command_id` auto-inc (gaps normal); `sender_seq` per sender from 1; `snapshot_id`, `checksum_id`, `chat_id`, etc. auto-inc and never used for ordering across senders; `SnapshotKind`/`ChecksumScope`/`ChatKind`/`UiEventKind`/`AdminActionKind`/`Visibility`/`MemberRole`/`Gamemode` are `SpacetimeType` enums whose variant names are ABI (append-only).
- Schema changes are hard cuts (plan 01 §6.7): no migrations, dev wipes expected; production publishes rotate to a new DB name if data must survive (22 owns deployment).

---

## 7. Oracle & verification (REQUIRED)

### 7a. Ported tests (Mindustry → `cargo test`)

Upstream has **no net test class**; `tests/src/test/java/ApplicationTests.java` contains only packet/IO-adjacent cases and no `NetServer` behavior tests (grep: no `Net`/`Packet`/`admin` test). The port therefore records the upstream anchors it reuses and defines behavior tests derived from `NetServer` guards.

| Upstream anchor | Rust test | Notes |
|---|---|---|
| `ApplicationTests.writeStringTest` | `mind_stb::relay::tests::command_string_caps` | All string fields (block/rule/content/channel/chat) survive round-trip and enforce caps. |
| `ApplicationTests.writeRules` / `writeRules2` | `mind_stb::relay::tests::rules_blob_roundtrip_and_cap` (with 12/04) | `SetRules` blob ≤ 100 000, epoch check. |
| `ApplicationTests.saveLoad` (entity half) | `mind_stb::snapshot::tests::dynamic_snapshot_roundtrip` | Blob → restore → checksum equality (uses 04/05 fixtures). |
| `Packets.ConnectPacket` fields (derived) | `mind_stb::session::tests::join_args_roundtrip` | No uuid/usid base64; identity + build/content/mod set. |
| `NetServer` connect guards (derived) | server `#[cfg(test)]` `relay::reducers::tests::{join_rejects_ban, join_rejects_whitelist, join_rejects_version, join_rejects_mod_sets, join_rejects_cap, join_rejects_password}` | `Assert` on `Err` strings; `cargo check --tests` typechecks, runtime in the `MIND_STDB_IT=1` ignored set. |
| `NetServer.clientSnapshot` clamps (derived) | `relay::methods::tests::{finite_float_guard, clock_skew_window, seq_must_increase, rate_classes}` | Cheap-validation pure helpers. |
| `NetServer.adminRequest` (derived) | `admin::tests::{admin_gate_denies_non_admin, host_can_kick, ban_writes_row_and_blocks_join, trace_requires_admin, switch_team_emits_command}` | |
| `NetServer.sendChatMessage` (derived) | `chat::tests::{rate_limit_window, length_cap, filter_mutes, team_requires_team, system_host_only}` | |
| `Call.setRules` (derived) | `relay::reducers::tests::{set_rules_host_only, set_rules_epoch_mismatch, set_rules_updates_match_state}` | |
| `ClientSnapshot`/plan caps (15) | `plans::tests::{chunk_reassembly_75, incomplete_group_ignored, plan_cap_1000, blob_cap_8k}` | |

Client-side unit tests (`mind-stdb`, network-free):

- `relay::tests::{orders_by_command_id_not_arrival, duplicate_command_id_is_ignored, per_sender_gap_is_flagged, autoincrement_numeric_gaps_are_not_loss}` (01; still green after the cutover).
- `commands::tests::command_kind_matrix` — table-driven over **every** `CommandKind` variant: variant → expected `SimCommand` (or explicit no-sim), caps row, role, `PredictPolicy`; fails on any unlisted variant (invariant §3.17-6).
- `commands::tests::{preflight_bounds, preflight_rates, preflight_host_gate, content_name_charset, blob_caps}`.
- `relay::tests::{echo_skip_by_sender_seq, foreign_rows_apply_in_order, prediction_rejected_after_timeout, command_ring_buffer_tail}`.
- `checksum::tests::{compare_at_command_id, host_canonical_correction, version_mismatch_ignored, scope_bits}`.
- `session::tests::{join_states, ready_gating, host_leave_ends_match, spectator_cannot_emit}`.
- `snapshot::tests::{chunk_reassembly, header_validation_rejects_mismatch, tail_replay_window, snapshot_keep_three}`.
- `transport::tests::stdb_transport_contract`.
- `admin::tests::console_command_mapping` (table-driven mapping of §4's console table).

Integration tests (`#[ignore]`, `MIND_STDB_IT=1`, DB `mindustry-it`): `it::two_clients_create_join_place_break`, `it::player_state_roundtrip`, `it::checksum_mismatch_triggers_resync`, `it::late_join_snapshot_tail`, `it::admin_kick_ban_whitelist`, `it::reconnect_resumes_match`, `it::mod_mismatch_rejected`. Benches `bench::bench_relay_roundtrip`, `bench::bench_input_state`, `bench::bench_snapshot_push`.

### 7b. Headless harness scenarios (`mind-headless`)

All are network-free except `mp_relay_live` (opt-in) and run under `cargo run -p mind-headless -- run <scenario>`; fixtures under `mind-core/tests/fixtures/relay/` and goldens under `tests/golden/mp/`.

| Scenario | Network | What it proves | Expected assertions |
|---|---|---|---|
| `mp_lobby_join` | none (synthetic row source) | Lifecycle reducer logic in isolation (`session` + views mirror) | host/member rows correct; ready gating; `player_count`; spectator rejected for sim commands |
| `mp_command_log_replay` | none | Two `Sim` instances fed the same generated `CommandKind` log through `RelayRuntime` (no Godot, no STDB) | identical `checksum_scoped` at every 60 ticks; echo skip produces one apply; goldens `tests/golden/mp/mp_command_log_replay.checksums` |
| `mp_validation_matrix` | none | Shared `tests/fixtures/relay/validation_matrix.json` (≥ 40 cases: accept/reject + reason) driven through `preflight_validate` and the module-side mirror | every row accepted/rejected as listed; reason strings stable |
| `mp_desync_injection` | none | Two Sims; at tick 600 a divergent `PlaceBlock` is applied to B only | checksums diverge at the next checkpoint; detector fires; host snapshot restore + tail replay makes A==B at C+window; `resync_count == 1` |
| `mp_late_join` | none | Sim A runs 3600 ticks + 300 commands; dynamic snapshot at 3600; Sim B regenerates terrain (06 seed), restores snapshot, replays tail | B checksum == A checksum; snapshot ≤ 4 MB; also the 1800-snapshot + 1800-tail variant |
| `mp_reconnect_gap` | none | Command stream with a pruned hole | gap flagged; snapshot path resolves; final equality |
| `mp_plan_snapshot_reassembly` | none | Chunked plan rows (75/chunk, 1000 total) | reassembled plan vector equals the source; incomplete group ignored |
| `mp_snapshot_bytes` | none | Serialize a mid-game state (600 buildings/300 units/4000 items) | p95 ≤ 512 KB, cap 4 MB (drives §7d) |
| `mp_relay_live` (opt-in) | local STDB | Real create/join/commands/checksums across two headless clients (no Godot) | applied counts match; zero order errors; checksum agreement |

### 7c. MCP playtest scenarios (concrete; open-godot-mcp)

Preconditions for all: `spacetime start`; `server/build.sh` published; the editor/game launched per the repo playtest skill (plan 00's analog; if `godot_health check` reports `BRIDGE_NOT_CONNECTED`, launch `nohup godot4 --editor --path /home/c/g/code_examples/mindustry-godot/client >/tmp/mind-editor.log 2>&1 &` in the background, wait ~20 s, `godot_instance list`). Two instances run via the project's per-instance launch args (`--p1`/`--p2`); **all runtime calls are sequential per instance** and **pid-stamped** (`"pid": OS.get_process_id()`, compared against `godot_game instances`). New pids ⇒ re-establish state. Every scenario ends with `godot_log errors` (clean) and a recorded screenshot path.

**`mp_two_client_build_visible`** (the primary scenario):
1. `godot_health check`; `godot_game instances` → record pid1/pid2; `godot_game play` (spine scene) with both instances.
2. Instance 1 eval `{"code":"var n=get_node(\"/root/MindNet\"); return {\"pid\":OS.get_process_id(),\"state\":n.session_state()}"}` → `state == "offline"|"connected"` before host, then `n.create_match("demo_flat", 1234, "survival", "public", 8, "{}")` → capture `match_id`; `spacetime sql mindustry "SELECT match_id, status, host FROM relay_match"` shows the row with `status=Lobby`.
3. Instance 2 eval join: `get_node("/root/MindNet").join_match(<match_id>, "")` → `true`; `spacetime sql "SELECT identity, role, ready FROM relay_member"` shows two rows.
4. Instance 1 `start_match(true)`; both instances `set_ready(true)` + `PlayerSpawn` happens on `Running`; assert via eval `get_state_json()` contains a player unit for each.
5. Instance 1 eval `MindSimHost.place_block` on (3,5) `"stone-wall"` (routes through `MindNet`/`RelayRuntime`); poll instance 2 `get_state_json()` for tile (3,5) within 2 s; assert `spacetime sql "SELECT sender_seq, kind FROM match_command WHERE match_id=<id>"` has the row.
6. Negative: instance 1 uses `send_command_json` with an out-of-bounds place (e.g. x = 9999) → `false` (preflight) and `spacetime logs` shows the rejection; no tile on either instance. Then force a server-side rejection with a stale `sender_seq` via a debug eval and assert the local prediction is corrected within one resync (checksum convergence).
7. `godot_screenshot` both; record paths + `godot_game stop`.

**`mp_movement_sync`**: after step 4 above, instance 2 holds a movement key (`godot_input` press/hold on the W/A/S/D binding) while instance 1 polls `MindNet.get_remote_player_state("<identity2>")` and the remote player node position; then swap. Assert monotonic position change on the observer and no local self-rubber-band; screenshot both; `godot_runtime_state watch` the remote node for 2 s.

**`mp_chat_admin_kick`**: instance 2 `send_chat("hello")`; instance 1 eval `get_state_json()`/`my_match_chat` mirror shows it within 1 s. Then instance 1 calls `spacetime call mindustry admin_kick '<identity2_hex>' "test"`; instance 2's `my_kick` view triggers the disconnect UI (eval `get_node("/root/MindUI").has_dialog()` / session state); `spacetime sql "SELECT * FROM admin_action_log"` shows the action. Rejoin attempt after `admin_ban` fails with the ban reason.

**`mp_late_join_snapshot`**: host + instance 2 build for ~60 s; instance 2 leaves; a third run (`--p3` if configured, else re-join same instance) joins while Running; assert the joiner reaches `InGame` and its `checksum_report()` matches the host's at the next checkpoint; record `snapshot_progress` and `spacetime sql` chunk counts.

**`mp_desync_recovery`** (debug builds only): `godot_exec` calls `MindNet.dev_inject_divergence()` on instance 2; assert the inspector `net` page shows `desync` then `resync_count=1`; final `checksum_report()` values equal; toast visible in the screenshot.

### 7d. Performance budgets + measurement

Baseline: 60 tps ⇒ 16.6 ms/frame; network must stay under ~2% of a frame per client. Measurement: `mind-headless bench mp_*` (reports p50/p95/p99) and the `MIND_STDB_IT=1` ignored benches; numbers recorded in the plan Changelog and gated by plan 23.

| Metric | Budget | Measurement |
|---|---|---|
| Command round-trip (emit → remote apply) | p95 ≤ 100 ms local; ≤ 250 ms remote | `bench_relay_roundtrip` (10 clients, 60 s) |
| Relay throughput | ≥ 300 committed commands/s for 60 s, 10 clients, 0 gap/dup; p99 apply ≤ 400 ms | `bench_relay_throughput` (extends 01) |
| Input/player-state reducer | ≥ 30 updates/s/player, p95 ≤ 150 ms; row ≤ 256 B | `bench_input_state` |
| `RelayRuntime` apply overhead | p99 ≤ 0.5 ms per command; queue drain ≤ 1 ms/frame at 100 rows | `bench mp_apply` + `TickReport` span |
| Checksum publish/compare | ≤ 50 µs one-shot; cadence ≤ 0.5% of tick time | `bench mp_checksum` |
| Dynamic snapshot | p95 ≤ 512 KB compressed, cap 4 MB; publish compress ≤ 400 ms host; late join ≤ 2 s local / ≤ 8 s remote | `mp_snapshot_bytes`, `mp_late_join` |
| Snapshot chunk subscription | ≤ 1 MB/s per downloading client, ≤ 2 MB/s total publisher | `bench_snapshot_push` |
| Game wave initial apply (16 players) | ≤ 100 ms; ≤ 1500 rows/s steady p95 | `mp_relay_live` instrumentation |
| Client relay memory | ≤ 8 MB buffers (command ring 4096 × ≤2 KB + snapshot staging) | alloc-audit + `relay_queue_depth()` |
| Command row size | chat ≤ 512 B; player state ≤ 256 B; sim command ≤ 2 KB p95 | `spacetime sql` sampling + `command_kind_matrix` caps |

### 7e. Exit criteria checklist

- [ ] `server/build.sh --check` (no drift) + `cargo check --manifest-path server/spacetimedb/Cargo.toml --tests` green.
- [ ] `cargo test -p mind-stdb` green network-free; 01's tests still pass after the schema cutover.
- [ ] All §6.1 tables/indexes exist (verified by `spacetime describe` + a schema-parity test).
- [ ] §7b scenarios pass; goldens committed under `tests/golden/mp/`.
- [ ] §7c scenarios executed with pid-stamped evidence, screenshots and SQL outputs recorded.
- [ ] §7d budgets met; regressions filed.
- [ ] 30-minute soak: 2 clients, 200 commands/s, 1 forced desync → final checksums equal, `resync_count == 1`, no queue growth.
- [ ] Ban/whitelist/kick/trace/wave/team/chat-filter all reachable from console and UI (14) and leave `admin_action_log` rows.
- [ ] Late join ≤ 8 s remote with the 512 KB p95 snapshot; reconnect resumes within the same budget.
- [ ] Offline single-player boot passes with the server stopped and `StdbMode::Offline` (invariant 1).
- [ ] `MindNet` API doc + `server/spacetimedb/AGENTS.md` delta section written; reconciliation notes to 20/22/23 sent.
- [ ] Every new file carries the GPL header (HLP §6.4).

---

## 8. Risks & open decisions

| # | Risk / decision | Default taken | Status |
|---|---|---|---|
| OD-21-A | **Player-possessed unit authority split under D2**: client-owned kinematics/health + masked checksum cohort (chosen) vs strict input-delay lockstep (exact checksums, stalls on slow peers, fights predicted movement). Load-bearing for what “desync” means. | Client-owned + `checksum_scoped` mask; authoritative sim removes the mask (§3.2/§6.5). | **locked 2026-10-01 (NUD-45=A)** |
| OD-21-B | **Snapshot authority**: host client publishes snapshots/digests; host leaving ends the match (no migration). Dedicated server recommended for persistence. | Host = `created_by`; no migration (upstream parity). | **locked 2026-10-01 (NUD-46=A)** |
| OD-21-C | **Ban key**: STDB identity only; no IP/subnet bans (no data source). | Identity + optional `banned_name_patterns`; subnet-ban console command rejects with a message. | **locked 2026-10-01 (NUD-47=A)** |
| OD-21-D | **Admin bootstrap**: seeded `ADMIN_IDENTITIES` const (+ `build.sh --admin`) vs first-claim reducer. | Seeded const, empty by default. | **locked 2026-10-01 (NUD-48=A)** |
| OD-21-E | **Transport (HLP OD3)**: STDB-only until the §3.13 thresholds trip; direct ENet/UDP only as a data plane with STDB authority. | STDB-only; thresholds recorded; user sign-off at execution if tripped. | **locked 2026-10-01 (NUD-49=A / NUD-03=A)** |
| OD-21-F | **Mod content catalog**: generated seed from the build manifest vs shape-only validation when `content_catalog` is empty. | Generate at `build.sh`; degrade to shape-only with a publish warning if unavailable. | Reconcile with 20/03 |
| OD-21-G | **Rules wire shape**: `rules_json: String` here vs 12 §3.9 `rules_blob` bytes. | String (≤100 000) for cheap validation; reconcile at M6. | Reconcile with 12 |
| OD-21-H | **Private matches**: `visibility: Unlisted` + optional salted password hash. | Both; no password UI beyond 14's JoinDialog field. | Default; note for 14 |
| R-21-1 | Rust SDK API uncertainty (bindings, per-view subscribe-on-demand for the Snapshot wave, reducer argument structs). | Plan 01 R1 isolation; M0 spike exercises a 16 KiB `Vec<u8>` row + on-demand view. | Monitor |
| R-21-2 | STDB row/subscription limits for large blobs (`rules_json` 100 KB, 16 KiB chunks). | M0 timeboxed spike; fall back to 8 KiB chunks / rules split if needed. | Monitor |
| R-21-3 | Command-log growth/profiling: late join within the 4096-command tail window + 60 s snapshot cadence assumes ≤ ~68 committed commands/s average (4096/60). Build spam (200/s) makes the host re-snapshot on demand instead of tail-replaying. | Retention + snapshot watermark; bench verifies. | Monitor |
| R-21-4 | Checksum comparison cost with 16 players × 2 s cadence is trivial, but `checksum_scoped` touching every entity may be measurable mid-game. | Reuse 05's hasher; mask only possessed units; budget §7d. | Monitor |
| R-21-5 | Host-client authority without anti-cheat: hidden information (fog) and client-owned health are trust-based. | Accepted under D2 (documented); authoritative sim is the fix. | Accepted |
| R-21-6 | `20` not on disk: mod manifest generation interface may change. | Freeze `match_mod`/`content_hash` shapes now; orchestrator reconciles 20's manifest output. | Reconcile with 20 |
| R-21-7 | `22` not on disk: dedicated-server binary name/entry, console wiring, service tokens. | Contract fixed in §3.11/§3.13; `MindNet` APIs are the seam. | Reconcile with 22 |
| R-21-8 | `23` not on disk: scenario/golden registration format. | Names + paths reserved; orchestrator adds to 23's suite. | Reconcile with 23 |
| R-21-9 | View-based admin gating (per-caller view bodies checking `admin_identity`) adds a DB lookup per subscribe; `admin_trace` turned out not to be expressible as a parameterized view. | Admin lists are views; trace data comes from `all_players`/`player_session` filtered client-side until 14 needs a dedicated admin view. | Monitor |
| R-21-10 | Chat filters are global (not per-match) and substring-only (no regex in reducers). | Default empty + admin-managed; per-match filters deferred until the authoritative sim owns moderation. | Accepted |
| R-21-11 | Player-state LWW at 15 Hz × 16 players = 240 updates/s of reducer traffic; may interact with the 1500 rows/s budget. | Budgeted in §7d; reduce to 10 Hz or delta rows only if measured over. | Monitor |
| R-21-12 | Host clock skew: `client_tick` windows and `sent_at` are server timestamps; host is a client. | `client_tick` diagnostics only; no ordering/decision depends on wall clock. | Accepted |

---

## 9. References

Read in full or by named section for this plan (2026-10-01):

- `mindustry-godot/HIGH_LEVEL_PLAN.md` (§0 locked decisions D1–D9 — D2 is the multiplayer contract; §2 architecture; §3 plan set row 21; §4 template; §5 P8 gate; §6 conventions; §7 verification; §9 parity ledger; §10 OD3).
- `mindustry-godot/PRELIMINARY_PLAN.md` (historical intent).
- `mindustry-godot/01_PLATFORM_STDB_IMPLEMENTATION_PLAN.md` (§3.3 API, §3.4 frame order, §3.6–3.10 waves/relay/validation/ordering — extended, not redefined; §3.11 gdext; §7.4 budgets; §8 R3–R14).
- `mindustry-godot/05_SIM_CORE_IMPLEMENTATION_PLAN.md` (§3.2 `Sim`, §3.4 schedule, §3.5 metadata, §6.4 `SimCommand`, §6.5 checksum, §7).
- `mindustry-godot/06_WORLD_TERRAIN_IMPLEMENTATION_PLAN.md` (§2.4 deferred STDB/world streaming, §3.4 tile ops, §3.12 STDB touchpoints, §6.5 IDs).
- `mindustry-godot/10_COMBAT_BULLETS_IMPLEMENTATION_PLAN.md` (§3.5 `createNet`, §3.13 STDB handshake, §6.4 wire/checksum shapes).
- `mindustry-godot/11_UNITS_AI_WAVES_IMPLEMENTATION_PLAN.md` (§2.3, §3.13, §6.3 controller codec, §6.5 `SimCommand` unit variants).
- `mindustry-godot/12_CAMPAIGN_IMPLEMENTATION_PLAN.md` (§3.9 relay shapes, §3.10 invariants, §4.2 reconciliation, §6.7 STDB-shared shapes).
- `mindustry-godot/13_LOGIC_MLOG_IMPLEMENTATION_PLAN.md` (§3.13 STDB surfaces, §3.14 ledger).
- `mindustry-godot/14_UI_IMPLEMENTATION_PLAN.md` (§3.9 Godot/STDB, §6.1 `ui_node` encoding, §6.3 relay payloads).
- `mindustry-godot/15_INPUT_RTS_IMPLEMENTATION_PLAN.md` (§3.3 input→command pipeline, §6.4 `RemoteAction`/`ActionBatch`, §6.5 plan snapshot).
- `mindustry-godot/16_RENDER_WORLD_IMPLEMENTATION_PLAN.md` (§3.12/§3.13 adapters), `17_FX_PARTS_IMPLEMENTATION_PLAN.md` (§2.5 headless/determinism, §3.16 ledger).
- `Mindustry/AGENTS.md`, `Mindustry/core/AGENTS.md`, `Mindustry/core/src/mindustry/AGENTS.md`.
- `Mindustry/core/src/mindustry/net/AGENTS.md`, `core/src/mindustry/core/AGENTS.md`, `core/src/mindustry/entities/AGENTS.md`, `Mindustry/server/AGENTS.md`.
- `Mindustry/core/src/mindustry/core/NetClient.java`, `NetServer.java`.
- `Mindustry/core/src/mindustry/net/{Net,Packet,Packets,NetworkIO,Streamable,NetConnection,Administration,Host,WorldReloader,ValidateException}.java` (skimmed `ArcNetProvider.java`, `CrashHandler.java` as out-of-scope).
- `Mindustry/tests/src/test/java/ApplicationTests.java` (net/IO-adjacent anchors only; no net test class exists).
- `/mnt/c/Users/Clinton/g/main/AGENTS.md`, `/mnt/c/Users/Clinton/g/main/server/AGENTS.md` (STDB rules, views, indexes, scheduled reducers, rate windows).
- `/mnt/c/Users/Clinton/g/main/client/sstdbsdk/{DatabaseConnector.cs,TableSubscriber.cs,TableBinderComponent.cs,AGENTS.md}` (skimmed; plan 01 owns the port).
- `/mnt/c/Users/Clinton/g/main/server/spacetimedb/src/player/{tables,views}.rs`, `src/main/{lifecycle,global,seeds}.rs`, `src/chat/mod.rs` (2.x patterns: composite indexes, sentinel views, AOI OR-chains, per-caller views, retention sweep).
- `C:\Users\Clinton\g\.opencode\skills\playtest\SKILL.md` (multi-instance rules: `--p1`/`--p2`, sequential calls, pid-stamp, connection-lost handling, `spacetime` CLI).

## Changelog

- 2026-10-01 — Draft v1 written. No implementation started. Open items before milestone gates: OD-21-A (M4), OD-21-B/C/D (M1/M6), OD-21-E (execution), OD-21-F/G (20/12 reconciliation). Orchestrator: regenerate bindings at M0, hand §7b/§7d names to plan 23, and reconcile the dedicated-server contract with plan 22.

- **2026-10-03 — M0–M2 implemented (`lane/f20-21`, base `main` @ `43d2c31`; commits `24b4cd0` M0, `fa1fc5f` M1 client, `f61a8b9` M2 headless, `82191d0` M1/M2 gdext).** M0: server schema cutover + full `CommandKind` variant set + `admin/`/`mods/` tables + `main/global.rs` constants; `PROTOCOL_VERSION=2`; bindings regenerated (`server/build.sh --check` drift-clean). M1: lifecycle reducers/views/seeds, ban/whitelist/password/cap/mod gates, `mind-stdb::MatchSession`, `MindNet` node (scene wiring deferred to orchestrator), `MindSimHost` pending-command drain. M2: `CommandSender`/preflight/`PredictPolicy`, `RelayTransport` seam, `RelayRuntime` echo-skip mapping, headless `mp_lobby_join`/`mp_command_log_replay`/`mp_validation_matrix` (shared fixture, 68 cases). **Verify:** `cargo test -p mind-stdb` 39 passed/3 ignored; `cargo test -p mind-core` 1497 passed/3 ignored (lib 1484); fmt + workspace clippy `-D warnings` clean; `server/build.sh --check` clean; `cargo check --manifest-path server/spacetimedb/Cargo.toml --tests` clean; `cargo check -p mind-gdext` clean. **Open:** no live STDB, so the M0 large-row spike and §7c MCP two-instance runs are unexecuted follow-ups; `Sim::checksum_scoped` mask needs plan 05; `mp_*` scenario-catalog registration deferred to plan 23; M3–M8 remain.
