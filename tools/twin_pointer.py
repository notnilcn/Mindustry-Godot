#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""twin_pointer.py — move the REAL X pointer for the Godot twin leg.

`godot_input mouse_motion` injects a synthetic event; it does not move the OS
cursor, and Godot's hover UI (`get_viewport().get_mouse_position()`, hover
boxes, tooltips) reads the real pointer. This helper warps that pointer with
pynput without touching the Java client (the parity-eval `java_input.py`
focuses the Mindustry window first, which fails when the Java client is not
running).

Run it with a python that has pynput (the parity-eval helper venv:
`$HOME/.local/share/parity-eval/venv/bin/python`) and with `DISPLAY` set:

    DISPLAY=:14 tools/twin_pointer.py move 704 489
    DISPLAY=:14 tools/twin_pointer.py pos

`tools/twin.sh` resolves the interpreter and adds the window origin (GP).

Usage:
  twin_pointer.py move X Y [--jiggle]   # absolute screen coordinates
  twin_pointer.py pos                   # print the current pointer position

`--jiggle` nudges 2 px and back first so a hover handler that only reacts to
pointer *motion* (not position) still sees a move when the pointer was already
parked on the target.
"""

from __future__ import annotations

import argparse
import json
import sys
import time

try:
    from pynput.mouse import Controller as MouseController
except ImportError:  # pragma: no cover - environment guard
    sys.exit(
        "pynput is missing; run twin_pointer.py with the parity-eval helper venv "
        "python ($PARITY_PY) and DISPLAY set"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)

    p_move = sub.add_parser("move", help="warp the pointer to absolute screen coordinates")
    p_move.add_argument("x", type=int)
    p_move.add_argument("y", type=int)
    p_move.add_argument(
        "--jiggle",
        action="store_true",
        help="nudge 2 px and back first to force a motion event",
    )
    p_move.add_argument(
        "--settle",
        type=float,
        default=0.15,
        help="seconds to sleep after the move (default 0.15)",
    )

    sub.add_parser("pos", help="print the current pointer position")

    args = parser.parse_args()
    mouse = MouseController()

    if args.action == "move":
        if args.jiggle:
            mouse.position = (args.x + 2, args.y + 2)
            time.sleep(0.05)
        mouse.position = (args.x, args.y)
        time.sleep(max(args.settle, 0.0))
        print(json.dumps({"move": [args.x, args.y], "pos": list(mouse.position)}))
        return 0

    if args.action == "pos":
        print(json.dumps({"pos": list(mouse.position)}))
        return 0

    return 2


if __name__ == "__main__":
    sys.exit(main())
