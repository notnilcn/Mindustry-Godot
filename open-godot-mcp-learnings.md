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
