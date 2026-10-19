## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/MapObjectivesCanvas.java (plan 19 M5 §3.9).
##
## Presentation-only canvas: pan, grid, node tiles with their type names,
## connector lines, drag-to-move, the `@add` query cursor and paste targeting.
## The objective graph data comes from `MindEditor.objectives_json`.

extends Control

## Emitted on a left click over one objective node (`MapObjectivesCanvas`
## tile selection -> `MapObjectivesDialog`'s field editors).
signal node_selected(index: int)
## A node was dragged to a new grid position (`Mover.touchUp` + `moveTile`).
signal node_moved(index: int, x: int, y: int)
## The query cursor was clicked (`tap` -> `placeQuery`).
signal query_placed(x: int, y: int)

## `MapObjectivesCanvas.{objWidth, objHeight, bounds}` in placement units.
const OBJ_WIDTH := 5
const OBJ_HEIGHT := 2
const BOUNDS := 100
const UNIT := 48.0


@export var editor_path: NodePath = ^"/root/Spine/MindEditor"

var _editor: Node = null
var _pan := Vector2.ZERO
var _dragging := false
## Index of the node under an active drag (`ObjectiveTilemap.moving`).
var _moving := -1
var _move_offset := Vector2.ZERO
## While the `@add` query cursor is active (`isQuerying`).
var _querying := false
## Highlighted node (`updateCard`-style selection state).
var _selected := -1
## Last mouse position delivered to this control (local space). `Core.input.mouse()`
## equivalent: the stretch transform makes `get_local_mouse_position()` unusable.
var _cursor := Vector2.ZERO


func _ready() -> void:
	_editor = get_node_or_null(editor_path)
	mouse_filter = Control.MOUSE_FILTER_STOP


func set_selected(index: int) -> void:
	_selected = index
	queue_redraw()


## `canvas.query(obj)` / `stopQuery`.
func set_querying(active: bool) -> void:
	_querying = active
	queue_redraw()


func is_querying() -> bool:
	return _querying


func _gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton or event is InputEventMouseMotion:
		_cursor = event.position
	if event is InputEventMouseButton:
		var button := (event as InputEventMouseButton).button_index
		if event.pressed and button == MOUSE_BUTTON_LEFT:
			if _querying:
				# `tap`: place the pending objective at the cursor.
				var cell := _query_cell(event.position)
				if valid_place(cell.x, cell.y):
					query_placed.emit(cell.x, cell.y)
				return
			# Left click selects a node; a press over one starts a drag, a
			# miss starts a pan (`Mover.touchDown` / pan gesture).
			var hit := _node_at(event.position)
			if hit >= 0:
				_moving = hit
				_move_offset = _origin() + Vector2(
					float(_node_x(hit)), float(_node_y(hit))) * UNIT - event.position
				node_selected.emit(hit)
				return
			_dragging = true
		elif button == MOUSE_BUTTON_MIDDLE:
			_dragging = event.pressed
		elif button == MOUSE_BUTTON_LEFT:
			if _moving >= 0:
				_finish_move()
				return
			_dragging = false
		queue_redraw()
	elif event is InputEventMouseMotion:
		if _moving >= 0:
			queue_redraw()
		elif _dragging:
			# `ElementGestureListener.pan`: clamp to the tilemap's valid range.
			_pan += (event as InputEventMouseMotion).relative
			_pan.x = clampf(_pan.x, -BOUNDS * UNIT + size.x, BOUNDS * UNIT)
			_pan.y = clampf(_pan.y, -BOUNDS * UNIT + size.y, BOUNDS * UNIT)
			queue_redraw()


## `screenToLocalCoordinates(tilemap)` query cursor cell (`tap`/paste math).
func cursor_cell() -> Vector2i:
	return _query_cell(_cursor)


## The last mouse position in canvas-local space (`Core.input.mouse()`).
func cursor_position() -> Vector2:
	return _cursor


## The node under the mouse (`doInput`'s copy hit-test).
func node_index_at(point: Vector2) -> int:
	return _node_at(point)


## `ObjectiveTilemap.validPlace` for a cell (public for paste/query).
func cell_valid(x: int, y: int) -> bool:
	return valid_place(x, y)


## `Mover.touchUp` + `moveTile`: snap to the grid, reject overlaps/bounds.
func _finish_move() -> void:
	var index := _moving
	_moving = -1
	if index < 0 or index >= _objectives().size():
		return
	var cell := _move_cell()
	var old_x := _node_x(index)
	var old_y := _node_y(index)
	if (cell.x != old_x or cell.y != old_y) and valid_place(cell.x, cell.y, index):
		node_moved.emit(index, cell.x, cell.y)
	queue_redraw()


## The drag's snapped tile (`Mover.touchUp`'s `round(moving.x / unitSize)`).
func _move_cell() -> Vector2i:
	var local := _cursor - _origin() + _move_offset
	return Vector2i(int(roundf(local.x / UNIT)), int(roundf(local.y / UNIT)))


## `ObjectiveTilemap.validPlace`: in bounds and not overlapping another node.
func valid_place(x: int, y: int, ignore := -1) -> bool:
	if x < -BOUNDS or y < -BOUNDS or x > BOUNDS or y > BOUNDS:
		return false
	var objectives := _objectives()
	for index in objectives.size():
		if index == ignore:
			continue
		var value: Dictionary = objectives[index] if objectives[index] is Dictionary else {}
		if not _has_pos(int(value.get("editorPos", 0))):
			continue
		var nx := _node_x_at(int(value.get("editorPos", 0)))
		var ny := _node_y_at(int(value.get("editorPos", 0)))
		if nx < x + OBJ_WIDTH and nx + OBJ_WIDTH > x and ny < y + OBJ_HEIGHT and ny + OBJ_HEIGHT > y:
			return false
	return true


## Index of the node whose rect contains `point`, or `-1`. Nodes paint in
## order, so the top-most (last) hit wins.
func _node_at(point: Vector2) -> int:
	var objectives := _objectives()
	for index in range(objectives.size() - 1, -1, -1):
		var value: Dictionary = objectives[index] if objectives[index] is Dictionary else {}
		var pos := int(value.get("editorPos", 0))
		if not _has_pos(pos):
			continue
		if _rect_at(pos).has_point(point):
			return index
	return -1


func _origin() -> Vector2:
	return size * 0.5 + _pan


## The tile the mouse is over (`screenToLocalCoordinates(tilemap)` math).
func _query_cell(point: Vector2) -> Vector2i:
	var local := point - _origin()
	return Vector2i(
		int(roundf(local.x / UNIT - OBJ_WIDTH * 0.5)),
		int(floorf(local.y / UNIT - 1.0)))


func _pack_pos(x: int, y: int) -> int:
	var value := (x & 0xFFFF) | ((y & 0xFFFF) << 16)
	if value >= 2147483648:
		value -= 4294967296
	return value


func _has_pos(editor_pos: int) -> bool:
	return editor_pos != _pack_pos(-999, -999)


func _node_x(index: int) -> int:
	var objectives := _objectives()
	var value: Dictionary = objectives[index] if index >= 0 and index < objectives.size() \
			and objectives[index] is Dictionary else {}
	return _node_x_at(int(value.get("editorPos", 0)))


func _node_y(index: int) -> int:
	var objectives := _objectives()
	var value: Dictionary = objectives[index] if index >= 0 and index < objectives.size() \
			and objectives[index] is Dictionary else {}
	return _node_y_at(int(value.get("editorPos", 0)))


func _node_x_at(editor_pos: int) -> int:
	var x := (editor_pos >> 16) & 0xFFFF
	return x - 65536 if x > 32767 else x


func _node_y_at(editor_pos: int) -> int:
	var y := editor_pos & 0xFFFF
	return y - 65536 if y > 32767 else y


func _rect_at(editor_pos: int) -> Rect2:
	return Rect2(
		_origin() + Vector2(float(_node_x_at(editor_pos)), float(_node_y_at(editor_pos))) * UNIT,
		Vector2(OBJ_WIDTH, OBJ_HEIGHT) * UNIT)


func _draw() -> void:
	var objectives := _objectives()
	_draw_grid()
	var positions: Array = []
	for objective: Variant in objectives:
		var value: Dictionary = objective if objective is Dictionary else {}
		var editor_pos := int(value.get("editorPos", 0))
		if not _has_pos(editor_pos):
			positions.append(Rect2())
			continue
		positions.append(_rect_at(editor_pos))
	for edge: Variant in _edges(objectives):
		var pair: Array = edge
		if pair[0] >= positions.size() or pair[1] >= positions.size():
			continue
		var from_rect: Rect2 = positions[pair[0]]
		var to_rect: Rect2 = positions[pair[1]]
		if from_rect.size == Vector2.ZERO or to_rect.size == Vector2.ZERO:
			continue
		draw_line(from_rect.get_center(), to_rect.get_center(),
			Color(0.98, 0.78, 0.34), 2.0)
	for index in positions.size():
		var rect: Rect2 = positions[index]
		if rect.size == Vector2.ZERO:
			continue
		var value: Dictionary = objectives[index]
		draw_rect(rect, Color(0.2, 0.2, 0.25, 0.9), true)
		draw_rect(rect, Color("ffd37f") if index == _selected else Color(0.85, 0.42, 0.33),
			false, 3.0 if index == _selected else 2.0)
		var type_label := _type_name(str(value.get("class", "")))
		draw_string(ThemeDB.fallback_font, rect.position + Vector2(8.0, 24.0), type_label,
			HORIZONTAL_ALIGNMENT_LEFT, rect.size.x - 16.0, 16, Color(0.9, 0.9, 0.9))
	if _querying:
		var cell := _query_cell(_cursor)
		var valid := valid_place(cell.x, cell.y)
		var ghost := Rect2(_origin() + Vector2(cell.x, cell.y) * UNIT,
			Vector2(OBJ_WIDTH, OBJ_HEIGHT) * UNIT)
		var ghost_color := Color("ffd37f") if valid else Color("ff6655")
		ghost_color.a = 0.4
		draw_rect(ghost, ghost_color, true)
		draw_rect(ghost, Color("ffd37f") if valid else Color("ff6655"), false, 3.0)
	if _moving >= 0:
		var cell := _move_cell()
		var valid := valid_place(cell.x, cell.y, _moving)
		var ghost := Rect2(_origin() + Vector2(cell.x, cell.y) * UNIT,
			Vector2(OBJ_WIDTH, OBJ_HEIGHT) * UNIT)
		var ghost_color := Color("ffd37f") if valid else Color("ff6655")
		ghost_color.a = 0.5
		draw_rect(ghost, ghost_color, true)


## `ObjectiveTilemap.draw`'s grid over the ±`bounds` play area.
func _draw_grid() -> void:
	var origin := _origin()
	# `Lines.stroke(3f); Draw.color(Pal.darkestGray)` (`MapObjectivesCanvas.java:152-156`).
	var color := Color(0.1, 0.1, 0.1)
	var start := -BOUNDS
	for i in range(2 * BOUNDS + 1):
		var unit := float(start + i)
		var x := origin.x + unit * UNIT
		draw_line(Vector2(x, origin.y - BOUNDS * UNIT), Vector2(x, origin.y + BOUNDS * UNIT), color, 3.0)
		var y := origin.y + unit * UNIT
		draw_line(Vector2(origin.x - BOUNDS * UNIT, y), Vector2(origin.x + BOUNDS * UNIT, y), color, 3.0)


## `obj.typeName()` (`ObjectiveMarker`-style bundle lookup with a class fallback).
func _type_name(tag: String) -> String:
	if tag.is_empty():
		return ""
	var key := "objective.%s.name" % tag.to_lower()
	var assets := MindWidgets.assets()
	if assets != null and assets.has_method("bundle_get"):
		var value := str(assets.call("bundle_get", key))
		if value != key:
			return value
	return tag


func _objectives() -> Array:
	if _editor == null or not _editor.has_method("objectives_json"):
		return []
	var parsed: Variant = JSON.parse_string(str(_editor.call("objectives_json")))
	return parsed if parsed is Array else []


func _edges(objectives: Array) -> Array:
	var out: Array = []
	for index in objectives.size():
		var value: Dictionary = objectives[index] if objectives[index] is Dictionary else {}
		for parent: Variant in value.get("parents", []):
			if int(parent) >= 0 and int(parent) < objectives.size():
				out.append([int(parent), index])
	return out
