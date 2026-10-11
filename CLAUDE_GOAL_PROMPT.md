# Claude Code goal prompt: Mindustry-Godot 1:1 port

Start Claude Code from `Mindustry-Godot/` so `.mcp.json` (open-godot-mcp) and
`.claude/skills/` (parity-eval, playtest) load. Then paste everything below the
line. The leading `/loop` with no interval makes the session self-pace until
the goal is met.

---

/loop Goal: make Mindustry-Godot (this repo) a full 1:1 port of ../Mindustry (Java reference, upstream pinned in parity/upstream.lock). Work autonomously, one unit of work per iteration, until every item in parity-ledger.json is `verified` or `wontfix` (with a justified reason) AND a fresh seed sweep of every surface (menu, every dialog, campaign, planet page, in-game HUD, every block family, units, editor, multiplayer) finds no new unverified items. Only then stop the loop.

## Ground rules (read once per session, re-read when unsure)
- Use the `parity-eval` skill as the process contract and the `playtest` skill for Godot MCP mechanics. Also read AGENTS.md, client/AGENTS.md, client/rust/AGENTS.md, PROGRESS.md, java-leg-learnings.md, open-godot-mcp-learnings.md. The skills mention opencode's /goal and /loop; in Claude Code this /loop session replaces them. Godot MCP tools appear as `mcp__open-godot-mcp__godot_*`.
- Run both clients at the same time on display :10 and compare them visually: the Java reference (../Mindustry/desktop/build/libs/Mindustry.jar, launched per parity-eval §0 with parity-java-home/) and the Godot client (editor + `godot_game play` with scene res://scenes/game.tscn). Look at both frames yourself (Read the PNGs) in addition to frame_diff.py metrics. Stored reference frames are fine for `reference` items (§3b), but boot Java whenever the reference is missing, stale, or you need to see how something behaves.
- Java profile: the fully-unlocked tech-tree state lives in ~/.local/share/Mindustry (see AGENTS.md "Java reference profile state"). Never re-import through the UI and never clear Game Data / research / saves.
- Log every hiccup in the matching learnings file the moment it happens (parity-eval §0).
- Commit per the skill: code fix first (`Fixes PAR-####`), docs/ledger in a separate commit. Do not push.

## Session start (every fresh session, and after a reboot)
1. `tools/parity-session.sh` (no --run; it would exec opencode). It starts Xvfb :10 and the Godot editor with the bridge on 6970. If it fails, launch per parity-eval §0 manually.
2. `.opencode/skills/parity-eval/scripts/bootstrap.sh --check`; `ls ../Mindustry/desktop/build/libs/Mindustry.jar`.
3. `godot_health check` → `godot_editor_read state` (project_path contains mindustry-godot) → open + play res://scenes/game.tscn → `godot_game status` runtime_connected → pid-stamp eval.
4. Launch the Java client (§0), normalize the window to 1152x648, and confirm both main menus with one capture each.
5. `git status`; finish or commit anything left uncommitted from a previous session (the ledger may have uncommitted edits; PAR-0140 is mid-fix with a resumable `fix.plan`).

## Picking work each iteration (priority order)
1. Any item in `fixing`/`in-progress` with a `fix.plan` → resume it (PAR-0140 first).
2. `mismatch` items.
3. Ledger `focus` (currently area `campaign` / Play > Campaign): in-scope non-verified items. When none remain, set `focus` to null.
4. `reference` items (Godot walk against stored Java evidence, §3b); most of these are real missing features found by the code scan, so expect to implement them.
5. `seeded` items, then `blocked` items whose blocker is now resolved. For multiplayer/mod/export blockers, build the missing fixture (second client, a test mod, the export pipeline) instead of leaving them blocked.
6. Queue empty → seed sweep the next unswept surface (§2), which creates new items.
Before running an item, claim it under the flock (§1).

## How to use subagents (required)
You own the live clients. Only one agent drives display :10, the Java client, and the Godot editor at a time, so you do the in-engine walks, captures, diffs, and final verification. Delegate the code work to subagents (`general-purpose`, model opus) with very detailed plans. Write every plan into the item's `fix.plan` first, then pass it to the subagent in full. A subagent prompt must contain:
- **Context**: item id, title, player-visible symptom, and the evidence: Java and Godot frame paths, diff metrics, and the structured-state values that differ.
- **Java ground truth**: exact ../Mindustry file paths and line ranges for the behavior (e.g. core/src/mindustry/world/blocks/defense/turrets/Turret.java:NNN-MMM), quoted or summarized line by line: constants, formulas, update order, edge cases, draw layers, bundle keys.
- **Godot target**: exact files and functions to change in client/ (GDScript) or client/rust/ (crate, module, struct, fn), where the new code hooks into the existing pipeline (system registration, tick order, #[func] exposure, UI scene/node path), and existing helpers to reuse.
- **Step-by-step implementation**: numbered steps naming the file, function, and change. Include data shapes and determinism constraints (fixed 60 Hz, FNV checksum stability, no floating nondeterminism in sim).
- **Tests to add and run**: exact `cargo test -p <crate> <name>` / clippy commands, new headless scenario or golden if the sim changed (and which existing goldens must remain unchanged), plus `tools/build.sh`.
- **Constraints**: do not touch the editor or display :10, do not launch clients, do not edit parity-ledger.json, do not commit. Stay inside the listed files unless the plan is wrong, and if it is, stop and report why.
- **Report back**: files changed, the diff summary, test output, anything uncertain.
Independent fixes in disjoint files may run as parallel subagents (use `isolation: "worktree"` when they touch overlapping crates, then merge). Research subagents (Explore) can map Java↔Godot code for the next items while you walk the current one. After a subagent returns: review its diff yourself, run `tools/build.sh`, relaunch the editor (mmap'd lib), re-run the item's full chain in-engine under the §4 gate, and only then commit and mark it verified. A subagent's word is never verification.

## Each iteration ends with
Ledger updated (status, steps, frames, fix, verified), run report in runs/<stamp>-<item>/report.md, commits made, PROGRESS.md headline numbers refreshed every ~10 items, then schedule the next wakeup right away (short delay; this is continuous work, not polling). If truly blocked on something only the user can do (credentials, hardware, a product decision), record it on the item, move on to other work, and list it in your next status message.
