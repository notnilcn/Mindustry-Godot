#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Throwaway audit: does every non-addon .gd under client/ have a Rust replacement?

Derives the required native class per .gd file from:
  * rewire_native_ui.py SCRIPT_CLASSES (scene-attached scripts),
  * the `ui/dialogs/<x>.gd` dialog-root convention (-> MindDialog),
  * `class_name` for code-only helpers,
then diffs that against the classes mind-gdext actually registers.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
CLIENT = ROOT / "client"
UI = CLIENT / "rust/mind-gdext/src/ui"

sys.path.insert(0, str(ROOT / "tools"))
import rewire_native_ui as rw  # noqa: E402

DIALOG_ROOT_RE = re.compile(r"^ui/dialogs/[a-z0-9_]+\.gd$")
CLASS_NAME_RE = re.compile(r"^class_name\s+(\w+)", re.MULTILINE)

# ---------------------------------------------------------------- registered
class_re = re.compile(r"#\[class\((?P<attrs>[^\]]*)\)\]")
struct_re = re.compile(r"pub struct (?P<name>\w+)")

registered: dict[str, str] = {}  # godot class name -> type name
for path in sorted(UI.rglob("*.rs")):
    text = path.read_text(encoding="utf-8")
    for m in class_re.finditer(text):
        attrs = m.group("attrs")
        s = struct_re.search(text, m.end())
        if s is None:
            continue
        type_name = s.group("name")
        rename = re.search(r"rename\s*=\s*(\w+)", attrs)
        registered[rename.group(1) if rename else type_name] = type_name

# Also the non-ui classes (root of the crate) for completeness.
for path in sorted((CLIENT / "rust/mind-gdext/src").glob("*.rs")):
    text = path.read_text(encoding="utf-8")
    for m in class_re.finditer(text):
        attrs = m.group("attrs")
        s = struct_re.search(text, m.end())
        if s is None:
            continue
        rename = re.search(r"rename\s*=\s*(\w+)", attrs)
        registered[rename.group(1) if rename else s.group("name")] = s.group("name")

# ---------------------------------------------------------------- code-only helpers
# .gd files never attached to a scene: the replacement is a Rust module, not a
# scene class. Value = evidence path relative to the crate.
HELPERS: dict[str, str] = {
    "ui/margins.gd": "ui/kit/margins.rs",
    "ui/mind_cell.gd": "ui/kit/cell.rs",
    "ui/mind_widgets.gd": "ui/kit/widgets/factory.rs",
    "ui/theme/mind_icons.gd": "ui/kit/icons.rs",
    "ui/theme/styles.gd": "ui/kit/styles.rs",
    "ui/theme/style_lookup.gd": "ui/kit/styles.rs",
    "ui/theme/theme_builder.gd": "ui/kit/theme.rs",
    "ui/layout/tree_layout.gd": "ui/kit/layout.rs",
    "ui/layout/row_tree_layout.gd": "ui/kit/layout.rs",
    "ui/dialogs/map_list_dialog.gd": "ui/dialogs/batch_campaign2.rs",
    "tools/shaders_check.gd": "ui/checks.rs",
    "tools/ui_layout_check.gd": "ui/checks.rs",
    "tools/ui_widgets_check.gd": "ui/checks.rs",
}

# Widgets with a class_name that are code-built (no scene attaches them).
WIDGET_TARGETS = {
    "ui/widgets/items_display.gd": "MindItemsDisplay",
    "ui/widgets/mind_border_image.gd": "MindBorderImage",
    "ui/widgets/mind_check.gd": "MindCheck",
    "ui/widgets/mind_collapser.gd": "MindCollapser",
    "ui/widgets/mind_grid_image.gd": "MindGridImage",
    "ui/widgets/mind_item_command_row.gd": "MindItemCommandRow",
    "ui/widgets/mind_item_selection.gd": "MindItemSelection",
    "ui/widgets/mind_mobile_button.gd": "MindMobileButton",
    "ui/widgets/mind_multi_req_image.gd": "MindMultiReqImage",
    "ui/widgets/mind_req_image.gd": "MindReqImage",
    "ui/widgets/mind_tooltip.gd": "MindTooltip",
    "ui/widgets/mind_warning_bar.gd": "MindWarningBar",
    "ui/widgets/research_items_display.gd": "MindResearchItemsDisplay",
    "ui/widgets/research_view.gd": "MindResearchView",
    "ui/widgets/schematic_image.gd": "MindSchematicImage",
    "ui/text/mind_label.gd": "MindLabel",
    "ui/theme/icon_effect.gd": "MindIconEffect",
}

gd_files = sorted(
    p.relative_to(CLIENT).as_posix()
    for p in CLIENT.rglob("*.gd")
    if "/addons/" not in p.as_posix()
)

missing: list[str] = []
ok: list[str] = []
note: list[str] = []


def lookup(target: str) -> str | None:
    """Registered Godot name for `target`, accounting for the `Rust*` prefix."""
    if target in registered:
        return target
    if f"Rust{target}" in registered:
        return f"Rust{target}"
    return None

for rel in gd_files:
    if rel in HELPERS:
        ev = UI.parent / HELPERS[rel]
        if ev.exists():
            ok.append(f"{rel:52s} -> module {HELPERS[rel]}")
        else:
            missing.append(f"{rel:52s} -> MISSING module {HELPERS[rel]}")
        continue
    if rel in WIDGET_TARGETS:
        target = WIDGET_TARGETS[rel]
        hit = lookup(target)
        if hit:
            ok.append(f"{rel:52s} -> {hit} (rename {registered[hit]})")
        else:
            missing.append(f"{rel:52s} -> MISSING class {target}")
        continue
    if rel in rw.SCRIPT_CLASSES:
        target = rw.SCRIPT_CLASSES[rel][0]
        hit = lookup(target)
        if hit:
            ok.append(f"{rel:52s} -> {hit} (rename {registered[hit]})")
        else:
            missing.append(f"{rel:52s} -> MISSING class {target}")
        continue
    if DIALOG_ROOT_RE.match(rel):
        if lookup("MindDialog"):
            ok.append(f"{rel:52s} -> MindDialog (dialogs/<x>.gd convention)")
        else:
            missing.append(f"{rel:52s} -> MISSING MindDialog")
        continue
    text = (CLIENT / rel).read_text(encoding="utf-8")
    cn = CLASS_NAME_RE.search(text)
    if cn and lookup(cn.group(1)):
        ok.append(f"{rel:52s} -> {cn.group(1)}")
        continue
    note.append(f"{rel:52s} -> UNCLASSIFIED (class_name={cn.group(1) if cn else None})")

print(f"registered extension classes: {len(registered)}")
print(f"non-addon .gd files:          {len(gd_files)}")
print(f"  mapped:                     {len(ok)}")
print(f"  UNMAPPED:                   {len(missing)}")
print(f"  unclassified:               {len(note)}")
print()
for line in missing:
    print("MISSING  " + line)
for line in note:
    print("NOTE     " + line)
if not missing and not note:
    print("every .gd has a Rust replacement")
