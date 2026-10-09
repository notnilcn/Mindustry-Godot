---
id: seed-survey
title: Seed sweep — boot both clients, enumerate a surface, one ledger item per element, classify with one action pair
status: seeded
applies_when: >-
  The ledger needs seeding or extension from live surface: seeding.last_sweep
  is null, a run revealed a fork with no items, or the operator asks for a seed
  sweep.
preconditions:
  - Both clients launchable on display :10 (java-reference-leg launch steps; boot-and-identity Godot steps).
  - Java window normalized to 1152x648; the Godot game is the embedded 1152x648 viewport.
  - parity-ledger.json readable; note the next PAR-#### id and the current seeding.last_sweep/frontier.
  - One surface (or one depth level) per sweep — do not chase the whole tree in one run.
tools: [computer-mcp_mouse_move, computer-mcp_click, computer-mcp_key_down, computer-mcp_key_up, computer-mcp_screenshot, godot_health, godot_editor_read, godot_editor_edit, godot_game, godot_exec, godot_input, godot_screenshot, godot_log]
path: none — this chain is the root recipe; items it finds store their own boot-replay path.
last_verified: 2026-10-09 runs/20261009-052658-seed-campaign (campaign surface: boot → Play > Campaign → first-run chooser → planet page; both legs; first mismatch recorded PAR-0015; see chains/campaign-planet-page-seed.md)
---

# seed-survey

A seed sweep explores, classifies, records — and never fixes. One surface per
sweep, both legs, one action pair per element. Every item it creates starts with
`status: seeded` and the boot replay as its `path`; the ones it classifies in
the same pass end as `verified` (`mode: seed`) or `mismatch` with the exact
calls.

## Steps

1. Create the sweep run and launch both legs:

   ```bash
   RUN="runs/$(date -u +%Y%m%d-%H%M%S)-seed-<area>"
   mkdir -p "$RUN/seed"
   ```

   Follow `java-reference-leg` §Steps 1–3 (detached launch, readiness line,
   unmaximize/resize/raise/capture) and `boot-and-identity` §Steps 1–6 (bridge, identity,
   `game.tscn`, play, pid stamp). Stamp both pids in `$RUN/run.json` and record
   the normalized Java geometry.

2. Capture the root/surface pair and gate it (this is the surface-render check
   and closes the surface item, e.g. PAR-0001 for boot):

   ```
   capture_screen.py --window-title Mindustry --output "$RUN/seed/step-01-<surface>-java.png"
   godot_screenshot {"action":"game"}    # copy the returned file to "$RUN/seed/step-01-<surface>-godot.png"
   frame_diff.py --java "$RUN/seed/step-01-<surface>-java.png" --godot "$RUN/seed/step-01-<surface>-godot.png" --out-json "$RUN/seed/diff-01.json"
   ```

3. Enumerate the surface on both sides.

   **Java** — the capture *is* the enumeration: read the image, list each
   label/row/icon with its capture-relative center. Screen click coordinate =
   the `region` origin from the capture report + the offset in the image. No
   OCR: labels come from reading the PNG, and a hover probe (move the pointer,
   capture, diff) locates controls you cannot place. Re-measure after any
   resize; never reuse coordinates from an older run.

   **Godot** — list the live controls in one loop-free eval and resolve each
   center in viewport coordinates (swap the class for `Range`, `LineEdit`, ...):

   ```
   godot_exec {"action":"eval","params":{"code":"var root = get_node(\"/root/Spine/Ui/UiRoot\")\nvar rows = root.find_children(\"*\", \"Button\", true, false).filter(func(b): return b.is_visible_in_tree()).map(func(b): return {\"path\": str(b.get_path()), \"text\": str((b as BaseButton).text), \"center\": (b as Control).get_global_rect().get_center()})\nreturn {\"pid\": OS.get_process_id(), \"buttons\": rows}"}}
   ```

   Match the two lists by label and semantics. An element present on one side
   only is a finding, not a rounding error.

4. Write one ledger item per element/action before classifying it. Fields:
   `id` (next `PAR-####`), player-visible `title`, `surface`, `parent` (the
   fork item that revealed it, `null` at the root), `status: "seeded"`,
   `chain` (allocate `chains/<area>-<element>.md`), `discovered_by` = this run
   dir, and `path` = the boot replay (exact calls; `java` then `godot`), with
   `path.confirm` stating how the fork proves itself. In-game actions (move,
   shoot, mine, place, drag, rotate, select, pause, skip wave) are items too —
   `path` then ends at a deterministic world (e.g. the
   `ground-zero-fresh-launch` replay) and `steps` carry the input.

5. Classify each element with one action pair (stop at the first dialog/menu it
   opens; do not chain deeper in this sweep):

   **Java** (raise first, every time):

   ```
   window_ctl.py raise Mindustry
   computer-mcp_mouse_move {"x": X, "y": Y}
   computer-mcp_click {"x": X, "y": Y, "button": "left"}
   capture_screen.py --window-title Mindustry --output "$RUN/seed/step-NN-<element>-java.png"
   ```

   Menu buttons often consume the first click as hover only — verify in the
   capture and send a second click at the same point before concluding a miss.

   **Godot** (center from the step-3 listing; one discrete call per event, then
   flush):

   ```
   godot_input {"action":"mouse_motion","params":{"position":{"x":X,"y":Y},"coords":"viewport"}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":true}}
   godot_input {"action":"mouse_button","params":{"button":"MOUSE_BUTTON_LEFT","position":{"x":X,"y":Y},"coords":"viewport","pressed":false}}
   godot_exec {"action":"eval","params":{"code":"Input.flush_buffered_events()\nvar h = get_tree().root.gui_get_hovered_control()\nreturn {\"pid\": OS.get_process_id(), \"hovered\": (\"\" if h == null else str(h.get_path()))}"}}
   godot_screenshot {"action":"game"}    # copy to "$RUN/seed/step-NN-<element>-godot.png"
   frame_diff.py --java "$RUN/seed/step-NN-<element>-java.png" --godot "$RUN/seed/step-NN-<element>-godot.png" --out-json "$RUN/seed/diff-NN.json"
   ```

   Apply the gate from the parity-eval skill §4. Pass → `status: verified`,
   `verified: {mode: "seed", date, run}`. Fail → `status: "mismatch"` with the
   step, both frames, the diff JSON and both legs' exact calls. The fix flow
   reproduces the recorded stop once, then fixes; the sweep never edits code.

6. Return both clients to the fork before the next element and confirm it:
   Java `computer-mcp_key_down {"key": "escape"}` + `computer-mcp_key_up
   {"key": "escape"}` (split calls; click the window first if focus was lost),
   or the dialog's Back/cancel control; Godot the same via `godot_input` or the
   dialog close button. Re-capture / re-list and assert the surface is
   unchanged. For destructive entries (Quit, Abandon), classify on the confirm
   dialog, then cancel — never complete them in a sweep.

7. Fork children: when an action opens a new surface (Play → submenu; Campaign
   → planet page; sector → in-game HUD), append the new surface to
   `seeding.frontier` and write its controls as items whose `path` extends the
   current one with that action's calls. Do not classify the children in this
   sweep — they are the next sweep's surface. If the sweep's bound allows one
   depth level, descend once and repeat steps 2–6 there.

8. Teardown and docs: quit Java gracefully (`java-reference-leg` §Steps 5), stop
   the Godot game if the sweep is over, set `seeding.last_sweep` to this run,
   pop the swept frontier entry, write `report.md` (surface, elements found,
   matches, mismatches, new ids, enumeration method), and commit docs only —
   never a code change from a sweep.

## Success signals

- One frame pair per classified element plus the surface pair, all non-blank;
  each has a `seed/diff-NN.json` whose metrics are recorded in the item.
- Every discovered element has a ledger item with `path`, `parent` and
  `discovered_by`; matches are `verified.mode: "seed"`, divergences are
  `mismatch` with both frames and both legs' calls.
- `seeding.last_sweep` updated, frontier reflects exactly the unswept forks,
  and both clients are back at a known state (or cleanly stopped).

## Failure modes

- A Java click lands nowhere: check the window was raised and the coordinate
  came from this run's capture `region`; menu buttons may need a second click.
- A Godot click does not land: the pointer may already be over the target, or a
  STOP control covers it — read `gui_get_hovered_control()` before retrying.
- Blank capture: retake; never classify a blank as a mismatch (parity-eval
  skill §4).
- An eval body errors and pins the frame loop: recover with `godot_game stop` +
  `play` (parity-eval skill §8).
- The Java surface changed since the capture (animation, toast): pause or wait
  for a stable frame; toasts/fades are not findings.

## Variants

- In-game surface sweep: start `path` from the `ground-zero-fresh-launch`
  replay, pause both clients, and enumerate HUD buttons/toolbar slots; world
  actions need the `step()`/pause pairing from `pause-step-interact`.
- Two-pass sweep without vision: enumerate Java labels from the Java source and
  locate them with hover probes; record the enumeration method in the report.
