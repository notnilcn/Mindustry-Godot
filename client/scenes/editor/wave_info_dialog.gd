## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/WaveInfoDialog.java (plan 19 M5 §3.9).
##
## UI-only: group-list editor + WaveGraph. `SpawnGroup` JSON is owned by plan 12;
## Rust validates/round-trips it through `MindEditor.waves_json`.

extends MindDialog

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"

var _editor: Node = null
var _list: VBoxContainer = null


func _ready() -> void:
	super._ready()
	_editor = get_node_or_null(editor_path)
	title_text = "Waves"
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
	var actions := HBoxContainer.new()
	actions.add_child(_button("Add", _add))
	actions.add_child(_button("Clear", _clear))
	cont.add_child(actions)
	_rebuild()


func _rebuild() -> void:
	if _list == null:
		return
	for child in _list.get_children():
		child.queue_free()
	var groups := _groups()
	if groups.is_empty():
		_list.add_child(_label("@waves.none"))
		return
	for index in groups.size():
		_list.add_child(_group_row(groups[index], index))


func _group_row(group: Variant, index: int) -> HBoxContainer:
	var value: Dictionary = group if group is Dictionary else {}
	var row := HBoxContainer.new()
	row.add_child(_label(str(value.get("type", "?"))))
	row.add_child(_button("Remove", _remove.bind(index)))
	return row


func _groups() -> Array:
	if _editor == null or not _editor.has_method("waves_json"):
		return []
	var parsed: Variant = JSON.parse_string(str(_editor.call("waves_json")))
	return parsed if parsed is Array else []


func _write(groups: Array) -> void:
	if _editor != null and _editor.has_method("set_waves_json"):
		_editor.call("set_waves_json", JSON.stringify(groups))


func _add() -> void:
	var groups := _groups()
	groups.append({"type": "dagger"})
	_write(groups)
	_rebuild()


func _remove(index: int) -> void:
	var groups := _groups()
	if index >= 0 and index < groups.size():
		groups.remove_at(index)
		_write(groups)
		_rebuild()


func _clear() -> void:
	_write([])
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
