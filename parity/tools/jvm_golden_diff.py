#!/usr/bin/env python3
"""Compare the committed source-derived golden with the JVM golden (NUD-10).

Usage: jvm_golden_diff.py
  parity/golden_content.json            (Rust source-derived gate)
  parity/java/jvm_golden_content.json   (`parity/java/run.sh` output)
  -> parity/reports/jvm_golden_diff.md
"""
import json
import sys
from pathlib import Path

RUST = Path("parity/golden_content.json")
JVM = Path("parity/java/jvm_golden_content.json")
OUT = Path("parity/reports/jvm_golden_diff.md")

# Upstream constructs that the M7 Rust port deliberately leaves to other plans
# (or that the JVM harness cannot observe). Deltas inside these counts are
# documented, not bugs against the plan-02 gate.
OWNERSHIP = {
    "bullet": "plan 10 registers turret ammo (`Blocks.java` `ammoTypes`) bullets",
    "unit": "plan 10 registers turret-created missile units (scathe missiles, build tower)",
    "sector": "plan 19 `SectorSubmissions.registerSectors()` auto-presets (46 authored + 49 submitted in JVM)",
}

# `Core.bundle` is not initialized by `ApplicationTests.launchApplication`, so
# every JVM `localizedName` falls back to the internal name.
LOCALIZED_NOTE = (
    "JVM harness `Core.bundle` is uninitialized, so the JVM golden's "
    "`localized` is the internal name. The Rust gate boots `parity/bundle_keys.json` "
    "and records real localized names; plan 03 covers bundle loading."
)


def entries(golden, type_name):
    for section in golden["types"]:
        if section["type"] == type_name:
            return section["entries"]
    return []


def main():
    rust = json.load(open(RUST, encoding="utf-8"))
    jvm = json.load(open(JVM, encoding="utf-8"))
    lines = [
        "# JVM golden diff (NUD-10)",
        "",
        f"- Rust gate: `{rust['generator']}` ({rust['mindustry_version']})",
        f"- JVM dump: `{jvm['generator']}`",
        "- Produced by `parity/tools/jvm_golden_diff.py`.",
        "",
        "## Per-type counts",
        "",
        "| type | rust | jvm | note |",
        "|---|---|---|---|",
    ]
    for type_name in sorted(set(list(rust["counts"]) + list(jvm["counts"]))):
        r = rust["counts"].get(type_name, 0)
        j = jvm["counts"].get(type_name, 0)
        note = "" if r == j else OWNERSHIP.get(type_name, "reconcile")
        lines.append(f"| {type_name} | {r} | {j} | {note} |")

    lines += ["", "## Block field diffs", ""]
    r_blocks = {e["name"]: e for e in entries(rust, "block")}
    j_blocks = {e["name"]: e for e in entries(jvm, "block")}
    rust_only = sorted(set(r_blocks) - set(j_blocks))
    jvm_only = sorted(set(j_blocks) - set(r_blocks))
    lines.append(f"- Names: rust-only {len(rust_only)}, jvm-only {len(jvm_only)}")
    field_diffs = []
    for name, re_ in r_blocks.items():
        je = j_blocks.get(name)
        if je is not None and re_["fields"] != je["fields"]:
            differing = {
                key: (re_["fields"].get(key), je["fields"].get(key))
                for key in set(re_["fields"]) | set(je["fields"])
                if re_["fields"].get(key) != je["fields"].get(key)
            }
            field_diffs.append((name, differing))
    lines.append(
        f"- Blocks with field diffs: {len(field_diffs)} "
        "(remaining generator/default reconciliation; the plan-02 CI gate uses the "
        "source-derived golden until these are closed)"
    )
    lines.append("")
    if field_diffs:
        lines.append("| block | field: rust -> jvm |")
        lines.append("|---|---|")
        for name, differing in field_diffs:
            rendered = ", ".join(f"`{k}`: {r} -> {j}" for k, (r, j) in sorted(differing.items()))
            lines.append(f"| {name} | {rendered} |")

    lines += ["", "## Known harness/loop artifacts", ""]
    lines.append(f"- Localized names: {LOCALIZED_NOTE}")
    lines.append(
        "- `air` reports raw `-1` fields because the upstream build's `@OverrideCallSuper` "
        "codegen did not rewrite `AirBlock.init()` (bytecode check: no `super.init()` call); "
        "the Rust port derives `health/scaledHealth/buildTime/liquidCapacity` as upstream does "
        "with working codegen."
    )
    lines.append("")
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    print(f"wrote {OUT} ({len(field_diffs)} field-diff blocks)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
