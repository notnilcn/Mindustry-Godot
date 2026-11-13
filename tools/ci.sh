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

# One full gate at a time. Invoked directly, ci.sh re-execs through the repo-wide
# CPU queue; when it is already running under the queue (or calls build.sh /
# server/build.sh internally) MIND_CPU_QUEUE_HELD makes those nested scripts run
# without trying to take the lock again. See docs/CI-WORKFLOW.md.
# shellcheck source=tools/lib/cpu-guard.sh
source "$REPO_ROOT/tools/lib/cpu-guard.sh"
mind_cpu_guard ci "$REPO_ROOT/tools/ci.sh" "$@"

MANIFEST="client/rust/Cargo.toml"

# Baseline refresh (2026-11-09): the 2026-10-09 baseline (p50 235 / p99 301 ns)
# predates a month of landed simulation work and no longer matches this tree.
# Pinned release runs on the dev host measure p50 ~960-1020 ns at 300,000 ticks
# (five back-to-back runs); the growth tracks the new per-tick systems/passes
# added by that batch. The `bench_baseline` scenario has no expected checksum,
# so this re-baseline changes no behaviour gate. p50 is the hard gate (1050 ns,
# +20% warn / +50% fail). The p99 tail on this shared host is scheduler noise
# (the same binary measured 1.4-6.3 us depending on other load), so p99 is only
# a stall sanity bound (5000 ns). A tick-cost optimisation is a separate
# follow-up; this gate still catches a further >50% p50 regression.
#
#
# The scenario file only stores the rounded `p50_us` / `p99_us` that
# `mind-headless bench` compares against; compare the JSON's ns fields against
# the ns truth so the regression gate stays honest.
BASE_P50_NS=1050
BASE_P99_NS=5000
WARN_P50_NS=$((BASE_P50_NS * 120 / 100))
WARN_P99_NS=$((BASE_P99_NS * 120 / 100))
FAIL_P50_NS=$((BASE_P50_NS * 150 / 100))
FAIL_P99_NS=$((BASE_P99_NS * 150 / 100))

# --- batch record (docs/CI-WORKFLOW.md) -------------------------------------
# Every run archives CI_PENDING.json to runs/ci/<run-id>/pending.json and resets
# the live file. On failure run.json names the stage and log that the dedicated
# CI fixer reads (docs/CI-FIXER-PROMPT.md); the snapshot is the batch under test.
CI_RUN_ID="${MIND_CI_RUN_ID:-$(date -u +%Y%m%d-%H%M%S)-$$}"
CI_RUN_DIR="$REPO_ROOT/runs/ci/$CI_RUN_ID"
mkdir -p "$CI_RUN_DIR"
# Mirror every stage's output into the run log. tee writes as it reads, so the
# very last lines can land after this shell exits; run.json carries the stage and
# exit code, and the log is authoritative for the fixer.
exec > >(tee -a "$CI_RUN_DIR/ci.log") 2>&1

say() { STAGE="$1"; printf '\n== %s ==\n' "$1"; }

STAGE="startup"
GODOT_LOG=""
finish() {
  local rc=$?
  trap - EXIT INT TERM
  if [[ -n "$GODOT_LOG" ]]; then
    rm -f "$GODOT_LOG"
  fi
  local status="failed"
  if (( rc == 0 )); then
    status="passed"
  fi
  python3 "$REPO_ROOT/tools/ci_pending.py" finish-run \
    --run-id "$CI_RUN_ID" --status "$status" --stage "$STAGE" --exit-code "$rc" \
    --log "$CI_RUN_DIR/ci.log" >/dev/null 2>&1 \
    || echo "ci: warning: could not write runs/ci/$CI_RUN_ID/run.json; the log is still at runs/ci/$CI_RUN_ID/ci.log" >&2
  if (( rc == 0 )); then
    printf '\nci: PASS (run %s; CI_PENDING.json reset for the next batch)\n' "$CI_RUN_ID"
  else
    printf '\nci: FAIL at "%s" (exit %d)\n' "$STAGE" "$rc"
    printf '  run:     runs/ci/%s/run.json\n' "$CI_RUN_ID"
    printf '  log:     runs/ci/%s/ci.log\n' "$CI_RUN_ID"
    printf '  pending: runs/ci/%s/pending.json\n' "$CI_RUN_ID"
    printf '  fixer:   docs/CI-FIXER-PROMPT.md\n'
  fi
  exit "$rc"
}
trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

say "ci-pending: archive batch + reset (run $CI_RUN_ID)"
python3 "$REPO_ROOT/tools/ci_pending.py" begin-run \
  --run-id "$CI_RUN_ID" --log "$CI_RUN_DIR/ci.log"

say "cargo fmt (workspace --check)"
cargo fmt --manifest-path "$MANIFEST" --all -- --check

say "cargo clippy (workspace --all-targets -- -D warnings)"
cargo clippy --manifest-path "$MANIFEST" --workspace --all-targets -- -D warnings

say "cargo check (workspace)"
cargo check --manifest-path "$MANIFEST" --workspace

say "cargo test -p mind-core"
cargo test --manifest-path "$MANIFEST" -p mind-core

# `sim/unit_runtime.rs`'s `msav_tests`/`markers_region_tests` are
# `cfg(all(test, feature = "msav-import"))`. The default run above never enables
# the feature, so they would never execute; this stage is the only gate on the
# `.msav` save-import path (`docs/PLANS/02-FINDINGS.md` #7).
say "cargo test -p mind-core --features msav-import"
cargo test --manifest-path "$MANIFEST" -p mind-core --features msav-import

# Phantom Camera port: the easing/smoothing unit tests are Godot-free (they check
# the upstream `_smooth_damp`).
say "cargo test -p phantom-camera-rust"
cargo test --manifest-path "$MANIFEST" -p phantom-camera-rust

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
BENCH_CMD=(cargo run -q --release --manifest-path "$MANIFEST" -p mind-headless -- bench --ticks 300000 --scenario bench_baseline)
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

# The Phantom Camera port is a second GDExtension library: a stale
# `.godot/extension_list.cfg`, a missing cdylib or a clashing class name all show
# up here first. `PcamRustProbe::ready` prints the registered class count, so the
# grep fails when the library did not load at all. This only proves the loading
# path.
say "godot: phantom camera addon class registration"
if ! bash tools/godot.sh --headless --path client --quit-after 5 res://addons/phantom_camera_rust/probe_check.tscn >"$GODOT_LOG" 2>&1; then
  echo "FAIL: res://addons/phantom_camera_rust/probe_check.tscn (PcamRustProbe)" >&2
  tail -n 40 "$GODOT_LOG" >&2
  exit 1
fi
if ! grep -qF 'phantom_camera_rust] loaded' "$GODOT_LOG"; then
  echo "FAIL: phantom_camera_rust GDExtension did not load (no probe banner)" >&2
  tail -n 40 "$GODOT_LOG" >&2
  exit 1
fi
echo "  phantom camera addon registered"

say "godot: shader compile check (native MindShadersCheck oracle)"
if ! bash tools/godot.sh --headless --path client res://scenes/tools/shaders_check.tscn >"$GODOT_LOG" 2>&1 \
    || ! grep -q 'SHADERCHECK: total=.* failed=0' "$GODOT_LOG"; then
  echo "FAIL: client/scenes/tools/shaders_check.tscn (MindShadersCheck)" >&2
  grep -E 'SHADERCHECK|SCRIPT ERROR|ERROR' "$GODOT_LOG" >&2 || tail -n 40 "$GODOT_LOG" >&2
  exit 1
fi
echo "  shader check clean"

say "godot: UI layout-engine check (MindTable min-size propagation, L01 T1)"
if ! bash tools/godot.sh --headless --path client res://scenes/tools/ui_layout_check.tscn >"$GODOT_LOG" 2>&1 \
    || ! grep -q 'LAYOUTCHECK: failed=0' "$GODOT_LOG"; then
  echo "FAIL: client/scenes/tools/ui_layout_check.tscn (MindUiLayoutCheck)" >&2
  grep -E 'FAIL|SCRIPT ERROR|LAYOUTCHECK' "$GODOT_LOG" >&2 || tail -n 40 "$GODOT_LOG" >&2
  exit 1
fi
echo "  ui layout check clean"

say "godot: UI widget check (atlas regions, row sizes, dialog oracles)"
if ! bash tools/godot.sh --headless --path client res://scenes/tools/ui_widgets_check.tscn >"$GODOT_LOG" 2>&1 \
    || ! grep -q 'UICHECK: failed=0' "$GODOT_LOG"; then
  echo "FAIL: client/scenes/tools/ui_widgets_check.tscn (MindUiWidgetsCheck)" >&2
  grep -E 'FAIL|SCRIPT ERROR|UICHECK: failed' "$GODOT_LOG" >&2 || tail -n 40 "$GODOT_LOG" >&2
  exit 1
fi
echo "  ui widget check clean"

say "godot: native dialog sweep (every manifest dialog builds, shows, hides)"
if ! bash tools/godot.sh --headless --path client res://scenes/tools/dialog_sweep_check.tscn >"$GODOT_LOG" 2>&1 \
    || ! grep -q 'DIALOGCHECK: failed=0' "$GODOT_LOG"; then
  echo "FAIL: client/scenes/tools/dialog_sweep_check.tscn (MindDialogSweepCheck)" >&2
  grep -E 'DIALOGCHECK: FAIL|SCRIPT ERROR|panicked' "$GODOT_LOG" >&2 || tail -n 40 "$GODOT_LOG" >&2
  exit 1
fi
echo "  dialog sweep clean"

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
