#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Single-writer helper for the parity-eval findings ledger.

Lifecycle:

    open -> godot-open -> godot-unverified -> godot-pass -> twin-unverified -> twin-verified
                     ^          | fail                       | fail
                     +----------+                            +-> open

The gap identifier seeds code-sourced candidates with `add --source code`
(status `open`), each carrying a Markdown fix sketch in the `plan` field.
A loop session claims a batch for its writer with
`claim --for writer --session loop-N` and works the items one at a time; the
parity evaluator claims one item (`claim --for godot-eval --id EV-####`) before
running the Godot leg; the session's twin evaluator claims one `godot-pass`
item (`claim --for twin --id EV-####`) for the Java-vs-Godot twin run.

Every claim is a cross-session lock: a finding with a fresh `owner` is
invisible to pickers until the owner releases it or the claim goes stale and
`reap` restores it. Findings carry a `session` label set at first claim;
`writer` retry picks, `godot-eval` picks and `twin` picks are restricted to
that session, so a loop never verifies another loop's worktree. Parallel
sessions share one ledger: `PARITY_LEDGER` overrides the default path, and
every mutating command holds `<ledger>.lock` for the whole read-modify-write.

Examples:
    record_finding.py add --area ui/menu --severity S2 \
        --title "Campaign button does not open the planet view" \
        --expected "Java: Play > Campaign opens the Serpulo planet view" \
        --actual "Godot: click leaves the menu unchanged" \
        --repro "scenario boot_menu step 4" \
        --plan "Seam: menu button emits no signal. Steps: bind the Campaign \
button to the planet dialog. Check: headless campaign_launch step 1." \
        --evidence runs/20261005-120000-boot_menu/godot/step-04.png
    record_finding.py set-plan --id EV-0001 --plan "<Markdown fix sketch>"
    record_finding.py claim --for writer --session loop-2 --count 3
    record_finding.py claim --for godot-eval --id EV-0001 --session loop-2
    record_finding.py release --id EV-0001 --status godot-pass --note "repro passes"
    record_finding.py verify --id EV-0001 --status twin-verified \
        --note "twin run at 0706963" --evidence runs/.../step-04.png
    record_finding.py reap --older-than-minutes 90
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
FORMAT = 2
# Phase statuses. A finding's status is its pipeline phase; the claim fields
# (`owner`, `claimed_at`, `claimed_for`) only mark the actor currently working
# the phase. `godot-pass` items are session-local: they wait for that loop's
# twin evaluator, not for any other loop.
STATUSES = (
    "open",
    "godot-open",
    "godot-unverified",
    "godot-pass",
    "twin-unverified",
    "twin-verified",
    "wontfix",
)
# What release/set-status/verify may assign; anything in STATUSES is legal.
SETTABLE = STATUSES
# Statuses a finding is finished with for the whole workflow.
TERMINAL = ("twin-verified", "wontfix")
CLAIM_KINDS = ("writer", "godot-eval", "twin")
# Status each claim kind writes while its actor holds the claim.
CLAIM_STATUS = {
    "writer": "godot-open",
    "godot-eval": "godot-unverified",
    "twin": "twin-unverified",
}
# Which statuses each kind may claim. `godot-open` is shared: a released item
# awaits the parity evaluator, a fresh owner means a writer is on it.
POOLS = {
    "writer": ("open", "godot-open"),
    "godot-eval": ("godot-open", "godot-unverified"),
    "twin": ("godot-pass", "twin-unverified"),
}
# Statuses from the pre-two-stage workflow, mapped on load so old ledgers keep
# working; `migrate` persists the mapping and clears the old claims.
LEGACY = {
    "in-progress": "godot-open",
    "code-verified": "godot-open",
    "needs-evaluation": "godot-open",
    "verified-fixed": "twin-verified",
    "verified-unfixed": "open",
    "regression": "open",
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


def resolve_session(value: str | None) -> str:
    """CLI --session wins, then PARITY_SESSION, then loop id, then `main`."""
    if value:
        return value
    env = os.environ.get("PARITY_SESSION")
    if env:
        return env
    loop = os.environ.get("PARITY_LOOP")
    return f"loop-{loop}" if loop else "main"


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
        return {"format": FORMAT, "updated": None, "findings": []}
    data = json.loads(path.read_text(encoding="utf-8"))
    data.setdefault("format", 1)
    data.setdefault("findings", [])
    return data


def map_status(status: str | None) -> str | None:
    return LEGACY.get(status, status) if status else status


def normalize(data: dict, annotate: bool = False, clear_claims: bool = False) -> int:
    """Map legacy statuses/prev_status onto the current set.

    With `clear_claims`, legacy claim fields are dropped (their kind labels no
    longer exist) and a note records the owner that was cleared.
    """
    changed = 0
    for finding in data["findings"]:
        status = finding.get("status")
        if status == "claimed":
            target = map_status(finding.get("prev_status")) or "godot-open"
            finding["status"] = target
            changed += 1
            if annotate:
                note(finding, f"migrated status claimed -> {target}")
        elif status in LEGACY:
            finding["status"] = LEGACY[status]
            changed += 1
            if annotate:
                note(finding, f"migrated status {status} -> {LEGACY[status]}")
        if finding.get("prev_status") in LEGACY:
            finding["prev_status"] = LEGACY[finding["prev_status"]]
        if clear_claims and any(
            finding.get(key)
            for key in ("owner", "claimed_at", "claimed_for", "prev_status")
        ):
            owner = finding.pop("owner", None)
            finding.pop("claimed_at", None)
            finding.pop("claimed_for", None)
            finding.pop("prev_status", None)
            changed += 1
            if annotate:
                note(finding, f"migrated: cleared legacy claim ({owner or 'unknown'})")
    return changed


def load(path: Path) -> dict:
    data = load_raw(path)
    normalize(data)
    return data


def save(path: Path, data: dict) -> None:
    data["format"] = FORMAT
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
    """Drop the actor lock; `session` is provenance and stays."""
    for key in ("owner", "claimed_at", "claimed_for", "prev_status"):
        finding.pop(key, None)


def apply_status(finding: dict, status: str) -> None:
    """Set a phase status; a globally redoable item drops its session."""
    finding["status"] = status
    if status == "open":
        finding.pop("session", None)


def session_matches(finding: dict, session: str | None) -> bool:
    """Unset sessions are adoptable; set sessions only by the same label."""
    owner = finding.get("session")
    if not owner:
        return True
    return session is not None and owner == session


def claimable(finding: dict, kind: str, session: str | None, stale_minutes: int) -> bool:
    if finding.get("status") not in POOLS[kind]:
        return False
    if finding.get("owner") and not is_stale(finding, stale_minutes):
        return False
    return session_matches(finding, session)


def pool_rows(
    data: dict,
    kind: str,
    *,
    session: str | None = None,
    severity: str | None = None,
    areas: list[str] | None = None,
    exclude: set[str] | None = None,
    stale_minutes: int = DEFAULT_STALE_MINUTES,
) -> list[dict]:
    """Claimable rows for `kind`, with an optional session restriction."""
    rows = []
    for f in data["findings"]:
        if not claimable(f, kind, session, stale_minutes):
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


def stamp_claim(finding: dict, kind: str, session: str, owner: str) -> None:
    if finding.get("owner"):
        note(finding, f"reclaimed stale claim from {finding['owner']}")
    finding["session"] = session
    finding["prev_status"] = finding["status"]
    finding["status"] = CLAIM_STATUS[kind]
    finding["owner"] = owner
    finding["claimed_at"] = now()
    finding["claimed_for"] = kind
    finding["last_verified"] = now()
    note(finding, f"claimed by {owner} for {kind} (session {finding['session']})")


def claim_payload(finding: dict) -> dict:
    return {
        "id": finding["id"],
        "severity": finding["severity"],
        "area": finding["area"],
        "title": finding["title"],
        "expected": finding.get("expected"),
        "actual": finding.get("actual"),
        "repro": finding.get("repro"),
        "plan": finding.get("plan"),
        "evidence": finding.get("evidence", []),
        "notes": finding.get("notes", [])[-3:],
        "prev_status": finding.get("prev_status"),
        "owner": finding.get("owner"),
        "claimed_for": finding.get("claimed_for"),
        "session": finding.get("session"),
    }


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
    """Record a verdict and drop the claim (twin evaluator path)."""
    with ledger_lock(args.ledger):
        data = load(args.ledger)
        finding = find(data, args.id)
        previous = finding["status"]
        apply_status(finding, args.status)
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
    """Update a status without dropping the claim (crash-safe handoff)."""
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


def cmd_set_plan(args: argparse.Namespace) -> int:
    """Replace a finding's fix plan (Markdown text)."""
    with ledger_lock(args.ledger):
        data = load(args.ledger)
        finding = find(data, args.id)
        finding["plan"] = args.plan
        if args.note:
            note(finding, f"[plan] {args.note}")
        save(args.ledger, data)
    print(f"{finding['id']}: plan updated")
    return 0


def cmd_claim(args: argparse.Namespace) -> int:
    """Atomically claim up to `--count` findings for a worker kind."""
    session = resolve_session(args.session)
    with ledger_lock(args.ledger):
        data = load(args.ledger)
        if args.id:
            rows = [find(data, args.id)]
            if not claimable(rows[0], args.for_, session, args.stale_minutes):
                print("null")
                return 3
        else:
            rows = pool_rows(
                data,
                args.for_,
                session=session,
                severity=args.severity,
                areas=args.area or None,
                exclude=set(args.exclude or []),
                stale_minutes=args.stale_minutes,
            )
            rows = sort_rows(rows)[: args.count]
        if not rows:
            print("null")
            return 3
        for finding in rows:
            stamp_claim(finding, args.for_, session, args.owner or f"oc-{os.getppid()}")
        save(args.ledger, data)
        payload = [claim_payload(f) for f in rows]
    print(json.dumps(payload[0] if len(payload) == 1 else payload, indent=2))
    return 0


def cmd_release(args: argparse.Namespace) -> int:
    """Drop a claim and restore the pre-claim (or named) status."""
    with ledger_lock(args.ledger):
        data = load(args.ledger)
        finding = find(data, args.id)
        previous = finding["status"]
        target = args.status or finding.get("prev_status") or "open"
        apply_status(finding, target)
        clear_claim(finding)
        finding["last_verified"] = now()
        if args.note:
            note(finding, f"released: {args.note}")
        if args.evidence:
            finding.setdefault("evidence", []).extend(args.evidence)
        save(args.ledger, data)
    print(f"{finding['id']}: {previous} -> {target}")
    return 0


def cmd_reap(args: argparse.Namespace) -> int:
    """Release stale claims back to their pre-claim status.

    `--session` restricts to one loop's claims; `--older-than-minutes 0` is the
    startup recovery sweep for a single session (no other actor can hold that
    session's claims while it is the only session running it).
    """
    with ledger_lock(args.ledger):
        data = load(args.ledger)
        reaped = []
        for finding in data["findings"]:
            if not finding.get("owner"):
                continue
            if args.session and finding.get("session") != args.session:
                continue
            if not is_stale(finding, args.older_than_minutes):
                continue
            owner = finding["owner"]
            previous = finding["status"]
            target = finding.get("prev_status") or "godot-open"
            apply_status(finding, target)
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
    rows = pool_rows(data, args.for_, session=resolve_session(args.session))
    counts: dict[str, int] = {}
    for finding in rows:
        area = str(finding.get("area", ""))
        counts[area] = counts.get(area, 0) + 1
    print(
        json.dumps(
            {
                "for": args.for_,
                "session": resolve_session(args.session),
                "total": len(rows),
                "areas": dict(sorted(counts.items())),
            },
            indent=2,
        )
    )
    return 0


def cmd_migrate(args: argparse.Namespace) -> int:
    """Persist the legacy-status mapping and clear legacy claims."""
    with ledger_lock(args.ledger):
        data = load_raw(args.ledger)
        changed = normalize(data, annotate=True, clear_claims=True)
        data["format"] = FORMAT
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
        session = resolve_session(args.session)
        rows = [f for f in rows if claimable(f, args.for_, session, DEFAULT_STALE_MINUTES)]
    if args.session:
        rows = [f for f in rows if f.get("session") == args.session]
    if args.severity:
        rows = [f for f in rows if f["severity"] == args.severity]
    if args.area:
        rows = [f for f in rows if f["area"].startswith(args.area)]
    if args.owner:
        rows = [f for f in rows if f.get("owner") == args.owner]
    print(json.dumps(rows, indent=2))
    return 0


def cmd_summary(args: argparse.Namespace) -> int:
    data = load(args.ledger)
    by_status: dict[str, int] = {}
    by_severity: dict[str, int] = {}
    pending_by_session: dict[str, int] = {}
    owned = 0
    for f in data["findings"]:
        by_status[f["status"]] = by_status.get(f["status"], 0) + 1
        by_severity[f["severity"]] = by_severity.get(f["severity"], 0) + 1
        if f.get("owner") and not is_stale(f, DEFAULT_STALE_MINUTES):
            owned += 1
        if f["status"] in ("godot-open", "godot-unverified", "godot-pass", "twin-unverified"):
            session = f.get("session") or "-"
            pending_by_session[session] = pending_by_session.get(session, 0) + 1
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
                    kind: len(pool_rows(data, kind, session=None))
                    for kind in CLAIM_KINDS
                },
                "by_status": by_status,
                "by_severity": by_severity,
                "pending_by_session": dict(sorted(pending_by_session.items())),
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
        default="code",
        help="evidence kind: code (file:line comparison) or engine (in-engine run)",
    )
    p_add.add_argument("--confidence", choices=CONFIDENCE, default=None)
    p_add.add_argument(
        "--status",
        choices=("open",),
        default="open",
        help="new findings start in the writer queue",
    )
    p_add.add_argument("--evidence", action="append", default=[])
    p_add.add_argument("--note", action="append", dest="notes", default=[])
    p_add.add_argument(
        "--plan",
        default=None,
        help="fix plan text (Markdown: seam, files, steps, check)",
    )
    p_add.set_defaults(func=cmd_add)

    p_verify = sub.add_parser(
        "verify", help="final verdict for an existing finding; drops the claim"
    )
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

    p_plan = sub.add_parser(
        "set-plan", help="replace a finding's fix plan without touching its claim"
    )
    p_plan.add_argument("--id", required=True)
    p_plan.add_argument("--plan", required=True, help="fix plan text (Markdown)")
    p_plan.add_argument("--note")
    p_plan.set_defaults(func=cmd_set_plan)

    p_claim = sub.add_parser(
        "claim", help="atomically claim claimable findings for a worker kind"
    )
    p_claim.add_argument(
        "--for",
        dest="for_",
        required=True,
        choices=CLAIM_KINDS,
        help="worker kind: writer, godot-eval, or twin",
    )
    p_claim.add_argument("--id", help="claim this exact finding (e.g. EV-0001)")
    p_claim.add_argument(
        "--count",
        type=int,
        default=1,
        help="claim up to this many findings (default: 1)",
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
        "--session",
        help="loop/session label (default: $PARITY_SESSION or loop-$PARITY_LOOP)",
    )
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
    p_release.add_argument("--evidence", action="append", default=[])
    p_release.set_defaults(func=cmd_release)

    p_reap = sub.add_parser("reap", help="release stale claims (dead sessions)")
    p_reap.add_argument(
        "--older-than-minutes",
        type=int,
        default=DEFAULT_STALE_MINUTES,
        help="claim age that makes a claim stale (default: %(default)s)",
    )
    p_reap.add_argument("--session", help="restrict to one session's claims")
    p_reap.set_defaults(func=cmd_reap)

    p_areas = sub.add_parser("areas", help="claimable areas (JSON) for a worker kind")
    p_areas.add_argument("--for", dest="for_", required=True, choices=CLAIM_KINDS)
    p_areas.add_argument("--session")
    p_areas.set_defaults(func=cmd_areas)

    p_migrate = sub.add_parser(
        "migrate", help="persist the legacy-status mapping and clear legacy claims"
    )
    p_migrate.set_defaults(func=cmd_migrate)

    p_list = sub.add_parser("list", help="list findings as JSON")
    p_list.add_argument("--status", action="append", choices=STATUSES)
    p_list.add_argument("--for", dest="for_", choices=CLAIM_KINDS)
    p_list.add_argument("--session")
    p_list.add_argument("--severity", choices=SEVERITIES)
    p_list.add_argument("--area")
    p_list.add_argument("--owner")
    p_list.set_defaults(func=cmd_list)

    p_summary = sub.add_parser("summary", help="counts by status/severity/claimability")
    p_summary.set_defaults(func=cmd_summary)

    args = parser.parse_args()
    args.ledger = resolve_ledger(args.ledger)
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
