#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Cross-process lease guard for MCP consumers on a shared host.

MCP servers that drive a display, an editor, or a game client are expensive.
An agent that is about to use MCP acquires a lease first; `MCP_SLOT_LIMIT`
(default 2) is the maximum number of live leases, so a request is refused while
the limit is reached. Refusal exits 3 and the caller stays off MCP — for
code-only agents that is the normal, cheaper path, and the two slots leave
room for an evaluator to run its Java and Godot legs.

    mcp_slot.py acquire --owner gap-campaign
    mcp_slot.py refresh --token <token>
    mcp_slot.py release --token <token>   # exact lease
    mcp_slot.py release                   # by $MCP_SLOT_OWNER_KEY (guard shells)
    mcp_slot.py status
    mcp_slot.py gc

Every command prints one JSON object. A lease is live while its heartbeat is
fresh (`MCP_SLOT_TTL`, default 900 s), so callers are usually short-lived tool
shells and crashed processes cannot leak slots past the TTL. The registry
lives outside any single worktree (`--dir`, then
`$MCP_SLOT_DIR`, then `$PARITY_EVALS_DIR/mcp-slots`, else the git common dir's
`.opencode/evals/mcp-slots`) and every mutation is serialized by an flock on
`slots.lock`, mirroring `record_finding.py`.
"""

from __future__ import annotations

import argparse
import json
import os
import secrets
import socket
import subprocess
import sys
import time
from contextlib import contextmanager
from datetime import datetime, timezone
from pathlib import Path

try:
    import fcntl
except ImportError:  # pragma: no cover - non-POSIX fallback
    fcntl = None

DEFAULT_LIMIT = 2
DEFAULT_TTL = 900
HOST = socket.gethostname()
EXIT_LIMIT = 3
EXIT_NOT_FOUND = 4


def now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def age_seconds(stamp: str) -> float:
    try:
        then = datetime.fromisoformat(stamp)
    except (TypeError, ValueError):
        return float("inf")
    return (datetime.now(timezone.utc) - then).total_seconds()


def resolve_state_dir(cli_dir: Path | None) -> Path:
    """CLI --dir wins, then env, then the shared git common dir."""
    if cli_dir is not None:
        return cli_dir
    env = os.environ.get("MCP_SLOT_DIR")
    if env:
        return Path(env)
    evals = os.environ.get("PARITY_EVALS_DIR")
    if evals:
        return Path(evals) / "mcp-slots"
    try:
        out = subprocess.run(
            ["git", "rev-parse", "--path-format=absolute", "--git-common-dir"],
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
        common = Path(out)
        if common.name == ".git":
            return common.parent / ".opencode" / "evals" / "mcp-slots"
        return common / ".opencode" / "evals" / "mcp-slots"
    except (OSError, subprocess.SubprocessError):
        return Path(".opencode/evals/mcp-slots")


@contextmanager
def registry_lock(state_dir: Path):
    state_dir.mkdir(parents=True, exist_ok=True)
    if fcntl is None:
        yield
        return
    with open(state_dir / "slots.lock", "w", encoding="utf-8") as handle:
        fcntl.flock(handle.fileno(), fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(handle.fileno(), fcntl.LOCK_UN)


def leases_dir(state_dir: Path) -> Path:
    return state_dir / "leases"


def load_leases(state_dir: Path) -> list[dict]:
    out: list[dict] = []
    for path in sorted(leases_dir(state_dir).glob("*.json")):
        try:
            out.append(json.loads(path.read_text(encoding="utf-8")))
        except (OSError, json.JSONDecodeError):
            continue
    return out


def lease_path(state_dir: Path, token: str) -> Path:
    return leases_dir(state_dir) / f"{token}.json"


def is_live(lease: dict, ttl: int) -> bool:
    """A lease is live while its heartbeat is fresh.

    Callers are often short-lived shell processes (the CLI convention) or a
    long-lived opencode process (the plugin guard), so liveness is heartbeat
    based; `pid` is metadata for status output, and the TTL bounds a crashed
    holder's leak.
    """
    return age_seconds(lease.get("heartbeat", "")) < ttl


def reap(state_dir: Path, ttl: int) -> list[str]:
    """Delete stale leases and return the removed owners."""
    removed: list[str] = []
    for lease in load_leases(state_dir):
        if is_live(lease, ttl):
            continue
        token = lease.get("token", "?")
        try:
            lease_path(state_dir, token).unlink()
        except OSError:
            continue
        removed.append(lease.get("owner", token))
    return removed


def holders(state_dir: Path, ttl: int) -> list[dict]:
    return [lease for lease in load_leases(state_dir) if is_live(lease, ttl)]


def emit(payload: dict, exit_code: int = 0) -> int:
    print(json.dumps(payload, indent=2))
    return exit_code


def lease_key(args: argparse.Namespace) -> str:
    """Dedupe key for a caller.

    The slot guard plugin exports `MCP_SLOT_OWNER_KEY` into every tool shell,
    so all MCP users inside one opencode process share one lease; without the
    plugin each caller's `--owner` label is its own key.
    """
    return os.environ.get("MCP_SLOT_OWNER_KEY") or args.owner


def new_lease(owner: str, token: str, key: str) -> dict:
    stamp = now()
    return {
        "token": token,
        "key": key,
        "owner": owner,
        "pid": os.getpid(),
        "host": HOST,
        "started": stamp,
        "heartbeat": stamp,
        "loop": os.environ.get("PARITY_LOOP"),
        "display": os.environ.get("DISPLAY") or os.environ.get("PARITY_DISPLAY"),
        "bridge_port": os.environ.get("PARITY_BRIDGE_PORT"),
    }


def cmd_acquire(args: argparse.Namespace) -> int:
    ttl = args.ttl
    key = lease_key(args)
    with registry_lock(args.state_dir):
        leases_dir(args.state_dir).mkdir(parents=True, exist_ok=True)
        reap(args.state_dir, ttl)
        live = holders(args.state_dir, ttl)
        for lease in live:
            if (lease.get("key") or lease.get("owner")) == key:
                lease["heartbeat"] = now()
                lease["pid"] = os.getpid()
                lease_path(args.state_dir, lease["token"]).write_text(
                    json.dumps(lease, indent=2) + "\n", encoding="utf-8"
                )
                return emit({"ok": True, "reused": True, **lease})
        if len(live) >= args.limit:
            return emit(
                {
                    "ok": False,
                    "reason": "limit",
                    "limit": args.limit,
                    "holders": [
                        {"owner": l.get("owner"), "pid": l.get("pid")} for l in live
                    ],
                },
                EXIT_LIMIT,
            )
        lease = new_lease(args.owner, secrets.token_hex(8), key)
        lease_path(args.state_dir, lease["token"]).write_text(
            json.dumps(lease, indent=2) + "\n", encoding="utf-8"
        )
    return emit({"ok": True, "reused": False, **lease})


def cmd_refresh(args: argparse.Namespace) -> int:
    with registry_lock(args.state_dir):
        path = lease_path(args.state_dir, args.token)
        if not path.is_file():
            return emit({"ok": False, "reason": "unknown-token"}, EXIT_NOT_FOUND)
        lease = json.loads(path.read_text(encoding="utf-8"))
        lease["heartbeat"] = now()
        if args.pid is not None:
            lease["pid"] = args.pid
        path.write_text(json.dumps(lease, indent=2) + "\n", encoding="utf-8")
    return emit({"ok": True, **lease})


def cmd_release(args: argparse.Namespace) -> int:
    """Drop a lease by token, by lease key, or by the exported process key."""
    key = args.key or os.environ.get("MCP_SLOT_OWNER_KEY")
    if not args.token and not key:
        return emit({"ok": False, "reason": "no-token-or-key"}, 2)
    with registry_lock(args.state_dir):
        if args.token:
            path = lease_path(args.state_dir, args.token)
            if not path.is_file():
                return emit({"ok": False, "reason": "unknown-token"}, EXIT_NOT_FOUND)
            path.unlink()
            return emit({"ok": True, "released": args.token})
        removed: list[str] = []
        for lease in load_leases(args.state_dir):
            if (lease.get("key") or lease.get("owner")) != key:
                continue
            try:
                lease_path(args.state_dir, lease.get("token", "")).unlink()
                removed.append(lease.get("owner", key))
            except OSError:
                continue
    if not removed:
        return emit({"ok": False, "reason": "unknown-key", "key": key}, EXIT_NOT_FOUND)
    return emit({"ok": True, "released": removed})


def cmd_status(args: argparse.Namespace) -> int:
    with registry_lock(args.state_dir):
        reap(args.state_dir, args.ttl)
        live = holders(args.state_dir, args.ttl)
    return emit(
        {
            "ok": True,
            "limit": args.limit,
            "ttl": args.ttl,
            "holders": live,
            "free": max(0, args.limit - len(live)),
        }
    )


def cmd_gc(args: argparse.Namespace) -> int:
    with registry_lock(args.state_dir):
        removed = reap(args.state_dir, args.ttl)
    return emit({"ok": True, "removed": removed})


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--dir",
        type=Path,
        dest="state_dir",
        default=None,
        help="lease registry dir (default: $MCP_SLOT_DIR or the git common dir)",
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=int(os.environ.get("MCP_SLOT_LIMIT", DEFAULT_LIMIT)),
        help="max concurrent leases including the caller (default: 2)",
    )
    parser.add_argument(
        "--ttl",
        type=int,
        default=int(os.environ.get("MCP_SLOT_TTL", DEFAULT_TTL)),
        help="heartbeat lifetime in seconds (default: 900)",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    p_acquire = sub.add_parser("acquire", help="take a slot, or exit 3 when full")
    p_acquire.add_argument("--owner", required=True, help="stable caller label")
    p_acquire.set_defaults(func=cmd_acquire)

    p_refresh = sub.add_parser("refresh", help="renew a lease heartbeat")
    p_refresh.add_argument("--token", required=True)
    p_refresh.add_argument("--pid", type=int, default=None)
    p_refresh.set_defaults(func=cmd_refresh)

    p_release = sub.add_parser("release", help="drop a lease by token or key")
    p_release.add_argument("--token", default=None)
    p_release.add_argument(
        "--key",
        default=None,
        help="lease key (defaults to $MCP_SLOT_OWNER_KEY, the guard's process key)",
    )
    p_release.set_defaults(func=cmd_release)

    p_status = sub.add_parser("status", help="show live leases and free slots")
    p_status.set_defaults(func=cmd_status)

    p_gc = sub.add_parser("gc", help="reap stale leases")
    p_gc.set_defaults(func=cmd_gc)

    args = parser.parse_args()
    args.state_dir = resolve_state_dir(args.state_dir)
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
