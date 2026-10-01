#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Builds the Rust side of the Godot client (mind-gdext cdylib + mind-headless)
# into client/bin/rust so mind.gdextension can find the shared library.
# Afterwards, syncs scenarios only if the Lane-B-owned sync script exists.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cargo build \
  --manifest-path "$REPO_ROOT/client/rust/Cargo.toml" \
  -p mind-gdext -p mind-headless \
  --target-dir "$REPO_ROOT/client/bin/rust" \
  "$@"

SYNC="$REPO_ROOT/tools/sync_scenarios.sh"
if [[ -f "$SYNC" ]]; then
  bash "$SYNC"
fi
