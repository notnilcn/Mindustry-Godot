<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Twin verification: runs layout and statuses

This page is the repo-side contract for twin-verification evidence. The generic
workflow — fork/action replay, statuses, evidence layout, checker/fixer loop and
the comparison gate — lives in
[`tools/ai-tools/twin/WORKFLOW.md`](../tools/ai-tools/twin/WORKFLOW.md). The
repo-specific counterpart and leg commands are in
[`tools/TWIN_COUNTERPART.md`](../tools/TWIN_COUNTERPART.md).

## Per-item runs layout

Every item run writes a single folder named after the item, with one subfolder
per client leg (`java` = reference, `godot` = port):

```
runs/
  PAR-00042/
    java/                                # Java reference frames (may stay empty)
      20261102-112233-01-menu.png
      20261102-112233-02-action.png
    godot/                               # Godot frames for this run
      20261102-112233-01-menu.png
      20261102-112233-02-action.png
    20261102-112233-run.md               # run report / observations
    20261102-112233-run.json             # optional structured evidence
    20261102-112233-java-game.log        # optional captured log
```

Rules:

- **One folder per item.** Do not create `runs/<stamp>-<item>/` run directories.
- **Images only under `java/` and `godot/`**, one leg per folder.
- **Every image filename starts with `YYYYmmdd-HHMMSS-`** (local time), then the
  step number and a short label.
- **Logs and text/JSON evidence live at the item root**, also timestamped; never
  inside `java/`/`godot/`.
- **The Java folder may stay empty.** A `reference-verified` item needs a Godot
  leg and a run report; a `twin-verified` item must have frames in both folders.
- **`runs/archive/**` is frozen reference evidence.** Do not move, copy or
  rename its files, and do not backfill the new per-item folders from it. Only
  frames captured in the current session belong in `runs/PAR-####/`.
- Use the packaged helper so paths and timestamps stay consistent:

```bash
export TWIN_REFERENCE_LEG=java TWIN_PORT_LEG=godot
tools/ai-tools/twin/twin_item.sh dir PAR-00042
tools/ai-tools/twin/twin_item.sh capture port PAR-00042 01-action -- <capture-command>
tools/ai-tools/twin/twin_item.sh note PAR-00042 run "what ran, what was compared, verdict"
```

`capture` passes the target file to the command as `$TWIN_SHOT_PATH`; the
counterpart page gives the concrete Java/Godot capture commands.

`runs/` stays gitignored and transient; the durable verdict is the item's
`status` in `TWIN-VERIFICATION.json`.

## Terminal statuses

The queue is complete when every item carries exactly one of:

| Status | Meaning | Required evidence |
|---|---|---|
| `reference-verified` | The Godot-only run matched the recorded reference: the archived Java/Godot frame when one exists, otherwise the item's recorded acceptance criteria and structured state. No code change was needed. | `runs/PAR-####/godot/` frames + a `*-run.md` (java/ may be empty). |
| `twin-verified` | A gap was found, the Godot side was fixed, and a fresh Java leg plus a fresh Godot leg match. | Frames in `runs/PAR-####/java/` and `runs/PAR-####/godot/`, a `*-run.md` describing the gap and fix, and the fix's code/test evidence. |

The legacy statuses (`seeded`, `reference`, `in-progress`, `mismatch`,
`fixing`, `verified`, `blocked`, `wontfix`) are transitional: a checker replaces
them only after an actual run. `wontfix` items still need a run before they can
be recorded as `reference-verified` (the accepted difference is the reference)
or `twin-verified` (if the difference was closed instead).

## Registry and queue commands

The packaged validator is the only registry tool:

```bash
python3 tools/ai-tools/twin/twin_verify.py check --root .            # validate the ledger
python3 tools/ai-tools/twin/twin_verify.py check --root . --evidence # + per-item evidence
python3 tools/ai-tools/twin/twin_verify.py next --root .             # first non-terminal item
python3 tools/ai-tools/twin/twin_verify.py stats --root .            # status counts
python3 tools/ai-tools/twin/twin_verify.py set-status PAR-0042 reference-verified \
  --note "short verdict"
```

`set-status` takes the registry lock and replaces `TWIN-VERIFICATION.json`
atomically. Run it in the checkout that owns the registry (or pass `--root`);
do **not** symlink the registry into a worktree, because the atomic replace
writes the path it is given. The registry declares `"legs": {"reference":
"java", "port": "godot"}` and `"schema": "ai-tools/twin-verification@1"`.

## Checker loop

Checkers run sequentially in the main checkout, never in parallel:

1. Take `twin_verify.py next`.
2. Godot-only run first: restore the item's fork, mirror the action, write
   `runs/PAR-####/godot/<stamp>-*.png`, compare to the archived reference (or
   the recorded assertions).
3. Match → set `reference-verified`, write the run report, continue.
4. Issue → stop; spawn exactly one nested fixer subagent with the item id, the
   observed symptom and the evidence path. The fixer runs the Java and Godot
   legs, fixes the code, re-runs, writes both leg folders, and sets
   `twin-verified`. Both then stop.
5. The parent starts the next checker at the following item.

Agent permissions inside the loop:

- Checkers and fixers may drive the game themselves (restart/play, click, eval,
  keyboard) and may update an item's `fork`/`action` tool-call chains when a run
  finds a better re-runnable sequence.
- Only verdict-supporting frames need to be kept in `runs/PAR-####/{java,godot}/`.
  In-flight navigation screenshots may stay in `/tmp` or be skipped; the run
  report is the durable narrative.
