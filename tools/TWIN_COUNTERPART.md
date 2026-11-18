<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Twin counterpart: Mindustry (Java)

The twin-verification workflow lives in [`tools/ai-tools/twin/`](ai-tools/twin/).
It verifies this repository (`Mindustry-Godot`, the Godot/Rust port) against the
non-Godot reference below. Read this file before starting a twin walk: the
generic workflow intentionally contains no repo-specific paths.

## Identity

| Field | Value |
|---|---|
| Counterpart | Mindustry, the Java/libGDX upstream game |
| Checkout | `~/g/code_examples/Mindustry` (`/home/c/g/code_examples/Mindustry`) |
| Remote | <https://github.com/notnilcn/Mindustry> |
| Branch / commit at handoff | `master` / `2cd7aeecf1378b3db456be9bfde8691b3cdc1bcc` |
| Reference leg | `java` |
| Port leg | `godot` |
| Reference jar | `~/g/code_examples/Mindustry/desktop/build/libs/Mindustry.jar` |
| Java | JDK 17 |
| Window title | `Mindustry` |
| Reference data dir | `parity-java-home/` in this repository |

The counterpart checkout is deliberately outside this repository. Never add it
as a submodule, never write into it, and never let a run mutate its git state.

The registry declares the legs:

```json
"legs": { "reference": "java", "port": "godot" }
```

`twin_verify.py` reads that object; `twin_item.sh` takes the leg names from
`TWIN_REFERENCE_LEG`/`TWIN_PORT_LEG`, so export them once per shell (or prefix
every capture). In a `parity-session.sh` worktree, `TWIN_REGISTRY` and
`TWIN_RUNS` already point at the main checkout; never symlink the registry
(atomic replace would fork it).

```bash
export TWIN_REFERENCE_LEG=java TWIN_PORT_LEG=godot
```

## Reading the counterpart

Source is authoritative. Read it without checking out or editing the
counterpart:

```bash
git -C ~/g/code_examples/Mindustry show HEAD:core/src/mindustry/<path>
git -C ~/g/code_examples/Mindustry ls-tree -r --name-only HEAD core/src/mindustry
```

Mapping rules:

- Every ported `.rs`/`.tscn` file cites its upstream file in the header; use
  that citation, not a guess from the file name.
- The simulation and content live under `core/src/mindustry/`; the desktop
  launcher and packaging live under `desktop/`; the dedicated server lives
  under `server/`.
- Tests, fixtures and assets are under `tests/`, `core/assets/` and the
  platform projects. Read them read-only when the item's acceptance criteria
  are behavioural.

## Driving the legs

The Java leg is real: launch the jar, normalize the window, drive it with input,
and capture frames. The repo-side helpers are the source of truth:

- [`docs/TWIN-CONTINUATION.md`](../docs/TWIN-CONTINUATION.md) — Linux launch
  command, Xvfb display, bridge and profile backup/restore.
- [`docs/TWIN-WINDOWS.md`](../docs/TWIN-WINDOWS.md) — Windows environment and the
  PowerShell launch recipe.
- [`tools/twin.sh`](twin.sh) — sourced **drivers** used by the packaged
  workflow: `twin_launch`/`twin_place_java`/`twin_stop`, `JW`/`JM`/`JD`/`JK`/`JS`
  for the Java leg and `GC`/`GW`/`GK`/`GS`/`GE`/`GP` for the Godot leg.
- [`tools/windows/java_leg.py`](windows/java_leg.py) — Windows driver
  (`launch`, `move`, `shot`, `click`, `key`, `text`, `stop`).
- [`tools/gmcp.py`](gmcp.py) — Godot MCP shell driver (also used by
  `tools/session-play.sh`).

`twin.sh` is a driver, not a second workflow: the registry, statuses, evidence
layout and checker/fixer loop come from the packaged workflow.

Rules for the reference leg:

- Back up `settings.bin` and `saves/` before profile surgery and restore them
  afterwards. Use the repo's `parity-java-home/`, not the developer's real
  Mindustry profile.
- Normalize the reference window to the same size the port is captured at
  (the workflow's default is 1152x648) before comparing frames.
- Capture fresh reference frames only when the item needs a fresh reference;
  otherwise compare against the archived reference evidence.
- Treat the counterpart as read-only at all times. If a fix is needed, it is a
  fix on the **port** side.

## Evidence mapping

| Packaged workflow | This repo |
|---|---|
| `runs/<ITEM>/<reference-leg>/` | `runs/<ITEM>/java/` |
| `runs/<ITEM>/<port-leg>/` | `runs/<ITEM>/godot/` |
| Registry | `TWIN-VERIFICATION.json` |
| Counterpart contract | this file |

The registry, statuses, per-item evidence layout and checker/fixer loop are
defined in [`tools/ai-tools/twin/WORKFLOW.md`](ai-tools/twin/WORKFLOW.md).

Use the packaged helper for every evidence path:

```bash
# Create runs/<ITEM>/{java,godot}, write the run report and capture a frame.
tools/ai-tools/twin/twin_item.sh dir PAR-0042
tools/ai-tools/twin/twin_item.sh note PAR-0042 run "compared title bar; matches"

# Linux: capture through the sourced drivers. The helper exports TWIN_SHOT_PATH.
tools/ai-tools/twin/twin_item.sh capture port PAR-0042 01-action -- \
  bash -c 'source tools/twin.sh && GS 01-action'
tools/ai-tools/twin/twin_item.sh capture reference PAR-0042 01-action -- \
  bash -c 'source tools/twin.sh && JS 01-action'

# Windows: the driver writes exactly the path the helper passed.
tools/ai-tools/twin/twin_item.sh capture reference PAR-0042 01-action -- \
  bash -c 'python tools/windows/java_leg.py shot --out "$TWIN_SHOT_PATH"'
```

Validate and advance the registry with the packaged tool (no lock-protected
transaction helper is needed; `set-status` replaces the file atomically):

```bash
python3 tools/ai-tools/twin/twin_verify.py check --root .
python3 tools/ai-tools/twin/twin_verify.py next --root .
python3 tools/ai-tools/twin/twin_verify.py set-status PAR-0042 reference-verified \
  --note "compared title bar; matches"
```
