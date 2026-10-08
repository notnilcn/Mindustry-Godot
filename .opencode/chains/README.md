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
| `probe-hud.md` | state JSON, inspector, screenshot, logs |
| `enter-campaign.md` | main menu → planet dialog → sector → launch (UI and facade variants) |
| `sector-preset-rules.md` | campaign launch → `get_rules_json` vs preset captureWave/winWave (EV-0049) |
| `ground-zero-fresh-launch.md` | clear leftover sector saves → restart → UI launch → core/entity/game-over assertions (EV-0037) |
| `ground-zero-production-probe.md` | fresh Ground Zero → placed drill on overlay ore + adjacent core → deterministic first-delivery bracket (EV-0047) |
| `campaign-rules-dialog.md` | menu → campaign planet page → `campaign.difficulty` rail button / MindUi `open_dialog` → body + liveness + close |
| `campaign-save-load.md` | campaign facade → launch → save / list / load a slot |
| `load-game-dialog.md` | main menu → Load Game → live slot list → card click queues + applies a load |
| `research-purchase.md` | campaign launch → research dialog → root rail → purchase a locked node |
| `java-reference-leg.md` | Java reference: launch → drive → capture → quit (computer-mcp) |

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

Consumers: the `twin-evaluator` and `gap-identifier` subagents,
`parity-orchestrator`, `parity-evaluator`, `parity-writer`, and any playtest
session. Skills point here; they do not duplicate the sequences.
