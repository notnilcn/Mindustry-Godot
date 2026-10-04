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

# Baseline refresh (2026-10-02, F2+05/03 join): plan 05 replaced the P0
# placeholder pump with the real fixed 60 Hz `sim::schedule`, so `bench_baseline`
# now pays the full per-tick schedule cost. Re-measured release truth on the
# merged tree is p50 130 ns / p99 301 ns (was 101/240 at M3). The scenario file
# only stores the rounded `p50_us` / `p99_us` that `mind-headless bench` compares
# against; compare the JSON's ns fields against the ns truth so the regression
# gate stays honest.
BASE_P50_NS=130
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
set +e
BENCH_JSON="$(cargo run -q --release --manifest-path "$MANIFEST" -p mind-headless -- bench --ticks 100000 --scenario bench_baseline)"
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
if grep -nE 'SCRIPT ERROR|SHADER ERROR|Parse Error|Failed to load|Cannot open|Can.t open|ERROR:' "$GODOT_LOG" >&2; then
  echo "FAIL: godot headless import logged parse/load errors (above)" >&2
  exit 1
fi
echo "  godot headless import clean"

say "boundary: mind-core must stay godot-free and tokio-free"
# NOTE: the repo path itself contains "mindustry-godot", so a naive `grep godot`
# on the default tree (which prints the root path) always matches. `--prefix none`
# puts one package name per line; anchor the match to the package name.
if cargo tree --manifest-path "$MANIFEST" -p mind-core --prefix none | grep -Eq '^(godot|tokio)( |$)'; then
  echo "FAIL: mind-core dependency tree contains godot/tokio" >&2
  exit 1
fi
if rg -n -g '*.rs' 'use godot|use tokio' client/rust/mind-core; then
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
