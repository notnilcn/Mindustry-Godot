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
#   echo '[["godot_game",{"action":"status"}], ...]' | tools/gmcp.py --batch
#
# Several TOOL JSON pairs on one command line run in order in one MCP session.
# Prints one JSON result per call. Exit 1 on the first tool error.

from __future__ import annotations

import argparse
import base64
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mcp_smoke import McpClient, SmokeError, find_mcp_bin  # noqa: E402


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
    ap.add_argument("--batch", action="store_true", help="read [[tool, args], ...] JSON from stdin")
    ap.add_argument("--timeout", type=float, default=60.0)
    ap.add_argument("--sleep", type=float, default=0.0, help="seconds to sleep between calls")
    ap.add_argument("pairs", nargs="*")
    a = ap.parse_args()

    calls: list[tuple[str, dict]] = []
    if len(a.pairs) % 2:
        ap.error("positional args must be TOOL JSON pairs")
    for i in range(0, len(a.pairs), 2):
        calls.append((a.pairs[i], json.loads(a.pairs[i + 1])))
    for code in a.eval:
        calls.append(("godot_exec", {"action": "eval", "params": {"code": code}}))
    if a.batch:
        calls.extend((t, p) for t, p in json.load(sys.stdin))

    mcp = McpClient(find_mcp_bin(None), [])
    try:
        for tool, args in calls:
            try:
                res = mcp.call(tool, args, timeout=a.timeout)
                print(json.dumps(res, default=str)[:20000])
            except SmokeError as e:
                print(json.dumps({"ok": False, "tool": tool, "error": str(e)}))
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
