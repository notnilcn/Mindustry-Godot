## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/MapView.java (plan 19 §3.5/§3.10).
##
## UI-only: pointer events are forwarded to `/root/Spine/MindEditor`; all
## projection and drawing primitives come back from Rust. This script never
## mutates the editor model directly.

extends Control

## Path to the MindEditor facade.
@export var editor_path: NodePath = ^"/root/Spine/MindEditor"

var _editor: Node = null
var _cursor := Vector2i(-1, -1)
var _drawing := false
var _last := Vector2i(-1, -1)
var _start := Vector2i(-1, -1)
var _last_tool := ""


func _ready() -> void:
	_editor = get_node_or_null(editor_path)
	mouse_filter = Control.MOUSE_FILTER_STOP


func _process(_delta: float) -> void:
	if _editor != null and is_instance_valid(_editor):
		queue_redraw()


func _gui_input(event: InputEvent) -> void:
	if _editor == null or not is_instance_valid(_editor):
		return
	if event is InputEventMouseMotion:
		_cursor = _tile_at(event.position)
		if _drawing:
			_drag_to(_cursor)
		queue_redraw()
		accept_event()
	elif event is InputEventMouseButton:
		if event.button_index == MOUSE_BUTTON_WHEEL_UP and event.pressed:
			_editor.call("map_view_zoom", 1.0)
		elif event.button_index == MOUSE_BUTTON_WHEEL_DOWN and event.pressed:
			_editor.call("map_view_zoom", -1.0)
		elif event.button_index == MOUSE_BUTTON_LEFT:
			if event.pressed:
				_begin(_tile_at(event.position), false)
			else:
				_end()
		elif event.button_index == MOUSE_BUTTON_RIGHT:
			if event.pressed:
				_begin(_tile_at(event.position), true)
			else:
				_end()
		accept_event()


func _tile_at(position: Vector2) -> Vector2i:
	var tile: Vector2 = _editor.call("map_view_project", position.x, position.y, size.x, size.y)
	return Vector2i(int(tile.x), int(tile.y))


func _begin(tile: Vector2i, temporary_eraser: bool) -> void:
	_drawing = true
	_last = tile
	_start = tile
	if temporary_eraser:
		_last_tool = str(_editor.call("tool"))
		_editor.call("set_tool", "eraser")
	_editor.call("map_view_touch", tile.x, tile.y)
	queue_redraw()


func _drag_to(tile: Vector2i) -> void:
	if not _drawing or tile == _last:
		return
	if str(_editor.call("tool")) == "line":
		# The line endpoint is committed on release (`touchedLine`).
		_last = tile
		return
	_editor.call("map_view_drag", _last.x, _last.y, tile.x, tile.y)
	_last = tile


func _end() -> void:
	if not _drawing:
		return
	_drawing = false
	if str(_editor.call("tool")) == "line":
		_editor.call("dev_draw_line", _start.x, _start.y, _last.x, _last.y)
	_editor.call("map_view_flush")
	if not _last_tool.is_empty():
		_editor.call("set_tool", _last_tool)
		_last_tool = ""
	queue_redraw()


func _draw() -> void:
	if _editor == null or not is_instance_valid(_editor):
		return
	var status: Dictionary = _editor.call("status")
	var map_w := int(status.get("width", 0))
	var map_h := int(status.get("height", 0))
	if map_w <= 0 or map_h <= 0:
		return
	# Canvas border (`Pal.remove`).
	draw_rect(Rect2(Vector2.ZERO, size), Color(0.85, 0.42, 0.33), false, 2.0)
	if bool(status.get("grid", false)):
		_draw_grid(map_w, map_h)
	if _cursor.x >= 0 and _cursor.y >= 0:
		var points: PackedVector2Array = _editor.call(
			"map_view_brush_outline", _cursor.x, _cursor.y, size.x, size.y
		)
		if points.size() >= 2:
			var closed := points.duplicate()
			closed.append(points[0])
			draw_polyline(closed, Color(0.85, 0.42, 0.33), 2.0)


func _draw_grid(map_w: int, map_h: int) -> void:
	var quarter := Color(0.98, 0.78, 0.34, 0.65)
	var center := Color(1.0, 0.83, 0.5)
	for fraction: float in [0.25, 0.5, 0.75]:
		var is_center: bool = fraction == 0.5
		var color := center if is_center else quarter
		var width := 3.0 if is_center else 2.0
		var vx := int(map_w * fraction)
		var vy := int(map_h * fraction)
		var top: Vector2 = _editor.call("map_view_unproject", vx, 0, size.x, size.y)
		var bottom: Vector2 = _editor.call("map_view_unproject", vx, map_h, size.x, size.y)
		draw_line(top, bottom, color, width)
		var left: Vector2 = _editor.call("map_view_unproject", 0, vy, size.x, size.y)
		var right: Vector2 = _editor.call("map_view_unproject", map_w, vy, size.x, size.y)
		draw_line(left, right, color, width)
