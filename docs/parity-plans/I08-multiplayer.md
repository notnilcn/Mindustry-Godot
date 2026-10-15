<!-- SPDX-License-Identifier: GPL-3.0-only -->
# I08 — Multiplayer: host, join, late join, world sync, chat, player list, admin, dedicated server

Round I, written 2026-10-15. Read `docs/I-environment.md` fully, then
`docs/I-INDEX.md` §3, then this plan. Start this plan only after I06 T1 is on `main`
(T4 and T5 need `SimHost.load_map_world` and the non-campaign session start).

Ledger items: PAR-0120 (world actions do not synchronize), PAR-0121 (chat inert),
PAR-0122 (player list empty; kick/ban/trace/team dead; Admins/Bans always "none"),
PAR-0123 (late join does not sync the world), PAR-0124 (dedicated `host` never creates a
networked match), PAR-0125 (chat filters database-global), PAR-0136 (Join Game forgets
added servers; Remote list empty), PAR-0151 (host dialog layout). Java captures of the
single-client halves are under `runs/20261010-234250-ref-PAR-0070-combat-fx/java/`
(`step-375-host.png`, `step-381-join.png`) — see the root `AGENTS.md` table.

---

## 0. Architecture (binding; do not redesign)

Java runs a host-authoritative TCP/UDP server (`J/core/NetServer.java`, `NetClient.java`,
`ArcNetProvider.java`): the host simulates, clients send input calls (`@Remote`, 188 of them)
and receive entity snapshots. The port replaced the transport with **SpacetimeDB in relay
mode** (`server/spacetimedb`, crate `mindustry_godot`, SpacetimeDB 2.10.1): the module is an
ordered per-match command log plus lobby/chat/admin tables. It runs NO simulation. Every
client runs the full sim. Player actions become `CommandKind` rows, every client applies them
in `command_id` order at fixed-tick boundaries, checksums are published every 120 ticks, and a
mismatch triggers a snapshot resync from the host. `MindNet.set_authority_mode` accepts only
`"relay"`. This plan COMPLETES that design; it does not add a Java-style socket server.

"1:1" here means: the same dialogs, flows, messages and in-game results as Java, as seen by
players, over the relay transport.

## 1. State on `main` (verified 2026-10-15 @ `b560e9b1`)

Server module (`server/spacetimedb/src/`, ~11k lines, unit-tested):
* Relay reducers (`relay/reducers.rs`): `create_match` :29 (map_id, map_seed, mode, mode_name,
  visibility, password?, max_players, rules_json, build_id, content_hash, mods, campaign_id?,
  sector_planet?, sector_id?), `join_match` :138 (match_id, password?, build_id, content_hash,
  role, mods; gates: ban, whitelist, cap, password, build/content/mods), `leave_match` :234 (host
  exit ends the match), `set_ready` :285, `start_match` :304, `publish_match_state` :339,
  `send_match_command` :396 (match_id, client_tick, kind).
* `relay/tables.rs` `CommandKind` (:386-457): Noop, Ping, PlaceBlock, BreakBlock, ConfigBlock,
  Rotate, DeletePlans, CommandBuilding, Inventory, Payload, UnitControl, UnitClear,
  BuildingControlSelect, UnitCommand, UnitCommandQueue, UnitStance, PlayerSpawn, Bullet,
  SetRules, SetRule, ResearchUnlock, CompleteObjective, ClearObjectives, SectorCapture,
  SaveSector, SkipWave, RunWave, AdminSwitchTeam, AdminTileOp, LogicSync, LogicClientData,
  MenuChoose, MenuBuilderChoose, TextInputResult, Custom.
* Snapshots (`relay/snapshot.rs`): `request_snapshot` :118, `publish_snapshot` :147 (chunked blob).
  Checksums (`relay/checksum.rs`): `publish_checksum` :62. Player state
  (`relay/player_state.rs`): `report_player_state` :162. Plans (`relay/plans.rs`). UI events
  (`relay/ui_events.rs`).
* Chat (`chat/mod.rs`): `send_chat` :85 (All|Team|System; sanitise, filter, rate limit),
  table `chat_filter` :67 (GLOBAL: PAR-0125).
* Admin (`admin/mod.rs`): `admin_kick` :354, `admin_ban` :402, `admin_unban` :455,
  `admin_set_whitelist` :480, `admin_whitelist` :500, `admin_grant` :525, `admin_set_config` :552,
  `admin_add_chat_filter` :635, `admin_remove_chat_filter` :665, `admin_skip_wave` :684,
  `admin_run_wave` :713, `admin_switch_team` :751, `admin_tile_op` :796; views `am_i_admin` :873,
  `all_bans` :882, `all_whitelist` :895, `all_admins` :908, `all_chat_filters` :921.
* Identity (`identity/reducers.rs`): `set_username` :24, `create_profile` :44,
  `update_client_settings` :68; views `local_player`, `all_players`, ...
* Relay views (`relay/views.rs`): `all_matches` :30, `my_match` :53, `my_match_members` :65,
  `my_match_state` :82, `my_kick` :100, `my_match_commands` :117, `my_match_player_states` :146,
  `my_match_chat` :199, `my_match_checksums` :216, `my_match_snapshots` :229,
  `my_match_snapshot_chunks` :242, `my_match_snapshot_requests` :262.

Client crate `client/rust/mind-stdb/` (Godot-free): `connector.rs` (one wrapper per reducer,
admin wrappers :824-957), `session.rs` `MatchSession` (`host` :205, `observe_match` :291),
`commands.rs` (`PredictPolicy`, `CommandSender::send` stamps `client_tick = sim_tick + INPUT_DELAY_TICKS (6)`,
prediction queue with echo skip), `relay.rs` `CommandStream` (applies rows in `command_id` order,
gap/regression errors), `snapshot.rs` (header encode/decode, `split_chunks`, `reassemble`,
`SnapshotProgress` — used only by `mind-headless/src/mp_scenarios.rs`), `checksum.rs`,
`waves.rs:12-47` (subscription waves; NO admin views), `config.rs` (`StdbMode` Offline/Online),
`protocol.rs` (`PROTOCOL_VERSION = 2`), `tests/it.rs` (`MIND_STDB_IT=1` integration tests).

Godot side:
* `MindNet` (`gdext/net/mod.rs`, node `/root/Spine/MindNet`, `scenes/game.tscn:46`): OFFLINE by
  default (:100); online only with `--db`/`--pN` on the command line (:174-215). `#[func]`s:
  `create_match` :243, `connect_to(address)` :282, `get_public_matches_json` :327, `join_match` :338,
  `leave_match` :371, `start_match` :380, `set_ready` :389, `session_state` :398,
  `get_match_state_json` :404, `get_members_json` :414, `last_applied_command_id` :420,
  `relay_queue_depth` :429, `relay_order_error` :435, `send_command_json` :445 (place/break only;
  NO caller), `checksum_report` :478, `authority` :519, `get_remote_player_state` :525,
  `get_remote_players_json` :551, `request_snapshot` :563, `snapshot_progress` :586 (never
  updated), `send_chat` :592 (always `All`; NO caller), `send_ui_result` :610,
  `set_authority_mode` :622. Signal `session_changed(state)` :238 is NEVER emitted. `_process`
  :131-159 drains `my_match`, members, state, `all_matches`, player states, checksums, snapshots,
  the relay, and publishes a checksum every 120 ticks. No binder for `my_match_chat`,
  `my_match_ui_events`, `my_kick`, `all_players` or the admin views (`attach_binders` :222-235).
* `gdext/net/relay.rs` `kind_to_sim` :21-39 maps ONLY `PlaceBlock`/`BreakBlock`, and
  `block_id(name)` maps `"router"` to 1 and EVERY other name to 0; team is always 0.
* Local player actions never reach `MindNet`: `gdext/input/desktop.rs:1631, :3272` and
  `input/mod.rs:1441` call `MindSimHost.enqueue_sim_command` directly.
* Host: `ui/dialogs/host_dialog.gd` `_host` :99-114 emits `host_requested(name, mode)` ->
  `ui/ui_root.gd` :375-377 / :407-418 `_on_host_requested` -> `net.create_match(map_id, 1, gamemode, "public", 8, rules)`
  (name and port ignored). Offline it fails silently (returns 0). No lobby UI; nothing calls
  `start_match`/`set_ready`; nothing publishes a snapshot.
* Join: `ui/dialogs/join_dialog.gd` `_submit_add` :301-307 emits `connect_requested(address)`
  -> `ui_root.gd` :391-395 `net.connect_to(address)`; a match row -> `join_requested(match_id)` ->
  `net.join_match(match_id, "")` (no password; `build_id` "" and `content_hash` 0). Nothing loads
  the host's world. Name/colour fields are unused. Added servers are not saved; the "remote"
  section is a spacer (:110-112).
* Chat: `ui/fragments/chat_fragment.gd` `_request_send` :72-78 -> `MindUi.chat_send(text, mode)`
  (`ui_host.rs:554-563`) which only emits `chat_message` (no listener). `add_message` :101 is never
  called. `/t` `/a` are text prefixes only; no `/votekick`.
* Player list: `MindUi.player_list_json()` returns `"[]"` (`ui_host.rs:580-582`);
  `player_action` logs and returns false. `admins_dialog.gd`, `bans_dialog.gd`,
  `trace_dialog.gd` read context keys nobody supplies.
* Dedicated server: `mind-headless server` / `serve` (`headless/server/`); `host`/`runwave` act on a
  Godot-free `LocalHost` fixture (`host.rs:351-355` just does `wave += 1`); `NetHost::host`/`run_wave`
  (`net_host.rs:278-335`) exist but nothing calls them, so the connector never opens.
  `--admin-token-file` is validated but unused (`dedicated.rs:167-171`).
* Tests: `mind-stdb` unit tests; `tests/it.rs` (ignored ITs); `headless/mp_scenarios.rs` defines 9
  scenarios that are not registered anywhere.
* Local server: `spacetime` CLI 2.10.1 is installed (`spacetime server list` has `local` =
  `127.0.0.1:3000`). It is not running now. `server/build.sh` publishes the module locally and
  regenerates the client bindings.

## 2. Tasks

Order: T1 -> T2 -> T3 -> T4 -> T5 -> T6 -> T7 -> T8 -> T9 -> T10 -> T11 -> T12 -> T13.
Every in-engine check from T4 on uses TWO clients: your session's game plus a second Godot
game. Use session numbers N and N+1 (e.g. 4 and 5) only when the coordinator has given you
both (a two-client plan needs two session slots: RAM rules, I-environment §1.1).

### T1 — Two-client harness and online-on-demand connection

**Acceptance.** `tools/mp-local.sh up` starts the local SpacetimeDB (if not running), publishes
the module (`server/build.sh`), and prints the address. In the client, `MindNet` goes online on
demand: Host/Join connect to the configured server (setting `stdb-server`, default
`127.0.0.1:3000/mindustry-godot`; check the db name `server/build.sh` publishes) without any
command-line flag. `session_changed(state)` is emitted on every state change. Connection
failures show Java's `@connectfail` / `@server.error` dialogs.

**Changes.**
1. `tools/mp-local.sh` (`up` | `down` | `status`): `spacetime start` in the background
   (log to `/tmp/opencode/stdb.log`), wait for `127.0.0.1:3000`, `bash server/build.sh`, print
   the db name. Document it in `tools/AGENTS.md`.
2. `gdext/net/mod.rs`: `#[func] ensure_online() -> bool` that, when offline, builds the Online
   connector from the `stdb-server` setting (or `--db` when given) the same way `connect_to` does,
   and opens the browser session. `create_match`/`join_match`/`connect_to` call it first.
   Emit `session_changed` from the one place the session state changes (wrap
   `self.session.state` writes in a setter).
3. Errors: when a reducer call or the connection fails, call
   `MindUi.show_error_key("@connectfail", <detail>)` (grep the MindUi error API; add one if
   needed) instead of only logging.
4. Two-client launch: document in this plan's summary the exact commands to run a second game
   process against the same server (`tools/session-play.sh M` for a second session M; the
   `--pN` identity suffix (`mind-stdb/src/identity.rs`) keeps the two identities distinct).

**Tests.** `mind-stdb` unit test for the setting parse (`host[:port][/db]`, reuse
`server_address_parsing` in `gdext/net/mod.rs:~945`).

**Verify.** `tools/mp-local.sh up`; `$G --eval 'return get_node("/root/Spine/MindNet").ensure_online()'` -> true;
`session_state()` -> `browsing`.

**Commit.** `Gameplay I08 T1: local multiplayer harness and online-on-demand connection`

---

### T2 — Player name and colour persist and reach the server

**Java.** `HostDialog` (`J/ui/dialogs/HostDialog.java:17-36`) and `JoinDialog` share: a `@name`
field (`settings "name"`, max 40) and a colour button (`PaletteDialog`, `settings "color-0"` as
RGBA int). `player.name/color` follow them.

**Changes.** `host_dialog.gd` and `join_dialog.gd` read/write `name` and `color-0` through
`MindUi.settings_get_text/put_text` (and an int variant). On change and on connect, call the
identity reducers (`Connector::set_username`, `create_profile`/`update_client_settings` for
the colour: check which reducer stores colour; add a `color` field to the profile reducer if
none does, regenerate bindings with `server/build.sh`). `@noname` when empty (Java
`HostDialog.java:56-59`).

**Verify.** Set the name in Join, restart, the field keeps it; `spacetime sql <db> "select * from player"`
shows the name.

**Commit.** `Gameplay I08 T2: player name and colour persist and sync to the server`

---

### T3 — Join dialog: saved servers and a working remote list (Fixes PAR-0136)

**Java** (`J/ui/dialogs/JoinDialog.java`, 833 lines): servers are kept in settings JSON
`"servers"` (`Core.settings.getJson("servers", ...)` :691, saved with `putJson` :774); the add
panel validates and adds; each server row shows name/ping/players/map/mode with refresh, edit
and remove buttons; "local" discovers LAN servers; "remote" fetches the community lists
(`Vars.serverJsonURLs`, `J/Vars.java:89`, `:703`).

**Port mapping.** A "server" is an STDB address (`host[:port][/db]`). "Remote" = the public
matches of every saved server plus the default server (`all_matches` view); "local" =
`127.0.0.1:3000`. The Java community URLs serve Java servers the port cannot talk to; do not
fetch them (record this as a deviation).

**Changes.** `join_dialog.gd`: persist the server list under `servers` (same JSON shape as Java:
`[{"ip": "...", "port": N}]`), rebuild the rows with Java's layout and buttons, ping = connection
round trip (`MindNet.ping_server(address)` new `#[func]` that opens a short-lived connector and
measures the subscription round trip), the match list per server from `get_public_matches_json`.
Search field and zoom button handlers per Java.

**Verify.** Add `127.0.0.1:3000`, restart, it is still listed (screenshot vs `step-381-join.png`);
with a hosted match (T4) the remote list shows it.

**Commit.** `Fixes PAR-0136: Join dialog persists servers and lists remote matches`

---

### T4 — Hosting publishes the current world (PAR-0120/0123 part 1)

**Java.** `HostDialog.runHost()` (`:80-99`): `net.host(port)`, `player.admin = true`,
`HostEvent`; the HostDialog is opened from the in-game pause menu (`PausedDialog`, `@hostserver`),
so the CURRENT world is hosted. Errors: `@server.error.addressinuse` / `@server.error`.

**Changes.**
1. `ui_root.gd` `_on_host_requested`: `MindNet.host_current_world(name)` (new `#[func]`) instead of
   the raw `create_match` call.
2. `MindNet.host_current_world`: `ensure_online`; `create_match` with the live map id/seed, the
   current rules JSON (`MindCampaign.get_rules_json()`), mode name, `"public"`, max players
   (setting, default Java's `playerlimit`), `build_id` = the port build string, `content_hash` =
   a hash of the content registry (`grep -rn "fn content_hash" client/rust/` — add one over
   the content name list if missing), mods = `MindMods.mod_strings()`; then
   `start_match(true)` immediately (Java has no lobby); then publish a snapshot (T5's encoder)
   so joiners can load; mark the local player admin.
3. The pause menu shows `@hostserver` (opens the host dialog) when offline and not in a campaign
   sector that forbids it (Java: `PausedDialog` shows "host" when `!net.active()`; read
   `J/ui/dialogs/PausedDialog.java` for the exact condition).

**Verify.** Client A in a custom game (I06 T1) -> Escape -> Host -> `session_state()` =
`in_game`, `spacetime sql <db> "select * from relay_match"` shows the match with status Running,
and a snapshot row exists.

**Commit.** `PAR-0120: hosting creates a running match from the current world and publishes a snapshot`

---

### T5 — Joining loads the host's world; late join catches up (Fixes PAR-0123)

**Java.** `NetClient` receives `WorldStream` (the map + state), then entity snapshots; the
loading fragment shows `@connecting` -> `@connecting.data` -> progress.

**Changes.**
1. Encoder (host side): `MindSimHost::encode_snapshot() -> Vec<u8>` = the save bytes of the live
   world (write through the existing save path to a temp file under `user://mp/` and read the
   bytes, or add a byte sink to `core/io/sim_io.rs`), wrapped with `mind_stdb::snapshot`'s header
   (command id = `last_applied_command_id`, sim tick, checksum) and split into chunks;
   `publish_snapshot`. The host answers every `my_match_snapshot_requests` row with a fresh snapshot.
2. Decoder (joiner side): `MindNet` binds `my_match_snapshot_chunks`, reassembles
   (`snapshot::reassemble`), updates `snapshot_progress`, writes the save to
   `user://mp/join-<match>.msav`, calls `SimHost.load_map_world(path)` (I06 T1) and the
   non-campaign session start used by `MindCampaign.start_map_mode` with the save's rules (factor
   a `start_loaded_world(rules)` helper out of I06's code if needed), sets the sim tick to the
   snapshot tick, and then applies every relayed command with `command_id` > the snapshot's command
   id in order (`CommandStream`). Show the loading fragment with `@connecting` and the progress.
3. `join_match` passes the real `build_id`/`content_hash`/mods; a mismatch shows Java's messages
   (`@server.kicked.clientOutdated`, `@server.kicked.serverOutdated`, mod mismatch: read
   `J/net/Packets.java` `KickReason` and their bundle keys).

**Verify.** Client A hosts (T4), builds 3 walls; client B joins -> B shows A's world including
the walls (screenshots of both); A builds a 4th wall after B joined -> B sees it (T6 needed for
this half; until T6 lands verify the snapshot half only).

**Commit.** `Fixes PAR-0123: joining loads the host snapshot and replays later commands`

---

### T6 — Every player action is relayed and applied on every client (Fixes PAR-0120)

**Changes.**
1. A single outbound seam: `MindSimHost::submit_player_command(cmd: SimCommand)`. When
   `MindNet` is in a match, convert to `CommandKind` (new `gdext/net/relay.rs::sim_to_kind`) and
   send it through `CommandSender::send` (prediction per `predict_policy`: `Immediate` kinds are
   ALSO enqueued locally now; `AwaitEcho` kinds only on echo); when offline, enqueue locally as
   today. Route every caller that enqueues a player action (`input/desktop.rs:1631, :3272`,
   `input/mod.rs:1441`, and `grep -rn "enqueue_sim_command" client/rust/mind-gdext/src` for the
   rest: configure, rotate, unit command/stance, payload, inventory transfer, building control,
   delete plans) through it.
2. `kind_to_sim` covers every `CommandKind` that has a `SimCommand` equivalent: resolve block /
   item / unit names through the content registry (`registry.block_by_name(...)`), never by a
   hardcoded table; the team is the SENDER's team from the member table (`get_members_json` has
   it), not 0; `player` is the sender's player slot. Kinds with no sim equivalent yet are
   listed in the summary (do not drop them silently: log once per kind).
3. Determinism: apply relayed commands at `client_tick` (the sender's tick + 6) on every client;
   a command whose tick is already in the past is applied at the next tick boundary (the host's
   checksum then decides; T13 resyncs).

**Tests.** `sim_to_kind`/`kind_to_sim` round trip for every variant with a sim equivalent
(`gdext/net/relay.rs` tests); `mind-stdb` relay ordering tests stay green; register the
`mp_command_log_replay` and `mp_late_join` scenarios from `headless/mp_scenarios.rs` in the
scenario registry and make them pass (`cargo run -p mind-headless -- run mp_late_join`).

**Verify.** Two clients: A places a conveyor line, configures a sorter, rotates a block, commands
a unit; B shows the same results (screenshots, `building_debug` on both); checksums equal
(`checksum_report()` on both shows no mismatch after 600 ticks).

**Commit.** `Fixes PAR-0120: player actions relay to every client and apply in command order`

---

### T7 — Remote players are visible: units, names, build plans

**Java.** Other players' units draw like any unit, with the player name above
(`J/graphics/OverlayRenderer.java` / `PlayerComp.draw`: name label with the player colour), and
their build plans/beams show when `showOtherBuildPlans` (`Renderer.java:42`).

**Changes.** Each client reports its player state (`report_player_state`, LocalOnly policy:
position, rotation, unit type, building plan) every N ticks; `MindNet` spawns/updates a
remote-player unit entity per member (kind `Player`-like but driven by the reported state; never
by local input), removes it on leave. Draw the name label (outline font, the player's colour).

**Verify.** Two clients: B sees A's alpha move and build (screenshot with A's name label).

**Commit.** `Gameplay I08 T7: remote players' units, names and build plans`

---

### T8 — Chat works: send, receive, team/admin modes, commands (Fixes PAR-0121)

**Java.** `ChatFragment` (`J/ui/fragments/ChatFragment.java`, 353): T (Binding.chat) opens the
field; Enter sends; history with up/down; modes `normal`, `team` (`/t `), `admin` (`/a `, admins
only); messages fade after a time; `NetServer` handles `/t`, `/a`, `/help`, `/votekick`, `/vote`
(`J/core/NetServer.java`, grep `register("` for the client command list).

**Changes.**
1. `MindUi.chat_send` -> `MindNet.send_chat(text, mode)` (map `team` -> `ChatKind::Team`, `admin`
   -> a new `ChatKind::Admin` in the server module if absent; the server filters recipients by team
   / admin). Bind `my_match_chat`; each new row -> `chat_fragment.add_message(formatted)` with
   Java's format (`[coral][[<name>[coral]]:[white] <text>`: read `NetClient.sendMessage`/
   `ChatFragment.addMessage` for the exact format).
2. Client commands: implement `/help`, `/t`, `/a`, `/votekick <player>`, `/vote y|n` as Java's
   `NetServer` client commands, executed server-side as reducers (`votekick_start`, `vote`) in
   `server/spacetimedb/src/chat/` (a vote session table per match; Java rules: 3+ players,
   `voteDuration`, required votes = `Math.ceil(players/2)`... read `NetServer.java` VoteSession).
3. Offline (no match) the chat fragment is not openable (Java: `ChatFragment` only when `net.active()`).

**Verify.** Two clients: A types "hi" -> B's chat shows `A: hi`; `/t` reaches only teammates.

**Commit.** `Fixes PAR-0121: chat sends, receives and renders, with team/admin modes and votekick`

---

### T9 — Player list and admin actions (Fixes PAR-0122)

**Java.** `PlayerListFragment` (`J/ui/fragments/PlayerListFragment.java`, 270): Tab shows every
player (icon, name, team colour, admin badge); admins get per-player buttons: kick, ban, trace
(`TraceDialog`), team switch (in PvP), admin toggle. `AdminsDialog`/`BansDialog` list admins/bans
with remove buttons.

**Changes.** `MindUi.player_list_json()` returns the match members from `MindNet`
(`get_members_json` + `all_players` names/colours + admin flag). `player_action(name, action)`
calls the matching admin reducer (`admin_kick`, `admin_ban`, `admin_switch_team`, `admin_grant`)
and returns the result. Add the admin views (`am_i_admin`, `all_bans`, `all_admins`) to a new
subscription wave entered when the local player is admin; feed `admins_dialog`/`bans_dialog`
contexts from them, with unban/unadmin buttons. Trace: `trace_dialog` from the player row
(the relay has no IP; show the identity and the fields the server has, list the rest as a
deviation). Kicked players get `my_kick` -> Java's kick dialog (`@server.kicked.<reason>`).

**Verify.** Two clients: A (host, admin) opens the list (Tab), kicks B -> B shows the kicked dialog
and returns to the menu; ban -> B cannot rejoin; Bans dialog lists B; unban works.

**Commit.** `Fixes PAR-0122: player list, kick/ban/trace/team actions and Admins/Bans dialogs`

---

### T10 — Dedicated server hosts a real networked match (Fixes PAR-0124)

**Java.** `ServerControl` (`JS/ServerControl.java`, 1471): `host [map] [mode]` loads the map and
starts hosting; `runwave`, `say`, `kick`, `ban`, `players`, ... act on the live game.

**Changes.** `mind-headless server`/`serve`: `host` opens the connector (`NetHost::ensure_connected`),
loads the map into a real `Sim` (the headless crate already runs sims for scenarios:
reuse that loader instead of the `LocalHost` fixture), creates the match (as T4, with the
dedicated server as host and no local player), publishes snapshots on request, steps the sim at
60 Hz, applies relayed commands (T6's `kind_to_sim` must live in a crate the headless can use:
move it to `mind-stdb` or a shared module), publishes checksums; `runwave` calls
`admin_run_wave` AND applies the wave locally; `say` -> `send_chat(System)`; `kick`/`ban` ->
admin reducers; authenticate with `--admin-token-file` (an admin identity token).

**Verify.** `cargo run -p mind-headless -- serve --db mindustry-godot --commands "host groundZero survival"`
(check the exact flags with `--help`); a Godot client joins (T5) and sees the map; `say hello`
shows in the client's chat; `runwave` spawns a wave on both.

**Commit.** `Fixes PAR-0124: the dedicated server hosts a networked match and its console commands act on it`

---

### T11 — Chat filters are per match (Fixes PAR-0125)

**Changes.** `server/spacetimedb/src/chat/mod.rs`: `chat_filter` gets `match_id: Option<u64>`
(`None` = server-wide, kept for the dedicated server's global config); `admin_add_chat_filter`
takes the match id (host/admin of that match); `send_chat` applies global + that match's filters.
Regenerate bindings (`server/build.sh`); update `mind-stdb` wrappers and the headless server.

**Tests.** Server module unit tests: a filter added in match 1 does not affect match 2.

**Commit.** `Fixes PAR-0125: chat filters and mutes are scoped to their match`

---

### T12 — Host dialog layout per Java (Fixes PAR-0151)

**Java.** `HostDialog` above: width 300; row 1 `@name` + field (max 40) + a 54x54 colour button
(`Styles.squarei`, image tinted with the player colour); row 2 `@server.port` + field (max 5,
valid 1..65535, settings `port`, default 6567); row 3 `@host` (300x70, disabled when the port is
invalid) + `?` (65x70, `@host.info`); `hostinfo` shown once (`getBoolOnce`). Reference
`step-375-host.png`. The port field has no meaning over the relay: keep it (stored in settings)
for layout parity and note it.

**Commit.** `Fixes PAR-0151: host dialog layout per HostDialog`

---

### T13 — Desync recovery and state publication

**Changes.** On a checksum mismatch (`drain_checksums` :777-802 already requests a snapshot), the
non-host client applies the next host snapshot (T5's decoder) and resumes from its command id;
`snapshot_progress` reports progress; the host publishes `publish_match_state` (wave, wavetime,
enemies, paused, game_over) every second so the HUD of joiners matches. Register and pass the
`mp_desync_injection`, `mp_reconnect_gap` and `mp_soak` scenarios.

**Commit.** `Gameplay I08 T13: desync resync from host snapshots and match state publication`

---

## 3. Order and hot files

T1..T13 in order. Hot files: `gdext/net/mod.rs`, `gdext/net/relay.rs`, `gdext/input/desktop.rs`
(T6: the enqueue call sites only), `ui/ui_root.gd`, server module (`server/spacetimedb/src/`;
always regenerate bindings with `server/build.sh` in the same commit and never hand-edit
`mind-stdb/src/module_bindings/`).

## 4. Ledger changes to report

| Item | After |
|---|---|
| PAR-0136 | verified (T3) |
| PAR-0123 | verified (T5) |
| PAR-0120 | verified (T6) |
| PAR-0121 | verified (T8) |
| PAR-0122 | verified (T9) |
| PAR-0124 | verified (T10) |
| PAR-0125 | verified (T11) |
| PAR-0151 | verified (T12) |

## 5. Done criteria

Two-client screenshots for T5-T9 under `runs/<date>-s<N>-I08-*`; server module tests
(`cd server/spacetimedb && cargo test`), `mind-stdb` tests and the registered `mp_*` scenarios
green; summary in `REPO/docs/I08-multiplayer_summary.md`.
