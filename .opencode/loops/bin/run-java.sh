#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# Launch the Java Mindustry reference for this parity loop with display and
# per-loop user data (settings/saves) isolation. The real home's Mindustry
# data dir is copied in once, so each loop starts from the normalized
# settings without sharing the live one. The display comes from the loop's
# dedicated weston + Xwayland compositor (GPU), started when down; a missing
# weston/Xwayland or PARITY_JAVA_DISPLAY=x11 falls back to Xvfb (software GL).
#
# Usage: run-java.sh [extra java args...]
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=../lib/loop-vars.sh
. "$script_dir/../lib/loop-vars.sh"
parity_loop_vars "${PARITY_LOOP:-1}"

# The display must exist before the client starts; create it when missing.
parity_ensure_display

jar="${MINDY_JAR:-$(dirname "$PARITY_MAIN")/Mindustry/desktop/build/libs/Mindustry.jar}"
[ -f "$jar" ] || { echo "run-java: reference jar not found: $jar" >&2; exit 1; }

java_bin="$(parity_loop_java)" || { echo "run-java: no JDK 17 found (set JAVA17_HOME)" >&2; exit 1; }

loop_home="$PARITY_LOOP_DIR/home"
seed_src="${MINDY_HOME_SEED:-$HOME/.local/share/Mindustry}"
if [ ! -d "$loop_home/.local/share/Mindustry" ] && [ -d "$seed_src" ]; then
  echo "[parity-loop $PARITY_LOOP] seeding Java data dir from $seed_src" >&2
  mkdir -p "$loop_home/.local/share"
  cp -a "$seed_src" "$loop_home/.local/share/Mindustry"
fi
mkdir -p "$loop_home" "$PARITY_LOOP_DIR"/{xdg-data,xdg-config,xdg-cache,logs}

echo "[parity-loop $PARITY_LOOP] java: display=$PARITY_DISPLAY java=$java_bin home=$loop_home" >&2
exec env \
  DISPLAY="$PARITY_DISPLAY" \
  HOME="$loop_home" \
  XDG_DATA_HOME="$PARITY_LOOP_DIR/xdg-data" \
  XDG_CONFIG_HOME="$PARITY_LOOP_DIR/xdg-config" \
  XDG_CACHE_HOME="$PARITY_LOOP_DIR/xdg-cache" \
  "$java_bin" -jar "$jar" "$@"
