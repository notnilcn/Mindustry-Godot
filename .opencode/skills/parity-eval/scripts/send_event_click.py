#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Synthetic XSendEvent click into the Mindustry window (bypasses XTest).

Usage: send_event_click.py X Y [left|right|middle] [motion]
  X Y are round-trip-safe SCREEN coordinates (window origin + client offset).
  motion: also send a MotionNotify first (some SDL paths need enter/hover).

Fallback for the xrdp :10 display where XTest clicks are dropped by SDL or the
window never generated an EnterNotify (java-leg-learnings.md 2026-10-10).
Prints JSON with the target window and the coordinates used. Run with the
parity-eval helper venv python and DISPLAY set.
"""
import json
import sys
import time

from Xlib import X, display, protocol
from Xlib.ext import xtest  # noqa: F401  (kept for parity with other helpers)

BUTTONS = {"left": 1, "middle": 2, "right": 3}


def find_window(d, name="Mindustry"):
    root = d.screen().root
    stack = [root]
    while stack:
        w = stack.pop()
        try:
            wname = w.get_wm_name()
            if wname and name.lower() in str(wname).lower():
                return w
        except Exception:
            pass
        try:
            stack.extend(w.query_tree().children)
        except Exception:
            pass
    return None


def main():
    d = display.Display()
    root = d.screen().root
    win = find_window(d)
    if win is None:
        print(json.dumps({"error": "Mindustry window not found"}))
        sys.exit(1)
    x, y = int(sys.argv[1]), int(sys.argv[2])
    detail = BUTTONS[sys.argv[3]] if len(sys.argv) > 3 else 1
    motion = len(sys.argv) > 4 and sys.argv[4] == "motion"

    origin = win.translate_coords(root, 0, 0)  # root point (0,0) in win coords
    # win origin in root coords = -origin
    wx, wy = -origin.x, -origin.y
    ex, ey = x - wx, y - wy

    if motion:
        ev = protocol.event.MotionNotify(
            time=X.CurrentTime, root=root, window=win, same_screen=1,
            child=X.NONE, root_x=x, root_y=y, event_x=ex, event_y=ey,
            state=0, detail=0, flags=X.PointerWindow)
        win.send_event(ev, event_mask=X.PointerMotionMask)

    press = protocol.event.ButtonPress(
        time=X.CurrentTime, root=root, window=win, same_screen=1, child=X.NONE,
        root_x=x, root_y=y, event_x=ex, event_y=ey, state=0, detail=detail)
    release = protocol.event.ButtonRelease(
        time=X.CurrentTime, root=root, window=win, same_screen=1, child=X.NONE,
        root_x=x, root_y=y, event_x=ex, event_y=ey, state=1 << (detail + 7),
        detail=detail)
    win.send_event(press, event_mask=X.ButtonPressMask)
    d.sync()
    time.sleep(0.08)
    win.send_event(release, event_mask=X.ButtonReleaseMask)
    d.sync()
    time.sleep(0.2)
    print(json.dumps({
        "cmd": "send_event_click", "window": hex(win.id),
        "origin": [wx, wy], "screen": [x, y], "event_xy": [ex, ey],
        "button": sys.argv[3] if len(sys.argv) > 3 else "left", "motion": motion,
    }))


if __name__ == "__main__":
    main()
