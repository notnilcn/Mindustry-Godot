#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# everything_you_need.sh - one-shot bootstrap for a fresh Mindustry-Godot checkout.
#
# Builds the Rust GDExtension (mind-gdext) + headless oracle (mind-headless) into
# client/bin/rust so res://mind.gdextension resolves, packs the runtime asset tree
# (assets/sprites, shaders, icons, sounds, locales) with the offline mind-tools
# pipeline, then mirrors the canonical scenario JSON into client/scenarios/.
# Run this before opening client/project.godot.
#
# Why not automatic in Godot: GDExtension libraries are loaded during project
# initialization, before any editor plugin or autoload runs, so there is no hook
# to build them on first open. Run this first, or wire it into a git
# post-checkout/post-merge hook.
#
# Usage:
#   ./everything_you_need.sh [--release] [--no-pack] [--stdb] [--import] [-- <cargo args>]
#
#   --release   build optimized (release) instead of debug
#   --no-pack   skip the offline asset pipeline; a fresh checkout needs it
#   --stdb      also publish the SpacetimeDB module + regenerate the client
#               bindings (needs the `spacetime` CLI and a running local server;
#               wipes local dev data). Checked-in bindings normally suffice.
#   --import    also run a headless Godot import/parse pass (needs `godot4`)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Resolve cargo/godot even from non-login shells (CI); harmless when already on PATH.
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"

RELEASE=0
PACK=1
STDB=0
IMPORT=0
CARGO_ARGS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --release) RELEASE=1; shift;;
    --no-pack) PACK=0; shift;;
    --stdb) STDB=1; shift;;
    --import) IMPORT=1; shift;;
    --) shift; CARGO_ARGS=("$@"); break;;
    -h|--help) sed -n '4,25p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0;;
    *) echo "everything_you_need.sh: unknown argument '$1' (see --help)" >&2; exit 2;;
  esac
done

command -v cargo >/dev/null 2>&1 || {
  echo "everything_you_need.sh: cargo not found. Install Rust (https://rustup.rs) and retry." >&2
  exit 1
}

CARGO_BUILD_ARGS=(
  build
  --manifest-path "$REPO_ROOT/client/rust/Cargo.toml"
  -p mind-gdext -p mind-headless
  --target-dir "$REPO_ROOT/client/bin/rust"
)
if [[ "$RELEASE" -eq 1 ]]; then
  CARGO_BUILD_ARGS+=(--release)
fi

echo "== [1/4] build mind-gdext + mind-headless =="
cargo "${CARGO_BUILD_ARGS[@]}" ${CARGO_ARGS[@]+"${CARGO_ARGS[@]}"}

if [[ "$PACK" -eq 1 ]]; then
  echo "== [2/4] pack runtime assets -> assets/ =="
  bash "$REPO_ROOT/tools/pack.sh" pack
else
  echo "== [2/4] asset pack skipped (--no-pack) =="
fi

echo "== [3/4] sync scenarios -> client/scenarios =="
bash "$REPO_ROOT/tools/sync_scenarios.sh"

if [[ "$STDB" -eq 1 ]]; then
  echo "== [4/4] publish SpacetimeDB module + regenerate bindings =="
  bash "$REPO_ROOT/server/build.sh"
else
  echo "== [4/4] SpacetimeDB bindings already checked in (skip; pass --stdb to regenerate) =="
fi

if [[ "$IMPORT" -eq 1 ]]; then
  echo "== import / parse check (headless Godot) =="
  bash "$REPO_ROOT/tools/godot.sh" --headless --editor --quit
fi

echo
echo "done: shared library is in client/bin/rust/<profile>/."
echo "      open client/project.godot, or run: tools/godot.sh"
