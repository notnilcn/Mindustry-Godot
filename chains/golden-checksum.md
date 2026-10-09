---
id: golden-checksum
title: Load a scenario, step 60 ticks in Playing, compare the committed checksum
status: seeded
applies_when: Before trusting any engine state; after a build; when a headless golden must match in-engine.
preconditions:
  - boot-and-identity completed, runtime connected, game in `Playing` (not paused).
tools: [godot_exec]
last_verified: not yet (seeded from .opencode/skills/playtest/SKILL.md §Recipes 2 and 5)
---

# golden-checksum

## Steps

1. Load and step in ONE loop-free eval (the fixed 60 Hz pump must not advance
   between the two calls):

   ```
   godot_exec {"action":"eval","params":{"code":"var host = get_node(\"/root/Spine/SimHost\")\nvar loaded = host.load_scenario(\"res://scenarios/spine_place_break.json\")\nvar tick = host.step(60)\nreturn {\"pid\": OS.get_process_id(), \"loaded\": loaded, \"tick\": tick, \"checksum\": str(host.get_checksum())}"}}
   ```

   Require `loaded: true`, `tick: 60`, and the checksum recorded in
   `scenarios/spine_place_break.json` (canonical `a1a7b96167c9718d`).

2. Pause after the golden state is verified:

   ```
   godot_exec {"action":"call","params":{"node_path":"/root/Spine/SimHost","method":"set_paused","args":[true]}}
   ```

3. For any other scenario, resolve the golden from
   `parity/scenario_catalog.json` + `parity/golden_manifest.json` and use the
   same load → step → compare order.

## Success signals

- `loaded: true`, `tick: 60`, checksum equals the committed golden.
- After `set_paused(true)`, `step(n)` raises the tick by exactly `n`.

## Failure modes

- Pausing before the step hashes the paused phase and yields the wrong
  checksum. Load and step in Playing first.
- `load_scenario` replaces the whole world/player: re-establish camera, pid and
  tick baselines before interpreting later results.
- Eval bodies with `for`/`while` time out; split heavy expressions.
- A `null` result with `ok: true` usually means the body errored mid-script.
