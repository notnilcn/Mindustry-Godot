#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Build, populate and validate TWIN-VERIFICATION.json.

TWIN-VERIFICATION.json replaces the retired chains.json (compact ledger and
recipe library) and parity-ledger.json (detailed parity ledger).  It keeps one
minimal record per parity item:

    title, status, similarity, fork, action, images

`fork` and `action` are the inlined Java and Godot tool-call chains.  `images`
is two index-aligned lists (java, godot) of canonical AVIF frames.  Only
verified items that are fixed or twin-verified get images; every other item's
PNGs stay in the gitignored runs/ tree.

The five Java-only recipes are paired 1:1 with a Godot recipe.  A pair that
does not reproduce the full Java action carries a `gap` string.

Usage:
    python3 tools/twin_verification.py build
    python3 tools/twin_verification.py images
    python3 tools/twin_verification.py check
    python3 tools/twin_verification.py prune --dry-run
    python3 tools/twin_verification.py prune --apply

`build` and `images` are one-shot migration commands: they read the retired
chains.json/parity-ledger.json (restore them from git history if a re-run is
ever needed).  `check` and `prune` work on the new file alone.

`images` needs Pillow with the AVIF plugin.  Use a scratch venv:

    python3 -m venv /tmp/twin-avif
    /tmp/twin-avif/bin/pip install pillow pillow-avif-plugin
    /tmp/twin-avif/bin/python tools/twin_verification.py images
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable

ROOT = Path(__file__).resolve().parents[1]
CHAINS = ROOT / "chains.json"
LEDGER = ROOT / "parity-ledger.json"
OUT = ROOT / "TWIN-VERIFICATION.json"
TWIN_DIR = ROOT / "parity" / "twin"
PRUNE_REPORT = Path("/tmp/opencode/twin_prune.json")


def _frame_diff_path() -> Path:
    candidates = [
        ROOT / ".opencode/skills/parity-eval/scripts/frame_diff.py",
        ROOT.parent / ".opencode/skills/parity-eval/scripts/frame_diff.py",
        Path.home() / ".claude/skills/parity-eval/scripts/frame_diff.py",
        Path.home() / ".opencode/skills/parity-eval/scripts/frame_diff.py",
    ]
    for candidate in candidates:
        if candidate.exists():
            return candidate
    return candidates[0]


FRAME_DIFF = _frame_diff_path()

SCHEMA = "mindustry-godot/twin-verification@1"
AVIF_QUALITY = 70
AVIF_SPEED = 6

# Pairings produced by the 2026-10-19 recipe-pairing pass.  Each Java-only
# recipe names exactly one Godot counterpart; `also_covers` lists other Godot
# recipes the same Java soak exercises.  All five pairs are partial: the gap
# describes what the Godot recipe does not reproduce.
JAVA_RECIPE_PAIRS: dict[str, dict[str, Any]] = {
    "java-campaign-loss": {
        "paired_with": "game-over-loss",
        "also_covers": [],
        "gap": (
            "Java reaches GameOverDialog via pause > Abandon > confirm > Continue -> planet map; "
            "Godot forces a zero-core serpulo:15 sector and only asserts stack ['restart']. "
            "No Abandon flow, Continue click, or planet return."
        ),
    },
    "java-custom-survival-wave": {
        "paired_with": "units-live-wave-runtime",
        "also_covers": ["hud-wave-enemies-skip"],
        "gap": (
            "Java plays a custom-game survival map and clicks the HUD skip triangle; "
            "Godot launches campaign groundZero and calls MindCampaign.run_wave()/step(). "
            "No custom map picker or skip click."
        ),
    },
    "java-join-dedicated-server": {
        "paired_with": "join-direct-connect",
        "also_covers": ["host-match-from-pause"],
        "gap": (
            "Java starts a dedicated server-release.jar, uses LAN discovery, connects and "
            "receives world data; Godot stops at MindNet.connect_to browsing. "
            "No dedicated server, LAN discovery, or world join."
        ),
    },
    "java-ref-ground-zero-tutorial": {
        "paired_with": "ground-zero-fresh-launch",
        "also_covers": [
            "ground-zero-resume-godot",
            "ground-zero-wasd-movement",
            "ground-zero-click-mining",
            "ground-zero-drill-placement",
            "ground-zero-conveyor-line",
        ],
        "gap": (
            "Java records R00-R16 (R00-R06 captured) driving the full tutorial chain; "
            "ground-zero-fresh-launch only launches and asserts build/core/objective 0. "
            "No R06+ chain (lead, turrets/ammo, walls, zones, defend, finish) and no gz.* bundle."
        ),
    },
    "java-reference-leg": {
        "paired_with": "boot-and-identity",
        "also_covers": ["ui-control-click"],
        "gap": (
            "Java covers detached launch, window normalize, menu drive, capture and quit; "
            "boot-and-identity ends at runtime_connected + pid stamp. "
            "No window normalize, menu capture, or quit half."
        ),
    },
}

GAPS = [
    "No Godot recipe reproduces java-ref-ground-zero-tutorial R06-R16 (lead, turrets/ammo, walls, zones, defend, finish); the gz.* objective bundle has no Godot counterpart.",
    "No dedicated-server/LAN-discovery/world-data join on the Godot side; join-direct-connect stops at MindNet browsing (PAR-0123/PAR-0124/PAR-0136).",
    "Campaign loss trigger differs: Godot forces a preset-less serpulo:15 zero-core sector; Java uses campaign sector 170 plus pause > Abandon. The Continue -> planet-map return is not exercised.",
    "Custom survival has no Godot custom-game map picker or HUD skip click; hud-wave-enemies-skip covers only the skip/HUD half.",
    "No generic Godot capture+quit leg; boot-and-identity ends at pid stamp, leaving java-reference-leg's capture/quit half unpaired.",
]


# ---------------------------------------------------------------------------
# source loading


def load_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as fh:
        return json.load(fh)


def load_sources() -> tuple[dict[str, Any], dict[str, dict[str, Any]]]:
    chains = load_json(CHAINS)
    ledger = load_json(LEDGER)
    detailed = {item["id"]: item for item in ledger["items"]}
    return chains, detailed


def scrub(value: Any) -> Any:
    """Replace stale references to the retired ledger files in prose/calls."""
    if isinstance(value, str):
        return (
            value.replace("parity-ledger.json", "TWIN-VERIFICATION.json")
            .replace("chains.json", "TWIN-VERIFICATION.json")
            .replace("seeding.last_sweep/frontier", "item queue")
        )
    if isinstance(value, list):
        return [scrub(entry) for entry in value]
    if isinstance(value, dict):
        return {key: scrub(entry) for key, entry in value.items()}
    return value


def expand_seq(chains: dict[str, Any], seq_id: str) -> list[Any]:
    calls = chains["calls"]
    seqs = chains["seqs"]
    expanded = [scrub(calls[call_id]) for call_id in seqs[seq_id]]
    # The detailed ledger sometimes used itself as a pseudo-tool to point at
    # path/reproduce; those entries are not calls and are dropped now that the
    # chains are inlined.
    return [
        call
        for call in expanded
        if not (isinstance(call, dict) and call.get("tool") in ("parity-ledger.json", "chains.json"))
    ]


# ---------------------------------------------------------------------------
# diff / similarity


def _resolve(path: str | None, bases: Iterable[Path]) -> Path | None:
    if not path:
        return None
    candidate = Path(path)
    if candidate.is_absolute():
        return candidate if candidate.exists() else None
    for base in bases:
        resolved = base / candidate
        if resolved.exists():
            return resolved
    resolved = ROOT / candidate
    return resolved if resolved.exists() else None


def _item_bases(compact: dict[str, Any], detailed: dict[str, Any]) -> list[Path]:
    bases: list[Path] = [ROOT]
    for value in (
        compact.get("run"),
        detailed.get("run"),
        (detailed.get("verified") or {}).get("twin_run"),
        (detailed.get("verified") or {}).get("writer_run"),
        (detailed.get("reference") or {}).get("run"),
    ):
        if value:
            path = ROOT / value if not Path(value).is_absolute() else Path(value)
            if path not in bases:
                bases.append(path)
    return bases


def _diff_metrics(path: Path) -> float | None:
    try:
        data = load_json(path)
    except Exception:
        return None
    if data.get("size_match") is False:
        return None
    changed = (data.get("diff") or {}).get("changed_fraction")
    if changed is None:
        return None
    return float(changed)


def _matches_pair(data: dict[str, Any], java: Path, godot: Path) -> bool:
    for key, want in (("java", java), ("godot", godot)):
        value = data.get(key)
        if not value:
            return False
        if Path(value).name != want.name:
            return False
    return True


def find_similarity(
    compact: dict[str, Any],
    detailed: dict[str, Any],
    bases: list[Path],
    pair: tuple[Path, Path] | None = None,
) -> float | None:
    """Return 100 * (1 - changed_fraction) for the action diff, if known."""
    candidates: list[Path] = []

    # Diff paths stored on the detailed item's steps.
    for step in detailed.get("steps") or []:
        diff = step.get("diff")
        resolved = _resolve(diff, bases)
        if resolved:
            candidates.append(resolved)

    # Any diff under the item's run directories.
    for base in bases:
        diff_dir = base / "diff"
        if diff_dir.is_dir():
            candidates.extend(sorted(diff_dir.glob("*.json")))

    # Prefer a diff that names an action pair, then a stored pair match, then
    # the lowest changed fraction.
    if pair:
        for path in candidates:
            try:
                data = load_json(path)
            except Exception:
                continue
            if _matches_pair(data, pair[0], pair[1]):
                changed = _diff_metrics(path)
                if changed is not None:
                    return round(100.0 * (1.0 - changed), 1)

    action = [p for p in candidates if "action" in p.name.lower()]
    ranked = action or candidates
    valid = [(path, _diff_metrics(path)) for path in ranked]
    valid = [(path, changed) for path, changed in valid if changed is not None]
    if not valid:
        return None
    changed = min(changed for _, changed in valid)
    return round(100.0 * (1.0 - changed), 1)


# ---------------------------------------------------------------------------
# items


def build_item(iid: str, compact: dict[str, Any], detailed: dict[str, Any], chains: dict[str, Any]) -> dict[str, Any]:
    item: dict[str, Any] = {}
    title = compact.get("t") or detailed.get("title")
    status = compact.get("st") or detailed.get("status")
    if title:
        item["title"] = title
    if status:
        item["status"] = status

    bases = _item_bases(compact, detailed)
    fork: dict[str, Any] = {}
    action: dict[str, Any] = {}

    for leg in ("java", "godot"):
        seq = (compact.get("path") or {}).get(leg)
        calls = expand_seq(chains, seq) if seq else scrub((detailed.get("path") or {}).get(leg))
        if calls:
            fork[leg] = calls

        seq = (compact.get("rep") or {}).get(leg)
        calls = expand_seq(chains, seq) if seq else scrub((detailed.get("reproduce") or {}).get(leg))
        if calls:
            action[leg] = calls

    if fork:
        item["fork"] = fork
    if action:
        item["action"] = action

    similarity = find_similarity(compact, detailed, bases)
    if similarity is not None:
        item["similarity"] = similarity
    return item


def build_items(chains: dict[str, Any], detailed: dict[str, dict[str, Any]]) -> dict[str, Any]:
    items: dict[str, Any] = {}
    for iid in sorted(chains["ledger"]):
        items[iid] = build_item(iid, chains["ledger"][iid], detailed.get(iid, {}), chains)
    return items


# ---------------------------------------------------------------------------
# recipes


def _split_steps(recipe: dict[str, Any]) -> tuple[list[str], list[str], list[str], list[str]]:
    java: list[str] = []
    godot: list[str] = []
    compare: list[str] = []
    shared: list[str] = []
    for step in recipe.get("steps") or []:
        text = str(step)
        if text.startswith("[java]"):
            java.append(text[len("[java]"):].lstrip())
        elif text.startswith("[godot]"):
            godot.append(text[len("[godot]"):].lstrip())
        elif text.startswith("[compare]"):
            compare.append(text[len("[compare]"):].lstrip())
        else:
            shared.append(text)

    tools = recipe.get("tools") or []
    has_java = any(str(tool).startswith(("java_", "window_", "capture_", "frame_")) for tool in tools)
    has_godot = any(str(tool).startswith("godot_") for tool in tools)
    if not (java or godot or compare) and shared:
        if has_godot and not has_java:
            godot, shared = shared, []
        elif has_java and not has_godot:
            java, shared = shared, []
    return java, godot, compare, shared


def build_recipes(chains: dict[str, Any]) -> dict[str, Any]:
    recipes: dict[str, Any] = {}
    for recipe in chains["chains"]:
        rid = recipe["id"]
        java, godot, compare, shared = _split_steps(recipe)
        entry: dict[str, Any] = {
            "title": scrub(recipe.get("title")),
            "status": recipe.get("status"),
        }
        for key in ("when", "pre", "success", "par", "refs", "tools"):
            if recipe.get(key) is not None:
                entry[key] = scrub(recipe[key])

        if java:
            entry.setdefault("java", {})["steps"] = scrub(java)
        if godot:
            entry.setdefault("godot", {})["steps"] = scrub(godot)
        if compare:
            entry.setdefault("compare", {})["steps"] = scrub(compare)
        if shared:
            entry["steps"] = scrub(shared)

        if rid in JAVA_RECIPE_PAIRS:
            pair = JAVA_RECIPE_PAIRS[rid]
            entry["paired_with"] = pair["paired_with"]
            if pair["also_covers"]:
                entry["also_covers"] = pair["also_covers"]
            entry["gap"] = pair["gap"]
        recipes[rid] = entry

    # Symmetric back-links on the Godot counterparts.
    for java_id, pair in JAVA_RECIPE_PAIRS.items():
        godot_id = pair["paired_with"]
        if godot_id in recipes:
            recipes[godot_id].setdefault("paired_with", java_id)

    return dict(sorted(recipes.items()))


# ---------------------------------------------------------------------------
# image selection


def _pair_from_verified(detailed: dict[str, Any], bases: list[Path]) -> tuple[Path, Path, str] | None:
    verified = detailed.get("verified") or {}
    java = _resolve(verified.get("java_frame"), bases)
    godot = _resolve(verified.get("godot_frame"), bases)
    if java and godot:
        return java, godot, "action"
    return None


def _pair_from_steps(detailed: dict[str, Any], bases: list[Path]) -> tuple[Path, Path, str] | None:
    steps = detailed.get("steps") or []
    for step in reversed(steps):
        java = _resolve(step.get("java_frame"), bases)
        godot = _resolve(step.get("godot_frame"), bases)
        if java and godot:
            return java, godot, "action"
    return None


def _run_dirs(compact: dict[str, Any], detailed: dict[str, Any], bases: list[Path]) -> list[Path]:
    candidates = [Path(base) for base in bases if base != ROOT]
    return [path for path in candidates if path.is_dir()]


def _pair_from_runs(compact: dict[str, Any], detailed: dict[str, Any], bases: list[Path], label: str | None = None) -> tuple[Path, Path, str] | None:
    for run in _run_dirs(compact, detailed, bases):
        java_dir = run / "java"
        godot_dir = run / "godot"
        if not java_dir.is_dir() or not godot_dir.is_dir():
            continue
        if label:
            java = java_dir / f"{label}.png"
            godot = godot_dir / f"{label}.png"
            if java.exists() and godot.exists():
                return java, godot, label
            continue
        java_names = {path.name for path in java_dir.glob("*.png")}
        godot_names = {path.name for path in godot_dir.glob("*.png")}
        common = java_names & godot_names
        if not common:
            continue
        for preferred in ("action.png", "fork.png"):
            if preferred in common:
                return java_dir / preferred, godot_dir / preferred, preferred[:-4]
        name = sorted(common)[0]
        return java_dir / name, godot_dir / name, Path(name).stem
    return None


def select_pairs(
    compact: dict[str, Any],
    detailed: dict[str, Any],
) -> tuple[tuple[Path, Path, str] | None, tuple[Path, Path, str] | None]:
    bases = _item_bases(compact, detailed)
    action = (
        _pair_from_verified(detailed, bases)
        or _pair_from_steps(detailed, bases)
        or _pair_from_runs(compact, detailed, bases, "action")
        or _pair_from_runs(compact, detailed, bases)
    )
    fork = None
    # Only add a fork pair when both action frames come from the same run, so
    # a cross-run Java reference + Godot fix pair never borrows another item's
    # fork frames.
    if action and action[0].parent.parent == action[1].parent.parent:
        run = action[0].parent.parent
        java = run / "java" / "fork.png"
        godot = run / "godot" / "fork.png"
        if java.exists() and godot.exists():
            fork = (java, godot, "fork")
    return action, fork


def _qualifies(compact: dict[str, Any], detailed: dict[str, Any]) -> bool:
    if (compact.get("st") or detailed.get("status")) != "verified":
        return False
    verified = detailed.get("verified") or {}
    fix = detailed.get("fix") or {}
    run = compact.get("run") or detailed.get("run") or ""
    if verified.get("mode") == "twin":
        return True
    if fix.get("status") in ("closed", "done", "none-needed"):
        return True
    return bool(re.search(r"twin|verify", run, re.I))


def _diff_for_pair(detailed: dict[str, Any], bases: list[Path], java: Path, godot: Path) -> float | None:
    candidates: list[Path] = []
    for step in detailed.get("steps") or []:
        resolved = _resolve(step.get("diff"), bases)
        if resolved:
            candidates.append(resolved)
    for base in bases:
        diff_dir = base / "diff"
        if diff_dir.is_dir():
            candidates.extend(sorted(diff_dir.glob("*.json")))
    for path in candidates:
        try:
            data = load_json(path)
        except Exception:
            continue
        if _matches_pair(data, java, godot):
            changed = _diff_metrics(path)
            if changed is not None:
                return round(100.0 * (1.0 - changed), 1)
    return None


def _run_frame_diff(java: Path, godot: Path) -> float | None:
    out = Path("/tmp/opencode/twin_pair_diff.json")
    out.parent.mkdir(parents=True, exist_ok=True)
    try:
        subprocess.run(
            [sys.executable, str(FRAME_DIFF), "--java", str(java), "--godot", str(godot), "--out-json", str(out)],
            check=True,
            capture_output=True,
            text=True,
        )
    except Exception as exc:
        print(f"frame_diff failed for {java} vs {godot}: {exc}", file=sys.stderr)
        return None
    changed = _diff_metrics(out)
    if changed is None:
        return None
    return round(100.0 * (1.0 - changed), 1)


def _convert(src: Path, dst: Path) -> None:
    from PIL import Image

    dst.parent.mkdir(parents=True, exist_ok=True)
    with Image.open(src) as image:
        image.convert("RGB").save(dst, "AVIF", quality=AVIF_QUALITY, speed=AVIF_SPEED)


def populate_images(doc: dict[str, Any], chains: dict[str, Any], detailed: dict[str, dict[str, Any]]) -> dict[str, Any]:
    from PIL import Image  # noqa: F401  (fail early without Pillow)

    try:
        import pillow_avif  # noqa: F401
    except Exception:
        sys.exit("pillow-avif-plugin is missing; run images with the scratch AVIF venv (see module docstring)")

    if TWIN_DIR.exists():
        for child in TWIN_DIR.iterdir():
            if child.is_dir() and child.name.startswith("PAR-"):
                shutil.rmtree(child)
    for item in doc["items"].values():
        item.pop("images", None)

    prune: list[dict[str, Any]] = []
    converted = 0
    for iid in sorted(doc["items"]):
        compact = chains["ledger"].get(iid, {})
        item_detailed = detailed.get(iid, {})
        if not _qualifies(compact, item_detailed):
            continue
        action, fork = select_pairs(compact, item_detailed)
        if action is None:
            continue

        java_dir = TWIN_DIR / iid / "java"
        godot_dir = TWIN_DIR / iid / "godot"
        images: dict[str, list[str]] = {"java": [], "godot": []}
        sources: dict[str, list[Path]] = {"java": [], "godot": []}

        pairs: list[tuple[Path, Path, str, str]] = []
        if fork:
            pairs.append((fork[0], fork[1], "01-fork", "fork"))
        label = "02-action" if fork else "01-action"
        pairs.append((action[0], action[1], label, "action"))

        for java_src, godot_src, name, kind in pairs:
            for leg, src, out_dir in (("java", java_src, java_dir), ("godot", godot_src, godot_dir)):
                dst = out_dir / f"{name}.avif"
                _convert(src, dst)
                images[leg].append(str(dst.relative_to(ROOT)))
                sources[leg].append(src)
                converted += 1
                if kind == "fork":
                    continue
                bases = _item_bases(compact, item_detailed)
                similarity = _diff_for_pair(item_detailed, bases, java_src, godot_src)
                if similarity is None:
                    similarity = _run_frame_diff(java_src, godot_src)
                if similarity is not None:
                    doc["items"][iid]["similarity"] = similarity

        doc["items"][iid]["images"] = images
        prune.append(
            {
                "item": iid,
                "java": [str(path) for path in sources["java"]],
                "godot": [str(path) for path in sources["godot"]],
            }
        )

    doc["updated"] = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    PRUNE_REPORT.parent.mkdir(parents=True, exist_ok=True)
    with PRUNE_REPORT.open("w", encoding="utf-8") as fh:
        json.dump({"converted": converted, "items": prune}, fh, indent=2)
    print(f"converted {converted} images for {len(prune)} items; prune report: {PRUNE_REPORT}")
    return doc


# ---------------------------------------------------------------------------
# validation


def check(doc: dict[str, Any]) -> list[str]:
    errors: list[str] = []
    if doc.get("schema") != SCHEMA:
        errors.append(f"schema is {doc.get('schema')!r}, expected {SCHEMA!r}")
    items = doc.get("items") or {}
    recipes = doc.get("recipes") or {}
    if not items:
        errors.append("items is empty")
    if not recipes:
        errors.append("recipes is empty")
    if not doc.get("gaps"):
        errors.append("gaps list is empty")
    statuses = {"seeded", "reference", "in-progress", "mismatch", "fixing", "verified", "blocked", "wontfix"}

    for iid, item in items.items():
        if not re.fullmatch(r"PAR-\d{4}", iid):
            errors.append(f"bad item id {iid}")
        if not item.get("title"):
            errors.append(f"{iid}: missing title")
        if item.get("status") not in statuses:
            errors.append(f"{iid}: bad status {item.get('status')!r}")
        if "similarity" in item and not (0 <= item["similarity"] <= 100):
            errors.append(f"{iid}: similarity {item['similarity']} outside 0-100")
        for phase in ("fork", "action"):
            calls = item.get(phase)
            if calls is None:
                continue
            if not isinstance(calls, dict):
                errors.append(f"{iid}: {phase} is not an object")
                continue
            for leg, chain in calls.items():
                if leg not in ("java", "godot"):
                    errors.append(f"{iid}: {phase} has unknown leg {leg}")
                if not isinstance(chain, list) or not chain:
                    errors.append(f"{iid}: {phase}.{leg} is not a non-empty list")
        images = item.get("images")
        if images is None:
            continue
        java = images.get("java") or []
        godot = images.get("godot") or []
        if len(java) != len(godot) or not java:
            errors.append(f"{iid}: image lists are not non-empty and index-aligned")
        for path in java + godot:
            if not path.endswith(".avif"):
                errors.append(f"{iid}: image is not AVIF: {path}")
            elif not (ROOT / path).exists():
                errors.append(f"{iid}: missing image {path}")
        if not item.get("similarity"):
            errors.append(f"{iid}: has images but no similarity")

    for rid, recipe in recipes.items():
        if not recipe.get("title"):
            errors.append(f"{rid}: missing title")
        paired = recipe.get("paired_with")
        if paired and paired not in recipes:
            errors.append(f"{rid}: paired_with {paired} does not resolve")
        for covered in recipe.get("also_covers") or []:
            if covered not in recipes:
                errors.append(f"{rid}: also_covers {covered} does not resolve")
        if recipe.get("gap") and not str(recipe["gap"]).strip():
            errors.append(f"{rid}: empty gap")

    for gap in doc.get("gaps") or []:
        if not str(gap).strip():
            errors.append("empty gap entry")
    return errors


# ---------------------------------------------------------------------------
# prune


def prune(apply: bool) -> None:
    if not PRUNE_REPORT.exists():
        sys.exit(f"missing {PRUNE_REPORT}; run `images` first")
    report = load_json(PRUNE_REPORT)
    sources: list[Path] = []
    for entry in report.get("items") or []:
        for key in ("java", "godot"):
            for path in entry.get(key) or []:
                candidate = Path(path)
                if not candidate.is_absolute():
                    candidate = ROOT / candidate
                if candidate.exists():
                    sources.append(candidate)

    # Associated diff artifacts for the converted source frames.
    artifacts: list[Path] = []
    for source in sources:
        run = source.parent.parent
        if (run / "diff").is_dir():
            for diff in (run / "diff").glob("*.json"):
                try:
                    data = load_json(diff)
                except Exception:
                    continue
                if Path(data.get("java", "")).name == source.name or Path(data.get("godot", "")).name == source.name:
                    artifacts.append(diff)
                    for key in ("heatmap", "side_by_side"):
                        value = data.get(key)
                        if value:
                            resolved = ROOT / value if not Path(value).is_absolute() else Path(value)
                            if resolved.exists():
                                artifacts.append(resolved)

    unique = sorted({path for path in sources + artifacts if path.exists()})
    total = sum(path.stat().st_size for path in unique)
    print(f"{'deleting' if apply else 'would delete'} {len(unique)} files, {total / (1024 * 1024):.1f} MiB")
    for path in unique:
        print(f"  {path.relative_to(ROOT) if path.is_relative_to(ROOT) else path}")
        if apply:
            path.unlink()
            parent = path.parent
            if parent.name in ("java", "godot", "diff") and parent.is_dir() and not any(parent.iterdir()):
                parent.rmdir()
    if apply:
        PRUNE_REPORT.unlink(missing_ok=True)


# ---------------------------------------------------------------------------
# CLI


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("build", help="write TWIN-VERIFICATION.json from the retired sources")
    sub.add_parser("images", help="select canonical pairs, convert AVIF, fill similarity")
    sub.add_parser("check", help="validate TWIN-VERIFICATION.json")
    prune_parser = sub.add_parser("prune", help="delete source PNG/diff artifacts replaced by AVIF")
    prune_parser.add_argument("--apply", action="store_true", help="delete (default: dry run)")

    args = parser.parse_args()

    if args.command == "build":
        chains, detailed = load_sources()
        doc = {
            "schema": SCHEMA,
            "updated": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "items": build_items(chains, detailed),
            "recipes": build_recipes(chains),
            "gaps": GAPS,
        }
        OUT.write_text(json.dumps(doc, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        print(f"wrote {OUT.relative_to(ROOT)}: {len(doc['items'])} items, {len(doc['recipes'])} recipes")
        return 0

    if args.command == "images":
        chains, detailed = load_sources()
        if not OUT.exists():
            sys.exit(f"{OUT} missing; run `build` first")
        doc = load_json(OUT)
        doc = populate_images(doc, chains, detailed)
        OUT.write_text(json.dumps(doc, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        print(f"updated {OUT.relative_to(ROOT)}")
        return 0

    if args.command == "check":
        doc = load_json(OUT)
        errors = check(doc)
        if errors:
            for error in errors:
                print(f"ERROR: {error}")
            return 1
        print(
            f"ok: {len(doc['items'])} items, {len(doc['recipes'])} recipes, "
            f"{len(doc['gaps'])} gaps, "
            f"{sum(1 for item in doc['items'].values() if item.get('images'))} image sets"
        )
        return 0

    if args.command == "prune":
        prune(args.apply)
        return 0

    parser.error("unknown command")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
