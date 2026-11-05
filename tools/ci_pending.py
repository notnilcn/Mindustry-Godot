#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Lock-protected CI_PENDING.json for the dedicated CI fixer.

Dev agents do not run the full gate after every edit. They append one entry
describing what landed, then the CI fixer runs `tools/ci.sh` on the batch.
`tools/ci.sh` calls `begin-run` at the start of every run: it archives the
pending file to `runs/ci/<run-id>/pending.json` and resets the live file, then
`finish-run` records the failing stage and log path for the fixer.

Every mutation takes an exclusive `flock` on CI_PENDING.json.lock and replaces
the file atomically, so parallel agents never lose an entry.

See docs/CI-WORKFLOW.md.

Usage:
  python3 tools/ci_pending.py add "Add MindFoo dialog" --area client/rust/... \
      --files path/a.rs,path/b.rs --gates fmt,clippy,godot-import \
      --repro "cargo test -p mind-gdext foo" --expected "dialog sweep passes"
  python3 tools/ci_pending.py list [--status pending] [--json]
  python3 tools/ci_pending.py show CP-20261105-120115
  python3 tools/ci_pending.py check
  python3 tools/ci_pending.py clear
  python3 tools/ci_pending.py begin-run --run-id 20261105-120115-1234
  python3 tools/ci_pending.py finish-run --run-id 20261105-120115-1234 \
      --status failed --stage "cargo clippy" --exit-code 1 --log runs/ci/.../ci.log
"""

from __future__ import annotations

import argparse
import datetime as dt
import fcntl
import getpass
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_PENDING = REPO_ROOT / "CI_PENDING.json"
DEFAULT_RUNS_DIR = REPO_ROOT / "runs" / "ci"

SCHEMA_VERSION = 1
ALLOWED_STATUS = ("pending", "blocked", "resolved")
RUN_ID_RE = re.compile(r"^[A-Za-z0-9._-]+$")


class PendingError(Exception):
    """A user-facing error that must not destroy CI_PENDING.json."""


def now_utc() -> str:
    return dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def env_path(name: str, default: Path) -> Path:
    raw = os.environ.get(name)
    if not raw:
        return default
    path = Path(raw)
    return path if path.is_absolute() else REPO_ROOT / path


def pending_path(args: argparse.Namespace) -> Path:
    raw = getattr(args, "file", None)
    if raw:
        path = Path(raw)
        return path if path.is_absolute() else Path.cwd() / path
    return env_path("MIND_CI_PENDING_FILE", DEFAULT_PENDING)


def runs_dir(args: argparse.Namespace) -> Path:
    raw = getattr(args, "runs_dir", None) or os.environ.get("MIND_CI_RUNS_DIR")
    if not raw:
        return DEFAULT_RUNS_DIR
    path = Path(raw)
    return path if path.is_absolute() else REPO_ROOT / path


def relative(path: Path) -> str:
    try:
        return str(path.resolve().relative_to(REPO_ROOT))
    except ValueError:
        return str(path.resolve())


def fresh_doc() -> dict[str, Any]:
    return {
        "schema": SCHEMA_VERSION,
        "description": (
            "Batch of landed changes waiting for the next full CI run. Append with "
            "`python3 tools/ci_pending.py add ...`; tools/ci.sh archives this file to "
            "runs/ci/<run-id>/pending.json and resets it at the start of every run. "
            "See docs/CI-WORKFLOW.md."
        ),
        "policy": {
            "full_gate": (
                "tools/run-cpu-intensive-task.sh --ci, owned by the CI fixer; "
                "do not run the full gate after every edit"
            ),
            "record": (
                "one entry per landed change/batch: files, likely gates, "
                "reproduction command and expected result"
            ),
            "wipe": "archived and reset by tools/ci.sh at run start",
        },
        "updated": now_utc(),
        "entries": [],
    }


def git_out(*args: str) -> str:
    try:
        proc = subprocess.run(
            ["git", "-C", str(REPO_ROOT), *args],
            capture_output=True,
            text=True,
            check=False,
            timeout=15,
        )
    except (OSError, subprocess.TimeoutExpired):
        return ""
    return proc.stdout.strip() if proc.returncode == 0 else ""


def git_meta() -> dict[str, Any]:
    meta: dict[str, Any] = {}
    branch = git_out("rev-parse", "--abbrev-ref", "HEAD")
    if branch and branch != "HEAD":
        meta["branch"] = branch
    head = git_out("rev-parse", "--short", "HEAD")
    if head:
        meta["head"] = head
    worktree = git_out("rev-parse", "--show-toplevel")
    if worktree:
        meta["worktree"] = worktree
    status = git_out("status", "--porcelain")
    meta["dirty_files"] = len([line for line in status.splitlines() if line.strip()])
    return meta


def default_agent() -> str:
    for name in ("CI_PENDING_AGENT", "MIND_AGENT", "AGENT_NAME", "OPENCODE_AGENT"):
        value = os.environ.get(name)
        if value:
            return value
    try:
        return getpass.getuser()
    except Exception:
        return "unknown"


def validate(doc: Any) -> list[str]:
    errors: list[str] = []
    if not isinstance(doc, dict):
        return ["top level is not a JSON object"]
    if doc.get("schema") != SCHEMA_VERSION:
        errors.append(f"schema is {doc.get('schema')!r}, expected {SCHEMA_VERSION}")
    entries = doc.get("entries")
    if not isinstance(entries, list):
        return errors + ["entries is not a list"]
    seen: set[str] = set()
    for index, entry in enumerate(entries):
        label = f"entries[{index}]"
        if not isinstance(entry, dict):
            errors.append(f"{label} is not an object")
            continue
        entry_id = entry.get("id")
        if not isinstance(entry_id, str) or not entry_id.strip():
            errors.append(f"{label}.id is missing")
        elif entry_id in seen:
            errors.append(f"{label}.id {entry_id!r} is duplicated")
        else:
            seen.add(entry_id)
        summary = entry.get("summary")
        if not isinstance(summary, str) or not summary.strip():
            errors.append(f"{label}.summary is missing")
        status = entry.get("status")
        if status is not None and status not in ALLOWED_STATUS:
            errors.append(f"{label}.status {status!r} is not one of {ALLOWED_STATUS}")
        for key in ("files", "gates"):
            value = entry.get(key)
            if value is not None and not (
                isinstance(value, list) and all(isinstance(item, str) for item in value)
            ):
                errors.append(f"{label}.{key} must be a list of strings")
        if "created_at" in entry and not isinstance(entry["created_at"], str):
            errors.append(f"{label}.created_at must be a string")
    return errors


def lock_path(path: Path) -> Path:
    resolved = path.resolve()
    return resolved.with_name(resolved.name + ".lock")


def load_doc(path: Path) -> dict[str, Any]:
    if not path.exists():
        return fresh_doc()
    try:
        doc = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise PendingError(f"{path}: unreadable/invalid JSON ({exc}); refusing to overwrite it") from exc
    errors = validate(doc)
    if errors:
        raise PendingError(
            f"{path}: invalid CI_PENDING.json; refusing to overwrite it:\n  - "
            + "\n  - ".join(errors)
        )
    return doc


def write_json(path: Path, doc: Any) -> None:
    path = path.resolve()
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=str(path.parent), prefix=".ci-pending-", suffix=".json")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(json.dumps(doc, indent=2, ensure_ascii=False) + "\n")
        os.replace(tmp, path)
    except BaseException:
        try:
            os.unlink(tmp)
        except OSError:
            pass
        raise


def write_doc(path: Path, doc: dict[str, Any]) -> None:
    doc["updated"] = now_utc()
    write_json(path, doc)


class Lock:
    """Exclusive advisory lock around one CI_PENDING.json read-modify-write."""

    def __init__(self, path: Path):
        self.path = lock_path(path)
        self.handle = None

    def __enter__(self) -> "Lock":
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.path.touch(exist_ok=True)
        self.handle = self.path.open("r+")
        fcntl.flock(self.handle, fcntl.LOCK_EX)
        return self

    def __exit__(self, *_exc: object) -> bool:
        assert self.handle is not None
        fcntl.flock(self.handle, fcntl.LOCK_UN)
        self.handle.close()
        return False


def split_list(values: list[str] | None) -> list[str]:
    out: list[str] = []
    for value in values or []:
        for item in value.split(","):
            item = item.strip()
            if item and item not in out:
                out.append(item)
    return out


def new_id(doc: dict[str, Any]) -> str:
    base = "CP-" + dt.datetime.now(dt.timezone.utc).strftime("%Y%m%d-%H%M%S")
    taken = {entry.get("id") for entry in doc["entries"] if isinstance(entry, dict)}
    candidate = base
    suffix = 2
    while candidate in taken:
        candidate = f"{base}-{suffix}"
        suffix += 1
    return candidate


def parse_field(pair: str) -> tuple[str, Any]:
    if "=" not in pair:
        raise PendingError(f"--field needs KEY=VALUE, got {pair!r}")
    key, raw = pair.split("=", 1)
    key = key.strip()
    if not key:
        raise PendingError(f"--field needs a non-empty key, got {pair!r}")
    try:
        value: Any = json.loads(raw)
    except json.JSONDecodeError:
        value = raw
    return key, value


def cmd_init(args: argparse.Namespace) -> int:
    path = pending_path(args)
    with Lock(path):
        if path.exists() and not args.force:
            raise PendingError(f"{path} already exists (use --force to reset it)")
        write_doc(path, fresh_doc())
    print(f"ci-pending: initialized {relative(path)}")
    return 0


def cmd_add(args: argparse.Namespace) -> int:
    path = pending_path(args)
    summary = (args.summary or args.summary_flag or "").strip()
    if not summary:
        raise PendingError("add needs a summary (positional or --summary)")
    if args.status not in ALLOWED_STATUS:
        raise PendingError(f"--status must be one of {ALLOWED_STATUS}")

    with Lock(path):
        doc = load_doc(path)
        entry: dict[str, Any] = {
            "id": args.id or new_id(doc),
            "created_at": now_utc(),
            "agent": args.agent or default_agent(),
            **git_meta(),
            "area": args.area,
            "summary": summary,
            "files": split_list(args.files),
            "gates": split_list(args.gates),
            "repro": args.repro,
            "expected": args.expected,
            "risk": args.risk,
            "notes": args.notes,
            "status": args.status,
        }
        for pair in args.field:
            key, value = parse_field(pair)
            if key == "id":
                raise PendingError("--field id=... is not allowed; use --id")
            entry[key] = value
        entry = {key: value for key, value in entry.items() if value not in (None, "")}
        if "id" in entry and isinstance(entry["id"], str):
            entry["id"] = entry["id"].strip()
        if any(existing.get("id") == entry["id"] for existing in doc["entries"]):
            raise PendingError(f"entry id {entry['id']!r} already exists")
        doc["entries"].append(entry)
        write_doc(path, doc)
        count = len(doc["entries"])

    print(f"ci-pending: added {entry['id']} ({count} pending) -> {relative(path)}")
    if not entry.get("files") or not entry.get("gates"):
        print(
            "ci-pending: hint: add --files and --gates so the CI fixer knows what to check",
            file=sys.stderr,
        )
    return 0


def _filtered_entries(doc: dict[str, Any], status: str | None) -> list[dict[str, Any]]:
    entries = [entry for entry in doc["entries"] if isinstance(entry, dict)]
    if status:
        entries = [entry for entry in entries if entry.get("status") == status]
    return entries


def cmd_list(args: argparse.Namespace) -> int:
    path = pending_path(args)
    doc = load_doc(path)
    entries = _filtered_entries(doc, args.status)
    if args.json:
        print(json.dumps(entries, indent=2, ensure_ascii=False))
        return 0
    print(f"CI_PENDING.json: {len(entries)} entr{'y' if len(entries) == 1 else 'ies'} ({relative(path)})")
    for entry in entries:
        area = entry.get("area") or "-"
        summary = entry.get("summary") or "(no summary)"
        if len(summary) > 100:
            summary = summary[:97] + "..."
        print(f"  {entry.get('id', '?'):<24} {entry.get('status', '?'):<8} {area:<40} {summary}")
    return 0


def cmd_show(args: argparse.Namespace) -> int:
    doc = load_doc(pending_path(args))
    for entry in doc["entries"]:
        if isinstance(entry, dict) and entry.get("id") == args.id:
            print(json.dumps(entry, indent=2, ensure_ascii=False))
            return 0
    raise PendingError(f"unknown entry id {args.id!r}")


def cmd_remove(args: argparse.Namespace) -> int:
    path = pending_path(args)
    with Lock(path):
        doc = load_doc(path)
        kept = [entry for entry in doc["entries"] if entry.get("id") != args.id]
        if len(kept) == len(doc["entries"]):
            raise PendingError(f"unknown entry id {args.id!r}")
        doc["entries"] = kept
        write_doc(path, doc)
        count = len(kept)
    print(f"ci-pending: removed {args.id} ({count} left)")
    return 0


def cmd_clear(args: argparse.Namespace) -> int:
    path = pending_path(args)
    with Lock(path):
        doc = load_doc(path)
        count = len(doc["entries"])
        write_doc(path, fresh_doc())
    print(f"ci-pending: cleared {count} entr{'y' if count == 1 else 'ies'} from {relative(path)}")
    return 0


def cmd_check(args: argparse.Namespace) -> int:
    path = pending_path(args)
    if not path.exists():
        print(f"ci-pending: FAIL: {relative(path)} does not exist (run `ci_pending.py init`)", file=sys.stderr)
        return 1
    try:
        doc = load_doc(path)
    except PendingError as exc:
        print(f"ci-pending: FAIL: {exc}", file=sys.stderr)
        return 1
    print(
        f"ci-pending: ok: {len(doc['entries'])} entries, schema {doc.get('schema')}, {relative(path)}"
    )
    if not doc.get("description") or not doc.get("policy"):
        print("ci-pending: warning: description/policy metadata is missing", file=sys.stderr)
    return 0


def _run_dir(args: argparse.Namespace) -> Path:
    if not RUN_ID_RE.fullmatch(args.run_id):
        raise PendingError(f"--run-id {args.run_id!r} must match {RUN_ID_RE.pattern}")
    return runs_dir(args) / args.run_id


def _tail(path: Path, lines: int = 80) -> list[str]:
    if not path.exists():
        return []
    try:
        # The log can be large; read only the tail cheaply enough for a few MB.
        with path.open("r", encoding="utf-8", errors="replace") as handle:
            return [line.rstrip("\n") for line in handle.readlines()[-lines:]]
    except OSError:
        return []


def _run_meta(
    args: argparse.Namespace,
    run_dir: Path,
    archive: Path,
    log: Path,
    pending_count: int,
) -> dict[str, Any]:
    meta: dict[str, Any] = {
        "schema": SCHEMA_VERSION,
        "run_id": args.run_id,
        "status": "running",
        "started_at": now_utc(),
        "finished_at": None,
        "stage": None,
        "exit_code": None,
        "run_dir": relative(run_dir),
        "pending_count": pending_count,
        "pending_snapshot": relative(archive),
        "log": relative(log),
        **git_meta(),
        # begin-run is a short-lived child of the gate shell; its parent pid is
        # the ci.sh process the fixer may want to correlate with.
        "pid": os.getppid(),
        "host": os.uname().nodename if hasattr(os, "uname") else "unknown",
    }
    return meta


def _write_run(args: argparse.Namespace, run_dir: Path, meta: dict[str, Any]) -> None:
    write_json(run_dir / "run.json", meta)
    write_json(runs_dir(args) / "last-run.json", meta)


def cmd_begin_run(args: argparse.Namespace) -> int:
    path = pending_path(args)
    run_dir = _run_dir(args)
    run_json = run_dir / "run.json"
    if run_json.exists() and not args.force:
        raise PendingError(f"{relative(run_json)} already exists (use --force to overwrite)")

    archive = Path(args.archive) if args.archive else run_dir / "pending.json"
    if not archive.is_absolute():
        archive = REPO_ROOT / archive
    log = Path(args.log) if args.log else run_dir / "ci.log"
    if not log.is_absolute():
        log = REPO_ROOT / log

    with Lock(path):
        doc = load_doc(path)
        entries = [entry for entry in doc["entries"] if isinstance(entry, dict)]
        run_dir.mkdir(parents=True, exist_ok=True)
        archive.parent.mkdir(parents=True, exist_ok=True)
        write_json(archive, doc)
        meta = _run_meta(args, run_dir, archive, log, len(entries))
        _write_run(args, run_dir, meta)
        write_doc(path, fresh_doc())

    print(
        f"ci-pending: archived {len(entries)} entr{'y' if len(entries) == 1 else 'ies'} "
        f"-> {relative(archive)}; {relative(path)} reset"
    )
    return 0


def cmd_finish_run(args: argparse.Namespace) -> int:
    if args.status not in ("passed", "failed"):
        raise PendingError("--status must be 'passed' or 'failed'")
    run_dir = _run_dir(args)
    run_json = run_dir / "run.json"
    if not run_json.exists():
        raise PendingError(f"{relative(run_json)} does not exist; begin-run must run first")
    try:
        meta = json.loads(run_json.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise PendingError(f"{relative(run_json)} is unreadable: {exc}") from exc

    finished = now_utc()
    meta["status"] = args.status
    meta["finished_at"] = finished
    meta["stage"] = args.stage
    meta["exit_code"] = args.exit_code
    if args.log:
        log = Path(args.log)
        if not log.is_absolute():
            log = REPO_ROOT / log
        meta["log"] = relative(log)
        if args.status == "failed":
            meta["failure"] = {
                "stage": args.stage,
                "exit_code": args.exit_code,
                "log_tail": _tail(log),
            }
    if not (args.status == "failed" and args.log):
        meta.pop("failure", None)

    started = meta.get("started_at")
    if isinstance(started, str):
        try:
            delta = dt.datetime.now(dt.timezone.utc) - dt.datetime.strptime(
                started, "%Y-%m-%dT%H:%M:%SZ"
            ).replace(tzinfo=dt.timezone.utc)
            meta["duration_s"] = max(0, int(delta.total_seconds()))
        except ValueError:
            pass

    run_dir.mkdir(parents=True, exist_ok=True)
    _write_run(args, run_dir, meta)

    history = runs_dir(args) / "history.jsonl"
    history.parent.mkdir(parents=True, exist_ok=True)
    with history.open("a", encoding="utf-8") as handle:
        handle.write(
            json.dumps(
                {
                    "run_id": args.run_id,
                    "status": args.status,
                    "stage": args.stage,
                    "exit_code": args.exit_code,
                    "pending_count": meta.get("pending_count", 0),
                    "finished_at": finished,
                },
                ensure_ascii=False,
            )
            + "\n"
        )

    if args.status == "failed":
        print(
            f"ci-pending: run {args.run_id} FAILED at '{args.stage}' "
            f"(exit {args.exit_code}); see {relative(run_json)}"
        )
    else:
        print(f"ci-pending: run {args.run_id} passed")
    return 0


def build_parser() -> argparse.ArgumentParser:
    # Declared on the top-level parser *and* on every subcommand, so
    # `--file ... add` and `add --file ...` both work. SUPPRESS on the child
    # keeps a value given before the subcommand from being reset to None.
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument(
        "--file",
        default=argparse.SUPPRESS,
        help="CI_PENDING.json path (default: repo root; MIND_CI_PENDING_FILE overrides)",
    )
    common.add_argument(
        "--runs-dir",
        default=argparse.SUPPRESS,
        help="run record directory (default: runs/ci; MIND_CI_RUNS_DIR overrides)",
    )

    parser = argparse.ArgumentParser(
        description="Lock-protected CI_PENDING.json for the dedicated CI fixer.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="See docs/CI-WORKFLOW.md. Every mutation is flock-protected and atomic.",
    )
    parser.add_argument(
        "--file",
        default=None,
        help="CI_PENDING.json path (default: repo root; MIND_CI_PENDING_FILE overrides)",
    )
    parser.add_argument(
        "--runs-dir",
        default=None,
        help="run record directory (default: runs/ci; MIND_CI_RUNS_DIR overrides)",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    init = sub.add_parser("init", parents=[common], help="create an empty CI_PENDING.json")
    init.add_argument("--force", action="store_true", help="reset an existing file")
    init.set_defaults(func=cmd_init)

    add = sub.add_parser(
        "add", aliases=["append", "record"], parents=[common], help="append one pending entry"
    )
    add.add_argument("summary", nargs="?", help="what landed (short)")
    add.add_argument("--summary", dest="summary_flag", help="what landed (alternative to positional)")
    add.add_argument("--id", help="explicit entry id (default: CP-<UTC timestamp>)")
    add.add_argument("--agent", help="agent/session name (default: $CI_PENDING_AGENT or $USER)")
    add.add_argument("--area", help="subsystem or path, e.g. client/rust/mind-gdext/src/ui")
    add.add_argument(
        "--files", action="append", help="changed file(s); repeat or comma-separate"
    )
    add.add_argument(
        "--gates",
        action="append",
        help="likely CI stages: fmt,clippy,cargo-check,cargo-test,godot-import,ui-widgets-check,...",
    )
    add.add_argument("--repro", help="command that reproduces the local check")
    add.add_argument("--expected", help="what a passing result looks like")
    add.add_argument("--risk", help="what could break / where to look first")
    add.add_argument("--notes", help="anything else the fixer needs")
    add.add_argument(
        "--status",
        default="pending",
        choices=ALLOWED_STATUS,
        help="entry status (default: pending)",
    )
    add.add_argument(
        "--field",
        action="append",
        default=[],
        metavar="KEY=VALUE",
        help="extra JSON field (value parsed as JSON when possible); repeatable",
    )
    add.set_defaults(func=cmd_add)

    listing = sub.add_parser("list", parents=[common], help="list pending entries")
    listing.add_argument("--status", choices=ALLOWED_STATUS, help="filter by status")
    listing.add_argument("--json", action="store_true", help="print entries as JSON")
    listing.set_defaults(func=cmd_list)

    show = sub.add_parser("show", parents=[common], help="print one entry as JSON")
    show.add_argument("id")
    show.set_defaults(func=cmd_show)

    remove = sub.add_parser("remove", aliases=["rm"], parents=[common], help="remove one entry")
    remove.add_argument("id")
    remove.set_defaults(func=cmd_remove)

    clear = sub.add_parser("clear", parents=[common], help="reset the live file")
    clear.set_defaults(func=cmd_clear)

    check = sub.add_parser("check", parents=[common], help="validate the live file")
    check.set_defaults(func=cmd_check)

    begin = sub.add_parser(
        "begin-run", parents=[common], help="archive + reset; called by tools/ci.sh"
    )
    begin.add_argument("--run-id", required=True)
    begin.add_argument("--archive", help="snapshot path (default: runs/ci/<run-id>/pending.json)")
    begin.add_argument("--log", help="run log path (default: runs/ci/<run-id>/ci.log)")
    begin.add_argument("--force", action="store_true", help="overwrite an existing run dir")
    begin.set_defaults(func=cmd_begin_run)

    finish = sub.add_parser(
        "finish-run", parents=[common], help="record the result; called by tools/ci.sh"
    )
    finish.add_argument("--run-id", required=True)
    finish.add_argument("--status", required=True, choices=("passed", "failed"))
    finish.add_argument("--stage", default="unknown")
    finish.add_argument("--exit-code", type=int, default=0)
    finish.add_argument("--log", help="run log path")
    finish.set_defaults(func=cmd_finish_run)

    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        return args.func(args)
    except PendingError as exc:
        print(f"ci_pending.py: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
