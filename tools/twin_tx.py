#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Atomic, lock-protected edits to TWIN-VERIFICATION.json.

Several agents share one queue file (`TWIN-VERIFICATION.json`, symlinked into
each session worktree). Plain read-modify-write with jq loses concurrent
updates, so every mutation here takes an exclusive `flock` on
`TWIN-VERIFICATION.json.lock` and replaces the file atomically. Reads are
lock-free.

The serialisation is byte-stable with the checked-in file: `json.dumps(...,
indent=2, ensure_ascii=False)` plus a trailing newline reproduces it exactly,
so an edit only changes the touched item.

Commands
--------

    twin_tx.py list [--status S]... [--nonterminal] [--owner A] [--tsv]
    twin_tx.py stats
    twin_tx.py show ID
    twin_tx.py next-id
    twin_tx.py claim ID --agent A [--force] [--expect STATUS]
    twin_tx.py release ID [--agent A]
    twin_tx.py set ID --status S [--similarity N] [--owner A]
                            [--field KEY=VALUE]... [--unset KEY]...
    twin_tx.py add --agent A --title T [--id PAR-####] [--status S]
                   [--similarity N] [--field KEY=VALUE]...
    twin_tx.py note ID --agent A --text TEXT

Exit codes: 0 ok, 2 usage/file error, 3 refused claim (owned or terminal).

Claiming: a claim sets `status` to `fixing` and stamps `claimed_by` /
`claimed_at` on the item (extra keys are allowed by the validator). A claim is
refused when the item is already terminal (`reference-verified`,
`twin-verified`) or already claimed by another agent, unless `--force` is
given. `--expect STATUS` additionally requires the pre-claim status. Stale
claims can be reclaimed with `--force`; `release` drops the claim keys.

This file is tooling, not game code: it follows the `tools/` Python style
(SPDX header, argparse, no third-party imports).
"""

from __future__ import annotations

import argparse
import fcntl
import json
import os
import re
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
QUEUE = ROOT / "TWIN-VERIFICATION.json"
LOCK = ROOT / "TWIN-VERIFICATION.json.lock"
TERMINAL = {"reference-verified", "twin-verified"}
STATUSES = TERMINAL | {
    "seeded",
    "reference",
    "in-progress",
    "mismatch",
    "fixing",
    "verified",
    "blocked",
    "wontfix",
}


def now_utc() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def load() -> dict:
    with QUEUE.open(encoding="utf-8") as fh:
        return json.load(fh)


def dump(doc: dict) -> None:
    """Write atomically; byte-stable for untouched parts."""
    doc["updated"] = now_utc()
    fd, tmp = tempfile.mkstemp(dir=str(QUEUE.parent), prefix=".twin-", suffix=".json")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            fh.write(json.dumps(doc, indent=2, ensure_ascii=False) + "\n")
        os.replace(tmp, QUEUE)
    except BaseException:
        try:
            os.unlink(tmp)
        except OSError:
            pass
        raise


class Tx:
    """Read-modify-write under an exclusive lock."""

    def __enter__(self):
        LOCK.touch(exist_ok=True)
        self.fh = LOCK.open("r+")
        fcntl.flock(self.fh, fcntl.LOCK_EX)
        return self

    def __exit__(self, *exc):
        fcntl.flock(self.fh, fcntl.LOCK_UN)
        self.fh.close()
        return False


def coalesce(doc: dict, iid: str) -> dict:
    item = doc["items"].get(iid)
    if item is None:
        sys.exit(f"{iid}: unknown item")
    return item


def parse_fields(pairs: list[str] | None) -> dict:
    out = {}
    for pair in pairs or []:
        if "=" not in pair:
            sys.exit(f"--field needs KEY=VALUE, got {pair!r}")
        key, value = pair.split("=", 1)
        try:
            out[key] = json.loads(value)
        except json.JSONDecodeError:
            out[key] = value
    return out


def cmd_list(args) -> int:
    doc = load()
    rows = []
    for iid, item in doc["items"].items():
        status = item.get("status")
        if args.status and status not in args.status:
            continue
        if args.nonterminal and status in TERMINAL:
            continue
        owner = item.get("claimed_by")
        if args.owner and owner != args.owner:
            continue
        rows.append((iid, status, owner or "-", item.get("title", "")))
    rows.sort()
    for iid, status, owner, title in rows:
        if args.tsv:
            print(f"{iid}\t{status}\t{owner}\t{title}")
        else:
            print(f"{iid:9s} {status:18s} {owner:14s} {title}")
    if not args.tsv:
        print(f"-- {len(rows)} item(s)")
    return 0


def cmd_stats(args) -> int:
    doc = load()
    counts: dict[str, int] = {}
    owners: dict[str, int] = {}
    for item in doc["items"].values():
        counts[item.get("status", "?")] = counts.get(item.get("status", "?"), 0) + 1
        owner = item.get("claimed_by")
        if owner:
            owners[owner] = owners.get(owner, 0) + 1
    for status in sorted(counts):
        print(f"{status:18s} {counts[status]}")
    print(f"total items         {len(doc['items'])}")
    for owner, count in sorted(owners.items()):
        print(f"claimed by {owner}: {count}")
    return 0


def cmd_show(args) -> int:
    doc = load()
    print(json.dumps(coalesce(doc, args.id), indent=2, ensure_ascii=False))
    return 0


def cmd_next_id(args) -> int:
    doc = load()
    highest = 0
    for iid in doc["items"]:
        match = re.fullmatch(r"PAR-(\d{4})", iid)
        if match:
            highest = max(highest, int(match.group(1)))
    print(f"PAR-{highest + 1:04d}")
    return 0


def cmd_claim(args) -> int:
    with Tx():
        doc = load()
        item = coalesce(doc, args.id)
        status = item.get("status")
        owner = item.get("claimed_by")
        if not args.force:
            if status in TERMINAL:
                print(f"{args.id}: already terminal ({status})", file=sys.stderr)
                return 3
            if owner and owner != args.agent:
                print(f"{args.id}: claimed by {owner} ({status})", file=sys.stderr)
                return 3
            if owner == args.agent and status == "fixing":
                print(f"{args.id}: already claimed by {args.agent}", file=sys.stderr)
                return 3
            if args.expect and status != args.expect:
                print(f"{args.id}: expected {args.expect}, found {status}", file=sys.stderr)
                return 3
        item["status"] = "fixing"
        item["claimed_by"] = args.agent
        item["claimed_at"] = now_utc()
        dump(doc)
    print(f"claimed {args.id} for {args.agent}")
    return 0


def cmd_release(args) -> int:
    with Tx():
        doc = load()
        item = coalesce(doc, args.id)
        owner = item.get("claimed_by")
        if args.agent and owner and owner != args.agent:
            print(f"{args.id}: claimed by {owner}, not {args.agent}", file=sys.stderr)
            return 3
        item.pop("claimed_by", None)
        item.pop("claimed_at", None)
        dump(doc)
    print(f"released {args.id}")
    return 0


def cmd_set(args) -> int:
    if args.status and args.status not in STATUSES:
        sys.exit(f"bad status {args.status!r}")
    with Tx():
        doc = load()
        item = coalesce(doc, args.id)
        if args.status:
            item["status"] = args.status
        if args.similarity is not None:
            if not 0 <= args.similarity <= 100:
                sys.exit("similarity must be 0-100")
            item["similarity"] = args.similarity
        if args.owner:
            item["claimed_by"] = args.owner
            item["claimed_at"] = now_utc()
        for key, value in parse_fields(args.field).items():
            item[key] = value
        for key in args.unset or []:
            item.pop(key, None)
        if args.status in TERMINAL:
            item.pop("claimed_by", None)
            item.pop("claimed_at", None)
        dump(doc)
    print(f"updated {args.id}")
    return 0


def cmd_add(args) -> int:
    with Tx():
        doc = load()
        iid = args.id
        if not iid:
            highest = 0
            for existing in doc["items"]:
                match = re.fullmatch(r"PAR-(\d{4})", existing)
                if match:
                    highest = max(highest, int(match.group(1)))
            iid = f"PAR-{highest + 1:04d}"
        if not re.fullmatch(r"PAR-\d{4}", iid):
            sys.exit(f"bad item id {iid!r}")
        if iid in doc["items"]:
            sys.exit(f"{iid}: already exists")
        item = {
            "title": args.title,
            "status": args.status,
            "fork": {"java": [{"tool": "note", "params": {"text": "see referenced Java source"}}]},
            "action": {"godot": [{"tool": "note", "params": {"text": "see runs/" + iid}}]},
        }
        if args.similarity is not None:
            item["similarity"] = args.similarity
        item["claimed_by"] = args.agent
        item["claimed_at"] = now_utc()
        item.update(parse_fields(args.field))
        doc["items"][iid] = item
        dump(doc)
    print(f"added {iid}")
    return 0


def cmd_note(args) -> int:
    with Tx():
        doc = load()
        item = coalesce(doc, args.id)
        notes = item.setdefault("notes", [])
        notes.append({"at": now_utc(), "by": args.agent, "text": args.text})
        dump(doc)
    print(f"noted {args.id}")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(
        prog="twin_tx.py",
        description="Lock-protected edits to the shared TWIN-VERIFICATION.json queue.",
    )
    sub = parser.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("list", help="list items")
    p.add_argument("--status", action="append", help="filter by status (repeatable)")
    p.add_argument("--nonterminal", action="store_true", help="hide terminal items")
    p.add_argument("--owner", help="filter by claimed_by")
    p.add_argument("--tsv", action="store_true", help="tab-separated rows")
    p.set_defaults(func=cmd_list)

    p = sub.add_parser("stats", help="status counts")
    p.set_defaults(func=cmd_stats)

    p = sub.add_parser("show", help="print one item as JSON")
    p.add_argument("id")
    p.set_defaults(func=cmd_show)

    p = sub.add_parser("next-id", help="print the next free PAR id")
    p.set_defaults(func=cmd_next_id)

    p = sub.add_parser("claim", help="claim an item (status -> fixing)")
    p.add_argument("id")
    p.add_argument("--agent", required=True)
    p.add_argument("--expect", help="require this pre-claim status")
    p.add_argument("--force", action="store_true", help="steal a claim / override a status")
    p.set_defaults(func=cmd_claim)

    p = sub.add_parser("release", help="drop the claim keys")
    p.add_argument("id")
    p.add_argument("--agent")
    p.set_defaults(func=cmd_release)

    p = sub.add_parser("set", help="update one item")
    p.add_argument("id")
    p.add_argument("--status")
    p.add_argument("--similarity", type=float)
    p.add_argument("--owner")
    p.add_argument("--field", action="append", metavar="KEY=VALUE")
    p.add_argument("--unset", action="append", metavar="KEY")
    p.set_defaults(func=cmd_set)

    p = sub.add_parser("add", help="record a new gap item")
    p.add_argument("--agent", required=True)
    p.add_argument("--title", required=True)
    p.add_argument("--id")
    p.add_argument("--status", default="seeded")
    p.add_argument("--similarity", type=float)
    p.add_argument("--field", action="append", metavar="KEY=VALUE")
    p.set_defaults(func=cmd_add)

    p = sub.add_parser("note", help="append a timestamped note")
    p.add_argument("id")
    p.add_argument("--agent", required=True)
    p.add_argument("--text", required=True)
    p.set_defaults(func=cmd_note)

    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
