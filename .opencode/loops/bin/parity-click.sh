#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Single-process mouse move+click for the Java reference on a loop display.
#
# Xvfb resets the pointer to the screen center when the XTEST client
# disconnects, so `computer-mcp mouse move` followed by a separate
# `computer-mcp mouse click` does not work on an Xvfb display. The persistent
# computer-mcp MCP server keeps its connection and is unaffected; this helper
# gives the CLI/script path the same behavior by moving and clicking inside a
# single process.
#
# Usage: parity-click.sh <x> <y> [left|right|middle] [--double]
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/loop-vars.sh
. "$script_dir/../lib/loop-vars.sh"
parity_loop_vars "${PARITY_LOOP:-1}"

if [ $# -lt 2 ]; then
  echo "usage: parity-click.sh <x> <y> [left|right|middle] [--double]" >&2
  exit 2
fi
x="$1"
y="$2"
shift 2
button="left"
double=0
for arg in "$@"; do
  case "$arg" in
    left | right | middle) button="$arg" ;;
    --double) double=1 ;;
    *)
      echo "parity-click: unknown argument: $arg" >&2
      exit 2
      ;;
  esac
done

py="${MCP_VENV_PYTHON:-}"
if [ -z "$py" ]; then
  for candidate in \
    "$HOME/.local/share/uv/tools/computer-mcp/bin/python" \
    "$HOME/.local/share/mcp-venv/bin/python" \
    python3; do
    if command -v "$candidate" >/dev/null 2>&1 && "$candidate" -c 'import pynput' >/dev/null 2>&1; then
      py="$candidate"
      break
    fi
  done
fi
if [ -z "$py" ]; then
  echo "parity-click: no python with pynput found (set MCP_VENV_PYTHON)" >&2
  exit 1
fi

DISPLAY="$PARITY_DISPLAY" "$py" - "$x" "$y" "$button" "$double" <<'PY'
import sys
import time

from pynput.mouse import Button, Controller

x, y = int(sys.argv[1]), int(sys.argv[2])
button = {"left": Button.left, "right": Button.right, "middle": Button.middle}[sys.argv[3]]
count = 2 if sys.argv[4] == "1" else 1

controller = Controller()
controller.position = (x, y)
time.sleep(0.05)
controller.click(button, count)
time.sleep(0.15)
PY
