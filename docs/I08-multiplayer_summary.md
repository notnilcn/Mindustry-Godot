# I08 — Multiplayer summary (pass 1, sessions 3+4, 2026-10-16)

Plan: `docs/parity-plans/I08-multiplayer.md`; environment: `docs/parity-plans/I-environment.md`.
Ran with **two session slots** as the plan's T1 requires: primary/dev **session 3**
(`Mindustry-Godot-session3`, branch `parity/session-3`, bridge 6990, display :12) and
test client **session 4** (`Mindustry-Godot-session4`, branch `parity/session-4`, bridge
7000, display :13). All commits are on `parity/session-3`; session 4 only ever received
merges and builds (never an authored commit). The Java client was never launched.

Status: **T1–T6 done (T5/T6 with the caveats below). T7–T13 not reached.**

## Per-task status

| Task | Status | Evidence / caveat |
|---|---|---|
| T1 local harness + online on demand | **done** | `tools/mp-local.sh`, `ensure_online`, `session_changed`, `@connectfail` modal; two-client launch documented below |
| T2 name/colour persist + sync | **done** | `player.color` new column + `set_color` reducer; join/host dialogs persist `name`/`color-0`; verified across restart + server rows |
| T3 join dialog servers (PAR-0136) | **done** | `servers` JSON persisted (add/edit/move/delete), per-server probes with ping/matches, `@host.invalid` for unreachable; screenshot vs `step-381-join.png` |
| T4 host current world (PAR-0120 part 1) | **done** | `host_current_world` + `my_matches` binder + deferred `start_match(true)` + `encode_snapshot`/`publish_snapshot`; Running match + snapshot row verified |
| T5 join loads host world (PAR-0123) | **done headless+in-engine (snapshot half)** | B loads the snapshot (walls from A's world visible), request/response + progress + `@connecting`; kick-reason dialogs NOT done (see blockers); the "4th wall after join" half needs T6 and is covered by T6's relay |
| T6 relay every player action (PAR-0120) | **done in-engine except checksum equality** | seam + full mapping + prediction; A's conveyors/sorter/configure/rotate applied on B; **checksum equality blocked** by `checksum_scoped` being a stub (below) |
| T7 remote players' units/names | not reached | |
| T8 chat (PAR-0121) | not reached | |
| T9 player list/admin (PAR-0122) | not reached | |
| T10 dedicated server (PAR-0124) | not reached | |
| T11 per-match chat filters (PAR-0125) | not reached | |
| T12 host dialog layout (PAR-0151) | not reached | |
| T13 desync resync/state publication | not reached | owns the checksum-scope prerequisite found by T6 |

## Commits on `parity/session-3`

```
435424cb Merge branch 'main' into parity/session-3
143c017b Fixes PAR-0120: player actions relay to every client and apply in command order
a58c9154 Fixes PAR-0123: joining loads the host snapshot and replays later commands
2d672b4e PAR-0120: hosting creates a running match from the current world and publishes a snapshot
efc733c8 Fixes PAR-0136: Join dialog persists servers and lists remote matches
957815ab Merge branch 'main' into parity/session-3
aee5d517 Gameplay I08 T2: player name and colour persist and sync to the server
b7bf9d81 Gameplay I08 T1: local multiplayer harness and online-on-demand connection
```

## Two-client launch (T1 item 4)

```bash
# SpacetimeDB + module (idempotent; log /tmp/opencode/stdb.log, pidfile /tmp/opencode/stdb.pid)
tools/mp-local.sh up            # -> 127.0.0.1:3000/mindustry
# client A (host/dev) — session 3, bridge 6990, display :12
tools/session-play.sh 3 --play; export OPEN_GODOT_MCP_PORT=6990
# client B (test client) — session 4, bridge 7000, display :13
tools/session-play.sh 4 --play; export OPEN_GODOT_MCP_PORT=7000
```

Both games use their own session XDG `user://` dirs, so settings and the STDB identity
tokens never collide (T1 showed three distinct player rows: publish CLI, client A
`0xc200ab42…`, client B `0xc2003de7…`). `--pN` remains for same-dir scoping; Rust
changes are built into both worktrees (`tools/build.sh` in WT3, then merge
`parity/session-3` into WT4 and build there).

Note: session 4's test mods (`assets-mod`, `basic-mod`, left by I07 verification) were
**disabled** (test-only, no commit) — with them enabled the content-hash/mods build
gate correctly rejects the join.

## Tests and exact results

```
cargo test -p mind-core --lib --features msav-import   -> 2681 passed; 0 failed; 4 ignored
cargo test -p mind-headless --lib                      -> 133 passed; 0 failed
cargo test -p mind-gdext --lib                         -> 54 passed; 0 failed
cargo test -p mind-stdb                                -> 49 passed; 0 failed; 4 ignored
cargo clippy -p mind-core -p mind-gdext --all-targets -- -D warnings   -> clean
cargo run -q -p mind-headless -- parity goldens        -> 97 PASS / 0 other
cargo run -q -p mind-headless -- run mp_command_log_replay -> pass (applied 240, checksums match)
cargo run -q -p mind-headless -- run mp_late_join          -> pass (snapshot+tail equal)
cargo check --manifest-path server/spacetimedb/Cargo.toml --tests -> clean
server/build.sh --check                                -> bindings drift-clean
godot --headless --editor --quit --path client | grep -E "SCRIPT ERROR|Parse Error" -> nothing
```

## In-engine evidence (runs/20261016-s3-I08-t1/)

* **T1**: `mp-local.sh up` published; A `ensure_online()` -> true, `session_state()` ->
  `browsing`, `session_changed("browsing")` observed; `spacetime sql` player rows for A
  and B distinct. `connect_to("127.0.0.1:3999/dead")` -> false +
  `t1-connectfail-dialog.png` (Connection error modal); reconnect -> browsing.
* **T2**: after a restart the Join dialog restores name "Pylon" + colour
  `0xE64D59FF` (`t2-join-identity.png`); edits land server-side (`player`:
  `0xc200ab42… | "Pylon" | 3863828991`); a name set while offline ("Beacon") is pushed
  on the next `ensure_online()`.
* **T3**: `servers` = `[{"ip":"127.0.0.1:3000","port":0}]` persists across restarts;
  add/edit/move/delete (with `@server.delete` confirm) all persist;
  `t3-join-servers.png` shows local + remote rows, ping, the hosted match
  (`groundZero survival 1/8`) and `10.8.0.84:3000` -> "Can't connect to host."
  (matching `runs/20261010-234250-ref-PAR-0070-combat-fx/java/step-381-join.png`).
* **T4**: A in custom game "Maze"/sandbox -> pause -> Host -> `session_state()` =
  `in_game`, `local_admin()` = true; `relay_match` match id `"Maze"` status
  (running); `match_snapshot` row (dynamic, 2 chunks, 17827 bytes);
  `t4-host-running.png`.
* **T5**: A placed 3 stone walls, hosted; B `join_match` -> `snapshot_sync` ->
  `in_game` progress 1.0; B's tiles (113..115,84) = `stone-wall`; both screenshots
  identical (`t5-host-a-walls.png`, `t5-join-b-walls.png`); the host answered a
  request with a fresh snapshot (server row, `request_snapshot` reducer).
* **T6**: A (session 3) via the seam: 3 conveyors + sorter placed, sorter configured
  `titanium`, rotated; B applied the relayed `match_command` rows in order
  (`last_applied_command_id` 17 on both); B: tiles `conveyor`, config
  `{"type":"item","id":6}`, rotation 3 = A's; `t6-host-a-actions.png`,
  `t6-join-b-actions.png`.

## Deviations from Java

1. **Community server fetch not implemented** (T3): the Java URLs
   (`Vars.serverJsonURLs`, `J/Vars.java:89`, `JoinDialog.java:703`) serve Java servers
   the relay cannot talk to; the port's Community section lists the active connector's
   `all_matches` with the Java search filter (`JoinDialog.java:417-424`).
2. **Colour lives on `player`** (T2), not `create_profile`: Java's `player.color` is
   per player and profiles can be multiple per identity; `HostDialog.java:33` stores
   `Color.rgba()` as the `color-0` setting — the port mirrors that and sends it with
   `set_color`.
3. **Ping is not an init packet** (T3): the relay has no Java `pingHost`; `ping_server`
   measures connect + lobby-subscription round trip (`Net.java`/`ArcNetProvider` has
   no equivalent).
4. **Host dialog mode row is presentational** (T4): the live world's rules are
   authoritative (`HostDialog.java` has no mode row; T12 checks the layout).
5. **`host_current_world(name, map_id, seed)`** (T4): the plan wrote `(name)`;
   `ui_root._current_map_id` computes the live map id from
   `MindCampaign.get_sector_state().mapName` (new key; `planet-sector` for campaigns).
6. **Probes gate the SDK's blocking connect** (T3): `DbConnectionBuilder::build` runs
   `block_on` on the caller (`spacetimedb-sdk-2.10.1/src/db_connection.rs:979`), so a
   black-holed address froze the frame loop; a 400 ms TCP reachability probe now
   precedes every `connect()` (`net/mod.rs::probe_reachable`).
7. **Join rejection shows the raw message** (T5): Java's `@server.kicked.*` texts need
   reducer-error surfacing; the SDK (2.10.1) does not expose server reducer errors as a
   client event, and `my_kick` handling is T9's. The join gate itself works (verified
   with the mods/content-hash rejection and the server log).
8. **Snapshot scope**: the joiner applies the newest *complete* snapshot (an older
   snapshot's chunks can complete first when they already exist) and then replays
   commands after its watermark (T6).
9. **`ConfigBlock.value` byte codec** (T6): the relay had no codec for
   `determinism::ConfigValue`; a tag-based `WireWriter` table (`net/relay.rs`) is the
   port's own format (both peers run the same code).
10. **`team`/`player` of relayed rows** (T6): the member table has no player slot, so
    `player = Some(0)` (the local path's convention, `desktop.rs:2955`) and `team` =
    member team (None -> the live default team; Java uses `player.team()`).

## Blockers / could not verify

* **T6 checksum equality**: `Sim::checksum_scoped` ignores its scope and returns the
  full-world checksum (`mind-core/src/sim/mod.rs:1774`), which includes each client's
  own player unit and tick-varying state; free-running peers can never match. The
  monitor consequently reports a desync at command 0. World-cohort scoping plus host
  state publication is T13's task.
* **T5 kick-reason dialogs** (deviation 7 above).
* **T6 unit command in-engine**: the mapping + round trip are unit-tested, but no unit
  was staged in the live fixture; the relay path is the same as the verified variants.
* **T6 rotate/config divergence found and fixed live**: `RelayRuntime` created after
  the `Connected` event never got the local identity, so the host double-applied its
  own echoes (rotation 2 vs 3). Fixed by seeding the identity in `ensure_relay`;
  re-verified equal.

## Ledger changes to apply (coordinator)

| Item | Recommendation |
|---|---|
| PAR-0136 | verified — T3 (persisted servers + remote match rows, evidenced) |
| PAR-0123 | verified — T5 (snapshot load + replay ordering; request/response verified) |
| PAR-0120 | verified — T4 (hosting creates a Running match + snapshot) + T6 (actions relay and apply in command order). Note: the checksum-equality acceptance clause is blocked by the `checksum_scoped` stub; recommend a note on the item pointing at T13 |
| PAR-0121/0122/0124/0125/0151 | untouched (T8–T13 not reached) |

## Done criteria (plan §5)

* Two-client screenshots for T5–T9: T5 and T6 captured; T7–T9 not reached.
* Server module typecheck, `mind-stdb` tests and the registered `mp_*` scenarios:
  green (`mp_command_log_replay`, `mp_late_join` pass; `mp_desync_injection`,
  `mp_reconnect_gap`, `mp_soak` not re-run).
* This summary exists.
* **Plan not complete**: T7–T13 remain; T6's checksum clause and T5's kick dialogs are
  recorded as blockers for T9/T13.

## Remaining work for the next pass

T7 (remote player units/names), T8 (chat), T9 (player list + admin, kick dialogs,
reducer-error surfacing), T10 (dedicated server; the sim<->kind mapping currently lives
in `mind-gdext/net/relay.rs` and must move to a crate the headless can use — mind-stdb
cannot see `SimCommand`, so a neutral module is needed), T11 (per-match chat filters),
T12 (host dialog layout), T13 (desync resync + **scoped checksum implementation** +
match-state publication).
