#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Windows driver for the Java reference leg of the twin-verification sweep.

Replaces the Linux `parity-eval` helpers (`window_ctl.py`, `java_input.py`,
`capture_screen.py`) that no longer exist on this host. The Java client is a
GLFW/SDL window, so everything here is user32 + Pillow `ImageGrab`.

Usage (run from the repo root; `python tools/windows/java_leg.py <cmd> ...`):

  launch [--jar PATH] [--wait SECS]   start Mindustry.jar, wait for its window
  find                                print the window title, hwnd and rects
  move [--x N] [--y N] [--width N] [--height N]
                                      move/resize so the *client* area is WxH
  shot --out PATH                     capture the client area to a PNG
  click --x N --y N [--button left|right] [--double]
                                      click client-relative coordinates
  mouse --x N --y N                   move the pointer to client coordinates
  key --keys "ctrl+a,enter,a"         press chords / single keys
  text --text "hello"                 type literal text
  stop                                close the Java client (SIGTERM-ish)
  data-dir                            print the Mindustry data dir on Windows

The reference pose is a 1152x648 client area at (0,0) — the geometry every
archived frame under `parity/twin/PAR-####/` uses. `move` then `find` to confirm.
"""

from __future__ import annotations

import argparse
import ctypes
import ctypes.wintypes as wt
import os
import subprocess
import sys
import time
from pathlib import Path

user32 = ctypes.WinDLL("user32", use_last_error=True)
kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)

# The host runs a HiDPI display (2560x1600 at 150%). Without per-monitor DPI
# awareness this process sees virtualized rectangles and ImageGrab returns the
# wrong region, which silently yields a cropped frame. Declare awareness before
# any window call.
_DpiAware = False


def become_dpi_aware() -> None:
    global _DpiAware
    if _DpiAware:
        return
    try:
        ctypes.WinDLL("shcore").SetProcessDpiAwareness(2)  # PROCESS_PER_MONITOR_DPI_AWARE
    except Exception:  # pragma: no cover - older Windows
        try:
            user32.SetProcessDPIAware()
        except Exception:
            pass
    _DpiAware = True


become_dpi_aware()

DEFAULT_JAR = Path(r"C:\Users\Clinton\g\code_examples\Mindustry\desktop\build\libs\Mindustry.jar")
DEFAULT_TITLE = "Mindustry"
CLIENT_SIZE = (1152, 648)

SW_RESTORE = 9
MOUSEEVENTF_LEFTDOWN = 0x0002
MOUSEEVENTF_LEFTUP = 0x0004
MOUSEEVENTF_RIGHTDOWN = 0x0008
MOUSEEVENTF_RIGHTUP = 0x0010
MOUSEEVENTF_WHEEL = 0x0800
INPUT_MOUSE = 0
INPUT_KEYBOARD = 1
KEYEVENTF_KEYUP = 0x0002
KEYEVENTF_UNICODE = 0x0004


class MOUSEINPUT(ctypes.Structure):
    _fields_ = [("dx", wt.LONG), ("dy", wt.LONG), ("mouseData", wt.DWORD),
                ("dwFlags", wt.DWORD), ("time", wt.DWORD), ("dwExtraInfo", ctypes.POINTER(wt.ULONG))]


class KEYBDINPUT(ctypes.Structure):
    _fields_ = [("wVk", wt.WORD), ("wScan", wt.WORD), ("dwFlags", wt.DWORD),
                ("time", wt.DWORD), ("dwExtraInfo", ctypes.POINTER(wt.ULONG))]


class INPUT(ctypes.Structure):
    class _U(ctypes.Union):
        _fields_ = [("mi", MOUSEINPUT), ("ki", KEYBDINPUT)]

    _anonymous_ = ("u",)
    _fields_ = [("type", wt.DWORD), ("u", _U)]


def _window_titles() -> list[tuple[int, str]]:
    out: list[tuple[int, str]] = []

    @ctypes.WINFUNCTYPE(wt.BOOL, wt.HWND, wt.LPARAM)
    def _cb(hwnd, _lparam):
        if not user32.IsWindowVisible(hwnd):
            return True
        length = user32.GetWindowTextLengthW(hwnd)
        if length == 0:
            return True
        buf = ctypes.create_unicode_buffer(length + 1)
        user32.GetWindowTextW(hwnd, buf, length + 1)
        # The Godot editor window is titled "<scene> - Mindustry-Godot - Godot Engine";
        # only the Java client is titled "Mindustry".
        if "Mindustry" in buf.value and "Godot" not in buf.value:
            out.append((hwnd, buf.value))
        return True

    user32.EnumWindows(_cb, 0)
    return out


def find_window(title: str = DEFAULT_TITLE) -> int:
    matches = _window_titles()
    for hwnd, text in matches:
        if text.startswith(title):
            return hwnd
    if matches:
        return matches[0][0]
    raise SystemExit("java_leg: no Mindustry window found (launch it first)")


def client_rect(hwnd: int) -> tuple[int, int, int, int]:
    rect = wt.RECT()
    user32.GetClientRect(hwnd, ctypes.byref(rect))
    origin = wt.POINT(0, 0)
    user32.ClientToScreen(hwnd, ctypes.byref(origin))
    return origin.x, origin.y, rect.right - rect.left, rect.bottom - rect.top


def window_rect(hwnd: int) -> tuple[int, int, int, int]:
    rect = wt.RECT()
    user32.GetWindowRect(hwnd, ctypes.byref(rect))
    return rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top


def frame_insets(hwnd: int) -> tuple[int, int, int, int]:
    """(left, top, right, bottom) frame thickness around the client area."""
    wx, wy, ww, wh = window_rect(hwnd)
    cx, cy, cw, ch = client_rect(hwnd)
    return cx - wx, cy - wy, (wx + ww) - (cx + cw), (wy + wh) - (cy + ch)


def set_client_size(hwnd: int, width: int, height: int, x: int = 0, y: int = 0) -> None:
    left, top, right, bottom = frame_insets(hwnd)
    user32.ShowWindow(hwnd, SW_RESTORE)
    user32.MoveWindow(hwnd, x, y, width + left + right, height + top + bottom, True)


def _send(*inputs: INPUT) -> None:
    array = (INPUT * len(inputs))(*inputs)
    sent = user32.SendInput(len(inputs), array, ctypes.sizeof(INPUT))
    if sent != len(inputs):
        raise SystemExit(f"java_leg: SendInput failed ({ctypes.get_last_error()})")


def _mouse(mouse_input: MOUSEINPUT) -> INPUT:
    item = INPUT(type=INPUT_MOUSE)
    item.mi = mouse_input
    return item


def client_to_screen(hwnd: int, x: float, y: float) -> tuple[int, int]:
    point = wt.POINT(int(x), int(y))
    user32.ClientToScreen(hwnd, ctypes.byref(point))
    return point.x, point.y


def move_pointer(hwnd: int, x: float, y: float) -> None:
    sx, sy = client_to_screen(hwnd, x, y)
    user32.SetCursorPos(sx, sy)
    _send(_mouse(MOUSEINPUT(dx=sx, dy=sy, mouseData=0, dwFlags=0x0001, time=0, dwExtraInfo=None)))


def click(hwnd: int, x: float, y: float, button: str = "left", double: bool = False) -> None:
    sx, sy = client_to_screen(hwnd, x, y)
    user32.SetCursorPos(sx, sy)
    time.sleep(0.05)
    down = MOUSEEVENTF_LEFTDOWN if button == "left" else MOUSEEVENTF_RIGHTDOWN
    up = MOUSEEVENTF_LEFTUP if button == "left" else MOUSEEVENTF_RIGHTUP
    sequence = [(down, 0), (up, 60)]
    if double:
        sequence += [(down, 60), (up, 0)]
    for flags, pause_ms in sequence:
        _send(_mouse(MOUSEINPUT(dx=sx, dy=sy, mouseData=0, dwFlags=flags, time=0, dwExtraInfo=None)))
        if pause_ms:
            time.sleep(pause_ms / 1000.0)


KEYS = {
    "ctrl": 0x11, "shift": 0x10, "alt": 0x12, "win": 0x5B,
    "enter": 0x0D, "return": 0x0D, "esc": 0x1B, "escape": 0x1B, "tab": 0x09,
    "space": 0x20, "backspace": 0x08, "delete": 0x2E, "insert": 0x2D,
    "home": 0x24, "end": 0x23, "pageup": 0x21, "pagedown": 0x22,
    "up": 0x26, "down": 0x28, "left": 0x25, "right": 0x27,
    "f1": 0x70, "f2": 0x71, "f3": 0x72, "f4": 0x73, "f5": 0x74,
}


def _vk(name: str) -> int:
    key = name.strip().lower()
    if key in KEYS:
        return KEYS[key]
    if len(key) == 1 and key.isalnum():
        return ord(key.upper())
    if key.startswith("0x"):
        return int(key, 16)
    raise SystemExit(f"java_leg: unknown key {name!r}")


def _key_event(vk: int, up: bool) -> INPUT:
    item = INPUT(type=INPUT_KEYBOARD)
    item.ki = KEYBDINPUT(wVk=vk, wScan=0, dwFlags=KEYEVENTF_KEYUP if up else 0, time=0, dwExtraInfo=None)
    return item


def press(keys: str) -> None:
    """`ctrl+a` presses ctrl, taps a, releases ctrl; commas separate chords."""
    for chord in keys.split(","):
        parts = [p for p in chord.replace("+", " ").split() if p]
        if not parts:
            continue
        vks = [_vk(p) for p in parts]
        for vk in vks:
            _send(_key_event(vk, False))
        for vk in reversed(vks):
            _send(_key_event(vk, True))
        time.sleep(0.05)


def type_text(text: str) -> None:
    for char in text:
        code = ord(char)
        down = INPUT(type=INPUT_KEYBOARD)
        down.ki = KEYBDINPUT(wVk=0, wScan=code, dwFlags=KEYEVENTF_UNICODE, time=0, dwExtraInfo=None)
        up = INPUT(type=INPUT_KEYBOARD)
        up.ki = KEYBDINPUT(wVk=0, wScan=code, dwFlags=KEYEVENTF_UNICODE | KEYEVENTF_KEYUP, time=0, dwExtraInfo=None)
        _send(down, up)
        time.sleep(0.02)


def capture(out: Path, hwnd: int) -> Path:
    from PIL import ImageGrab  # imported lazily so `find`/`launch` stay fast

    # ImageGrab reads the screen, so the window must be on top of every other one.
    user32.ShowWindow(hwnd, SW_RESTORE)
    user32.BringWindowToTop(hwnd)
    user32.SetForegroundWindow(hwnd)
    time.sleep(0.4)
    x, y, w, h = client_rect(hwnd)
    image = ImageGrab.grab(bbox=(x, y, x + w, y + h), all_screens=True)
    out.parent.mkdir(parents=True, exist_ok=True)
    image.save(out)
    print(f"{out} {image.width}x{image.height}")
    return out


def launch(jar: Path, wait: float, width: int = CLIENT_SIZE[0], height: int = CLIENT_SIZE[1]) -> int:
    if not jar.exists():
        raise SystemExit(f"java_leg: jar not found: {jar}")
    # Arc's SDL backend does not apply a post-launch WM_SIZE resize on Windows, so
    # the reference geometry has to come from the launcher flags (DesktopLauncher
    # reads `-maximized`, `-width`, `-height`) instead of resizing afterwards.
    subprocess.Popen(
        ["java", "-jar", str(jar), "-maximized", "false", "-width", str(width), "-height", str(height)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        creationflags=getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0),
    )
    deadline = time.time() + wait
    while time.time() < deadline:
        try:
            hwnd = find_window()
        except SystemExit:
            time.sleep(1.0)
            continue
        if client_rect(hwnd)[2] > 100:
            set_client_size(hwnd, width, height, 0, 0)
            time.sleep(1.0)
            print(f"window ready hwnd=0x{hwnd:x} client={client_rect(hwnd)}")
            return hwnd
        time.sleep(1.0)
    raise SystemExit(f"java_leg: no Mindustry window within {wait:.0f}s")


def stop() -> None:
    for hwnd, _title in _window_titles():
        user32.PostMessageW(hwnd, 0x0010, 0, 0)  # WM_CLOSE
    time.sleep(1.0)
    subprocess.run(["taskkill", "/IM", "java.exe", "/F"], capture_output=True, check=False)


def data_dir() -> None:
    appdata = os.environ.get("APPDATA", str(Path.home() / "AppData" / "Roaming"))
    print(Path(appdata) / "Mindustry")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Windows driver for the Java twin leg")
    sub = parser.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("launch")
    p.add_argument("--jar", default=str(DEFAULT_JAR))
    p.add_argument("--wait", type=float, default=120.0)
    p = sub.add_parser("find")
    p = sub.add_parser("move")
    p.add_argument("--x", type=int, default=0)
    p.add_argument("--y", type=int, default=0)
    p.add_argument("--width", type=int, default=CLIENT_SIZE[0])
    p.add_argument("--height", type=int, default=CLIENT_SIZE[1])
    p = sub.add_parser("shot")
    p.add_argument("--out", required=True)
    p = sub.add_parser("click")
    p.add_argument("--x", type=float, required=True)
    p.add_argument("--y", type=float, required=True)
    p.add_argument("--button", choices=["left", "right"], default="left")
    p.add_argument("--double", action="store_true")
    p = sub.add_parser("mouse")
    p.add_argument("--x", type=float, required=True)
    p.add_argument("--y", type=float, required=True)
    p = sub.add_parser("key")
    p.add_argument("--keys", required=True)
    p = sub.add_parser("text")
    p.add_argument("--text", required=True)
    sub.add_parser("stop")
    sub.add_parser("data-dir")

    args = parser.parse_args(argv)
    if args.cmd == "data-dir":
        data_dir()
        return 0
    if args.cmd == "stop":
        stop()
        return 0
    if args.cmd == "launch":
        launch(Path(args.jar), args.wait)
        return 0

    hwnd = find_window()
    if args.cmd == "find":
        print(f"hwnd=0x{hwnd:x} title={dict(_window_titles()).get(hwnd, '')!r}")
        print(f"window={window_rect(hwnd)} client={client_rect(hwnd)} insets={frame_insets(hwnd)}")
        return 0
    if args.cmd == "move":
        set_client_size(hwnd, args.width, args.height, args.x, args.y)
        time.sleep(0.6)
        print(f"client={client_rect(hwnd)}")
        return 0
    if args.cmd == "shot":
        capture(Path(args.out), hwnd)
        return 0
    if args.cmd == "click":
        click(hwnd, args.x, args.y, args.button, args.double)
        return 0
    if args.cmd == "mouse":
        move_pointer(hwnd, args.x, args.y)
        return 0
    if args.cmd == "key":
        press(args.keys)
        return 0
    if args.cmd == "text":
        type_text(args.text)
        return 0
    raise SystemExit(f"java_leg: unhandled command {args.cmd}")


if __name__ == "__main__":
    sys.exit(main())
