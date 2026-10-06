Multiplayer & Social Audit — Mindustry-Godot (read-only)
Verified against ledger first: only EV-0026 touches dialog plumbing; none of the below was previously filed.
Ranked NEW gaps
GAP: No player-facing way to host — HostDialog is unreachable and its Host button is a no-op
- severity: S1
- port: client/ui/dialogs/host_dialog.gd:11,56-60 (signal only; no listener anywhere), client/ui/dialogs_manifest.json:19 (registered but no entry), client/scenes/game.tscn (no PausedDialog node; no paused_dialog.gd exists), client/ui/fragments/menu_fragment.gd:33-49 (no host entry)
- upstream: core/src/mindustry/core/UI.java:62,214 (PausedDialog created), core/src/mindustry/ui/dialogs/PausedDialog.java:98-105,152 (@hostserver -> ui.host.show()), HostDialog.java:51-73 (port field, runHost)
- symptom: Pressing Host (if the dialog could be opened) emits host_requested, which nothing listens to; there is no menu or pause-menu button that opens the Host dialog at all. A player cannot start a session.
- campaign: blocks
- confidence: high
GAP: Join dialog never establishes a session; Connect/Add emit an unconnected signal
- severity: S1
- port: client/ui/dialogs/join_dialog.gd:12,235-245 (connect_requested has no listener), :99-115 (global/remote sections are static empty labels), :173-204 (local rows from _context["servers"], never supplied — menu opens with "{}" at menu_fragment.gd:318), no port field, no refresh/fetch, no match-id path; client/rust/mind-gdext/src/net/mod.rs:54-56,174 (networking offline unless --db/--pN)
- upstream: core/src/mindustry/ui/dialogs/JoinDialog.java:626-652 (connect -> net.connect), :403-412 local discovery, :237-243/:702+ remote/community fetch, :675-682 version gate
- symptom: "OK"/Connect closes the form and does nothing; the server list is permanently "@hosts.none"; even a hand-entered address cannot reach MindNet.join_match (which requires a numeric match id, not an address). No session can ever start from the UI.
- campaign: blocks
- confidence: high
GAP: Campaign co-op progression is not synchronized and is unreachable even server-side
- severity: S1
- port server: server/spacetimedb/src/campaign/mod.rs:25-101 (tables exist), :162-168 (writes host-only), :442-487 (persist_from_command host-only), :542-605 (my_campaign_* views filter host == ctx.sender() — joiners see zero rows)
- port client: client/rust/mind-stdb/src/waves.rs:12-50 (campaign tables/views are in no subscription wave), client/rust/mind-gdext/src/net/mod.rs:224-243 (create_match never passes campaign_id; HostParams.campaign_id left None at session.rs:97), no GDScript/mainline Rust call to create_campaign/save_sector_info/set_unlock/advance_turn; client/rust/mind-gdext/src/campaign.rs:122-147 (campaign is local MindCampaign only)
- upstream: core/src/mindustry/net/NetworkIO.java:33-43 (host writes all researched content to rules.researched), :45-64 (full world/rules transfer), core/src/mindustry/ui/dialogs/PausedDialog.java:98-105 (host a live campaign sector); research host-only matches ResearchDialog.java:506,552,568
- symptom: Two players can never share a campaign: no UI links a match to a campaign, the server campaign views are invisible to non-hosts, and no client reads/writes the campaign tables. Campaign remains strictly single-player/local.
- campaign: blocks
- confidence: high
GAP: In-match simulation commands are never routed through the relay
- severity: S1
- port: client/rust/mind-gdext/src/sim_host.rs:168-173 (mouse place/break applied locally only), client/rust/mind-gdext/src/net/mod.rs:368-397 (send_command_json supports only place/break and has no GDScript caller), client/rust/mind-gdext/src/relay.rs only drains inbound; no outbound bridge for config/rotate/plans/unit/SetRules/ResearchUnlock/SectorCapture
- upstream: core/src/mindustry/core/NetServer.java command handling + core/src/mindustry/net/AGENTS.md (client input snapshots/build plans are relayed and validated)
- symptom: If a session were somehow established, each client's world would silently diverge: local edits never reach the server, and the server's command log never replays other players' edits back. No "connection" gameplay exists.
- campaign: blocks
- confidence: high
GAP: Chat is a dead local loop — send goes nowhere, receive path absent, fragment cannot be opened
- severity: S2
- port: client/ui/fragments/chat_fragment.gd:72-78 -> client/rust/mind-gdext/src/ui/ui_host.rs:494-503 (emits chat_message, no listener; grep finds no add_message caller and no chat_message connection), :101 buffer only via dead call; no my_match_chat binder in client/rust/mind-gdext/src/net/mod.rs:190-199; chat_fragment.gd:123-129 toggle() never called (upstream keybind not dispatched — mind-core/src/input/binding.rs:775 has no consumer)
- upstream: core/src/mindustry/ui/fragments/ChatFragment.java:221-236 (Call.sendChatMessage), :65 (Binding.chat toggles while net.active())
- symptom: Typing in the chat field does nothing (no echo, no send); incoming chat can never be displayed; the chat panel is invisible by default and cannot be opened.
- campaign: degrades
- confidence: high
GAP: Player list is always empty, cannot be opened, and has no moderation actions
- severity: S2
- port: client/rust/mind-gdext/src/ui/ui_host.rs:520-522 (player_list_json() hardcoded "[]"), :508-515 (player_action logs and returns false), client/ui/fragments/player_list_fragment.gd:75-77 (every row click hardcodes "spectate"), :103-106 (toggle() has no caller; no HUD button, no keybind dispatch), my_kick subscribed (waves.rs:37) but no binder; port lacks kick/ban/trace/team/votekick UI entirely
- upstream: core/src/mindustry/ui/fragments/PlayerListFragment.java:149-247 (kick/ban/trace/team/admin/votekick), :240-246 vote kick, opened via HudFragment.java:406-410 and DesktopInput.java:233-234
- symptom: No player roster, team list, ping, or moderation UI can ever be shown; a host cannot kick/ban/vote from the UI.
- campaign: degrades
- confidence: high
GAP: No player-visible disconnect, reconnect, kick or version-mismatch handling
- severity: S2
- port: client/rust/mind-gdext/src/stdb.rs:237-259 emits disconnected/connect_error/resync, but no .gd connects to any of them; client/rust/mind-gdext/src/net/mod.rs:137 only updates session state; protocol.rs:40 check_protocol has no caller (export/tests only); session.rs/net/mod.rs pass empty build_id and content_hash 0 so server compat checks are skipped (relay/reducers.rs:160-166)
- upstream: core/src/mindustry/core/NetClient.java:138-145 (@disconnect error UI), :420-443 (kick shows reason / auto-reconnect on server restart), JoinDialog.java:654-673 (reconnect), :675-682 (safeConnect version mismatch)
- symptom: A dropped connection, kick, protocol/build mismatch or desync produces no prompt, toast or reconnect; the player sees nothing (desync is auto-snapshot-resolved silently in net/mod.rs:720-722).
- campaign: degrades
- confidence: high
GAP: Admins/Bans dialogs are inert shells; their open path uses unregistered dialog names
- severity: S3
- port: client/ui/dialogs/admins_dialog.gd:26-38 and client/ui/dialogs/bans_dialog.gd:25-36 (render _context["admins"]/["banned"], no action buttons/confirm), client/ui/fragments/player_list_fragment.gd:86-100 opens "server.bans"/"server.admins", but the registry names are bans/admins (dialogs_manifest.json:26-27), so open_dialog logs "unknown dialog" (ui_host.rs:174-179) and returns false; context is passed as "" so set_context_json is skipped (ui_host.rs:187-189)
- upstream: core/src/mindustry/ui/dialogs/AdminsDialog.java:42-52 (unadmin with confirm), BansDialog.java:42-47 (unban with confirm)
- symptom: The dialogs can never be opened from the player list, and even when opened show only "@server.admins.none"/"@server.bans.none"; no unadmin/unban is possible.
- campaign: n-a
- confidence: high
GAP: Ping HUD is hardcoded to 0 ms
- severity: S3
- port: client/rust/mind-gdext/src/ui/hud.rs:51-53,79 (ping: i32 default 0, no writer), client/ui/fragments/hud_fragment.gd:56-57 displays it
- upstream: core/src/mindustry/core/NetClient.java:45,403-409,784-785 (1 s ping loop, getPing()), HudFragment.java:553-572 (fps/ping label)
- symptom: HUD always shows "0 ms"; latency/connection quality is invisible.
- campaign: n-a
- confidence: high
GAP: No team selection, vote-kick or admin command surface; server reducers and console commands are unreachable
- severity: S3
- port: no team-select UI (grep finds none); client/rust/mind-core/src/ui/console.rs:74 (with_defaults registers only help), ui_host.rs:49,81,526 uses that registry; client/rust/mind-stdb/src/connector.rs:824-940 implements admin_kick/ban/unban/switch_team/... but MindNet exposes none; admin_switch_team/AdminTileOp require host (relay/methods.rs:358-368)
- upstream: PlayerListFragment.java:187-213 (team select), :240-246 (votekick), server/src/mindustry/server/ServerControl.java (kick/ban/... server commands)
- symptom: Hosts/admins cannot change teams, initiate votes, or use any server/admin command; PvP and moderation are non-functional.
- campaign: degrades
- confidence: high
Coverage notes
 1. Server multiplayer is substantially built and tested: match directory/membership, private ordered command log, rate/role gates, checksums, snapshots, chat filters, moderation and campaign tables (server/spacetimedb/src/{relay,chat,admin,campaign}), with live integration tests (client/rust/mind-stdb/tests/it.rs). The failure is client/UI wiring, not server capability.
 2. MindNet is a scene node (client/scenes/game.tscn:42), not an autoload (client/project.godot:22-29), and zero .gd files reference it; StdbConnector is the only pumped connector and defaults offline (stdb.rs:110-112), with MCP-only dev_create_match/dev_join_match helpers (stdb.rs:397-452).
 3. Host dialog also omits upstream's port field/info/name validation (HostDialog.java:51-73), and its mode values (@mode.survival, host_dialog.gd:14) don't match parse_gamemode's raw keys (net/mod.rs:780-788) — a latent bug behind the unreachable path.
 4. Research/sector gate semantics actually match upstream host-only research; the missing piece is state sync. The port has Rules.researched (mind-core/src/io/json/rules.rs:587; tech_tree.rs:382-389) but no host population equivalent of NetworkIO.java:33-43.
 5. Chat server side supports All/Team/System (server/spacetimedb/src/chat/mod.rs:85-124), but MindNet.send_chat hardcodes ChatKind::All (net/mod.rs:515-527), so team/admin modes are lost even if wired.
 6. waves.rs includes lobby all_matches but no client binder subscribes to a server browser; nothing translates an address/selection into the numeric match_id that join_match requires.
 7. Ping/roster models exist in Rust (mind-core/src/ui/player_list.rs, hud.rs) but the Godot consumers (player_list_fragment.gd, hud_fragment.gd) read only the stubbed endpoints.
 8. Kick delivery view my_kick is in the Game wave but never bound by MindNet/StdbConnector, so a kicked player would not be told; same for my_match_ui_events.
 9. Adjacent, not filed: map_play_dialog.gd:12,50 play_requested also has no listener, suggesting the single-player Custom Game path has a similar unwired action; out of scope here.
10. No file was modified during this audit; findings are source-traced only, with no in-engine execution.
Direct answers
- Can a player currently host a game from the UI? No. The Host dialog is registered but has no player-facing opener (its upstream opener, the in-game PausedDialog, is absent), and its host_requested signal has no listener. The only create-match paths are MCP dev helpers.
- Can a player currently join a game from the UI? No. The Join dialog opens from Play > Join Game, but Connect/Add only emit an unconnected connect_requested, the server lists are static/empty, there is no address→match_id resolution, and networking is offline by default unless the game is launched with --db/--pN.
- Is co-op campaign progression synchronized? No. Campaign state is local (MindCampaign, local settings) and single-player only. The server's campaign/sector/unlock tables and my_campaign_* views exist but are host-only, in no subscription wave, never written by the client, and RelayMatch.campaign_id is never set because MindNet.create_match drops it. Two players cannot progress a campaign together.
