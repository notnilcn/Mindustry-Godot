<!-- SPDX-License-Identifier: GPL-3.0-only -->
# Twin counterpart: Mindustry (Java)

The twin-verification workflow in [`tools/ai-tools/twin/`](ai-tools/twin/)
verifies the port in this repository (`Mindustry-Godot`, the Godot/Rust client)
against the non-Godot reference below. Read this file before starting a twin
walk; the generic workflow intentionally contains no repo-specific paths.

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

## Driving the reference leg

The Java leg is real: launch the jar, normalize the window, drive it with input,
and capture frames. The repo-side helpers are the source of truth:

- [`docs/TWIN-CONTINUATION.md`](../docs/TWIN-CONTINUATION.md) — launch command,
  Xvfb display, bridge, and profile backup/restore.
- [`docs/TWIN-WINDOWS.md`](../docs/TWIN-WINDOWS.md) — Windows environment and the
  PowerShell launch recipe.
- [`tools/twin.sh`](twin.sh) — sourced Linux Java+Godot walk helpers
  (`twin_launch`, `JW`/`JM`/`JD`/`JK`/`JS`, `twin_stop`).
- [`tools/windows/java_leg.py`](windows/java_leg.py) — Windows driver
  (`launch`, `move`, `shot`, `click`, `key`, `text`, `stop`).
- [`tools/gmcp.py`](gmcp.py), [`tools/twin_item.sh`](twin_item.sh) —
  port-leg capture and per-item evidence.

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

| Generic workflow | This repo |
|---|---|
| `runs/<ITEM>/<reference-leg>/` | `runs/<ITEM>/java/` |
| `runs/<ITEM>/<port-leg>/` | `runs/<ITEM>/godot/` |
| Registry | `TWIN-VERIFICATION.json` |
| Counterpart contract | this file |

The registry, statuses, per-item evidence layout and checker/fixer loop are
defined in [`tools/ai-tools/twin/WORKFLOW.md`](ai-tools/twin/WORKFLOW.md).

The checked-in `TWIN-VERIFICATION.json` predates the generic `legs` object.
Point the generic validator at its concrete leg names explicitly:

```bash
TWIN_REFERENCE_LEG=java TWIN_PORT_LEG=godot \
  python3 tools/ai-tools/twin/twin_verify.py check --root .
```
