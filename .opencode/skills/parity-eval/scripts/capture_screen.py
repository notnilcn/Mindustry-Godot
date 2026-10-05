#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Capture the X11 screen (or a crop) to PNG and report non-blank statistics.

Used by the parity-eval skill for the Java reference leg, where no in-engine
screenshot API exists. Requires mss (installed with computer-mcp).

Examples:
    capture_screen.py --output run/java/step-01.png
    capture_screen.py --output crop.png --rect 0,0,1280,720
    capture_screen.py --window-title "Mindustry" --output win.png
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
except ImportError:  # OCR/stats fall back to raw-byte math
    Image = None


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


def stats_from_png(path: Path) -> dict[str, float]:
    if Image is None:
        return {}
    with Image.open(path) as img:
        gray = img.convert("L")
        hist = gray.histogram()
        total = sum(hist)
        if total == 0:
            return {}
        mean = sum(i * n for i, n in enumerate(hist)) / total
        var = sum(n * (i - mean) ** 2 for i, n in enumerate(hist)) / total
        return {"mean": round(mean, 2), "std": round(var ** 0.5, 2)}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--rect", help="crop as x,y,width,height")
    parser.add_argument("--window-title", help="locate window via xdotool")
    parser.add_argument("--monitor", type=int, default=1, help="mss monitor index")
    parser.add_argument("--ocr", action="store_true", help="run tesseract on the result")
    args = parser.parse_args()

    rect: tuple[int, int, int, int] | None = None
    if args.rect:
        try:
            rect = tuple(int(v) for v in args.rect.split(","))  # type: ignore[assignment]
            if len(rect) != 4:
                raise ValueError
        except ValueError:
            parser.error("--rect must be x,y,width,height")
    elif args.window_title:
        rect = window_rect(args.window_title)

    with mss.mss() as sct:
        if rect:
            x, y, w, h = rect
            region = {"left": x, "top": y, "width": w, "height": h}
        else:
            region = sct.monitors[args.monitor]
        grab = sct.grab(region)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    mss.tools.to_png(grab.rgb, grab.size, output=str(args.output))

    report: dict[str, object] = {
        "path": str(args.output),
        "width": grab.size[0],
        "height": grab.size[1],
        "captured_at": datetime.now(timezone.utc).isoformat(),
    }
    if rect:
        report["region"] = list(rect)
    elif args.window_title:
        report["warning"] = "window not found via xdotool; captured full monitor"

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
