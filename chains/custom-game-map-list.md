---
id: custom-game-map-list
title: Menu > Play > Custom Game opens the live map grid and its rows carry real registry metadata
status: verified
applies_when: Verifying that Custom Game (and the sibling Editor list) renders rows from the live `MindPreview` map registry with real width/height/author/path instead of the 0x0 `default_map_entries()` fixtures (EV-0039 class), or any map-list regression that must be judged on structured rows rather than pixels.
preconditions:
  - boot-and-identity completed (editor bridge, res://scenes/game.tscn played with the explicit scene param, runtime_connected true, pid stamped).
  - Standalone menu visible and the boot fade hidden (MenuGroup.visible true, OverlayLayer/fade_in hidden).
  - `MindPreview.map_count() > 0` at boot (log line `MindPreview ready (18 maps)`); an empty registry makes both dialogs fall back to the name-only fixtures.
tools: [godot_exec, godot_input, godot_log, godot_screenshot]
last_verified: 2026-10-08 afd380f (runs/20261008-190748-ev0039-custom-game-godot); 2026-10-08 1d8f75b twin loop-1 (runs/20261008-221916-ev0039-twin)
---

# custom-game-map-list

## Steps

1. Clear the log and stamp the runtime together with the boot registry state
   (pid, map count, first registry row). `maps_list()` is the same source both
   dialogs prefer:

   ```
   godot_log {"action":"clear"}
   godot_exec {"action":"eval","params":{"code":"var ui_root = get_node(\"/root/Spine/Ui/UiRoot\")\nvar fade = ui_root.get_node_or_null(\"OverlayLayer/fade_in\")\nvar preview = get_node_or_null(\"/root/Spine/MindPreview\")\nvar rows = preview.call(\"maps_list\") if preview != null else []\nvar first = rows[0] if rows.size() > 0 else {}\nreturn {\"pid\": OS.get_process_id(), \"frames\": Engine.get_process_frames(), \"menu_visible\": ui_root.get_node(\"MenuGroup\").visible, \"fade_visible\": (fade != null and fade.visible), \"map_count\": (int(preview.call(\"map_count\")) if preview != null else -1), \"row_count\": rows.size(), \"first_row\": {\"name\": str(first.get(\"name\", \"\")), \"width\": int(first.get(\"width\", 0)), \"height\": int(first.get(\"height\", 0)), \"author\": str(first.get(\"author\", \"\")), \"path\": str(first.get(\"path\", \"\")), \"custom\": bool(first.get(\"custom\", false)), \"preview\": bool(first.get(\"preview\", false))}}"}}
   ```

   Require `map_count` 18, `row_count` 18, and a first row with real
   width/height/author/path (`Ancient Caldera` 256x256 `Anuke`, path under
   `assets/maps/default/`).

2. Resolve the menu `Play` button (first child of the runtime-built sidebar) and
   arm a pressed counter; it is a code-created, `owner == null` Button (EV-0062
   case):

   ```
   godot_exec {"action":"eval","params":{"code":"var menu = get_node(\"/root/Spine/Ui/UiRoot/MenuGroup/menu\")\nvar b = menu.get_node(\"Sidebar/Buttons\").get_child(0)\nif not b.has_meta(\"click_armed\"):\n\tb.set_meta(\"click_armed\", true)\n\tb.set_meta(\"click_pressed\", 0)\n\tb.pressed.connect(func() -> void: b.set_meta(\"click_pressed\", int(b.get_meta(\"click_pressed\")) + 1))\nvar c = (b as Control).get_global_rect().get_center()\nreturn {\"pid\": OS.get_process_id(), \"path\": str(b.get_path()), \"owned\": b.owner != null, \"center\": {\"x\": int(c.x), \"y\": int(c.y)}, \"pressed\": int(b.get_meta(\"click_pressed\"))}"}}
   ```

3. Click Play with one discrete `godot_input` call per event (never batch):

   ```
   godot_input {"action":"mouse_motion","params":{"position":{"x":CX,"y":CY},"coords":"viewport"}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":CX,"y":CY},"coords":"viewport","pressed":true}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":CX,"y":CY},"coords":"viewport","pressed":false}}
   ```

4. Flush and confirm the submenu opened; resolve the `Custom Game` entry by its
   label text (the Button carries the display string in a `Label` child, not in
   `Button.text`) and return its center:

   ```
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar menu = get_node(\"/root/Spine/Ui/UiRoot/MenuGroup/menu\")\nvar play = menu.get_node(\"Sidebar/Buttons\").get_child(0)\nvar submenu = menu.get_node(\"Submenu\")\nvar cg: Control = null\nfor child in menu.get_node(\"Submenu/Buttons\").get_children():\n\tif child is Button:\n\t\tfor l in child.find_children(\"*\", \"Label\", true, false):\n\t\t\tif str(l.text) == \"Custom Game\":\n\t\t\t\tcg = child\nvar center := {\"x\": -1, \"y\": -1}\nif cg != null:\n\tvar c = cg.get_global_rect().get_center()\n\tcenter = {\"x\": int(c.x), \"y\": int(c.y)}\nreturn {\"pid\": OS.get_process_id(), \"play_pressed\": int(play.get_meta(\"click_pressed\", -1)), \"submenu_visible\": submenu.visible, \"custom_center\": center, \"frames\": Engine.get_process_frames()}"}}
   ```

5. Click `Custom Game` (same three calls as step 3), flush, and confirm the
   dialog stack plus the dialog's own row source. `MindDialog._maps()` prefers
   `MindPreview.maps_list()` and only falls back to the read-model fixtures when
   the registry is empty, so this eval is the player-visible row set:

   ```
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar ui = get_node(\"/root/MindUi\")\nvar ui_root = get_node(\"/root/Spine/Ui/UiRoot\")\nvar dlg = ui_root.get_node_or_null(\"DialogLayer/custom\")\nif dlg == null or not dlg.visible:\n\treturn {\"pid\": OS.get_process_id(), \"stack\": str(ui.call(\"dialog_stack\")), \"custom_visible\": false}\nvar rows: Array = dlg.call(\"_maps\")\nvar project := func(r: Dictionary) -> Dictionary: return {\"name\": str(r.get(\"name\", \"\")), \"width\": int(r.get(\"width\", 0)), \"height\": int(r.get(\"height\", 0)), \"author\": str(r.get(\"author\", \"\")), \"path\": str(r.get(\"path\", \"\"))}\nreturn {\"pid\": OS.get_process_id(), \"stack\": str(ui.call(\"dialog_stack\")), \"custom_visible\": true, \"row_count\": rows.size(), \"all_live\": rows.all(func(r): return int(r.get(\"width\", 0)) > 0 and int(r.get(\"height\", 0)) > 0 and not str(r.get(\"path\", \"\")).is_empty()), \"any_zero_dim\": rows.any(func(r): return int(r.get(\"width\", 0)) == 0 or int(r.get(\"height\", 0)) == 0), \"first_three\": rows.slice(0, 3).map(project), \"frames\": Engine.get_process_frames()}"}}
   ```

6. Optional Editor cross-check in the same run: click the dialog `Back` button
   (label `Back` under `DialogLayer/custom`, flushes to stack `[]`), click the
   menu `Editor` button the same way as step 2-3, then run the step-5 eval
   against `DialogLayer/editor_maps` (same `_maps()` contract).

7. `godot_screenshot game` for the audit frame and `godot_log errors` for the
   clean-run check.

## Success signals

- Step 1: `map_count == 18`, first registry row real (`Ancient Caldera`,
  256x256, `Anuke`, path `<repo>/assets/maps/default/caldera.msav`).
- Step 4: `play_pressed` increments to 1, `submenu_visible == true`,
  labels `["", "Campaign", "Join Game", "Custom Game", "Load Game"]`.
- Step 5: `stack == ["custom"]`, `row_count == 18`, `all_live == true`,
  `any_zero_dim == false`; first rows `Ancient Caldera 256x256/Anuke`,
  `Archipelago 500x500`, `Debris Field 400x400` with
  `assets/maps/default/*.msav` paths; the grid labels include those map names.
- `godot_log errors` has no entry attributable to the run (the
  `[W] previous launch may have crashed (leftover launchid.dat)` marker from
  `godot_game stop` is expected, see learnings).

## Verified twin run (loop-1, 2026-10-08, 1d8f75b, EV-0039)

Steps 1-5 re-ran at pid 99383 (window 1152x648): `map_count()==18`, Custom Game
`stack ["custom"]` with `all_live==true`/`any_zero_dim==false`; step 6 Editor
cross-check gave `stack ["editor_maps"]`, 18 live rows; clicking the
`Archipelago` card pushed `stack ["custom","map_play"]` (the dialog renders the
raw key `@map.play` as its title and has no map preview/High Score — separate
symptom, not part of this chain's pass criteria). A second fresh boot (pid
104609) reproduced `map_count()==18` with boot log `[I] MindPreview ready (18
maps)`. The Java reference was run at the same 1152x648 and showed the same 18
built-ins with real previews (its grid order is the `MapPriority.recent`
comparator, i.e. name order for built-ins, vs the registry's PvP-last order
here). Content was cross-checked against the built-in MSAV headers with
`client/bin/rust/debug/mind-headless maps list --dir assets/maps/default --json`:
18/18 match name/width/height/author, `canyon.msav` excluded on both sides (not
in upstream `Maps.defaultMapNames`). Evidence:
`runs/20261008-221916-ev0039-twin/` (`godot/state-01-registry.json` …
`state-04-fresh-boot.json`, `java/builtin-msav-metadata.json`).

## Failure modes

- **Fixture rows.** If `map_count() == 0`, `_maps()` silently returns the
  `default_map_entries()` fixtures (18 rows, width=0/height=0, no author/path):
  assert the registry before judging the dialog. The registry source is the
  resolved assets dir's `maps/default/*.msav`; a missing pack or the wrong
  worktree leaves it empty.
- **Blocking read model.** Do not call `MindUi.campaign_views()` /
  `campaign_views_json` from an eval to inspect the dialog: that path blocks the
  eval (and previously the debugger) and is only the empty-registry fallback in
  any case. `_maps()` is the direct source.
- **Click swallow.** The menu/submenu Buttons are code-created (`owner == null`);
  a press that only hovers means the EV-0062 input-capture regression. Always
  flush with `Input.flush_buffered_events()` and read the stack back.
- **Preview field.** Live rows carry `preview: false` until preview PNGs are
  generated (plan-19 pipeline); the tile draws the documented placeholder. Judge
  the registry metadata, not the preview bitmap.
- **Row order.** The live registry sorts by plain name with PvP maps last
  (`Ancient Caldera` first, `Archipelago` second); resolve rows by name, not by
  index.
