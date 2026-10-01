#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Resolves the Godot 4.7 binary ($GODOT_BIN -> godot4 -> godot4-mono), ensures it
# is executable, and runs it against <repo>/client unless the caller already
# passed --path. Set GODOT_VERBOSE=1 to print the resolved binary.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CLIENT_DIR="$REPO_ROOT/client"

resolve_godot() {
  if [[ -n "${GODOT_BIN:-}" ]]; then
    printf '%s\n' "$GODOT_BIN"
    return 0
  fi
  if command -v godot4 >/dev/null 2>&1; then
    command -v godot4
    return 0
  fi
  if command -v godot4-mono >/dev/null 2>&1; then
    command -v godot4-mono
    return 0
  fi
  return 1
}

if ! GODOT="$(resolve_godot)"; then
  echo "godot.sh: no Godot binary found." >&2
  echo "  Set GODOT_BIN to a Godot 4.7 binary, or install Godot 4.7.2 so that" >&2
  echo "  'godot4' (preferred) or 'godot4-mono' (fallback) is on PATH." >&2
  exit 1
fi

if [[ ! -x "$GODOT" ]]; then
  chmod +x "$GODOT" 2>/dev/null || true
fi
if [[ ! -x "$GODOT" ]]; then
  echo "godot.sh: resolved binary '$GODOT' is not executable." >&2
  exit 1
fi

if [[ "${GODOT_VERBOSE:-0}" == "1" ]]; then
  echo "godot.sh: using '$GODOT'" >&2
fi

has_path=0
for arg in "$@"; do
  case "$arg" in
    --path|--path=*)
      has_path=1
      break
      ;;
  esac
done

if ((has_path)); then
  exec "$GODOT" "$@"
else
  exec "$GODOT" --path "$CLIENT_DIR" "$@"
fi
