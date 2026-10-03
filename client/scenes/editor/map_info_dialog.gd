## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/MapInfoDialog.java (plan 19 M4 §3.10).
##
## UI-only: metadata fields and the seven sub-dialog routes. Values are written
## to `/root/Spine/MindEditor` tags; every route opens its own dialog scene.

extends MindDialog

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"
@export var routes: Array[String] = [
	"map_info", "wave_info", "map_objectives", "map_generate",
	"map_locales", "map_processors", "map_assets",
]

var _editor: Node = null
var _name: LineEdit = null
var _description: TextEdit = null
var _author: LineEdit = null


func _ready() -> void:
	super._ready()
	_editor = get_node_or_null(editor_path)
	title_text = "Map Info"
	title_color = MindStyles.ACCENT if Engine.has_singleton("MindStyles") else title_color


func shown() -> void:
	_build()


## Rebuilds the metadata fields from the current tags.
func _build() -> void:
	if cont == null:
		return
	for child in cont.get_children():
		child.queue_free()
	var tags := _tags()
	_name = _field("name", str(tags.get("name", "")), 50)
	_description = _area("description", str(tags.get("description", "")), 1000)
	_author = _field("author", str(tags.get("author", "")), 50)
	var row := HBoxContainer.new()
	for route in routes:
		# code-instantiated: the seven route buttons are data-driven.
		var button := Button.new()
		button.text = route
		button.pressed.connect(func() -> void: _open_route(route))
		row.add_child(button)
	cont.add_child(row)


func _tags() -> Dictionary:
	if _editor != null and _editor.has_method("tags"):
		var value: Variant = _editor.call("tags")
		if value is Dictionary:
			return value
	return {}


func _field(key: String, value: String, max_length: int) -> LineEdit:
	var edit := LineEdit.new()
	edit.text = value
	edit.max_length = max_length
	edit.text_changed.connect(func(text: String) -> void: _set_tag(key, text))
	cont.add_child(edit)
	return edit


func _area(key: String, value: String, max_length: int) -> TextEdit:
	var edit := TextEdit.new()
	edit.text = value
	cont.add_child(edit)
	edit.text_changed.connect(func() -> void: _set_tag(key, edit.text))
	if max_length > 0 and edit.text.length() > max_length:
		edit.text = edit.text.substr(0, max_length)
	return edit


func _set_tag(key: String, value: String) -> void:
	if _editor != null and _editor.has_method("set_tag"):
		_editor.call("set_tag", key, value)


func _open_route(route: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("open_dialog"):
		ui.call("open_dialog", route, "{}")
