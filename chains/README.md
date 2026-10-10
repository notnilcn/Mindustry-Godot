# MCP call-chain ledger

Proven, project-specific sequences of MCP calls for Mindustry-Godot. Agents
consult the matching chain before composing their own tool calls and append a
chain (or bump one to `verified`) after a run confirms the sequence. A chain
covers one leg (Godot via `godot_*`; Java via the shell helpers
`java_input.py` / `window_ctl.py` / `capture_screen.py`) or both legs of a
mirrored parity item.

A chain has two kinds of sequence:

- **Path (fast-forward)** — navigation to the item's fork, replayed back-to-back
  per leg as one tool-call chain. It is not evidence: never capture or diff its
  steps; confirm the fork once per leg and mirror what happens next.
- **Steps (under test)** — the action(s) after the fork, mirrored one pair per
  step and judged by the comparison gate.

Generic MCP mechanics stay in the skills; this directory holds only what is
specific to this project: which sequence enters a flow, which node/method to
touch, which precondition is easy to miss.

> Historical note: entries dated before 2026-10-09 reference the retired
> dual-loop workflow (`.opencode/loops`, `.opencode/evals/runs`, `PARITY_*`
> variables, `EV-####` ledger ids). Those paths no longer exist; treat the
> sequences as recipes, not as live paths. Current runs live in
> `runs/<stamp>-<item>/`, progress lives in `parity-ledger.json`, and work is
> driven by the `/loop` / `/goal` plugins.

## Java-leg tooling (computer-mcp retired 2026-10-11)

The Java reference leg runs through the parity-eval shell helpers in
`.opencode/skills/parity-eval/scripts/`, executed with the helper venv python
(`PARITY_PY="${PARITY_PY:-$HOME/.local/share/parity-eval/venv/bin/python}"`,
created by `bootstrap.sh`) and `DISPLAY=:10` per call. Chains recorded before
2026-10-11 name `computer-mcp_*` tools; convert them with:

| retired computer-mcp call | shell helper |
|---|---|
| `mouse_move {x,y}` | `java_input.py move X Y` |
| `click {x,y,button}` | `java_input.py click X Y BUTTON` |
| `double_click {x,y,button}` | `java_input.py dblclick X Y BUTTON` |
| `button_down` / `button_up {button}` | `java_input.py mousedown` / `mouseup X Y BUTTON` |
| `drag {start,end}` | `java_input.py drag X1 Y1 X2 Y2 left 300` |
| `key_press {key}` | `java_input.py key NAME 60` (toggles: `keydown` + `keyup`) |
| `key_down` / `key_up {key}` | `java_input.py keydown` / `keyup NAME` |
| `type {text}` | `java_input.py type TEXT` |
| `screenshot[_window]` | `capture_screen.py --window-title Mindustry --output PATH` |
| window ops / focus | `window_ctl.py raise/move/resize/unmaximize/list`; every `java_input.py` action activates the window first |
| scroll | `java_input.py scroll N` |
| timed key combos | `java_input.py seq '[[...]]'` (one process, one focus) |

`java_input.py` gives every call a fresh focus (EWMH `_NET_ACTIVE_WINDOW` +
`set_input_focus`), so a click cannot be swallowed as the activation click;
`send_event_click.py` is the XSendEvent fallback and `record_input.py` proves
delivery. Friction goes to `java-leg-learnings.md`.

## Layout

| File | Chain |
|---|---|
| `seed-survey.md` | seed sweep: boot both clients → enumerate a surface → one ledger item per element → classify with one action pair |
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
| `campaign-autosave-exit-save.md` | fresh/resumed serpulo:15 → launch save → stepped autosave interval → pause Quit / campaign_complete `@menu` / clean-quit dispose exit saves (EV-0064) |
| `load-game-dialog.md` | main menu → Load Game → live slot list → card click queues + applies a load |
| `research-purchase.md` | campaign launch → research dialog → root rail → purchase a locked node |
| `placement-picker-audit.md` | campaign launch → in-game placement picker → catalog filtering / icon / clipping audit (EV-0013) |
| `java-reference-leg.md` | Java reference: launch → drive → capture → quit (shell helpers) |
| `java-join-dedicated-server.md` | Java reference: `server:dist` dedicated server on 6567 → Join Game LAN-discovery row → connect into it (EV-0060) |
| `java-custom-survival-wave.md` | Java reference: custom survival game → skip/timer wave → spawn/move/combat captures |
| `java-campaign-loss.md` | Java reference: campaign sector → pause `Abandon` → core self-destruct → `GameOverDialog` + `Continue` (EV-0061 twin) |
| `units-live-wave-runtime.md` | Godot: fresh groundZero → `run_wave` → live unit/bullet assertions (EV-0048) |
| `hud-wave-enemies-skip.md` | fresh groundZero → MindHud wave/enemies/status text vs fragment mirror + skip button `canSkipWave` + capture toast (EV-0053) |
| `command-mode-hold-vs-tap.md` | Twin: Java Shift-hold affordance (Command Mode panel) vs Godot `command_mode` state, hold + `commandmodehold=false` tap branches (EV-0044) |
| `placement-rotation.md` | select conveyor → R and wheel rotate pending (`input_state_json.rotation`) → mouse place → dump tile `rot` → R over placed building cycles `rot` mod 4 (EV-0056) |
| `custom-game-map-list.md` | menu Play → Custom Game → `DialogLayer/custom._maps()` live rows (18, real width/height/author/path) + Editor cross-check (EV-0039) |
| `campaign-live-views-capture.md` | planet dialog live sector state → `capture_sector` → panel/read-model update (EV-0051) |
| `block-config-fragment.md` | place a configurable block → `block_config` fragment → config options read-back |
| `campaign-save-restore.md` | save a campaign slot → reload it → production/state equality |
| `database-grid-audit.md` | settings/database grid scroll + cell click → content rows |
| `keybind-rebind-camera.md` | settings Controls → rebind a camera key → held-key pan read-back |
| `pause-save-dialog.md` | paused menu → Save Game dialog → slot write |
| `research-root-switch.md` | research dialog → switch root rail → node list rebuild |
| `settings-gamedata-actions.md` | settings Game Data → export/import/clear actions |

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
path: none | one-line summary of the fast-forward this chain replays   # optional
last_verified: <date> <commit> (<run dir>) | not yet
---
```

Body sections: `## Path (fast-forward)` (optional; navigation only),
`## Steps (under test)` (exact tool + action + params, one step per call, behind
the gate), `## Success signals`, `## Failure modes`, `## Variants`.

## Rules

- **One chain per file.** Parallel agents editing different chains never
  conflict; never rewrite another agent's chain mid-run.
- **Statuses are honest.** `seeded` means compiled from a skill recipe, not yet
  re-run; `verified` requires a run directory and a pid/commit in
  `last_verified`.
- **Steps are exact.** Tool name, action, and the parameter JSON that worked.
  No prose-only sequences, no hardcoded click coordinates — resolve controls
  through `get_global_rect().get_center()`.
- **The path is not evidence.** A `## Path` replays navigation back-to-back per
  leg (one tool-call chain per leg); never capture or diff its steps. Confirm
  the fork once per leg (Godot: structured state; Java: one capture), then
  mirror `## Steps` under the comparison gate. Coordinates in either section
  are re-measured from the current capture.
- **Fork prefixes are shared, paths are per item.** Several ledger items may
  share a navigation prefix; each item still carries its own exact call list
  and replays it independently — never assume a client sits at the fork.
- **Preconditions include the host.** Servers that need a display or another
  process-start environment variable belong in `preconditions`. Both legs share
  display `:10`: the Java helpers take `DISPLAY=:10` per call (run with
  `$PARITY_PY`; `parity-session.sh --run` exports the session display), and the
  Godot editor runs there too (bridge port 6970). Raise the Java window
  (`window_ctl.py raise Mindustry`) before a Java capture.
- **Keep it current.** When a step fails at the same commit, fix the chain in
  the same change that fixes the tooling.
- **Server-tag each chain.** The Godot leg uses open-godot-mcp (`godot_*`
  tools); the Java leg uses the shell helpers (`java_input.py` for input,
  `window_ctl.py` for window ops, `capture_screen.py` for captures). Name the
  scripts in the chain's `tools` list so the next agent picks the right path.
- **Friction is not a chain.** One-off tool errors and their workarounds go to
  the repo-root `open-godot-mcp-learnings.md` / `java-leg-learnings.md`,
  not here.

Consumers: the `/loop` and `/goal` parity sessions driven through the
`parity-eval` skill, and any playtest session. Skills point here; they do not
duplicate the sequences.
