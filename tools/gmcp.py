#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
#
# gmcp.py — call open-godot-mcp tools from the shell (for agents whose session
# has no MCP connection). Reuses the stdio client from mcp_smoke.py.
#
#   tools/gmcp.py godot_health '{"action":"check"}'
#   tools/gmcp.py godot_exec '{"action":"eval","params":{"code":"OS.get_process_id()"}}'
#   tools/gmcp.py --eval 'get_node("/root/Spine/SimHost").get_tick()'
#   tools/gmcp.py --screenshot out.png          # game viewport -> PNG
#   tools/gmcp.py --text 'sand'                 # type into the focused Control
#   echo '[["godot_game",{"action":"status"}], ...]' | tools/gmcp.py --batch
#
# Several TOOL JSON pairs on one command line run in order in one MCP session.
# Prints one JSON result per call. Exit 1 on the first tool error.
#
# Text entry (L01 T4): --text sends godot_input `text`, which the runtime
# autoload (_input_text) turns into one fresh InputEventKey press + release per
# character with `unicode` set (plus keycode/shift for ASCII letters, digits and
# space), one process frame apart, delivered to the GUI focus owner. Focus the
# field first with a real click (GC). The result reports focus_before/after and
# the LineEdit's text. `godot_input key` carries no unicode, so it cannot type.
# --text-frame-delay N spaces characters N process frames apart (default 1).
#
# Harness guards for eval calls (J10 T2):
#   * Leading whitespace is normalised to tabs before the body is sent: the
#     addon wraps it as "func _eval():\n\t" + code.replace("\n", "\n\t"), so a
#     space-indented body becomes tab+spaces on every line, which GDScript
#     rejects as mixed indentation -- that parse error trips the editor
#     break-on-error and pins the frame loop (PAR-0345).
#   * An eval watchdog (--eval-timeout, default 10 s) aborts a hung eval and
#     issues `godot_game stop` + `play` on the pinned runtime, then exits 1
#     with a "watchdog" JSON summary. A bad body (e.g. `$I....`, an invalid
#     node path, a wrong-arity method) errors the same way (PAR-0347).
#     Pass --eval-timeout 0 for the plain --timeout (no client-side cut) and
#     --no-watchdog to leave recovery to the caller.

from __future__ import annotations

import argparse
import base64
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mcp_smoke import McpClient, SmokeError, find_mcp_bin  # noqa: E402

# Spaces per GDScript indent level; used to convert space indentation to tabs.
INDENT_UNIT = 4

DEFAULT_EVAL_TIMEOUT = 10.0
DEFAULT_WATCHDOG_SCENE = "res://scenes/game.tscn"
DEFAULT_WATCHDOG_WAIT = 30.0


def normalize_eval_indent(code: str) -> tuple[str, bool]:
    """Rewrite per-line leading whitespace to tabs.

    The addon wraps the body as "func _eval():\\n\\t" + code.replace("\\n", "\\n\\t"),
    appending each line's own leading whitespace after an injected tab. A
    space-indented body then starts every line with tab+spaces, which GDScript
    rejects as mixed indentation; the parse error trips the editor's
    break-on-error and pins the frame loop (PAR-0345). Converting each line's
    leading whitespace to tabs keeps the levels and removes the mix.
    """
    lines = []
    changed = False
    for line in code.split("\n"):
        stripped = line.lstrip(" \t")
        indent = line[: len(line) - len(stripped)]
        if " " in indent:
            width = indent.count("\t") * INDENT_UNIT + indent.count(" ")
            new_indent = "\t" * ((width + INDENT_UNIT - 1) // INDENT_UNIT)
            changed = True
            line = new_indent + stripped
        lines.append(line)
    return "\n".join(lines), changed


def prepare_call(tool: str, args: dict, quiet: bool = False) -> dict:
    """Apply the eval-body normaliser to a tool call; returns a copy."""
    if tool != "godot_exec" or not isinstance(args, dict) or args.get("action") != "eval":
        return args
    params = args.get("params")
    if not isinstance(params, dict) or not isinstance(params.get("code"), str):
        return args
    code, changed = normalize_eval_indent(params["code"])
    if not changed:
        return args
    if not quiet:
        print("[gmcp] normalised eval indentation (PAR-0345)", file=sys.stderr)
    args = json.loads(json.dumps(args))
    args["params"]["code"] = code
    return args


def text_call(text: str, frame_delay: int = 1) -> dict:
    """godot_input args that type `text` into the focused Control (L01 T4)."""
    return {"action": "text", "params": {"text": text, "frame_delay": max(int(frame_delay), 0)}}


def watchdog_recover(mcp: McpClient, scene: str, wait: float) -> dict:
    """stop + play (bounded wait) after a pinned eval; prints the summary."""
    summary: dict = {"watchdog": "eval stalled the frame loop; stop+play", "actions": []}
    for tool, args in (
        ("godot_game", {"action": "stop"}),
        ("godot_game", {"action": "play", "params": {"scene": scene}}),
    ):
        try:
            summary["actions"].append({args["action"]: mcp.call(tool, args, timeout=30.0)})
        except SmokeError as e:
            summary["actions"].append({f"{args['action']}_error": str(e)})
    deadline = time.monotonic() + max(wait, 0.0)
    connected = False
    while time.monotonic() < deadline:
        time.sleep(1.5)
        try:
            status = mcp.call("godot_game", {"action": "status"}, timeout=20.0)
        except SmokeError:
            continue
        if status.get("runtime_connected"):
            connected = True
            break
    summary["runtime_connected"] = connected
    summary["note"] = (
        "the game restarted: re-establish pid/tick/camera state"
        if connected
        else "runtime did not reconnect within the watchdog wait"
    )
    print(json.dumps(summary, default=str))
    return summary


def _find_png(obj):
    """Locate a base64 PNG payload anywhere in a tool result."""
    if isinstance(obj, dict):
        for k, v in obj.items():
            if isinstance(v, str) and len(v) > 200 and k in ("data", "image", "png", "base64", "image_base64"):
                return v
            r = _find_png(v)
            if r:
                return r
    elif isinstance(obj, list):
        for v in obj:
            r = _find_png(v)
            if r:
                return r
    return None


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--eval", action="append", default=[], help="GDScript expression for godot_exec eval")
    ap.add_argument("--screenshot", help="write a game screenshot PNG to this path")
    ap.add_argument(
        "--text",
        action="append",
        default=[],
        help="type this string into the GUI focus owner (godot_input text: one unicode key press+release per char)",
    )
    ap.add_argument("--text-frame-delay", type=int, default=1, help="process frames between typed characters")
    ap.add_argument("--batch", action="store_true", help="read [[tool, args], ...] JSON from stdin")
    ap.add_argument("--timeout", type=float, default=60.0)
    ap.add_argument(
        "--eval-timeout",
        type=float,
        default=DEFAULT_EVAL_TIMEOUT,
        help=f"eval watchdog cutoff in seconds (default {DEFAULT_EVAL_TIMEOUT:g}; 0 keeps --timeout)",
    )
    ap.add_argument(
        "--no-watchdog",
        action="store_true",
        help="on an eval timeout, do not run godot_game stop/play recovery",
    )
    ap.add_argument(
        "--watchdog-scene",
        default=DEFAULT_WATCHDOG_SCENE,
        help="scene godot_game play replays after a watchdog recovery",
    )
    ap.add_argument(
        "--watchdog-wait",
        type=float,
        default=DEFAULT_WATCHDOG_WAIT,
        help="seconds to wait for runtime_connected after a watchdog replay",
    )
    ap.add_argument("--sleep", type=float, default=0.0, help="seconds to sleep between calls")
    ap.add_argument("pairs", nargs="*")
    a = ap.parse_args()

    calls: list[tuple[str, dict]] = []
    if len(a.pairs) % 2:
        ap.error("positional args must be TOOL JSON pairs")
    for i in range(0, len(a.pairs), 2):
        calls.append((a.pairs[i], json.loads(a.pairs[i + 1])))
    for text in a.text:
        calls.append(("godot_input", text_call(text, a.text_frame_delay)))
    for code in a.eval:
        calls.append(("godot_exec", {"action": "eval", "params": {"code": code}}))
    if a.batch:
        calls.extend((t, p) for t, p in json.load(sys.stdin))

    mcp = McpClient(find_mcp_bin(None), [])
    try:
        for tool, args in calls:
            is_eval = tool == "godot_exec" and isinstance(args, dict) and args.get("action") == "eval"
            args = prepare_call(tool, args)
            call_timeout = a.timeout
            if is_eval and a.eval_timeout > 0:
                call_timeout = min(a.timeout, a.eval_timeout)
            try:
                res = mcp.call(tool, args, timeout=call_timeout)
                print(json.dumps(res, default=str)[:20000])
                if is_eval and not a.no_watchdog:
                    # A parse error returns ok:true with result:null and an
                    # inline error after pinning the loop; recover like a
                    # timeout, before the next call hits the pinned runtime.
                    if res.get("result") is None and isinstance(res.get("error"), dict):
                        watchdog_recover(mcp, a.watchdog_scene, a.watchdog_wait)
                        return 1
            except SmokeError as e:
                print(json.dumps({"ok": False, "tool": tool, "error": str(e)}))
                if is_eval and not a.no_watchdog:
                    watchdog_recover(mcp, a.watchdog_scene, a.watchdog_wait)
                return 1
            if a.sleep:
                time.sleep(a.sleep)
        if a.screenshot:
            raw = mcp.request("tools/call", {"name": "godot_screenshot", "arguments": {"action": "game"}}, timeout=a.timeout)
            data = None
            for c in raw.get("content") or []:
                if c.get("type") == "image":
                    data = c.get("data")
                    break
            if data is None:
                data = _find_png(raw)
            src = (raw.get("structuredContent") or {}).get("path")
            if data is None and src:
                import shutil
                shutil.copyfile(src, a.screenshot)
                print(json.dumps({"ok": True, "screenshot": a.screenshot}))
                return 0
            if data is None:
                print(json.dumps({"ok": False, "error": "no image in screenshot result", "raw": str(raw)[:2000]}))
                return 1
            with open(a.screenshot, "wb") as f:
                f.write(base64.b64decode(data))
            print(json.dumps({"ok": True, "screenshot": a.screenshot}))
    finally:
        mcp.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
