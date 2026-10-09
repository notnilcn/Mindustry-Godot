---
id: ground-zero-resume-godot
title: Godot — resume a saved Ground Zero sector from the planet page (save cycle -> Play > Campaign > chooser > Go)
status: verified
applies_when: >-
  A campaign-resume item needs the Godot leg inside the resumed serpulo:15
  (groundZero) world, entered from the planet page's Go action, with the
  resume structured state asserted.
preconditions:
  - boot-and-identity completed (bridge 6970, `res://scenes/game.tscn`, runtime_connected true, pid stamped).
  - Deterministic resume save: clear the sector slot, restart, run the load+start+exit-save cycle below.
  - The `Select Starting Campaign` chooser page is part of the planet dialog and appears
    when no campaign is preset; both branches are verified.
tools: [godot_health, godot_game, godot_exec, godot_input, godot_screenshot, godot_log]
last_verified: 2026-10-10 PAR-0037 twin (runs/20261009-150509-PAR-0037-twin, game pid 204104, Godot 512702d+round-21 tree)
---

# ground-zero-resume-godot

## Steps

1. **Write the resume save (menu state).** Clear then restart so the facade boots with no slot:

   ```
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\")\nvar before = str(c.list_save_slots())\nvar cleared = c.call(\"clear_planet_campaign_saves\", \"serpulo\")\nreturn {\"pid\": OS.get_process_id(), \"slots_before\": before, \"cleared\": cleared, \"slots_after\": str(c.list_save_slots())}"}}
   godot_game {"action":"stop"}
   godot_game {"action":"play","params":{"scene":"res://scenes/game.tscn"}}
   ```

   After the new pid connects, launch fresh, step, and exit-save in one eval (writes
   `user://saves/sector-serpulo-15.msav` and resets to the menu):

   ```
   godot_exec {"action":"eval","params":{"code":"var h = get_node(\"/root/Spine/SimHost\")\nvar c = get_node(\"/root/Spine/MindCampaign\")\nvar loaded = h.load_sector(\"serpulo\", 15)\nvar started = c.start_sector(\"serpulo\", 15)\nvar tick = h.step(3)\nvar q = c.run_exit_save()\nreturn {\"pid\": OS.get_process_id(), \"loaded\": loaded, \"started\": started, \"tick\": tick, \"q\": str(q), \"exists\": FileAccess.file_exists(\"user://saves/sector-serpulo-15.msav\"), \"slots\": str(c.list_save_slots()), \"core_items\": h.core_items_json()}"}}
   ```

   Expect `loaded/started/q true`, `exists true`, `core_items "{}"`.

2. **UI walk — one click per `execute` call, probe between clicks** (viewport coords;
   press `mouse_motion` -> press true -> press false -> `Input.flush_buffered_events()`):

   - Play `(230,149)` -> probe `MenuGroup` `Submenu.visible`.
   - Campaign `(460,149)` -> probe `dialog_stack == ["planet"]`.
   - Chooser page (`_selected_sector == -1`): Serpulo card `(412,194)` -> probe `_selected_planet == "serpulo"`;
     OK `(499,634)` -> probe `_selected_sector`; it can read `-1` in the same round-trip (race) — re-probe
     in the next call, it self-resolves; never click a `-1`-resolved point.

   ```
   godot_exec {"action":"eval","params":{"code":"var d = get_node_or_null(\"/root/Spine/Ui/UiRoot/DialogLayer/planet\")\nvar sel = int(d.get(\"_selected_sector\")) if d != null else -999\nvar sec = d.call(\"_find_sector\", \"serpulo\", sel) if d != null else {}\nvar act = d.call(\"_sector_action\", sec) if d != null else {}\nreturn {\"pid\": OS.get_process_id(), \"sel\": sel, \"sector_name\": str(sec.get(\"name\", \"\")), \"action\": str(act)}"}}
   ```

   Expect `sel 15`, `sector_name "groundZero"`, action `@sectors.go` (mode `go`). Resolve the Go
   button center from the live tree (verified filter form):

   ```
   godot_exec {"action":"eval","params":{"code":"var d = get_node_or_null(\"/root/Spine/Ui/UiRoot/DialogLayer/planet\")\nvar row = d.get_node_or_null(\"Center/Panel/Layout/Buttons\") if d != null else null\nvar inner = row.find_children(\"*\", \"Button\", true, false).filter(func(b): return b.is_visible_in_tree() and b.get_parent() != row) if row != null else []\nvar b = inner[0] as Control if not inner.is_empty() else null\nvar p = b.get_global_rect().get_center() if b != null else Vector2(-1, -1)\nreturn {\"pid\": OS.get_process_id(), \"x\": int(p.x), \"y\": int(p.y), \"n\": inner.size()}"}}
   ```

   Expect `n 1`, `(576,613)`. Hide the debug overlays with **single-statement** evals
   (`HudGroup/hud/FpsBox`, `HudGroup/performance` -> `visible = false`), click Go, wait ~13 s.

3. **Asserts (pid-stamped; all five in one eval):**

   ```
   godot_exec {"action":"eval","params":{"code":"var c = get_node(\"/root/Spine/MindCampaign\")\nvar h = get_node(\"/root/Spine/SimHost\")\nvar st = get_node_or_null(\"/root/Spine/Ui/UiRoot/HudGroup/hud/WavesStack/WavesMain/Status\")\nvar cam = get_node(\"/root/Spine/World/Camera2D\")\nvar ts = cam.tile_to_screen(129, 55)\nreturn {\"pid\": OS.get_process_id(), \"tick\": h.get_tick(), \"sector_state\": str(c.get_sector_state()), \"core_items_json\": h.core_items_json(), \"status_text\": str(st.text) if st != null else \"(missing)\", \"camera_pos\": str(cam.get(\"position\")), \"tile_to_screen_129_55\": str(ts)}"}}
   ```

   Expect `sector_state` campaign true / planet serpulo / sector 15 / hasCore true / hasSave true,
   `core_items_json "{}"`, status text containing `Obtain:` + `0/15` + `Copper`,
   camera `(1032.0, 440.0)`, `tile_to_screen(129,55) (576.0, 324.0)`.

4. `godot_screenshot {"action":"game"}` -> copy to the run dir; `godot_game stop`; `godot_log errors`.

## Failure modes

- A statement-level `if`/`for` body in an eval times out and pins the loop -> keep evals
  single-statement/ternary; recover with stop+play (save slots survive).
- `get_viewport().gui_get_hovered_control()` from an eval node errors (viewport null); use
  `get_tree().root.gui_get_hovered_control()` instead.
- `_selected_sector == -1` right after the chooser OK is the known view-build race; re-probe once.
- The planet dialog opens on the chooser page while no campaign is preset; the card + OK selects it.
- Hide both debug overlays before captures; an `if`-block hide coincided once with a
  `Parameter "mesh" is null.` engine error, single statements did not.
