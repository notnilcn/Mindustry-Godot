## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/MapObjectivesCanvas.java (plan 19 M5 §3.9).
##
## Presentation-only canvas: pan, node tiles, connector lines and the query
## cursor. The objective graph data comes from `MindEditor.objectives_json`.

extends Control

## `MapObjectivesCanvas.{objWidth, objHeight, bounds}` in placement units.
const OBJ_WIDTH := 5
const OBJ_HEIGHT := 2
const BOUNDS := 100
const UNIT := 48.0

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"

var _editor: Node = null
var _pan := Vector2.ZERO
var _dragging := false


func _ready() -> void:
	_editor = get_node_or_null(editor_path)
	mouse_filter = Control.MOUSE_FILTER_STOP


func _gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		if event.button_index == MOUSE_BUTTON_MIDDLE or event.button_index == MOUSE_BUTTON_LEFT:
			_dragging = event.pressed
		queue_redraw()
	elif event is InputEventMouseMotion and _dragging:
		_pan += event.relative
		queue_redraw()


func _draw() -> void:
	var objectives := _objectives()
	var positions: Array = []
	for objective: Variant in objectives:
		var value: Dictionary = objective if objective is Dictionary else {}
		positions.append(_node_rect(int(value.get("editorPos", 0))))
	for edge: Variant in _edges(objectives):
		var pair: Array = edge
		draw_line(positions[pair[0]].get_center(), positions[pair[1]].get_center(), Color(0.98, 0.78, 0.34), 2.0)
	for index in positions.size():
		draw_rect(positions[index], Color(0.2, 0.2, 0.25, 0.9), true)
		draw_rect(positions[index], Color(0.85, 0.42, 0.33), false, 2.0)


func _node_rect(editor_pos: int) -> Rect2:
	var x := float((editor_pos >> 16) & 0xFFFF)
	var y := float(editor_pos & 0xFFFF)
	if x > 32767.0:
		x -= 65536.0
	if y > 32767.0:
		y -= 65536.0
	var origin := size * 0.5 + _pan
	return Rect2(origin + Vector2(x, y) * UNIT, Vector2(OBJ_WIDTH, OBJ_HEIGHT) * UNIT)


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
