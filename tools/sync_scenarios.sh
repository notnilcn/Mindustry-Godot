#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Mirrors canonical scenario JSON (repo-root scenarios/) into client/scenarios/
# so `MindSimHost.load_scenario("res://scenarios/<name>.json")` works in-engine.
# client/scenarios/ is a gitignored mirror (never edit it by hand).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$REPO_ROOT/scenarios"
DST="$REPO_ROOT/client/scenarios"

if [[ ! -d "$SRC" ]]; then
  echo "sync_scenarios: missing $SRC" >&2
  exit 1
fi

mkdir -p "$DST"
# Exact mirror: remove stale JSONs, then copy the canonical set.
find "$DST" -maxdepth 1 -type f -name '*.json' -delete
shopt -s nullglob
files=("$SRC"/*.json)
for file in "${files[@]}"; do
  cp -f "$file" "$DST/"
done
echo "sync_scenarios: synced ${#files[@]} file(s) -> client/scenarios/"
