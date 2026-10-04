#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Rebuilds the Rust GDExtension (`tools/build.sh`) whenever the Rust workspace
# under client/rust/ changes. Prefers inotifywait; falls back to a polling loop
# when it is absent. Build output (client/rust/target, client/bin) is ignored so
# the loop never retriggers itself.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WATCH_DIR="$REPO_ROOT/client/rust"
BUILD_SCRIPT="$REPO_ROOT/tools/build.sh"

run_build() {
  echo "[watch] change detected; rebuilding"
  if ! "$BUILD_SCRIPT"; then
    echo "[watch] build failed; still watching"
  fi
}

if command -v inotifywait >/dev/null 2>&1; then
  echo "[watch] watching $WATCH_DIR with inotifywait (Ctrl+C to stop)"
  # `-m` keeps the monitor open; paths under target/ or bin/ are dropped before
  # the extension check so their generated files never cause a rebuild loop.
  inotifywait -q -m -r -e modify,create,delete,move \
    --format '%w%f' "$WATCH_DIR" |
    while IFS= read -r path; do
      case "$path" in
        */target/* | */bin/*) continue ;;
      esac
      case "$path" in
        *.rs | *.toml) run_build ;;
      esac
    done
else
  echo "[watch] inotifywait unavailable; polling every 2s (Ctrl+C to stop)"
  snapshot() {
    find "$WATCH_DIR" -type f \( -name '*.rs' -o -name '*.toml' \) \
      -not -path '*/target/*' -not -path '*/bin/*' \
      -printf '%T@ %p\n' 2>/dev/null | sort
  }
  last="$(snapshot)"
  while true; do
    sleep 2
    current="$(snapshot)"
    if [[ "$current" != "$last" ]]; then
      last="$current"
      run_build
    fi
  done
fi
