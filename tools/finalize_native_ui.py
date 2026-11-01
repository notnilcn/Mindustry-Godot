#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Final cutover from GDScript UI to native GDExtension classes.

Run this only when every `.gd` scene script has a Rust replacement. It:

1. removes the temporary `rename = Rust<Name>,` attributes from
   `client/rust/mind-gdext/src/ui/**/*.rs` (the Rust type name becomes the
   Godot class name again),
2. runs `tools/rewire_native_ui.py --apply` to swap every `.tscn` script for a
   native class type and to drop the script `ext_resource`s,
3. verifies no non-addon script reference remains,
4. deletes every non-addon `.gd` file under `client/`.

`--check` (default) only reports. `--keep-gdscript` performs steps 1–3 without
deleting the files (for a staged commit).
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
CLIENT = REPO_ROOT / "client"
UI_ROOT = REPO_ROOT / "client/rust/mind-gdext/src/ui"

RENAME_RE = re.compile(r"rename = Rust[A-Za-z0-9_]+, ")

# Areas whose every file must be native before the cutover.
GD_ROOTS = [CLIENT / "ui", CLIENT / "scenes", CLIENT / "tools"]


def gd_files() -> list[Path]:
    files: list[Path] = []
    for root in GD_ROOTS:
        if not root.exists():
            continue
        for path in sorted(root.rglob("*.gd")):
            if "/addons/" in path.as_posix():
                continue
            files.append(path)
    return files


def remaining_script_refs() -> list[str]:
    refs: list[str] = []
    for scene in sorted(CLIENT.rglob("*.tscn")):
        if "/addons/" in scene.as_posix():
            continue
        for line in scene.read_text(encoding="utf-8").splitlines():
            if '[ext_resource type="Script"' in line and 'res://addons/' not in line:
                refs.append(f"{scene.relative_to(REPO_ROOT)}: {line}")
    return refs


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apply", action="store_true", help="perform the cutover")
    parser.add_argument(
        "--keep-gdscript",
        action="store_true",
        help="do not delete the .gd files (staged cutover)",
    )
    args = parser.parse_args()

    pending = gd_files()
    print(f"{len(pending)} non-addon .gd file(s) still present")
    if pending:
        for path in pending[:20]:
            print(f"  {path.relative_to(REPO_ROOT)}")
        if len(pending) > 20:
            print(f"  … and {len(pending) - 20} more")

    renames = 0
    for path in sorted(UI_ROOT.rglob("*.rs")):
        text = path.read_text(encoding="utf-8")
        matches = RENAME_RE.findall(text)
        if not matches:
            continue
        renames += len(matches)
        if args.apply:
            path.write_text(RENAME_RE.sub("", text), encoding="utf-8")
    print(f"{renames} temporary `rename = Rust*` attribute(s)")

    if not args.apply:
        refs = remaining_script_refs()
        print(f"{len(refs)} non-addon script ext_resource(s) still in scenes")
        return 0

    # Rewire after the rename removal so class names resolve again.
    result = subprocess.run(
        [sys.executable, str(REPO_ROOT / "tools/rewire_native_ui.py"), "--apply", "--allow-gdscript"],
        cwd=REPO_ROOT,
        check=False,
    )
    if result.returncode != 0:
        print("rewire failed; not deleting any GDScript", file=sys.stderr)
        return result.returncode

    refs = remaining_script_refs()
    if refs:
        print(
            f"{len(refs)} non-addon script reference(s) remain; not deleting GDScript",
            file=sys.stderr,
        )
        for ref in refs:
            print(f"  {ref}", file=sys.stderr)
        return 1

    if args.keep_gdscript:
        print("scenes rewired; .gd files kept (--keep-gdscript)")
        return 0

    for path in pending:
        path.unlink()
    print(f"deleted {len(pending)} non-addon .gd file(s)")
    print("next: rebuild, run the headless import gate and the UI oracles")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
