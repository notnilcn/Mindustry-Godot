#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Gate the ported Phantom Camera demo tree: no GDScript addon bindings, no oracle paths.

Run from anywhere:

    python client/addons/phantom_camera_rust/dev/verify_examples.py

Asserts, over ``client/addons/phantom_camera_rust/`` (``examples/``, ``fonts/``
and the manager autoload scene):

1. no ``res://addons/phantom_camera/`` path survives (the oracle's location);
2. no ``script = ExtResource(...)`` binds an addon ``.gd`` -- the only surviving
   script bindings are the demo driver scripts under ``examples/scripts/``;
3. every ``type=`` in a ``[node]``/``[sub_resource]`` header (and every
   ``.tres`` ``[gd_resource]``) names an engine class or one of the native
   classes in ``client/rust/phantom-camera-rust/src/lib.rs::REGISTERED_CLASSES``;
4. the editor-only ``align_*_with_view = Callable()`` lines are gone;
5. every ``ExtResource("id")``/``SubResource("id")`` reference resolves (catches
   an over-eager ``[ext_resource]`` deletion);
6. the demo driver scripts are **byte-identical** to the oracle copies, so a
   parity run differs only in the addon implementation;
7. ``phantom_camera_manager.tscn`` exists and roots on ``PhantomCameraManager``.

Exit 0 = pass, 1 = violations, 2 = setup error. Python 3 stdlib only.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[4]
ADDON = REPO_ROOT / "client" / "addons" / "phantom_camera_rust"
ORACLE = REPO_ROOT / "parity" / "pcam_oracle" / "phantom_camera"
LIB_RS = REPO_ROOT / "client" / "rust" / "phantom-camera-rust" / "src" / "lib.rs"

ORACLE_PATH = "res://addons/phantom_camera/"
PORT_PATH = "res://addons/phantom_camera_rust/"
DEMO_SCRIPTS = PORT_PATH + "examples/scripts/"

REGISTERED_RE = re.compile(
    r"pub const REGISTERED_CLASSES:\s*&\[&str\]\s*=\s*&\[(?P<body>.*?)\];", re.DOTALL
)
HEADER_RE = re.compile(
    r"^\[(?P<kind>ext_resource|node|sub_resource|gd_resource)\b(?P<attrs>[^\]]*)\]"
)
ID_RE = re.compile(r'(?<=\s)id="(?P<id>[^"]+)"')
TYPE_RE = re.compile(r'(?<=\s)type="(?P<type>[^"]+)"')
PATH_RE = re.compile(r'(?<=\s)path="(?P<path>[^"]+)"')
REF_RE = re.compile(r'\b(?P<kind>Ext|Sub)Resource\("(?P<id>[^"]+)"\)')
SCRIPT_REF_RE = re.compile(r'^script = ExtResource\("(?P<id>[^"]+)"\)', re.MULTILINE)
CALLABLE_RE = re.compile(
    r"^align_(?:transform|position|rotation)_with_view = Callable\(\)", re.MULTILINE
)

#: Engine classes the ported demo scenes subclass/instance. Deliberately an
#: explicit allowlist: a bare ``type="Resource"`` is *not* accepted because it
#: means a script binding was not ported to its native resource class, and a new
#: engine class in a scene should be a reviewed addition here, not a silent pass.
ENGINE_CLASSES = frozenset(
    {
        "Area2D", "Area3D", "BoxMesh", "BoxShape3D", "CSGBox3D", "CSGCylinder3D",
        "CSGMesh3D", "CSGSphere3D", "CSGTorus3D", "Camera2D", "Camera3D",
        "CameraAttributesPractical", "CanvasLayer", "CapsuleMesh", "CapsuleShape3D",
        "CharacterBody2D", "CharacterBody3D", "CollisionShape2D", "CollisionShape3D",
        "ColorRect", "Control", "Curve2D", "Curve3D", "CylinderMesh", "DirectionalLight3D",
        "Label", "Label3D", "MeshInstance3D", "Node", "Node2D", "Node3D", "Panel", "Path2D",
        "Path3D", "PrismMesh", "RectangleShape2D", "RichTextLabel", "Sprite2D",
        "StandardMaterial3D", "StaticBody3D", "StyleBoxFlat", "TextureRect", "TileMapLayer",
        "TileSet", "TileSetAtlasSource", "VBoxContainer",
    }
)

TEXT_SUFFIXES = (".tscn", ".tres", ".gd", ".import", ".txt", ".svg", ".uid")


def read_text(path: Path) -> str:
    return path.read_bytes().decode("utf-8", errors="replace")


def registered_classes() -> list[str]:
    match = REGISTERED_RE.search(read_text(LIB_RS))
    if match is None:
        raise SystemExit(f"error: REGISTERED_CLASSES not found in {LIB_RS}")
    names = re.findall(r'"([^"]+)"', match.group("body"))
    if not names:
        raise SystemExit(f"error: REGISTERED_CLASSES in {LIB_RS} is empty")
    return names


def ported_files() -> list[Path]:
    files = [
        path
        for root in (ADDON / "examples", ADDON / "fonts")
        if root.is_dir()
        for path in sorted(root.rglob("*"))
        if path.is_file()
    ]
    manager = ADDON / "phantom_camera_manager.tscn"
    if manager.is_file():
        files.append(manager)
    return files


def check_scene(rel: str, text: str, known: frozenset[str]) -> list[str]:
    """Checks 2-5 for one ``.tscn``/``.tres``."""
    errors: list[str] = []
    declared: dict[str, str] = {}
    demo_scripts: set[str] = set()

    for line in text.split("\n"):
        match = HEADER_RE.match(line)
        if match is None:
            continue
        kind = match.group("kind")
        ident = ID_RE.search(match.group("attrs"))
        if ident is not None:
            declared[ident.group("id")] = kind
        if kind != "ext_resource":
            type_match = TYPE_RE.search(match.group("attrs"))
            if type_match is None:
                # Legitimately type-less node headers, both written by Godot for a
                # node that is not a fresh class instance: ``instance=ExtResource(...)``
                # (a scene instance) and ``index="N"`` (an editable-instance child
                # override, whose class comes from the instanced scene).
                if kind == "node" and ('instance=ExtResource(' in line or ' index="' in line):
                    continue
                errors.append(f"{rel}: [{kind}] header has no type=")
            elif type_match.group("type") not in known:
                errors.append(
                    f"{rel}: [{kind}] type={type_match.group('type')!r} is neither an engine "
                    f"class nor a class in REGISTERED_CLASSES"
                )
            continue
        # 2. the only legitimate script binding is a demo driver script.
        type_match = TYPE_RE.search(line)
        if type_match is None or type_match.group("type") != "Script":
            continue
        path_match = PATH_RE.search(line)
        script_path = path_match.group("path") if path_match else ""
        if ident is not None:
            demo_scripts.add(ident.group("id"))
        if not script_path.startswith(DEMO_SCRIPTS):
            errors.append(
                f"{rel}: [ext_resource] binds non-demo script {script_path!r} "
                f"(must be under {DEMO_SCRIPTS})"
            )

    for match in REF_RE.finditer(text):
        ident, kind = match.group("id"), match.group("kind")
        if ident not in declared:
            errors.append(f'{rel}: {kind}Resource("{ident}") is not declared')
    for match in SCRIPT_REF_RE.finditer(text):
        if match.group("id") not in demo_scripts:
            errors.append(
                f'{rel}: script = ExtResource("{match.group("id")}") does not resolve to a '
                f"demo driver script"
            )

    # 4. the editor-only Callables are gone.
    if CALLABLE_RE.search(text):
        errors.append(f"{rel}: still carries an align_*_with_view = Callable() line")
    return errors


def verify(verbose: bool) -> list[str]:
    registered = registered_classes()
    known = frozenset(ENGINE_CLASSES | set(registered))
    files = ported_files()
    if not files:
        raise SystemExit(f"error: no ported files under {ADDON}")

    errors: list[str] = []
    counts = {".tscn": 0, ".tres": 0, ".gd": 0}

    for path in files:
        rel = path.relative_to(ADDON).as_posix()
        raw = path.read_bytes()

        # 1. no oracle path anywhere, binary assets included.
        if ORACLE_PATH.encode() in raw:
            errors.append(f"{rel}: contains the oracle path {ORACLE_PATH}")

        if path.suffix not in TEXT_SUFFIXES:
            continue
        text = raw.decode("utf-8", errors="replace")
        if path.suffix in (".tscn", ".tres"):
            counts[path.suffix] += 1
            errors.extend(check_scene(rel, text, known))
        elif path.suffix == ".gd":
            counts[".gd"] += 1

    # 6. the demo driver scripts are byte-identical to the oracle.
    oracle_scripts = ORACLE / "examples" / "scripts"
    expected = sorted(oracle_scripts.rglob("*.gd")) if oracle_scripts.is_dir() else []
    for src in expected:
        rel = src.relative_to(ORACLE / "examples").as_posix()
        dst = ADDON / "examples" / rel
        if not dst.is_file():
            errors.append(f"examples/{rel}: missing from the ported tree")
        elif dst.read_bytes() != src.read_bytes():
            errors.append(f"examples/{rel}: differs from the oracle copy (must be byte-identical)")
    if counts[".gd"] != len(expected):
        errors.append(f"examples: {counts['.gd']} .gd files ported, {len(expected)} in the oracle")

    # 7. the manager autoload scene.
    manager = ADDON / "phantom_camera_manager.tscn"
    if not manager.is_file():
        errors.append("phantom_camera_manager.tscn: missing")
    else:
        root = next(
            (line for line in manager.read_text("utf-8").split("\n") if line.startswith("[node")),
            "",
        )
        if 'type="PhantomCameraManager"' not in root:
            errors.append(
                f"phantom_camera_manager.tscn: root is not PhantomCameraManager ({root!r})"
            )

    if verbose:
        print(f"  native classes: {', '.join(registered)}")
        print(f"  engine classes: {len(ENGINE_CLASSES)}")
    print(
        f"{len(files)} files checked ({counts['.tscn']} scenes, {counts['.tres']} resources, "
        f"{counts['.gd']} scripts), {len(registered)} native classes"
    )
    return errors


def main(argv: list[str]) -> int:
    if not ADDON.is_dir():
        print(f"error: addon tree missing at {ADDON}", file=sys.stderr)
        return 2
    errors = verify(verbose=any(a in ("--verbose", "-v") for a in argv))
    if errors:
        for error in errors:
            print(f"FAIL {error}", file=sys.stderr)
        print(f"{len(errors)} violation(s)", file=sys.stderr)
        return 1
    print("ok")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
