#!/usr/bin/env python3
"""Generate source-derived audit inputs for plan 02 M7.

Outputs (committed):
  parity/bundle_keys.json    key/value map parsed from
                             core/assets/bundles/bundle.properties
  parity/asset_manifest.json region list (+ item/liquid expectations) scanned
                             from core/assets-raw/sprites/**/*.png

The real `asset_manifest.json` is plan 03's artifact; this source-derived
fallback drives the M7 region audit until plan 03 replaces it.

Usage: gen_audit_inputs.py [upstream_core_dir]
  upstream_core_dir defaults to
  /home/c/g/code_examples/Mindustry/core
"""
import json
import re
import sys
from pathlib import Path

CORE = Path(sys.argv[1] if len(sys.argv) > 1
            else "/home/c/g/code_examples/Mindustry/core")
BUNDLE = CORE / "assets/bundles/bundle.properties"
SPRITES = CORE / "assets-raw/sprites"
OUT_DIR = Path("parity")

# Vanilla content with no upstream `<type>.<name>.name` key (verified absent in
# bundle.properties): internal placeholder blocks (air/build*), ore overlays and
# ore walls, hidden planets, `status.none`, internal/missile units, and the
# hidden `weather.suspendParticles`. Missing keys outside this list fail the
# bundle audit.
BUNDLE_EXEMPT = sorted([
    "block.air.name",
    *[f"block.build{i}.name" for i in range(1, 17)],
    "block.ore-beryllium.name",
    "block.ore-coal.name",
    "block.ore-copper.name",
    "block.ore-crystal-thorium.name",
    "block.ore-lead.name",
    "block.ore-scrap.name",
    "block.ore-thorium.name",
    "block.ore-titanium.name",
    "block.ore-tungsten.name",
    "block.ore-wall-beryllium.name",
    "block.ore-wall-graphite.name",
    "block.ore-wall-thorium.name",
    "block.ore-wall-tungsten.name",
    "block.legacy-mech-pad.name",
    "block.legacy-unit-factory.name",
    "block.legacy-unit-factory-air.name",
    "block.legacy-unit-factory-ground.name",
    "block.command-center.name",
    "planet.gier.name",
    "planet.notva.name",
    "planet.tantros.name",
    "planet.verilus.name",
    "status.none.name",
    "unit.anthicus-missile.name",
    "unit.block.name",
    "unit.disrupt-missile.name",
    "unit.dummy.name",
    "unit.quell-missile.name",
    "weather.suspend-particles.name",
])


def parse_properties(path):
    """Minimal Java .properties parser (no unicode escapes needed for keys)."""
    entries = {}
    pending = None
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if pending is not None:
            line = pending + line
            pending = None
        if not line or line.startswith("#") or line.startswith("!"):
            continue
        if line.endswith("\\") and not line.endswith("\\\\"):
            pending = line[:-1]
            continue
        match = re.match(r"^([^=:\s][^=:]*?)\s*[=:]\s*(.*)$", line)
        if match:
            entries[match.group(1)] = match.group(2)
        else:
            entries[line] = ""
    return entries


def main():
    keys = parse_properties(BUNDLE)
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    (OUT_DIR / "bundle_keys.json").write_text(
        json.dumps(
            {
                "source": "core/assets/bundles/bundle.properties (source-derived; plan 03 owns bundle loading)",
                "keys": dict(sorted(keys.items())),
                "exempt": BUNDLE_EXEMPT,
            },
            indent=2,
            ensure_ascii=False,
        )
        + "\n",
        encoding="utf-8",
        newline="\n",
    )
    print(f"bundle_keys: {len(keys)} keys")

    regions = sorted({path.stem for path in SPRITES.rglob("*.png")})
    content = {}
    for path in sorted(SPRITES.rglob("*.png")):
        family = path.parent.name
        if family == "items" and path.stem.startswith("item-"):
            content.setdefault(f"item.{path.stem[len('item-'):]}", []).append(path.stem)
        elif family == "items" and path.stem.startswith("liquid-"):
            content.setdefault(f"liquid.{path.stem[len('liquid-'):]}", []).append(path.stem)
    (OUT_DIR / "asset_manifest.json").write_text(
        json.dumps(
            {
                "source": "core/assets-raw/sprites (source-derived fallback; plan 03 owns the packed manifest)",
                "regions": regions,
                "content": dict(sorted(content.items())),
            },
            indent=2,
            ensure_ascii=False,
        )
        + "\n",
        encoding="utf-8",
        newline="\n",
    )
    print(f"asset_manifest: {len(regions)} regions, {len(content)} content expectations")


if __name__ == "__main__":
    main()
