#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""ASCII zoom for X11 screenshots — a text-only stand-in for vision/OCR.

Renders a crop of a PNG as characters so a text-only model can locate and
(read, for large glyphs) UI elements.  Modes:
  --mode ramp   grayscale ramp (default), good for layout
  --mode max    per-block max luminance, good for bright text on dark bg
  --mode bin    per-block max >= threshold -> '#', for reading glyphs

Usage:
  ascii_zoom.py IMG --rect x,y,w,h [--scale N] [--mode max] [--threshold 140]
"""
from __future__ import annotations

import argparse
import sys
from PIL import Image

RAMP = " .:-=+*#%@"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("image")
    ap.add_argument("--rect", default=None, help="x,y,w,h crop (default full image)")
    ap.add_argument("--scale", type=int, default=2, help="pixels per character block")
    ap.add_argument("--mode", choices=["ramp", "max", "bin"], default="ramp")
    ap.add_argument("--threshold", type=int, default=140)
    ap.add_argument("--invert", action="store_true", help="bright bg, dark text")
    args = ap.parse_args()

    im = Image.open(args.image).convert("L")
    if args.rect:
        x, y, w, h = (int(v) for v in args.rect.split(","))
        im = im.crop((x, y, x + w, y + h))
    if args.invert:
        im = Image.eval(im, lambda v: 255 - v)
    W, H = im.size
    s = max(1, args.scale)
    out = []
    for by in range(0, H, s):
        row = []
        for bx in range(0, W, s):
            vals = []
            for yy in range(by, min(by + s, H)):
                for xx in range(bx, min(bx + s, W)):
                    vals.append(im.getpixel((xx, yy)))
            v = max(vals)
            if args.mode == "bin":
                row.append("#" if v >= args.threshold else " ")
            elif args.mode == "max":
                row.append(RAMP[min(9, v * 10 // 256)])
            else:
                m = sum(vals) // len(vals)
                row.append(RAMP[min(9, m * 10 // 256)])
        out.append("".join(row))
    print(f"# {args.image} rect={args.rect or 'full'} size={W}x{H} scale={s} mode={args.mode}")
    print("\n".join(out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
