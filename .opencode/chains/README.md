# MCP call-chain ledger

Proven, project-specific sequences of open-godot-mcp calls for Mindustry-Godot.
Agents consult the matching chain before composing their own tool calls and
append a chain (or bump one to `verified`) after a run confirms the sequence.

Generic MCP mechanics stay in the skills; this directory holds only what is
specific to this project: which sequence enters a flow, which node/method to
touch, which precondition is easy to miss.

## Layout

| File | Chain |
|---|---|
| `boot-and-identity.md` | editor bridge → `game.tscn` → runtime connected → pid stamp |
| `golden-checksum.md` | `load_scenario` + `step(60)` + checksum vs committed golden |
| `pause-step-interact.md` | pause, step, API and mouse place/break, input flush |
| `input-controls.md` | no-scenario world, keyboard input vs `get_input_state_json`, API comparison |
| `ui-control-click.md` | runtime-created Button click (menu Play, paused Settings) → pressed counter + submenu/dialog stack read-back (EV-0062) |
| `editor-maps-row-click.md` | menu Editor → live map registry (18 rows) → row click → `EditorDialog.visible` + `MindEditor.status` file/size (EV-0038) |
| `rts-select-orders.md` | command mode → drag/tap/double-tap select → right-click order vs `pending_command_count`/`commands_applied` (EV-0055) |
| `probe-hud.md` | state JSON, inspector, screenshot, logs |
| `enter-campaign.md` | main menu → planet dialog → sector → launch (UI and facade variants) |
| `host-match-from-pause.md` | pause `@hostserver` → host dialog port field/runHost → `MindNet.create_match` vs `in_lobby` + `relay_match` row (EV-0059) |
| `paused-dialog-buttons.md` | campaign launch → Escape → paused dialog entry/rect dump (desktop set, `@objective` full text) (EV-0063) |
| `join-direct-connect.md` | main menu Join Game → Add Server address → OK → `MindNet.connect_to` vs `browsing` + live `all_matches` rows (EV-0060) |
| `sector-preset-rules.md` | campaign launch → `get_rules_json` vs preset captureWave/winWave (EV-0049) |
| `ground-zero-fresh-launch.md` | clear leftover sector saves → restart → UI launch → core/entity/game-over assertions (EV-0037) |
| `game-over-loss.md` | `load_sector`/`start_sector` serpulo 170 → live zero-core game over → game-over dialog opens, HUD flag deferred, frames keep advancing (EV-0061) |
| `ground-zero-production-probe.md` | fresh Ground Zero → placed drill on overlay ore + adjacent core → deterministic first-delivery bracket (EV-0047) |
| `factory-recipe-probe.md` | fresh Frozen Forest (serpulo/86) → drill on coal + graphite-press + core sink → graphite bracket at 600/1200 (EV-0058) |
| `campaign-rules-dialog.md` | menu → campaign planet page → `campaign.difficulty` rail button / MindUi `open_dialog` → body + liveness + close |
| `campaign-save-load.md` | campaign facade → launch → save / list / load a slot |
| `load-game-dialog.md` | main menu → Load Game → live slot list → card click queues + applies a load |
| `research-purchase.md` | campaign launch → research dialog → root rail → purchase a locked node |
| `placement-picker-audit.md` | campaign launch → in-game placement picker → catalog filtering / icon / clipping audit (EV-0013) |
| `java-reference-leg.md` | Java reference: launch → drive → capture → quit (computer-mcp) |
| `java-join-dedicated-server.md` | Java reference: `server:dist` dedicated server on 6567 → Join Game LAN-discovery row → connect into it (EV-0060) |
| `java-custom-survival-wave.md` | Java reference: custom survival game → skip/timer wave → spawn/move/combat captures |
| `java-campaign-loss.md` | Java reference: campaign sector → pause `Abandon` → core self-destruct → `GameOverDialog` + `Continue` (EV-0061 twin) |
| `units-live-wave-runtime.md` | Godot: fresh groundZero → `run_wave` → live unit/bullet assertions (EV-0048) |
| `hud-wave-enemies-skip.md` | fresh groundZero → MindHud wave/enemies/status text vs fragment mirror + skip button `canSkipWave` + capture toast (EV-0053) |
| `command-mode-hold-vs-tap.md` | Twin: Java Shift-hold affordance (Command Mode panel) vs Godot `command_mode` state, hold + `commandmodehold=false` tap branches (EV-0044) |
| `placement-rotation.md` | select conveyor → R rotates pending (`input_state_json.rotation`) → mouse place → dump tile `rot` → R over placed building cycles `rot` mod 4 (EV-0056) |
| `custom-game-map-list.md` | menu Play → Custom Game → `DialogLayer/custom._maps()` live rows (18, real width/height/author/path) + Editor cross-check (EV-0039) |
| `campaign-live-views-capture.md` | planet dialog live sector state → `capture_sector` → panel/read-model update (EV-0051) |

## Entry schema

```markdown
---
id: boot-and-identity
title: one line
status: seeded | verified
applies_when: when this sequence is the right one to run
preconditions:
  - bridge up, runtime connected, display/env requirements
tools: [godot_health, godot_editor_read, ...]
last_verified: <date> <commit> (<run dir>) | not yet
---
```

Body sections: `## Steps` (exact tool + action + params, one step per call),
`## Success signals`, `## Failure modes`, `## Variants`.

## Rules

- **One chain per file.** Parallel agents editing different chains never
  conflict; never rewrite another agent's chain mid-run.
- **Statuses are honest.** `seeded` means compiled from a skill recipe, not yet
  re-run; `verified` requires a run directory and a pid/commit in
  `last_verified`.
- **Steps are exact.** Tool name, action, and the parameter JSON that worked.
  No prose-only sequences, no hardcoded click coordinates — resolve controls
  through `get_global_rect().get_center()`.
- **Preconditions include the host.** Servers that need a display or another
  process-start environment variable belong in `preconditions`; the Java leg
  (computer-mcp) needs `DISPLAY` at opencode start, the Godot leg does not (the
  loop wrapper hands it the weston/Wayland compositor through
  `WAYLAND_DISPLAY`/`XDG_RUNTIME_DIR`).
- **Keep it current.** When a step fails at the same commit, fix the chain in
  the same change that fixes the tooling.
- **Server-tag each chain.** The Godot leg uses open-godot-mcp (`godot_*`
  tools); the Java leg uses computer-mcp (`computer-mcp_*` tools, or
  `parity-click.sh` for scripted clicks). Name the server in the chain's
  `tools` list so the next agent picks the right protocol.
- **Friction is not a chain.** One-off tool errors and their workarounds go to
  the repo-root `open-godot-mcp-learnings.md` / `computer-mcp-learnings.md`,
  not here.

Consumers: the `gap-identifier`, `parity-writer`, and `twin-evaluator`
subagents, the `parity-orchestrator` / `twin-orchestrator` contracts, and any
playtest session. Skills point here; they do not duplicate the sequences.
