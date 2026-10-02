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
## Path to the STDB autoload whose `net` page this inspector renders.
@export var net_path: NodePath = ^"/root/StdbConnector"
## Content type listed in `ContentList` (plan 02 §3.7), e.g. `item`, `block`.
@export var content_type: String = "item"

var _host: Node = null
var _camera: Node = null
var _assets: Node = null
var _elapsed := 0.0

@onready var _label: Label = $Label
@onready var _net_label: Label = $NetLabel
@onready var _content_counts: Label = $ContentCounts
@onready var _content_list: ItemList = $ContentList
@onready var _region_preview: TextureRect = $RegionPreview


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
	_refresh_content()


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
	lines.append("update: %d  state: %s" % [
		int(_host.call("get_update_id")),
		str(_host.call("get_state")),
	])
	lines.append("paused: %s" % ("true" if bool(_host.call("is_paused")) else "false"))
	lines.append("selected: %s" % str(_host.call("selected_block")))
	lines.append("cursor: %s" % _cursor_tile_text())
	lines.append("checksum: %s" % str(state.get("checksum", "")))
	lines.append("groups: %s" % _group_counts_text())
	_label.text = "\n".join(lines)
	_refresh_net()


## Live per-group entity counts (plan 05 M9 inspector surface); read-only.
func _group_counts_text() -> String:
	if _host == null or not is_instance_valid(_host):
		return "-"
	var counts: Dictionary = _host.call("get_group_counts")
	var parts := PackedStringArray()
	for key in ["all", "build", "unit", "bullet", "player"]:
		if counts.has(key):
			parts.append("%s=%d" % [key, int(counts[key])])
	return " ".join(parts)


## `net` page (plan 01 §3.11): connector state, wave flags, relay counters.
## Read-only: every value comes from the StdbConnector autoload.
func _refresh_net() -> void:
	var connector := get_node_or_null(net_path)
	if connector == null or not is_instance_valid(connector):
		_net_label.text = "net: no StdbConnector autoload"
		return
	var lines := PackedStringArray()
	var identity: String = str(connector.call("local_identity_hex"))
	lines.append("net: %s  %s" % [
		str(connector.call("state")),
		identity.substr(0, 8) if not identity.is_empty() else "-",
	])
	lines.append("waves: base=%s lobby=%s game=%s" % [
		_wave_text(connector, "base"),
		_wave_text(connector, "lobby"),
		_wave_text(connector, "game"),
	])
	lines.append("relay: applied=%d last=%d err=%s" % [
		int(connector.call("dev_relay_applied_count")),
		int(connector.call("dev_last_command_id")),
		str(connector.call("relay_order_error")) if not str(connector.call("relay_order_error")).is_empty() else "-",
	])
	_net_label.text = "\n".join(lines)


func _wave_text(connector: Node, wave: String) -> String:
	return "on" if bool(connector.call("wave_applied_state", wave)) else "off"


## Content registry readout (plan 02 §3.7): per-type counts plus the ordered
## entries of `content_type`. Read-only; sourced from MindSimHost (the plan's
## `MindCore` singleton lands with plan 00's extension contract).
func _refresh_content() -> void:
	if _host == null or not is_instance_valid(_host):
		return
	var counts: Dictionary = _host.call("content_counts")
	if counts.is_empty():
		_content_counts.text = "content: -"
		_content_list.clear()
		return
	var lines := PackedStringArray(["content"])
	for key in counts.keys():
		lines.append("%s: %d" % [str(key), int(counts[key])])
	_content_counts.text = "\n".join(lines)
	var names: PackedStringArray = _host.call("content_list", content_type)
	_content_list.clear()
	for name in names:
		_content_list.add_item(str(name))


func _cursor_tile_text() -> String:
	if _camera == null or not is_instance_valid(_camera):
		_camera = get_node_or_null(camera_path)
	if _camera == null or not is_instance_valid(_camera):
		return "-"
	var mouse := get_viewport().get_mouse_position()
	var tile: Vector2i = _camera.call("screen_to_tile", mouse.x, mouse.y)
	return "(%d, %d)" % [tile.x, tile.y]


## Resolves the `MindAssets` autoload (plan 03 M5).
func _assets_node() -> Node:
	if _assets == null or not is_instance_valid(_assets):
		_assets = get_node_or_null("/root/MindAssets")
	return _assets


## M5 inspector fixture: displays an atlas region's texture (plan 03 §7.1c
## step 6). Returns `true` when the region resolved. Read-only.
func show_region(region_name: String) -> bool:
	var assets := _assets_node()
	if assets == null:
		return false
	var texture: Texture2D = assets.call("find_region", region_name)
	_region_preview.texture = texture
	return texture != null


## M5 inspector fixture: displays a content UI icon by looking up the generated
## `ui/<type>-<name>-ui` region. The `Iconc` font-glyph rendering path lands
## with plan 03 M7.
func show_icon(icon_name: String) -> bool:
	for prefix in ["block", "unit", "item", "liquid", "status"]:
		if show_region("ui/%s-%s-ui" % [prefix, icon_name]):
			return true
	return false
