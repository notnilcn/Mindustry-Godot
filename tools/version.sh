#!/usr/bin/env bash
# Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
# SPDX-License-Identifier: GPL-3.0-only
#
# Generates client/assets/version.properties (upstream key set) for plan 22 M0
# §3.2. Values come from flags; commit hash/date default to the current git
# state. The file is gitignored and regenerated on each release build.
#
# Usage:
#   tools/version.sh [--build-number N] [--revision R] [--type T]
#                    [--modifier M] [--commit-hash H] [--build-date D]
#                    [--out PATH]

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$REPO_ROOT/client/assets/version.properties"

TYPE="unknown"
MODIFIER="unknown"
BUILD_NUMBER="-1"
REVISION="0"
COMMIT_HASH=""
BUILD_DATE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --build-number) BUILD_NUMBER="$2"; shift 2 ;;
    --revision) REVISION="$2"; shift 2 ;;
    --type) TYPE="$2"; shift 2 ;;
    --modifier) MODIFIER="$2"; shift 2 ;;
    --commit-hash) COMMIT_HASH="$2"; shift 2 ;;
    --build-date) BUILD_DATE="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    *) echo "version.sh: unknown option $1" >&2; exit 2 ;;
  esac
done

if [[ -z "$COMMIT_HASH" ]]; then
  COMMIT_HASH="$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo unknown)"
fi
if [[ -z "$BUILD_DATE" ]]; then
  BUILD_DATE="$(date -u +%Y-%m-%d)"
fi

if [[ "$BUILD_NUMBER" =~ ^-?[0-9]+$ ]] && (( BUILD_NUMBER < 0 )); then
  BUILD="-1"
elif [[ "$REVISION" =~ ^[0-9]+$ ]] && (( REVISION > 0 )); then
  BUILD="${BUILD_NUMBER}.${REVISION}"
else
  BUILD="$BUILD_NUMBER"
fi

mkdir -p "$(dirname "$OUT")"
# LF-only, upstream key order.
printf 'type=%s\nnumber=4\nmodifier=%s\ncommitHash=%s\nbuildDate=%s\nbuild=%s\n' \
  "$TYPE" "$MODIFIER" "$COMMIT_HASH" "$BUILD_DATE" "$BUILD" > "$OUT"

echo "version.sh: wrote $OUT"
cat "$OUT"
