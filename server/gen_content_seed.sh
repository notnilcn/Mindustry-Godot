#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Regenerates the vanilla content manifest consumed by the STDB module's
# `seed_content_catalog` (plan 21 §3.12.4 / §6.8). The manifest is generated
# from the plan-02 content registry; `server/build.sh` invokes this when
# `GEN_CONTENT_SEED=1` is set (opt-in so the default publish stays fast).
#
# Usage: bash server/gen_content_seed.sh
#
# When the content build is unavailable the checked-in `content_seed.rs` stays
# empty and the module logs the documented shape-only degradation (OD-21-F).

set -euo pipefail

export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT="$SCRIPT_DIR/spacetimedb/src/main/content_seed.rs"

echo "== generating vanilla content seed -> $OUT =="
cargo run -q --manifest-path "$REPO_ROOT/client/rust/Cargo.toml" -p mind-headless -- \
  content seed --out "$OUT"

echo "== done: $(grep -c '^    (' "$OUT") content entries =="
