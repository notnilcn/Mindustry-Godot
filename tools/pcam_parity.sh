#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
#
# pcam_parity.sh - parity harness for the Rust Phantom Camera port.
#
# Records the observable camera pipeline of the same demo scenes twice and diffs
# the two JSONL streams with tools/pcam_compare.py:
#
#   --baseline   install the GDScript oracle from parity/pcam_oracle/phantom_camera
#                into client/addons/phantom_camera, disable the Rust extension,
#                record goldens into parity/pcam_golden/<scene>.jsonl, restore.
#   --candidate  record the Rust addon's ported scenes into
#                client/rust/target/pcam/<scene>.jsonl.
#   --compare    run pcam_compare.py over goldens vs candidate.
#   (default)    --candidate then --compare.
#
# Options: --scene <substr>, --frames N (default 240), --json [PATH] (machine
#          readable compare report, - for stdout), --main-scene <res://path>
#          with --out <absolute jsonl path>, -v/--verbose, -h/--help.
#
# Environment: GODOT_BIN (Godot 4.7 binary), PCAM_SKIP_IMPORT=1 (skip the
# `godot --editor --quit` import/refresh), PCAM_KEEP_GENERATED=1 (keep the
# generated scenes/observer for debugging).
#
# Exit codes (parity/AGENTS.md): 0 pass, 1 mismatch, 2 usage/IO. A scene that
# cannot be recorded because its classes are not registered is reported SKIP, not
# a failure: the Rust port is being written while the harness runs.
#
# --- why the recorder is injected through a generated scene ------------------
#
# The demo scenes are the thing under test, but they contain no recorder and we
# must not edit them. The runner therefore writes a temporary scene that instances
# the demo scene as a child of a plain `Node` root ("Harness") and puts the
# recorder beside it as a sibling:
#
#   [node name="Harness" type="Node"]
#   [node name="Demo" parent="." instance=ExtResource("<demo scene>")]
#   [node name="Recorder" type="PcamRecorder" parent="."]
#
# A `.tscn` file is data, not code, so the `# code-instantiated:` convention (for
# nodes created in Rust/GDScript) does not apply here. Generating the wrapper is
# what lets one command record every demo scene without touching them;
# dev/record.tscn stays as the minimal hand-runnable recorder scene. The other
# form works too: `--main-scene res://... --out <abs path>` runs a scene that
# already contains a PcamRecorder and validates the file it wrote.
#
# --- why --baseline observes with GDScript -----------------------------------
#
# PcamRecorder is registered by the very GDExtension the baseline must disable:
# the oracle's GDScript `class_name PhantomCamera2D` and the native class of the
# same name cannot be imported into one project, so the Rust library has to be off
# while the oracle runs. The baseline leg therefore uses a generated GDScript twin
# of src/dev/recorder.rs (dev/_generated/pcam_observer.gd, written and deleted by
# this script, never committed) that emits the identical `pcam-record/v1` schema.
# The schema is the contract between the legs: pcam_compare.py compares schema,
# dimension and class-name evidence, so an observer divergence shows up as a
# mismatch instead of passing silently.
#
# --- recording command -------------------------------------------------------
#
#   <godot> --headless --path <client> --fixed-fps 60 --quit-after <N+120> \
#       res://addons/phantom_camera_rust/dev/_generated/<scene>.tscn
#
# --fixed-fps 60 makes `delta` exactly 1/60 and removes real-time
# synchronization, so two runs are comparable frame by frame; --quit-after is only
# a watchdog (the recorder quits itself), so a scene that fails to start cannot
# hang the harness. The recorder prints "PCAMREC: wrote N frames to <path>" and
# exits 0 on success, "PCAMREC: <reason>" and exits 2 on failure.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
# Paths handed to Godot must be in the *host* form. Native Linux shells already
# are; Git Bash (`pwd -W`) and WSL (`wslpath -w`) must hand a Windows Godot a
# C:/... path instead of /c/... or /mnt/c/...
if REPO_ROOT_WIN="$(cd -- "$REPO_ROOT" && pwd -W 2>/dev/null)"; then
    :
elif command -v wslpath >/dev/null 2>&1 && REPO_ROOT_WIN="$(wslpath -w "$REPO_ROOT" 2>/dev/null)"; then
    REPO_ROOT_WIN="${REPO_ROOT_WIN//\\//}"
else
    REPO_ROOT_WIN="$REPO_ROOT"
fi
# `user://`-free absolute path for one repo-relative file, in host form.
host_path() { printf '%s/%s' "$REPO_ROOT_WIN" "${1#"$REPO_ROOT/"}"; }

CLIENT_DIR="$REPO_ROOT/client"
CLIENT_DIR_HOST="$REPO_ROOT_WIN/client"
ADDONS_DIR="$CLIENT_DIR/addons"
PROJECT_FILE="$CLIENT_DIR/project.godot"
EXTENSION_LIST="$CLIENT_DIR/.godot/extension_list.cfg"

ORACLE_SRC="$REPO_ROOT/parity/pcam_oracle/phantom_camera"
ORACLE_INSTALL="$ADDONS_DIR/phantom_camera"
ORACLE_MANAGER_RES="res://addons/phantom_camera/scripts/managers/phantom_camera_manager.gd"
ORACLE_EXAMPLES="$ORACLE_INSTALL/examples/example_scenes"
ORACLE_EXAMPLES_RES="res://addons/phantom_camera/examples/example_scenes"

RUST_ADDON="$ADDONS_DIR/phantom_camera_rust"
RUST_EXTENSION="$RUST_ADDON/phantom_camera_rust.gdextension"
RUST_EXTENSION_RES="res://addons/phantom_camera_rust/phantom_camera_rust.gdextension"
RUST_MANAGER_SCENE="$RUST_ADDON/phantom_camera_manager.tscn"
RUST_MANAGER_RES="res://addons/phantom_camera_rust/phantom_camera_manager.tscn"
RUST_EXAMPLES="$RUST_ADDON/examples/example_scenes"
RUST_EXAMPLES_RES="res://addons/phantom_camera_rust/examples/example_scenes"
GENERATED_DIR="$RUST_ADDON/dev/_generated"
GENERATED_RES="res://addons/phantom_camera_rust/dev/_generated"
OBSERVER_GD="$GENERATED_DIR/pcam_observer.gd"
OBSERVER_RES="$GENERATED_RES/pcam_observer.gd"

GOLDEN_DIR="$REPO_ROOT/parity/pcam_golden"
CANDIDATE_DIR="$CLIENT_DIR/rust/target/pcam"
LOG_DIR="$CANDIDATE_DIR/_logs"
COMPARE_TOOL="$SCRIPT_DIR/pcam_compare.py"

FRAMES=240
SCENE_FILTER=""
MAIN_SCENE=""
MAIN_SCENE_OUT=""
JSON_REPORT=""
VERBOSE=0

# --- run state (all of it is undone by restore_tree) -------------------------
PROJECT_BACKUP=""
EXTENSION_LIST_BACKUP=""
ORACLE_MOVED=0
EXTENSION_DISABLED=0
GIT_STATUS_TAKEN=0
GIT_STATUS_BACKUP=""
RECORDED_COUNT=0
RECORDING_LEGS=0

# --- logging -----------------------------------------------------------------
# Progress goes to stderr: `record_generated` runs inside a command substitution,
# so anything it wrote to stdout would end up inside the results TSV.
log() { printf '%s\n' "$*" >&2; }
verbose() {
    if [ "$VERBOSE" -eq 1 ]; then
        printf '  %s\n' "$*" >&2
    fi
}
warn() { printf 'pcam_parity: warning: %s\n' "$*" >&2; }
die() {
    printf 'pcam_parity: error: %s\n' "$*" >&2
    exit 2
}
usage() {
    awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "${BASH_SOURCE[0]}"
}

# --- restore -----------------------------------------------------------------
restore_tree() {
    local status=$?
    trap - EXIT INT TERM
    # Restoration must never abort halfway: keep going and report at the end.
    set +e

    if [ -n "$PROJECT_BACKUP" ] && [ -f "$PROJECT_BACKUP" ]; then
        mv -f "$PROJECT_BACKUP" "$PROJECT_FILE"
        PROJECT_BACKUP=""
    fi
    if [ -n "$EXTENSION_LIST_BACKUP" ] && [ -f "$EXTENSION_LIST_BACKUP" ]; then
        mkdir -p -- "$(dirname -- "$EXTENSION_LIST")"
        mv -f "$EXTENSION_LIST_BACKUP" "$EXTENSION_LIST"
        EXTENSION_LIST_BACKUP=""
    fi
    if [ "$EXTENSION_DISABLED" -eq 1 ] && [ -f "$RUST_EXTENSION.disabled" ]; then
        mv -f "$RUST_EXTENSION.disabled" "$RUST_EXTENSION"
        EXTENSION_DISABLED=0
    fi
    if [ "$ORACLE_MOVED" -eq 1 ] && [ -d "$ORACLE_INSTALL" ]; then
        mkdir -p -- "$(dirname -- "$ORACLE_SRC")"
        if [ -d "$ORACLE_SRC" ]; then
            # Never nest: `mv dir existing-dir` would bury the tree one level deeper.
            cp -a -- "$ORACLE_INSTALL/." "$ORACLE_SRC/" && rm -rf -- "$ORACLE_INSTALL"
        else
            mv -f "$ORACLE_INSTALL" "$ORACLE_SRC"
        fi
        ORACLE_MOVED=0
    fi
    if [ "${PCAM_KEEP_GENERATED:-0}" = 1 ]; then
        if [ -d "$GENERATED_DIR" ]; then
            log "kept generated files in ${GENERATED_DIR#"$REPO_ROOT/"}"
        fi
    elif [ -d "$GENERATED_DIR" ]; then
        rm -rf -- "$GENERATED_DIR"
    fi

    if [ "$GIT_STATUS_TAKEN" -eq 1 ] && [ -n "$GIT_STATUS_BACKUP" ] && [ -f "$GIT_STATUS_BACKUP" ]; then
        local after difference critical
        after="$(git -C "$REPO_ROOT" status --porcelain=v1 -uall 2>/dev/null | sort || true)"
        difference="$(diff --unchanged-line-format='' --old-line-format='- %L' --new-line-format='+ %L' \
            "$GIT_STATUS_BACKUP" <(printf '%s\n' "$after") 2>/dev/null | grep -vE '^[-+] .. ' || true)"
        # Paths this script is allowed to touch, and only for the duration of the
        # run: if one of them is still dirty the restore is incomplete.
        critical="$(printf '%s\n' "$difference" | grep -E '^[-+] .. (client/addons/phantom_camera/|client/addons/phantom_camera_rust/phantom_camera_rust\.gdextension|client/addons/phantom_camera_rust/dev/_generated/|client/project\.godot|client/\.godot/extension_list\.cfg)' || true)"
        # Outputs the harness is meant to produce, plus anything another agent is
        # concurrently editing (this checkout is shared), are not restore failures.
        difference="$(printf '%s\n' "$difference" |
            grep -vE '^[-+] .. (parity/pcam_golden/|client/rust/target/|client/bin/|client/\.godot/)' || true)"
        if [ -n "$critical" ]; then
            warn "the working tree was not restored exactly:"
            printf '%s\n' "$critical" >&2
            if [ "$status" -eq 0 ]; then
                status=2
            fi
        fi
        if [ -n "$difference" ]; then
            warn "unrelated paths changed during the run (shared checkout, ignoring):"
            printf '%s\n' "$difference" >&2
        fi
        rm -f -- "$GIT_STATUS_BACKUP"
    fi

    exit "$status"
}

begin_run() {
    trap restore_tree EXIT INT TERM
    mkdir -p -- "$LOG_DIR"
    if git -C "$REPO_ROOT" rev-parse --git-dir >/dev/null 2>&1; then
        GIT_STATUS_BACKUP="$(mktemp "${TMPDIR:-/tmp}/pcam_status.XXXXXX")"
        GIT_STATUS_TAKEN=1
        git -C "$REPO_ROOT" status --porcelain=v1 -uall 2>/dev/null | sort >"$GIT_STATUS_BACKUP" || true
    fi
}

# --- godot -------------------------------------------------------------------
resolve_godot() {
    local candidate
    for candidate in "${GODOT_BIN:-}" godot4 godot4-mono godot; do
        [ -n "$candidate" ] || continue
        if command -v "$candidate" >/dev/null 2>&1; then
            command -v "$candidate"
            return 0
        fi
    done
    # Windows checkout reached through Git Bash/WSL: godot4 may only be on the
    # Windows PATH, so ask `where.exe` before giving up.
    if command -v where.exe >/dev/null 2>&1; then
        candidate="$(where.exe godot4 2>/dev/null | tr -d '\r' | head -n 1 || true)"
        if [ -n "$candidate" ] && command -v wslpath >/dev/null 2>&1; then
            candidate="$(wslpath -u "$candidate" 2>/dev/null || printf '%s' "$candidate")"
        fi
        if [ -n "$candidate" ] && [ -x "$candidate" ]; then
            printf '%s' "$candidate"
            return 0
        fi
    fi
    return 1
}

GODOT=""
require_godot() {
    if [ -z "$GODOT" ]; then
        GODOT="$(resolve_godot || true)"
        [ -n "$GODOT" ] || die "no Godot binary found (set GODOT_BIN to a Godot 4.7 executable)"
    fi
    verbose "godot: $GODOT"
}

refresh_extension_list() {
    require_godot
    if [ "${PCAM_SKIP_IMPORT:-0}" = 1 ]; then
        warn "PCAM_SKIP_IMPORT=1: not running the editor import (extension list patched directly)"
        return 0
    fi
    log "importing the project (godot --headless --editor --quit) to refresh the extension list"
    if ! "$GODOT" --headless --path "$CLIENT_DIR_HOST" --editor --quit >"$LOG_DIR/import.log" 2>&1; then
        warn "editor import returned non-zero; see ${LOG_DIR#"$REPO_ROOT/"}/import.log"
    fi
}

backup_extension_list() {
    [ -f "$EXTENSION_LIST" ] || return 0
    if [ -z "$EXTENSION_LIST_BACKUP" ]; then
        EXTENSION_LIST_BACKUP="$(mktemp "${TMPDIR:-/tmp}/pcam_extlist.XXXXXX")"
        cp -- "$EXTENSION_LIST" "$EXTENSION_LIST_BACKUP"
    fi
}

set_extension_listed() { # $1 = 1 list the Rust extension, 0 remove it
    backup_extension_list
    mkdir -p -- "$(dirname -- "$EXTENSION_LIST")"
    [ -f "$EXTENSION_LIST" ] || : >"$EXTENSION_LIST"
    local filtered
    filtered="$(grep -vxF "$RUST_EXTENSION_RES" "$EXTENSION_LIST" || true)"
    if [ "$1" -eq 1 ]; then
        printf '%s\n' "$filtered" "$RUST_EXTENSION_RES" | sed '/^$/d' >"$EXTENSION_LIST"
    else
        printf '%s\n' "$filtered" | sed '/^$/d' >"$EXTENSION_LIST"
    fi
    verbose "extension list: Rust extension listed=$1"
}

# --- project.godot autoload --------------------------------------------------
ensure_manager_autoload() { # $1 = manager res:// path
    local res="$1" line tmp
    line="PhantomCameraManager=\"*$res\""

    if grep -qxF "$line" "$PROJECT_FILE"; then
        verbose "manager autoload already points at $res"
        return 0
    fi
    if grep -qF 'PhantomCameraManager=' "$PROJECT_FILE"; then
        warn "project.godot declares a different PhantomCameraManager autoload; rewriting it for this run"
    fi

    if [ -z "$PROJECT_BACKUP" ]; then
        PROJECT_BACKUP="$(mktemp "${TMPDIR:-/tmp}/pcam_project.XXXXXX")"
        cp -- "$PROJECT_FILE" "$PROJECT_BACKUP"
    fi
    tmp="$(mktemp "${TMPDIR:-/tmp}/pcam_project_new.XXXXXX")"
    awk -v line="$line" '
        BEGIN { inserted = 0; in_autoload = 0 }
        /^\[autoload\]$/ { print; print line; inserted = 1; in_autoload = 1; next }
        /^\[/ { in_autoload = 0 }
        { if (in_autoload && /^PhantomCameraManager=/) next; print }
        END { if (!inserted) { print ""; print "[autoload]"; print ""; print line } }
    ' "$PROJECT_FILE" >"$tmp"
    mv -f "$tmp" "$PROJECT_FILE"
    log "added [autoload] PhantomCameraManager=\"*$res\" (restored on exit)"
}

# --- scene discovery ---------------------------------------------------------
discover_scenes() { # $1 = example_scenes dir, $2 = its res:// root
    local root="$1" res_root="$2"
    [ -d "$root" ] || return 0
    find "$root" -type f -name '*.tscn' ! -path '*/sub_scenes/*' -print0 2>/dev/null |
        sort -z |
        while IFS= read -r -d '' file; do
            local name="${file##*/}"
            printf '%s\t%s\n' "${name%.tscn}" "${res_root}/${file#"$root"/}"
        done
}

scene_require_class() { # $1 = demo scene res path
    case "$1" in
    */3D/*) printf 'PhantomCamera3D' ;;
    */2D/*) printf 'PhantomCamera2D' ;;
    *) printf '' ;;
    esac
}

# --- generated recorder scene ------------------------------------------------
write_wrapper_scene() { # $1 = name, $2 = demo res, $3 = out jsonl (native path), $4 = leg
    local name="$1" demo="$2" out="$3" leg="$4" require target="$GENERATED_DIR/$1.tscn"
    require="$(scene_require_class "$demo")"
    mkdir -p -- "$GENERATED_DIR"

    {
        printf '[gd_scene format=3]\n\n'
        printf '[ext_resource type="PackedScene" path="%s" id="1_demo"]\n' "$demo"
        if [ "$leg" = baseline ]; then
            printf '[ext_resource type="Script" path="%s" id="2_recorder"]\n\n' "$OBSERVER_RES"
            printf '[node name="Harness" type="Node"]\n\n'
            printf '[node name="Demo" parent="." instance=ExtResource("1_demo")]\n\n'
            printf '[node name="Recorder" type="Node" parent="."]\n'
            printf 'script = ExtResource("2_recorder")\n'
        else
            printf '\n'
            printf '[node name="Harness" type="Node"]\n\n'
            printf '[node name="Demo" parent="." instance=ExtResource("1_demo")]\n\n'
            printf '[node name="Recorder" type="PcamRecorder" parent="."]\n'
        fi
        printf 'out_path = "%s"\n' "$out"
        printf 'frames = %d\n' "$FRAMES"
        printf 'camera_path = NodePath("")\n'
        printf 'host_path = NodePath("")\n'
        printf 'require_class = "%s"\n' "$require"
    } >"$target"
}

# The GDScript twin of src/dev/recorder.rs: same schema, same tapped signals, same
# field order and 9-decimal floats. See the script header for why it exists.
write_observer() {
    mkdir -p -- "$GENERATED_DIR"
    cat >"$OBSERVER_GD" <<'GDSCRIPT'
# SPDX-License-Identifier: GPL-3.0-only
#
# GENERATED by tools/pcam_parity.sh --baseline: GDScript twin of
# client/rust/phantom-camera-rust/src/dev/recorder.rs. Do not edit, do not commit.
# It exists because the Rust recorder's library has to be disabled while the
# GDScript oracle runs (both declare PhantomCamera2D). Schema: pcam-record/v1.
extends Node

const SCHEMA := "pcam-record/v1"
const TAPPED_SIGNALS := [
	"became_active",
	"became_inactive",
	"tween_started",
	"is_tweening",
	"tween_completed",
	"dead_zone_changed",
	"dead_zone_reached",
	"noise_emitted",
]

@export var out_path: String = "user://pcam_record.jsonl"
@export var frames: int = 240
@export var camera_path: NodePath
@export var host_path: NodePath
@export var require_class: String = ""

var _file: FileAccess = null
var _frame: int = 0
var _written: int = 0
var _dims: int = 2
var _camera: Node = null
var _host: Node = null
var _pcam: Node = null
var _linked: Node = null
var _sinks: Array = []
var _pending: Array = []
var _done: bool = false
var _out_display: String = ""


func _ready() -> void:
	if Engine.is_editor_hint():
		_done = true
		return
	var problem := _start()
	if problem != "":
		_abort(problem)


func _process(delta: float) -> void:
	if _done or Engine.is_editor_hint():
		return

	var active := _active_pcam()
	var same := false
	if is_instance_valid(active) and is_instance_valid(_pcam):
		same = active == _pcam
	elif not is_instance_valid(active) and not is_instance_valid(_pcam):
		same = true
	if not same:
		_pcam = active
		var problem := _check_require_class()
		if problem != "":
			_abort(problem)
			return
		_bind_pcam()

	_write_line(_frame_line(delta))
	_frame += 1
	_written += 1

	if frames > 0 and _written >= frames:
		_finish()


func _start() -> String:
	var global := ProjectSettings.globalize_path(out_path)
	_out_display = global
	var directory := global.get_base_dir()
	if directory != "" and not DirAccess.dir_exists_absolute(directory):
		var make_error := DirAccess.make_dir_recursive_absolute(directory)
		if make_error != OK:
			return "cannot create output directory %s: %s" % [directory, make_error]

	_file = FileAccess.open(global, FileAccess.WRITE)
	if _file == null:
		return "cannot open output file %s: %s" % [global, FileAccess.get_open_error()]

	var camera := _resolve_camera()
	if camera.is_empty():
		return "no Camera2D/Camera3D under the scene root"
	_camera = camera[0]
	_dims = camera[1]
	_host = _resolve_host()
	_pcam = _active_pcam()

	var problem := _check_require_class()
	if problem != "":
		return problem
	_bind_pcam()
	_write_line(_header_line())
	return ""


func _resolve_camera() -> Array:
	if camera_path != NodePath(""):
		var node := get_node_or_null(camera_path)
		if node == null:
			return []
		if node is Camera3D:
			return [node, 3]
		if node is Camera2D:
			return [node, 2]
		return []

	var root := _scene_root()
	if root == null:
		return []
	for candidate in _descendants(root):
		if candidate is Camera3D:
			return [candidate, 3]
		if candidate is Camera2D:
			return [candidate, 2]
	return []


func _resolve_host() -> Node:
	if host_path != NodePath(""):
		return get_node_or_null(host_path)
	var root := _scene_root()
	if root == null:
		return null
	for candidate in _descendants(root):
		if candidate.has_method("get_active_pcam"):
			return candidate
	return null


func _scene_root() -> Node:
	var tree := get_tree()
	if tree == null:
		return null
	var current := tree.current_scene
	if current != null:
		return current
	var node: Node = self
	while node.get_parent() != null:
		node = node.get_parent()
	return node


func _descendants(root: Node) -> Array:
	var found: Array = []
	for child in root.get_children():
		found.append(child)
		found.append_array(_descendants(child))
	return found


func _varcall_class(node: Node) -> String:
	if not is_instance_valid(node):
		return ""
	return str(node.call("get_class"))


func _active_pcam() -> Node:
	if not is_instance_valid(_host):
		return null
	if not _host.has_method("get_active_pcam"):
		return null
	var value: Variant = _host.call("get_active_pcam")
	if value is Node and is_instance_valid(value):
		return value
	return null


func _check_require_class() -> String:
	if require_class == "":
		return ""
	if is_instance_valid(_pcam):
		var found := _varcall_class(_pcam)
		if found != require_class:
			return "require_class=%s but the active Phantom Camera is '%s'" % [require_class, found]
		return ""
	var candidate := _fallback_pcam()
	if candidate != null:
		return ""
	return "no node of class '%s' in the scene tree (classes not registered?)" % require_class


func _fallback_pcam() -> Node:
	if require_class == "":
		return null
	var root := _scene_root()
	if root == null:
		return null
	for candidate in _descendants(root):
		if _varcall_class(candidate) == require_class:
			return candidate
	return null


func _bind_pcam() -> void:
	_unbind_pcam()
	if not is_instance_valid(_pcam):
		return
	for signal_name in TAPPED_SIGNALS:
		if not _pcam.has_signal(signal_name):
			continue
		var sink := func(_a: Variant = null, _b: Variant = null, _c: Variant = null, _d: Variant = null) -> void:
			_pending.append("%s@%d" % [signal_name, _frame])
		_pcam.connect(signal_name, sink)
		_sinks.append([signal_name, sink])
	_linked = _pcam


func _unbind_pcam() -> void:
	if is_instance_valid(_linked):
		for entry in _sinks:
			var signal_name: String = entry[0]
			var sink: Callable = entry[1]
			if _linked.is_connected(signal_name, sink):
				_linked.disconnect(signal_name, sink)
	_sinks.clear()
	_linked = null


func _header_line() -> String:
	var version := Engine.get_version_info()
	# The host activates its first Phantom Camera on its first frame, after this
	# node's _ready(); fall back to the node that satisfies require_class so the
	# header names the same class on both legs.
	var pcam: Node = _pcam
	if not is_instance_valid(pcam):
		pcam = _fallback_pcam()
	return ('{"kind":"header","schema":"%s","observer":"gdscript","godot":"%s","godot_hex":%d,'
		+ '"max_fps":%d,"dims":%d,"camera_class":"%s","camera_node":"%s","host_class":"%s",'
		+ '"host_node":"%s","pcam_class":"%s","pcam_node":"%s","require_class":"%s","frames":%d,'
		+ '"out":"%s"}') % [
		SCHEMA,
		_jstr(str(version["string"])),
		int(version["hex"]),
		Engine.max_fps,
		_dims,
		_jstr(_varcall_class(_camera)),
		_node_path(_camera),
		_jstr(_varcall_class(_host)),
		_node_path(_host),
		_jstr(_varcall_class(pcam)),
		_node_path(pcam),
		_jstr(require_class),
		frames,
		_jstr(_out_display),
	]


func _frame_line(delta: float) -> String:
	var telemetry := _camera_telemetry()
	var line := '{"f":%d,"d":%s,"pos":%s,"rot":%s,"scale_zoom":%s,"pcam":%s,"signals":%s}' % [
		_frame,
		_jf(delta),
		_jfloats(telemetry[0]),
		_jfloats(telemetry[1]),
		_jfloats(telemetry[2]),
		_pcam_line(),
		_jstrings(_pending),
	]
	# Signals are flushed with this line and then dropped, exactly like
	# `std::mem::take` in src/dev/recorder.rs.
	_pending.clear()
	return line


func _camera_telemetry() -> Array:
	if not is_instance_valid(_camera):
		return [[], [], []]
	if _dims == 3:
		var transform_3d: Transform3D = _camera.global_transform
		var quaternion := transform_3d.basis.get_rotation_quaternion()
		return [
			[transform_3d.origin.x, transform_3d.origin.y, transform_3d.origin.z],
			[quaternion.x, quaternion.y, quaternion.z, quaternion.w],
			[_camera.fov, _camera.size, _camera.near, _camera.far, _camera.h_offset, _camera.v_offset],
		]
	var transform_2d: Transform2D = _camera.get_global_transform()
	var zoom: Vector2 = _camera.zoom
	return [
		[transform_2d.origin.x, transform_2d.origin.y],
		[atan2(transform_2d.x.y, transform_2d.x.x)],
		[zoom.x, zoom.y],
	]


func _pcam_line() -> String:
	if not is_instance_valid(_pcam):
		return "null"
	return '{"active":%s,"priority":%s,"transform":%s,"noise":%s,"has_noise":%s}' % [
		_optional_bool("is_active"),
		_optional_int("get_priority"),
		_optional_transform("get_transform_output"),
		_optional_transform("get_noise_transform"),
		_optional_bool("has_noise_resource"),
	]


func _optional_bool(method: String) -> String:
	if not _pcam.has_method(method):
		return "null"
	return "true" if bool(_pcam.call(method)) else "false"


func _optional_int(method: String) -> String:
	if not _pcam.has_method(method):
		return "null"
	return str(int(_pcam.call(method)))


func _optional_transform(method: String) -> String:
	if not _pcam.has_method(method):
		return "null"
	var value: Variant = _pcam.call(method)
	if value is Transform2D:
		var transform_2d: Transform2D = value
		return _jfloats([
			transform_2d.x.x, transform_2d.x.y, transform_2d.y.x, transform_2d.y.y,
			transform_2d.origin.x, transform_2d.origin.y,
		])
	if value is Transform3D:
		var transform_3d: Transform3D = value
		return _jfloats([
			transform_3d.basis.x.x, transform_3d.basis.x.y, transform_3d.basis.x.z,
			transform_3d.basis.y.x, transform_3d.basis.y.y, transform_3d.basis.y.z,
			transform_3d.basis.z.x, transform_3d.basis.z.y, transform_3d.basis.z.z,
			transform_3d.origin.x, transform_3d.origin.y, transform_3d.origin.z,
		])
	return "null"


func _write_line(line: String) -> void:
	if _file == null:
		return
	_file.store_line(line)
	_file.flush()


func _finish() -> void:
	_done = true
	_unbind_pcam()
	if _file != null:
		_file.close()
		_file = null
	print("PCAMREC: wrote %d frames to %s" % [_written, _out_display])
	get_tree().quit(0)


func _abort(message: String) -> void:
	_done = true
	_unbind_pcam()
	_file = null
	push_error("PCAMREC: " + message)
	get_tree().quit(2)


func _node_path(node: Node) -> String:
	if not is_instance_valid(node):
		return ""
	return _jstr(str(node.get_path()))


func _jstr(value: String) -> String:
	return value.replace("\\", "\\\\").replace("\"", "\\\"")


func _jf(value: float) -> String:
	if not is_finite(value):
		return "null"
	return "%.9f" % value


func _jfloats(values: Array) -> String:
	var parts: Array = []
	for value in values:
		parts.append(_jf(float(value)))
	return "[" + ",".join(parts) + "]"


func _jstrings(values: Array) -> String:
	var parts: Array = []
	for value in values:
		parts.append("\"" + _jstr(str(value)) + "\"")
	return "[" + ",".join(parts) + "]"
GDSCRIPT
    verbose "generated observer ${OBSERVER_GD#"$REPO_ROOT/"}"
}

# --- recording ---------------------------------------------------------------
count_frames() { # $1 = jsonl file -> number of frame lines (header excluded)
    [ -f "$1" ] || {
        printf '0'
        return 0
    }
    local lines
    lines="$(wc -l <"$1" | tr -d '[:space:]')"
    if [ "$lines" -gt 1 ]; then printf '%s' "$((lines - 1))"; else printf '0'; fi
}

classify_failure() { # $1 = leg, $2 = log file -> skip|fail
    local leg="$1" log="$2"
    [ -f "$log" ] || {
        printf 'fail'
        return 0
    }
    if grep -qE 'no node of class' "$log"; then
        printf 'skip'
        return 0
    fi
    if [ "$leg" = baseline ]; then
        if grep -qE 'hides a native class|nonexistent class' "$log"; then
            printf 'skip'
            return 0
        fi
    elif grep -qE 'PcamRecorder|Unknown class|nonexistent class|Parse Error' "$log"; then
        printf 'skip'
        return 0
    fi
    printf 'fail'
}

run_godot_scene() { # $1 = scene res path, $2 = log file -> godot exit code
    local scene_res="$1" log_file="$2"
    require_godot
    set +e
    # stdin from /dev/null: the caller may be a `while read` loop over the scene
    # list, and a child that reads stdin would swallow the remaining scenes.
    "$GODOT" --headless --path "$CLIENT_DIR_HOST" --fixed-fps 60 \
        --quit-after "$((FRAMES + 120))" "$scene_res" </dev/null >"$log_file" 2>&1
    local status=$?
    set -e
    return "$status"
}

record_generated() { # $1 = leg, $2 = scene name, $3 = demo res, $4 = out dir -> TSV line
    local leg="$1" name="$2" res="$3" out_dir="$4"
    local out="$out_dir/$name.jsonl" log_file="$LOG_DIR/$leg-$name.log"
    write_wrapper_scene "$name" "$res" "$(host_path "$out")" "$leg"

    rm -f -- "$out"
    local godot_status=0
    run_godot_scene "$GENERATED_RES/$name.tscn" "$log_file" || godot_status=$?

    local recorded
    recorded="$(count_frames "$out")"
    if [ "$recorded" -ge "$FRAMES" ]; then
        printf 'PASS\t%s\t%s\t\n' "$name" "$recorded"
        return 0
    fi
    if [ "$recorded" -gt 0 ]; then
        printf 'FAIL\t%s\t%s\tpartial recording (%s of %s frames, godot exit %s)\n' \
            "$name" "$recorded" "$recorded" "$FRAMES" "$godot_status"
        return 1
    fi

    local verdict reason
    verdict="$(classify_failure "$leg" "$log_file")"
    reason="$(grep -E 'PCAMREC:|hides a native class|nonexistent class|Parse Error|Unknown class' "$log_file" 2>/dev/null |
        head -n 1 | tr -d '\r' | cut -c1-160 || true)"
    [ -n "$reason" ] || reason="no recording (godot exit $godot_status, see ${log_file#"$REPO_ROOT/"})"
    if [ "$verdict" = skip ]; then
        printf 'SKIP\t%s\t0\tclasses not registered: %s\n' "$name" "$reason"
        return 0
    fi
    printf 'FAIL\t%s\t0\t%s\n' "$name" "$reason"
    return 1
}

print_table() { # $1 = leg
    local leg="$1" file="$LOG_DIR/$leg-results.tsv"
    local row_status row_name row_frames row_reason
    [ -f "$file" ] || return 0
    log ""
    log "$leg recordings"
    printf '  %-42s %6s  %s\n' "scene" "frames" "result"
    while IFS=$'\t' read -r row_status row_name row_frames row_reason; do
        [ -n "${row_status:-}" ] || continue
        row_reason="$(printf '%s' "${row_reason:-}" | tr -d '\r' | cut -c1-160)"
        if [ "$row_status" = PASS ]; then
            printf '  %-42s %6s  %s\n' "$row_name" "$row_frames" "PASS"
        else
            printf '  %-42s %6s  %s%s\n' "$row_name" "$row_frames" "$row_status" "${row_reason:+ ($row_reason)}"
        fi
    done <"$file"
}

record_all() { # $1 = leg, $2 = examples dir, $3 = res:// root, $4 = out dir -> failure count
    local leg="$1" examples="$2" res_root="$3" out_dir="$4"
    local failures=0 total=0 entry name res
    local scene_list

    [ -d "$examples" ] || {
        warn "no demo scenes at ${examples#"$REPO_ROOT/"}"
        return 0
    }
    mkdir -p -- "$out_dir"
    if [ "$leg" = baseline ]; then
        write_observer
    fi

    # Materialize the scene list: a child that reads stdin would otherwise eat the
    # remaining entries of a process-substitution loop.
    scene_list="$(mktemp "${TMPDIR:-/tmp}/pcam_scenes.XXXXXX")"
    discover_scenes "$examples" "$res_root" >"$scene_list"
    : >"${LOG_DIR:?}/$leg-results.tsv"

    while IFS=$'\t' read -r name res; do
        [ -n "$name" ] || continue
        if [ -n "$SCENE_FILTER" ] && [[ "$name" != *"$SCENE_FILTER"* ]]; then
            continue
        fi
        total=$((total + 1))
        log "  [$leg] $name"
        if entry="$(record_generated "$leg" "$name" "$res" "$out_dir" </dev/null)"; then
            :
        else
            failures=$((failures + 1))
        fi
        printf '%s\n' "$entry" >>"$LOG_DIR/$leg-results.tsv"
        case "$entry" in
        PASS*) RECORDED_COUNT=$((RECORDED_COUNT + 1)) ;;
        esac
    done <"$scene_list"
    rm -f -- "$scene_list"

    if [ "$total" -eq 0 ]; then
        log "  no scenes matched${SCENE_FILTER:+ (--scene '$SCENE_FILTER')}"
        return 0
    fi

    print_table "$leg"
    return "$failures"
}

# --- legs --------------------------------------------------------------------
leg_preflight_candidate() {
    if [ ! -d "$RUST_EXAMPLES" ]; then
        warn "no ported demo scenes at ${RUST_EXAMPLES#"$REPO_ROOT/"} (the port is still landing); SKIP"
        return 1
    fi
    local library
    library="$(find "$CLIENT_DIR/bin/rust" -maxdepth 2 -type f \
        \( -name 'phantom_camera_rust.dll' -o -name 'libphantom_camera_rust.so' -o -name 'libphantom_camera_rust.dylib' \) \
        -print -quit 2>/dev/null || true)"
    if [ -z "$library" ]; then
        warn "phantom_camera_rust library not built (cargo build --manifest-path client/rust/Cargo.toml -p phantom-camera-rust --target-dir client/bin/rust); SKIP"
        return 1
    fi
    if [ ! -f "$RUST_EXTENSION" ]; then
        warn "no ${RUST_EXTENSION#"$REPO_ROOT/"} (disabled?); SKIP"
        return 1
    fi
    if [ ! -f "$RUST_MANAGER_SCENE" ]; then
        warn "no ${RUST_MANAGER_SCENE#"$REPO_ROOT/"} (the PhantomCameraManager autoload root); SKIP"
        return 1
    fi
    set_extension_listed 1
    ensure_manager_autoload "$RUST_MANAGER_RES"
}

leg_preflight_baseline() {
    if [ -d "$ORACLE_INSTALL" ]; then
        warn "client/addons/phantom_camera already exists; treating it as the oracle install"
    else
        [ -d "$ORACLE_SRC" ] || die "oracle not found at ${ORACLE_SRC#"$REPO_ROOT/"} (and no client/addons/phantom_camera)"
        mv -f "$ORACLE_SRC" "$ORACLE_INSTALL"
        ORACLE_MOVED=1
        log "installed the GDScript oracle at ${ORACLE_INSTALL#"$REPO_ROOT/"} (moved back on exit)"
    fi
    if [ -f "$RUST_EXTENSION" ]; then
        mv -f "$RUST_EXTENSION" "$RUST_EXTENSION.disabled"
        EXTENSION_DISABLED=1
        log "disabled ${RUST_EXTENSION#"$REPO_ROOT/"} for the baseline run"
    fi
    set_extension_listed 0
    ensure_manager_autoload "$ORACLE_MANAGER_RES"
    refresh_extension_list
}

run_generated_leg() { # $1 = leg
    local leg="$1" examples res_root out_dir
    RECORDING_LEGS=$((RECORDING_LEGS + 1))
    require_godot
    if [ "$leg" = baseline ]; then
        log "== baseline: GDScript oracle =="
        leg_preflight_baseline
        examples="$ORACLE_EXAMPLES"
        res_root="$ORACLE_EXAMPLES_RES"
        out_dir="$GOLDEN_DIR"
    else
        log "== candidate: Rust GDExtension =="
        leg_preflight_candidate || return 0
        examples="$RUST_EXAMPLES"
        res_root="$RUST_EXAMPLES_RES"
        out_dir="$CANDIDATE_DIR"
    fi
    record_all "$leg" "$examples" "$res_root" "$out_dir"
}

run_main_scene() { # $1 = leg; records a pre-authored scene, no wrapper generated
    local leg="$1" out_dir
    RECORDING_LEGS=$((RECORDING_LEGS + 1))
    require_godot
    if [ "$leg" = baseline ]; then
        leg_preflight_baseline
        out_dir="$GOLDEN_DIR"
    else
        leg_preflight_candidate || return 0
        out_dir="$CANDIDATE_DIR"
    fi
    mkdir -p -- "$out_dir"

    local name log_file
    name="$(basename -- "${MAIN_SCENE%.tscn}")"
    log_file="$(mktemp "${TMPDIR:-/tmp}/pcam_main.XXXXXX")"
    log "== $leg: recording --main-scene $MAIN_SCENE =="
    rm -f -- "$MAIN_SCENE_OUT"
    local godot_status=0
    run_godot_scene "$MAIN_SCENE" "$log_file" || godot_status=$?

    local recorded
    recorded="$(count_frames "$MAIN_SCENE_OUT")"
    local status reason
    if [ "$recorded" -ge "$FRAMES" ]; then
        status=PASS
        reason=""
        RECORDED_COUNT=$((RECORDED_COUNT + 1))
    elif [ "$recorded" -gt 0 ]; then
        status=FAIL
        reason="partial recording ($recorded of $FRAMES frames, godot exit $godot_status)"
    else
        status=FAIL
        reason="no frames written to $MAIN_SCENE_OUT (godot exit $godot_status, see $log_file)"
    fi
    printf '%s\t%s\t%s\t%s\n' "$status" "$name" "$recorded" "$reason" >"$LOG_DIR/$leg-results.tsv"
    print_table "$leg"
    if [ "$status" != PASS ]; then
        return 1
    fi
    return 0
}

run_compare() {
    local python
    python="$(command -v python3 || command -v python || true)"
    [ -n "$python" ] || die "python3 not found (needed for tools/pcam_compare.py)"
    [ -f "$COMPARE_TOOL" ] || die "missing ${COMPARE_TOOL#"$REPO_ROOT/"}"

    log "== compare: parity/pcam_golden vs client/rust/target/pcam =="
    local args=(--golden "$GOLDEN_DIR" --candidate "$CANDIDATE_DIR")
    if [ -n "$SCENE_FILTER" ]; then
        args+=(--scene "$SCENE_FILTER")
    fi
    if [ "$VERBOSE" -eq 1 ]; then
        args+=(-v)
    fi
    if [ -n "$JSON_REPORT" ]; then
        args+=(--json "$JSON_REPORT")
    fi
    set +e
    "$python" "$COMPARE_TOOL" "${args[@]}"
    local status=$?
    set -e
    return "$status"
}

# --- argument parsing --------------------------------------------------------
declare -a MODES=()
while [ $# -gt 0 ]; do
    case "$1" in
    --baseline) MODES+=(baseline) ;;
    --candidate) MODES+=(candidate) ;;
    --compare) MODES+=(compare) ;;
    --scene)
        shift
        [ $# -gt 0 ] || die "--scene needs a substring"
        SCENE_FILTER="$1"
        ;;
    --scene=*) SCENE_FILTER="${1#*=}" ;;
    --frames)
        shift
        [ $# -gt 0 ] || die "--frames needs a number"
        FRAMES="$1"
        ;;
    --frames=*) FRAMES="${1#*=}" ;;
    --main-scene)
        shift
        [ $# -gt 0 ] || die "--main-scene needs a res:// path"
        MAIN_SCENE="$1"
        ;;
    --main-scene=*) MAIN_SCENE="${1#*=}" ;;
    --out)
        shift
        [ $# -gt 0 ] || die "--out needs a path"
        MAIN_SCENE_OUT="$1"
        ;;
    --out=*) MAIN_SCENE_OUT="${1#*=}" ;;
    --json)
        if [ $# -gt 1 ] && [ "${2#-}" = "$2" ]; then
            shift
            JSON_REPORT="$1"
        else
            JSON_REPORT="-"
        fi
        ;;
    --json=*) JSON_REPORT="${1#*=}" ;;
    -v | --verbose) VERBOSE=1 ;;
    -h | --help)
        usage
        exit 0
        ;;
    *)
        printf 'pcam_parity: error: unknown argument %s\n' "$1" >&2
        usage >&2
        exit 2
        ;;
    esac
    shift
done

case "$FRAMES" in
'' | *[!0-9]*) die "--frames must be a positive integer (got '$FRAMES')" ;;
esac
if [ "$FRAMES" -le 0 ]; then
    die "--frames 0 records forever, which the harness cannot diff; pass a positive count"
fi

if [ -n "$MAIN_SCENE" ]; then
    case "$MAIN_SCENE" in
    res://*) ;;
    *) die "--main-scene takes a res:// path (got '$MAIN_SCENE')" ;;
    esac
    [ -n "$MAIN_SCENE_OUT" ] || die "--main-scene also needs --out <absolute jsonl path> (the scene writes it itself)"
    [ ${#MODES[@]} -le 1 ] || die "--main-scene records a single scene; combine it with at most one of --baseline/--candidate"
    [ ${#MODES[@]} -eq 1 ] || MODES=(candidate)
fi

[ ${#MODES[@]} -gt 0 ] || MODES=(candidate compare)
[ -f "$PROJECT_FILE" ] || die "not a Godot project checkout: ${PROJECT_FILE#"$REPO_ROOT/"} missing"
[ -f "$COMPARE_TOOL" ] || die "missing ${COMPARE_TOOL#"$REPO_ROOT/"}"

begin_run

status=0
for mode in "${MODES[@]}"; do
    case "$mode" in
    baseline)
        if [ -n "$MAIN_SCENE" ]; then
            run_main_scene baseline || status=1
        else
            run_generated_leg baseline || status=1
        fi
        ;;
    candidate)
        if [ -n "$MAIN_SCENE" ]; then
            run_main_scene candidate || status=1
        else
            run_generated_leg candidate || status=1
        fi
        ;;
    compare)
        if [ "$RECORDING_LEGS" -gt 0 ] && [ "$RECORDED_COUNT" -eq 0 ]; then
            warn "nothing was recorded in this run; skipping the comparison (SKIP, not a failure)"
        else
            set +e
            run_compare
            compare_status=$?
            set -e
            if [ "$compare_status" -eq 2 ]; then
                status=2
            elif [ "$compare_status" -eq 1 ] && [ "$status" -eq 0 ]; then
                status=1
            fi
        fi
        ;;
    esac
    if [ "$status" -eq 2 ]; then
        break
    fi
done

exit "$status"
