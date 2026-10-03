#!/usr/bin/env bash
# Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
# SPDX-License-Identifier: GPL-3.0-only
#
# Headless Godot export wrapper for plan 22 M1/M7 (§3.4/§6.8). Verifies the
# matching Godot export templates are installed, runs a headless export for the
# requested platform preset, and optionally checks the artifact. Outputs default
# under the gitignored client/bin/export/; override with --out.
#
# Usage:
#   tools/export.sh [--platform windows|linux|macos|android|ios]
#                   [--target release|debug] [--preset NAME] [--out PATH]
#                   [--clean] [--verify]

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GODOT_BIN="${GODOT_BIN:-$(command -v godot4 || command -v godot4-mono || true)}"
PLATFORM="windows"
TARGET="release"
PRESET=""
OUT=""
CLEAN=0
VERIFY=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --platform) PLATFORM="$2"; shift 2 ;;
    --target) TARGET="$2"; shift 2 ;;
    --preset) PRESET="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --clean) CLEAN=1; shift ;;
    --verify) VERIFY=1; shift ;;
    -h|--help)
      sed -n '3,12p' "$0" | sed 's/^# \{0,1\}//'
      exit 0 ;;
    *) echo "export.sh: unknown option $1" >&2; exit 2 ;;
  esac
done

if [[ -z "$GODOT_BIN" || ! -x "$GODOT_BIN" ]]; then
  echo "export.sh: godot4 not found (set GODOT_BIN)" >&2
  exit 2
fi

EXPORT_DIR="$REPO_ROOT/client/bin/export"
case "$PLATFORM" in
  windows) PRESET="${PRESET:-Windows Desktop}"; OUT="${OUT:-$EXPORT_DIR/Mindustry-Godot.exe}" ;;
  linux)   PRESET="${PRESET:-Linux/X11}";       OUT="${OUT:-$EXPORT_DIR/Mindustry-Godot.x86_64}" ;;
  macos)   PRESET="${PRESET:-macOS}";           OUT="${OUT:-$EXPORT_DIR/Mindustry-Godot.zip}" ;;
  android) PRESET="${PRESET:-Android}";         OUT="${OUT:-$EXPORT_DIR/Mindustry-Godot.apk}" ;;
  ios)     PRESET="${PRESET:-iOS}";             OUT="${OUT:-$EXPORT_DIR/Mindustry-Godot.ipa}" ;;
  *) echo "export.sh: unknown platform $PLATFORM" >&2; exit 2 ;;
esac

GODOT_VERSION="$("$GODOT_BIN" --version 2>&1 | head -n1 | awk -F. '{print $1"."$2"."$3}')"
TEMPLATES_DIR="${GODOT_TEMPLATES_DIR:-$HOME/.local/share/godot/export_templates/${GODOT_VERSION}.stable}"
if [[ ! -f "$TEMPLATES_DIR/version.txt" ]]; then
  echo "export.sh: Godot ${GODOT_VERSION} export templates missing at $TEMPLATES_DIR" >&2
  echo "  install Godot_v${GODOT_VERSION}-stable_export_templates.tpz under $TEMPLATES_DIR" >&2
  exit 2
fi

mkdir -p "$(dirname "$OUT")"
if (( CLEAN )); then rm -f "$OUT"; fi

echo "export.sh: preset='$PRESET' target=$TARGET -> $OUT"
"$GODOT_BIN" --headless --path "$REPO_ROOT/client" "--export-$TARGET" "$PRESET" "$OUT"

if (( VERIFY )); then
  for _ in $(seq 1 60); do [[ -s "$OUT" ]] && break; sleep 0.25; done
  if [[ ! -s "$OUT" ]]; then
    echo "export.sh: export artifact missing or empty: $OUT" >&2
    exit 1
  fi
  echo "export.sh: verified $OUT ($(wc -c < "$OUT") bytes)"
fi
