#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""XRecord watcher: print delivered input events (button/motion/key) for a few seconds.

Usage: record_input.py SECONDS

Delivery proof for a Java-leg action that looks dead: run this in the
background, issue the action, and check whether the event reached the window
(java-leg-learnings.md). Run with the parity-eval helper venv python and
DISPLAY set.
"""
import os
import signal
import sys
import time

from Xlib import X, display
from Xlib.ext import record
from Xlib.protocol import rq

NAMES = {2: "KeyPress", 3: "KeyRelease", 4: "ButtonPress", 5: "ButtonRelease",
         6: "MotionNotify", 7: "EnterNotify", 8: "LeaveNotify", 9: "FocusIn", 10: "FocusOut"}


def main():
    secs = float(sys.argv[1]) if len(sys.argv) > 1 else 8
    d = display.Display()
    ctx = d.record_create_context(0, [record.AllClients], [{
        "core_requests": (0, 0),
        "core_replies": (0, 0),
        "ext_requests": (0, 0, 0, 0),
        "ext_replies": (0, 0, 0, 0),
        "delivered_events": (0, 0),
        "device_events": (X.KeyPress, X.MotionNotify),
        "errors": (0, 0),
        "client_started": False,
        "client_died": False,
        "server_started": False,
        "server_stopped": False,
    }])

    start = time.time()

    def callback(reply):
        if reply.category != record.FromServer:
            return
        if reply.client_swapped or not len(reply.data) or reply.data[0] < 2:
            return
        data = reply.data
        while len(data):
            try:
                ev, data = rq.EventField(None).parse_binary_value(data, d.display, None, None)
            except Exception:
                break
            kind = NAMES.get(ev.type, ev.type)
            win = getattr(ev, "window", None)
            child = getattr(ev, "child", None)
            wid = hex(win.id) if win is not None and hasattr(win, "id") else str(win)
            cid = hex(child.id) if child is not None and hasattr(child, "id") else str(child)
            detail = getattr(ev, "detail", "")
            rx = getattr(ev, "root_x", getattr(ev, "event_x", ""))
            ry = getattr(ev, "root_y", getattr(ev, "event_y", ""))
            print(f"{time.time()-start:6.2f} {kind} win={wid} child={cid} detail={detail} x={rx} y={ry} send={getattr(ev,'send_event',False)}", flush=True)

    def finish(signum, frame):
        # record_enable_context() blocks until the context ends, so stop from
        # the alarm: print the marker and leave; the server frees the context
        # when the client disconnects.
        print("record done", flush=True)
        os._exit(0)

    signal.signal(signal.SIGALRM, finish)
    signal.setitimer(signal.ITIMER_REAL, max(secs, 0.1))
    print(f"recording for {secs}s", flush=True)
    d.record_enable_context(ctx, callback)


if __name__ == "__main__":
    main()
