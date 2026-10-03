#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
#
# mcp_smoke.py — stdio MCP driver for the local spine smoke test
# (`00_FOUNDATION_IMPLEMENTATION_PLAN.md` §7c steps 1-6 and 9).
#
# It spawns its own `open-godot-mcp` stdio server process (the bridge WebSocket
# accepts multiple MCP clients, so this works while another editor session --
# e.g. opencode's own MCP connection -- is attached), performs the MCP
# handshake, then drives the running Godot editor + game:
#
#   1. godot_editor_edit open_scene res://scenes/spine.tscn + explicit play
#   2. pid-stamped liveness eval (get_tick/get_checksum)
#   3. load_scenario + step(60) in Playing -> golden checksum a1a7b96167c9718d
#   4. pause, API place_block/break_block
#   5. tile_to_screen + real godot_input mouse click (place/break via _input)
#   6. godot_log errors clean + startup [I] lines present, then stop
#
# Constraints (documented in the repo playtest skill):
#   * The editor must already be running (this script does not launch it).
#   * Bridge default port 6970; the headless import check may briefly hold 6971.
#   * Spawning the Windows .exe from WSL python3 via interop was tested on this
#     host and works; `--mcp-bin` / OPEN_GODOT_MCP_BIN override the lookup.
#
# Exit codes: 0 pass, 1 assertion/tool failure, 2 bridge not connected / setup.

from __future__ import annotations

import argparse
import json
import os
import queue
import shutil
import subprocess
import sys
import threading
import time
from collections import deque

GOLDEN_CHECKSUM = "a1a7b96167c9718d"
SPINE_SCENE = "res://scenes/spine.tscn"
SCENARIO = "res://scenarios/spine_place_break.json"
HOST_PATH = "/root/Spine/SimHost"
CAMERA_PATH = "/root/Spine/World/Camera2D"

DEFAULT_MCP_CANDIDATES = (
    "~/.local/bin/open-godot-mcp.exe",
    "~/.local/bin/open-godot-mcp",
    r"C:\Users\Clinton\.local\bin\open-godot-mcp.exe",
    "/mnt/c/Users/Clinton/.local/bin/open-godot-mcp.exe",
)


class SmokeError(RuntimeError):
    pass


class ToolError(SmokeError):
    pass


def as_int(value) -> int | None:
    """Godot ints may arrive as JSON floats through the bridge; normalize."""
    if isinstance(value, bool) or value is None:
        return None
    if isinstance(value, int):
        return value
    if isinstance(value, float) and value.is_integer():
        return int(value)
    return None


def section(title: str) -> None:
    print(f"\n== {title} ==", flush=True)


def ok(msg: str) -> None:
    print(f"  ok  {msg}", flush=True)


def fail(msg: str) -> None:
    print(f"FAIL  {msg}", file=sys.stderr, flush=True)


def find_mcp_bin(explicit: str | None) -> str:
    if explicit:
        return explicit
    env = os.environ.get("OPEN_GODOT_MCP_BIN")
    if env:
        return env
    found = shutil.which("open-godot-mcp")
    if found:
        return found
    # WSL: ask the Windows interop resolver before falling back to fixed paths.
    cmd = shutil.which("cmd.exe")
    if cmd:
        try:
            out = subprocess.run(
                [cmd, "/c", "where", "open-godot-mcp.exe"],
                capture_output=True, text=True, timeout=20,
            )
            first = out.stdout.strip().splitlines()
            if out.returncode == 0 and first:
                drive = first[0].strip()
                wsl = "/mnt/" + drive[0].lower() + drive[2:].replace("\\", "/")
                return wsl
        except (OSError, subprocess.SubprocessError):
            pass
    for cand in DEFAULT_MCP_CANDIDATES:
        path = os.path.expanduser(cand)
        if os.path.exists(path):
            return path
    raise SmokeError(
        "open-godot-mcp binary not found; set OPEN_GODOT_MCP_BIN or pass --mcp-bin"
    )


class McpClient:
    """Minimal newline-JSON MCP client over a stdio child process."""

    def __init__(self, exe: str, server_args: list[str], verbose: bool = False):
        self.exe = exe
        self.verbose = verbose
        self.stderr_lines: deque[str] = deque(maxlen=200)
        self.proc = subprocess.Popen(
            [exe, *server_args],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        self._lines: queue.Queue[bytes | None] = queue.Queue()
        self._next_id = 0
        threading.Thread(target=self._pump_stdout, daemon=True).start()
        threading.Thread(target=self._pump_stderr, daemon=True).start()
        self._handshake()

    # -- plumbing ---------------------------------------------------------

    def _pump_stdout(self) -> None:
        assert self.proc.stdout is not None
        for line in iter(self.proc.stdout.readline, b""):
            self._lines.put(line)
        self._lines.put(None)

    def _pump_stderr(self) -> None:
        assert self.proc.stderr is not None
        for line in iter(self.proc.stderr.readline, b""):
            self.stderr_lines.append(line.decode("utf-8", "replace").rstrip())

    def _send(self, msg: dict) -> None:
        assert self.proc.stdin is not None
        self.proc.stdin.write((json.dumps(msg) + "\n").encode("utf-8"))
        self.proc.stdin.flush()

    def request(self, method: str, params: dict, timeout: float = 45.0) -> dict:
        self._next_id += 1
        rid = self._next_id
        self._send({"jsonrpc": "2.0", "id": rid, "method": method, "params": params})
        deadline = time.monotonic() + timeout
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise SmokeError(f"MCP timeout after {timeout:.0f}s waiting for {method}")
            try:
                line = self._lines.get(timeout=min(remaining, 0.5))
            except queue.Empty:
                continue
            if line is None:
                raise SmokeError(f"MCP server exited during {method}")
            text = line.decode("utf-8", "replace").strip()
            if not text:
                continue
            try:
                msg = json.loads(text)
            except ValueError:
                if self.verbose:
                    print(f"  [mcp stdout] {text[:200]}", flush=True)
                continue
            if msg.get("id") == rid:
                if "error" in msg:
                    raise SmokeError(f"{method} error: {msg['error']}")
                return msg.get("result", {})
            # server-initiated notification/request; ignore
            if self.verbose:
                print(f"  [mcp note] {json.dumps(msg)[:200]}", flush=True)

    def notify(self, method: str, params: dict | None = None) -> None:
        msg = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            msg["params"] = params
        self._send(msg)

    def call(self, tool: str, arguments: dict, timeout: float = 60.0) -> dict:
        result = self.request(
            "tools/call", {"name": tool, "arguments": arguments}, timeout=timeout
        )
        payload = result.get("structuredContent")
        if payload is None:
            content = result.get("content") or []
            if content and content[0].get("type") == "text":
                try:
                    payload = json.loads(content[0].get("text", "{}"))
                except ValueError:
                    payload = {}
            else:
                payload = {}
        if result.get("isError") or payload.get("ok") is False:
            err = payload.get("error") or result.get("error") or "tool returned isError"
            if isinstance(err, dict):
                err = err.get("message") or err.get("code") or str(err)
            raise ToolError(f"{tool}: {err}")
        return payload

    def _handshake(self) -> None:
        info = self.request(
            "initialize",
            {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "mind-mcp-smoke", "version": "0.1.0"},
            },
            timeout=30.0,
        )
        self.server_version = (info.get("serverInfo") or {}).get("version", "?")
        self.notify("notifications/initialized")

    def close(self) -> None:
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.proc.kill()


# -- eval helpers --------------------------------------------------------


def eval_code(mcp: McpClient, code: str, timeout: float = 60.0):
    payload = mcp.call(
        "godot_exec", {"action": "eval", "params": {"code": code}}, timeout=timeout
    )
    return payload.get("result")


def call_func(mcp: McpClient, node_path: str, method: str, args: list,
              timeout: float = 60.0):
    payload = mcp.call(
        "godot_exec",
        {
            "action": "call",
            "params": {"node_path": node_path, "method": method, "args": args},
        },
        timeout=timeout,
    )
    return payload.get("result")


def pid_stamp(mcp: McpClient) -> dict:
    return eval_code(
        mcp,
        "return {\"pid\": OS.get_process_id(), "
        "\"project\": ProjectSettings.globalize_path(\"res://\"), "
        f"\"tick\": get_node(\"{HOST_PATH}\").get_tick(), "
        f"\"checksum\": str(get_node(\"{HOST_PATH}\").get_checksum())}}",
    )


def state_json(mcp: McpClient) -> dict:
    raw = eval_code(mcp, f"return get_node(\"{HOST_PATH}\").get_state_json()")
    if not isinstance(raw, str):
        raise SmokeError(f"get_state_json returned {type(raw).__name__}, expected string")
    return json.loads(raw)


def tile_block(state: dict, x: int, y: int) -> str | None:
    for tile in state.get("world", {}).get("tiles", []):
        if tile.get("x") == x and tile.get("y") == y:
            return tile.get("block")
    return None


def wait_until(description: str, predicate, timeout: float = 5.0,
               interval: float = 0.15) -> None:
    deadline = time.monotonic() + timeout
    last = None
    while time.monotonic() < deadline:
        last = predicate()
        if last:
            return
        time.sleep(interval)
    raise SmokeError(f"timed out waiting for {description} (last={last!r})")


def launch_hint(repo_root: str) -> str:
    native = repo_root.replace("\\", "/")
    if len(native) >= 2 and native[1] == ":":
        wsl_client = f"/mnt/{native[0].lower()}{native[2:]}/client"
    else:
        wsl_client = f"{native}/client"
    return (
        "Launch the editor first (WSL2 Ubuntu, WSLg window):\n"
        f"  nohup godot4 --editor --path {wsl_client} >/tmp/mind-editor.log 2>&1 &\n"
        "  # from a Windows terminal:\n"
        "  wsl -d Ubuntu -e bash -lc "
        f"\"nohup godot4 --editor --path {wsl_client} >/tmp/mind-editor.log 2>&1 &\"\n"
        "Wait ~20 s, then re-run this smoke."
    )


# -- smoke flow ----------------------------------------------------------


def run(repo_root: str, mcp_bin: str, server_args: list[str], verbose: bool,
        keep_running: bool) -> int:
    section("mcp smoke: connect")
    mcp = McpClient(mcp_bin, server_args, verbose=verbose)
    ok(f"spawned {mcp_bin} (server v{mcp.server_version})")

    played = False
    try:
        # With no editor on the bridge port the health tool itself errors instead
        # of reporting bridge_connected=false. A headless editor import (CI runs
        # one) can momentarily hold/release the bridge, so retry briefly before
        # declaring the bridge absent.
        health = None
        deadline = time.time() + 20.0
        while True:
            try:
                health = mcp.call("godot_health", {"action": "check"})
            except ToolError:
                health = None
            if health and health.get("bridge_connected"):
                break
            if time.time() >= deadline:
                detail = "health tool error" if health is None else repr(health)
                fail(f"MCP bridge is not connected to a Godot editor ({detail})")
                print(launch_hint(repo_root), file=sys.stderr)
                return 2
            time.sleep(0.5)
        ok("bridge connected")

        # Step 0: identity preflight before any godot_game call.
        state = mcp.call("godot_editor_read", {"action": "state"})
        project_path = str(state.get("project_path", ""))
        if "mindustry-godot" not in project_path.replace("\\", "/"):
            fail(f"editor identity mismatch: project_path={project_path!r}")
            return 1
        # The runtime-side identity check (res:// path) runs after play, in the
        # pid-stamp eval -- godot_exec requires a connected game process.
        ok(f"identity: {project_path}")

        # Step 0b: start from a clean log buffer so step 9 is our own evidence.
        mcp.call("godot_log", {"action": "clear"})

        # Step 1: open the spine scene and play it explicitly.
        mcp.call(
            "godot_editor_edit",
            {"action": "open_scene", "params": {"path": SPINE_SCENE}},
        )
        mcp.call("godot_game", {"action": "play", "params": {"scene": SPINE_SCENE}})
        played = True
        wait_until(
            "runtime connection",
            lambda: mcp.call("godot_game", {"action": "status"}).get("runtime_connected"),
            timeout=40.0,
        )
        ok(f"playing {SPINE_SCENE} (runtime connected)")

        # Step 2: pid stamp + liveness.
        stamp = pid_stamp(mcp)
        pid = as_int(stamp.get("pid"))
        if pid is None:
            fail(f"pid stamp missing: {stamp!r}")
            return 1
        if "mindustry-godot" not in str(stamp.get("project", "")).replace("\\", "/"):
            fail(f"runtime identity mismatch: res:// -> {stamp.get('project')!r}")
            return 1
        ok(f"pid={pid} tick={stamp.get('tick')} checksum={stamp.get('checksum')}")

        # Step 3+4: load the scenario and step(60) while Playing. Both calls are
        # in ONE loop-free eval so the fixed-step pump cannot advance between
        # them; the checksum assertion is therefore exact.
        result = eval_code(
            mcp,
            f"var host = get_node(\"{HOST_PATH}\")\n"
            f"var loaded = host.load_scenario(\"{SCENARIO}\")\n"
            "var tick = host.step(60)\n"
            "return {\"pid\": OS.get_process_id(), \"loaded\": loaded, "
            "\"tick\": tick, \"checksum\": str(host.get_checksum())}",
        )
        if as_int(result.get("pid")) != pid:
            fail(f"pid changed mid-run: {pid} -> {result.get('pid')}")
            return 1
        if result.get("loaded") is not True:
            fail(f"load_scenario failed: {result!r}")
            return 1
        if as_int(result.get("tick")) != 60:
            fail(f"expected tick 60 after step(60), got {result.get('tick')!r}")
            return 1
        if result.get("checksum") != GOLDEN_CHECKSUM:
            fail(
                f"checksum mismatch: {result.get('checksum')!r} != {GOLDEN_CHECKSUM}"
            )
            return 1
        ok(f"load_scenario + step(60) -> tick=60 checksum={GOLDEN_CHECKSUM}")

        # Pause for the deterministic API/input steps (corrected §7c order).
        mcp.call(
            "godot_exec",
            {"action": "call", "params": {"node_path": HOST_PATH,
                                          "method": "set_paused",
                                          "args": [True]}},
        )
        paused = eval_code(mcp, f"return get_node(\"{HOST_PATH}\").is_paused()")
        if paused is not True:
            fail("set_paused(true) did not pause the sim")
            return 1
        ok("paused")

        # Step 5: API place/break.
        placed = call_func(mcp, HOST_PATH, "place_block", [3, 5, "stone-wall"])
        if placed is not True:
            fail(f"place_block(3,5,'stone-wall') -> {placed!r}")
            return 1
        block = tile_block(state_json(mcp), 3, 5)
        if block != "stone-wall":
            fail(f"tile (3,5) is {block!r}, expected 'stone-wall'")
            return 1
        broken = call_func(mcp, HOST_PATH, "break_block", [3, 5])
        if broken is not True:
            fail(f"break_block(3,5) -> {broken!r}")
            return 1
        block = tile_block(state_json(mcp), 3, 5)
        if block is not None:
            fail(f"tile (3,5) still {block!r} after break")
            return 1
        ok("API place_block/break_block round-trip")

        # Step 6: real input path. Resolve the tile center in viewport space,
        # click it, then step(1) so the paused pump advances (the click itself
        # applies through MindSimHost::_input immediately).
        screen = eval_code(
            mcp, f"return get_node(\"{CAMERA_PATH}\").tile_to_screen(7, 7)"
        )
        if not isinstance(screen, dict) or "x" not in screen or "y" not in screen:
            fail(f"tile_to_screen(7,7) -> {screen!r}")
            return 1
        tick_before = as_int(
            eval_code(mcp, f"return get_node(\"{HOST_PATH}\").get_tick()")
        )
        click = {"position": {"x": screen["x"], "y": screen["y"]}, "coords": "viewport",
                 "pressed": True}
        mcp.call("godot_input", {"action": "mouse_button",
                                 "params": {**click, "button": "MOUSE_BUTTON_LEFT"}})
        wait_until(
            "tile (7,7) placed by mouse",
            lambda: tile_block(state_json(mcp), 7, 7) == "stone-wall",
        )
        tick_after = as_int(eval_code(mcp, f"return get_node(\"{HOST_PATH}\").step(1)"))
        if tick_after != tick_before + 1:
            fail(f"step(1) did not advance tick: {tick_before} -> {tick_after}")
            return 1
        ok(f"mouse click placed (7,7), tick {tick_before} -> {tick_after}")

        mcp.call("godot_input", {"action": "mouse_button",
                                 "params": {"position": click["position"],
                                            "coords": "viewport", "pressed": True,
                                            "button": "MOUSE_BUTTON_RIGHT"}})
        wait_until(
            "tile (7,7) broken by mouse",
            lambda: tile_block(state_json(mcp), 7, 7) is None,
        )
        tick_before = as_int(
            eval_code(mcp, f"return get_node(\"{HOST_PATH}\").get_tick()")
        )
        tick_after = as_int(eval_code(mcp, f"return get_node(\"{HOST_PATH}\").step(1)"))
        if tick_after != tick_before + 1:
            fail(f"step(1) did not advance tick: {tick_before} -> {tick_after}")
            return 1
        ok(f"mouse click broke (7,7), tick {tick_before} -> {tick_after}")

        # Step 9: logs.
        logs = mcp.call("godot_log", {"action": "errors"})
        errors = logs.get("errors", [])
        if errors:
            fail(f"godot_log errors is not empty: {json.dumps(errors)[:600]}")
            return 1
        entries = mcp.call(
            "godot_log", {"action": "get", "params": {"count": 200}}
        ).get("entries", [])
        startup = [
            e for e in entries
            if "[I]" in str(e.get("message", "")) or "MindSimHost ready" in str(e.get("message", ""))
        ]
        if not startup:
            fail("no [I] startup lines in godot_log get")
            return 1
        ok(f"logs clean, {len(startup)} startup line(s) present")

        section("mcp smoke: PASS")
        return 0
    finally:
        if played and not keep_running:
            try:
                mcp.call("godot_game", {"action": "stop"})
                ok("game stopped")
            except SmokeError as err:
                print(f"  warn  stop failed: {err}", flush=True)
        mcp.close()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mcp-bin", default=None,
                        help="path to the open-godot-mcp binary")
    parser.add_argument("--repo-root", default=None,
                        help="mindustry-godot repo root (default: script parent)")
    parser.add_argument("--server-arg", action="append", default=[],
                        help="extra argument forwarded to the MCP server (repeatable)")
    parser.add_argument("--keep-running", action="store_true",
                        help="leave the game playing after the smoke")
    parser.add_argument("--verbose", action="store_true",
                        help="print raw MCP traffic")
    args = parser.parse_args(argv)

    repo_root = args.repo_root or os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    if not os.path.isfile(os.path.join(repo_root, "client", "project.godot")):
        fail(f"--repo-root does not look like the mindustry-godot repo: {repo_root}")
        return 2

    try:
        mcp_bin = find_mcp_bin(args.mcp_bin)
    except SmokeError as err:
        fail(str(err))
        return 2

    try:
        return run(repo_root, mcp_bin, args.server_arg, args.verbose, args.keep_running)
    except SmokeError as err:
        fail(str(err))
        return 1


if __name__ == "__main__":
    sys.exit(main())
