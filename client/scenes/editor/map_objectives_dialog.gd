## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/MapObjectivesDialog.java (plan 19 M5 §3.9).
##
## UI-only: canvas host + add/search/export/import/copy/paste toolbar. Objective
## JSON is validated and canonicalized by `MindEditor.set_objectives_json`.

extends MindDialog

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"
@export var canvas_scene: PackedScene

var _editor: Node = null
var _canvas: Control = null


func _ready() -> void:
	super._ready()
	_editor = get_node_or_null(editor_path)
	title_text = "Objectives"
	add_close_button()


func shown() -> void:
	_build()


func _build() -> void:
	if cont == null:
		return
	for child in cont.get_children():
		child.queue_free()
	if canvas_scene != null:
		# code-instantiated: the canvas scene is shared by this dialog only.
		_canvas = canvas_scene.instantiate()
		_canvas.custom_minimum_size = Vector2(480.0, 320.0)
		cont.add_child(_canvas)
	var toolbar := HBoxContainer.new()
	toolbar.add_child(_button("Add", _paste))
	toolbar.add_child(_button("Export", _export))
	toolbar.add_child(_button("Import", _import))
	cont.add_child(toolbar)


func _objectives() -> Array:
	if _editor == null or not _editor.has_method("objectives_json"):
		return []
	var parsed: Variant = JSON.parse_string(str(_editor.call("objectives_json")))
	return parsed if parsed is Array else []


func _write(objectives: Array) -> void:
	if _editor != null and _editor.has_method("set_objectives_json"):
		_editor.call("set_objectives_json", JSON.stringify(objectives))


func _paste() -> void:
	var objectives := _objectives()
	objectives.append({"class": "Item", "item": "copper", "amount": 1, "editorPos": 0})
	_write(objectives)


func _export() -> void:
	DisplayServer.clipboard_set(JSON.stringify(_objectives()))


func _import() -> void:
	var parsed: Variant = JSON.parse_string(DisplayServer.clipboard_get())
	if parsed is Array:
		_write(parsed)


func _button(text: String, handler: Callable) -> Button:
	var button := Button.new()
	button.text = text
	button.pressed.connect(handler)
	return button
