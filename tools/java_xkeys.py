#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""java_xkeys.py — deliver key events to the Java reference client by XSendEvent.

On the parity-session Xvfb displays the X input focus stays on the Godot window,
so XTEST keyboard input (pynput / `java_input.py type|key`) never reaches the
Java client: its text fields show a caret but take no characters
(java-leg-learnings.md 2026-10-08, 2026-10-14). Sending KeyPress/KeyRelease
straight to the client window with XSendEvent bypasses the focus: SDL accepts
the synthetic events and Arc text fields receive the characters (L01 T3,
2026-10-18). Focus the field with a real click first (`JW x y`).

Usage (run with the parity-eval venv python, DISPLAY set):
    java_xkeys.py [--title Mindustry] sand            # type characters
    java_xkeys.py @BackSpace @BackSpace @Escape        # @<X keysym name>
    java_xkeys.py @space                               # e.g. pause toggle
    java_xkeys.py --hold-ms 1000 d                     # hold D for 1 s (editor pan)

Each argument is either literal text (one key per character) or `@Keysym`.
Prints one JSON object. Exit 1 when the window is not found.
"""

from __future__ import annotations

import argparse
import json
import sys
import time

from Xlib import X, XK, display
from Xlib.protocol import event

ALIASES = {" ": "space", "\n": "Return", "\t": "Tab"}


def find_window(win, title):
    for child in win.query_tree().children:
        try:
            if child.get_wm_name() == title:
                return child
        except Exception:  # noqa: BLE001 - windows can vanish mid-walk
            pass
        found = find_window(child, title)
        if found is not None:
            return found
    return None


def key_names(tokens):
    for token in tokens:
        if token.startswith("@") and len(token) > 1:
            yield token[1:]
        else:
            for ch in token:
                yield ALIASES.get(ch, ch)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--title", default="Mindustry")
    ap.add_argument("--gap-ms", type=int, default=80, help="delay between keys")
    ap.add_argument("--hold-ms", type=int, default=0,
                    help="hold each key this long between press and release (held movement keys)")
    ap.add_argument("keys", nargs="+")
    a = ap.parse_args()
    d = display.Display()
    root = d.screen().root
    win = find_window(root, a.title)
    if win is None:
        print(json.dumps({"ok": False, "error": f"no window titled {a.title!r}"}))
        return 1
    sent = []
    for name in key_names(a.keys):
        keysym = XK.string_to_keysym(name)
        keycode = d.keysym_to_keycode(keysym)
        if not keycode:
            print(json.dumps({"ok": False, "error": f"no keycode for {name!r}"}))
            return 1
        shift = X.ShiftMask if (len(name) == 1 and name.isupper()) else 0
        for cls in (event.KeyPress, event.KeyRelease):
            ev = cls(time=X.CurrentTime, root=root, window=win, same_screen=1, child=X.NONE,
                     root_x=0, root_y=0, event_x=0, event_y=0, state=shift, detail=keycode)
            win.send_event(ev, propagate=True)
            d.sync()
            if cls is event.KeyPress and a.hold_ms > 0:
                time.sleep(a.hold_ms / 1000.0)
        sent.append(name)
        time.sleep(a.gap_ms / 1000.0)
    print(json.dumps({"ok": True, "window": hex(win.id), "sent": sent}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
