---
id: ground-zero-click-mining
title: Ground Zero — click the copper ore to begin mining (player mine order + tick)
status: verified
applies_when: >-
  Driving PAR-0045-family items: the player unit's click-to-mine flow
  (`InputHandler.tryBeginMine`/`MinerComp.update`) in a launched sector.
preconditions:
  - boot-and-identity + ground-zero-wasd-movement path replayed (world live, player unit present).
  - The session worktree is built (tools/build.sh) and the editor relaunched.
tools:
  - godot_exec
  - godot_input
  - godot_screenshot
path: >-
  boot-and-identity -> ground-zero-wasd-movement (launch + optional S/W) ->
  world with the player unit at/near the copper field
last_verified: 2026-10-09 commit 80e2684 (runs/20261009-095320-PAR-0045/, pid 192138)
---

# ground-zero-click-mining

Player mining port: `PlayerInput.mine_tile` (host order) + `update_mining`
(`MinerComp.update`: rate `mine_speed`, threshold `50 + hardness*15` or `50+15`,
single-stack acceptance, range `mine_range.max(40)`), driven from
`mind-gdext/src/input/desktop.rs::try_mine_at` (no-selection left press).
`MindInput` consumes world presses, so the mine order must be set inside
`desktop.rs`, never in `MindSimHost::unhandled_input`.

## Steps (under test)

1. Clear the placement selection so the click means "mine, not place":
   `godot_input {"action":"key","params":{"key":"q","pressed":true}}` then
   `pressed:false`, flush with
   `godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nreturn OS.get_process_id()"}}`.
   Confirm `MindInput.get_input_state_json()` shows `"block":null`.
2. Resolve the ore tile to viewport coordinates:
   `godot_exec {"action":"eval","params":{"code":"var cam = get_node(\"/root/Spine/World/Camera2D\")\nreturn cam.tile_to_screen(137, 53)"}}`
   (the Ground Zero copper tile under the tutorial highlight).
3. Click it with discrete events:
   `godot_input {"action":"mouse_motion","params":{"position":{"x":X,"y":Y},"coords":"viewport"}}`,
   then `mouse_button` press `true` and `false` at the same point; flush with an
   eval.
4. Confirm the order: `godot_exec` eval
   `return str(get_node("/root/Spine/SimHost").call("player_mine_tile"))` ->
   `[137, 53]` (persists through tick validation; invalid tiles are cleared).
5. Bring the unit into range: hold `d` (or the direction of the ore) with
   `godot_input key d pressed:true`, let the game run **unpaused** for the walk
   (the axis upload lives in `MindSimHost::process` and is skipped while
   paused), then `pressed:false`.
6. Read production:
   `godot_exec` eval `return get_node("/root/Spine/SimHost").call("player_mined_count")`
   -> climbs (6.5/tick -> one copper per 10 ticks while in range and not full).

## Success signals

- `player_mine_tile` `[x, y]` after the click and still set while mining.
- `player_mined_count` increasing (0 -> 4 in the recorded run).
- Clearing an active tile with a second click stops mining
  (`tryStopMine` branch).

## Failure modes

- Clicking while a block is selected places a block instead; `Q` first.
- Without a player unit the click falls through to `tileTapped`.
- A paused game never uploads the move axis; use real-time holds for the walk.
- Calling non-`#[func]` Rust methods from GDScript errors and trips
  break-on-error (`player_can_mine` is Rust-only; use `player_mine_tile` /
  `player_mined_count`, both `#[func]`).
- The objective counter/HUD is PAR-0028's scope, so the frame gate stays
  blocked until that lands.
