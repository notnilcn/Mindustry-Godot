---
id: command-mode-hold-vs-tap
title: Command-mode key (Shift) twin run — Java hold affordance vs Godot input state, both branches
status: verified
applies_when: >-
  Twin-evaluating EV-0044-family findings: default RTS command-mode reachability
  from the keyboard, hold-vs-tap semantics, or the `commandmodehold` setting.
preconditions:
  - Loop manifest exported; Java jar built; computer-mcp registered with DISPLAY set at opencode start.
  - One X11 client at a time on the loop display (Xvfb has no WM).
  - Godot leg after boot-and-identity (res://scenes/game.tscn played with the explicit scene param, runtime_connected true, pid stamped); res://scenarios/input_controls is absent, the default empty world is the stage.
tools:
  - computer-mcp_mouse_move
  - computer-mcp_click
  - computer-mcp_key_down
  - computer-mcp_key_up
  - godot_exec
  - godot_input
  - godot_game
  - godot_log
last_verified: 2026-10-08 986cda3 (runs/l2-20261008-150205-input_controls-twin)
---

# command-mode-hold-vs-tap

Reference semantics: `commandmodehold` defaults **true**
(`SettingsMenuDialog.java:416`), so `commandMode = input.keyDown(Binding.commandMode)`
(`DesktopInput.java:303-304`) — on only while the key is held, off on release. The
tap/toggle branch (`DesktopInput.java:305-306`) runs only when the setting is false.
The gate (`DesktopInput.java:300-302`) requires `block == null`, no field/dialog, and
no live player unit with `canBoost` sharing the key. A fresh campaign launch controls
no unit, so the boost guard is inactive and the gate passes.

## Java leg (computer-mcp, 900x700+190+10 at loop-2)

1. Launch via the loop wrapper (`chains/java-reference-leg.md` step 1) and poll the
   log for `Total time to load`. On a fresh data dir the campaign route includes the
   first-run dialog: Play (405,185) -> Campaign (647,188) -> `Select Starting
   Campaign` Serpulo (477,310) -> OK (640,673) -> planet `Ground Zero` selected ->
   Launch (640,677).
2. Confirm the probe state: no controlled unit (WASD pans the camera), empty block
   catalog pre-research (no placement block selected), no dialog/field. The tutorial
   hint is a label with a Skip button and does not close the gate.
3. Capture two baselines ~3 s apart, then hold the key with
   `computer-mcp_key_down {"key":"shift"}`, capture twice while held, release with
   `computer-mcp_key_up {"key":"shift"}`, capture after ~3 s.
4. Measure with `frame_diff.py`; the command-mode affordance is the bottom panel
   swapping from the block catalog to `Command Mode` / `[no units]`
   (`PlacementFragment.java:447-475`, bundle `commandmode.name`) plus the golden
   diamond command cursor. Verified 986cda3: noise 0.01 mean-abs, baseline-vs-held
   12.2 mean-abs / 10.07 % changed, held-vs-held 0.01, after-release-vs-baseline 0.0.
   A latched toggle would stay on after release — it does not.

## Godot leg (open-godot-mcp)

1. `godot_log clear`; hide the menu
   (`godot_exec call /root/Spine/Ui/UiRoot set_menu_visible [false]`); baseline
   `get_input_state_json()`.
2. `godot_input key Q down+up`, then an eval with `Input.flush_buffered_events()`
   reading `block`/`action_count`/`last_action` (expect `block=null`,
   `clear_building`).
3. `godot_input key Shift down`; in a separate eval read
   `get_input_state_json()` while held (default: `command_mode=true`,
   `action_count` +1, `last_action=command_mode`). Release; separate eval must read
   `false`.
4. Tap branch: `godot_exec call /root/MindUi settings_set ["commandmodehold", false]`,
   press (toggles on), release (ignored), press (toggles off), then restore
   `true` and re-verify one hold cycle. Verified 986cda3 (pid 197940): 3->4 on hold,
   4->5 on release, tap 6/6/7, restore 8/9; API `toggle_command_mode` 10/11.
5. Persist the samples for the run dir by appending JSON lines from the eval with
   `FileAccess.open("user://ev0044-evidence.jsonl", FileAccess.READ_WRITE)` +
   `seek_end()`/`store_line()`, then copy the file out of
   `<worktree>/.opencode/loops/run/loop-2/xdg-data/godot/app_userdata/Mindustry-Godot/`.

## Success signals

- Java: held frame shows `Command Mode`/`[no units]` + diamond cursor and differs
  from baseline by a large changed fraction; release returns to the baseline.
- Godot: held `command_mode=true` with `last_action=command_mode`, release `false`;
  with the setting off, press toggles, release is ignored; setting restored and hold
  re-verified.
- All Godot evals share one pid; `godot_log errors` shows no input-probe errors.

## Failure modes

- The Godot game runs **embedded** in the editor: `DisplayServer.window_set_size`
  is refused (`Embedded window can't be resized.`). Do not match the Java window
  size by resizing; compare structured state and say so.
- `godot_exec` eval bodies containing `await` without the eval `await` param error
  with `Trying to call an async function without "await".` and stall the frame loop;
  `godot_game stop` + `play` recovers (re-stamp the pid).
- Sampling only after release reads `false` even when the path works — sample while
  held in its own round-trip.
- A toggle-on-release observation means a stale extension is loaded: check
  `grep libmind_gdext /proc/<pid>/maps` + sha256 and restart the editor if the
  binary predates the fix.
