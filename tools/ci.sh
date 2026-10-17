#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Full P0 CI gate (`00_FOUNDATION_IMPLEMENTATION_PLAN.md` §3.8/M7):
#   fmt + clippy + check + mind-core tests
#   headless goldens (spine_place_break / spine_determinism / spine_many_commands)
#   scenario mirror sync assertion
#   bench_baseline budget (+20% warn / +50% fail, ns resolution)
#   Godot build + headless editor import/parse check
#   crate boundary greps, mind-stdb check, STDB module typecheck, bindings drift
#
# The MCP smoke is intentionally NOT here: it needs the interactive editor and
# is local-only (`tools/mcp-smoke.sh`).

set -euo pipefail

# Resolve cargo/godot/spacetime even from non-login shells (CI, MCP tools).
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

MANIFEST="client/rust/Cargo.toml"

# Baseline refresh (2026-10-09, session2 merge): plan-loop landings since the
# 10-02 re-measure roughly doubled the fixed-`sim::schedule` per-tick cost on
# the flat bench map. Pinned, back-to-back release truth on this tree is
# p50 ~235 ns (stable across 10 runs, 220-250) / p99 ~295 ns (≈ old 301, kept).
# The session2 delta itself is nil: merge-base eab320c benches p50 ~220 ns on
# the same box/pinning, so the drift accumulated over the week, not in the
# merge. Revisit with a profile (not a re-baseline) if p50 grows again.
# The scenario file only stores the rounded `p50_us` / `p99_us` that
# `mind-headless bench` compares against; compare the JSON's ns fields against
# the ns truth so the regression gate stays honest.
BASE_P50_NS=235
BASE_P99_NS=301
WARN_P50_NS=$((BASE_P50_NS * 120 / 100))
WARN_P99_NS=$((BASE_P99_NS * 120 / 100))
FAIL_P50_NS=$((BASE_P50_NS * 150 / 100))
FAIL_P99_NS=$((BASE_P99_NS * 150 / 100))

say() { printf '\n== %s ==\n' "$1"; }

say "cargo fmt (workspace --check)"
cargo fmt --manifest-path "$MANIFEST" --all -- --check

say "cargo clippy (workspace --all-targets -- -D warnings)"
cargo clippy --manifest-path "$MANIFEST" --workspace --all-targets -- -D warnings

say "cargo check (workspace)"
cargo check --manifest-path "$MANIFEST" --workspace

say "cargo test -p mind-core"
cargo test --manifest-path "$MANIFEST" -p mind-core

# Plan 23 §7f: matrix + registry drift + T0 catalog smoke. Read-only, no
# Godot/GPU. `--tests` turns on the "every landed matrix row resolves" gate.
say "parity check (matrix + registries) + T0 smoke"
PARITY_DIR="$(mktemp -d)"
cargo test --manifest-path "$MANIFEST" -p mind-core     -- --list > "$PARITY_DIR/tests-core.txt"
cargo test --manifest-path "$MANIFEST" -p mind-headless -- --list > "$PARITY_DIR/tests-headless.txt"
cat "$PARITY_DIR/tests-core.txt" "$PARITY_DIR/tests-headless.txt" > "$PARITY_DIR/tests.txt"
cargo run -q --manifest-path "$MANIFEST" -p mind-headless -- parity check --tests "$PARITY_DIR/tests.txt"
cargo run -q --manifest-path "$MANIFEST" -p mind-headless -- parity goldens
cargo run -q --manifest-path "$MANIFEST" -p mind-headless -- run-all --tier T0
rm -rf "$PARITY_DIR"

say "golden: run spine_place_break"
cargo run -q --manifest-path "$MANIFEST" -p mind-headless -- run spine_place_break --json > /dev/null

say "golden: run spine_determinism"
cargo run -q --manifest-path "$MANIFEST" -p mind-headless -- run spine_determinism --json > /dev/null

say "golden: run spine_many_commands"
cargo run -q --manifest-path "$MANIFEST" -p mind-headless -- run spine_many_commands --json > /dev/null

say "scenario mirror: sync + diff (scenarios/ -> client/scenarios/)"
bash tools/sync_scenarios.sh
if ! diff -r scenarios client/scenarios; then
  echo "FAIL: client/scenarios is not an exact mirror of scenarios/" >&2
  exit 1
fi

say "bench: bench_baseline budget (warn +20% / fail +50% vs ${BASE_P50_NS}/${BASE_P99_NS} ns)"
# Build unpinned (all cores), then measure the single-threaded tick loop pinned
# to one CPU. The baseline truth above was measured pinned; the tick loop only
# runs ~24 ms of wall time, so unpinned runs absorb scheduler migration and
# preemption spikes into the p99 tail (240 ns p50 / 591 ns p99 observed on an
# idle host post-reboot, ~80% of unpinned runs over +50% here) while p50 stays
# at baseline. taskset is util-linux; fall back to the unpinned cargo run.
cargo build -q --release --manifest-path "$MANIFEST" -p mind-headless
BENCH_CMD=(cargo run -q --release --manifest-path "$MANIFEST" -p mind-headless -- bench --ticks 100000 --scenario bench_baseline)
if command -v taskset >/dev/null 2>&1; then
  BENCH_CMD=(taskset -c "$(( $(nproc) - 1 ))" "${BENCH_CMD[@]}")
fi
set +e
BENCH_JSON="$("${BENCH_CMD[@]}")"
BENCH_RC=$?
set -e
printf '  %s\n' "$BENCH_JSON"
if [[ -z "$BENCH_JSON" ]]; then
  echo "FAIL: bench produced no JSON (exit $BENCH_RC)" >&2
  exit 1
fi
read -r P50_NS P99_NS BENCH_STATUS <<<"$(printf '%s' "$BENCH_JSON" | python3 -c '
import json, sys
d = json.load(sys.stdin)
print(d["p50_ns"], d["p99_ns"], d["baseline_status"])
')"
if [[ ! "$P50_NS" =~ ^[0-9]+$ || ! "$P99_NS" =~ ^[0-9]+$ ]]; then
  echo "FAIL: could not parse bench JSON ns fields" >&2
  exit 1
fi
if [[ "$BENCH_RC" -ne 0 || "$BENCH_STATUS" == "fail" ]] \
  || ((P50_NS > FAIL_P50_NS)) || ((P99_NS > FAIL_P99_NS)); then
  echo "FAIL: bench regression beyond +50% (p50 ${P50_NS}ns, p99 ${P99_NS}ns; base ${BASE_P50_NS}/${BASE_P99_NS}ns)" >&2
  exit 1
fi
if ((P50_NS > WARN_P50_NS)) || ((P99_NS > WARN_P99_NS)); then
  echo "WARN: bench regression beyond +20% (p50 ${P50_NS}ns, p99 ${P99_NS}ns; base ${BASE_P50_NS}/${BASE_P99_NS}ns)"
else
  echo "  bench ok: p50 ${P50_NS}ns / p99 ${P99_NS}ns"
fi

say "godot: build mind-gdext + mind-headless (client/bin/rust)"
bash tools/build.sh

say "godot: headless editor import/parse check"
GODOT_LOG="$(mktemp)"
trap 'rm -f "$GODOT_LOG"' EXIT
if ! bash tools/godot.sh --headless --editor --quit --path client >"$GODOT_LOG" 2>&1; then
  echo "FAIL: godot --headless --editor --quit exited non-zero" >&2
  tail -n 40 "$GODOT_LOG" >&2
  exit 1
fi
# Godot prints a benign `ERROR: <n> resources still in use at exit` cleanup line
# on every headless editor quit; drop it before the parse/load error scan.
if grep -nE 'SCRIPT ERROR|SHADER ERROR|Parse Error|Failed to load|Cannot open|Can.t open|ERROR:' "$GODOT_LOG" |
    grep -vE 'resources still in use at exit' >&2; then
  echo "FAIL: godot headless import logged parse/load errors (above)" >&2
  exit 1
fi
echo "  godot headless import clean"

say "godot: UI layout-engine check (MindTable min-size propagation, L01 T1)"
if ! bash tools/godot.sh --headless --path client --script res://tools/ui_layout_check.gd >"$GODOT_LOG" 2>&1 \
    || ! grep -q 'LAYOUTCHECK: failed=0' "$GODOT_LOG"; then
  echo "FAIL: client/tools/ui_layout_check.gd" >&2
  grep -E 'FAIL|SCRIPT ERROR|LAYOUTCHECK' "$GODOT_LOG" >&2 || tail -n 40 "$GODOT_LOG" >&2
  exit 1
fi
echo "  ui layout check clean"

say "boundary: mind-core must stay godot-free and tokio-free"
# NOTE: the repo path itself contains "mindustry-godot", so a naive `grep godot`
# on the default tree (which prints the root path) always matches. `--prefix none`
# puts one package name per line; anchor the match to the package name.
if cargo tree --manifest-path "$MANIFEST" -p mind-core --prefix none | grep -Eq '^(godot|tokio)( |$)'; then
  echo "FAIL: mind-core dependency tree contains godot/tokio" >&2
  exit 1
fi
# Use grep, as the GitHub workflow does: `rg` is not installed on every host
# and a missing command under `if` would silently skip this gate.
if grep -RnE --include='*.rs' 'use (godot|tokio)' client/rust/mind-core; then
  echo "FAIL: mind-core source imports godot/tokio" >&2
  exit 1
fi

say "cargo check -p mind-stdb (includes checked-in generated bindings)"
cargo check --manifest-path "$MANIFEST" -p mind-stdb

say "cargo check server/spacetimedb (STDB module typecheck)"
cargo check --manifest-path server/spacetimedb/Cargo.toml --tests

say "STDB bindings drift gate (server/build.sh --check)"
bash server/build.sh --check

say "ci: OK"
