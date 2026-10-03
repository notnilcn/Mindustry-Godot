## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/MapLoadDialog.java (plan 19 M4 §3.10).
##
## UI-only: a 250x90 map-button grid. Load stays disabled until a row is picked;
## selecting calls `MindEditor.begin_edit_map(path)`.

extends MindDialog

## `MapLoadDialog.buttonSize`.
const BUTTON_SIZE := Vector2(250.0, 90.0)

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"
@export var map_dir := "user://maps"

var _editor: Node = null
var _selected := ""
var _load: Button = null


func _ready() -> void:
	super._ready()
	_editor = get_node_or_null(editor_path)
	title_text = "Load Map"
	add_close_button()
	_load = Button.new()
	_load.text = "Load"
	_load.disabled = true
	_load.pressed.connect(_load_selected)
	if buttons != null:
		buttons.add_child(_load)


func shown() -> void:
	_build()


func _build() -> void:
	if cont == null:
		return
	for child in cont.get_children():
		child.queue_free()
	var dir := DirAccess.open(map_dir)
	if dir == null:
		var empty := Label.new()
		empty.text = "@maps.none"
		cont.add_child(empty)
		return
	var grid := GridContainer.new()
	grid.columns = 2
	for file in dir.get_files():
		if not file.ends_with(".msav"):
			continue
		var button := Button.new()
		button.text = file.get_basename()
		button.custom_minimum_size = BUTTON_SIZE
		button.pressed.connect(_select.bind(map_dir.path_join(file)))
		grid.add_child(button)
	cont.add_child(grid)


func _select(path: String) -> void:
	_selected = path
	if _load != null:
		_load.disabled = false


func _load_selected() -> void:
	if _selected.is_empty():
		return
	if _editor != null and _editor.has_method("begin_edit_map"):
		_editor.call("begin_edit_map", _selected)
	hide_dialog()
