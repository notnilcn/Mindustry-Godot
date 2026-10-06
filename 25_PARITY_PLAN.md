# 25 — PARITY CAMPAIGN PLAN (ACTIVE)

> **Status:** ACTIVE — starts and drives the in-engine parity feedback loop.
> A fresh agent session reads this file and runs the next milestone. Session
> state lives in `.opencode/evals/findings.json` and the newest directory under
> `.opencode/evals/runs/`, so any session can resume mid-campaign.

## 1. Start a session

Interactive (recommended — you can watch and intervene):

1. Open a terminal in the repo root and start `opencode`.
2. Paste this prompt:

   > Read `25_PARITY_PLAN.md`. Run the next unfinished milestone in order.
   > Use the `evaluator` subagent for all in-engine evaluation, and record
   > findings in `.opencode/evals/findings.json`. Fix one `EV-####` at a time
   > and have the evaluator re-verify the exact repro before moving on. Stop
   > after the session budget in §6 and report where you stopped.

Non-interactive:

```bash
opencode run "Read 25_PARITY_PLAN.md and execute the next unfinished milestone. Follow the nearest AGENTS.md for every change."
```

The loop primitives this plan orchestrates:

| Entry point | Half | What it does |
|---|---|---|
| `/parity-eval [scope]` | evaluate | Twin-run the Java reference and the Godot client, record evidenced gaps, update the ledger. |
| `/parity-loop [scope]` | fix + verify | Evaluate if needed, fix the top `EV-####`, then have the evaluator re-verify that exact repro. One finding per run. |

## 2. Roles and contracts

| Role | Who | Writes | Never |
|---|---|---|---|
| Evaluator | `evaluator` subagent (`.opencode/agent/evaluator.md`) | `.opencode/evals/**` only | game code, tests, scenes, config |
| Implementer | the primary session agent | game code + tests | the ledger (claims fixes in commit messages as `Fixes EV-####`) |

- **Evidence or it did not happen.** Every finding points at a screenshot, state
  JSON, log excerpt or checksum under its run directory.
- **Only the evaluator marks `verified-fixed`**, and only after re-running the
  finding's exact repro in a fresh engine run.
- **Docs are claims.** `parity/system_checklist.md`, `parity/reports/gate_*.json`
  and READMEs are inputs to decide where to look, never proof of parity.
- One game client at a time: Java leg first, quit it, then the Godot leg.

Read `.opencode/skills/parity-eval/SKILL.md` for the full protocol and
`.opencode/skills/playtest/SKILL.md` for the Godot MCP launch flow, node map and
host gotchas (input flush fallback, screenshot staleness, `godot` binary name).
Do not duplicate those recipes in this plan.

## 3. Milestone 0 — bring-up (do this once)

| Step | Command / check | State |
|---|---|---|
| MCP preconditions | `MCP_VENV=... .opencode/skills/parity-eval/scripts/bootstrap.sh --check` | [x] |
| Rust client built | `tools/build.sh` produces `client/bin/rust/debug/libmind_gdext.so` | [x] |
| Java reference built | `../Mindustry/desktop/build/libs/Mindustry.jar` exists | [x] |
| Editor + bridge | launch editor, `godot_health check` → `bridge_connected: true`; `godot_editor_read state` → project is Mindustry-Godot | [x] |
| First twin-run | `/parity-eval boot_menu` writes a report and the ledger's first records | [x] |

M0 exit: the `boot_menu` report exists with non-blank captures from both legs,
and `findings.json` is no longer empty.

## 4. Milestones — player-visible flow order

Priority is what a player hits first; within a milestone, S1/S2 gaps jump the
queue. Scenario ids come from `.opencode/skills/parity-eval/SKILL.md` §7 and
`parity/mcp_catalog.json` (catalog wins where ids overlap).

| Milestone | Goal | Scenarios | Exit |
|---|---|---|---|
| M1 — menu & settings | Boot, logo/background, button tree, submenus, settings tabs/apply, language | `boot_menu`, `settings_ui`, `ui_dialogs` | Screens/OCR match the Java reference at the same window size; every submenu/dialog opens and closes |
| M2 — campaign entry & HUD | Play → Campaign → planet → sector → launch; core items HUD, wave counter, catalog, minimap, toolbar | `campaign_launch`, `hud_ingame` | A sector reaches playable HUD state; HUD values match Java for the same seed/sector |
| M3 — building & economy | Select/place/rotate/configure/break; drill→conveyor→core flow; power graph | `placement_basic`, `logistics_flow`, `power_grid`, `input_zoom_pan`, `save_load` | Structured state (item counts, power, tick/checksum) matches; save→load reproduces state |
| M4 — combat, units, logic | Wave summon, turret fire, HP/damage/death FX; spawn/move/attack orders; processor program | `combat_wave`, `units_move`, `logic_program` | In-engine behavior matches the Java reference and the committed headless goldens |
| M5 — breadth & platform | Map editor, mods, multiplayer relay, export surfaces | `editor_basic`, `mp_host_join` + per-layer render/FX sweep | Each scenario has a report; residual gaps are ledgered with severity and owner |

## 5. Per-finding loop

1. **Evaluate** the milestone scope with the `evaluator` subagent (Java leg
   first, then Godot). It writes `runs/<stamp>-<scenario>/report.md` and adds
   `EV-####` records.
2. **Pick one**: `python3 .opencode/skills/parity-eval/scripts/record_finding.py list --status open`
   — highest severity, oldest first, scoped to the current milestone.
3. **Fix it** following the nearest `AGENTS.md`; smallest change that restores
   the expected behavior, plus the smallest test that would have caught it.
   Parity-pinned ABI (content ids, checksums, sprite names) is append-only.
4. **Check it** with the narrowest command (`cargo test -p <crate>`,
   `tools/ci.sh` for cross-cutting).
5. **Claim it** in the commit message (`Fixes EV-0001`). Parity fix commits are
   pre-authorized: commit each verified fix directly without asking, and never push.
6. **Re-verify**: evaluator re-runs the finding's exact repro and updates the
   ledger to `verified-fixed` (or `regression`). No claim without a fresh repro.
7. Repeat from §5.2 until the milestone exit criteria hold.

`/parity-loop` automates steps 1–6 for one finding. `wontfix` requires a note
saying why (upstream deviation accepted, platform limitation, duplicate).

## 6. Session budget and stop conditions

- Run up to **three parallel loops** with disjoint scopes
  (`.opencode/loops/`, `/parity-parallel`). Each loop evaluates at most
  **3 scenarios** and completes at most **5 fix/verify iterations** per
  session; then provide a continuation prompt so the task can be handed off in
  a new session and stop. Quality over throughput; every claim must survive
  re-verification.
- Stop immediately on: MCP bridge down, missing display/Java, a corrupted
  ledger, or an evaluator/fixer deadlock (two rounds without new evidence).
- Never leave a finding half-claimed: if the fix is unverified, say so in the
  summary; the ledger stays `open`/`in-progress`.
- Resume by reading `record_finding.py summary`, the open S1/S2 list, and the
  newest run directory. The first unfinished milestone in §4 is the queue.

## 7. Exit criteria (campaign complete)

- Every §4 scenario has a run report (parity, gaps, or explicitly blocked with
  the blocker recorded).
- No `open` S1/S2 findings; S3/S4 are `verified-fixed` or `wontfix` with notes.
- `tools/ci.sh` is green at the tip, and `parity/system_checklist.md` is
  updated from in-engine evidence, not from code inspection.

## 8. Progress tracker

| Milestone | Status | Last run | Open S1/S2 | Notes |
|---|---|---|---|---|
| M0 bring-up | done | 20261005-150056-boot_menu | — | First twin-run complete; 8 findings filed. |
| M1 menu & settings | in-progress | 20261006-085223-ev0023-verify2 | none | `boot_menu`, `settings_ui`, `ui_dialogs` evaluated; EV-0001/0002/0003/0005/0009/0019/0023/0026 verified-fixed. Open: EV-0004 (S4), EV-0006/0007/0008 (About, S3), EV-0020/0021/0022/0024/0025/0027/0028 (settings/dialog layout, S3). |
| M2 campaign & HUD | in-progress | 20261006-071416-ev0018-verify | none | EV-0016 (launch camera snaps to core at default zoom 4) and EV-0017 (block picker renders 46px block / 50px category icon buttons) verified-fixed; EV-0018 (UI clicks leak into world place/break, S2) verified-fixed. EV-0010/0011/0012/0014/0015 stay verified. Open: EV-0013 (icons render; catalog still lacks unlock/empty-category filtering, S3). |
| M3 building & economy | not-started | — | — | |
| M4 combat, units, logic | not-started | — | — | |
| M5 breadth & platform | not-started | — | — | |
