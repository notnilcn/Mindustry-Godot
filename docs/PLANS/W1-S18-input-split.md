<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W1-S18 — Split `mind-gdext/src/input/desktop.rs` (5544) and `input/mod.rs` (2132)

**Wave:** W1 (pure mechanical split, RULES §3).
**Owns:** `client/rust/mind-gdext/src/input/{desktop.rs,mod.rs}` and the new files in `input/`
(`input/desktop/**`, `input/api_*.rs`, `input/glue.rs`, `input/glue_mobile.rs`, `input/effects.rs`). The existing `bindings.rs`, `events.rs`,
`gesture.rs` and `mobile.rs` are untouched.
**Effort:** large.

## `desktop.rs`: `DesktopBridge` (Godot glue for the `mind_core::input::desktop` port of `input/DesktopInput.java`)

`git mv input/desktop.rs input/desktop/mod.rs`. `input/mod.rs` keeps `mod desktop;` (or `pub mod`) unchanged.

`DesktopBridge` is a plain Rust struct, not a Godot class, so plain `impl` blocks can go anywhere.

| File | Lines at `291e30a8` | Contents |
|---|---|---|
| `desktop/mod.rs` | 1–148 (consts), 237–368 (`HostWorld` + `impl PlacementWorld`), 475–612 (`WaveGate`, `struct DesktopBridge`), 619–722 (impl head: `new` etc.) | type definitions, re-exports |
| `desktop/effect.rs` | 149–236 | `Effect` enum |
| `desktop/helpers.rs` | 369–474 | remaining free helpers from 36–418 that are not consts |
| `desktop/keys.rs` | 723–1054 | keys, axes, handle |
| `desktop/actions.rs` | 1055–1396 | `dispatch_action` (**one big match; move it whole**) |
| `desktop/command_mode.rs` | 1397–1732 | command mode, units, stance, payload |
| `desktop/hotkeys.rs` | 1733–2146 | transfer, rotate, zoom, pick, hotkeys |
| `desktop/selection.rs` | 2147–2497 | select plans, schematic, key-up, last config, placement view |
| `desktop/mouse.rs` | 2498–3415 | mouse press/release, plan grabbing, `tile_tapped` (3085–3269) |
| `desktop/placement.rs` | 3416–3795 | move, place line, commit place/select, scroll |
| `desktop/config.rs` | 3796–4327 | `config_spec` (about 500 lines, whole) |
| `desktop/content.rs` | 4328–4524 | free content/config helpers |
| `desktop/tests.rs` | 4525–5544 | tests (about 1020 lines) |

Re-export the items imported elsewhere: `ui/fragments/hud.rs:45` (`WaveGate`) and `input/mod.rs`
(`DesktopBridge`, `Effect`, `WaveGate`, `HostWorld`, `PlacementView`, at lines 40, 645, 744, 767,
790, 1559, 2064). Private methods used across files become `pub(super)` (scope `input::desktop`).
Items that `input/mod.rs` uses get `pub(in crate::input)`, the same as today's effective
visibility of a private item in a module that `input` declares.

## `input/mod.rs`: `MindInput` (a Godot Node; two `#[godot_api]` blocks at 94 and 315, 74 `#[func]`)

Today it has two `#[godot_api]` blocks: 94 is the `INode` impl and 315 is the inherent one. Keep
the `INode` impl and the **inherent block's `#[signal]`s** (316–327) in `mod.rs`.

| File | Lines | Contents | Block |
|---|---|---|---|
| `mod.rs` | 1–327 | submodule decls 13–17, struct 45–94, `INode` 95–315, the primary inherent block with the signals | primary |
| `glue.rs` | 328–679 | private helpers (bootstrap, ui-capture) | plain `impl MindInput` |
| `glue_mobile.rs` | 680–910 | mobile routing | plain |
| `effects.rs` | 911–1125 | `drain_effects` | plain |
| `api_core.rs` | 1126–1272 | first `#[func]` group | `#[godot_api(secondary)]` |
| `api_keybinds.rs` | 1273–1377 | keybinds | secondary |
| `api_state.rs` | 1378–1564 | state JSON | secondary |
| `api_selection.rs` | 1565–1723 | selection/command (incl. config 1659–1700) | secondary |
| `api_building.rs` | 1724–1957 | building | secondary |
| `api_mobile.rs` | 1958–2132 | mobile funcs, `gestures_json` 2129 | secondary |

Each file starts with `use super::*;`. Check which lines inside the `#[func]` ranges are actually
private helpers. They go to a plain `impl` in the same file (RULES §3.5).

## Verification (RULES §6)

```bash
B=$(git merge-base main HEAD); I=client/rust/mind-gdext/src/input
python3 tools/refactor/api_surface.py | diff /tmp/S18-surface-before.txt -       # empty: MindInput funcs/signals unchanged
python3 tools/refactor/move_check.py --base $B --old $I/desktop.rs --new $I/desktop
python3 tools/refactor/move_check.py --base $B --old $I/mod.rs --new $I/mod.rs $I/glue.rs $I/glue_mobile.rs $I/effects.rs $I/api_core.rs $I/api_keybinds.rs $I/api_state.rs $I/api_selection.rs $I/api_building.rs $I/api_mobile.rs
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p mind-gdext --all-targets -- -D warnings
run --cargo-test -p mind-gdext --lib input
```

Orchestrator post-merge: a short MCP playtest of input (place a block, drag a line, rotate,
command-mode select).
