## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/logic/LogicDialog.java (plan 13 M4, I03 T7).
##
## The mlog editor shell: a scrolling statement list (`LCanvas`) plus the
## `@back`/`@edit`/`@variables`/`@add` button row. T7 renders each statement's
## header row (category colour, index, localized `lst.<name>` label and the
## copy/delete controls); the per-statement field layouts land in T8, the drag/
## jump/add dialogs in T9 and the variables/globals dialogs in T10.
##
## Load/save goes through `MindLogic.parse_statements` / `write_statements` and
## `SimHost.logic_configure_code` (`LogicBlock.showEditDialog`).

extends MindDialog

## `LogicDialog.show(code, null, true, consumer)`: `MapGenerateDialog`'s
## `LogicFilter` code option has no world tile; the edited code is echoed back
## through this signal instead of a processor write.
signal code_edited(code: String)

## `LCategory` colours (`Pal`; `LCategory.java:16-22`).
const CATEGORY_COLORS := {
	"io": Color8(0xa0, 0x8a, 0x8a),
	"block": Color8(0xd4, 0x81, 0x6b),
	"operation": Color8(0x87, 0x7b, 0xad),
	"control": Color8(0x6b, 0xb2, 0xb2),
	"unit": Color8(0xc7, 0xb5, 0x9d),
	"world": Color8(0x6b, 0x84, 0xd4),
	"unknown": Color8(0x4d, 0x4d, 0x4d),
}

var _x := 0
var _y := 0
var _privileged := false
var _code := ""
## Editor target (`MapProcessorsDialog`'s name button /
## `LogicBuild.showEditDialog(true)`): read/write through `MindEditor`.
var _editor_mode := false
## Standalone `show(code, null, true, consumer)` mode (`MapGenerateDialog`'s
## `LogicFilter` code option): no tile; the code is echoed via `code_edited`.
var _filter_mode := false
## `[{name, category, fields: [String]}]`.
var _statements: Array = []
var _list: MindTable = null
var _modified := false


func _ready() -> void:
	should_pause = true
	full_dialog = true
	super._ready()
	set_title_text(_t("@logic"))
	_build()


func _build() -> void:
	var table := content_table()
	_list = MindTable.new()
	# code-instantiated: the statement list is one scrolling stack whose row
	# count is the program length, not scene-authorable.
	_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_list.size_flags_vertical = Control.SIZE_EXPAND_FILL
	# code-instantiated: the scroll body owns the runtime-sized statement list.
	var scroll := MindScroll.new()
	scroll.custom_minimum_size = Vector2(520, 320)
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.add_child(_list)
	table.add(scroll).grow_x_axis().grow_y_axis().pad(4)
	table.row()

	var controls := HBoxContainer.new()
	table.add(controls).pad(4)
	var back := MindWidgets.icon_button("left", _t("@back"), "cleart")
	back.pressed.connect(_close)
	controls.add_child(back)
	var edit := MindWidgets.icon_button("edit", _t("@edit"), "cleart")
	edit.pressed.connect(_open_edit_menu)
	controls.add_child(edit)
	var variables := MindWidgets.icon_button("menu", _t("@variables"), "cleart")
	variables.disabled = true
	variables.tooltip_text = _t("@variables")
	controls.add_child(variables)
	var add := MindWidgets.icon_button("add", _t("@add"), "cleart")
	add.disabled = true
	add.tooltip_text = _t("@add")
	controls.add_child(add)


## `LogicDialog.show`: loads the processor's program into the statement list.
## `context.editor` targets an editor world tile (`MindEditor`,
## `MapProcessorsDialog`'s name button -> `showEditDialog(true)`);
## `context.filter` is the standalone `show(code, null, true, consumer)` mode
## (the map-generate `LogicFilter` code option); otherwise the live sim
## (`LogicBlock` config).
func shown() -> void:
	_x = int(_context.get("x", 0))
	_y = int(_context.get("y", 0))
	_editor_mode = bool(_context.get("editor", false))
	_filter_mode = bool(_context.get("filter", false))
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null:
		return
	if _filter_mode:
		_privileged = bool(_context.get("privileged", true))
		_code = str(_context.get("code", ""))
	elif _editor_mode:
		var editor := get_node_or_null("/root/Spine/MindEditor")
		if editor == null:
			return
		var state: Variant = editor.call("logic_get_state", _x, _y)
		if state is Dictionary:
			_privileged = bool(state.get("privileged", false))
			_code = str(state.get("code", ""))
		else:
			_code = ""
	else:
		var host := get_node_or_null("/root/Spine/SimHost")
		if host == null:
			return
		var state: Variant = host.call("logic_get_state", _x, _y)
		if state is Dictionary:
			_privileged = bool(state.get("privileged", false))
		_code = str(host.call("logic_get_code", _x, _y))
	var parsed: Variant = logic.call("parse_statements", _code, _privileged)
	_statements = parsed if parsed is Array else []
	_modified = false
	_rebuild_list()


## `LogicDialog.hidden`: saves through `LogicBlock.showEditDialog`'s callback.
func hidden() -> void:
	if not _modified:
		return
	_save()


func _rebuild_list() -> void:
	if _list == null:
		return
	_list.clear()
	for index in _statements.size():
		var row: Variant = _statements[index]
		if not (row is Dictionary):
			continue
		_list.add(_statement_row(index, row)).grow_x_axis().pad(2)
		_list.row()


## One statement element: the header row (`LCanvas.StatementElem`) plus the
## generic field builder (T8; `LStatement.field`/`fields`).
func _statement_row(index: int, row: Dictionary) -> Control:
	var name := str(row.get("name", "noop"))
	var category := str(row.get("category", "unknown"))
	# code-instantiated: one element per statement; the count is the program
	# length, not scene-authorable.
	var panel := PanelContainer.new()
	panel.modulate = CATEGORY_COLORS.get(category, CATEGORY_COLORS["unknown"])
	var column := VBoxContainer.new()
	panel.add_child(column)
	var box := HBoxContainer.new()
	column.add_child(box)
	var label := Label.new()
	label.text = "%d  %s" % [index, _t("lst." + str(row.get("bundle", name)))]
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	box.add_child(label)
	var copy := MindWidgets.glyph_button("copy", "C", "clearNonei")
	copy.custom_minimum_size = Vector2(24, 24)
	copy.tooltip_text = _t("@copy")
	copy.pressed.connect(_copy_statement.bind(index))
	box.add_child(copy)
	var delete := MindWidgets.glyph_button("cancel", "X", "clearNonei")
	delete.custom_minimum_size = Vector2(24, 24)
	delete.tooltip_text = _t("@delete")
	delete.pressed.connect(_delete_statement.bind(index))
	box.add_child(delete)
	column.add_child(_field_row(index, row))
	return panel


## The generic field builder (`LStatement.fields`, `LStatement.java:233-287`):
## one labelled text field per `StatementField`, in serialization order.
func _field_row(index: int, row: Dictionary) -> Control:
	var fields_box := HBoxContainer.new()
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null:
		return fields_box
	var name := str(row.get("name", "noop"))
	var values: Array = row.get("fields", [])
	var metas: Variant = logic.call("statement_fields", name)
	if not (metas is Array):
		return fields_box
	for field_index in (metas as Array).size():
		var meta: Variant = (metas as Array)[field_index]
		if not (meta is Dictionary):
			continue
		var field_name := str((meta as Dictionary).get("name", ""))
		var label := Label.new()
		label.text = field_name
		label.tooltip_text = "%s.%s" % [name, field_name]
		fields_box.add_child(label)
		var value := str(values[field_index]) if field_index < values.size() else ""
		var edit := LineEdit.new()
		edit.text = value
		edit.custom_minimum_size = Vector2(90, 32)
		edit.tooltip_text = "%s.%s" % [name, field_name]
		edit.text_submitted.connect(_set_field.bind(index, field_index))
		edit.focus_exited.connect(_commit_field.bind(index, field_index, edit))
		fields_box.add_child(edit)
	return fields_box


## `field(table, value, setter)`: `setter.get(sanitize(text))`.
func _set_field(text: String, index: int, field_index: int) -> void:
	_commit_field_value(text, index, field_index)


## Commit on focus loss (`TextField.changed` in Java runs per keystroke; the
## port commits on submit/blur so typing stays cheap).
func _commit_field(index: int, field_index: int, edit: LineEdit) -> void:
	_commit_field_value(edit.text, index, field_index)


func _commit_field_value(text: String, index: int, field_index: int) -> void:
	if index < 0 or index >= _statements.size():
		return
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null:
		return
	var sanitized := str(logic.call("sanitize", text))
	var row: Dictionary = _statements[index]
	var values: Array = row.get("fields", [])
	if field_index >= values.size():
		return
	if str(values[field_index]) == sanitized:
		return
	values[field_index] = sanitized
	row["fields"] = values
	_statements[index] = row
	_modified = true


## `StatementElem.copy`: duplicates the element after itself.
func _copy_statement(index: int) -> void:
	if index < 0 or index >= _statements.size():
		return
	_statements.insert(index + 1, (_statements[index] as Dictionary).duplicate(true))
	_modified = true
	_rebuild_list()


func _delete_statement(index: int) -> void:
	if index < 0 or index >= _statements.size():
		return
	_statements.remove_at(index)
	_modified = true
	_rebuild_list()


## `LogicDialog`'s `@edit` sub-dialog (`LogicDialog.java:141-190`): a popup with
## `@editor.worldprocessors.editname` (privileged only), `@clear`,
## `@copy.clipboard`, `@load.clipboard` and `@logic.restart`.
func _open_edit_menu() -> void:
	var popup := PopupPanel.new()
	# code-instantiated: the edit menu is a transient popup over the dialog.
	add_child(popup)
	var box := VBoxContainer.new()
	popup.add_child(box)
	var clear := MindWidgets.button(_t("@clear"))
	clear.pressed.connect(
		func() -> void:
			_statements.clear()
			_modified = true
			_rebuild_list()
			popup.hide()
	)
	box.add_child(clear)
	var copy_buffer := MindWidgets.button(_t("@copy.clipboard"))
	copy_buffer.pressed.connect(
		func() -> void:
			var logic := get_node_or_null("/root/Spine/MindLogic")
			if logic != null:
				DisplayServer.clipboard_set(str(logic.call("write_statements", _statements)))
			popup.hide()
	)
	box.add_child(copy_buffer)
	var load_buffer := MindWidgets.button(_t("@load.clipboard"))
	load_buffer.pressed.connect(
		func() -> void:
			_load_from_clipboard()
			popup.hide()
	)
	box.add_child(load_buffer)
	var restart := MindWidgets.button(_t("@logic.restart"))
	restart.pressed.connect(
		func() -> void:
			_save()
			popup.hide()
	)
	box.add_child(restart)
	popup.popup_centered()


## `@load.clipboard`: parse the clipboard text into the statement list.
func _load_from_clipboard() -> void:
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null:
		return
	var text := DisplayServer.clipboard_get()
	if text.strip_edges().is_empty():
		return
	var parsed: Variant = logic.call("parse_statements", text, _privileged)
	if parsed is Array:
		_statements = parsed
		_modified = true
		_rebuild_list()


## `LCanvas.save` -> `LogicBuild.showEditDialog`'s consumer.
func _save() -> void:
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null:
		return
	var code := str(logic.call("write_statements", _statements))
	if _filter_mode:
		# `modified.get(result)`: the generate dialog owns the filter JSON.
		_code = code
		_modified = false
		code_edited.emit(code)
		return
	var error := ""
	if _editor_mode:
		var editor := get_node_or_null("/root/Spine/MindEditor")
		if editor == null:
			return
		error = str(editor.call("logic_configure_code", _x, _y, code))
	else:
		var host := get_node_or_null("/root/Spine/SimHost")
		if host == null:
			return
		error = str(host.call("logic_configure_code", _x, _y, code))
	if error.is_empty():
		_code = code
		_modified = false
	elif error.begins_with("toolong:"):
		_show_toolong(error.trim_prefix("toolong:").to_int())


func _show_toolong(length: int) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("toast"):
		return
	var assets := get_node_or_null("/root/MindAssets")
	var text := "too long"
	if assets != null and assets.has_method("bundle_format"):
		text = str(assets.call("bundle_format", "logic.error.toolong", ["16000", str(length)]))
	ui.call("toast", text, "error")


## Closes through `MindUi` so the stack and the pause governor stay in sync
## (`BaseDialog.hide`); the manifest name is `logic`.
func _close() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("close_dialog"):
		if bool(ui.call("close_dialog", "logic")):
			return
	hide_dialog()
