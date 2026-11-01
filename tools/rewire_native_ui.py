#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Rewire Godot scenes from GDScript resources to native GDExtension classes.

The UI port replaces every `client/**/*.gd` script with a native Rust class.
This script performs the mechanical `.tscn` half:

* root nodes of dialog/fragment/editor scenes become the native class type and
  lose their `script = ExtResource(...)` property;
* nodes that attached a script to an engine type (`type="Label"` +
  `mind_rich_label.gd`) become the native type (`type="MindRichLabel"`);
* every `ScrollContainer` becomes `MindScroll` (the wheel easing moved into the
  class);
* `ext_resource` Script entries that no longer have a user are removed.

Usage (from the repo root):

    python3 tools/rewire_native_ui.py --check     # report only (default)
    python3 tools/rewire_native_ui.py --apply     # rewrite the scenes

`--allow-gdscript` keeps a script attached (only for an unfinished area); by
default any remaining non-addon `.gd` reference is an error.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
CLIENT = REPO_ROOT / "client"

# script path -> (native class, base type). The base type is the engine class
# the Rust class extends, so an attached node's `type=` can be swapped safely.
SCRIPT_CLASSES: dict[str, tuple[str, str]] = {
    # root modules
    "ui/mind_dialog.gd": ("MindDialog", "Control"),
    "ui/ui_root.gd": ("MindUiRoot", "Control"),
    "ui/state_inspector.gd": ("MindStateInspector", "Control"),
    "ui/planet_view.gd": ("MindPlanetView", "Control"),
    "ui/menu_background.gd": ("MindMenuBackground", "TextureRect"),
    "ui/menu_flyers.gd": ("MindMenuFlyers", "Control"),
    "scenes/landing_core.gd": ("MindLandingCore", "Node2D"),
    # text widgets
    "ui/text/mind_label.gd": ("MindLabel", "RichTextLabel"),
    "ui/text/mind_rich_label.gd": ("MindRichLabel", "RichTextLabel"),
    # scene-attached widgets
    "ui/widgets/mind_minimap.gd": ("MindMinimapWidget", "Control"),
    "ui/widgets/player_status.gd": ("MindPlayerStatus", "Control"),
    "ui/widgets/mind_bar.gd": ("MindBar", "Control"),
    "ui/widgets/core_items_display.gd": ("CoreItemsDisplay", "GridContainer"),
    # fragments
    "ui/fragments/block_config_fragment.gd": ("MindBlockConfigFragment", "Control"),
    "ui/fragments/block_inventory_fragment.gd": ("MindBlockInventoryFragment", "Control"),
    "ui/fragments/chat_fragment.gd": ("MindChatFragment", "Control"),
    "ui/fragments/console_fragment.gd": ("MindConsoleFragment", "Control"),
    "ui/fragments/fade_in_fragment.gd": ("MindFadeInFragment", "ColorRect"),
    "ui/fragments/hints_fragment.gd": ("MindHintsFragment", "Control"),
    "ui/fragments/hud_fragment.gd": ("MindHudFragment", "Control"),
    "ui/fragments/loading_fragment.gd": ("MindLoadingFragment", "Control"),
    "ui/fragments/menu_fragment.gd": ("MindMenuFragment", "Control"),
    "ui/fragments/minimap_fragment.gd": ("MindMinimapFragment", "Control"),
    "ui/fragments/performance_fragment.gd": ("MindPerformanceFragment", "Control"),
    "ui/fragments/placement_fragment.gd": ("MindPlacementFragment", "Control"),
    "ui/fragments/plan_config_fragment.gd": ("MindPlanConfigFragment", "Control"),
    "ui/fragments/player_list_fragment.gd": ("MindPlayerListFragment", "Control"),
    # editor
    "scenes/editor/banned_content_dialog.gd": ("MindBannedContentDialog", "Control"),
    "scenes/editor/map_editor_dialog.gd": ("MindMapEditorDialog", "Control"),
    "scenes/editor/map_generate_dialog.gd": ("MindMapGenerateDialog", "MindDialog"),
    "scenes/editor/map_info_dialog.gd": ("MindMapInfoDialog", "MindDialog"),
    "scenes/editor/map_load_dialog.gd": ("MindMapLoadDialog", "MindDialog"),
    "scenes/editor/map_locales_dialog.gd": ("MindMapLocalesDialog", "MindDialog"),
    "scenes/editor/map_objectives_dialog.gd": ("MindMapObjectivesDialog", "MindDialog"),
    "scenes/editor/map_objectives_canvas.gd": ("MindMapObjectivesCanvas", "Control"),
    "scenes/editor/map_processors_dialog.gd": ("MindMapProcessorsDialog", "MindDialog"),
    "scenes/editor/map_resize_dialog.gd": ("MindMapResizeDialog", "MindDialog"),
    "scenes/editor/map_view.gd": ("MindMapView", "Control"),
    "scenes/editor/sector_generate_dialog.gd": ("MindSectorGenerateDialog", "MindDialog"),
    "scenes/editor/wave_graph.gd": ("MindWaveGraph", "Control"),
    "scenes/editor/wave_info_dialog.gd": ("MindWaveInfoDialog", "MindDialog"),
    "scenes/editor/data/map_assets_dialog.gd": ("MindMapAssetsDialog", "MindDialog"),
}

# Dialog scenes whose root is an instanced `mind_dialog_base.tscn`: the root
# type comes from the base scene, so only the `script` property is dropped.
DIALOG_ROOT_RE = re.compile(r"^ui/dialogs/[a-z0-9_]+\.gd$")

# Scripts selected for the current migration batch (`--only`); empty = all.
only: set[str] = set()

EXT_RE = re.compile(
    r'^\[ext_resource type="Script"[^\]]*path="res://(?P<path>[^"]+)"[^\]]*id="(?P<id>[^"]+)"\]\s*$'
)
NODE_RE = re.compile(r'^\[node name="(?P<name>[^"]+)"(?P<attrs>[^\]]*)\]\s*$')
SCRIPT_PROP_RE = re.compile(r'^script = ExtResource\("(?P<id>[^"]+)"\)\s*$')


def rel(path: Path) -> str:
    return path.relative_to(CLIENT).as_posix()


def rewrite_scene(path: Path, apply: bool) -> tuple[bool, list[str]]:
    """Rewrites one `.tscn`; returns (changed, messages)."""
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()
    messages: list[str] = []

    script_paths: dict[str, str] = {}
    for line in lines:
        match = EXT_RE.match(line)
        if match:
            script_paths[match.group("id")] = match.group("path")

    out: list[str] = []
    pending_node_index: int | None = None
    pending_script_id: str | None = None
    pending_script_line: str | None = None
    used_ids: set[str] = set()
    changed = False

    def finish_node() -> None:
        nonlocal pending_node_index, pending_script_id, pending_script_line, changed
        if pending_node_index is None:
            return
        if pending_script_id is None:
            return
        script = script_paths.get(pending_script_id, "")
        relative = script.removeprefix("res://")
        if only and relative not in only:
            # Not part of this migration batch: keep the script attached, in
            # its original position right after the node header.
            if pending_script_line is not None:
                out.insert(pending_node_index + 1, pending_script_line)
            pending_script_id = None
            pending_script_line = None
            return
        if relative in SCRIPT_CLASSES:
            native, _base = SCRIPT_CLASSES[relative]
        elif DIALOG_ROOT_RE.match(relative):
            native = "MindDialog"
        else:
            messages.append(f"{rel(path)}: unmapped script {script}")
            if pending_script_line is not None:
                out.insert(pending_node_index + 1, pending_script_line)
            pending_script_id = None
            pending_script_line = None
            return
        header = out[pending_node_index]
        match = NODE_RE.match(header)
        if match is None:
            messages.append(f"{rel(path)}: cannot parse node header {header!r}")
            return
        attrs = match.group("attrs")
        # Drop the property line and, for a non-instanced root/attached node,
        # swap the engine type for the native class.
        if "instance=ExtResource(" in attrs:
            # Inherited scene root: the base scene already carries `MindDialog`.
            # The `script` property was never appended, so there is nothing to
            # pop (popping would drop the last real property line).
            changed = True
        elif 'type="' in attrs:
            new_attrs = re.sub(r'type="[^"]*"', f'type="{native}"', attrs)
            out[pending_node_index] = f'[node name="{match.group("name")}"{new_attrs}]'
            changed = True
        else:
            messages.append(f"{rel(path)}: node without type or instance: {header!r}")
            return
        for entry, entry_id in list(script_paths.items()):
            if entry_id == pending_script_id:
                used_ids.add(entry_id)
        pending_script_id = None

    for line in lines:
        node_match = NODE_RE.match(line)
        if node_match is not None:
            finish_node()
            pending_node_index = len(out)
            pending_script_id = None
            # `ScrollContainer` -> `MindScroll` for every plain pane.
            if 'type="ScrollContainer"' in line:
                line = line.replace('type="ScrollContainer"', 'type="MindScroll"')
                changed = True
            out.append(line)
            continue
        prop = SCRIPT_PROP_RE.match(line)
        if prop is not None and pending_node_index is not None:
            pending_script_id = prop.group("id")
            pending_script_line = line
            continue
        out.append(line)
    finish_node()

    # Remove ext_resource Script entries that no longer have a user.
    unused = {
        entry_id
        for entry_id, script in script_paths.items()
        if f'ExtResource("{entry_id}")' not in "\n".join(out)
    }
    if unused:
        out = [
            line
            for line in out
            if not (
                (match := EXT_RE.match(line)) is not None and match.group("id") in unused
            )
        ]
        changed = True

    if changed and apply:
        path.write_text("\n".join(out) + "\n", encoding="utf-8")
    return changed, messages


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apply", action="store_true", help="rewrite the scenes")
    parser.add_argument(
        "--allow-gdscript",
        action="store_true",
        help="report but do not fail on remaining non-addon .gd references",
    )
    parser.add_argument(
        "--only",
        action="append",
        default=[],
        metavar="SCRIPT",
        help="only rewire this script path (repeatable, path relative to client/)",
    )
    args = parser.parse_args()
    global only
    only = set(args.only)

    scenes = sorted(CLIENT.rglob("*.tscn"))
    changed = 0
    problems: list[str] = []
    for scene in scenes:
        if "/addons/" in scene.as_posix():
            continue
        did_change, messages = rewrite_scene(scene, args.apply)
        changed += int(did_change)
        problems.extend(messages)

    # Any remaining non-addon script reference is a porting gap.
    remaining: list[str] = []
    for scene in scenes:
        if "/addons/" in scene.as_posix():
            continue
        for line in scene.read_text(encoding="utf-8").splitlines():
            match = EXT_RE.match(line)
            if match and not match.group("path").startswith("res://addons/"):
                remaining.append(f"{rel(scene)}: {match.group('path')}")

    for problem in problems:
        print(f"error: {problem}", file=sys.stderr)
    for entry in remaining:
        print(f"gdscript: {entry}", file=sys.stderr)

    print(
        f"{'rewrote' if args.apply else 'would rewrite'} {changed} scene(s); "
        f"{len(remaining)} non-addon script reference(s) remain"
    )
    if problems:
        return 1
    if remaining and not args.allow_gdscript:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
