# AGENTS.md — Mindustry-Godot

Godot 4.7 client of the Mindustry 1:1 port: GDScript UI (`client/ui`, `client/scenes`) over the Rust simulation (`client/rust/`). Read [`client/AGENTS.md`](client/AGENTS.md) and [`client/rust/AGENTS.md`](client/rust/AGENTS.md) before touching a tree. The parity contract lives in [`.opencode/skills/parity-eval/SKILL.md`](.opencode/skills/parity-eval/SKILL.md); the work queue and proof record is [`parity-ledger.json`](parity-ledger.json); progress is summarized in [`PROGRESS.md`](PROGRESS.md).

## Reference soak — deferred blocker group

The `/goal reference-soak` stages Java-only reference evidence onto the 2026-10-10 code-scan items (PAR-0070..PAR-0141). Stageable items run **first**, in id order; only after every stageable item is settled does the soak work the group below, which is staged **last** because a single Java client cannot complete it. For each blocker item the soak stages the single-client half where it exists, records the missing fixture as the blocker on the ledger item, then updates this table.

| Item | Title | Blocker | Single-client half | Status |
|---|---|---|---|---|
| PAR-0115 | Installing a mod changes nothing in the game | mod fixture (content mod) | Mods dialog state | staged: runs/20261010-234250-ref-PAR-0070-combat-fx (menu-05-mods.png; No mods found) |
| PAR-0116 | Mods that ship scripts never run / no unsupported mark | script-mod fixture | Mods dialog unsupported warning surface | staged (same run; dialog captured empty; needs fixture) |
| PAR-0117 | Disabling a mod does not stick after restart | mod fixture + restart | Mods dialog toggle | staged (same run; needs fixture + restart) |
| PAR-0118 | After a mod-caused crash: no Restore button | crashing-mod fixture | Mods dialog restore entry | staged (same run; never run against the seeded profile) |
| PAR-0119 | Exported builds ship without assets (placeholder quads) | export build/pipeline (Godot side) | boot the packaged Java jar | staged: the jar boots with assets (step-300; menu renders) |
| PAR-0120 | Multiplayer: world actions do not synchronize | second client | host/join dialog state | staged: step-375-host.png + step-381-join.png |
| PAR-0121 | Multiplayer: chat is inert | second client | chat input/send half | staged: join dialog rows (step-381); chat needs a hosted match |
| PAR-0122 | Multiplayer: player list empty; kick/ban/trace/team dead | second client (+ admin surface) | Admins/Bans dialogs, empty player list | staged: host dialog (step-375); list needs a hosted match |
| PAR-0123 | Multiplayer: late join does not sync the world | second client | join dialog / loading state | staged: step-381 (Discovering games.../connect states) |
| PAR-0124 | Dedicated server: `host` never creates a networked match | dedicated server process | console `host` command surface | staged: java/dedicated-server-host.log (server boots; `host` opens 6567) |
| PAR-0125 | Chat filters are database-global, not per match | second client + live DB | host dialog chat-filter controls | staged: host dialog (step-375); needs a live DB |
| PAR-0136 | Join Game forgets added servers; Remote list always empty | none (fully stageable) — kept last by policy | full single-client walk (add/persist/restart) | staged: step-381-join.png (the persisted 10.8.0.84 entry survived every relaunch; community fetch 270) |

Update the Status column (`pending` -> `staged: <run dir>` or `blocked: <reason>`) as each row is worked; the ledger item stays the source of truth for detail. All twelve rows were staged on 2026-10-11 in `runs/20261010-234250-ref-PAR-0070-combat-fx/`; each ledger item carries its `reference` block with the captured half and the recorded missing fixture (the multiplayer ones need a second client; the mods need fixtures; PAR-0119's export pipeline is Godot-side). PAR-0136 settled as `reference` (the persistence surface is proven by the surviving server entry).

Soak progress (2026-10-11, session 1): 76 of 141 ledger items now carry Java reference blocks (55 status=reference; the rest are cross-links on mismatch/blocked/seeded items). New staged run: `runs/20261010-234250-ref-PAR-0070-combat-fx/` (continuous client JVM pid 81138 -> relaunched 108093; Mud Flats sandbox world with mech/source/generator/battery/void/switch/processor staged; Domain survival combat walk). Blocked-staging notes recorded on PAR-0084 (needs a throwaway profile for the sector loss) and PAR-0093 (message config re-stage). The blocker group below is still pending.

## Java reference profile state

The operator imported `mindustry.save.full.tech.tree.zip` (workspace root, `../mindustry.save.full.tech.tree.zip`) through Settings > Game Data > Import Data (the import closes the client). The fully-unlocked state lives in the default Java data dir (`~/.local/share/Mindustry`); a reference soak seeds `parity-java-home` from it once before starting. Do not re-import through the UI and never clear Game Data / research / saves. The Godot side needs a working data import before reference-backed walks of late-game items can mirror this state (ledger notes: PAR-0069, PAR-0134).
