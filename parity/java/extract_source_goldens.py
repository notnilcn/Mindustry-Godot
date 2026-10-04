#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
#
# Source-derived oracle golden extractor (plan 23 M2 fallback, NUD-10/A).
#
# The JVM one-off dumps (`parity/java/Dump*.java`) stay the primary oracle, but
# they need a Gradle-built upstream checkout. When that build is unavailable
# (as in this lane), this script derives the same *field-order / catalogue*
# goldens directly from the upstream Java sources at `parity/upstream.lock`.
# The extractor is deterministic and records the source file sha256 so the
# golden is pinned to the exact upstream commit.
#
# Usage: extract_source_goldens.py [mindustry_src] [out_root]
#   mindustry_src  defaults to ../Mindustry
#   out_root       defaults to the repo's parity/golden directory

import hashlib
import json
import os
import re
import sys

MD = sys.argv[1] if len(sys.argv) > 1 else "../Mindustry"
REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
OUT = sys.argv[2] if len(sys.argv) > 2 else os.path.join(REPO, "parity", "golden")
LOCK = os.path.join(REPO, "parity", "upstream.lock")
PLAN13 = os.path.join(REPO, "13_LOGIC_MLOG_IMPLEMENTATION_PLAN.md")


def sha256(path):
    with open(path, "rb") as handle:
        return hashlib.sha256(handle.read()).hexdigest()


def write(domain, name, payload):
    directory = os.path.join(OUT, domain)
    os.makedirs(directory, exist_ok=True)
    path = os.path.join(directory, name)
    with open(path, "w") as handle:
        json.dump(payload, handle, indent=2)
        handle.write("\n")
    return path


def upstream_relative(path):
    return os.path.relpath(path, MD).replace(os.sep, "/")


def commit():
    try:
        with open(LOCK) as handle:
            return json.load(handle).get("mindustry_commit", "")
    except OSError:
        return ""


# --- logic: LogicIO statement field order -----------------------------------
# Upstream `LStatements.java` declares one statement class per registered name;
# the plan-13 §6.2 table records the serialized field order (Java declaration
# order). Parse that table so the golden is reproducible without a JVM.
def extract_logic():
    fields = []
    with open(PLAN13) as handle:
        lines = handle.readlines()
    for line in lines:
        match = re.match(
            r"^\|\s*(\d+)\s*\|\s*`([^`]+)`\s*\|\s*([^|]+)\|\s*(.*?)\s*\|\s*$", line
        )
        if not match:
            continue
        index, reg, variant, field_cell = match.groups()
        ordered = re.findall(r"`([^`]+)`", field_cell)
        names = [token.split()[-1] for token in ordered]
        fields.append(
            {
                "index": int(index),
                "reg": reg,
                "variant": variant.strip(),
                "fields": ordered,
                "names": names,
            }
        )
    return {
        "format": 1,
        "domain": "logic",
        "kind": "field_order",
        "source": "13_LOGIC_MLOG_IMPLEMENTATION_PLAN.md#6.2",
        "upstream_commit": commit(),
        "count": len(fields),
        "statements": fields,
    }


# --- UI: UiKey ordinals ------------------------------------------------------
def extract_ui():
    path = os.path.join(MD, "core/src/mindustry/ui/builder/UiKey.java")
    with open(path) as handle:
        text = handle.read()
    body = text.split("{", 1)[1].split(";", 1)[0]
    body = re.sub(r"//[^\n]*", "", body)
    keys = [token.strip() for token in body.split(",") if token.strip()]
    return {
        "format": 1,
        "domain": "ui",
        "kind": "ui_keys",
        "source": "core/src/mindustry/ui/builder/UiKey.java",
        "source_sha256": sha256(path),
        "upstream_commit": commit(),
        "count": len(keys),
        "keys": keys,
    }


# --- IO: Rules public field order -------------------------------------------
def extract_io():
    path = os.path.join(MD, "core/src/mindustry/game/Rules.java")
    with open(path) as handle:
        lines = handle.readlines()
    field_re = re.compile(
        r"^\s{4}public\s+(?:static\s+)?(?:final\s+)?"
        r"([A-Za-z0-9_<>,\.\[\]\s]+?)\s+([a-zA-Z_][A-Za-z0-9_]*)\s*(?:=|;)"
    )
    fields = []
    for line in lines:
        match = field_re.match(line)
        if match:
            fields.append({"type": match.group(1).strip(), "name": match.group(2)})
    return {
        "format": 1,
        "domain": "io",
        "kind": "rules_field_order",
        "source": "core/src/mindustry/game/Rules.java",
        "source_sha256": sha256(path),
        "upstream_commit": commit(),
        "count": len(fields),
        "fields": fields,
    }


# --- FX: Effect catalogue order ---------------------------------------------
def extract_fx():
    path = os.path.join(MD, "core/src/mindustry/content/Fx.java")
    with open(path) as handle:
        lines = handle.readlines()
    effects = []
    in_block = False
    for line in lines:
        stripped = line.strip()
        if not in_block and "public static final Effect" in line:
            in_block = True
            continue
        if in_block:
            if stripped.startswith(";"):
                break
            match = re.match(r"^\s{4}([a-zA-Z_][A-Za-z0-9_]*)\s*=\s*new\s", line)
            if match:
                effects.append(match.group(1))
    return {
        "format": 1,
        "domain": "fx",
        "kind": "fx_order",
        "source": "core/src/mindustry/content/Fx.java",
        "source_sha256": sha256(path),
        "upstream_commit": commit(),
        "count": len(effects),
        "effects": effects,
    }


def main():
    if not os.path.isdir(MD):
        print(f"extract_source_goldens: upstream `{MD}` not found", file=sys.stderr)
        return 2
    produced = [
        write("logic", "field_order.json", extract_logic()),
        write("ui", "ui_keys.json", extract_ui()),
        write("io", "rules_fields.json", extract_io()),
        write("fx", "fx_order.json", extract_fx()),
    ]
    for path in produced:
        print(f"wrote {path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
