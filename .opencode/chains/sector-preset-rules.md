---
id: sector-preset-rules
title: Launch a campaign sector and assert the applied preset rules (captureWave -> winWave)
status: verified
applies_when: Verifying World.setSectorRules / Control.playNewSector preset-rule application (EV-0049 family); reading MindCampaign rules JSON after a launch.
preconditions:
  - boot-and-identity completed (game.tscn playing, runtime connected, pid stamped).
  - "Sector preset exists and is unlocked. Upstream `presets.groundZero = 170` (../Mindustry/core/assets/planets/serpulo.json); the port registry stores the same preset at serpulo sector 15 with captureWave=10, alwaysUnlocked, addStartingItems, noLighting (client/rust/mind-core/src/content/registries/sectors.rs:324-337). The id is an internal remap; the player-visible rules are what the assertions cover."
tools: [godot_health, godot_game, godot_exec, godot_log]
last_verified: 2026-10-08 761a483 (runs/20261008-020931-ev0049-campaign-preset-rules-twin, runs/20261007-152746-ev0049-campaign-preset-rules-godot)
---

# sector-preset-rules

## Steps

1. Play the entry scene explicitly, then `godot_log clear` and pid-stamp:

   ```
   godot_game {"action":"play","params":{"scene":"res://scenes/game.tscn"}}
   godot_log {"action":"clear"}
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nreturn {\"pid\": OS.get_process_id(), \"project\": ProjectSettings.globalize_path(\"res://\"), \"tick\": h.get_tick()}"}}
   ```

2. Load the preset map on the sim host:

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nreturn {\"pid\": OS.get_process_id(), \"loaded\": h.load_sector(\"serpulo\", 15), \"tick\": h.get_tick()}"}}
   ```

   Expect `loaded: true`, tick reset to 0, log line `load_sector: loaded preset map `serpulo/groundZero` (256x256)`.

3. Start the campaign sector on `MindCampaign` (not `SimHost`):

   ```
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\")\nreturn {\"pid\": OS.get_process_id(), \"started\": c.start_sector(\"serpulo\", 15), \"sector_state\": c.get_sector_state()}"}}
   ```

   Expect `started: true`, `hasSave: false` (fresh-launch branch), `planet: "serpulo"`, `sector: 15`.

4. Assert the applied preset rules from the rules JSON:

   ```
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\")\nvar raw = str(c.get_rules_json())\nvar d = JSON.parse_string(raw)\nreturn {\"pid\": OS.get_process_id(), \"waves\": d.get(\"waves\"), \"winWave\": d.get(\"winWave\"), \"attackMode\": d.get(\"attackMode\"), \"planet\": d.get(\"planet\"), \"sector\": d.get(\"sector\"), \"lighting\": d.get(\"lighting\")}"}}
   ```

   Expected for serpulo 15: `waves=true`, `winWave=10` (preset captureWave -> winWave),
   `attackMode=false`, `sector="serpulo-15"`, `planet="serpulo"`, `lighting=false`
   (preset noLighting). Re-read after any panic to confirm stability.

5. Check `godot_log errors`; recover a stalled frame loop with `godot_game stop` + `play`
   (re-stamp the pid) before trusting further calls.

## Success signals

- `waves=true` and `winWave=<preset captureWave>`; `attackMode=false` for a capture sector.
- Values identical across a second read and across a fresh stop/play instance.

## Failure modes

- `waves=false, winWave=0` after launch is the pre-`9f5f90d` bug (`apply_sector_preset_rules`
  not called from `play_new_sector`); check `git merge-base --is-ancestor 9f5f90d HEAD`.
- On main/loop-1 the bare launch has `hasCore=false` (EV-0037: no core materialization), which
  drives the zero-core game-over path into the EV-0061 `MindUi::hud_set_visible` panic and stalls
  the frame loop. `godot_exec` state evals still answer through the debugger channel and the rules
  values are unaffected; do not attribute the panic to the rules path.
- Keep eval bodies state-only. A `FileAccess` write inside an eval timed out (15 s) and wrote
  nothing while the loop was stalled; write artifacts from the eval response host-side instead.
