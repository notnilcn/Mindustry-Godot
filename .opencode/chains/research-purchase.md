---
id: research-purchase
title: Campaign research — open the dialog, select a root, purchase a locked node
status: verified
applies_when: Verifying the ResearchDialog purchase flow (canSpend -> spend -> unlock + checkAutoUnlocks) against a launched campaign sector with a stocked core.
preconditions:
  - boot-and-identity completed; a sector launched with a live core and items (enter-campaign facade variant, groundZero sector 15 + loadout).
  - Dialog rows come from the MindUi vanilla fixture while MindCampaign lacks campaign_views_json (EV-0051): root nodes carry root=null and sit behind the broken `techtree.<null>` rail entry.
  - `godot_input sequence` times out here; use discrete mouse_button press/release plus `Input.flush_buffered_events()`.
tools: [godot_exec, godot_input, godot_log]
last_verified: 2026-10-08 9efd068 (runs/l2-20261008-012103-ev0052-research-purchase-godot)
---

# research-purchase

## Steps

1. Open the dialog (the context argument is a JSON string):

   ```
   godot_exec {"action":"eval","params":{"code":"var ui = get_node(\"/root/MindUi\")\nreturn ui.open_dialog(\"research\", JSON.stringify({\"planet\": \"serpulo\"}))"}}
   ```

2. Resolve the second root-rail button (the locked roots are the `root=null`
   fixture rows behind it) and click it with discrete input + flush:

   ```
   godot_exec {"action":"eval","params":{"code":"var d = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/research\")\nvar b = d._roots.get_children()[1]\nvar p = (b as Control).get_global_rect().get_center()\nreturn {\"x\": int(p.x), \"y\": int(p.y), \"text\": b.text}"}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":true}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":false}}
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar d = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/research\")\nreturn {\"root_name\": d._root_name, \"rows\": d._nodes.get_children().size()}"}}
   ```

3. `_select_root` rebuilds rows without a layout pass and the MindTable queued
   sort does not run, so all rows stay overlapped at y=38. Force it before
   resolving centers:

   ```
   godot_exec {"action":"eval","params":{"code":"var d = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/research\")\nd._nodes.sort_now()\nreturn true"}}
   ```

4. List the enabled (locked+ready) node buttons; only rows inside the scroll
   viewport are clickable:

   ```
   godot_exec {"action":"eval","params":{"code":"var d = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/research\")\nvar out = []\nfor r in d._nodes.get_children():\n\tfor c in r.get_children():\n\t\tif c is Button and not c.disabled and not str(c.text).is_empty():\n\t\t\tvar p = (c as Control).get_global_rect().get_center()\n\t\t\tout.append({\"text\": str(c.text), \"id\": int(c.get_instance_id()), \"x\": int(p.x), \"y\": int(p.y)})\nreturn {\"enabled\": out}"}}
   ```

5. Hover-confirm the target, press+release it, flush, and assert:

   ```
   godot_input {"action":"mouse_motion","params":{"coords":"viewport","position":{"x":X,"y":Y}}}
   godot_exec {"action":"eval","params":{"code":"var hov = get_tree().root.gui_get_hovered_control()\nreturn {\"hover_text\": str(hov.get(\"text\")) if hov != null else null, \"hover_id\": hov.get_instance_id() if hov != null else null}"}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":true}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":false}}
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar c = get_node(\"/root/Spine/MindCampaign\")\nvar h = get_node(\"/root/Spine/SimHost\")\nreturn {\"tech\": {\"unlocked\": int(c.get_tech_state().get(\"unlocked\"))}, \"core_items\": str(h.core_items_json())}"}}
   ```

## Success signals

- `get_tech_state().unlocked` increases; `core_items_json()` debits exactly the
  node's research cost; `godot_log` gains `research auto-unlocked N content`;
  the purchased row's button instance id changes (row rebuild).

## Failure modes

- The dialog's `Body` can read 0-height if measured in the same frame as
  `open_dialog`; re-measure after a frame before resolving coordinates.
- After the purchase a row may still show locked/enabled because rows are the
  frozen fixture (EV-0051); `get_tech_state().unlocked` is authoritative.
- The `@unlocked` toast is transient; capture it in the same eval as the click
  if it matters.
- An eval error trips break-on-error and freezes the frame loop; recover with
  `godot_game stop` + `play` (`godot_game resume` did not revive it here).
