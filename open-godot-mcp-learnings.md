# open-godot-mcp learnings

Operational friction hit while driving the Godot client through open-godot-mcp
and the `playtest` skill, with the workaround that got past it. Reusable
sequences belong in `.opencode/chains/`; this file is for everything that is
not a chain: environment quirks, tool errors, timing traps.

The `parity-writer` (Godot leg) and `twin-evaluator` append a dated entry after
a run that taught them something. Never rewrite another session's entry; keep
it to what happened, what worked, and the run directory or chain that shows it.
If nothing new happened, no entry.

## 2026-10-07 — Seed notes from the playtest and parity-eval skills

- `godot_game play` without an explicit `params.scene` reports success but
  attaches nothing; every later `godot_exec` fails with
  `RUNTIME_NOT_CONNECTED`. Always pass `res://scenes/game.tscn`.
- Injected input sits in the accumulated-input buffer: after a `godot_input`
  press/release, flush with `Input.flush_buffered_events()` and assert the
  target state in the same eval.
- `godot_screenshot game` can return a stale presented frame when the editor
  covers the game window. Call `get_window().move_to_foreground()` first, or
  capture the X screen and crop.
- Eval bodies with `for`/`while` time out; a `null` result with `ok: true`
  usually means the body errored — check `godot_log errors` before retrying.
- An eval that errors stops in the editor Debugger (break-on-error) and freezes
  the frame loop; clear `godot_log` before a scenario and recover with
  `godot_game stop` + `godot_game play`.

## 2026-10-07 — Slot limiter counts a dead owner's lease until TTL; check the fix is in this worktree first

- The first `godot_health check` after launching the loop-1 editor was refused
  with `[mcp-slot-guard] MCP slot limit reached (MCP_SLOT_LIMIT)` even though one
  holder was obviously dead. `mcp_slot.py status` showed `free: 0`: stale lease
  `oc-4240` (pid 11854 dead) still counted because its heartbeat was inside the
  default 900 s TTL (expires ~15:12Z), and loop-2's live lease held the second
  slot. `gc` cannot reap before the TTL; the guard's documented behavior is to
  stay off MCP and release the finding as `godot-open` with the refusal as the
  blocker. If a slot is needed sooner, only the TTL expiry frees it.
- Before spending a slot on a named writer commit, verify it is actually in this
  loop's worktree: `git merge-base --is-ancestor <fix> HEAD` and
  `strings client/bin/rust/debug/libmind_gdext.so | grep -c <new_fn>`. Here
  EV-0037's fix `e85732c` existed only on `parity/loop-2` (its binary, built
  01:04, contained `materialize_map_buildings`), while the finding's session label
  was loop-1 whose worktree (`main @5f5719d`) and pre-fix binary (20:16) lacked
  it entirely; a ledger recovery note had claimed "present on main" from a
  `git log --all` sighting. An in-engine run there could only re-observe the
  unfixed behavior, so the evaluation was released `godot-open` with the worktree
  mismatch, not a verdict on the fix.
- Evidence: `runs/20261007-150716-ev0037-core-launch-godot/` (blocked before any
  repro; no game started).

## 2026-10-08 — Drag input: select through MindInput, and the preview lies over occupied tiles

- `SimHost.select_block(name)` (the sim-host API) only changes the host's
  selected block; the input controller keeps the settings-persisted block from
  boot. The first left-drag after a restart therefore placed the persisted
  `duo` line at x=10..15 y=16 even though `SimHost.selected_block()` read
  `conveyor`. Use `godot_exec call MindInput.select_block_by_name(name)` and
  assert `get_input_state_json().block` before dragging. Sequence in
  `.opencode/chains/pause-step-interact.md` step 1.
- `placement_preview_json()` keeps painting the drag rect over
  occupied/blocked tiles; the release enqueues `Place` commands that leave the
  occupied tiles unchanged, so a drag can look accepted while the dump shows
  nothing placed (and `commands_applied` still grows). Assert tiles from
  `get_state_json()`, not the preview.
- Right-press behavior on an empty conveyor in the same run: opens
  `OverlayLayer/block_inventory` at the click position with a 1-child grid
  (close X only) and does not enter breaking mode; right-drag over
  x=10..15 y=16 leaves all 6 placed (`commands_applied` unchanged) while the
  copper-wall control breaks 6 -> 0. This is the EV-0043 gap still reproducing
  at main 5f5719d, not a setup error.
- Slot limiter: the stale `oc-4240` lease from the entry above expired on
  schedule at 15:12:26Z; a retry of `godot_health check` after that minute had
  the guard reap it and acquire succeeded (loop-2's live lease kept the other
  slot). Waiting out the TTL and retrying once is the working path.
- Evidence: `runs/20261008-011233-input_controls-godot/` (clean repro on pid
  27548; first instance pid 20742 discarded, notes in the run's `godot/`).

## 2026-10-08 — a frozen frame loop: `godot_game resume` is a no-op, stop+play is the fix

- An eval error (`Invalid access to property or key 'text' on a base object of
  type 'VBoxContainer'`) tripped the editor break-on-error as the seed note
  warns, but recovery differed: `godot_game resume` returned `ok: true` and
  `get_tree().paused` read `false`, yet `Engine.get_process_frames()` stayed at
  1493 across ~10 s and no `godot_input` landed. `godot_game stop` + `play` was
  the only thing that revived the loop; every artifact after that is from the
  fresh runtime (pid 32663). Check the frame counter (not just `paused`) before
  trusting an input call, and expect to restart rather than resume.
- `godot_input sequence` timed out after 15 s and injected nothing (the rail
  `button_pressed` did not change); the same click as discrete
  `mouse_button` press/release + `Input.flush_buffered_events()` landed. Chains
  now carry the discrete form (`chains/research-purchase.md`).
- Evidence: `runs/l2-20261008-012103-ev0052-research-purchase-godot/` (EV-0052
  godot-pass; stalled instance discarded, fresh run recorded).

## 2026-10-08 — a file-write eval times out; after the EV-0061 panic, state evals still answer

- A `godot_exec` eval that wrote `MindCampaign.get_rules_json()` to a run-dir file with
  `FileAccess.open_absolute_path(path, FileAccess.WRITE).store_string(...)` timed out after
  15 s and wrote nothing, while the frame loop was already stalled (frames stuck at 1079 after
  the EV-0061 `MindUi::hud_set_visible` panic). Plain state evals kept answering through the
  debugger channel and read identical rules values, so the panic stalls the frame loop but not
  `godot_exec`; do not mistake a stall for a dead runtime. Recovered with `godot_game stop` +
  `play` (fresh pid 50912) and captured the rules JSON from the eval response host-side instead
  of writing from inside the game.
- On main/loop-1 a bare Ground Zero launch always ends in the EV-0061 panic (zero-core game over,
  because EV-0037 means `load_sector` has no ECS building entities and `hasCore=false`). The
  rules values under test are unaffected and stable across two fresh instances; attribute the
  panic to EV-0037/EV-0061, never to the campaign-rules path.
- Evidence: `runs/20261007-152746-ev0049-campaign-preset-rules-godot/` (EV-0049 godot-pass;
  run1 pid 47764 discarded after the stall, run2 pid 50912 recorded). Sequence now in
  `chains/sector-preset-rules.md`.

## 2026-10-08 — a frozen *viewport* (not just stale screenshots): frames_drawn pins, xwd is the fallback

- `godot_screenshot game` returned byte-identical PNGs for hours (7 files, md5
  `946100a1…`, including frames captured by an earlier session), and
  `get_window().move_to_foreground()` did not change that. The underlying state
  was worse than the seed note's "stale presented frame": `Engine.get_frames_drawn()`
  stayed pinned at 11703 while `Engine.get_process_frames()`, `Time.get_ticks_msec()`
  and every `godot_exec` kept advancing — the render loop was not drawing at all
  (editor-embedded game window spawned with `--wid <editor host>` under Xvfb).
  `xwd -id <game window>` of the live window showed the same frozen menu, and
  input-driven UI state (dialog visible, cards built, `_slots`) was correct but
  never rendered.
- `godot_game stop` + `play` revived drawing: a fresh pid's `frames_drawn` advanced
  ~140/s and an `xwd` capture of the game window showed the opened Load Game
  dialog. Check `frames_drawn` twice before trusting any screenshot; restart rather
  than trying to force a repaint. After the restart the MCP `godot_screenshot`
  content was again byte-identical to the pre-restart menu frames while frames
  advanced (static menu, possibly a cached image), so the run's visual evidence was
  taken from the game X window (`xwininfo -root -children` to find it, `xwd -id`
  to dump, a local XWD->PNG parser to decode). When the render is live, prefer
  structured state; keep xwd for the pixels.
- A separate eval trap: calling a CanvasItem-only method bare in an eval body
  (`get_viewport_rect()`) timed out for the full 15 s with **no** `godot_log errors`
  entry, so the documented "timeout means the body errored, check the log" gave no
  signal. Use `DisplayServer.window_get_size()` / `get_tree().root.get_visible_rect()`
  instead, and prefer `map`/`filter` over `for` loops (a 3-iteration loop over
  `get_children()` returned, an 8-iteration dict-building loop timed out).
- UI layout trap seen in the same session: the Play submenu's `Submenu/Buttons`
  has a non-Button spacer at child 0 and its rows are laid out relative to the
  clicked button; the Load Game button's resolved center moved from (460,35) after
  first show to (460,359) once layout settled. Always re-resolve the rect in the
  eval immediately before the click — see `chains/load-game-dialog.md`.
- Evidence: `runs/l2-20261008-012632-ev0054-load-game-godot/` (EV-0054 godot-pass;
  stalled pid 44068 discarded, fresh pid 56232 recorded, `godot/step-02-load-dialog.png`
  from xwd). Sequence now in `chains/load-game-dialog.md`.

## 2026-10-08 — eval-body errors can pin the viewport, and the campaign dialogs read a boot-time fixture

- Two `godot_exec` evals timed out back-to-back while reading campaign data: the
  first body called a method that does not exist on the node
  (`MindCampaign.campaign_views()` logged `Invalid call. Nonexistent function` and
  the wrapper still burned its 15 s), the second built a large array inside a
  `filter` lambda. After that `Engine.get_frames_drawn()` pinned at 13903 for ~40 s
  while `Time.get_ticks_msec()` and evals kept advancing, `godot_screenshot game`
  returned byte-identical stale menu PNGs, and an `xwd` capture of window
  0x1200002 on :11 showed the same stale frame — UI node state still updated
  (planet dialog closed, settings dialog `visible=true`) but nothing was drawn.
  `godot_game stop` + `play` recovered; the documented "check the log on eval
  timeout" did give the culprit here (unlike the earlier silent-timeout case).
- Project fact discovered in the same run: `MindUi.campaign_views()` falls back to
  `CampaignViews::vanilla_fixture()` when the live
  `MindCampaign.campaign_views_json` endpoint is absent, and the planet/loadout
  dialogs cache that snapshot at `_ready()`. In this build the endpoint is absent,
  so the planet dialog shows fixture sector flags (`groundZero` `has_base`/`captured`
  → action `@sectors.go`) no matter the real save state, and clearing campaign saves
  mid-process does not refresh it. Restart the game after clearing, and check
  `MindCampaign ready (0 save slot(s))` + `list_save_slots()` for live state. A
  no-save `go` click still launches fresh (`play_sector` → `play_new_sector`).
- Evidence: `runs/l2-20261008-014155-ev0037-core-launch-godot/` (EV-0037 godot-pass;
  stalled pid 85285 discarded, pids 111646/114964 recorded; stall in
  `evidence/stall-observation.md`; sequence now in `chains/ground-zero-fresh-launch.md`).

## 2026-10-08 — config-fragment close X ignores injected clicks (hover still works)

- While staging the sorter config check for EV-0043, the `block_config`
  fragment's close X (`@Button@2583`, global rect (617,294,28,34),
  `mouse_filter=0`, one `pressed` connection) would not fire from any injected
  click: discrete `godot_input mouse_button` press+release, the one-call
  `godot_input sequence` (motion + press + release, frame_delay 1), and a
  synchronous `get_viewport().push_input(down)` / `push_input(up)` pair all left
  `block_config.visible == true`. `get_tree().root.gui_get_hovered_control()`
  did report the X after the same injected motion, so the coordinates map and
  the Control is pickable — only activation fails. The handler itself works:
  `xb.emit_signal("pressed")` closed the fragment immediately.
- Workaround used for the run (staging only, no sim state touched): close the
  fragment via its own emitted `pressed` signal, and capture the config-open
  artifact while it is open instead of after a click. A future test that needs a
  real click path on an `OverlayLayer` fragment `Button` should budget for this
  and record the fallback in `run.json`.
- Evidence: `runs/20261008-015146-input_controls-godot/` (EV-0043 godot-pass),
  staging note in `godot/godot-break-evidence.json`.

## 2026-10-08 — a malformed eval body pauses the debugger (frames freeze) and mimics the EV-0041 wedge

- While evaluating EV-0041, two `godot_exec` eval bodies errored and froze the
  running game's frames/tick: `await Engine.get_main_loop().process_frame`
  (logged `Trying to call an async function without "await"`) and a multi-line
  lambda (`var extra = …` + `if/elif` inside `func`). `godot_debugger sessions`
  reported `{active:true,paused:true}` and `stack_trace` pointed into
  `gdscript://<negative-id>.gd`; `godot_debugger resume` re-broke immediately and
  only `godot_game stop` + `play` recovered. While paused, `godot_input sequence`
  calls time out (their `process_frame` await never resolves) and screenshots
  repeat byte-identical frozen frames. This is exactly the pre-fix EV-0041
  symptom (frozen `Engine.get_process_frames()`), so a debugger pause must be
  ruled out before attributing a freeze to the game.
- Workaround: keep eval bodies to simple statements and single-expression
  `map`/`filter` lambdas, never `await` inside eval; on a frozen frames read,
  check `godot_debugger sessions` + `stack_trace` first. Also
  `move_to_foreground()` logs a deprecation error on Godot 4.7.2 — use
  `get_window().grab_focus()` before screenshots (`probe-hud` chain updated).
- Evidence: `runs/l2-20261007-160101-ev0041-campaign-rules-godot/` (EV-0041
  godot-pass; pids 154748/161392 recorded; sequence in
  `chains/campaign-rules-dialog.md`).

## 2026-10-08 — `SimHost.place_block` returns true when the live runtime silently rejects; anchor differs by block size

- Staging the EV-0047 production probe, `place_block(128, 47, "mechanical-drill")`
  returned true but placed nothing: the 2x2 drill footprint overlapped a
  just-placed `core-shard` (3x3). The runtime logged only
  `[D] place command rejected at (128, 47) by the live block runtime` and
  `SimHost::apply` still returns `Ok`, so the GDScript bool is not a placement
  result. Read back a filtered `get_state_json()` tile window after placing.
- Anchor convention: odd-size blocks anchor on the given tile (center), even-size
  blocks use it as the top-left (`valid_place` offset `-(size-1)/2`). A 2x2 at
  (129,48) is (129-130,48-49); a 3x3 at (130,46) is (129-131,45-47).
- Project fact from the same run: `core_items_json()` reports the first core in
  `entities_by_seq()` order, so a materialized map core (Ground Zero build_id 5)
  shadows any later-placed core; break it first when a placed core must be
  observed.
- Also: a mechanical drill (`drillTime 600`, max 4 ore tiles) delivers its first
  item only at tick ~183 including warmup, so an empty core at the repro's
  120-tick sample is expected rate behavior, not inertness.
- Evidence: `runs/l2-20261008-021834-ev0047-production-godot/` (EV-0047
  godot-pass; pid 182200; sequence in `chains/ground-zero-production-probe.md`).

## 2026-10-08 — a batched drag loses its tail: the final masked motion + release coalesce

- Sending a right-drag as one `godot_batch` of `godot_input` ops broke only 5 of
  6 conveyors: the final masked motion onto tile 15 and the release at the same
  position were processed too tightly and tile 15 survived (`commands_applied`
  6 -> 11; `mode=none`). A discrete right-press at the survivor broke it
  (11 -> 12). Re-running the sweep with one discrete tool call per event broke
  all 6 (18 -> 24). Placement drags were fine in a batch; only the break sweep
  tail lost an event.
- Workaround: for break/sweep tails, send one `godot_input` call per motion and
  release; keep `godot_batch` for placement drags. Chain
  `pause-step-interact.md` now carries the note.
- Also re-confirmed: an eval body that *times out* (here a `while`-loop tree
  walk) stalls the frame loop like an error break — `godot_game resume` does
  not revive it, `stop` + `play` does; check `Engine.get_process_frames()`
  twice before trusting input. Two stalled pids (231551, 245330) were discarded
  this way.
- Evidence: `runs/20261008-020931-input_controls-twin/` (EV-0043 twin pass,
  pid 249867; `godot/godot-break-evidence.json` step_3/step_4).

## 2026-10-08 — `start_sector_with_loadout` takes a JSON string; persistent `settings.bin` unlocks defeat the EV-0052 baseline

- `MindCampaign.start_sector_with_loadout(planet, sector, loadout)`'s third
  parameter is a GString (JSON), not a Godot Array: passing `[{...}]` raises
  `parameter #2 (GString) conversion` in the Rust call, logs
  `Bug: Invalid call error code 1337`, and trips break-on-error
  (`godot_debugger sessions` → paused). `godot_debugger resume` recovered it
  here. The accepted shape is
  `JSON.stringify([{"item":"copper","amount":500},{"item":"lead","amount":500}])`;
  the Java-style `{"copper":500}` deserializes to amount 0 and silently yields
  an empty core.
- The port persists tech unlocks as `*-unlocked` keys in
  `user://config/settings.bin`
  (`…/xdg-data/godot/app_userdata/Mindustry-Godot/config/settings.bin`) and they
  survive process restarts. After my EV-0037 UI launch the live tech state
  booted at `unlocked=70` and `research("conveyor")` returned true without
  spending (already unlocked live, while the fixture dialog row still showed it
  locked) — the EV-0052 purchase only reproduces from the repro's 9 baseline.
  Back up/remove settings.bin before the setup; clear saves and then restart,
  or the campaign model loads the boot-time autosave and carries its unlock set.
- With the fresh profile the exact repro works first try: `unlocked` 9 → 70,
  copper 500 → 495, log `[I] research auto-unlocked 60 content`.
- Evidence: `runs/l2-20261007-171145-ev0052-research-twin/` (pid 248258;
  `godot/state-01-purchase.json`, `godot/settings-pre-reset.bin`).

## 2026-10-08 — a loaded 256x256 sector dump cannot be JSON-parsed in one eval; `get_meta` logs on missing keys

- `SimHost.get_state_json()` on a fresh Ground Zero is ~6.6 MB (len 6_622_635).
  Calling `JSON.parse_string()` on it inside one eval exceeds the 30 s
  `godot_exec` budget, and the oversized reply then trips the editor websocket:
  `godot_log errors` shows source `editor`, message
  `Condition "outbound_buffer_size > 0 && (wslay_event_get_queued_msg_length(wsl_ctx) + p_buffer_size > (uint32_t)outbound_buffer_size)" is true. Returning: ERR_OUT_OF_MEMORY`.
  Workaround: call `get_state_json()` once, stash it with
  `SimHost.set_meta("dump", s)`, then read with `String.find/count/substr`
  (substring context around a `find` gives tile coordinates). Never return the
  dump through the bridge.
- `Node.get_meta("k")` on a missing key logs
  `The object does not have any 'meta' values with the key 'dump'.` to the game
  log (it counts as a `godot_log errors` entry); guard with `has_meta("k")` or
  set the meta first.
- `UiRoot.set_menu_visible(false)` hides `MenuGroup`/shows `HudGroup`, but the
  standalone `menu` fragment node under `MenuGroup` keeps `.visible == true`;
  assert on `MenuGroup`/`HudGroup` (or a screenshot), not the fragment.
- Evidence: `runs/l2-20261008-034337-ev0047-production-twin/` (twin sweep, pid
  267163; `godot/map-probe.json`, `godot/game.log`) and
  `runs/l2-20261008-034014-ev0054-load-game-twin/`.

## 2026-10-08 — an *errored* eval (not just a timeout) stalls the loop; camera transform lags a frame; paused HUD covers the center probe point

- An eval body that errors trips break-on-error and pins the frame loop just like
  a timeout: a probe using a guessed dialog button path
  (`/root/Spine/Ui/UiRoot/DialogLayer/paused/settings`) logged `Node not found`
  plus `has_meta` on a null instance, and `Engine.get_process_frames()` stayed at
  10368 for ~14 s (`godot_log errors` shows the cause). Recovery is
  `godot_game stop` + `play` (restart, do not resume). Locate dialog buttons with
  `paused.find_child("settings", true, false)` — the real path is
  `/root/Spine/Ui/UiRoot/DialogLayer/paused/Center/Panel/Layout/Buttons/settings`,
  not a direct child of the dialog (which also cost a 15 s TIMEOUT).
- `Camera2D.tile_to_screen`/`screen_to_tile` read the viewport canvas transform,
  which updates on the next frame after `center_on_tile`/`pan_to`: reading back in
  the same eval returns stale coordinates (tile (16,16) resolved to (5841,5589)
  instead of (576,324)). Resolve click coordinates one MCP round-trip after moving
  the camera.
- While paused, the viewport center is covered by the STOP
  `HudGroup/hud/PausedBanner` and the lower-right by `HudGroup/placement/Panel`.
  A paused world-click probe aimed at center reads `pending_command_count == 0`
  and looks like an input regression; probe a point whose
  `gui_get_hovered_control()` is a PASS control (or none) first — at (200,200)
  hovered was `HudGroup/hud` (PASS), the click enqueued (`pending` 1), and
  `step(1)` applied a conveyor at tile (4,12).
- Evidence: `runs/20261008-133538-ev0062-ui-click-godot/` (EV-0062 godot-pass,
  pid 20583; `godot/state-05-world-input-regression.json`).

## 2026-10-08 — `godot_exec` budget is 15 s, not 30 s; a timed-out `step(50000)` keeps running and freezes the campaign HUD mirror

- A single `SimHost.step(50000)` eval returned
  `TIMEOUT ... timed out after 15.0s` (the earlier 6.6 MB-dump entry says 30 s —
  the wrapper's actual budget on this host is 15 s). The game does **not**
  abort the eval: the step ran to completion (`get_tick()` came back at
  +50000), and every later eval blocked until it finished (`sleep 20` + poll,
  then a cheap eval answered again).
- After that timed-out step the campaign HUD mirror went stale in a way that
  outlived the timeout: `MindCampaign.get_hud_state()` kept returning the
  pre-step `wavetime`/`enemies` (wavetime 7184.54 unchanged across a `step(600)`
  and a later pump), while `SimHost.get_group_counts()` stayed correct. For
  live-runtime liveness, assert on `get_group_counts()` / `str()` aggregates,
  not the mirrored HUD.
- Budget workaround: chunk large advances (e.g. 6-10 × `step(600)`) and read the
  group counts each chunk; unit movement/weapons evidence is then visible
  without any single long call.
- Also confirmed for the EV-0048 leg: `place_block` can return `true` while the
  live block runtime rejects the tile — the only trace is a `[D] place command
  rejected at (x, y)` info line and an unchanged `build` group count.
- Evidence: `runs/20261008-134638-ev0048-live-unit-runtime-godot/` (EV-0048
  godot-pass, pid 36615; `run.json` anomalies, `report.md`).

## 2026-10-08 — a rebuilt GDExtension is not picked up by a running loop editor; verify what the game actually mapped

- The loop-2 editor (pid 17601, started 13:35) was already running when the
  writer's fix `ca81672` was committed at 14:00 and the extension rebuilt at
  14:02. `godot_game play` starts the game *inside* the editor process, which
  mmap'd `libmind_gdext.so` at editor start: playing without restarting the
  editor would have evaluated the pre-fix code even though the file on disk was
  new. The library mtime/sha alone cannot tell you what a long-lived editor
  loaded.
- Workaround: compare the built `client/bin/rust/debug/libmind_gdext.so`
  mtime/sha against the fix commit; rebuild with `tools/build.sh` when it
  predates the commit; then kill the editor and relaunch
  `.opencode/loops/bin/run-godot-editor.sh` before `godot_game play`. Cheap
  pre-flight on the artifact: `nm -C <so> | grep <new-symbol>` (the pre-fix .so
  had no `read_command_mode_hold`/`update_command_mode`). Post-play proof of what
  is loaded: `grep libmind_gdext /proc/<game pid>/maps` plus a `sha256sum` of
  that path.
- Evidence: `.opencode/evals/runs/l2-20261008-140421-input_controls-godot/`
  (EV-0044 godot-pass, game pid 81133; `godot/rts-evidence.json` library block).

## 2026-10-08 — loop-1 twin Godot leg (EV-0048): no new friction

- Explicitly nothing new: the `units-live-wave-runtime` chain ran unchanged at
  a1ca816 (fresh groundZero checksum `4353bdfd835ec038`, `run_wave` + `step(1)`
  -> unit 0->1 at checksum `9f527206f0c78da9`, second wave -> 4 units, bullets
  by tick 420, empty error log). Recorded for the next twin evaluator; no entry
  beyond this one. Evidence:
  `runs/20261008-040830-ev0048-live-unit-runtime-twin/godot/`.

## 2026-10-08 — loop-1 twin Godot leg (EV-0062): window resize no-op, `move_to_foreground()` deprecation in the error log, `center_on_tile` applies next frame

- `get_window().size = Vector2i(900,700)` did not stick (the next read still
  reported 1152x648). Record the actual window size instead of assuming the
  resize took effect.
- `get_window().move_to_foreground()` (the screenshot-staleness workaround)
  records an ERROR in the log: `The "move_to_foreground()" method is
  deprecated, use "grab_focus()" instead.` It was the only non-empty error-log
  artifact of this run; clearing the log and re-doing one click gave
  `godot_log errors: []`. Prefer `grab_focus()` for the staleness workaround.
- `MindCamera2D.center_on_tile(x,y)` takes effect on the next frame:
  `screen_to_tile()` in the same eval read a stale transform ((-270,-262)
  instead of (4,12)); after a frame the mapping was (200,200) -> (4,12) as in
  the EV-0062 godot-pass run.
- No other friction: `boot-and-identity` + `ui-control-click` ran unchanged at
  a1ca816 (pid 145410, frames 614..21295, errors [] after the clean click).
  Evidence: `runs/20261008-143432-ev0062-twin/godot/`.

## 2026-10-08 — loop-2 twin Godot leg (EV-0044): eval `await` stalls the frame loop; the editor game is embedded and cannot be resized

- `godot_exec` eval code containing `await get_tree().process_frame` (without the
  eval `await` param) errored with `Trying to call an async function without
  "await".` and wedged the frame loop: `Engine.get_process_frames()` stayed at
  1372 while `Time.get_ticks_msec()` advanced 4.4 s across two reads, and a
  `window_set_size` from the same eval never applied. Recovery per the existing
  rule: `godot_game stop` + `play` (new pid 197940), re-stamp before any input.
  The quoted log error is the probe's own, not a game defect.
- The editor game runs **embedded**: `DisplayServer.window_set_size(Vector2i(900,700))`
  returns but the next read still reports 1152x648 and the log says `Embedded
  window can't be resized.` Do not plan a pixel comparison around matching the
  Java window size; for EV-0044 the Godot side compared structured input state.
- Evidence artifact that worked: append one JSON line per probe from the eval
  with `FileAccess.open("user://ev0044-evidence.jsonl", FileAccess.READ_WRITE)` +
  `seek_end()`/`store_line()`, then copy the file out of
  `<worktree>/.opencode/loops/run/loop-2/xdg-data/godot/app_userdata/Mindustry-Godot/`
  into the run dir — the exact eval-produced samples survive as evidence.
- `get_window().move_to_foreground()` re-confirmed the EV-0062 deprecation ERROR
  in the log; prefer `grab_focus()`.
- Evidence: `runs/l2-20261008-150205-input_controls-twin/` (`godot/rts-evidence.jsonl`,
  `godot/log-errors.json`, `run.json`).

## 2026-10-08 — loop-2 twin Godot leg (EV-0055): `toggle_command_mode()` is transient under hold-mode; tap radius

- `MindInput.toggle_command_mode()` returns true and `get_input_state_json`
  reads `command_mode: true` when sampled in the **same** `godot_exec` eval, but
  a separate read one frame later is false again while Shift is up: with the
  default `commandmodehold=true` the update recomputes
  `command_mode = keyDown(shift)` every frame, so the API latch only survives
  within the frame. For a persistent command mode hold Shift (the player path);
  use the API only as a same-eval positive control.
- The `set_selectable_units_json` seam takes **world pixels** (TILESIZE=8) and a
  tap selects the closest commandable unit within `UNIT_TAP_RADIUS` (11 world
  px) of the cursor; a tap outside that radius silently reads as empty ground
  and clears the selection. Registration order/visible icons do not matter —
  the unit must sit at the tap point's world coords. `chains/rts-select-orders.md`
  corrected accordingly.
- `get_window().move_to_foreground()` re-confirmed the already-logged
  deprecation ERROR (`use "grab_focus()" instead`); it was the only error entry
  and landed after all input probes. Prefer `grab_focus()` and attribute the
  entry in the report when the workaround is used.
- Evidence: `runs/l2-20261008-051657-ev0055-rts-select-twin/godot/rts-evidence.json`,
  `godot/step-01-command-mode-selected.png`.

## 2026-10-08 — a loop worktree without the generated atlas silently text-falls-back every icon check

- The loop-2 worktree (`Mindustry-Godot-loop2`) has no
  `assets/sprites/sprites.atlas.json`: it is gitignored and produced by
  `tools/pack.sh`, and the sync/merge does not carry it into a fresh worktree
  (main checkout has it). `MindAssets` resolves the asset dir by probing
  `res://assets` then `<project>/../assets` and therefore boots with zero
  regions; the run log shows `[assets] ready ok=…` before any evaluator
  `godot_log clear`, so the condition is easy to miss.
- Symptom: every `MindWidgets.image_button_first` icon resolves `null`, so
  `PlacementFragment` block buttons keep `has_icon: false` and render their
  label text at 67–120px instead of 46px icons; category glyphs still render
  (they are child `Label`s, not `icon`), and the world background can render
  blank. Distinguish from a code regression by probing the atlas, not the UI:
  `get_node("/root/MindAssets").call("region_count")` is 0 and
  `call("has_region", "block-duo-ui")` is false.
- Workaround for icon-dependent checks in a fresh worktree: run the pack
  (`tools/pack.sh`) or point `mindustry/assets_dir` at a checkout whose atlas
  exists before launching the editor; otherwise record the limitation and judge
  only code-path observables. This run (`EV-0013`) failed on the catalog
  filtering independently of the atlas and recorded the icon half as
  environment-limited.
- Evidence: `runs/l2-20261008-133536-ev0013-block-picker-godot/` (pid 18767;
  `godot/state-04-ingame.json` assets.region_count = 0,
  `godot/step-04-ingame-hud.png` text-fallback buttons).

## 2026-10-08 — the paused banner is a STOP Control at viewport center; paused-run world clicks there vanish

- While `SimHost.set_paused(true)`, `/root/Spine/Ui/UiRoot/HudGroup/hud/PausedBanner`
  is visible with `mouse_filter=0` (STOP) and a ~64x26 `get_global_rect()`
  centered on the viewport. `MindInput.ui_captures_at` (upstream
  `Core.scene.hasMouse()`) drops any world press whose point falls inside a STOP
  control, so a right-click `command_tap` or selection tap aimed at the exact
  center silently does nothing while paused — `pending_command_count` and
  `selected_units` stay unchanged with no log entry.
- Workaround: before injecting a world click, probe the point in one eval —
  `get_tree().get_root().find_children("*", "", true, false).filter(func(n):
  n is Control and n.is_visible_in_tree() and int(n.get("mouse_filter")) == 0
  and n.get_global_rect().has_point(pt))` — or simply click off-center. Do not
  use viewport center for paused-run probes, and when a click appears to be a
  no-op, check STOP hits before suspecting the input path.
- Node-path reminder for the RTS checks: `get_input_state_json` /
  `toggle_command_mode` / `set_selectable_units_json` / `selection_json` live on
  `/root/Spine/Input` (`MindInput`), while `set_paused` / `step` /
  `pending_command_count` / `get_state_json` live on `/root/Spine/SimHost`.
- Evidence: `runs/l2-20261008-134830-ev0055-rts-select-godot/` (pid 33526; step 5
  of `godot/rts-selection-evidence.json`, screenshot
  `godot/step-06-after-live-order.png`).

## 2026-10-08 — loop editor predating a rebuild; `campaign_views_json` needs a planet arg

- Binary freshness for a loop editor is two-tier: the game process loads
  `libmind_gdext.so` when it starts, but the editor itself was started at 14:03
  with the pre-7352edb library while the writer's build landed at 14:24. The
  editor was killed and relaunched with
  `.opencode/loops/bin/run-godot-editor.sh` (same bridge 6980) before the run; a
  cheap in-engine probe settles it either way —
  `MindCampaign.has_method("block_catalog_json")` was `true` at 7352edb and
  `false` at a1ca816. `strings libmind_gdext.so | grep block_catalog_json` plus
  `stat` on the `.so` prove the library, not the process that loaded it.
- `MindUi.campaign_views_json()` takes one parameter (planet GString); calling
  it with zero arguments logs `godot-rust function call failed:
  MindUi::campaign_views_json() … function has 1 parameter, but received 0
  arguments` and burns the full 15 s eval timeout. Unlike the earlier
  "malformed eval body pauses the debugger" entry, this one did **not** stall
  the frame loop: `Time.get_ticks_msec()` kept advancing, the pid stayed live
  and the next eval answered normally — check `godot_log errors` for the
  godot-rust reason before assuming a frozen loop. It also falls back to
  `CampaignViews::vanilla_fixture()` (no live `MindCampaign.campaign_views_json`
  in this build), so it is not a usable way to enumerate live unlock state; that
  is what `MindCampaign.block_catalog_json()` / `get_tech_state()` are for.
- Evidence: `runs/l2-20261008-142848-ev0013-block-picker-godot/` (EV-0013
  godot-open; pid 133157; the fixture fallback and freshness probe in
  `godot/state-04-ingame.json`, the timeout error in `godot/log-launch-errors.json`).

## 2026-10-08 — data-driven fragment reads are one frame late; `godot_input sequence` wants `steps[]`

- The placement fragment rebuilds its rail/grid with `child.queue_free()` for
  the old children, and `queue_free` only removes them at the end of the
  frame. A `godot_exec` that toggles `visible` (or calls the fragment's reload)
  and reads `get_children()` in the **same eval** still sees the old, queued
  children — a cleared catalog then looked like "8 categories / 2 blocks"
  instead of 0/0, and the first `state-04` artifact had to be rewritten from a
  follow-up eval. Read data-driven Control children in a later eval than the
  rebuild that queued them (same-frame reads are fine only when the previous
  list was already empty).
- `godot_input sequence` takes `{"steps":[{"type":"mouse_button","params":{...}},
  ...],"frame_delay":1}`. The flat `{"events":[...]}` form fails with
  `INVALID_ARGUMENT: steps[] required (array of {type, params} dicts)` and
  injects nothing; the per-click press+release form above landed every UI step
  of the campaign launch at 986cda3.
- Evidence: `runs/l2-20261008-145144-ev0013-block-picker-godot/` (EV-0013
  godot-pass; pid 179192; the rewritten `godot/state-04-fresh-groundzero.json`
  and the 8-category post-research read in `godot/state-05-after-research.json`).

## 2026-10-08 — a parse-error eval freezes the game while `godot_game status` still looks live

- A `godot_exec` body that used `(var c = …; {…})` as a dict value (a GDScript
  parse error) returned `TIMEOUT` after 15 s and left the game loop paused at
  the debugger. Unlike `godot_game status`, which kept reporting
  `fps: 145`, `draw_calls: 88`, `process_time_ms: 3` (those monitors come from
  the editor process, not the game), the game's own
  `Engine.get_process_frames()` stayed frozen at the same value across later
  evals. `godot_log errors` stayed empty.
- Consequence for UI evals: injected `mouse_button`/`mouse_motion` calls return
  `ok` and `Input.flush_buffered_events()` runs, but nothing dispatches while
  the loop is frozen — a real Host-button click looked like a dead UI and the
  first EV-0059 pre-fix repro at pid 10878 was discarded as contaminated. The
  clean repro is pid 15176 in
  `godot/state-pre-fix-repro.json` (`runs/20261008-083744-ev0059-host-godot/`).
- Workaround: prove liveness with two pid-stamped eval reads of
  `Engine.get_process_frames()` (not `godot_game status`); on a pin,
  `godot_debugger sessions` + `stop`/`play` (never `resume`).
- Evidence: `runs/20261008-083744-ev0059-host-godot/` (EV-0059 godot-pass; pids
  10878 frozen / 15176 clean / 19772 final; sequence in
  `chains/host-match-from-pause.md`).

## 2026-10-08 — `godot_input text` errors and never types; use fresh push_input key events

- Typing `127.0.0.1` into the join dialog's focused `LineEdit` failed with both
  input actions: `godot_input {"action":"text",...}` returned `ok` but the field
  stayed empty **and** left a Godot error in the log (`An input event object is
  being parsed more than once in the same frame…` — the runtime autoload reuses
  one `InputEventKey` object for the press and the release in one frame), while
  `godot_input {"action":"key",...}` sets `keycode`/`physical_keycode` but no
  `unicode`, which `LineEdit` ignores. Workaround (parity-eval skill fallback,
  validated): an eval loop that pushes a **fresh** `InputEventKey` down+up per
  character — `keycode = OS.find_keycode_from_string(ch)`, `physical_keycode`
  the same, `unicode = ch.unicode_at(0)`, `vp.push_input(...)` — typed all nine
  characters in one eval. `godot_input text` should be avoided on this project
  until the addon reuses fresh events.
- Same run, a `godot_exec` eval that used a `filter` lambda on the dialog's
  button row (`b.get_child(0).get_child(1).text`) errored on the childless `?`
  button and paused the debugger loop — the existing "parse-error eval freezes"
  entry class, but this was a **runtime** null/child-access error, not a parse
  error, and `godot_debugger resume` again did not unpause. Recovery is
  `godot_game stop` + `play`; index the known button order instead of mapping
  with lambdas over mixed rows.
- Small `for` evals were otherwise fine (9-char typing loop, 4-element list
  loop) — the 15 s timeout is not triggered by short loops, only by errors.
- Evidence: `runs/20261008-184534-ev0060-join-godot/` (EV-0060 godot-pass; pid
  50570; fallback typing recorded in `godot/state-04-typed.json`, clean
  `godot/game.log` final run; sequence in `chains/join-direct-connect.md`).

## 2026-10-08 — launching the loop wrapper from the worktree breaks the bridge; and the boot camera edge-pans away

- Launch the editor through the **main checkout's** wrapper
  (`$PARITY_MAIN/.opencode/loops/bin/run-godot-editor.sh`), not the worktree
  copy. `loop-vars.sh` resolves `main` from the wrapper's own path, so
  `.opencode/loops/bin/run-godot-editor.sh` executed inside
  `Mindustry-Godot-loop2` computed `PARITY_LOOP_DIR` as the *worktree* runtime
  dir and started a second `wayland-mind2` weston under it. The Godot Wayland
  socket path then became
  `<worktree>/.opencode/loops/run/loop-2/wayland/godot-wayland-0` (109 bytes),
  over the 108-byte UNIX limit: `Can't connect to a Wayland display` → x11
  fallback, editor game `--display-driver x11`, and `godot_health check`
  `BRIDGE_NOT_CONNECTED` while the editor process still lived. The wrapper
  printed `[parity-loop 2] starting weston headless on wayland-mind2` plus a
  fresh java weston — a duplicate-compositor tell. Recovery: kill the worktree
  editor/game/duplicate westons, relaunch the main wrapper (reuses the pidfile
  compositor in the main `.opencode/loops/run/loop-2`).
- Default-world camera edge pan: after boot the OS pointer sits at (0,0), and
  `MindCamera2D` edge-pans while idle — camera position went (128,128) →
  (-7507,-7507) over ~2 minutes, so `tile_to_screen(7,7)` returned (27285,27033)
  and a click there would have missed the world. Park the pointer first
  (`godot_input mouse_motion` to the viewport center) and `center_on_tile(16,16)`
  before resolving coordinates; the verified sequence is
  `chains/placement-rotation.md` step 1.
- `MindSimHost.apply_sim_command_json` has no `rotate` op (only `place`/`break`),
  so a finding's "send SimCommand::Rotate via MCP" repro alternative must go
  through `MindInput.rotate_placed()`/the `rotateplaced` (R) binding instead.
- State-dump tiles hardcoded `rot: 0` (and `team: 0`) in `sim/dump.rs`; a rotated
  building placed through the real input path still read `rot: 0` until 996f288
  made the dump read `BuildingComp.rot`. When a rotation finding's oracle is
  `get_state_json`, assert the dump, not just the input-state `rotation` field.
- Evidence: `runs/l2-20261008-184642-ev0056-rotation-godot/` (pre-fix fail, pids
  32063/33292) and `runs/l2-20261008-185444-ev0056-rotation-godot/` (EV-0056
  godot-pass, pid 71447; sequence in `chains/placement-rotation.md`).

## 2026-10-08 — MindHud/MindUi live at `/root`, not under `/root/Spine`; and `stop` leaves a crash-looking warning

- An EV-0061 probe used `/root/Spine/MindHud` (the other Rust facades are scene
  children there). Both `MindHud` and `MindUi` are **autoloads at the tree root**
  (`/root/MindHud`, `/root/MindUi`), so the eval raised `Node not found` + a null
  call, and because an errored eval trips break-on-error the frame loop pinned
  (`godot_debugger sessions` -> `paused: true`) while the next `godot_exec`
  timed out after 15 s. Recovery: `godot_game stop` + `play` (fresh pid), redo
  with the autoload paths. The verified sequence is `chains/game-over-loss.md`.
- `godot_game stop` does not delete `launchid.dat`, so the next launch logs
  `[W] previous launch may have crashed (leftover launchid.dat); check .../crashes/`
  even for a clean stop. Do not treat the warning as a crash: compare mtimes in
  `$PARITY_LOOP_DIR/xdg-data/Mindustry-Godot/crashes/` against the run start.
- Evidence: `runs/20261008-185511-ev0061-game-over-godot/` (EV-0061 godot-pass,
  pid 74313; the aborted first attempt was discarded and the repro re-run after
  stop+play, so the recorded state files are all from the fresh pid).

## 2026-10-08 — editor-shell frames stay byte-identical across map loads/grid toggles (MapView `_draw` vs SubViewportContainer layering)

- During the EV-0038 Godot leg, `godot_screenshot game` and
  `get_viewport().get_texture().get_image()` returned the same bytes for two
  different loaded maps (Archipelago 500x500, Ancient Caldera 256x256) and for a
  `MindEditor.set_grid(true/false)` toggle, even though `Engine.get_frames_drawn()`
  advanced in lockstep with `Engine.get_process_frames()` and the game window had
  lost focus (`get_window().has_focus() == false`; `grab_focus()` did not restore
  it). Do not immediately call this "stale screenshot".
- Probe liveness with a scene change the shell really draws: setting
  `EditorDialog.modulate = Color(1,0,0,1)` changed the captured pixels, proving the
  capture path was live. The identical frames come from layering:
  `scenes/editor/map_view.tscn` puts a full-rect `SubViewportContainer` (with an
  empty `EditorWorldView` Node2D) over the `MapView` root, so the script's `_draw`
  border/grid/brush primitives never composite into the window (plan-19
  EditorWorldView render stub — a separate gap, not EV-0038).
- Judge editor row/load flows on `MindEditor.status()` JSON (file/width/height/
  last_error) rather than pixels, and keep `godot_log clear` + empty
  `godot_log errors` as the clean-run evidence.
- Also: the map-editor shell is `/root/Spine/Ui/EditorDialog` (a sibling of
  `UiRoot`), not `/root/Spine/Ui/UiRoot/EditorDialog`; the wrong path reads null
  and looks like the dialog never opened.
- Evidence: `runs/20261008-190113-ev0038-editor-row-godot/` (EV-0038 godot-pass,
  pid 87432; sequence in `chains/editor-maps-row-click.md`).

## 2026-10-08 — EV-0057: a rebind target key that is also bound elsewhere opens that action's dialog and gates the camera

- Rebinding Pan Left (`move_x`) to `J` in Settings > Controls and holding it
  moved the camera for exactly one frame, then `pan_axis` read `[0,0]` and
  `gameplay_input_active` false. `J` is still the default `research` binding, so
  the same key-down opened the research dialog (`dialog_stack` `["research"]`)
  and `DesktopBridge.pan_axis` gates to `(0,0)` while `ui_dialog` is true. The
  binding consumption was working; the key choice was the confound. Rebind to an
  unbound key (`U`/`K`/`O`/`L`) before judging, and read the dialog stack before
  calling a rebind dead.
- The KeybindDialog search filter only updates through `_on_search_changed`;
  assigning `_search_field.text` and calling `_rebuild` leaves all 88 entries
  visible (`_entries` is never filtered — matching runs at render time).
- Restarted instance edge pan: after `stop`+`play` the OS pointer can sit at
  viewport `(0,0)` and `Input.warp_mouse` is a no-op under the loop's Wayland
  compositor, so `MouseInput.auto_pan` drifts the camera every frame (measured
  -0.919 px/frame/axis). Judge reload legs on `pan_axis`/`boost_pressed` plus
  drift reversal, or take a no-key baseline first.
- `get_window().move_to_foreground()` for screenshot freshness logs
  `The "move_to_foreground()" method is deprecated...` as an error-level entry in
  `godot_log errors`; treat it as a known tool-side warning, not a run failure.
- Evidence: `runs/l2-20261008-190017-ev0057-keybind-rebind-godot/` (EV-0057
  godot-pass, pids 83855/96741; sequence in `chains/keybind-rebind-camera.md`).

## 2026-10-08 — `godot_game play` reports `runtime_ready` before the runtime can be reached

- `godot_game play {scene: res://scenes/game.tscn}` returned `runtime_ready: true`,
  but the immediate follow-up `godot_game status` reported `runtime_connected:
  false` with `instances[0] = {active: true, args: [...], instance: 1, pid: 0,
  ready: false}` and no `viewport_size`/`fps` fields. Reading that first status as
  a failed attach wastes a stop/play cycle: poll `status` (the runtime connected
  after ~5 s in this run, pid 101131, viewport 1152x648) and only treat a
  persistently `false` status as broken.
- Same run: the initial `godot_log` buffer after `clear` is empty until the game
  boots, so a `godot_log errors` check taken immediately after `play` is not a
  clean-run signal yet — re-read it after the first eval answers.
- Evidence: `runs/20261008-190748-ev0039-custom-game-godot/` (EV-0039 godot-pass,
  pid 101131; sequence in `chains/custom-game-map-list.md`).


## 2026-10-08 — EV-0051: eval TIMEOUTs are usually eval-body errors; no frame_diff deps in the loop env

- A `godot_exec` eval that read `.text` on a Control (the Play submenu's child 0
  is a spacer, not a Button) came back as `TIMEOUT ... after 15.0s` and left the
  editor debugger `paused: true` with `Engine.get_process_frames()` pinned. The
  actual message was only in `godot_log errors` (`Invalid access to property or
  key 'text' on a base object of type 'Control'`). Never attribute a 15 s eval
  timeout to the method being called (the earlier EV-0051 note blamed
  `MindUi.campaign_views()`) until `godot_log errors` is read; recover with
  `godot_game stop` + `play` (`resume` re-breaks), and clear the log so the
  aborted eval does not count as run noise.
- This loop session had no Python imaging stack: the default
  `$HOME/.local/share/mcp-venv` does not exist, system `python3` has no numpy
  and the uv-tool python has no PIL, so `frame_diff.py` cannot run. Fall back to
  the structured JSON comparison (which is higher in the comparison order
  anyway) and record the PNG sha256s as the only frame evidence; do not block
  the verdict on frame metrics.
- The Play → Campaign submenu is data-built and starts with a spacer Control:
  Campaign is `MenuGroup/menu/Submenu/Buttons` child 1, not 0. Planet-dialog
  clicks and the capture read-back sequence are in `chains/campaign-live-views-capture.md`.
- Evidence: `runs/20261008-191650-ev0051-campaign-live-views-godot/` (EV-0051
  godot-pass, pid 123288; earlier aborted pid 121357 recovered with stop+play).

## 2026-10-08 — EV-0058: frozen viewport again on the loop-2 GPU weston session; `move_to_foreground` is an error in 4.7

- The loop-2 game viewport froze for the whole EV-0058 run:
  `Engine.get_frames_drawn()` pinned at 482 (run 1) then 94 (after a
  `godot_game stop`+`play`, which briefly revived drawing 2 → 94) while
  `Engine.get_process_frames()` advanced 14706 → 15829 (run 1) and 3068 → 4250
  (run 2). `MindSimHost.capture("user://…")`, `godot_screenshot game` and
  `RenderingServer.force_draw()` all returned the same stale 1152x648 frame
  (sha256 `429c93dc…a269`, even across restarts). Verdict was taken from the
  pid-stamped `core_items_json()`/checksum JSON per the comparison order; the
  PNGs are recorded as frozen-viewport artifacts, not as before/after evidence.
- `get_window().move_to_foreground()` is deprecated in Godot 4.7 and lands as an
  **error-level** `godot_log` entry ("The \"move_to_foreground()\" method is
  deprecated, use \"grab_focus()\" instead."), which fails the empty-error-log
  requirement if it runs inside the scenario window. The 4.7 replacement is
  `get_window().grab_focus()`; the run had to `godot_log clear` + replay the
  repro to get a clean error list.
- `frame_diff.py` still cannot run in this loop env (no `$MCP_VENV`), but
  `uv run --with pillow --with numpy python3 …` works and gives mean/std for a
  single PNG when frame metrics are needed.
- Ore coordinates for a feedable factory came from an offline MSAV probe
  (`SaveIo::load_bytes` behind the `msav-import` feature in a throwaway
  `/tmp` crate): frozenForest (serpulo/86) has `ore-coal` at (104-107,54-58),
  groundZero has none. The state dump omits floor/overlay, so this offline probe
  is the only way to find ore without loading a sector and scanning tiles.
- Evidence: `runs/l2-20261008-192815-ev0058-factory-recipes-godot/` (EV-0058
  godot-pass, pid 148326; sequence now in `chains/factory-recipe-probe.md`).

## 2026-10-08 — EV-0053: two eval traps that pin the frame loop; same-eval toast assertion

- `godot_exec` evals that `await` without `params.await: true` fail with
  "Trying to call an async function without \"await\"" — the eval returns a 15 s
  `TIMEOUT`, break-on-error pins the frame loop (`godot_debugger sessions` →
  `paused: true`) and every later eval also times out. Recovery is
  `godot_game stop` + `play` (`resume` re-breaks); one game instance was lost
  to this in the EV-0053 run (pid 132811). Split the eval instead of awaiting,
  or pass `await: true` explicitly.
- `MindUi` exports both a `toast` **method** and a `toast` signal. GDScript
  `ui.toast` resolves to the method Callable, so `ui.toast.connect(cb)` and
  `ui.toast.is_connected(...)` error with "Nonexistent function … in base
  'Callable'" — same break-on-error pin (this cost two more EV-0053 instances,
  pids 140553 and 141380). Connect the signal by name: `ui.connect("toast",
  cb)`. MindHud has no `toast` method, so `h.toast.connect(cb)` is safe there.
- Capture-edge toast read-back: in two EV-0053 instances, reading
  `OverlayLayer` children 0.7–0.8 s after `MindCampaign.capture_sector()` showed
  no label even though a recorder on `MindHud.toast` (and later `MindUi.toast`)
  had the event; repeating the edge in an instrumented instance (pid 147174)
  with the recorder and the overlay read in the **same eval immediately after**
  the capture call showed the label ("Sector [accent]groundZero[white]
  Captured!") at all three points. Assert transient toasts on signal recorders
  plus an immediate same-eval overlay read, not on a post-sleep overlay probe.
- Breaking the player core with `SimHost.break_block(129,55)` removed the core
  building (group `build` 61 → 60, remaining core tiles rejected as empty) but
  `get_hud_state().hasCore` stayed true for 400+ ticks with the pump running,
  so the `sector.lost` HUD toast edge never fired. Core-count bookkeeping is a
  campaign-runtime seam, not a HUD producer gap; plan core-loss toasts through
  a scenario that actually lands a `GameOver`/`SectorLose` event.
- Evidence: `runs/20261008-192317-ev0053-hud-wave-enemies/` (EV-0053
  godot-pass, pid 147174; sequence now in `chains/hud-wave-enemies-skip.md`).

## 2026-10-08 — EV-0063: a dialog `shown()` callback into `MindUi` poisons the node; empty loop-2 atlas pack

- `MindUi.open_dialog` holds a mutable Rust bind for the whole call, so a
  dialog's `shown()` must never call back into `MindUi` (`is_mobile()`,
  `dialog_stack()`, …): the reentrant `Gd<T>::bind()` panics "failed, already
  bound", the eval that triggered it (and every later `MindUi` call) times out,
  and the process must be restarted — `godot_game status` still reports
  `runtime_connected: true` and healthy fps while the UI host stays wedged.
  Defer the callback out of the bind (`call_deferred`, the EV-0061/EV-0063
  idiom) or read the value in `_ready`. Evidence:
  `runs/l2-20261008-193444-ev0063-paused-dialog-godot/` (pre-fix panic on pids
  160535/161231; `godot_log errors` shows the panic and crash reports).
- The loop-2 worktree's `assets/` did not carry the packed sprite atlas, so
  `MindAssets.load_assets()` returns before `load_bundle`: `bundle_get(
  "objective")` echoed the raw key and the headless `ui_widgets_check.gd` failed
  10 asset/credits assertions (`[assets] ready ok=false … regions=0`). Copying
  the generated `assets/sprites/sprites.atlas.json` + `sprites.png`/`sprites2-4.png`
  from the main checkout into the worktree (same `inputsHash`) made
  `[assets] ready ok=true pages=4 regions=5135` and the check `failed=0`.
  Generated/gitignored content; do not commit it.
- `start_sector('serpulo',170)` (generated, no core) game-overs before an Escape
  can open the pause dialog; pause in the same eval as the launch, or launch a
  preset sector (`serpulo:15` = groundZero, which also carries the `@objective`
  description). Sequence: `chains/paused-dialog-buttons.md`.

## 2026-10-08 — EV-0024: `godot_log get count=N` returns the oldest N, not the newest

- After a long verification window, `godot_log {"action":"get","params":{"count":5}}`
  returned the first five buffered entries (the whole window's oldest lines), not
  the last five; the fresh backend lines (`imported … restarting`) looked missing
  until `count` was raised past the buffer size. `godot_log errors` is unaffected.
  Ask for `count` >= the buffer size when reading a `[ui]` backend log tail.
- Same run: the Rust `MindUi.data_export/import/export_crash_logs` endpoints take
  literal filesystem paths; passing `user://…` from an eval wrote a relative
  `client/user:/…` directory until the path was `ProjectSettings.globalize_path`d
  first. The button flow is unaffected (the native chooser yields absolute paths).
- Evidence: `runs/l2-20261008-195122-ev0024-gamedata-godot/` (EV-0024 godot-pass,
  pid 180250/186793; sequence now in `chains/settings-gamedata-actions.md`).

## 2026-10-08 — EV-0028: mutation + `get_global_rect()` in one eval reads the pre-layout center

- Resolving a runtime cell's center immediately after mutating scroll state in
  the same eval returns the position from before the re-layout:
  `body.scroll_vertical = 0` followed by
  `content-copper.get_global_rect().get_center()` reported `y = -64` (the scrolled
  position); a second eval one frame later returned the correct `(331, 316)` and
  the click landed. Resolve centers in a follow-up eval after any scroll, tab
  switch, or dialog rebuild — same class as the accumulated-input flush rule.
- The menu/submenu Buttons keep their text on a child Label row, so
  `button.text` is empty for the sidebar entries; index the child row or read
  `get_child(0).get_child(1).text` instead of matching on `text`.
- Evidence: `runs/l2-20261008-201236-ui_dialogs-godot/` (EV-0028 godot-pass,
  pid 214901; sequence now in `chains/database-grid-audit.md`).

## 2026-10-08 — EV-0059 twin Godot leg: no new friction

- The Godot leg followed `chains/host-match-from-pause.md` exactly (fresh editor
  via `run-godot-editor.sh`, `game.tscn`, pid 33491, 1152x648) with no new
  tooling issues: clicks landed, the screenshot was non-stale
  (`frames_drawn` 17014), and `godot_log errors` was `[]`. Nothing to add
  beyond the existing entries. Evidence:
  `runs/20261008-105728-ev0059-host-twin/godot/`.

## 2026-10-08 — EV-0060 twin Godot leg: embedded-game `frames_drawn` is not a staleness gate; list-rebuild eval pitfalls

- `Engine.get_frames_drawn()` stayed pinned at 2 across the whole
  editor-embedded game session while `Engine.get_process_frames()` grew,
  `get_window().grab_focus()` changed nothing, and `godot_game stop` + `play`
  did not change it either — yet `godot_screenshot game` returned live,
  state-current frames (menu → submenu → join dialog → typed address all
  differed and showed the current state). Do not use a pinned `frames_drawn`
  alone to declare captures stale for the editor-embedded game; cross-check the
  frame content/metrics.
- Do not rebuild a dialog list and read its children in the same eval:
  `_rebuild_public()` followed by `_global_list.get_child(0).…` errored
  (`Index p_index = 0 is out of bounds`) and tripped break-on-error (debugger
  `paused: true`; `resume` no-ops — recover with `godot_game stop` + `play`).
  An eval body containing `await` wedged the same way.
- The join dialog's public list only rebuilds in `shown()`: after the connector
  becomes `browsing` with the dialog open, the visible list still renders the
  pre-connect state. The reliable visible check is Back → Play → Join Game:
  the reopened dialog rendered 2 live rows (`spine survival 1/8`), and
  `_global_list.get_child_count()` returned 2.
- Evidence: `runs/20261008-112749-ev0060-join-twin/`
  (`godot/state-connect-56882.json`, `godot/step-07-public-list.png`,
  `godot/godot-runtime.log`).

## 2026-10-08 — EV-0061 twin: pinned frames_drawn returned a stale boot-menu screenshot

- Both twin instances (primary pid 78253 and a fresh pid 80270 after stop+play)
  had `frames_drawn` pinned at 2-4 while `Engine.get_process_frames()`
  advanced (1854→4230, 5065→5632) and `godot_game status` reported fps=145 /
  draw_calls=28-38. `godot_screenshot game` returned the boot main-menu frame
  even while the game-over dialog node was visible with `hud_visible=false`;
  the writer's earlier screenshot at afd380f shows the same stale menu. So for
  the editor-embedded PIE window a pinned `frames_drawn` can still mean the
  presented image is stale — read structured state (dialog node
  labels/visible, MindUi stack) for the verdict and note the caveat. This is
  the mirror of the EV-0060 entry above where the same pinned counter still
  produced live captures; cross-check the frame content.
- Reading a dialog's action button: `Button.text` is empty because
  `MindWidgets.icon_button` nests an HBox with icon+label Labels; use
  `button.find_children("*","Label",true,false)` and
  `Array.map(func(l): return str(l.text))`. The game-over dialog's campaign
  action read back as `["", "Continue"]` with one `pressed` connection
  (`chains/game-over-loss.md`).
- Evidence: `runs/20261008-215055-ev0061-game-over-twin/`
  (`godot/state-05-dialog-node.json`, `godot/step-01-game-over-dialog.png`).

## 2026-10-08 — EV-0038 twin: EditorDialog shell makes a re-opened editor_maps list input-dead

- Re-opening the maps list with `MindUi.open_dialog("editor_maps", "{}")`
  while `/root/Spine/Ui/EditorDialog` is visible leaves the list present but
  unable to receive clicks: a row click at the resolved center (572,99) did
  nothing and `MindEditor.status` kept the previously loaded map.
  `gui_get_hovered_control()` at that point returned
  `/root/Spine/Ui/EditorDialog/MapView` — the shell is a full-rect
  (1152x648) node layered above the UiRoot DialogLayer, so it eats the press
  even though the row button's `get_global_rect().get_center()` still resolves.
- Workaround: exit the editor with the shell's
  `/root/Spine/Ui/EditorDialog/LeftTools/BackButton` (center 181,52), then the
  reopened list receives input and the first-row click works (caldera.msav
  256x256, `last_error {}`). The chain's old variant note claiming
  `MindUi.open_dialog` is a valid back-out was corrected in
  `chains/editor-maps-row-click.md`.
- Boot note for this run: `godot_log errors` carried two `[W]` entries
  (leftover `launchid.dat` from a previous unclean stop; "menu background 1224
  tiles missing regions"). The second appears in 43 unrelated run logs and the
  first is a launch artifact; neither is EV-0038-related, and no
  editor/save-version errors were present.
- Evidence: `runs/20261008-121029-ev0038-editor-row-twin/`
  (`godot/state-03-row-click-archipelago.json`, `godot/state-04-first-row-caldera.json`,
  `godot/log-errors.json`).

## 2026-10-08 — loop-1 EV-0039 twin Godot leg: clearing the log after boot loses the boot banner

- `godot_log clear` right after `godot_game status` -> `runtime_connected`
  drops the boot lines (`[I] MindPreview ready (18 maps)`); while the client
  idles in the menus no new lines are emitted, so a later `godot_log get`
  returns `[]` and `errors` returns `[]`. The banner only reappears on a fresh
  boot: capture `godot_log get` before clearing, or stop+play and read it (done
  here at pid 104609).
- `Engine.get_frames_drawn()` read 14 while process frames advanced and
  `godot_screenshot game` returned three distinct, current dialog frames
  (Custom Game grid at pid 99383, Editor map list, MapPlay). This matches the
  EV-0060 datapoint in this file: a small/pinned-looking counter alone is not
  proof of stale captures — cross-check successive captures against the
  structured state rather than trusting the counter.
- Evidence: `runs/20261008-221916-ev0039-twin/` (`godot/game.log`,
  `godot/step-01-custom-dialog.png`, `godot/step-02-editor-maps.png`,
  `godot/step-03-map-play.png`).

## 2026-10-08 — loop-1 EV-0051 twin Godot leg: live model vs snapshot panel widget

- `MindDialog.campaign_views()` re-fetches `MindCampaign.campaign_views_json`
  on every call (`mind_dialog.gd:315-320`), so structured reads after a state
  change are live immediately. The **built side-panel widget** is a snapshot of
  the rows at build time: after `capture_sector` the dialog's `campaign_views()`
  already returned `captured=true`, but the action button still showed the old
  label until `MindUi.open_dialog("planet","{}")` rebuilt the panel. The
  `campaign-live-views-capture` chain's reopen step is therefore mandatory for
  player-visible assertions; the JSON read alone is not.
- `capture_sector` opens the `restart` (sector captured) dialog on top of
  `planet`; close it with `MindUi.close_dialog("restart")` before reading the
  panel (stack becomes `["planet"]`).
- `godot_log clear` before play then `godot_log errors` reports the startup
  `[W] [platform] previous launch may have crashed (leftover launchid.dat)`
  entry at level `error`; it is a launch artifact of this host, not a scenario
  error. The scenario itself added no error lines (pid 116138).
- The port's planet view (`client/ui/planet_view.gd`) has no wheel zoom or
  per-sector colour/marker rendering (the globe is shader/selected-tile based),
  so Java-vs-Godot pixel comparison of sector ownership colours is not possible
  from screenshots; use the dialog JSON rows plus the panel label.
- Evidence: `runs/20261008-223232-ev0051-campaign-live-views-twin/`
  (`godot/state-01-planet-view-before-capture.json`,
  `godot/state-02-planet-view-after-capture.json`,
  `godot/step-03-gz-owned-attacked.png`, `godot/game.log`).

## 2026-10-08 — loop-1 EV-0053 twin Godot leg: LoadingLayer swallows the first HUD click; toast label timing

- After `start_sector_with_loadout` the HUD is visible but
  `/root/Spine/Ui/UiRoot/LoadingLayer/loading` shows "Loading... 0%" and
  intercepts input (`SimHost.io_pending()` = 1). A discrete `godot_input`
  press/release on the skip button did nothing; `gui_get_hovered_control()`
  named `LoadingLayer/loading/Background` instead of `skip`. `step(30)` drained
  the pending save (`io_pending` 0), the fragment's 0.2 s refresh then hid the
  overlay, and the same discrete click drove the button (wave 0->1, wavetime
  14400->7200). Probe hover before concluding a control is dead.
- `capture_sector()`'s toast label has left `OverlayLayer` within ~0.6 s, so a
  delayed read looks like nothing rendered. The signal recorders still capture
  `MindHud.toast` + the `MindUi` toast, and
  `MindHud.push_toast("probe", "ok")` read in the same eval shows the rendered
  label synchronously — use that for the render hop instead of a sleep.
- Evidence: `runs/20261008-225009-ev0053-hud-twin/`
  (`godot/state-01-baseline-hud.json`, `godot/state-03-capture-toast.json`,
  `godot/step-03-captured.png`, `godot/game.log`); chain updated
  (`chains/hud-wave-enemies-skip.md`).

## 2026-10-08 — loop-2 worktree runtime dir exceeds the 108-byte UNIX socket limit; killed editors leave embedder sockets

- The worktree `.opencode/loops/bin/run-godot-editor.sh` resolves loop vars
  relative to its own location, so a loop-2 launch from
  `Mindustry-Godot-loop2/.opencode/...` uses the worktree loop dir. Godot's
  Wayland embedder socket path there
  (`.../Mindustry-Godot-loop2/.opencode/loops/run/loop-2/wayland/godot-wayland-0`,
  108 chars + NUL) is over the 108-byte limit: the editor logs `socket path
  "..." plus null terminator exceeds 108 bytes` / `Can't connect to a Wayland
  display` and falls back to X11, where the worktree Xwayland `:3` had already
  died (`X connection to :3 broken` killed the embedded game). Launch loop-2
  through the main checkout instead — `$PARITY_MAIN/.opencode/loops/bin/
  run-godot-editor.sh` — whose `.../Mindustry-Godot/.opencode/loops/run/loop-2/
  wayland` dir is 102 bytes and matches the session env/MCP paths.
- A killed (`godot_editor_edit quit` or SIGTERM) editor leaves its
  `godot-wayland-*` socket + `.lock` behind; the next editor's bind fails while
  the path exists (`Can't bind embedding socket`), so remove the stale files in
  `$PARITY_WAYLAND_DIR` before relaunching. Beware: a stale socket can also be
  held by an unrelated desktop process, so `ss -xlp` (not the file alone) tells
  you whether a listener is live.
- The worktree launch also spawned duplicate `wayland-mind2`/`mind2-java`
  weston compositors under the worktree dir while the main-path ones (session
  env) stayed up; the duplicate java weston's Xwayland died mid-run. Use the
  main wrapper so the session's compositors are reused.
- Evidence: `$PARITY_LOOP_DIR/logs/editor.log` 2026-10-08 20:30-20:35;
  `runs/l2-20261008-203338-input_controls-godot/`.

## 2026-10-08 — hints panel swallows center clicks; edge pan drifts the camera between evals

- After ~8 s of playtime the HUD `hints` fragment shows its full-screen
  `Panel` with `mouse_filter` STOP; `MindInput.ui_captures_at` then drops world
  presses at the point, so a left tap on a placed building looks dead
  (`block_config` stays hidden, `action_count` unchanged). Exhaust the
  catalogue before clicking (`hints.set("_next", 27)` + `complete_hint()`) or
  click off-center; one eval probing STOP hits at the point
  (`find_children("*", "Control", true, false).filter(...)`) localizes it.
- On the headless compositor the OS pointer parks at a window edge, so
  `MindCamera2D` edge-pans every frame: the camera drifted from (128,128) to
  (-3947,-3963) between two evals because every mouse injection was overridden
  by auto-pan. Call `Camera2D.set_process(false)` before `center_on_tile` and
  resolve `tile_to_screen` in a **later** eval (the canvas transform updates at
  frame end, so the same-eval point after a recenter is stale); restore
  `set_process(true)` before teardown.
- Reusable sequence in `chains/block-config-fragment.md`; evidence
  `runs/l2-20261008-203338-input_controls-godot/` (pid 249159; fragment
  `_config.options` 22 after the fix).
