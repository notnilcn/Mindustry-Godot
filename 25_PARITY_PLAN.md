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
   > Use the `gap-identifier` subagent for code-level gap discovery and
   > code-verification, and record findings in
   > `.opencode/evals/findings.json`. Fix one `EV-####` at a time; once the fix
   > is committed and code-verified, have the `evaluator` subagent re-verify the
   > exact repro in-engine. Stop after the session budget in §6 and report
   > where you stopped.

Non-interactive:

```bash
opencode run "Read 25_PARITY_PLAN.md and execute the next unfinished milestone. Follow the nearest AGENTS.md for every change."
```

The loop primitives this plan orchestrates:

| Entry point | Half | What it does |
|---|---|---|
| `/parity-campaign [plan]` | orchestrate | Seed the queue, launch up to three loop runners, watch them, then run the final verification sweep. One command for a whole session. |
| `/parity-gap [scope]` | identify | Fan out code-only `gap-identifier` agents over disjoint areas; add code-sourced candidates to the ledger. |
| `/parity-eval [scope]` | final verify | Twin-run the Java reference and the Godot client; the only path to `verified-fixed`. |
| `/parity-loop [scope]` | fix + verify | Gap-identify if needed, fix the top `EV-####`, code-verify the diff, then have the evaluator re-verify that exact repro in-engine. One finding per run. |
| `/parity-parallel [plan]` | launch loops | Spawn the loop-runner processes from a plan (the launch half of `/parity-campaign`). |

## 2. Roles and contracts

| Role | Who | Writes | Never |
|---|---|---|---|
| Gap identifier | `gap-identifier` subagent (`.opencode/agent/gap-identifier.md`) | `.opencode/evals/**`, `.opencode/chains/**` | game code, tests, scenes, config; `verified-fixed` |
| Evaluator | `evaluator` subagent (`.opencode/agent/evaluator.md`) | `.opencode/evals/**`, `.opencode/chains/**` | game code, tests, scenes, config |
| Implementer | the primary session agent | game code + tests | the ledger (claims fixes in commit messages as `Fixes EV-####`) |

- **Evidence matches the source.** A code-sourced candidate points at
  `../Mindustry` and `client/` file:line pairs plus a repro sketch; an
  engine-sourced finding points at a screenshot, state JSON, log excerpt or
  checksum under its run directory.
- **Two-stage verdicts.** The gap identifier marks `code-verified` after
  reading the fix diff; **only the evaluator marks `verified-fixed`**, and only
  after re-running the finding's exact repro in a fresh engine run.
- **Docs are claims.** `parity/system_checklist.md`, `parity/reports/gate_*.json`
  and READMEs are inputs to decide where to look, never proof of parity. The
  code-audit inventory under `.opencode/evals/` is seed material, not truth.
- **MCP is a capped, shared resource.** At most `MCP_SLOT_LIMIT` (default 2)
  agents drive MCP at once through
  `.opencode/skills/parity-eval/scripts/mcp_slot.py`; a further caller stays
  code-only. The two slots are what the loops and their evaluator legs share.
  computer-mcp additionally needs `DISPLAY` at opencode process start. Within
  one loop, one game client at a time: Java leg first, quit it, then the Godot
  leg.

Read `.opencode/skills/parity-eval/SKILL.md` for the full protocol and
`.opencode/skills/playtest/SKILL.md` for the Godot MCP launch flow, node map and
host gotchas (input flush fallback, screenshot staleness, `godot` binary name).
Project-specific MCP call sequences live in `.opencode/chains/`; consult the
matching chain before composing calls. Do not duplicate those recipes in this
plan.

## 3. Milestone 0 — bring-up (do this once)

| Step | Command / check | State |
|---|---|---|
| MCP preconditions | `MCP_VENV=... .opencode/skills/parity-eval/scripts/bootstrap.sh --check` | [x] |
| Rust client built | `tools/build.sh` produces `client/bin/rust/debug/libmind_gdext.so` | [x] |
| Java reference built | `../Mindustry/desktop/build/libs/Mindustry.jar` exists | [x] |
| Editor + bridge | launch editor, `godot_health check` → `bridge_connected: true`; `godot_editor_read state` → project is Mindustry-Godot | [x] |
| computer-mcp registered | opencode has `computer-mcp_*` tools; `DISPLAY` set at process start (session env or global config `environment`) | [x] |
| MCP slots | `python3 .opencode/skills/parity-eval/scripts/mcp_slot.py status` → `limit: 3`, free slots; leases never stale in the registry | [x] |
| Chains ledger | `.opencode/chains/` present and reachable from the playtest skill | [x] |
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

1. **Gap-identify** the milestone scope with the `gap-identifier` subagent,
   code-only: compare `../Mindustry` with `client/`, add `EV-####` candidates
   (`--source code`) with file:line evidence and a repro sketch, and dedupe
   against the ledger and the code-audit inventory. No engine run.
2. **Pick one**: `python3 .opencode/skills/parity-eval/scripts/record_finding.py claim
   --owner "loop-${PARITY_LOOP:-1}"` — highest severity, oldest first, scoped
   to the current milestone.
3. **Fix it** following the nearest `AGENTS.md`; smallest change that restores
   the expected behavior, plus the smallest test that would have caught it.
   Parity-pinned ABI (content ids, checksums, sprite names) is append-only.
4. **Check it** with the narrowest command (`cargo test -p <crate>`,
   `tools/ci.sh` for cross-cutting).
5. **Claim it** in the commit message (`Fixes EV-0001`). Parity fix commits are
   pre-authorized: commit each verified fix directly without asking, and never push.
6. **Code-verify**: the gap identifier re-reads the fix diff against upstream
   behavior and marks `code-verified` — or releases the finding with the exact
   reason the diff does not close the gap.
7. **Final verify**: only when the fix is committed and `code-verified`, the
   evaluator re-runs the finding's exact repro and updates the ledger to
   `verified-fixed` (or `regression`). No claim without a fresh repro. Batch
   only findings whose repro shares one scenario/state.
8. Repeat from §5.2 until the milestone exit criteria hold.

`/parity-gap` fans step 1 out over areas; `/parity-loop` automates steps 1–7 for
one finding; `/parity-eval` is the final-verification half alone. `wontfix`
requires a note saying why (upstream deviation accepted, platform limitation,
duplicate, or the code reading shows the gap is closed).

## 6. Session budget and stop conditions

- **Wave model.** Gap scans and code-verification are code-only and cheap: run
  many `gap-identifier` agents in parallel (disjoint areas, shared ledger).
  Fixing scales per worktree. In-engine verification is display-bound and is
  the scarce stage — keep it to **three parallel loops**
  (`.opencode/loops/`, `/parity-parallel`) and batch findings that share one
  scenario into a single engine run.
- **MCP cap.** Every agent that uses MCP takes a lease first
  (`.opencode/skills/parity-eval/scripts/mcp_slot.py`, `MCP_SLOT_LIMIT` default
  2); the next caller stays code-only. computer-mcp additionally needs
  `DISPLAY` at opencode process start.
- Each loop evaluates at most **3 scenarios** and completes at most **5
  fix/verify iterations** per session; then provide a continuation prompt so
  the task can be handed off in a new session and stop. Quality over
  throughput; every claim must survive re-verification.
- Stop immediately on: MCP bridge down, missing `computer-mcp_*` tools
  (displayless launch), missing display/Java, a corrupted ledger, a stale or
  corrupt MCP slot registry, or an evaluator/fixer deadlock (two rounds without
  new evidence).
- Never leave a finding half-claimed: if the fix is unverified, say so in the
  summary; the ledger stays `open`/`code-verified`/`in-progress`.
- Resume by reading `record_finding.py summary`, the open S1/S2 list, the
  `code-verified` final-verify queue, and the newest run directory. The first
  unfinished milestone in §4 is the queue.

## 7. Exit criteria (campaign complete)

- Every §4 scenario has a run report (parity, gaps, or explicitly blocked with
  the blocker recorded).
- No `open`/`code-verified` S1/S2 findings; S3/S4 are `verified-fixed` or
  `wontfix` with notes. `code-verified` alone does not close a finding — the
  evaluator's in-engine verdict does.
- `tools/ci.sh` is green at the tip, and `parity/system_checklist.md` is
  updated from in-engine evidence, not from code inspection.

## 8. Progress tracker

| Milestone | Status | Last run | Open S1/S2 | Notes |
|---|---|---|---|---|
| M0 bring-up | done | 20261005-150056-boot_menu | — | First twin-run complete; 8 findings filed. |
| M1 menu & settings | in-progress | 20261006-102909-ev0034-verify | none | `boot_menu`, `settings_ui`, `ui_dialogs` evaluated; verified-fixed through EV-0034 (EV-0025 uiscale apply+confirm, EV-0032 settings panel, EV-0034 prompt markup). Open: EV-0024 (blocked: no ported data-IO/campaign/research/crash-log subsystem for 7 of the 9 Game Data actions), EV-0028 (needs the Core Database UI over the existing unlock model). |
| M2 campaign & HUD | in-progress | 20261006-071416-ev0018-verify | none | EV-0016 (launch camera snaps to core at default zoom 4) and EV-0017 (block picker renders 46px block / 50px category icon buttons) verified-fixed; EV-0018 (UI clicks leak into world place/break, S2) verified-fixed. EV-0010/0011/0012/0014/0015 stay verified. Open: EV-0013 (needs the same unlock/catalog filtering model as EV-0024/0028). |
| M3 building & economy | not-started | — | — | |
| M4 combat, units, logic | not-started | — | — | |
| M5 breadth & platform | not-started | — | — | |

Seed queue: `EV-0046`…`EV-0060` are code-sourced candidates from the 2026-10-06
code-audit inventory (campaign spine, HUD, input, saves, content, multiplayer).
None are engine-verified yet; the fix queue is `record_finding.py list --status
open`, severity first.
