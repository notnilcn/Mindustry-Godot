#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Port the Phantom Camera demo scenes from the GDScript oracle to the Rust addon.

Ported from: ``parity/pcam_oracle/phantom_camera/examples/**`` (Phantom Camera
0.11.0.3, MIT, Marcus Skov). The oracle is read-only; the ported copies live in
``client/addons/phantom_camera_rust/`` and name the native GDExtension classes
(``client/rust/phantom-camera-rust/src/lib.rs::REGISTERED_CLASSES``) instead of
binding addon ``.gd`` scripts.

What it does, per file, in this order
-------------------------------------
1. ``[ext_resource]`` / ``[node]`` / ``[sub_resource]`` / ``[gd_resource]``
   rewriting (``.tscn`` and ``.tres`` only):
   * a node whose ``script = ExtResource(id)`` resolves to an oracle
     ``scripts/**_with_class_name.gd`` becomes ``type="<NativeClass>"`` and the
     dead ``script =`` line is dropped;
   * a ``[sub_resource type="Resource"]`` (or a ``.tres`` ``[gd_resource]``)
     carrying such a script becomes ``type="<NativeClass>"`` (``script_class=``
     is dropped too: it names a *script* class and no longer exists);
   * every ``[ext_resource]`` that is now unreferenced is deleted.
2. The editor-only ``align_{transform,position,rotation}_with_view = Callable()``
   lines are stripped: the Callables are set by the GDScript editor plugin at
   runtime and the Rust port does not export them.
3. Every remaining ``res://addons/phantom_camera/`` path is rewritten to
   ``res://addons/phantom_camera_rust/`` (byte-level, so binary assets and
   ``*.import`` ``source_file=`` lines are covered and untouched bytes stay
   byte-identical).

The demo helper scripts (``examples/scripts/**.gd``) carry no oracle path and no
addon script binding, so they come out byte-identical to the oracle copies --
that is the contract: both sides run the *same* driver script.

Usage
-----
    python client/addons/phantom_camera_rust/dev/rewrite_examples.py
    python client/addons/phantom_camera_rust/dev/rewrite_examples.py --check

``--check`` builds the tree in memory, compares it against what is on disk and
exits 1 on any drift; it writes nothing. Without it the tree is regenerated
(stale files under ``examples/`` and ``fonts/`` are pruned).

Only Python 3 stdlib is used.
"""

from __future__ import annotations

import argparse
import hashlib
import re
import sys
from pathlib import Path

# dev/ -> phantom_camera_rust/ -> addons/ -> client/ -> repo root
REPO_ROOT = Path(__file__).resolve().parents[4]
ORACLE = REPO_ROOT / "parity" / "pcam_oracle" / "phantom_camera"
ADDON = REPO_ROOT / "client" / "addons" / "phantom_camera_rust"
LIB_RS = REPO_ROOT / "client" / "rust" / "phantom-camera-rust" / "src" / "lib.rs"

#: oracle-relative dirs that are ported, and where they land in the addon.
TREES = (("examples", "examples"), ("fonts", "fonts"))

ORACLE_PREFIX = b"res://addons/phantom_camera/"
PORT_PREFIX = b"res://addons/phantom_camera_rust/"

EXT_RESOURCE_RE = re.compile(r"^\[ext_resource\b(?P<attrs>[^\]]*)\]\s*$")
BLOCK_RE = re.compile(r"^\[(?P<kind>node|sub_resource|gd_resource|resource)\b(?P<attrs>[^\]]*)\]")
ATTR_RE = re.compile(r'(?P<key>[A-Za-z_][A-Za-z0-9_]*)(?:=(?:"(?P<quoted>[^"]*)"|(?P<bare>[^\s\]]+)))?')
SCRIPT_LINE_RE = re.compile(r'^script = ExtResource\("(?P<id>[^"]+)"\)\s*$')
TYPE_ATTR_RE = re.compile(r'(?<=\s)type="[^"]*"')
SCRIPT_CLASS_ATTR_RE = re.compile(r'\s+script_class="[^"]*"')
CALLABLE_LINE_RE = re.compile(r"^align_(?:transform|position|rotation)_with_view = Callable\(\)\s*$")
IMPORTSRC_RE = re.compile(r'^source_file="(?P<src>[^"]+)"', re.MULTILINE)
IMPORTHASH_RE = re.compile(r'(?<=-)[0-9a-f]{32}(?=\.[A-Za-z0-9.]+")')
CLASS_NAME_RE = re.compile(r"^class_name\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)", re.MULTILINE)
REGISTERED_RE = re.compile(
    r"pub const REGISTERED_CLASSES:\s*&\[&str\]\s*=\s*&\[(?P<body>.*?)\];", re.DOTALL
)


class PortError(Exception):
    """A scene/resource the rewriter cannot fully port."""


def read_text(path: Path) -> str:
    """Decode UTF-8, preserving BOM and line endings exactly."""
    return path.read_bytes().decode("utf-8")


def registered_classes() -> list[str]:
    """The native class names exported by the crate (single source of truth)."""
    match = REGISTERED_RE.search(read_text(LIB_RS))
    if match is None:
        raise PortError(f"REGISTERED_CLASSES not found in {LIB_RS}")
    return re.findall(r'"([^"]+)"', match.group("body"))


def oracle_script_classes() -> dict[str, str]:
    """``scripts/<...>.gd`` (oracle-relative) -> native class name.

    Derived from the oracle's own ``class_name`` declarations, so a new oracle
    release or a new class in the crate is picked up without edits here. Every
    mapped class must be in :func:`registered_classes`.
    """
    known = set(registered_classes())
    mapping: dict[str, str] = {}
    for gd in sorted((ORACLE / "scripts").rglob("*.gd")):
        match = CLASS_NAME_RE.search(read_text(gd))
        if match is None:
            continue
        name = match.group("name")
        if name not in known:
            raise PortError(
                f"{gd.relative_to(ORACLE).as_posix()} declares class_name {name}, "
                f"which is not exported by {LIB_RS}"
            )
        mapping[gd.relative_to(ORACLE).as_posix()] = name
    if not mapping:
        raise PortError(f"no class_name found under {ORACLE / 'scripts'}")
    return mapping


def parse_attrs(attrs: str) -> dict[str, str]:
    out: dict[str, str] = {}
    for match in ATTR_RE.finditer(attrs):
        key = match.group("key")
        value = match.group("quoted")
        if value is None:
            value = match.group("bare") or ""
        out.setdefault(key, value)
    return out


def _blocks(lines: list[str]) -> list[tuple[int, int, str]]:
    """``(first_line, end_line, kind)`` for every ``[...]`` block header.

    ``end_line`` is the index of the next block header (or ``len(lines)``), so
    the block body is ``lines[first_line + 1:end_line]``.
    """
    headers = [(i, m.group("kind")) for i, line in enumerate(lines) if (m := BLOCK_RE.match(line))]
    windows: list[tuple[int, int, str]] = []
    for index, (start, kind) in enumerate(headers):
        end = headers[index + 1][0] if index + 1 < len(headers) else len(lines)
        windows.append((start, end, kind))
    return windows


def _rewrite_type(line: str, native: str) -> str:
    return TYPE_ATTR_RE.sub(f'type="{native}"', line, count=1)


def port_scene(text: str, rel: str, script_classes: dict[str, str]) -> tuple[str, list[str]]:
    """Rewrite one ``.tscn``/``.tres``. Returns ``(new_text, report)``."""
    lines = text.split("\n")
    # A .tres puts the binding in the [resource] block but the type in [gd_resource].
    gd_resource = next((i for i, line in enumerate(lines) if line.startswith("[gd_resource")), None)

    # ext_resource id -> oracle-relative path, for the addon *.gd* bindings only.
    addon_scripts: dict[str, str] = {}
    for line in lines:
        if not (m := EXT_RESOURCE_RE.match(line)):
            continue
        attrs = parse_attrs(m.group("attrs"))
        path = attrs.get("path", "")
        if attrs.get("type") != "Script" or not path.startswith(str(ORACLE_PREFIX, "utf-8")):
            continue
        oracle_rel = path[len(str(ORACLE_PREFIX, "utf-8")) :]
        if oracle_rel in script_classes:
            addon_scripts[attrs.get("id", "")] = oracle_rel

    if not addon_scripts:
        return text, []

    drop: set[int] = set()
    report: list[str] = []

    for start, end, kind in _blocks(lines):
        body = range(start + 1, end)
        script_ids = [
            m.group("id")
            for i in body
            if (m := SCRIPT_LINE_RE.match(lines[i])) and m.group("id") in addon_scripts
        ]
        if not script_ids:
            continue
        native = script_classes[addon_scripts[script_ids[0]]]
        if len(script_ids) > 1:
            report.append(f"{rel}: block at line {start + 1} carries {len(script_ids)} addon scripts")
        attrs = parse_attrs(BLOCK_RE.match(lines[start]).group("attrs"))
        if kind == "resource":
            # .tres: the [resource] body binds the script, [gd_resource] names the type.
            if gd_resource is None:
                report.append(f"{rel}: [resource] at line {start + 1} has no [gd_resource] header")
                continue
            header = parse_attrs(BLOCK_RE.match(lines[gd_resource]).group("attrs"))
            if header.get("type") != native:
                if header.get("type", "Resource") != "Resource":
                    report.append(
                        f"{rel}: [gd_resource] has type={header.get('type')!r}, "
                        f"expected 'Resource' before {native}"
                    )
                lines[gd_resource] = _rewrite_type(lines[gd_resource], native)
            lines[gd_resource] = SCRIPT_CLASS_ATTR_RE.sub("", lines[gd_resource])
        elif attrs.get("type") == native:
            pass  # already ported (idempotent re-run)
        elif kind == "sub_resource":
            if attrs.get("type", "Resource") != "Resource":
                report.append(
                    f"{rel}: [sub_resource] at line {start + 1} has type={attrs.get('type')!r}, "
                    f"expected 'Resource' before {native}"
                )
            lines[start] = _rewrite_type(lines[start], native)
        elif kind == "gd_resource":
            lines[start] = _rewrite_type(lines[start], native)
            lines[start] = SCRIPT_CLASS_ATTR_RE.sub("", lines[start])
        elif kind == "node":
            if "instance" in attrs:
                report.append(
                    f"{rel}: node at line {start + 1} binds an addon script but is a scene "
                    f"instance ({native}); Godot forbids script overrides on instances"
                )
                continue
            if "type" not in attrs:
                report.append(f"{rel}: node at line {start + 1} has no type= to rewrite to {native}")
                continue
            lines[start] = _rewrite_type(lines[start], native)
        else:  # pragma: no cover - defensive; BLOCK_RE lists every kind handled above
            report.append(f"{rel}: unexpected block kind {kind!r} at line {start + 1}")
            continue
        drop.update(i for i in body if SCRIPT_LINE_RE.match(lines[i]))

    # Drop the now-unreferenced ext_resource declarations.
    for i, line in enumerate(lines):
        if i in drop or not (m := EXT_RESOURCE_RE.match(line)):
            continue
        ident = parse_attrs(m.group("attrs")).get("id", "")
        if ident in addon_scripts:
            drop.add(i)

    remaining = [line for i, line in enumerate(lines) if i not in drop]
    text = "\n".join(remaining)

    # Sanity: no addon script may still be referenced anywhere in the file.
    for ident, oracle_rel in addon_scripts.items():
        if f'ExtResource("{ident}")' in text:
            raise PortError(f"{rel}: {oracle_rel} (id {ident}) is still referenced after porting")

    return text, report


def strip_callables(text: str, rel: str) -> tuple[str, list[str]]:
    """Drop the editor-plugin Callable lines; returns ``(text, notes)``."""
    lines = text.split("\n")
    kept = [line for line in lines if not CALLABLE_LINE_RE.match(line)]
    if len(kept) == len(lines):
        return text, []
    dropped = len(lines) - len(kept)
    return "\n".join(kept), [f"{rel}: stripped {dropped} align_*_with_view = Callable() line(s)"]


def rewrite_import(data: bytes) -> bytes:
    """Re-key a ``.import`` remap path to its new source path.

    Godot names imported artifacts ``res://.godot/imported/<file>-<md5(res_path)>.<ext>``
    (``EditorFileSystem::_get_imported_file_path``), so moving a source file
    invalidates the hash. Recomputing it here keeps the ported ``.import`` files
    byte-identical to what the editor would write, which is what makes
    ``--check`` stable across an import run instead of churning every time.
    """
    text = data.decode("utf-8")
    match = IMPORTSRC_RE.search(text)
    if match is None:
        return data
    digest = hashlib.md5(match.group("src").encode("utf-8")).hexdigest()
    return IMPORTHASH_RE.sub(digest, text).encode("utf-8")


def transform(data: bytes, rel: str, script_classes: dict[str, str]) -> tuple[bytes, list[str]]:
    """Port one file: class/binding rewrite (text scenes) then path rewrite."""
    report: list[str] = []
    if rel.endswith((".tscn", ".tres")):
        text, report = port_scene(data.decode("utf-8"), rel, script_classes)
        # Also run on files with no addon binding at all: the Callable strip is
        # independent of them, and a stray line must never survive the port.
        text, callable_notes = strip_callables(text, rel)
        report += callable_notes
        data = text.encode("utf-8")
    data = data.replace(ORACLE_PREFIX, PORT_PREFIX)
    if rel.endswith(".import"):
        data = rewrite_import(data)
    return data, report


def build_tree() -> tuple[dict[str, bytes], list[str]]:
    """Every ported file, keyed by path relative to the addon root."""
    script_classes = oracle_script_classes()
    tree: dict[str, bytes] = {}
    report: list[str] = []
    for src_dir, dst_dir in TREES:
        for path in sorted((ORACLE / src_dir).rglob("*")):
            if not path.is_file():
                continue
            rel = f"{dst_dir}/{path.relative_to(ORACLE / src_dir).as_posix()}"
            tree[rel], notes = transform(path.read_bytes(), rel, script_classes)
            report.extend(notes)
    return tree, report


def existing(tree: dict[str, bytes]) -> set[str]:
    """Files currently on disk under the ported trees."""
    found: set[str] = set()
    for _, dst_dir in TREES:
        root = ADDON / dst_dir
        if not root.is_dir():
            continue
        found.update(p.relative_to(ADDON).as_posix() for p in root.rglob("*") if p.is_file())
    return found


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--check",
        action="store_true",
        help="compare the on-disk tree with what this script would write; write nothing",
    )
    args = parser.parse_args(argv)

    if not ORACLE.is_dir():
        print(f"error: oracle missing at {ORACLE}", file=sys.stderr)
        return 2

    tree, report = build_tree()
    on_disk = existing(tree)

    if args.check:
        drift = [rel for rel, data in sorted(tree.items()) if not (ADDON / rel).is_file() or (ADDON / rel).read_bytes() != data]
        drift += sorted(on_disk - set(tree))
        for rel in drift:
            print(f"drift: {rel}", file=sys.stderr)
        print(f"{len(tree)} files checked, {len(drift)} drifted")
        return 1 if drift else 0

    for rel, data in tree.items():
        dst = ADDON / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        dst.write_bytes(data)
    pruned = sorted(on_disk - set(tree))
    for rel in pruned:
        (ADDON / rel).unlink()

    for note in report:
        print(note)
    print(
        f"wrote {len(tree)} files under {ADDON.name}/"
        f" ({len(tree) - sum(1 for r in tree if r.startswith(TREES[1][0] + '/'))} examples, "
        f"{sum(1 for r in tree if r.startswith(TREES[1][0] + '/'))} fonts)"
        f"{f', pruned {len(pruned)} stale' if pruned else ''}, {len(report)} notes"
    )
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except PortError as exc:
        print(f"error: {exc}", file=sys.stderr)
        sys.exit(2)
