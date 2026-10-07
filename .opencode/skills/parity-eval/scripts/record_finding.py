#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Single-writer helper for the parity-eval findings ledger.

The gap identifier seeds code-sourced candidates (`add --source code`, status
`open`, or `needs-evaluation` when the candidate wants an in-engine look). The
evaluator writes engine-sourced findings (`add --source engine`) and the final
verdicts (`verify`). Implementers claim work with `claim --for fix`; evaluators
claim with `claim --for evaluation`; `/loop-gaps` loops claim either with
`claim --for loop`.

Every claim is a cross-session lock: a finding with a fresh `owner` is
invisible to all pickers until the owner releases it, verifies it, or the claim
goes stale and `reap` restores it. Parallel sessions share one ledger:
`PARITY_LEDGER` overrides the default path, and every mutating command holds
`<ledger>.lock` for the whole read-modify-write.

Examples:
    record_finding.py add --area ui/menu --severity S2 \
        --title "Campaign button does not open the planet view" \
        --expected "Java: Play > Campaign opens the Serpulo planet view" \
        --actual "Godot: click leaves the menu unchanged" \
        --repro "scenario boot_menu step 4" \
        --evidence runs/20261005-120000-boot_menu/godot/step-04.png --plan 14
    record_finding.py add --area game/campaign --severity S1 --source code \
        --confidence high --title "Launch never applies the sector rules" \
        --expected "World.java:265-330 setSectorRules applies the preset rules" \
        --actual "campaign.rs:124 calls play_new_sector with Rules::default()" \
        --repro "start_sector('serpulo',170); eval rules.waves"
    record_finding.py claim --for fix --area ui --owner fix-session-3
    record_finding.py set-status --id EV-0001 --status needs-evaluation \
        --note "fix committed; evaluator queue"
    record_finding.py verify --id EV-0001 --status verified-fixed \
        --note "re-ran boot_menu at 0706963" --evidence runs/.../step-04.png
    record_finding.py verify --id EV-0002 --status verified-unfixed \
        --note "repro still shows the gap"
    record_finding.py release --id EV-0001 --note "fix abandoned"
    record_finding.py reap --older-than-minutes 90
    record_finding.py areas --for fix
    record_finding.py list --status needs-evaluation
    record_finding.py summary
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from contextlib import contextmanager
from datetime import datetime, timezone
from pathlib import Path

try:
    import fcntl
except ImportError:  # pragma: no cover - non-POSIX fallback
    fcntl = None

DEFAULT_LEDGER = Path(".opencode/evals/findings.json")
# `claimed` is the transient cross-session lock. A finding can also carry an
# owner while its status reads `needs-evaluation` (the fixer marked it for the
# evaluator mid-loop); pickers skip any finding with a fresh owner.
STATUSES = (
    "open",
    "needs-evaluation",
    "claimed",
    "verified-fixed",
    "verified-unfixed",
    "regression",
    "wontfix",
)
# What verify/set-status/release may assign (never the transient lock).
SETTABLE = tuple(s for s in STATUSES if s != "claimed")
# Statuses from the scrapped workflow, mapped on load so old ledgers keep
# working; `migrate` persists the mapping with a note.
LEGACY = {
    "in-progress": "needs-evaluation",
    "code-verified": "needs-evaluation",
}
# A finding is done for the whole workflow once it is verified-fixed or is an
# accepted deviation.
TERMINAL = ("verified-fixed", "wontfix")
CLAIM_KINDS = ("fix", "evaluation", "loop")
# Which statuses each kind of worker may claim.
POOLS = {
    "fix": ("open", "regression", "verified-unfixed"),
    "evaluation": ("needs-evaluation",),
    "loop": ("open", "regression", "verified-unfixed", "needs-evaluation"),
}
SEVERITIES = ("S1", "S2", "S3", "S4")
SOURCES = ("code", "engine")
CONFIDENCE = ("high", "medium", "low")
DEFAULT_STALE_MINUTES = 90


def now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def resolve_ledger(path: Path | None) -> Path:
    """CLI --ledger wins, then PARITY_LEDGER, then the repo default."""
    if path is not None:
        return path
    env = os.environ.get("PARITY_LEDGER")
    return Path(env) if env else DEFAULT_LEDGER


@contextmanager
def ledger_lock(path: Path):
    """Exclusive cross-process lock for the whole read-modify-write."""
    if fcntl is None:
        yield
        return
    lock_path = path.with_name(path.name + ".lock")
    lock_path.parent.mkdir(parents=True, exist_ok=True)
    with open(lock_path, "w", encoding="utf-8") as handle:
        fcntl.flock(handle.fileno(), fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(handle.fileno(), fcntl.LOCK_UN)


def load_raw(path: Path) -> dict:
    if not path.is_file():
        return {"format": 1, "updated": None, "findings": []}
    data = json.loads(path.read_text(encoding="utf-8"))
    data.setdefault("format", 1)
    data.setdefault("findings", [])
    return data


def normalize(data: dict, annotate: bool = False) -> int:
    """Map legacy statuses onto the current set; optionally persist the note."""
    changed = 0
    for finding in data["findings"]:
        status = finding.get("status")
        if status not in LEGACY:
            continue
        finding["status"] = LEGACY[status]
        changed += 1
        if annotate:
            note(finding, f"migrated status {status} -> {LEGACY[status]}")
    return changed


def load(path: Path) -> dict:
    data = load_raw(path)
    normalize(data)
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


def note(finding: dict, text: str) -> None:
    finding.setdefault("notes", []).append(f"{now()}: {text}")


def claim_stamp(finding: dict) -> datetime | None:
    raw = finding.get("claimed_at")
    if not raw:
        return None
    try:
        return datetime.fromisoformat(raw)
    except ValueError:
        return None


def is_stale(finding: dict, minutes: int) -> bool:
    """True when a claim is old enough to reclaim or reap (or has no stamp)."""
    stamp = claim_stamp(finding)
    if stamp is None:
        return True
    return (datetime.now(timezone.utc) - stamp).total_seconds() >= minutes * 60


def clear_claim(finding: dict) -> None:
    for key in ("owner", "claimed_at", "claimed_for", "prev_status"):
        finding.pop(key, None)


def pool_rows(
    data: dict,
    kind: str,
    *,
    severity: str | None = None,
    areas: list[str] | None = None,
    exclude: set[str] | None = None,
    stale_minutes: int = DEFAULT_STALE_MINUTES,
) -> list[dict]:
    """Claimable rows for `kind`: in the pool, no fresh owner, filters pass."""
    rows = []
    for f in data["findings"]:
        if f.get("status") not in POOLS[kind]:
            continue
        if f.get("owner") and not is_stale(f, stale_minutes):
            continue
        if exclude and f.get("id") in exclude:
            continue
        if severity and f.get("severity") != severity:
            continue
        if areas and not any(str(f.get("area", "")).startswith(a) for a in areas):
            continue
        rows.append(f)
    return rows


def sort_rows(rows: list[dict]) -> list[dict]:
    return sorted(
        rows, key=lambda f: (SEVERITIES.index(f["severity"]), f.get("first_seen", ""))
    )


def cmd_add(args: argparse.Namespace) -> int:
    with ledger_lock(args.ledger):
        data = load(args.ledger)
        finding = {
            "id": next_id(data),
            "area": args.area,
            "severity": args.severity,
            "status": args.status,
            "title": args.title,
            "expected": args.expected,
            "actual": args.actual,
            "repro": args.repro,
            "source": args.source,
            "confidence": args.confidence,
            "evidence": list(args.evidence or []),
            "notes": list(args.notes or []),
            "plan": args.plan,
            "first_seen": now(),
            "last_verified": now(),
        }
        if os.environ.get("PARITY_LOOP"):
            finding["found_by_loop"] = os.environ["PARITY_LOOP"]
        data["findings"].append(finding)
        save(args.ledger, data)
    print(finding["id"])
    return 0


def cmd_verify(args: argparse.Namespace) -> int:
    """Record an engine verdict and drop the claim."""
    with ledger_lock(args.ledger):
        data = load(args.ledger)
        finding = find(data, args.id)
        previous = finding["status"]
        finding["status"] = args.status
        finding["last_verified"] = now()
        clear_claim(finding)
        if args.note:
            note(finding, f"[verify] {args.note}")
        if args.evidence:
            finding.setdefault("evidence", []).extend(args.evidence)
        save(args.ledger, data)
    print(f"{finding['id']}: {previous} -> {args.status}")
    return 0


def cmd_set_status(args: argparse.Namespace) -> int:
    """Update a status without dropping the claim.

    Used by a fixer to move a claimed finding into the evaluator queue
    (`needs-evaluation`) before the evaluation leg runs, so a session that dies
    mid-evaluation leaves it pickable. While a claim is live, `prev_status` is
    updated too, so `reap`/`release` restore the finding to the last real
    status rather than the status it had when it was claimed.
    """
    with ledger_lock(args.ledger):
        data = load(args.ledger)
        finding = find(data, args.id)
        previous = finding["status"]
        finding["status"] = args.status
        if finding.get("owner"):
            finding["prev_status"] = args.status
        if args.note:
            note(finding, f"[{args.status}] {args.note}")
        save(args.ledger, data)
    print(f"{finding['id']}: {previous} -> {args.status}")
    return 0


def cmd_claim(args: argparse.Namespace) -> int:
    """Atomically claim the highest-priority claimable finding for a kind."""
    with ledger_lock(args.ledger):
        data = load(args.ledger)
        rows = pool_rows(
            data,
            args.for_,
            severity=args.severity,
            areas=args.area or None,
            exclude=set(args.exclude or []),
            stale_minutes=args.stale_minutes,
        )
        if not rows:
            print("null")
            return 3
        finding = sort_rows(rows)[0]
        if finding.get("owner"):
            note(finding, f"reclaimed stale claim from {finding['owner']}")
        owner = args.owner or f"oc-{os.getppid()}"
        finding["prev_status"] = finding["status"]
        finding["status"] = "claimed"
        finding["owner"] = owner
        finding["claimed_at"] = now()
        finding["claimed_for"] = args.for_
        finding["last_verified"] = now()
        note(finding, f"claimed by {owner} for {args.for_}")
        save(args.ledger, data)
    print(
        json.dumps(
            {
                "id": finding["id"],
                "severity": finding["severity"],
                "area": finding["area"],
                "title": finding["title"],
                "expected": finding.get("expected"),
                "actual": finding.get("actual"),
                "repro": finding.get("repro"),
                "evidence": finding.get("evidence", []),
                "notes": finding.get("notes", [])[-3:],
                "prev_status": finding.get("prev_status"),
                "owner": owner,
                "claimed_for": args.for_,
            },
            indent=2,
        )
    )
    return 0


def cmd_release(args: argparse.Namespace) -> int:
    """Drop a claim and restore the finding to its pre-claim (or named) status."""
    with ledger_lock(args.ledger):
        data = load(args.ledger)
        finding = find(data, args.id)
        previous = finding["status"]
        target = args.status or finding.get("prev_status") or "open"
        finding["status"] = target
        clear_claim(finding)
        finding["last_verified"] = now()
        if args.note:
            note(finding, f"released: {args.note}")
        save(args.ledger, data)
    print(f"{finding['id']}: {previous} -> {target}")
    return 0


def cmd_reap(args: argparse.Namespace) -> int:
    """Release claims older than the TTL back to their pre-claim status."""
    with ledger_lock(args.ledger):
        data = load(args.ledger)
        reaped = []
        for finding in data["findings"]:
            if not finding.get("owner") or not is_stale(
                finding, args.older_than_minutes
            ):
                continue
            owner = finding["owner"]
            previous = finding["status"]
            target = finding.get("prev_status") or "needs-evaluation"
            finding["status"] = target
            clear_claim(finding)
            note(
                finding,
                f"reaped stale claim ({owner}, was {previous}); restored {target}",
            )
            reaped.append(finding["id"])
        if reaped:
            save(args.ledger, data)
    print(json.dumps(reaped))
    return 0


def cmd_areas(args: argparse.Namespace) -> int:
    """Print distinct claimable areas for a kind, with counts."""
    data = load(args.ledger)
    rows = pool_rows(data, args.for_)
    counts: dict[str, int] = {}
    for finding in rows:
        area = str(finding.get("area", ""))
        counts[area] = counts.get(area, 0) + 1
    print(
        json.dumps(
            {
                "for": args.for_,
                "total": len(rows),
                "areas": dict(sorted(counts.items())),
            },
            indent=2,
        )
    )
    return 0


def cmd_migrate(args: argparse.Namespace) -> int:
    """Persist the legacy-status mapping from the scrapped workflow."""
    with ledger_lock(args.ledger):
        data = load_raw(args.ledger)
        changed = normalize(data, annotate=True)
        if changed:
            save(args.ledger, data)
    print(json.dumps({"migrated": changed}))
    return 0


def cmd_list(args: argparse.Namespace) -> int:
    data = load(args.ledger)
    rows = data["findings"]
    if args.status:
        wanted = set(args.status)
        rows = [f for f in rows if f["status"] in wanted]
    if args.for_:
        pool = set(POOLS[args.for_])
        rows = [
            f
            for f in rows
            if f["status"] in pool
            and not (f.get("owner") and not is_stale(f, DEFAULT_STALE_MINUTES))
        ]
    if args.severity:
        rows = [f for f in rows if f["severity"] == args.severity]
    if args.area:
        rows = [f for f in rows if f["area"].startswith(args.area)]
    if args.owner:
        rows = [f for f in rows if f.get("owner") == args.owner]
    print(json.dumps(rows, indent=2))
    return 0


def cmd_next_scope(args: argparse.Namespace) -> int:
    """Print the area of the highest-priority claimable finding.

    Pending means any pool status with no fresh owner (`wontfix` and
    `verified-fixed` are terminal). `--exclude` skips areas already covered by
    active loops, matching either direction of the `area/` prefix.
    """
    data = load(args.ledger)
    exclude = [a.strip() for a in (args.exclude or "").split(",") if a.strip()]

    def covered(area: str) -> bool:
        return any(
            area == e or area.startswith(e + "/") or e.startswith(area + "/")
            for e in exclude
        )

    rows = [
        f
        for f in pool_rows(data, "loop")
        if not covered(str(f.get("area", "")))
    ]
    if not rows:
        print("null")
        return 3
    print(sort_rows(rows)[0]["area"])
    return 0


def cmd_summary(args: argparse.Namespace) -> int:
    data = load(args.ledger)
    by_status: dict[str, int] = {}
    by_severity: dict[str, int] = {}
    owned = 0
    for f in data["findings"]:
        by_status[f["status"]] = by_status.get(f["status"], 0) + 1
        by_severity[f["severity"]] = by_severity.get(f["severity"], 0) + 1
        if f.get("owner") and not is_stale(f, DEFAULT_STALE_MINUTES):
            owned += 1
    total = len(data["findings"])
    terminal = sum(by_status.get(s, 0) for s in TERMINAL)
    print(
        json.dumps(
            {
                "total": total,
                "terminal": terminal,
                "remaining": total - terminal,
                "owned": owned,
                "claimable": {
                    kind: len(pool_rows(data, kind)) for kind in CLAIM_KINDS
                },
                "by_status": by_status,
                "by_severity": by_severity,
                "updated": data.get("updated"),
            },
            indent=2,
        )
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--ledger",
        type=Path,
        default=None,
        help="ledger path (default: $PARITY_LEDGER or .opencode/evals/findings.json)",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    p_add = sub.add_parser("add", help="record a new finding")
    p_add.add_argument("--area", required=True, help="e.g. ui/menu, combat/turrets")
    p_add.add_argument("--severity", required=True, choices=SEVERITIES)
    p_add.add_argument("--title", required=True)
    p_add.add_argument("--expected", required=True)
    p_add.add_argument("--actual", required=True)
    p_add.add_argument("--repro", required=True)
    p_add.add_argument(
        "--source",
        choices=SOURCES,
        default="engine",
        help="evidence kind: engine (in-engine run) or code (file:line comparison)",
    )
    p_add.add_argument("--confidence", choices=CONFIDENCE, default=None)
    p_add.add_argument(
        "--status",
        choices=("open", "needs-evaluation"),
        default="open",
        help="initial queue: open (fixer) or needs-evaluation (in-engine triage)",
    )
    p_add.add_argument("--evidence", action="append", default=[])
    p_add.add_argument("--note", action="append", dest="notes", default=[])
    p_add.add_argument("--plan", default=None, help="owning plan, e.g. 14")
    p_add.set_defaults(func=cmd_add)

    p_verify = sub.add_parser("verify", help="engine verdict for an existing finding; drops the claim")
    p_verify.add_argument("--id", required=True)
    p_verify.add_argument("--status", required=True, choices=SETTABLE)
    p_verify.add_argument("--note")
    p_verify.add_argument("--evidence", action="append", default=[])
    p_verify.set_defaults(func=cmd_verify)

    p_set = sub.add_parser(
        "set-status", help="update status while keeping the claim (crash-safe handoff)"
    )
    p_set.add_argument("--id", required=True)
    p_set.add_argument("--status", required=True, choices=SETTABLE)
    p_set.add_argument("--note")
    p_set.set_defaults(func=cmd_set_status)

    p_claim = sub.add_parser(
        "claim", help="atomically claim the highest-priority claimable finding"
    )
    p_claim.add_argument(
        "--for",
        dest="for_",
        required=True,
        choices=CLAIM_KINDS,
        help="worker pool: fix, evaluation, or loop (fixer+evaluator)",
    )
    p_claim.add_argument(
        "--area",
        action="append",
        default=[],
        help="repeatable; OR prefix match (e.g. --area ui --area input)",
    )
    p_claim.add_argument(
        "--exclude",
        action="append",
        default=[],
        help="repeatable; finding ids to skip (e.g. items already attempted)",
    )
    p_claim.add_argument("--severity", choices=SEVERITIES)
    p_claim.add_argument(
        "--stale-minutes",
        type=int,
        default=DEFAULT_STALE_MINUTES,
        help="reclaim claims older than this (default: %(default)s)",
    )
    p_claim.add_argument("--owner", help="owner label (default: oc-<parent pid>)")
    p_claim.set_defaults(func=cmd_claim)

    p_release = sub.add_parser(
        "release", help="drop a claim; defaults to the pre-claim status"
    )
    p_release.add_argument("--id", required=True)
    p_release.add_argument("--status", choices=SETTABLE)
    p_release.add_argument("--note")
    p_release.set_defaults(func=cmd_release)

    p_reap = sub.add_parser("reap", help="release stale claims (dead sessions)")
    p_reap.add_argument(
        "--older-than-minutes",
        type=int,
        default=DEFAULT_STALE_MINUTES,
        help="claim age that makes a claim stale (default: %(default)s)",
    )
    p_reap.set_defaults(func=cmd_reap)

    p_areas = sub.add_parser("areas", help="claimable areas (JSON) for a worker pool")
    p_areas.add_argument("--for", dest="for_", required=True, choices=CLAIM_KINDS)
    p_areas.set_defaults(func=cmd_areas)

    p_migrate = sub.add_parser(
        "migrate", help="persist the legacy-status mapping from the scrapped workflow"
    )
    p_migrate.set_defaults(func=cmd_migrate)

    p_list = sub.add_parser("list", help="list findings as JSON")
    p_list.add_argument("--status", action="append", choices=STATUSES)
    p_list.add_argument("--for", dest="for_", choices=CLAIM_KINDS)
    p_list.add_argument("--severity", choices=SEVERITIES)
    p_list.add_argument("--area")
    p_list.add_argument("--owner")
    p_list.set_defaults(func=cmd_list)

    p_summary = sub.add_parser("summary", help="counts by status/severity/claimability")
    p_summary.set_defaults(func=cmd_summary)

    p_next = sub.add_parser(
        "next-scope",
        help="print the area of the highest-priority claimable finding; exit 3 when none",
    )
    p_next.add_argument(
        "--exclude",
        help="comma-separated areas already covered by active loops",
    )
    p_next.set_defaults(func=cmd_next_scope)

    args = parser.parse_args()
    args.ledger = resolve_ledger(args.ledger)
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
