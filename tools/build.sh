#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Builds the Rust side of the Godot client (mind-gdext cdylib + mind-headless)
# into client/bin/rust so mind.gdextension can find the shared library.
# Also builds the phantom-camera-rust addon, which is its own repository
# (client/addons/phantom_camera_rust, vendored as a submodule) with its own
# Cargo workspace; its tools/build.sh installs the cdylib next to the addon's
# .gdextension under client/addons/phantom_camera_rust/bin/<profile>/.
# Afterwards, syncs scenarios only if the Lane-B-owned sync script exists.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Serialize with every other CPU-heavy task (docs/CI-WORKFLOW.md).
# shellcheck source=tools/lib/cpu-guard.sh
source "$REPO_ROOT/tools/lib/cpu-guard.sh"
mind_cpu_guard build "$REPO_ROOT/tools/build.sh" "$@"

ADDON_DIR="$REPO_ROOT/client/addons/phantom_camera_rust"
if [[ ! -f "$ADDON_DIR/Cargo.toml" ]]; then
  echo "build.sh: missing addon checkout at $ADDON_DIR" >&2
  echo "          run: git submodule update --init --recursive" >&2
  exit 1
fi

cargo build \
  --manifest-path "$REPO_ROOT/client/rust/Cargo.toml" \
  -p mind-gdext -p mind-headless \
  --target-dir "$REPO_ROOT/client/bin/rust" \
  "$@"

# The addon is a self-contained workspace, so build it through its own script
# (which installs the library where the addon's .gdextension expects it).
# Forward only profile selection; other cargo args do not apply to its manifest.
addon_args=()
for arg in "$@"; do
  case "$arg" in
    --release|--debug|--all) addon_args+=("$arg") ;;
  esac
done
# Keep the addon build's install path deterministic: its script copies from
# `<addon>/target/<profile>`, so an inherited CARGO_TARGET_DIR would break it.
env -u CARGO_TARGET_DIR bash "$ADDON_DIR/tools/build.sh" "${addon_args[@]}"

SYNC="$REPO_ROOT/tools/sync_scenarios.sh"
if [[ -f "$SYNC" ]]; then
  bash "$SYNC"
fi
