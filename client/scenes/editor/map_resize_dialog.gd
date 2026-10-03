## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/MapResizeDialog.java (plan 19 M4 §3.10).
##
## UI-only: 50..800/step-50 width/height + any-int shifts, then
## `MindEditor.resize_map(w, h, sx, sy)`.

extends MindDialog

## `MapResizeDialog.{minSize, maxSize, increment}`.
const MIN_SIZE := 50
const MAX_SIZE := 800
const INCREMENT := 50

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"

var _editor: Node = null
var _width := 0
var _height := 0
var _shift_x := 0
var _shift_y := 0


func _ready() -> void:
	super._ready()
	_editor = get_node_or_null(editor_path)
	title_text = "Resize Map"
	add_close_button()


func shown() -> void:
	var status := _status()
	_width = int(status.get("width", MIN_SIZE))
	_height = int(status.get("height", MIN_SIZE))
	_build()


func _build() -> void:
	if cont == null:
		return
	for child in cont.get_children():
		child.queue_free()
	_size_field("Width", true)
	_size_field("Height", false)
	_shift_field("Shift X", true)
	_shift_field("Shift Y", false)
	# code-instantiated: the OK button is part of this dialog's own button row.
	var ok := Button.new()
	ok.text = "OK"
	ok.pressed.connect(_apply)
	if buttons != null:
		buttons.add_child(ok)


func _size_field(text: String, horizontal: bool) -> void:
	var row := HBoxContainer.new()
	row.add_child(_label(text))
	var edit := LineEdit.new()
	edit.text = str(_width if horizontal else _height)
	edit.max_length = 3
	edit.text_changed.connect(_on_size_changed.bind(horizontal))
	row.add_child(edit)
	cont.add_child(row)


func _on_size_changed(text: String, horizontal: bool) -> void:
	var value := int(text)
	if value < MIN_SIZE or value > MAX_SIZE:
		return
	if horizontal:
		_width = value
	else:
		_height = value


func _shift_field(text: String, horizontal: bool) -> void:
	var row := HBoxContainer.new()
	row.add_child(_label(text))
	var edit := LineEdit.new()
	edit.text = str(_shift_x if horizontal else _shift_y)
	edit.max_length = 4
	edit.text_changed.connect(_on_shift_changed.bind(horizontal))
	row.add_child(edit)
	cont.add_child(row)


func _on_shift_changed(text: String, horizontal: bool) -> void:
	if horizontal:
		_shift_x = int(text)
	else:
		_shift_y = int(text)


func _label(text: String) -> Label:
	var label := Label.new()
	label.text = text
	return label


func _status() -> Dictionary:
	if _editor != null and _editor.has_method("status"):
		var value: Variant = _editor.call("status")
		if value is Dictionary:
			return value
	return {}


func _apply() -> void:
	if _editor != null and _editor.has_method("resize_map"):
		_editor.call("resize_map", _width, _height, _shift_x, _shift_y)
	hide_dialog()
