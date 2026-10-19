# AGENTS.md — `addons/open_godot_mcp` (GDScript editor bridge)

This folder is the **Godot-side half** of Open-Godot-MCP: an `EditorPlugin` that exposes the editor and the running game to the Python MCP server over a WebSocket bridge. Only this Godot half is vendored in this checkout; the upstream Python server (`src/open_godot_mcp/`), its `tests/`, and `Docs/` are **external** and not present here, so `Docs:` references below point at upstream documentation rather than local files.

Downstream projects consume this folder as a plain addon copy (sometimes symlinked). Keep it self-contained: no imports from outside `res://addons/open_godot_mcp/`. Read the root [`AGENTS.md`](../../../AGENTS.md) and the client project map [`client/AGENTS.md`](../../AGENTS.md) first.

## Layout

| Path | Role |
|---|---|
| `plugin.gd` / `plugin.cfg` | Entry point (v0.1.10). `_enter_tree()` order: EditorSettings/ProjectSettings defaults → bridge `start_server()` → dock → export plugin → debugger plugin → editor logger → runtime autoload inject. `_exit_tree()` tears down in reverse. |
| `bridge/websocket_server.gd` | TCPServer + `WebSocketPeer` bridge (Godot 4's peer is client-only, so TCP accept + wrap). JSON-RPC 2.0 `tool_invoke` → `tool_result`/`event`; handshake carries `session_id`, versions, `auth_token`. Heartbeat ping every 5 s. |
| `bridge/dispatcher.gd` | `tool_name` → handler instance registry; fans out `set_bridge()` from the server's `_ready`. One handler may serve several tools (`godot_game`/`godot_game_time`, `godot_editor_read`/`_edit`, `godot_filesystem`/`godot_docs`/`godot_log`). |
| `handlers/*_handler.gd` | One file per tool family (24 files). Editor-direct ops run locally; game-runtime ops forward over the debugger channel. |
| `debugger/mcp_debugger_plugin.gd` | Editor ↔ game IPC over `EditorDebuggerSession` / `EngineDebugger`. Prefix `ogm`: editor→game `ogm:call [request_id, method, params]`, game→editor `ogm:response` / `ogm:error` / boot beacon `ogm:hello [pid, args]`. |
| `runtime/runtime_autoload.gd` | Runs **in the game process** as `McpRuntimeAutoload`: input sim, state observation (`_mcp_state()` protocol), eval, screenshots, `Engine.time_scale` clock; also serves standalone-WebSocket mode for `godot_network launch_instance`. |
| `runtime/network_conditioner.gd` | `McpNetworkConditioner` (`MultiplayerPeerExtension`) wrapping a peer to inject latency/loss/jitter; installed explicitly by the test under `godot_network`. |
| `runtime/mcp_test_suite.gd` | `OgmTestSuite` base class for `godot_test`: project-side `res://tests/` scripts with `test_*()` funcs, `set_up()`/`tear_down()` and `assert_*` helpers. |
| `utils/` | Shared: `error_codes.gd` (`MCPErrorCodes`), `variant_codec.gd` (`VariantCodec`), `port_resolver.gd` (`PortResolver`), `scene_path.gd` (`OgmScenePath`), `screenshot_cleanup.gd`, `update_manager.gd`, `update_reload_runner.gd`. |
| `dock/` | Status dock + language selector + Agnes/NVIDIA opt-in config + update banner. Strings in `dock/i18n/*.json` (`en.json` is the source). |
| `export/mcp_export_plugin.gd` | Clears the runtime autoload from the in-memory `project.godot` during export and restores it in-editor afterwards. |

## Handler contract

Every handler (except the debugger/export plugins):

```gdscript
extends RefCounted
const _EC = preload("res://addons/open_godot_mcp/utils/error_codes.gd")
var _bridge: Node  # set via set_bridge(); dispatcher fans it out

func handle(tool: String, action: String, params: Dictionary) -> Dictionary:
    match action:
        "status":
            return _EC.ok({...})
        _:
            return _EC.fail("INVALID_ARGUMENT", "Unknown action: %s" % action)
```

- Return **only** `_EC.ok(payload)` / `_EC.fail(code, message, extra?)` — never a raw dict. Codes follow the upstream `Docs/02-Tools/Index.md` error table; the list is not closed.
- Runtime-bound handlers forward via `dbg.call_runtime(method, call_params)` (awaited). Null-guard in order: `_bridge` → `get_debugger()` → `RUNTIME_NOT_CONNECTED`.
- `params.instance` is **1-based launch order** (`godot_game instances`); `0`/omitted = first instance. Never renumber; `_game_sessions` order in the debugger plugin is the source of truth.
- JSON boundary: encode outbound Variants with `VariantCodec.encode_variant`, decode inbound with `decode_variant` (Vector2↔`{"x","y"}`, etc., per upstream `Docs/02-Tools/Index.md`).

## Conventions

- **Tabs, GDScript, `@tool` only where the engine needs it** (plugin, debugger/export plugins, dock). Plain `extends RefCounted` elsewhere.
- **No `class_name` on handlers.** Only shared utils/components declare one (`MCPErrorCodes`, `VariantCodec`, `PortResolver`, `OgmScenePath`, `OgmTestSuite`, `McpNetworkConditioner`) to avoid polluting consuming projects. Everything else is referenced via `const X = preload("res://addons/open_godot_mcp/...")`.
- Every file opens with a `##` doc header naming its tool(s) and its upstream `Docs:` reference, e.g. `## Input handler — godot_input … Docs: 02-Tools/Input.md`.
- **Never hand-edit `.uid` files** — Godot manages them; editing breaks script UIDs.
- Settings keys (don't rename without updating `plugin.gd` and the upstream installation docs): `open_godot_mcp/bridge/*` (port, auto_port, heartbeat), `open_godot_mcp/runtime/*` (auto_inject, strip_on_export), `open_godot_mcp/security/*` (allow_eval, read_only, auth_token), `open_godot_mcp/ui/language`; ProjectSettings `open_godot_mcp/screenshot_max_count|_max_age_hours` and `autoload/McpRuntimeAutoload`.
- Timeouts: debugger `call_runtime` default **15 s**, game-ready wait **20 s**. An eval timeout usually means the evaluated body errored — surface it, don't raise the timeout.

## Adding a tool or action

1. Implement/extend the `handlers/` file (match on `action`, fail `INVALID_ARGUMENT` on unknown).
2. Register the tool name in `bridge/dispatcher.gd` (`_register("godot_x", _XHandler)`); one handler may serve several tool names.
3. Upstream only (not in this checkout): add the Python wrapper under `src/open_godot_mcp/tools/`, its tool description, and the doc under `Docs/02-Tools/` (zh-TW docs; code examples stay English).
4. If the runtime must serve it, add the method to `runtime/runtime_autoload.gd` behind the same `method` string the handler passes to `call_runtime`.

## Verification

- GDScript: no local unit runner. Verify live with `godot_health check`, then the relevant tool (`godot_game status`, `godot_test` for project-side `res://tests/` suites extending `OgmTestSuite` — none are committed in this checkout).
- Upstream Python: `pytest tests/` runs in the upstream repo only; this checkout has no Python package or tests.
- After touching the bridge/dispatcher/runtime channel, re-run a play/stop cycle plus one runtime round-trip (`digest` or `eval`) before calling it done.
- Start agent sessions at the consuming project root: `.mcp.json` (repo root here) launches the stdio `open-godot-mcp` server with `OPEN_GODOT_MCP_PORT=6970`, and is only discovered when the session starts there (or via the client's global config).

## Pitfalls

- `EditorInterface` / `EditorDebuggerPlugin` exist **only in the editor**. `runtime_autoload.gd` must guard with `Engine.is_editor_hint()` and never assume editor APIs.
- Signal/lambda capture from the debugger message handler is unreliable — the codebase polls a `_pending[request_id]` dict instead. Follow that pattern.
- Debugger session 0 is reused for the first game connection; a session counts as a game session only after its `started` signal. Don't skip index 0.
- `input` forwarding renames the action key to `input_type` to avoid colliding with InputMap `params["action"]`. Keep that rename.
- Export must never ship the autoload: test `strip_on_export` on/off after touching `export/` or the autoload registration.
- **`Engine.get_process_frames()` alone is not a liveness probe.** Observed pinned at one value for minutes while the game kept processing — `Time.get_ticks_msec()` advanced between `godot_exec` evals and `Engine.get_frames_per_second()` reported ~165. Check a ticks delta across two evals first; treat `get_process_frames()` as corroboration only.
- **`godot_screenshot burst` blocks its `call_runtime` round-trip for the whole burst plus per-frame capture cost**: `count: 8, duration_ms: 2400` exceeded the 15 s timeout. Keep bursts small (≈4 frames / ≤500 ms) or take individual `game` screenshots.
- **A runtime `eval` that returns `null` with `ok: true` usually means the body errored mid-script** (e.g. a bad Variant access in a compound one-liner). Split heavy expressions into small evals; a body containing `for`/`while` loops also times out.
- **An eval that errors, times out, or calls a method the node/class does not expose trips the editor break-on-error and pins the frame loop** while plain evals still answer (`godot_debugger sessions` → `paused: true`). Recover with `godot_game stop` + `play`; `resume` re-breaks. Keep eval bodies single-statement where possible, keep `var` declarations at the top level, and never block the main thread inside eval (`OS.delay_msec` stops `_process` and the clock) — wait between calls instead. Only real autoload singletons resolve as bare identifiers; scene nodes need `get_node("/root/...")` (a bare node name raises and pins). Missing Dictionary keys, null derefs on guessed child paths, and `get_meta()` on a missing key all count as errors.
- **Known-timeout eval bodies**: whole-state dumps parsed in eval (`JSON.parse_string(get_state_json())` + filter), `tile_blocks()`-style whole-grid walks, `campaign_views_json`/`section_json` payloads, for-loops over dialog children, filter/map lambdas over mixed rows. Query one known node per eval; stash multi-MB strings with `set_meta` and read slices later.
- **A host facade call while it is mid-build can panic `Gd<T>::bind() failed, already bound`** — the mutable Rust bind is held during dialog build/close, and a reentrant callback into it wedges the UI until restart. Defer callbacks (`call_deferred`) or read the value in `_ready`; after opening/closing a runtime-built dialog, settle a beat before reading the host's stack state and probe the dialog node's `visible` instead.
- **Synthetic `push_input` drives Control clicks but not every path** — world-placement/action paths that consume OS mouse input need real `godot_input`; before world input check no dialog lingers over the scene and the sim is not paused. Canvas/camera transforms update at frame end — read `tile_to_screen`-style coordinates in a call after the move.
- **Byte-identical screenshots are not automatically stale** — a static scene encodes identically; prove the capture path is live by mutating a node the shell really draws (e.g. `modulate`) and re-capturing, or compare against structured state. A truly pinned render (`Engine.get_frames_drawn()` frozen while ticks advance) needs `godot_game stop` + `play`.
- **Orphaned game processes survive `stop`/editor kills** and starve the editor/bridge (on software Vulkan they can take the editor down with a TDR). After a session, sweep `pgrep -f "godot --path"` and kill children whose `--editor-pid` is no longer alive.
