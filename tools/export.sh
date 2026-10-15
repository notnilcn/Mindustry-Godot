#!/usr/bin/env bash
# Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
# SPDX-License-Identifier: GPL-3.0-only
#
# Headless Godot export wrapper for plan 22 M1/M7 (§3.4/§6.8); I07 T7 ships
# the runtime asset subset beside the artifact. Verifies the matching Godot
# export templates are installed, runs a headless export for the requested
# platform preset, copies the assets/ subset next to the output, and
# optionally checks the artifact. Outputs default under the gitignored
# client/bin/export/; override with --out.
#
# Usage:
#   tools/export.sh [PLATFORM] [--platform windows|linux|macos|android|ios]
#                   [--target release|debug] [--preset NAME] [--out PATH]
#                   [--clean] [--verify] [--with-assets|--no-assets]
#                   [--assets-only DIR]
#
# --with-assets (default) copies the runtime subset of assets/ next to the
# exported binary, running tools/pack.sh pack first when the atlas manifest
# is missing. --no-assets skips the copy. --assets-only DIR copies the
# subset and exits without running a Godot export (no templates needed).
# PLATFORM is a positional shorthand for --platform (e.g. `export.sh linux`).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GODOT_BIN="${GODOT_BIN:-$(command -v godot4 || command -v godot4-mono || command -v godot || true)}"
PLATFORM="windows"
TARGET="release"
PRESET=""
OUT=""
CLEAN=0
VERIFY=0
WITH_ASSETS=1
ASSETS_ONLY=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --platform) PLATFORM="$2"; shift 2 ;;
    windows|linux|macos|android|ios) PLATFORM="$1"; shift ;;
    --target) TARGET="$2"; shift 2 ;;
    --preset) PRESET="$2"; shift 2 ;;
    --out) OUT="$2"; shift 2 ;;
    --clean) CLEAN=1; shift ;;
    --verify) VERIFY=1; shift ;;
    --with-assets) WITH_ASSETS=1; shift ;;
    --no-assets) WITH_ASSETS=0; shift ;;
    --assets-only) ASSETS_ONLY="$2"; shift 2 ;;
    -h|--help)
      sed -n '12,22p' "$0" | sed 's/^# \{0,1\}//'
      exit 0 ;;
    *) echo "export.sh: unknown option $1" >&2; exit 2 ;;
  esac
done

# The generated atlas manifest is gitignored, so a fresh checkout needs the
# offline pack pipeline before an export can ship real sprites.
ensure_packed_assets() {
  local atlas="$REPO_ROOT/assets/sprites/sprites.atlas.json"
  if [[ ! -f "$atlas" ]]; then
    echo "export.sh: $atlas missing; running tools/pack.sh pack"
    bash "$REPO_ROOT/tools/pack.sh" pack
  fi
}

# Copies the runtime asset subset (everything mind-gdext reads at runtime)
# under <dest>/assets/. Each entry names its consumer; this list is the
# export contract, keep it in sync with the readers:
#   sprites/            gdext/assets/atlas.rs (atlas pages + manifest, the
#                       2048 fallback atlas, block_colors.png)
#   fonts/              gdext/assets/fonts.rs
#   bundles/            gdext/assets/bundle.rs (bundle*.properties)
#   locales/            gdext/assets/bundle.rs (load_locales)
#   icons/              gdext/assets/bundle.rs (icons.properties,
#                       icon_codes.json)
#   sounds/             gdext/assets/audio.rs + gdext/audio/streams.rs
#   music/              gdext/audio/streams.rs (music/*)
#   maps/               gdext/assets/loader.rs (assets_maps_dir, preset maps)
#   shaders/            gdext/render/shaders.rs (shader.index.json, godot/)
#   baseparts/          gdext/sim_host.rs (base schematics)
#   cursors/            gdext/assets/atlas.rs (cursors/<name>.png)
#   planets/            gdext/campaign.rs (assets/planets/<name>.json)
#   contributors        gdext/assets/atlas.rs (contributors file)
#   sounds.index.json   gdext/assets/audio.rs
# Never shipped: assets/parity, assets-raw, assets/scripts (Java JS),
# assets/bloomshaders and assets/cubemaps (not read by the port).
copy_runtime_assets() {
  local root="$1" dest="$2/assets"
  mkdir -p "$dest"
  local entry
  for entry in sprites fonts bundles locales icons sounds music maps shaders \
               baseparts cursors planets contributors sounds.index.json; do
    local src="$root/assets/$entry"
    if [[ ! -e "$src" ]]; then
      echo "export.sh: warning: assets/$entry missing (skipped)" >&2
      continue
    fi
    if [[ -d "$src" ]]; then
      mkdir -p "$dest/$entry"
      cp -a "$src/." "$dest/$entry/"
    else
      cp -a "$src" "$dest/$entry"
    fi
  done
}

if [[ -n "$ASSETS_ONLY" ]]; then
  ensure_packed_assets
  copy_runtime_assets "$REPO_ROOT" "$ASSETS_ONLY"
  echo "export.sh: assets subset copied to $ASSETS_ONLY/assets"
  exit 0
fi

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

if (( WITH_ASSETS )); then
  ensure_packed_assets
  copy_runtime_assets "$REPO_ROOT" "$(dirname "$OUT")"
  echo "export.sh: assets subset copied to $(dirname "$OUT")/assets"
fi

if (( VERIFY )); then
  for _ in $(seq 1 60); do [[ -s "$OUT" ]] && break; sleep 0.25; done
  if [[ ! -s "$OUT" ]]; then
    echo "export.sh: export artifact missing or empty: $OUT" >&2
    exit 1
  fi
  echo "export.sh: verified $OUT ($(wc -c < "$OUT") bytes)"
fi
