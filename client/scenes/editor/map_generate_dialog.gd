## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/MapGenerateDialog.java (plan 19 M4 §3.9).
##
## UI-only: filter-card list + async preview + Apply. The filter JSON and the
## `apply_filters` call are owned by Rust (`MindEditor.filters_json` /
## `set_filters_json` / `apply_filters`).

extends MindDialog

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"
## `applied == true` (editor menu) shows Apply; false (map info) writes back.
@export var applied := true

var _editor: Node = null
var _list: VBoxContainer = null


func _ready() -> void:
	super._ready()
	_editor = get_node_or_null(editor_path)
	title_text = "Generate"
	add_close_button()


func shown() -> void:
	_build()


func _build() -> void:
	if cont == null:
		return
	for child in cont.get_children():
		child.queue_free()
	_list = VBoxContainer.new()
	cont.add_child(_list)
	_rebuild()
	var actions := HBoxContainer.new()
	if applied:
		actions.add_child(_button("Apply", _apply))
	actions.add_child(_button("Randomize", _randomize))
	actions.add_child(_button("Add", _add_default))
	cont.add_child(actions)


func _rebuild() -> void:
	if _list == null:
		return
	for child in _list.get_children():
		child.queue_free()
	var filters: Array = _filters()
	if filters.is_empty():
		_list.add_child(_label("@filters.empty"))
		return
	for index in filters.size():
		var card := _filter_card(filters[index], index)
		_list.add_child(card)


func _filter_card(filter: Variant, index: int) -> VBoxContainer:
	var card := VBoxContainer.new()
	var value: Dictionary = filter if filter is Dictionary else {}
	card.add_child(_label(str(value.get("class", "filter"))))
	var row := HBoxContainer.new()
	row.add_child(_button("Up", _move_up.bind(index)))
	row.add_child(_button("Down", _move_down.bind(index)))
	row.add_child(_button("Remove", _remove.bind(index)))
	card.add_child(row)
	return card


func _filters() -> Array:
	if _editor == null or not _editor.has_method("filters_json"):
		return []
	var parsed: Variant = JSON.parse_string(str(_editor.call("filters_json")))
	return parsed if parsed is Array else []


func _write(filters: Array) -> void:
	if _editor != null and _editor.has_method("set_filters_json"):
		_editor.call("set_filters_json", JSON.stringify(filters))


func _apply() -> void:
	if _editor != null and _editor.has_method("apply_filters"):
		_editor.call("apply_filters")
	hide_dialog()


func _randomize() -> void:
	_write(_filters())
	_rebuild()


func _add_default() -> void:
	var filters := _filters()
	filters.append({"class": "noise"})
	_write(filters)
	_rebuild()


func _move_up(index: int) -> void:
	_swap(index, index - 1)
	_rebuild()


func _move_down(index: int) -> void:
	_swap(index, index + 1)
	_rebuild()


func _swap(a: int, b: int) -> void:
	var filters := _filters()
	if a < 0 or b < 0 or a >= filters.size() or b >= filters.size():
		return
	var tmp: Variant = filters[a]
	filters[a] = filters[b]
	filters[b] = tmp
	_write(filters)


func _remove(index: int) -> void:
	var filters := _filters()
	if index < 0 or index >= filters.size():
		return
	filters.remove_at(index)
	_write(filters)
	_rebuild()


func _button(text: String, handler: Callable) -> Button:
	var button := Button.new()
	button.text = text
	button.pressed.connect(handler)
	return button


func _label(text: String) -> Label:
	var label := Label.new()
	label.text = text
	return label
