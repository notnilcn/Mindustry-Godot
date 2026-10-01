## SPDX-License-Identifier: GPL-3.0-only
##
## State inspector overlay for the M4 spine (plan 00 §3.5/§3.10).
##
## Reads `MindSimHost.get_state_json()` (the same schema as the headless dump)
## every 250 ms and on the `state_changed(tick, checksum)` signal. This script
## never writes sim state; it is UI layout + read-only formatting only.

extends Control

## Poll interval in seconds (plan §3.10 extension contract).
const POLL_INTERVAL := 0.25

## Path to the sim host; overridable when this scene is reused elsewhere.
@export var host_path: NodePath = ^"../../SimHost"
## Path to the camera used for the cursor-tile readout.
@export var camera_path: NodePath = ^"../../World/Camera2D"

var _host: Node = null
var _camera: Node = null
var _elapsed := 0.0

@onready var _label: Label = $Label


func _ready() -> void:
	_host = get_node_or_null(host_path)
	if _host == null:
		_host = get_node_or_null("/root/Spine/SimHost")
	if _host != null and _host.has_signal("state_changed"):
		if not _host.state_changed.is_connected(_on_state_changed):
			_host.state_changed.connect(_on_state_changed)
	_camera = get_node_or_null(camera_path)
	if _camera == null:
		_camera = get_node_or_null("/root/Spine/World/Camera2D")
	_refresh()


func _process(delta: float) -> void:
	_elapsed += delta
	if _elapsed >= POLL_INTERVAL:
		_elapsed = 0.0
		_refresh()


func _on_state_changed(_tick: int, _checksum: String) -> void:
	_refresh()


func _refresh() -> void:
	if _host == null or not is_instance_valid(_host):
		return
	var parsed: Variant = JSON.parse_string(_host.call("get_state_json"))
	if not (parsed is Dictionary):
		return
	var state: Dictionary = parsed

	var lines := PackedStringArray()
	lines.append("tick: %d" % int(state.get("tick", 0)))
	lines.append("paused: %s" % ("true" if bool(_host.call("is_paused")) else "false"))
	lines.append("selected: %s" % str(_host.call("selected_block")))
	lines.append("cursor: %s" % _cursor_tile_text())
	lines.append("checksum: %s" % str(state.get("checksum", "")))
	_label.text = "\n".join(lines)


func _cursor_tile_text() -> String:
	if _camera == null or not is_instance_valid(_camera):
		_camera = get_node_or_null(camera_path)
	if _camera == null or not is_instance_valid(_camera):
		return "-"
	var mouse := get_viewport().get_mouse_position()
	var tile: Vector2i = _camera.call("screen_to_tile", mouse.x, mouse.y)
	return "(%d, %d)" % [tile.x, tile.y]
