## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/MapResizeDialog.java
##
## UI-only: the four fields with Java's filters (`digitsOnly` + the
## `minSize..maxSize` validity for size, any int for the shifts), then the
## `@cancel`/`@ok` buttons calling `MindEditor.resize_map(w, h, sx, sy)`.

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
	set_title_key("@editor.resizemap")
	should_pause = false
	super._ready()
	_editor = get_node_or_null(editor_path)
	clear_buttons()
	# `buttons.defaults().size(200f, 50f); button("@cancel"); button("@ok")`.
	add_button(_t("@cancel"), _close_pressed, "", 200.0)
	add_button(_t("@ok"), _ok, "", 200.0)


## `shown(() -> { cont.clear(); width = editor.width(); ... })`.
func shown() -> void:
	var status := _status()
	_width = int(status.get("width", MIN_SIZE))
	_height = int(status.get("height", MIN_SIZE))
	_build()


func _build() -> void:
	if cont == null:
		return
	for child in cont.get_children():
		cont.remove_child(child)
		child.queue_free()
	var root := content_table()
	var table := VBoxContainer.new()
	table.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	root.add(table).grow_x_axis().pad(4)
	# `for(boolean w : Mathf.booleans)`: width then height, then the shifts.
	table.add_child(_field_row(_t("@width"), str(_width), 3, true))
	table.add_child(_field_row(_t("@height"), str(_height), 3, false))
	table.add_child(_shift_row(_t("@editor.shiftx"), str(_shift_x), true))
	table.add_child(_shift_row(_t("@editor.shifty"), str(_shift_y), false))


## One size row (`table.field(...).valid(canParsePositiveInt && in range)`).
func _field_row(label_text: String, value: String, max_length: int, horizontal: bool) -> Control:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 8)
	var label := MindWidgets.label(label_text)
	label.custom_minimum_size.x = 90.0
	row.add_child(label)
	var edit := MindWidgets.field("")
	edit.text = value
	edit.max_length = max_length
	edit.custom_minimum_size = Vector2(180.0, 60.0)
	edit.text_changed.connect(_on_size_changed.bind(horizontal))
	row.add_child(edit)
	return row


## `digitsOnly` filter + `valid(value)` (`50 <= value <= 800`).
func _on_size_changed(text: String, horizontal: bool) -> void:
	if text.is_empty() or not text.is_valid_int():
		return
	var value := int(text)
	if value < MIN_SIZE or value > MAX_SIZE:
		return
	if horizontal:
		_width = value
	else:
		_height = value


## One shift row (`Strings::canParseInt`, max length 4).
func _shift_row(label_text: String, value: String, horizontal: bool) -> Control:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 8)
	var label := MindWidgets.label(label_text)
	label.custom_minimum_size.x = 90.0
	row.add_child(label)
	var edit := MindWidgets.field("")
	edit.text = value
	edit.max_length = 4
	edit.custom_minimum_size = Vector2(180.0, 60.0)
	# Leading `-` stays typeable; the value commits only on a parseable string.
	edit.text_changed.connect(func(text: String) -> void:
		if text.is_empty() or text == "-" or not text.is_valid_int():
			return
		if horizontal:
			_shift_x = int(text)
		else:
			_shift_y = int(text))
	row.add_child(edit)
	return row


## `cons.get(width, height, shiftX, shiftY)` + `hide()`.
func _ok() -> void:
	if _editor != null and _editor.has_method("resize_map"):
		_editor.call("resize_map", _width, _height, _shift_x, _shift_y)
	_close_pressed()


func _status() -> Dictionary:
	if _editor != null and _editor.has_method("status"):
		var value: Variant = _editor.call("status")
		if value is Dictionary:
			return value
	return {}
