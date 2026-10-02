#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Runs parity/java/DumpContent.java against a Gradle-built upstream checkout and
# writes the JVM golden (plan 02 §6.4, NUD-10). Never writes into the upstream
# repos; the classpath is read from build outputs and the Gradle cache.
#
# Usage: run.sh [mindustry_src] [arc_src] [out_json]
#   mindustry_src      defaults to /mnt/c/Users/Clinton/g/code_examples/Mindustry
#   arc_src            defaults to /mnt/c/Users/Clinton/g/code_examples/Arc
#   out_json           defaults to parity/java/jvm_golden_content.json
#   MINDY_JVM_CP       overrides the discovered classpath
#   MINDY_VERSION      label emitted in the golden (default v146)
#
# Prerequisite (in a *copy* of the checkouts, not the read-only sources; the
# Gradle build generates core/assets/{locales,version.properties} and writes
# build output, so the read-only upstream tree is left untouched):
#   sed -i 's/ --illegal-access=permit//' Mindustry/gradle.properties  # flag is gone in JDK 17
#   ./gradlew --no-daemon :core:jar :tests:testClasses
# Run from `Mindustry/core/assets`: Arc resolves internal files (bundle,
# version.properties) relative to the working directory in headless mode:
#   cd Mindustry/core/assets && bash <this script> <Mindustry> <Arc> <out>
set -euo pipefail

MINDY_SRC=${1:-/mnt/c/Users/Clinton/g/code_examples/Mindustry}
ARC_SRC=${2:-/mnt/c/Users/Clinton/g/code_examples/Arc}
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
OUT=${3:-"$SCRIPT_DIR/jvm_golden_content.json"}

if [[ -n "${MINDY_JVM_CP:-}" ]]; then
  CP=$MINDY_JVM_CP
else
  parts=()
  while IFS= read -r dir; do
    parts+=("$dir")
  done < <(find "$MINDY_SRC" "$ARC_SRC" -type d \
    \( -path '*/build/classes/java/main' -o -path '*/build/classes/java/test' \
       -o -path '*/build/resources/main' -o -path '*/build/resources/test' \) 2>/dev/null)
  while IFS= read -r jar; do
    parts+=("$jar")
  done < <(find "$MINDY_SRC" "$ARC_SRC" -path '*/build/libs/*.jar' 2>/dev/null)
  if [[ -d "$HOME/.gradle/caches/modules-2" ]]; then
    while IFS= read -r jar; do
      parts+=("$jar")
    done < <(find "$HOME/.gradle/caches/modules-2" -name '*.jar' 2>/dev/null)
  fi
  CP=$(IFS=:; echo "${parts[*]}")
fi

if [[ -z "$CP" ]]; then
  echo "run.sh: empty classpath; build the upstream checkout first" >&2
  exit 2
fi

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
javac -cp "$CP" -d "$TMP" "$SCRIPT_DIR/DumpContent.java"
java -cp "$CP:$TMP" DumpContent "$OUT" "${MINDY_VERSION:-v146}"
