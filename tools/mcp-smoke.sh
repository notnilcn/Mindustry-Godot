#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Local integration gate: drives the running Godot editor + game through a
# fresh stdio `open-godot-mcp` server and checks the spine end-to-end
# (`00_FOUNDATION_IMPLEMENTATION_PLAN.md` §7c steps 1-6 and 9).
#
# Requirements:
#   * Linux dev host, editor already running per the repo playtest skill:
#       nohup godot4 --editor --path client \
#         >/tmp/mind-editor.log 2>&1 &
#   * `python3` on PATH (stdlib only).
#   * The MCP bridge (ws://127.0.0.1:6970) accepts multiple clients, so this can
#     run while opencode's own MCP server is attached; the other instance does
#     not need to be closed.
#
# If the bridge is not connected the driver prints the launch command and exits
# non-zero. GPU/MCP checks are local-only; `.github/workflows/ci.yml` never runs
# this script.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! command -v python3 >/dev/null 2>&1; then
  echo "mcp-smoke.sh: python3 not found on PATH" >&2
  exit 1
fi

exec python3 "$REPO_ROOT/tools/mcp_smoke.py" "$@"
