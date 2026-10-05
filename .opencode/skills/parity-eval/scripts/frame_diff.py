#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Compare two frames from the Java reference and the Godot client.

Emits machine-readable metrics so a text-only model can reason about visual
parity without seeing the images: size/blank checks, changed-pixel ratio,
coarse layout distance, palette distance, and optional OCR text.

Examples:
    frame_diff.py --java j.png --godot g.png --out-json diff.json
    frame_diff.py --java j.png --godot g.png --out-heatmap heat.png \
        --out-side-by-side side.png
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

try:
    from PIL import Image, ImageChops, ImageStat
except ImportError:  # pragma: no cover - environment guard
    sys.exit("Pillow is missing; run bootstrap.sh to create the MCP venv")


def stats(img: Image.Image) -> dict[str, float]:
    st = ImageStat.Stat(img)
    means = [round(v, 2) for v in st.mean]
    stds = [round(v, 2) for v in st.stddev]
    gray_std = round(ImageStat.Stat(img.convert("L")).stddev[0], 2)
    return {
        "mean_rgb": means,
        "std_rgb": stds,
        "std_luma": gray_std,
        "blank": gray_std < 1.5,
    }


def changed_fraction(diff: Image.Image, tolerance: int) -> dict[str, float]:
    masks = [ch.point(lambda p, t=tolerance: 255 if p > t else 0) for ch in diff.split()]
    mask = ImageChops.lighter(ImageChops.lighter(masks[0], masks[1]), masks[2])
    hist = mask.histogram()
    total = diff.width * diff.height
    return {
        "changed_fraction": round(hist[255] / total, 5) if total else 0.0,
        "tolerance": tolerance,
    }


def diff_percentiles(gray: Image.Image) -> dict[str, float]:
    hist = gray.histogram()
    total = sum(hist) or 1
    out: dict[str, float] = {}
    targets = {"p50": 0.50, "p90": 0.90, "p95": 0.95, "p99": 0.99}
    for name, q in targets.items():
        acc = 0
        for value, count in enumerate(hist):
            acc += count
            if acc / total >= q:
                out[name] = value
                break
    return out


def coarse_layout(a: Image.Image, b: Image.Image, cells: int = 16) -> dict[str, float]:
    size = (cells, cells)
    ca = a.resize(size, Image.Resampling.BOX)
    cb = b.resize(size, Image.Resampling.BOX)
    d = ImageChops.difference(ca, cb)
    means = ImageStat.Stat(d).mean
    return {
        "cells": cells,
        "mean_abs": round(sum(means) / len(means), 2),
        "max_channel_mean": round(max(means), 2),
    }


def palette_distance(a: Image.Image, b: Image.Image, bits: int = 4) -> float:
    """Total-variation distance between quantized color histograms (0..1)."""
    levels = 1 << bits
    shift = 8 - bits

    def histogram(img: Image.Image) -> list[int]:
        small = img.resize((64, 64), Image.Resampling.BOX)
        hist = [0] * (levels ** 3)
        for r, g, b_ in small.getdata():
            idx = ((r >> shift) << (2 * bits)) | ((g >> shift) << bits) | (b_ >> shift)
            hist[idx] += 1
        return hist

    ha, hb = histogram(a), histogram(b)
    total = sum(ha) or 1
    tv = 0.5 * sum(abs(x - y) for x, y in zip(ha, hb)) / total
    return round(tv, 4)


def ocr(path: Path) -> str | None:
    if not shutil.which("tesseract"):
        return None
    try:
        out = subprocess.run(
            ["tesseract", str(path), "-"],
            capture_output=True, text=True, timeout=60, check=False,
        )
        return out.stdout.strip()
    except OSError:
        return None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--java", required=True, type=Path)
    parser.add_argument("--godot", required=True, type=Path)
    parser.add_argument("--out-json", type=Path)
    parser.add_argument("--out-heatmap", type=Path)
    parser.add_argument("--out-side-by-side", type=Path)
    parser.add_argument("--tolerance", type=int, default=12,
                        help="per-channel delta counted as a changed pixel (default 12)")
    parser.add_argument("--ocr", action="store_true")
    args = parser.parse_args()

    for path in (args.java, args.godot):
        if not path.is_file():
            parser.error(f"missing frame: {path}")

    a = Image.open(args.java).convert("RGB")
    b = Image.open(args.godot).convert("RGB")

    report: dict[str, object] = {
        "java": str(args.java),
        "godot": str(args.godot),
        "compared_at": datetime.now(timezone.utc).isoformat(),
        "java_size": list(a.size),
        "godot_size": list(b.size),
        "size_match": a.size == b.size,
        "java_stats": stats(a),
        "godot_stats": stats(b),
    }

    if a.size != b.size:
        report["resized_for_compare"] = True
        b = b.resize(a.size, Image.Resampling.BILINEAR)

    diff = ImageChops.difference(a, b)
    report["diff"] = {
        "mean_abs_rgb": [round(v, 2) for v in ImageStat.Stat(diff).mean],
        **changed_fraction(diff, args.tolerance),
        **diff_percentiles(diff.convert("L")),
    }
    report["coarse_layout"] = coarse_layout(a, b)
    report["palette_distance"] = palette_distance(a, b)

    if args.out_heatmap:
        args.out_heatmap.parent.mkdir(parents=True, exist_ok=True)
        diff.convert("L").point(lambda p: min(255, p * 3)).save(args.out_heatmap)
        report["heatmap"] = str(args.out_heatmap)

    if args.out_side_by_side:
        args.out_side_by_side.parent.mkdir(parents=True, exist_ok=True)
        canvas = Image.new("RGB", (a.width * 2 + 8, a.height), (24, 24, 24))
        canvas.paste(a, (0, 0))
        canvas.paste(b, (a.width + 8, 0))
        canvas.save(args.out_side_by_side)
        report["side_by_side"] = str(args.out_side_by_side)

    if args.ocr:
        java_text = ocr(args.java)
        godot_text = ocr(args.godot)
        report["ocr"] = {
            "java": java_text,
            "godot": godot_text,
            "text_match": java_text == godot_text,
        }

    if args.out_json:
        args.out_json.parent.mkdir(parents=True, exist_ok=True)
        args.out_json.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
