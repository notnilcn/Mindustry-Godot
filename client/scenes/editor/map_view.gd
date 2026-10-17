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
var _world_view: Node = null
var _camera: Camera2D = null
## Middle-drag pan (`MapView.pan` while the temporary `zoom` tool is active).
var _panning := false
var _pan_position := Vector2.ZERO


func _ready() -> void:
	_editor = get_node_or_null(editor_path)
	mouse_filter = Control.MOUSE_FILTER_STOP
	_camera = get_node_or_null(^"ViewportContainer/SubViewport/Camera2D")
	_world_view = get_node_or_null(^"ViewportContainer/SubViewport/EditorWorldView")


func _process(delta: float) -> void:
	# Hidden with the shell (playtest / main menu): WASD belongs to the game.
	if not is_visible_in_tree():
		return
	if _editor != null and is_instance_valid(_editor):
		_pan_keys(delta)
		_drive_view()
		queue_redraw()


## `MapView.act`: WASD/arrows pan (`offset -= axis * 15 * delta / zoom`) unless a
## text field has focus or Ctrl is held. Keys belong to a dialog on top
## (`Core.scene.hasField()` / the editor not being the top dialog).
func _pan_keys(delta: float) -> void:
	if _modal():
		return
	var focus := get_viewport().gui_get_focus_owner()
	if focus is LineEdit or focus is TextEdit:
		return
	if Input.is_key_pressed(KEY_CTRL):
		return
	var ax := 0.0
	var ay := 0.0
	if Input.is_physical_key_pressed(KEY_A) or Input.is_physical_key_pressed(KEY_LEFT):
		ax -= 1.0
	if Input.is_physical_key_pressed(KEY_D) or Input.is_physical_key_pressed(KEY_RIGHT):
		ax += 1.0
	if Input.is_physical_key_pressed(KEY_W) or Input.is_physical_key_pressed(KEY_UP):
		ay -= 1.0
	if Input.is_physical_key_pressed(KEY_S) or Input.is_physical_key_pressed(KEY_DOWN):
		ay += 1.0
	if ax != 0.0 or ay != 0.0:
		_editor.call("map_view_pan", -ax * 15.0 * delta, -ay * 15.0 * delta)


## `MapView.draw`: the `EditorRenderer` is drawn into the view rect
## `(centerx - sclwidth/2, centery - sclheight/2, sclwidth, sclheight)`; the
## `SubViewport` camera mirrors that projection (`project`/`unproject` math).
func _drive_view() -> void:
	if _camera == null or _world_view == null:
		return
	var status: Dictionary = _editor.call("status")
	var map_w := int(status.get("width", 0))
	var map_h := int(status.get("height", 0))
	if map_w <= 0 or map_h <= 0:
		return
	var state: Dictionary = _editor.call("map_view_state")
	var zoom := float(state.get("zoom", 1.0))
	var offset_x := float(state.get("offset_x", 0.0))
	var offset_y := float(state.get("offset_y", 0.0))
	var view := minf(size.x, size.y)
	var scl_w := view * zoom
	var scl_h := scl_w * float(map_h) / float(map_w)
	# View-pixel position of the world origin (tile 0,0 = world 0,0).
	var origin_x := offset_x * zoom - scl_w / 2.0 + size.x / 2.0
	var origin_y := offset_y * zoom - scl_h / 2.0 + size.y / 2.0
	var scale_px := scl_w / float(map_w) / 8.0
	_camera.position = Vector2(
		(size.x / 2.0 - origin_x) / scale_px,
		(size.y / 2.0 - origin_y) / scale_px,
	)
	_camera.zoom = Vector2(scale_px, scale_px)
	# Visible world-pixel rect for the chunked editor renderer.
	_world_view.call(
		"set_view_rect",
		-origin_x / scale_px,
		-origin_y / scale_px,
		size.x / scale_px,
		size.y / scale_px,
	)


func _gui_input(event: InputEvent) -> void:
	if _editor == null or not is_instance_valid(_editor):
		return
	if event is InputEventMouseMotion:
		_cursor = _tile_at(event.position)
		# `MapView.pan` while the middle-drag `zoom` tool is active.
		if _panning:
			var delta: Vector2 = event.position - _pan_position
			_pan_position = event.position
			_editor.call("map_view_pan", delta.x, delta.y)
		elif _drawing:
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
		elif event.button_index == MOUSE_BUTTON_MIDDLE:
			if event.pressed:
				_begin_pan(event.position)
			else:
				_end_pan()
		accept_event()


## `MapView.touchDown`: middle swaps to the `zoom` tool until release.
func _begin_pan(position: Vector2) -> void:
	_panning = true
	_pan_position = position
	if _last_tool.is_empty():
		_last_tool = str(_editor.call("tool"))
		_editor.call("set_tool", "zoom")


## `MapView.touchUp`: the temporary `zoom` tool is restored.
func _end_pan() -> void:
	_panning = false
	if not _last_tool.is_empty():
		_editor.call("set_tool", _last_tool)
		_last_tool = ""


## `MapView.act`: Shift or Alt swaps to the temporary `pick` tool until release.
func _unhandled_input(event: InputEvent) -> void:
	if _editor == null or not is_instance_valid(_editor) or not is_visible_in_tree():
		return
	var key := event as InputEventKey
	if key == null or key.echo or (key.keycode != KEY_SHIFT and key.keycode != KEY_ALT):
		return
	if key.pressed:
		if _modal():
			return
		if _last_tool.is_empty():
			_last_tool = str(_editor.call("tool"))
			_editor.call("set_tool", "pick")
	elif not _last_tool.is_empty():
		_editor.call("set_tool", _last_tool)
		_last_tool = ""


## A dialog or prompt is up over the editor shell (`MapEditorDialog.has_modal`).
func _modal() -> bool:
	var shell := get_parent()
	return shell != null and shell.has_method("has_modal") and bool(shell.call("has_modal"))


## Drops an in-progress stroke/pan and restores a temporary tool (the shell is
## hiding; `MapView` would otherwise keep the eraser/zoom/pick swap).
func cancel_input() -> void:
	_drawing = false
	_panning = false
	_cursor = Vector2i(-1, -1)
	if not _last_tool.is_empty() and _editor != null and is_instance_valid(_editor):
		_editor.call("set_tool", _last_tool)
	_last_tool = ""
	queue_redraw()


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
	if bool(status.get("grid", false)):
		_draw_grid(map_w, map_h)
	if _cursor.x >= 0 and _cursor.y >= 0:
		var points: PackedVector2Array = _editor.call(
			"map_view_brush_outline", _cursor.x, _cursor.y, size.x, size.y
		)
		if points.size() >= 2:
			var closed := points.duplicate()
			closed.append(points[0])
			# `Draw.color(Pal.accent); Lines.stroke(Scl.scl(2f))` (MapView.java:282-283).
			draw_polyline(closed, MindStyles.ACCENT, 2.0)
	# The view frame, drawn last: `Draw.color(Pal.accent); Lines.stroke(Scl.scl(3f));
	# Lines.rect(x, y, width, height)` (MapView.java:323-325). Inset so the 3 px
	# stroke stays inside the clipped view (L03 T3, PAR-0421: was a 2 px Pal.remove line).
	draw_rect(Rect2(Vector2(1.5, 1.5), size - Vector2(3.0, 3.0)), MindStyles.ACCENT, false, 3.0)


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
