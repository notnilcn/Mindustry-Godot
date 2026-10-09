#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""List / raise / move / resize X11 windows by exact WM_NAME.

The Java reference and the Godot editor share one display in the single-display
parity setup, so the Java window must be unobscured before
`capture_screen.py --window-title Mindustry` can read it. This works through
python-xlib, so xdotool/wmctrl are not needed.

Examples:
    window_ctl.py list
    window_ctl.py raise Mindustry
    window_ctl.py move Mindustry 40 40
    window_ctl.py unmaximize Mindustry      # before resize when maximized
    window_ctl.py resize Mindustry 1152 648

Run it with a python that has python-xlib (the computer-mcp uv-tool python
does): ~/.local/share/uv/tools/computer-mcp/bin/python
"""

from __future__ import annotations

import argparse
import sys

try:
    from Xlib import X, display, protocol
except ImportError:  # pragma: no cover - environment guard
    sys.exit("python-xlib is missing; run with the computer-mcp uv-tool python")

MATCH_HELP = "exact WM_NAME (case-insensitive); use --contains for a substring"


def collect_windows(root) -> list[tuple[object, str]]:
    found: list[tuple[object, str]] = []

    def walk(win) -> None:
        try:
            name = win.get_wm_name()
            if name:
                found.append((win, str(name)))
        except Exception:
            pass
        try:
            for child in win.query_tree().children:
                walk(child)
        except Exception:
            pass

    walk(root)
    return found


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)

    p_list = sub.add_parser("list", help="list titled windows with geometry")
    p_list.add_argument("--contains", help="substring filter")

    for name, helptext in (
        ("raise", "stack the window above its siblings (unobscure)"),
        ("move", "move the window to x y"),
        ("resize", "resize the window to width height"),
        ("unmaximize", "remove the maximized WM state so resize takes effect"),
    ):
        p = sub.add_parser(name, help=helptext)
        p.add_argument("title", help=MATCH_HELP)
        p.add_argument("--contains", action="store_true", help="match by substring")
        if name == "move":
            p.add_argument("x", type=int)
            p.add_argument("y", type=int)
        elif name == "resize":
            p.add_argument("width", type=int)
            p.add_argument("height", type=int)

    args = parser.parse_args()

    d = display.Display()
    root = d.screen().root
    windows = collect_windows(root)

    if args.action == "list":
        rows = [
            (name, win)
            for win, name in windows
            if not args.contains or args.contains.lower() in name.lower()
        ]
        for name, win in rows:
            try:
                geo = win.get_geometry()
                print(
                    f"0x{win.id:x} x={geo.x} y={geo.y} "
                    f"w={geo.width} h={geo.height} name={name!r}"
                )
            except Exception as exc:  # window vanished mid-walk
                print(f"0x{win.id:x} name={name!r} geometry error: {exc}")
        return 0

    needle = args.title.lower()
    matches = [
        win
        for win, name in windows
        if ((needle in name.lower()) if args.contains else (name.lower() == needle))
    ]
    if not matches:
        names = sorted({name for _, name in windows})
        print(f"no window matching {args.title!r}; titled windows: {names}", file=sys.stderr)
        return 2

    for win in matches:
        if args.action == "raise":
            win.configure(stack_mode=X.Above)
        elif args.action == "move":
            win.configure(x=args.x, y=args.y)
        elif args.action == "resize":
            win.configure(width=args.width, height=args.height)
        elif args.action == "unmaximize":
            # A maximized window ignores XConfigureWindow; ask the WM to drop
            # the state first (same message a titlebar double-click sends).
            atom = d.intern_atom("_NET_WM_STATE")
            state = [
                d.intern_atom("_NET_WM_STATE_MAXIMIZED_HORZ"),
                d.intern_atom("_NET_WM_STATE_MAXIMIZED_VERT"),
            ]
            event = protocol.event.ClientMessage(
                window=win, client_type=atom, data=(32, [0, *state, 1, 0])
            )
            root.send_event(
                event, event_mask=X.SubstructureRedirectMask | X.SubstructureNotifyMask
            )
        d.sync()
        geo = win.get_geometry()
        print(
            f"{args.action} 0x{win.id:x} -> x={geo.x} y={geo.y} "
            f"w={geo.width} h={geo.height}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
