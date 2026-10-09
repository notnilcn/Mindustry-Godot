---
id: campaign-live-views-capture
title: Planet dialog live sector state → capture → panel/read-model update
status: verified
applies_when: Verifying that the planet dialog derives owned/captured/attacked sector state from the live Campaign (not the vanilla fixture), and that a sector capture flips the panel and the read models.
preconditions:
  - boot-and-identity completed (game in the main menu, runtime connected, pid stamped).
  - The loop user data may already hold a sector save (`load_sector` logs `resuming serpulo:15`); record the pre-capture flags either way — the live-vs-fixture check also holds because the vanilla fixture seeds groundZero `captured=true/attacked=false`.
tools: [godot_exec, godot_input, godot_log, godot_screenshot]
last_verified: 2026-10-08 1d8f75b (runs/20261008-191650-ev0051-campaign-live-views-godot); 2026-10-08 1d8f75b twin (runs/20261008-223232-ev0051-campaign-live-views-twin)
---

# campaign-live-views-capture

## Steps

1. Open the planet view. Menu Play is `MenuGroup/menu/Sidebar/Buttons` child 0;
   resolve its center and click it via `ui-control-click` (discrete
   `mouse_motion` + press + release, then a separate eval calls
   `Input.flush_buffered_events()`). After the flush the live submenu is
   `MenuGroup/menu/Submenu/Buttons`; **child 0 is a spacer Control**, Campaign
   is child 1 (its label text "Campaign" sits at `child.get_child(0).get_child(1)`).
   Click child 1.

2. First-run chooser (`_show_select`): the two cards live in
   `DialogLayer/planet/Center/Panel/Layout/Body/Content` → child 0 (MindTable)
   → child 0 (HBox row) → child 0/1; the OK button is
   `.../Center/Panel/Layout/Buttons` child 0 and starts `disabled`. Click the
   Serpulo card, flush, click OK.

3. Read the dialog's own model (not a screenshot):

   ```
   godot_exec {"action":"eval","params":{"code":"var dlg = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/planet\")\nvar views = dlg.call(\"campaign_views\")\nvar gz = views[\"sectors\"].filter(func(s): return str(s.get(\"preset\", \"\")) == \"groundZero\")\nvar row = (gz[0] if not gz.is_empty() else {})\nvar action = dlg.call(\"_sector_action\", row)\nreturn {\"pid\": OS.get_process_id(), \"gz\": row, \"action\": action, \"stack\": str(get_node(\"/root/MindUi\").call(\"dialog_stack\"))}"}}
   ```

4. Capture through the facade (three separate evals; `load_sector` is the heavy
   one):

   ```
   godot_exec {"action":"eval","params":{"code":"return get_node(\"/root/Spine/SimHost\").load_sector(\"serpulo\", 15)"}}
   godot_exec {"action":"eval","params":{"code":"return get_node(\"/root/Spine/MindCampaign\").start_sector_with_loadout(\"serpulo\", 15, \"\")"}}
   godot_exec {"action":"eval","params":{"code":"return get_node(\"/root/Spine/MindCampaign\").call(\"capture_sector\")"}}
   ```

5. Reopen the planet dialog: `MindUi.open_dialog("planet", "{}")` —
   `_selected_planet` persists, so `shown()` goes straight to the planet view
   and refreshes the live views. The capture edge opens the `restart`
   (game-over/captured) dialog on top; close it with
   `MindUi.close_dialog("restart")` before reading the planet panel.

6. Re-read step 3 plus the action button's visible label:

   ```
   godot_exec {"action":"eval","params":{"code":"var dlg = get_node(\"/root/Spine/Ui/UiRoot/DialogLayer/planet\")\nvar views = dlg.call(\"campaign_views\")\nvar gz = views[\"sectors\"].filter(func(s): return str(s.get(\"preset\", \"\")) == \"groundZero\")\nvar panel = dlg.get_node(\"Center/Panel/Layout/Buttons\").get_child(2)\nvar col = panel.get_child(0)\nvar action = col.get_child(col.get_child_count() - 1)\nreturn {\"pid\": OS.get_process_id(), \"gz\": gz[0], \"labels\": action.find_children(\"*\", \"Label\", true, false).map(func(l): return l.text)}"}}
   ```

## Success signals

- `dialog_stack` is `["planet"]` with selected sector 15 (`groundZero`); the
  before/after `campaign_views()` rows differ on `captured` (false→true) and
  `attacked`/`frozen` (true→false), and the panel action mode flips `go`→`resume`
  with visible label `Resume`.
- `MindUi.campaign_views_json("serpulo")` agrees with the dialog's cache.
- `godot_log errors` is empty for the run.
- Twin re-run 2026-10-08 (pid 116138, `runs/20261008-223232-ev0051-campaign-live-views-twin`):
  at step 3 the live rows discriminate from the fixture on **two** sectors
  (`saltFlats` locked=true/captured=false, fixture captured=true); the dialog's
  `campaign_views()` re-projects on every call, but the built side-panel widget
  is a snapshot — reopen with `MindUi.open_dialog("planet","{}")` to refresh the
  action button label.

## Failure modes

- An eval body that errors reports as a 15 s TIMEOUT and trips the editor
  break-on-error. `godot_log errors` names the real message (e.g. `Invalid
  access to property or key 'text' on a base object of type 'Control'` when
  child 0 of the submenu — the spacer — is treated as a Button). Recover with
  `godot_game stop` + `play`; `resume` re-breaks.
- Do not hardcode the Play/Campaign/card/OK coordinates; resolve centers with
  `get_global_rect().get_center()` and re-read after every flush.
- `start_sector_with_loadout` on a sector with a save takes the resume path
  (`play_sector`); `capture_sector` still flips `wasCaptured` and the views.
- Sessions without OCR/vision: judge on the dialog JSON and the action button's
  child Label texts, not screenshots (frame_diff also needs PIL/numpy, see
  `open-godot-mcp-learnings.md`).
