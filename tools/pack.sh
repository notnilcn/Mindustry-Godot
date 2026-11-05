#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Runs the offline asset pipeline (plan 03). Examples:
#   tools/pack.sh migrate --from "$MIND_UPSTREAM" --to .
#   tools/pack.sh pack            # full pack (M1+)
# All arguments are forwarded to `mind-tools`.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Serialize with every other CPU-heavy task (docs/CI-WORKFLOW.md).
# shellcheck source=tools/lib/cpu-guard.sh
source "$REPO_ROOT/tools/lib/cpu-guard.sh"
mind_cpu_guard pack "$REPO_ROOT/tools/pack.sh" "$@"

cargo run \
  --manifest-path "$REPO_ROOT/client/rust/Cargo.toml" \
  --release -p mind-tools -- "$@"
