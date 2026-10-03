#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# tools/parity.sh — local driver for the plan-23 parity harness (23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md §3.6/§6.8).
#
# Mirrors the runnable subset of the nightly `parity-nightly.yml` on the dev host:
#   --check              structural registries + landed-row resolution + golden sha256
#   --suite smoke|gate|full   `parity run --suite <s>` over the file-backed catalog
#   --phase Pn           restrict `--suite` to scenarios whose phase is <= Pn
#   --gate Pn            write parity/reports/gate_Pn.json (phase evidence of record)
#   --mcp                headless half of `parity mcp-parity` (in-engine half is editor-gated)
#   --bench              validate the performance-budget coverage (`parity bench-gate`)
#   --soak <profile>     bounded soak run (`mid`/`stress`; full duration is nightly-owned)
#   --json               pass `--json` through to the harness
#
# The T1/T2 suites, checksum matrix, in-engine MCP capture and full soak durations
# stay on the self-hosted nightly runners (NUD-40/A); this script never fakes them.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$REPO_ROOT/client/rust/Cargo.toml"
OUT_DIR="${PARITY_OUT_DIR:-$REPO_ROOT/client/rust/target/parity}"
mkdir -p "$OUT_DIR"

run_headless() {
  cargo run -q --manifest-path "$MANIFEST" -p mind-headless -- "$@"
}

build_test_list() {
  local tests="$OUT_DIR/tests.txt"
  cargo test --manifest-path "$MANIFEST" -p mind-core     -- --list > "$OUT_DIR/tests-core.txt"
  cargo test --manifest-path "$MANIFEST" -p mind-headless -- --list > "$OUT_DIR/tests-headless.txt"
  cat "$OUT_DIR/tests-core.txt" "$OUT_DIR/tests-headless.txt" > "$tests"
  printf '%s\n' "$tests"
}

SUITE=""
PHASE=""
GATE=""
SOAK=""
JSON=0
DO_CHECK=0
DO_MCP=0
DO_BENCH=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --check) DO_CHECK=1; shift ;;
    --suite) SUITE="${2:?--suite needs smoke|gate|full}"; shift 2 ;;
    --phase) PHASE="${2:?--phase needs Pn}"; shift 2 ;;
    --gate) GATE="${2:?--gate needs Pn}"; shift 2 ;;
    --soak) SOAK="${2:?--soak needs mid|stress}"; shift 2 ;;
    --mcp) DO_MCP=1; shift ;;
    --bench) DO_BENCH=1; shift ;;
    --json) JSON=1; shift ;;
    -h|--help)
      sed -n '2,20p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0 ;;
    *) echo "parity.sh: unknown argument '$1'" >&2; exit 2 ;;
  esac
done

# No selector: default to the CI subset (structural check + smoke suite).
if [[ -z "$SUITE$GATE$SOAK" && $DO_CHECK -eq 0 && $DO_MCP -eq 0 && $DO_BENCH -eq 0 ]]; then
  DO_CHECK=1
  SUITE="smoke"
fi

rc=0
if ((DO_CHECK)); then
  TESTS="$(build_test_list)"
  run_headless parity check --tests "$TESTS" || rc=$?
  run_headless parity goldens || rc=$?
fi

if [[ -n "$SUITE" ]]; then
  args=(parity run --suite "$SUITE")
  [[ -n "$PHASE" ]] && args+=(--phase "$PHASE")
  ((JSON)) && args+=(--json)
  run_headless "${args[@]}" || rc=$?
fi

if [[ -n "$GATE" ]]; then
  args=(parity gate "$GATE" --out "$REPO_ROOT/parity/reports/gate_$GATE.json")
  ((JSON)) && args+=(--json)
  run_headless "${args[@]}" || rc=$?
fi

if ((DO_MCP)); then
  args=(parity mcp-parity --suite T0)
  ((JSON)) && args+=(--json)
  run_headless "${args[@]}" || rc=$?
fi

if ((DO_BENCH)); then
  args=(parity bench-gate)
  ((JSON)) && args+=(--json)
  run_headless "${args[@]}" || rc=$?
fi

if [[ -n "$SOAK" ]]; then
  args=(parity soak --profile "$SOAK")
  ((JSON)) && args+=(--json)
  run_headless "${args[@]}" || rc=$?
fi

exit "$rc"
