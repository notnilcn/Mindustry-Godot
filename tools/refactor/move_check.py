#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Verbatim-move checker for refactor splits.

Compares the code lines of OLD files at a git revision against NEW files in the
working tree, as multisets, after normalizing away what a pure move is allowed
to change (indentation, visibility modifiers, `use`/`mod` lines, headers, doc
comments, lone braces, `impl` headers). Anything printed is a line whose count
differs: review each one. A clean pure move prints only structural lines.

usage: move_check.py --base <rev> --old <path>... --new <path-or-dir>...
"""
import argparse
import collections
import re
import subprocess
import sys
from pathlib import Path

VIS = re.compile(r"\bpub(\s*\((crate|super|self|in\s+[\w:]+)\))?\s+")
SKIP = [
    re.compile(r"^(pub\s+)?use\s"),
    re.compile(r"^(pub(\(.*?\))?\s+)?mod\s+\w+\s*;"),
    re.compile(r"^(pub(\(.*?\))?\s+)?mod\s+\w+\s*\{$"),  # inline `mod tests {` header
    re.compile(r"^//"),  # comments, docs, SPDX/citation headers
    re.compile(r"^#\[godot_api"),
    re.compile(r"^#\[cfg\(test\)\]$"),
    re.compile(r"^#!\[allow"),
    re.compile(r"^(unsafe\s+)?impl\b.*\{$"),
    re.compile(r"^[\}\)\];,]*$"),
]


def norm(line: str):
    s = line.strip()
    if not s:
        return None
    s = VIS.sub("", s)
    for rx in SKIP:
        if rx.search(s):
            return None
    return s


USE_START = re.compile(r"^(pub(\s*\([^)]*\))?\s+)?use\s")


def count(text: str, bag: collections.Counter):
    in_use = False  # inside a multi-line `use … { … };`
    for line in text.splitlines():
        s = line.strip()
        if in_use:
            in_use = not s.endswith(";")
            continue
        if USE_START.match(s) and not s.endswith(";"):
            in_use = True
            continue
        n = norm(line)
        if n is not None:
            bag[n] += 1


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", required=True)
    ap.add_argument("--old", nargs="+", required=True)
    ap.add_argument("--new", nargs="+", required=True)
    a = ap.parse_args()
    old, new = collections.Counter(), collections.Counter()
    for p in a.old:
        count(subprocess.run(["git", "show", f"{a.base}:{p}"], check=True,
                             capture_output=True, text=True).stdout, old)
    for p in a.new:
        path = Path(p)
        files = sorted(path.rglob("*.rs")) if path.is_dir() else [path]
        for f in files:
            count(f.read_text(encoding="utf-8"), new)
    removed = old - new
    added = new - old
    for label, bag in (("only-in-old (LOST?)", removed), ("only-in-new (ADDED?)", added)):
        print(f"== {label}: {sum(bag.values())} lines")
        for line, n in sorted(bag.items()):
            print(f"{n:4} {line}")
    sys.exit(1 if removed else 0)


if __name__ == "__main__":
    main()
