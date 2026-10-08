#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Orientation-level declaration scan for Java, Rust and GDScript sources.

`/seed-gaps` uses this to build candidate hypotheses cheaply: it prints the
type and function declarations of a file or directory tree as

    path:line<TAB>kind<TAB>name

so an agent can spot an upstream surface that has no port counterpart without
reading every file. The scan is heuristic (line regex, no AST) and only
orientation-grade: read the real files before recording anything.

    surface_scan.py ../Mindustry/core/src/mindustry/core --kind funcs
    surface_scan.py client/rust/mind-core/src/ui --pattern dialog
    surface_scan.py client/rust/mind-core/src --json

Languages are picked per extension (`.java`, `.rs`, `.gd`); `--lang` limits the
walk to one of them. Directories are walked recursively; build/vendor
directories are skipped unless `--all` is passed.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

SKIP_DIRS = {
    ".git",
    ".gradle",
    ".idea",
    ".import",
    "__pycache__",
    "addons",
    "build",
    "dist",
    "node_modules",
    "target",
}
LANGUAGE_BY_SUFFIX = {".java": "java", ".rs": "rust", ".gd": "gd"}
FUNC_KINDS = frozenset({"fn", "method", "func"})

JAVA_DECL = re.compile(
    r"^\s*(?:(?:public|protected|private|abstract|final|static|sealed|non-sealed|strictfp)\s+)*"
    r"(class|interface|enum|record|@interface)\s+(\w+)"
)
JAVA_METHOD = re.compile(r"(?P<name>[A-Za-z_]\w*)\s*\(")
JAVA_ANNOTATION = re.compile(r"@\w+(?:\([^)]*\))?")
JAVA_KEYWORDS = frozenset(
    "if for while switch catch synchronized return new super this assert do "
    "throw case yield when".split()
)
# Text before the method name that marks a call/expression, not a declaration.
JAVA_REJECT = re.compile(
    r"[.=();]|->|::|\b(?:new|return|throw|assert|super|this|case|yield)\b"
)

RUST_FN = re.compile(
    r"^\s*(?:pub(?:\s*\([^)]*\))?\s+)?"
    r"(?:async\s+|unsafe\s+|const\s+|extern\s+\"[^\"]*\"\s+)*fn\s+(\w+)"
)
RUST_TYPE = re.compile(
    r"^\s*(?:pub(?:\s*\([^)]*\))?\s+)?"
    r"(struct|enum|trait|union|type|mod|impl)\b\s*(.*)$"
)

GD_FUNC = re.compile(r"^\s*(?:static\s+)?func\s+(\w+)")
GD_DECL = re.compile(r"^\s*(class_name|signal|enum|const)\s+(\w+)")


def java_decls(line: str) -> list[tuple[str, str]]:
    stripped = line.lstrip()
    if stripped.startswith(("//", "*", "/*")):
        return []
    match = JAVA_DECL.match(line)
    if match:
        return [(match.group(1), match.group(2))]
    for match in JAVA_METHOD.finditer(line):
        name = match.group("name")
        if name in JAVA_KEYWORDS:
            continue
        start = match.start("name")
        if start and line[start - 1] in ".@":
            continue
        before = JAVA_ANNOTATION.sub("", line[:start]).strip()
        if not before or JAVA_REJECT.search(before):
            continue
        return [("method", name)]
    return []


def rust_decls(line: str) -> list[tuple[str, str]]:
    stripped = line.lstrip()
    if stripped.startswith(("//", "*")):
        return []
    match = RUST_FN.match(line)
    if match:
        return [("fn", match.group(1))]
    match = RUST_TYPE.match(line)
    if not match:
        return []
    kind, rest = match.group(1), match.group(2)
    if kind == "impl":
        text = re.split(r"[\{;]| where ", rest, maxsplit=1)[0]
        text = re.sub(r"^<[^>]*>\s*", "", text.strip())
        text = re.sub(r"\s+", " ", text)
        return [("impl", text[:80])] if text else []
    name = re.match(r"[A-Za-z_]\w*", rest.strip())
    return [(kind, name.group(0))] if name else []


def gd_decls(line: str) -> list[tuple[str, str]]:
    stripped = line.lstrip()
    if stripped.startswith("#"):
        return []
    match = GD_FUNC.match(line)
    if match:
        return [("func", match.group(1))]
    match = GD_DECL.match(line)
    return [(match.group(1), match.group(2))] if match else []


SCANNERS = {"java": java_decls, "rust": rust_decls, "gd": gd_decls}


def iter_files(paths: list[str], lang: str | None, include_all: bool):
    seen: set[Path] = set()
    for raw in paths:
        path = Path(raw)
        if not path.exists():
            print(f"surface_scan: no such path: {raw}", file=sys.stderr)
            continue
        if path.is_file():
            candidates = [path]
        else:
            candidates = sorted(p for p in path.rglob("*") if p.is_file())
        for candidate in candidates:
            found = LANGUAGE_BY_SUFFIX.get(candidate.suffix)
            if found is None or (lang and found != lang):
                continue
            if not include_all and path.is_dir():
                relative = candidate.relative_to(path)
                if any(part in SKIP_DIRS for part in relative.parts[:-1]):
                    continue
            resolved = candidate.resolve()
            if resolved in seen:
                continue
            seen.add(resolved)
            yield found, candidate


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("paths", nargs="+", help="file or directory to scan")
    parser.add_argument(
        "--lang",
        choices=("java", "rust", "gd"),
        help="only scan this language (default: all recognized extensions)",
    )
    parser.add_argument(
        "--kind",
        choices=("all", "funcs", "types"),
        default="all",
        help="funcs: functions/methods only; types: declarations only",
    )
    parser.add_argument("--pattern", help="case-insensitive regex on the declaration name")
    parser.add_argument("--json", action="store_true", help="JSON array output")
    parser.add_argument("--all", action="store_true", help="include build/vendor directories")
    args = parser.parse_args()

    matcher = re.compile(args.pattern, re.IGNORECASE) if args.pattern else None
    rows: list[tuple[str, int, str, str]] = []
    for language, file in iter_files(args.paths, args.lang, args.all):
        try:
            text = file.read_text(encoding="utf-8", errors="replace")
        except OSError as error:
            print(f"surface_scan: {file}: {error}", file=sys.stderr)
            continue
        scanner = SCANNERS[language]
        for number, line in enumerate(text.splitlines(), 1):
            for kind, name in scanner(line):
                is_func = kind in FUNC_KINDS
                if args.kind == "funcs" and not is_func:
                    continue
                if args.kind == "types" and is_func:
                    continue
                if matcher and not matcher.search(name):
                    continue
                rows.append((str(file), number, kind, name))

    if args.json:
        payload = [
            {"file": file, "line": number, "kind": kind, "name": name}
            for file, number, kind, name in rows
        ]
        print(json.dumps(payload, indent=2))
    else:
        for file, number, kind, name in rows:
            print(f"{file}:{number}\t{kind}\t{name}")

    if not rows and not any(Path(raw).exists() for raw in args.paths):
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
