# Twin-verification continuation (handoff 2026-11-02)

The native-Rust UI re-verification of all 450 `TWIN-VERIFICATION.json` items is
paused for a session handoff. Read this page, then continue the sequential
checker loop. Do not commit, do not switch branches/worktrees, and do not run
agents in parallel: the working tree already contains this session's fixes.

## State at handoff

- **23 / 450 terminal.** 18 `reference-verified`, 5 `twin-verified`
  (`PAR-0008`, `0010`, `0011`, `0018`, `0019`).
- **Next start: `PAR-0024`.** Terminal: `PAR-0001..0007`, `PAR-0009`,
  `PAR-0012..0023`.
- Non-terminal remainder: 391 `verified`, 16 `fixing`, 8 `seeded`,
  6 `mismatch`, 5 `wontfix`, 1 `reference`.
- Evidence folders: `runs/PAR-####/` for every checked item (this session);
  `runs/archive/**` is frozen and untouched.
- Handoff detail and the exact working-tree list: `runs/TWIN-HANDOFF.md`.
- `python3 tools/twin_verification.py check` → `ok: 450 items, 49 recipes,
  5 gaps, 25 image sets`.

## What landed (uncommitted, keep it)

Docs/contract:

- All non-addon code is `.rs` or `.tscn`; the rule is stated in the root
  [`AGENTS.md`](../AGENTS.md), [`client/AGENTS.md`](../client/AGENTS.md),
  [`client/rust/AGENTS.md`](../client/rust/AGENTS.md) and the rewritten
  [`client/ui/README.md`](../client/ui/README.md).
- Per-item runs layout: [`docs/TWIN-RUNS.md`](TWIN-RUNS.md),
  [`tools/twin_item.sh`](../tools/twin_item.sh), `tools/AGENTS.md` row,
  `tools/twin_verification.py` docstring/status set.
- The `parity-eval` skill §3c and the runs-layout row are updated (workspace
  `.claude/skills/parity-eval/SKILL.md`).

Fixes (all verified with fresh Java+Godot legs):

| Item | Fix |
|---|---|
| PAR-0008 | `DiscordDialog` rebuilt to `DiscordDialog.java` (no title, 520x70 box, colour bar + 70 px glyph + parsed accent text, 170x50 icon buttons); menu glyphs 26→30 px. |
| PAR-0010 | Chooser caption `line_separation` -8→0 and `pad_top` 4. |
| PAR-0011 | Selected-caption `autowrap_mode` applied after `root.add` (the `MindLabel::ready` reset trap). |
| PAR-0018 | `planet.rs::row_array` accepts gdext typed read-model arrays; planet page `Panel` mouse_filter=PASS; `lookAt(startSector)` after chooser OK. |
| PAR-0019 | `MindUiRoot::on_node_added` re-entrancy panic hardened: the signal listener only queues an `InstanceId`; `process` drains and hooks buttons outside the borrow (unit test added). |

## Environment

- Display `:10` (Xvfb). Editor bridge `ws://127.0.0.1:6970`. If the bridge is
  down: `tools/parity-session.sh` (session 1) starts Xvfb + the editor and waits
  for the port. If only the game is down: `godot_game play` with the explicit
  `{"scene":"res://scenes/game.tscn"}`; never eval in the same round-trip.
- Java reference: `../Mindustry/desktop/build/libs/Mindustry.jar`, JDK 17,
  data dir `parity-java-home`, parity venv
  `$HOME/.local/share/parity-eval/venv/bin/python`, helpers under
  `/home/clintonnguyen/g/code_examples/.claude/skills/parity-eval/scripts/`.
  Launch detached with `DISPLAY=:10` and the three XDG dirs under
  `parity-java-home`, then `unmaximize` → `resize 1152 648` → `move`.
  Back up `settings.bin` + `saves/` before profile surgery and restore after.
- Capture helper: `tools/twin_item.sh godot-shot PAR-#### <label>` /
  `java-shot`; reports at `runs/PAR-####/<stamp>-run.md`. In-flight navigation
  screenshots may stay in `/tmp`.
- After a Rust change: `bash tools/build.sh`, then kill + relaunch the editor
  (the extension is mmap'd) and play the scene again.

## Known residuals / traps

1. **PAR-0024..0027 are blocked on campaign progression, not a code defect.**
   The Configure Loadout dialog needs a calm *based* launcher and an unlocked
   base-less destination (`findLaunchCandidate`). A first-run profile only has
   groundZero selectable, so a free launch skips the dialog (correct). See
   `runs/PAR-0024/20261102-143108-blocked.md` and `runs/TWIN-HANDOFF.md` §1.
   The dialog can be sanity-checked with
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

1. Read the first item whose status is not `reference-verified`/`twin-verified`.
2. Spawn ONE checker subagent (prompt below) at that item, capped at ~12 items.
3. The checker walks Godot-only, writes per-item evidence, and stops at the
   first gap. It spawns exactly one nested fixer, waits, verifies the fix, then
   stops.
4. Launch the next checker at the new first non-terminal item. Repeat until all
   450 items are terminal.

## Checker prompt template

> You are the TWIN CHECKER for /home/clintonnguyen/g/code_examples/Mindustry-Godot.
> Work only in this main checkout (branch main); never switch branches/worktrees,
> never commit, never use the `workflow` tool, never run agents in parallel.
>
> Read `docs/TWIN-RUNS.md`, `AGENTS.md`, and the `parity-eval`/`playtest` skills
> (or their files under `.claude/skills/`). The UI is native Rust; all non-addon
> code is `.rs`/`.tscn`. The 450-item queue is `TWIN-VERIFICATION.json`; every
> item must end `reference-verified` or `twin-verified`.
>
> Environment: editor + MCP bridge 6970, game at main menu, display :10. Java is
> stopped; do not launch it in the checker role. `godot_game play` needs the
> explicit scene; no eval in the same round-trip. Use `tools/twin_item.sh` for
> evidence; in-flight screenshots may stay in `/tmp`.
>
> Start at `<FIRST NON-TERMINAL>`. Walk upward, up to 12 items, STOP EARLY at
> the first gap. You may drive the game and update an item's `fork`/`action`
> chains when a run finds a better re-runnable sequence.
>
> For each item: establish the fork (restart when unsure; campaign profile state
> matters); replay the Godot fork/action with real input; capture verdict
> frame(s) to `runs/PAR-####/godot/`; compare with the archived frame
> (`parity/twin/PAR-####/`, `runs/archive/**`) or the recorded acceptance +
> upstream Java source + structured state. `godot_log errors` must be empty.
>
> PASS → set `reference-verified`, write a short `runs/PAR-####/<stamp>-run.md`,
> continue. ISSUE → stop, leave a non-terminal status (`mismatch`), and spawn
> exactly ONE nested fixer with a complete brief: item id/title, symptom, exact
> reproduction, evidence, Java+Godot requirement, fix+rebuild+relaunch steps,
> the per-item layout, permission to drive/update chains, and `twin-verified`
> only after a fresh Java leg and fresh Godot leg match. Wait for it, verify its
> status/evidence, then stop and report.
>
> Guardrails: no source edits/build in the checker; no Java; no commits; keep
> reports short; leave the game at the main menu; run
> `python3 tools/twin_verification.py check`. Log hiccups in
> `open-godot-mcp-learnings.md`. Final reply:
> `start=<PAR> next=<PAR> checked=<n> gap=<PAR|none> fixed=<yes|no|n/a>` plus
> ≤3 short lines; details go in the item files.

The fixer brief must additionally require: back up/restore the Java profile,
run both legs at the same fork, rebuild + relaunch after a Rust change, write
`runs/PAR-####/{java,godot}/` + a run report, add a unit/oracle check when the
change is testable, and set `twin-verified` only after the fresh pair matches.

## Paste-ready opening prompt

```
Continue the twin-verification sweep in /home/clintonnguyen/g/code_examples/Mindustry-Godot (branch main; the working tree intentionally contains this session's fixes — do not commit or revert them). Read AGENTS.md, docs/TWIN-RUNS.md and docs/TWIN-CONTINUATION.md first, then run the sequential checker loop described there starting at the first item whose status is not reference-verified/twin-verified (currently PAR-0024). Spawn one checker subagent at a time; each walks up to ~12 items Godot-only, stops at the first gap, spawns exactly one nested fixer (Java+Godot legs, fix, twin-verified), then the next checker continues. No parallel agents, no branches/worktrees, no commits. Keep replies tiny; write details to runs/PAR-####/ and TWIN-VERIFICATION.json. Done when all 450 items are reference-verified or twin-verified.
```
