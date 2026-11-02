# Twin verification: runs layout and statuses (2026-11-02)

This page is the repo-side contract for twin-verification evidence after the
GDScript→Rust UI cutover. The workflow itself (fork/action replay, frame gate,
fix mode) lives in the `parity-eval` skill; this page owns the file layout and
the two terminal statuses.

## Per-item runs layout

Every item run writes a single folder named after the item, with one subfolder
per client leg:

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

- **One folder per item.** Do not create `runs/<stamp>-<item>/` run directories
  any more.
- **Images only under `java/` and `godot/`**, one leg per folder.
- **Every image filename starts with `YYYYmmdd-HHMMSS-`** (local time), then the
  step number and a short label.
- **Logs and text/JSON evidence live at the item root**, also timestamped; never
  inside `java/`/`godot/`.
- **The Java folder may stay empty.** A `reference-verified` item needs no Java
  leg; a `twin-verified` item must have frames in both folders.
- **`runs/archive/**` is frozen reference evidence.** Do not move, copy or
  rename its files, and do not backfill the new per-item folders from it. Only
  frames captured in the current session belong in `runs/PAR-####/`.
- Use `tools/twin_item.sh` so the paths and timestamps stay consistent:
  `dir`, `godot-shot`, `java-shot`, `note`, `log` (see `tools/AGENTS.md`).

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

## Checker loop

Checkers run sequentially in the main checkout, never in parallel:

1. Take the first item in `PAR-####` order whose status is not one of the two
   terminal statuses.
2. Godot-only run first: restore the item's fork, mirror the action, write
   `runs/PAR-####/godot/<stamp>-*.png`, compare to the archived reference (or
   the recorded assertions).
3. Match → set `reference-verified`, write the run report, continue.
4. Issue → stop; spawn exactly one nested fixer subagent with the item id, the
   observed symptom and the evidence path. The fixer runs the Java and Godot
   legs, fixes the code, re-runs, writes both leg folders, and sets
   `twin-verified`. Both then stop.
5. The parent starts the next checker at the following item id.

Agent permissions inside the loop:

- Checkers and fixers may drive the game themselves (restart/play, click, eval,
  keyboard) and may update an item's `fork`/`action` tool-call chains when a run
  finds a better re-runnable sequence.
- Only verdict-supporting frames need to be kept in `runs/PAR-####/{java,godot}/`.
  In-flight navigation screenshots may stay in `/tmp` or be skipped; the run
  report is the durable narrative.

