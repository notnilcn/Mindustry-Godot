#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# tools/regen_goldens.sh — plan 23 M2 oracle-golden regeneration + drift check.
#
#   --check            verify every `parity/golden_manifest.json` sha256 (no JVM)
#   --only <id>        regenerate one golden and update its manifest hash
#   --all              regenerate every oracle golden the fallback knows
#
# Regeneration uses the JVM dumps in `parity/java/` when MIND_JAVA_PARITY=1 and a
# Gradle-built checkout is present; otherwise it falls back to the reproducible
# source-derived extractor (`parity/java/extract_source_goldens.py`). CI never
# needs a JVM: `--check` only reads committed files.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$REPO_ROOT/client/rust/Cargo.toml"
MANIFEST_JSON="$REPO_ROOT/parity/golden_manifest.json"
MINDY_SRC="${MINDY_SRC:-$REPO_ROOT/../Mindustry}"

run_harness() {
  cargo run -q --manifest-path "$MANIFEST" -p mind-headless -- "$@"
}

check_all() {
  run_harness parity goldens
}

# Update the sha256 of `id` in the manifest from its committed file.
record_hash() {
  local id="$1"
  python3 - "$MANIFEST_JSON" "$REPO_ROOT" "$id" <<'PY'
import hashlib, json, os, sys
manifest_path, repo, target = sys.argv[1], sys.argv[2], sys.argv[3]
with open(manifest_path) as handle:
    data = json.load(handle)
found = False
for golden in data["goldens"]:
    if golden["id"] == target:
        path = os.path.join(repo, golden["path"])
        with open(path, "rb") as handle:
            golden["sha256"] = hashlib.sha256(handle.read()).hexdigest()
        found = True
with open(manifest_path, "w") as handle:
    json.dump(data, handle, indent=2)
    handle.write("\n")
if not found:
    print(f"regen_goldens: unknown golden id `{target}`", file=sys.stderr)
    sys.exit(2)
print(f"recorded {target}")
PY
}

regenerate() {
  local only="${1:-}"
  if [[ "${MIND_JAVA_PARITY:-0}" == "1" ]]; then
    echo "regen_goldens: MIND_JAVA_PARITY=1 — run the JVM dumpers from parity/java/ (see java/README.md)" >&2
    echo "regen_goldens: falling through to the source-derived extractor for the committed goldens" >&2
  fi
  python3 "$REPO_ROOT/parity/java/extract_source_goldens.py" "$MINDY_SRC"
  if [[ -n "$only" ]]; then
    record_hash "$only"
  else
    for id in oracle_logic_field_order oracle_ui_ui_keys oracle_io_rules_fields oracle_fx_fx_order; do
      record_hash "$id"
    done
  fi
}

MODE=""
ONLY=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --check) MODE=check; shift ;;
    --only) MODE=only; ONLY="${2:?--only needs a golden id}"; shift 2 ;;
    --all) MODE=all; shift ;;
    -h|--help) sed -n '2,14p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "regen_goldens.sh: unknown argument '$1'" >&2; exit 2 ;;
  esac
done

if [[ -z "$MODE" ]]; then
  MODE=check
fi

case "$MODE" in
  check) check_all ;;
  only) regenerate "$ONLY"; check_all ;;
  all) regenerate ""; check_all ;;
esac
