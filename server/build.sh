#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Publishes the mindustry_godot SpacetimeDB module and regenerates the checked-in
# Rust client bindings. Bash is primary (WSL2 Ubuntu); build.ps1 is the Windows
# parity wrapper.
#
# Usage:
#   server/build.sh [--db NAME] [--server HOST] [--check]
#
#   --db NAME      database name (default: mindustry)
#   --server HOST  server nickname/URL (default: local == http://127.0.0.1:3000)
#   --check        drift gate: regenerate into a temp dir and diff against the
#                  checked-in bindings; never publishes, exit 0 when clean.
#
# Publishing wipes local dev data (`--delete-data=always`); seeds re-run. The
# project server/db defaults also live in server/spacetime.json.
#
# NOTE (2.10.1): STDB database names match `^[a-z0-9]+(-[a-z0-9]+)*$` —
# underscores are rejected ("invalid characters in database name"). The
# module/crate stays `mindustry_godot`; the local database is `mindustry`
# (integration `mindustry-it`).

set -euo pipefail

# Resolve tools even from non-login shells (CI); harmless when already on PATH.
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MODULE_PATH="$SCRIPT_DIR/spacetimedb"
BINDINGS_DIR="$REPO_ROOT/client/rust/mind-stdb/src/module_bindings"

DB="mindustry"
SERVER="local"
CHECK=0

usage() {
  sed -n '8,22p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --db)
      [[ $# -ge 2 ]] || { echo "build.sh: --db needs a value" >&2; exit 2; }
      DB="$2"; shift 2;;
    --server)
      [[ $# -ge 2 ]] || { echo "build.sh: --server needs a value" >&2; exit 2; }
      SERVER="$2"; shift 2;;
    --check)
      CHECK=1; shift;;
    -h|--help)
      usage; exit 0;;
    *)
      echo "build.sh: unknown argument '$1' (see --help)" >&2; exit 2;;
  esac
done

command -v spacetime >/dev/null 2>&1 || {
  echo "build.sh: spacetime CLI not found on PATH (install 2.10.1 in WSL2 Ubuntu)" >&2
  exit 1
}

echo "== mindustry-godot STDB build =="
echo "   db:      $DB"
echo "   server:  $SERVER"
echo "   module:  $MODULE_PATH"
echo "   bindings: $BINDINGS_DIR"
echo "   CLI:     $(spacetime --version | head -n 1)"

if [[ "$CHECK" -eq 1 ]]; then
  TMP_DIR="$(mktemp -d)"
  trap 'rm -rf "$TMP_DIR"' EXIT

  echo "== check: regenerate bindings into $TMP_DIR =="
  spacetime generate --lang rust --out-dir "$TMP_DIR" --module-path "$MODULE_PATH" -y

  if diff -ru "$BINDINGS_DIR" "$TMP_DIR"; then
    echo "== check: bindings are drift-clean =="
    exit 0
  fi
  echo "FAIL: generated bindings differ from $BINDINGS_DIR" >&2
  echo "      run server/build.sh to regenerate them (never hand-edit)." >&2
  exit 1
fi

echo "== generate rust bindings =="
spacetime generate --lang rust --out-dir "$BINDINGS_DIR" --module-path "$MODULE_PATH" -y

# Plan 21 §3.12.4: opt-in vanilla content manifest generation. Off by default so
# the normal publish stays fast; `GEN_CONTENT_SEED=1 server/build.sh` regenerates
# `main/content_seed.rs` before publishing (empty stays a valid shape-only seed).
if [[ "${GEN_CONTENT_SEED:-0}" == "1" ]]; then
  echo "== generate content seed =="
  bash "$SCRIPT_DIR/gen_content_seed.sh" \
    || echo "warning: content seed generation failed; keeping the existing manifest"
fi

echo "== publish $DB to $SERVER (wipes dev data) =="
spacetime publish "$DB" --server "$SERVER" --module-path "$MODULE_PATH" --delete-data=always -y

echo "== done: bindings in $BINDINGS_DIR =="
