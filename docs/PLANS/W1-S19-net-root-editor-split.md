<!-- SPDX-License-Identifier: GPL-3.0-only -->
# W1-S19 — Split `net/mod.rs` (3101), `ui/root.rs` (2419), `ui/editor_views.rs` (3099), `editor/mod.rs` (2904)

**Wave:** W1 (pure mechanical split, RULES §3).
**Owns** (paths under `client/rust/mind-gdext/src/`):
- `net/mod.rs` plus new files in `net/` (the existing `call.rs` and `relay.rs` are not yours);
- `ui/root.rs` → `ui/root/**`;
- `ui/editor_views.rs` → `ui/editor_views/**`;
- `editor/mod.rs` plus new files in `editor/` (the existing `editor_renderer.rs`, `map_view.rs` and
  `preview.rs` are not yours).

**Effort:** large. These are four independent parts; commit each one separately and in this order:
editor_views (easiest), root, net, editor.

## Part A — `ui/editor_views.rs`: four Godot classes for the map editor UI

`git mv ui/editor_views.rs ui/editor_views/mod.rs`. Each class moves **whole**: its struct,
`#[derive(GodotClass)]`, and all its impl blocks, including both `#[godot_api]` blocks. A class that
is entirely inside one file keeps its own primary block, so no secondary blocks are needed.

| File | Lines at `291e30a8` | Contents | Upstream (`core/src/mindustry/`) |
|---|---|---|---|
| `mod.rs` | 1–164 | shared helpers (`defer_action`, `editor_call`, `variant_*`, `dict_*`, `bundle`; `pub(super)`), `mod` lines, `pub use` of the 4 classes | – |
| `map_editor_dialog.rs` | 165–1180 | `MindMapEditorDialog` (`#[godot_api]` at 234, 354) | `editor/MapEditorDialog.java` |
| `map_view.rs` | 1181–1713 | `MindMapView` (1213, 1388) | `editor/MapView.java` |
| `wave_graph.rs` | 1714–2447 | `MindWaveGraph` (1772, 1885; `draw_plot` 2016–2262) | `editor/WaveGraph.java` |
| `objectives_canvas.rs` | 2448–3061 | `MindMapObjectivesCanvas` (2494, 2643) | `editor/MapObjectivesCanvas.java` |
| `tests.rs` | 3062–3099 | tests | – |

Importers: `ui/root.rs:43` and `audio/mod.rs:56` (`MindMapEditorDialog`), and `ui/mod.rs:13`. The header of
this file has the two lines in reverse order; copy that order.

## Part B — `ui/root.rs`: `MindUiRoot` (a Control; one `#[godot_api]` block of 43 `#[func]` mixed with private fns)

`git mv ui/root.rs ui/root/mod.rs`. The section comments inside the block are the seams:

| File | Lines | Section(s) | Block |
|---|---|---|---|
| `mod.rs` | 1–329 + 330–707 | consts, node hooks, `Prompt` types, struct, `IControl`, the primary `#[godot_api]` block with "public surface" (332–707) and **every `#[signal]`** | primary |
| `prompts.rs` | 708–927, 996–1149, 1967–2124 | prompt signals, prompt handlers, prompt builder | secondary (handlers are `#[func]`s connected as signal targets; keep them `#[func]`) + plain |
| `net_intents.rs` | 928–995 | net intents | secondary |
| `boot.rs` | 1150–1435 | boot/theme, manifest binding, signal wiring | plain or secondary, as needed |
| `ui_scale.rs` | 1436–1613 | UI scale, mod-init prompt | as needed |
| `messages.rs` | 1614–1762 | logic messages / join | as needed |
| `escape.rs` | 1763–1966 | Esc / background pause | as needed |
| `accessors.rs` | 2125–2264 | accessors, helpers | plain |
| `free.rs` | 2266–2355 | free fns (`json_string` 2340, `json_bool` 2349) | – |
| `tests.rs` | 2356–2419 | tests | – |

If a `#[signal]` sits inside one of the later sections, it **stays in `mod.rs`**'s primary block.
Importers: `toasts.rs`, `landing.rs`, `player_list.rs`, `batch_c.rs`, `checks.rs`, `input/mod.rs`,
`campaign.rs`, `hud.rs`, `block_config.rs`.

## Part C — `net/mod.rs`: `MindNet` (a Node; 52 `#[func]`)

| File | Lines | Contents | Block |
|---|---|---|---|
| `mod.rs` | 1–408 | consts, `ProbeState`, `HostedWorld`, `JoinLoad`, `ServerProbe`, struct 127–254, `INode` 255–407 | – |
| `api.rs` | 409–1375 | the `#[godot_api]` block (52 funcs). **It stays primary**: either keep it in `mod.rs`, or move all of it to `api.rs` as the one primary block. Only `#[signal]`s force a location; if it has signals, keep it in `mod.rs` | primary |
| `probe.rs` | 1450–1552 (+ 1376–1449 head of the private impl) | probes | plain |
| `snapshot.rs` | 1553–1752, 2407–2588 | publish snapshot (1610–1685), snapshots/join | plain |
| `roster.rs` | 1753–1922, 2175–2240 | relay/rows/roster | plain |
| `calls.rs` | 1923–2174 | player actions/calls. **The name `calls.rs` would sit next to the existing `call.rs`; use `actions.rs` instead** | plain |
| `drain.rs` | 2241–2406 | drain relay/outbox/commands | plain |
| `adapters.rs` | 2589–2863 (2648–2863 core) | adapters and free fns | – |
| `tests.rs` | 2864–3101 | tests | – |

`lib.rs:43` re-exports `MindNet`; keep the type in `mod.rs`.

## Part D — `editor/mod.rs`: `MindEditor` (one `#[godot_api]` block of 123 `#[func]`, sectioned by comments)

| File | Lines | Section | Block |
|---|---|---|---|
| `mod.rs` | 1–282 | helpers 69–124, struct 125–146, `INode` 147–182, private impl 183–282 | primary (keep a small primary block with any `#[signal]`s, and with the state funcs at 284–503 if there are no signals) |
| `api_state.rs` | 283–503 | state | secondary |
| `api_banned.rs` | 504–678 | banned | secondary |
| `api_palette.rs` | 679–741 | palette | secondary |
| `api_lifecycle.rs` | 742–878 | lifecycle | secondary |
| `api_data.rs` | 879–2221 | dialog data / filters / waves / objectives / processors / data patches. **About 1340 lines; split at the section comments** into `api_filters.rs`, `api_waves.rs`, `api_objectives.rs`, `api_processors.rs`, `api_patches.rs` | secondary |
| `api_playtest.rs` | 2222–2409 | playtest | secondary |
| `api_map_view.rs` | 2410–2592 | map view | secondary |
| `api_dev.rs` | 2593–2828 | dev/MCP | secondary |
| `free.rs` | 2829–2904 | free fns | – |

Importers: `ui/editor_views.rs` (22 uses), `ui/dialogs/batch_e.rs` (7), `campaign.rs` (4),
`ui/state_inspector.rs` (3), …. They use `MindEditor` methods, whose names are unchanged.

## Verification (RULES §6)

```bash
B=$(git merge-base main HEAD); G=client/rust/mind-gdext/src
python3 tools/refactor/api_surface.py | diff /tmp/S19-surface-before.txt -          # empty
python3 tools/refactor/move_check.py --base $B --old $G/ui/editor_views.rs --new $G/ui/editor_views
python3 tools/refactor/move_check.py --base $B --old $G/ui/root.rs --new $G/ui/root
python3 tools/refactor/move_check.py --base $B --old $G/net/mod.rs --new $G/net/mod.rs $G/net/{api,probe,snapshot,roster,actions,drain,adapters,tests}.rs
python3 tools/refactor/move_check.py --base $B --old $G/editor/mod.rs --new $G/editor/mod.rs $G/editor/api_*.rs $G/editor/free.rs
run --cmd -- cargo clippy --manifest-path client/rust/Cargo.toml -p mind-gdext --all-targets -- -D warnings
run --cargo-test -p mind-gdext --lib
```

(Drop any `--new` path you did not create.) Orchestrator post-merge: `dialog_sweep_check`,
`ui_widgets_check`, plus a headless boot.
