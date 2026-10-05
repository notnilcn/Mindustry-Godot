#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Single-writer helper for the parity-eval findings ledger.

The evaluator is the only writer. Implementers claim fixes in commit messages
("Fixes EV-0001"); the evaluator re-runs the repro and calls `verify`.

Examples:
    record_finding.py add --area ui/menu --severity S2 \
        --title "Campaign button does not open the planet view" \
        --expected "Java: Play > Campaign opens the Serpulo planet view" \
        --actual "Godot: click leaves the menu unchanged" \
        --repro "scenario boot_menu step 4" \
        --evidence runs/20261005-120000-boot_menu/godot/step-04.png --plan 14
    record_finding.py verify --id EV-0001 --status verified-fixed \
        --note "re-ran boot_menu at 0706963" --evidence runs/.../step-04.png
    record_finding.py list --status open
    record_finding.py summary
"""

from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

DEFAULT_LEDGER = Path(".opencode/evals/findings.json")
STATUSES = ("open", "in-progress", "verified-fixed", "regression", "wontfix")
SEVERITIES = ("S1", "S2", "S3", "S4")


def now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def load(path: Path) -> dict:
    if not path.is_file():
        return {"format": 1, "updated": None, "findings": []}
    data = json.loads(path.read_text(encoding="utf-8"))
    data.setdefault("format", 1)
    data.setdefault("findings", [])
    return data


def save(path: Path, data: dict) -> None:
    data["updated"] = now()
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".json.tmp")
    tmp.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    tmp.replace(path)


def next_id(data: dict) -> str:
    used = {f["id"] for f in data["findings"]}
    n = 1
    while f"EV-{n:04d}" in used:
        n += 1
    return f"EV-{n:04d}"


def find(data: dict, finding_id: str) -> dict:
    for f in data["findings"]:
        if f["id"] == finding_id:
            return f
    sys.exit(f"unknown finding id: {finding_id}")


def cmd_add(args: argparse.Namespace) -> int:
    data = load(args.ledger)
    finding = {
        "id": next_id(data),
        "area": args.area,
        "severity": args.severity,
        "status": "open",
        "title": args.title,
        "expected": args.expected,
        "actual": args.actual,
        "repro": args.repro,
        "evidence": list(args.evidence or []),
        "notes": list(args.notes or []),
        "plan": args.plan,
        "first_seen": now(),
        "last_verified": now(),
    }
    data["findings"].append(finding)
    save(args.ledger, data)
    print(finding["id"])
    return 0


def cmd_verify(args: argparse.Namespace) -> int:
    data = load(args.ledger)
    finding = find(data, args.id)
    previous = finding["status"]
    finding["status"] = args.status
    finding["last_verified"] = now()
    if args.note:
        finding["notes"].append(f"{now()}: {args.note}")
    if args.evidence:
        finding.setdefault("evidence", []).extend(args.evidence)
    save(args.ledger, data)
    print(f"{finding['id']}: {previous} -> {args.status}")
    return 0


def cmd_list(args: argparse.Namespace) -> int:
    data = load(args.ledger)
    rows = data["findings"]
    if args.status:
        rows = [f for f in rows if f["status"] == args.status]
    if args.severity:
        rows = [f for f in rows if f["severity"] == args.severity]
    if args.area:
        rows = [f for f in rows if f["area"].startswith(args.area)]
    print(json.dumps(rows, indent=2))
    return 0


def cmd_summary(args: argparse.Namespace) -> int:
    data = load(args.ledger)
    by_status: dict[str, int] = {}
    by_severity: dict[str, int] = {}
    for f in data["findings"]:
        by_status[f["status"]] = by_status.get(f["status"], 0) + 1
        by_severity[f["severity"]] = by_severity.get(f["severity"], 0) + 1
    print(json.dumps({
        "total": len(data["findings"]),
        "by_status": by_status,
        "by_severity": by_severity,
        "updated": data.get("updated"),
    }, indent=2))
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ledger", type=Path, default=DEFAULT_LEDGER)
    sub = parser.add_subparsers(dest="command", required=True)

    p_add = sub.add_parser("add", help="record a new finding")
    p_add.add_argument("--area", required=True, help="e.g. ui/menu, combat/turrets")
    p_add.add_argument("--severity", required=True, choices=SEVERITIES)
    p_add.add_argument("--title", required=True)
    p_add.add_argument("--expected", required=True)
    p_add.add_argument("--actual", required=True)
    p_add.add_argument("--repro", required=True)
    p_add.add_argument("--evidence", action="append", default=[])
    p_add.add_argument("--note", action="append", dest="notes", default=[])
    p_add.add_argument("--plan", default=None, help="owning plan, e.g. 14")
    p_add.set_defaults(func=cmd_add)

    p_verify = sub.add_parser("verify", help="re-verification result for an existing finding")
    p_verify.add_argument("--id", required=True)
    p_verify.add_argument("--status", required=True, choices=STATUSES)
    p_verify.add_argument("--note")
    p_verify.add_argument("--evidence", action="append", default=[])
    p_verify.set_defaults(func=cmd_verify)

    p_list = sub.add_parser("list", help="list findings as JSON")
    p_list.add_argument("--status", choices=STATUSES)
    p_list.add_argument("--severity", choices=SEVERITIES)
    p_list.add_argument("--area")
    p_list.set_defaults(func=cmd_list)

    p_summary = sub.add_parser("summary", help="counts by status/severity")
    p_summary.set_defaults(func=cmd_summary)

    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
