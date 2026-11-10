#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Print the Godot-visible API surface of the gdext crates, one sorted line per item.

Lines: `func <Class>::<name>`, `signal <Class>::<name>`, `class <Class>`.
Run before and after a refactor; the diff must be empty unless the plan says otherwise.
"""
import re
import sys
from pathlib import Path

ROOTS = sys.argv[1:] or ["client/rust/mind-gdext/src", "client/rust/phantom-camera-rust/src"]
IMPL_RE = re.compile(r"^\s*impl\s+(?:[\w:]+\s+for\s+)?(\w+)\b")
CLASS_RE = re.compile(r"#\[derive\([^)]*GodotClass")
STRUCT_RE = re.compile(r"^\s*pub(?:\([^)]*\))?\s+struct\s+(\w+)|^\s*struct\s+(\w+)")
FN_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:unsafe\s+)?fn\s+(\w+)")
RENAME_RE = re.compile(r"rename\s*=\s*(\w+)")

out = set()
for root in ROOTS:
    for path in sorted(Path(root).rglob("*.rs")):
        lines = path.read_text(encoding="utf-8").splitlines()
        current = None
        pending = None  # ("func"|"signal", rename or None)
        want_class = False
        for line in lines:
            if CLASS_RE.search(line):
                want_class = True
            if want_class:
                m = STRUCT_RE.match(line)
                if m:
                    out.add(f"class {m.group(1) or m.group(2)}")
                    want_class = False
            m = IMPL_RE.match(line)
            if m:
                current = m.group(1)
            s = line.strip()
            if s.startswith("#[func") :
                r = RENAME_RE.search(s)
                pending = ("func", r.group(1) if r else None)
                continue
            if s.startswith("#[signal"):
                pending = ("signal", None)
                continue
            if pending:
                m = FN_RE.match(line)
                if m:
                    kind, rename = pending
                    out.add(f"{kind} {current}::{rename or m.group(1)}")
                    pending = None
for item in sorted(out):
    print(item)
