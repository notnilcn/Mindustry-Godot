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
| `probe-hud.md` | state JSON, inspector, screenshot, logs |
| `enter-campaign.md` | main menu → planet dialog → sector → launch (UI and facade variants) |

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
  (computer-mcp) needs `DISPLAY` at opencode start, the Godot leg does not.
- **Keep it current.** When a step fails at the same commit, fix the chain in
  the same change that fixes the tooling.

Consumers: the `evaluator` and `gap-identifier` subagents, `loop-runner`, and
any playtest session. Skills point here; they do not duplicate the sequences.
