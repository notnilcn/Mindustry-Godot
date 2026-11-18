<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Twin-verification continuation (Linux handoff)

This page is the Linux environment and handoff note for the packaged twin
workflow. The workflow itself lives in
[`tools/ai-tools/twin/WORKFLOW.md`](../tools/ai-tools/twin/WORKFLOW.md); the
repo-specific counterpart and capture commands are in
[`tools/TWIN-COUNTERPART.md`](../tools/TWIN_COUNTERPART.md). For evidence paths
and terminal statuses see [`TWIN-RUNS.md`](TWIN-RUNS.md).

## State at handoff

The registry is `TWIN-VERIFICATION.json` (616 items, `java`/`godot` legs). It
contains older terminal and in-progress records; never rewind a terminal item.
Get the current frontier rather than trusting a count in this file:

```bash
python3 tools/ai-tools/twin/twin_verify.py stats --root .
python3 tools/ai-tools/twin/twin_verify.py next --root .
```

Evidence for checked items is under `runs/PAR-####/{java,godot}/`;
`runs/archive/**` is frozen and untouched.

## Environment

- Display `:10` (Xvfb). Editor bridge `ws://127.0.0.1:6970`. If the bridge is
  down: `tools/parity-session.sh` (session 1) starts Xvfb + the editor and waits
  for the port. If only the game is down: `godot_game play` with the explicit
  `{"scene":"res://scenes/game.tscn"}`; never eval in the same round-trip.
- Java reference: `../Mindustry/desktop/build/libs/Mindustry.jar`, JDK 17,
  data dir `parity-java-home`, parity venv
  `$HOME/.local/share/parity-eval/venv/bin/python`, helpers under
  `$HOME/.claude/skills/parity-eval/scripts/`.
  Launch detached with `DISPLAY=:10` and the three XDG dirs under
  `parity-java-home`, then `unmaximize` → `resize 1152 648` → `move`.
  Back up `settings.bin` + `saves/` before profile surgery and restore after.
  `tools/twin.sh` wraps this lifecycle (`twin_launch`, `twin_place_java`,
  `twin_stop`).
- After a Rust change: `bash tools/build.sh` (it self-queues through the
  packaged resource gate), then kill + relaunch the editor (the extension is
  mmap'd) and play the scene again.
- Export the evidence leg names once per shell:

```bash
export TWIN_REFERENCE_LEG=java TWIN_PORT_LEG=godot
```

`tools/parity-session.sh` exports `TWIN_REGISTRY` (the main checkout's
`TWIN-VERIFICATION.json`) and `TWIN_RUNS` for session worktrees. Do not symlink
the registry into a worktree: `twin_verify.py set-status` replaces the path it
is given, so a symlink would fork the queue.

## Working commands

```bash
bash tools/parity-session.sh                    # Xvfb :10 + editor + bridge 6970
tools/godot.sh -e                               # or open the editor directly
python3 tools/ai-tools/twin/twin_verify.py next --root .
python3 tools/ai-tools/twin/twin_verify.py stats --root .
bash tools/build.sh && bash tools/parity.sh     # build + oracle gate (both self-queue)
python3 tools/ai-tools/twin/twin_verify.py check --root .
```

Evidence and status edits go through the packaged helper and validator:

```bash
tools/ai-tools/twin/twin_item.sh dir PAR-0042
tools/ai-tools/twin/twin_item.sh capture port PAR-0042 01-action -- \
  bash -c 'source tools/twin.sh && GS 01-action'
tools/ai-tools/twin/twin_item.sh capture reference PAR-0042 01-action -- \
  bash -c 'source tools/twin.sh && JS 01-action'
tools/ai-tools/twin/twin_item.sh note PAR-0042 run "compared …; matches"
python3 tools/ai-tools/twin/twin_verify.py set-status PAR-0042 reference-verified \
  --note "short verdict"
```

`TWIN-VERIFICATION.json` is written atomically by `twin_verify.py set-status`;
there is no separate transaction helper. Evidence lives under
`runs/PAR-####/` (gitignored, never `runs/archive/**`).

## Known residuals / traps

1. **PAR-0024..0027 are blocked on campaign progression, not a code defect.**
   The Configure Loadout dialog needs a calm *based* launcher and an unlocked
   base-less destination (`findLaunchCandidate`). A first-run profile only has
   groundZero selectable, so a free launch skips the dialog (correct). See
   `runs/PAR-0024/` and `runs/TWIN-HANDOFF.md` §1. The dialog can be
   sanity-checked with
   `MindUi.open_dialog("launch_loadout", {"planet":"serpulo","sector":170,"launcher":15})`.
2. Erekir has no `presets` remap upstream either; one reference marker differs.
3. Campaign sector ids: the globe row for groundZero is **170**; `start_sector(...,15)`
   loads a plain tile 15. Use the dialog path or 170.
4. `MindLabel::ready()` resets `autowrap_mode` to OFF; a wrap set before tree
   entry is lost. Check live; apply after `add`.
5. The `on_node_added` queue/drain and `row_array` typed-array fixes are
   regression boundaries — do not revert them.
6. The canonical AVIF pairs for PAR-0010/0011 are still the pre-fix frames.
   The fresh evidence is in `runs/PAR-0010/`, `runs/PAR-0011/`. Refreshing
   `parity/twin/PAR-####/{java,godot}/01-action.avif` is optional follow-up.

## The loop

The loop is defined in
[`tools/ai-tools/twin/WORKFLOW.md`](../tools/ai-tools/twin/WORKFLOW.md) §6–§9
and the prompt is [`docs/TWIN-CHECKER-PROMPT.md`](TWIN-CHECKER-PROMPT.md).

1. Read the first item from `twin_verify.py next`.
2. Spawn ONE checker subagent at that item, capped at ~10–12 items.
3. The checker walks Godot-only, writes per-item evidence, and stops at the
   first gap. It spawns exactly one nested fixer, waits, verifies the fix, then
   stops.
4. Launch the next checker at the new first non-terminal item. Repeat until
   `twin_verify.py check --evidence` is clean and every item is terminal.

Do not commit, do not switch branches/worktrees, and do not run checkers in
parallel.
