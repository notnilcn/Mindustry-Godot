---
id: research-root-switch
title: Research dialog — switch tech-tree roots and assert the rail stays vertical + the node table repopulates
status: verified
applies_when: Verifying the research dialog root rail after a root switch (EV-0042); works on a fresh boot with the fixture roots (`serpulo`, `<null>`) and does not need a launched campaign.
preconditions:
  - boot-and-identity completed; `runtime_connected: true`; the dialog builds its rail from `campaign_section("research")` (live campaign views or the MindUi fixture).
  - Rail order on the fixture is `techtree.serpulo` then `techtree.<null>`; with a live campaign it is the TechTree.roots order.
tools: [godot_exec, godot_input, godot_log]
last_verified: 2026-10-08 417d400 (runs/l2-20261008-201920-ev0042-research-rail-godot)
---

# research-root-switch

## Steps

1. Open the dialog (the context argument is a JSON string):

   ```
   godot_exec {"action":"eval","params":{"code":"var ui = get_node(\"/root/MindUi\")\nreturn ui.open_dialog(\"research\", JSON.stringify({\"planet\": \"serpulo\"}))"}}
   ```

2. Read the rail rects, pressed flags and node count (global coords; at 1152x648 the fixture rail is (10,40) and (10,78)):

   ```
   godot_exec {"action":"eval","params":{"code":"var d = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/research\")\nvar c0 = d._roots.get_child(0)\nvar c1 = d._roots.get_child(1)\nvar r0 = (c0 as Control).get_global_rect()\nvar r1 = (c1 as Control).get_global_rect()\nreturn {\"pid\": OS.get_process_id(), \"root_name\": d._root_name, \"rail0\": [int(r0.position.x), int(r0.position.y), int(r0.size.x), int(r0.size.y)], \"rail1\": [int(r1.position.x), int(r1.position.y), int(r1.size.x), int(r1.size.y)], \"pressed\": [c0.button_pressed, c1.button_pressed], \"node_rows\": d._nodes.get_children().size()}"}}
   ```

3. Switch to the second root and read the final geometry in the same eval — this is the exact EV-0042 repro (the rebuild must place the rail synchronously):

   ```
   godot_exec {"action":"eval","params":{"code":"var d = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/research\")\nvar b1 = d._roots.get_child(1)\nb1.emit_signal(\"pressed\")\nvar c0 = d._roots.get_child(0)\nvar c1 = d._roots.get_child(1)\nvar r0 = (c0 as Control).get_global_rect()\nvar r1 = (c1 as Control).get_global_rect()\nvar n0 = (d._nodes.get_child(0) as Control).get_global_rect()\nreturn {\"pid\": OS.get_process_id(), \"root_name\": d._root_name, \"rail0\": [int(r0.position.x), int(r0.position.y), int(r0.size.x), int(r0.size.y)], \"rail1\": [int(r1.position.x), int(r1.position.y), int(r1.size.x), int(r1.size.y)], \"pressed\": [c0.button_pressed, c1.button_pressed], \"node_rows\": d._nodes.get_children().size(), \"node_row0\": [int(n0.position.x), int(n0.position.y)]}"}}
   ```

4. Real-input switch (optional, stronger): resolve the target center in one eval, then discrete press/release + flush (a `sequence` call times out here):

   ```
   godot_exec {"action":"eval","params":{"code":"var d = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/research\")\nvar p = (d._roots.get_child(1) as Control).get_global_rect().get_center()\nreturn {\"x\": int(p.x), \"y\": int(p.y)}"}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":true}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":false}}
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar d = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/research\")\nreturn {\"pid\": OS.get_process_id(), \"root_name\": d._root_name, \"node_rows\": d._nodes.get_children().size()}"}}
   ```

## Success signals

- Rail rects stay a vertical list after the switch (fixture: (10,40) and (10,78), 176x34 each); the clicked root's `button_pressed` is true and the other stays at its own y so it remains clickable.
- `node_rows` matches the selected root (fixture: 1 for serpulo, 222 for `<null>`), and the first rows sit at distinct y (40, 78, …) — not stacked.
- `godot_log errors` is empty; clear the log after any `move_to_foreground()` nudge, which lands as an error-level deprecation entry in Godot 4.7.

## Failure modes

- Before 417d400 both buttons stack at (8,38) after the switch and stay there across frames; the queued `Container.queue_sort` never lays the rebuilt nested tables out, so `_rebuild_roots`/`_rebuild_nodes` force `sort_now()` + deferred `sort_now()`.
- Reading the rects after a root click done with `godot_input` needs `Input.flush_buffered_events()`; without it the click still sits in the accumulated-input buffer.
