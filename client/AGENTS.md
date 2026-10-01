# AGENTS.md — `client/` (Godot 4.7 project)

The Godot side of Mindustry-Godot. **Behavior lives in Rust** (`client/rust/`, GDExtension via `mind-gdext`); this tree owns scenes, GDScript UI, the project file and third-party addons. Read the root [`AGENTS.md`](../AGENTS.md) first; this file is the client-specific map.

## Layout

| Path | Contents |
|---|---|
| `project.godot` | Main scene `res://scenes/spine.tscn`; renderer `mobile` (locked OD-R2); MCP autoload + editor plugin. |
| `mind.gdextension` | Maps `res://bin/rust/{debug,release}/libmind_gdext.{so,dll,dylib}` to the Rust cdylib. Build via `tools/build.sh`. |
| `scenes/spine.tscn` | P0 spine rig: `/root/Spine/{SimHost, World/TileGrid, World/Camera2D, Ui/StateInspector}`. |
| `scenes/ui/state_inspector.tscn`, `ui/state_inspector.gd` | Read-only overlay; polls `SimHost.get_state_json()` every 250 ms and on `state_changed`. |
| `rust/` | Cargo workspace: `mind-core` (Godot-free sim), `mind-headless`, `mind-gdext`, `mind-stdb`. Never build it from inside the editor. |
| `bin/`, `.godot/`, `scenarios/` | Build output / editor cache / generated scenario mirror — all gitignored. |
| `addons/open_godot_mcp/` | Editor bridge + runtime autoload for MCP verification; don't edit unless working on the addon. |
| `addons/blastbullets2d/`, `addons/phantom_camera/` | Retained but unused at P0 (OD-R12); plans 10/15 evaluate them. |

## Conventions

- **`.tscn`-first** (`HIGH_LEVEL_PLAN.md` §6.6): static UI, world scaffolding, autoloads and debug views belong in scenes so a human can open and navigate the project in the editor. Rust owns behavior, not static scene construction.
- Any node created in code needs a `# code-instantiated: <specific reason>` comment at the site ("spawned at runtime", "pooled/high-churn", "count driven by server rows" — not "easier in code").
- GDScript is UI-only: it reads `MindSimHost` (JSON/signals), never mutates sim state.
- Generated STDB bindings (`rust/mind-stdb/src/module_bindings/`) are **never hand-edited**; regenerate with `server/build.sh` (drift gate `--check`).
- New files LF/UTF-8; cite ported Mindustry sources in headers.

## Build, run, verify (WSL2 Ubuntu)

```bash
tools/build.sh                                  # mind-gdext + mind-headless -> client/bin/rust, syncs scenarios
godot4 --path client                            # opens res://scenes/spine.tscn
bash tools/godot.sh --headless --editor --quit --path client   # import/parse check
tools/ci.sh                                     # full gate (audits Rust + this tree)
tools/mcp-smoke.sh                              # in-engine spine smoke (needs the editor running)
```

In-engine verification goes through open-godot-mcp with **pid-stamped** evals; the repo skill [`.opencode/skills/playtest/SKILL.md`](../.opencode/skills/playtest/SKILL.md) has the launch flow, node map and recipes. Full plan: [`00_FOUNDATION_IMPLEMENTATION_PLAN.md`](../00_FOUNDATION_IMPLEMENTATION_PLAN.md) §3.4–§3.5, §7c.
