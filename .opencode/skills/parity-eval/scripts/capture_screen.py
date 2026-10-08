#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Capture the Java reference's X11 window (or a screen crop) to PNG.

The loop's Java display is Xwayland on a headless weston compositor: the X
root window carries no GPU pixels there, so mss/root grabs come back black,
while the Xvfb fallback composites normally. Window capture reads the live
client area with XGetImage and is the reliable path on both servers, so
`--window-title` finds a window by WM_NAME through Xlib (no xdotool needed).
Without a title the script grabs the monitor first and, when that image is
blank, retries the "Mindustry" window and records the fallback in the report.

Used by the parity-eval skill for the Java reference leg, where no in-engine
screenshot API exists. Requires mss (installed with computer-mcp); window
capture additionally uses python-xlib and Pillow from the same venv.

Examples:
    capture_screen.py --output run/java/step-01.png
    capture_screen.py --output win.png --window-title Mindustry
    capture_screen.py --output crop.png --rect 0,0,1280,720
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

try:
    import mss
    import mss.tools
except ImportError:  # pragma: no cover - environment guard
    sys.exit("mss is missing; run bootstrap.sh to create the MCP venv")

try:
    from PIL import Image
except ImportError:  # pragma: no cover - OCR/stats fall back to raw-byte math
    Image = None

try:
    import Xlib.display
    import Xlib.X
except ImportError:  # pragma: no cover - window capture degrades to xdotool
    Xlib = None

# The Java client's WM_NAME; used when a bare monitor grab comes back blank.
FALLBACK_WINDOW_TITLE = "Mindustry"
# Anything smaller than this is a helper surface, not the game window.
MIN_WINDOW_SIZE = 100


def window_rect(title: str) -> tuple[int, int, int, int] | None:
    """Return (x, y, w, h) for a window title via xdotool, or None."""
    try:
        ids = subprocess.run(
            ["xdotool", "search", "--name", title],
            capture_output=True, text=True, timeout=5, check=False,
        ).stdout.split()
        if not ids:
            return None
        geom = subprocess.run(
            ["xdotool", "getwindowgeometry", "--shell", ids[-1]],
            capture_output=True, text=True, timeout=5, check=False,
        ).stdout
        values = dict(
            line.split("=", 1) for line in geom.splitlines() if "=" in line
        )
        return (int(values["X"]), int(values["Y"]), int(values["WIDTH"]), int(values["HEIGHT"]))
    except (OSError, KeyError, ValueError):
        return None


def find_window(title: str):
    """Return (display, window) for the first WM_NAME match, or (display, None)."""
    display = Xlib.display.Display()
    found = None

    def walk(win) -> None:
        nonlocal found
        if found is not None:
            return
        try:
            name = win.get_wm_name()
            geom = win.get_geometry()
            if (
                name
                and title.lower() in str(name).lower()
                and geom.width >= MIN_WINDOW_SIZE
                and geom.height >= MIN_WINDOW_SIZE
            ):
                found = win
                return
            for child in win.query_tree().children:
                walk(child)
        except Exception:  # noqa: BLE001 - broken/unmapped windows are skipped
            return

    walk(display.screen().root)
    return display, found


def capture_window(title: str):
    """Capture a window matched by WM_NAME.

    Returns (PIL image, (x, y, w, h), window id). Raises LookupError when no
    window matches and RuntimeError when the capture prerequisites are missing.
    """
    if Xlib is None or Image is None:
        raise RuntimeError("window capture needs python-xlib and Pillow")
    display, win = find_window(title)
    if win is None:
        raise LookupError(f"no window matching {title!r}")
    geom = win.get_geometry()
    raw = win.get_image(0, 0, geom.width, geom.height, Xlib.X.ZPixmap, 0xFFFFFFFF)
    image = Image.frombytes("RGB", (geom.width, geom.height), raw.data, "raw", "BGRX")
    try:
        # Called on the destination window: window origin in root coordinates.
        origin = display.screen().root.translate_coords(win, 0, 0)
        x, y = origin.x, origin.y
    except Exception:  # noqa: BLE001 - position is informational only
        x = y = 0
    return image, (x, y, geom.width, geom.height), hex(win.id)


def stats_from_image(image) -> dict[str, float]:
    """Mean/stddev of the luma channel; shared by every capture mode."""
    hist = image.convert("L").histogram()
    total = sum(hist)
    if total == 0:
        return {}
    mean = sum(i * n for i, n in enumerate(hist)) / total
    var = sum(n * (i - mean) ** 2 for i, n in enumerate(hist)) / total
    return {"mean": round(mean, 2), "std": round(var ** 0.5, 2)}


def stats_from_png(path: Path) -> dict[str, float]:
    if Image is None:
        return {}
    with Image.open(path) as img:
        return stats_from_image(img)


def _blank(stats: dict[str, float]) -> bool:
    std = stats.get("std")
    return isinstance(std, (int, float)) and std < 1.5


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--rect", help="crop as x,y,width,height")
    parser.add_argument("--window-title", help="capture this WM_NAME window directly")
    parser.add_argument("--monitor", type=int, default=1, help="mss monitor index")
    parser.add_argument("--ocr", action="store_true", help="run tesseract on the result")
    args = parser.parse_args()

    image = None
    grab = None
    rect: tuple[int, int, int, int] | None = None
    capture = "monitor"
    window_id: str | None = None
    notes: list[str] = []

    if args.rect:
        try:
            rect = tuple(int(v) for v in args.rect.split(","))  # type: ignore[assignment]
            if len(rect) != 4:
                raise ValueError
        except ValueError:
            parser.error("--rect must be x,y,width,height")
        capture = "rect"
    elif args.window_title:
        if Xlib is not None and Image is not None:
            try:
                image, rect, window_id = capture_window(args.window_title)
            except (LookupError, RuntimeError) as exc:
                parser.error(str(exc))
            capture = "window"
        else:
            # Without Xlib, fall back to the old xdotool geometry + crop.
            rect = window_rect(args.window_title)
            if rect is None:
                notes.append("window not found via xdotool; captured full monitor")
            capture = "rect" if rect else "monitor"

    if image is None:
        with mss.mss() as sct:
            if rect:
                x, y, w, h = rect
                region = {"left": x, "top": y, "width": w, "height": h}
            else:
                region = sct.monitors[args.monitor]
            grab = sct.grab(region)
        if Image is not None:
            image = Image.frombytes("RGB", grab.size, grab.rgb)

    if image is None:
        # Pillow is missing; keep the legacy direct-to-PNG path.
        args.output.parent.mkdir(parents=True, exist_ok=True)
        mss.tools.to_png(grab.rgb, grab.size, output=str(args.output))
    else:
        # On Xwayland the X root has no GPU pixels: a blank monitor grab means
        # the game window must be read directly.
        if capture == "monitor" and Xlib is not None and _blank(stats_from_image(image)):
            try:
                image, rect, window_id = capture_window(FALLBACK_WINDOW_TITLE)
                capture = "window"
                notes.append(
                    f"monitor grab was blank; captured the {FALLBACK_WINDOW_TITLE} window"
                )
            except (LookupError, RuntimeError):
                pass
        args.output.parent.mkdir(parents=True, exist_ok=True)
        image.save(args.output)

    report: dict[str, object] = {
        "path": str(args.output),
        "capture": capture,
        "width": image.width if image is not None else grab.size[0],
        "height": image.height if image is not None else grab.size[1],
        "captured_at": datetime.now(timezone.utc).isoformat(),
    }
    if rect:
        report["region"] = list(rect)
    if window_id:
        report["window_id"] = window_id
    if notes:
        report["note"] = "; ".join(notes)
    if image is not None:
        report.update(stats_from_image(image))
    else:
        report.update(stats_from_png(args.output))
    std = report.get("std")
    report["blank"] = bool(isinstance(std, (int, float)) and std < 1.5)

    if args.ocr:
        try:
            ocr = subprocess.run(
                ["tesseract", str(args.output), "-"],
                capture_output=True, text=True, timeout=30, check=False,
            )
            report["ocr"] = ocr.stdout.strip()
        except OSError:
            report["ocr_error"] = "tesseract not installed"

    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
