## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/MapProcessorsDialog.java (plan 19 M6 §3.10).
##
## UI-only: search + processor rows. The `world-processor` scan and tag/icon
## reads are owned by plan 13's `LogicBlockState` via `MindEditor.processors`.

extends MindDialog

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"

var _editor: Node = null
var _list: VBoxContainer = null
var _search := ""


func _ready() -> void:
	super._ready()
	_editor = get_node_or_null(editor_path)
	title_text = "World Processors"
	add_close_button()


func shown() -> void:
	_build()


func _build() -> void:
	if cont == null:
		return
	for child in cont.get_children():
		child.queue_free()
	var search := LineEdit.new()
	search.placeholder_text = "@players.search"
	search.text_changed.connect(_on_search)
	cont.add_child(search)
	_list = VBoxContainer.new()
	cont.add_child(_list)
	_rebuild()


func _on_search(text: String) -> void:
	_search = text.to_lowercase()
	_rebuild()


func _rebuild() -> void:
	if _list == null:
		return
	for child in _list.get_children():
		child.queue_free()
	var entries := _processors()
	var shown := 0
	for entry: Variant in entries:
		var value: Dictionary = entry if entry is Dictionary else {}
		var tag := str(value.get("tag", "")).to_lowercase()
		if not _search.is_empty() and not tag.contains(_search):
			continue
		shown += 1
		var row := HBoxContainer.new()
		row.add_child(_label("[%d, %d] %s" % [int(value.get("x", 0)), int(value.get("y", 0)), str(value.get("tag", ""))]))
		_list.add_child(row)
	if shown == 0:
		_list.add_child(_label("@editor.worldprocessors.none"))


func _processors() -> Array:
	if _editor != null and _editor.has_method("processors"):
		var value: Variant = _editor.call("processors")
		if value is Array:
			return value
	return []


func _label(text: String) -> Label:
	var label := Label.new()
	label.text = text
	return label
