#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Java reference leg input helper: focus the Mindustry client window (by
WM_NAME), then pynput mouse/keyboard actions.

One-shot focused input for the parity-eval Java leg: every call re-activates
the window (EWMH ClientMessage `_NET_ACTIVE_WINDOW` + `set_input_focus`) before
the action, so a click cannot be swallowed as the activation click. Run with
the parity-eval helper venv python and DISPLAY set:

    PARITY_PY="${PARITY_PY:-$HOME/.local/share/parity-eval/venv/bin/python}"
    DISPLAY=:10 "$PARITY_PY" .opencode/skills/parity-eval/scripts/java_input.py click 310 216 left

Usage:
  java_input.py focus
  java_input.py hover X Y
  java_input.py move X Y
  java_input.py click X Y [left|right|middle]
  java_input.py dblclick X Y [left|right|middle] [gap_ms]
  java_input.py mousedown X Y [left|right|middle]   # hold (a world button_down can stay logically held)
  java_input.py mouseup X Y [left|right|middle]     # release a held button
  java_input.py drag X1 Y1 X2 Y2 [left|right|middle] [ms]
  java_input.py scroll N            # positive = up
  java_input.py key NAME [hold_ms]  # esc/enter/space/shift/ctrl/alt/tab/backspace/delete/up/down/left/right/<char>
  java_input.py keydown NAME
  java_input.py keyup NAME
  java_input.py type TEXT
  java_input.py seq '[["keydown","shift"],["key","1",50],["keyup","shift"],["sleep",120]]'

`seq` focuses once and runs every action in one process — use it for timed key
combos (in-game block select) and for batching a fast-forward chain. Every
subcommand prints one JSON object; coordinates are SCREEN coordinates (window
origin + client offset).
"""
import json
import sys
import time

from Xlib import X, display, protocol
from pynput.keyboard import Controller as KeyController, Key
from pynput.mouse import Controller as MouseController, Button

BUTTONS = {"left": Button.left, "right": Button.right, "middle": Button.middle}
KEYS = {
    "esc": Key.esc, "escape": Key.esc, "enter": Key.enter, "return": Key.enter,
    "space": Key.space, "shift": Key.shift, "ctrl": Key.ctrl, "alt": Key.alt,
    "tab": Key.tab, "backspace": Key.backspace, "delete": Key.delete,
    "up": Key.up, "down": Key.down, "left": Key.left, "right": Key.right,
    "home": Key.home, "end": Key.end, "page_up": Key.page_up, "page_down": Key.page_down,
    "comma": ",", "period": ".", "slash": "/", "semicolon": ";", "apostrophe": "'",
    "grave": "`", "minus": "-", "equals": "=", "bracketleft": "[", "bracketright": "]",
    "backslash": "\\",
}

SEQ_COMMANDS = (
    "hover", "move", "click", "dblclick", "mousedown", "mouseup", "drag",
    "scroll", "key", "keydown", "keyup", "type", "sleep", "focus",
)


def find_window(display_obj, name="Mindustry"):
    root = display_obj.screen().root
    stack = [root]
    while stack:
        win = stack.pop()
        try:
            wname = win.get_wm_name()
            if wname and name.lower() in str(wname).lower():
                return win
        except Exception:
            pass
        try:
            stack.extend(win.query_tree().children)
        except Exception:
            pass
    return None


def focus(display_obj):
    win = find_window(display_obj)
    if win is None:
        print(json.dumps({"error": "Mindustry window not found"}))
        sys.exit(1)
    root = display_obj.screen().root
    ev = protocol.event.ClientMessage(
        window=win, client_type=display_obj.intern_atom("_NET_ACTIVE_WINDOW"),
        data=(32, [1, X.CurrentTime, 0, 0, 0]))
    root.send_event(ev, event_mask=X.SubstructureRedirectMask | X.SubstructureNotifyMask)
    display_obj.sync()
    time.sleep(0.2)
    win.set_input_focus(X.RevertToParent, X.CurrentTime)
    display_obj.sync()
    time.sleep(0.3)
    return win


def parse_button(args, default=Button.left):
    if args and args[0] in BUTTONS:
        return BUTTONS[args[0]], args[0]
    return default, "left"


def parse_key(name):
    return KEYS.get(name.lower(), name if len(name) == 1 else name)


def run_action(mouse, keyboard, cmd, args, out):
    """Perform one action (no focus); args are strings. Raises ValueError."""
    if cmd in ("hover", "move"):
        x, y = int(args[0]), int(args[1])
        mouse.position = (x, y)
        time.sleep(0.5)
        out["pos"] = tuple(mouse.position)
    elif cmd == "click":
        x, y = int(args[0]), int(args[1])
        button, name = parse_button(args[2:])
        mouse.position = (x, y)
        time.sleep(0.5)
        mouse.press(button)
        time.sleep(0.15)
        mouse.release(button)
        time.sleep(0.15)
        out["click"] = [x, y]
        out["button"] = name
    elif cmd == "dblclick":
        x, y = int(args[0]), int(args[1])
        button, name = parse_button(args[2:])
        gap = (int(args[3]) if len(args) > 3 and args[3].isdigit() else 120) / 1000.0
        mouse.position = (x, y)
        time.sleep(0.5)
        for _ in range(2):
            mouse.press(button)
            time.sleep(0.08)
            mouse.release(button)
            time.sleep(gap)
        out["dblclick"] = [x, y]
        out["button"] = name
    elif cmd in ("mousedown", "mouseup"):
        # mousedown X Y [button] / mouseup X Y [button]
        x, y = int(args[0]), int(args[1])
        button, name = parse_button(args[2:])
        mouse.position = (x, y)
        time.sleep(0.5)
        if cmd == "mousedown":
            mouse.press(button)
        else:
            mouse.release(button)
        time.sleep(0.15)
        out[cmd] = [x, y]
        out["button"] = name
    elif cmd == "drag":
        x1, y1, x2, y2 = (int(v) for v in args[:4])
        button, name = parse_button(args[4:])
        dur = (int(args[5]) if len(args) > 5 and args[5].isdigit() else 300) / 1000.0
        mouse.position = (x1, y1)
        time.sleep(0.3)
        mouse.press(button)
        steps = max(8, int(dur * 60))
        for i in range(1, steps + 1):
            mouse.position = (x1 + (x2 - x1) * i / steps, y1 + (y2 - y1) * i / steps)
            time.sleep(dur / steps)
        mouse.release(button)
        time.sleep(0.2)
        out["drag"] = [x1, y1, x2, y2]
        out["button"] = name
    elif cmd == "scroll":
        n = int(args[0])
        mouse.scroll(0, n)
        time.sleep(0.3)
        out["scroll"] = n
    elif cmd in ("key", "keydown", "keyup"):
        name = args[0]
        key = parse_key(name)
        if cmd == "key":
            hold = (int(args[1]) if len(args) > 1 and args[1].isdigit() else 60) / 1000.0
            keyboard.press(key)
            time.sleep(hold)
            keyboard.release(key)
        elif cmd == "keydown":
            keyboard.press(key)
        else:
            keyboard.release(key)
        time.sleep(0.2)
        out["key"] = name
    elif cmd == "type":
        keyboard.type(args[0])
        time.sleep(0.2)
        out["typed"] = args[0]
    elif cmd == "sleep":
        ms = int(args[0]) if args else 100
        time.sleep(ms / 1000.0)
        out["sleep"] = ms
    elif cmd == "focus":
        out["focused"] = True
    else:
        raise ValueError(f"unknown cmd {cmd}")


def main():
    d = display.Display()
    mouse = MouseController()
    keyboard = KeyController()
    cmd = sys.argv[1]
    if cmd == "seq":
        actions = json.loads(sys.argv[2])
        focus(d)
        results = []
        for entry in actions:
            if not isinstance(entry, list) or not entry:
                raise SystemExit(f"seq entry must be [cmd, ...]: {entry!r}")
            name, *args = entry
            if name not in SEQ_COMMANDS:
                raise SystemExit(f"unknown seq cmd {name!r} (allowed: {', '.join(SEQ_COMMANDS)})")
            out = {}
            if name == "focus":
                focus(d)
                out["focused"] = True
            else:
                run_action(mouse, keyboard, name, [str(a) for a in args], out)
            results.append({"cmd": name, **out})
        print(json.dumps({"cmd": "seq", "actions": results}))
        return
    focus(d)
    out = {"cmd": cmd}
    if cmd in SEQ_COMMANDS and cmd != "focus":
        run_action(mouse, keyboard, cmd, sys.argv[2:], out)
    elif cmd == "focus":
        out["focused"] = True
    else:
        print(json.dumps({"error": f"unknown cmd {cmd}"}))
        sys.exit(2)
    print(json.dumps(out))


if __name__ == "__main__":
    main()
