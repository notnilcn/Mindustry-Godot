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
## Variables panel state (`LogicDialog`'s 15-frame refresh).
var _vars_popup: PopupPanel = null
var _vars_rows: Array = []
var _vars_timer := 0
## Statement element nodes (parallel to `_statements`; `LCanvas.StatementElem`).
var _row_nodes: Array = []
## `index -> JumpButton` for the jump elements (`LCanvas.JumpButton`).
var _jump_buttons: Dictionary = {}
## Jump-curve + insertion overlay (`LCanvas.DragLayout.draw`, `JumpCurve`).
var _overlay: Control = null
## Drag-reorder state (`LCanvas.DragLayout`): dragged element, pointer origin
## and the current insertion index.
var _drag_index := -1
var _drag_active := false
var _drag_start := Vector2.ZERO
var _drop_index := -1
## Jump-target selection state (`LCanvas.JumpButton` touch drag).
var _jump_selecting := -1
var _jump_hover := -1
var _jump_moved := false


func _ready() -> void:
	should_pause = true
	full_dialog = true
	super._ready()
	set_title_text(_t("@logic"))
	_build()
	# `LCanvas.DragLayout`/`JumpCurve` redraw every frame while visible.
	set_process(true)


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
	variables.pressed.connect(_open_variables_panel)
	controls.add_child(variables)
	var add := MindWidgets.icon_button("add", _t("@add"), "cleart")
	add.pressed.connect(_open_add_dialog)
	controls.add_child(add)

	# code-instantiated: the jump-arrow and drop-line overlay sits above the
	# statement list (`LCanvas.DragLayout.draw` + `JumpCurve`); it never takes
	# mouse events.
	_overlay = Control.new()
	_overlay.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_overlay.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(_overlay)
	_overlay.draw.connect(_draw_overlay)


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
	_row_nodes.clear()
	_jump_buttons.clear()
	for index in _statements.size():
		var row: Variant = _statements[index]
		if not (row is Dictionary):
			continue
		var panel := _statement_row(index, row)
		_list.add(panel).grow_x_axis().pad(2)
		_list.row()
		_row_nodes.append(panel)
	# A rebuilt grid gets no deferred sort (dialogs call `sort_now` + a
	# deferred repeat once the cells are added; `MindTable.sort_now`).
	_list.sort_now()
	_list.call_deferred("sort_now")
	if _overlay != null:
		_overlay.queue_redraw()


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
	# `JumpStatement.build`: `jump -> <dest>` while a target is set.
	if name == "jump" and _jump_target(row) >= 0:
		label.text = "%d  %s -> %d" % [
			index, _t("lst." + str(row.get("bundle", name))), _jump_target(row)
		]
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
	# `StatementElem`'s header drag: left-drag reorders, middle-click copies.
	box.gui_input.connect(_header_input.bind(index, box))
	column.add_child(_field_row(index, row))
	return panel


## `StatementElem`'s header `touchDown`/`touchDragged`/`touchUp`
## (`LCanvas.java:465-513`): left-drag reorders (the drop lands at
## `insertPosition`), middle-click copies.
func _header_input(event: InputEvent, index: int, box: Control) -> void:
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_MIDDLE and event.pressed:
		_copy_statement(index)
		box.accept_event()
		return
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT:
		if event.pressed:
			_drag_index = index
			_drag_active = false
			_drag_start = event.position
			box.accept_event()
		else:
			if _drag_active:
				_finish_drag()
			_drag_index = -1
			_drag_active = false
			if _overlay != null:
				_overlay.queue_redraw()
			box.accept_event()
		return
	if event is InputEventMouseMotion and _drag_index == index:
		if not _drag_active and event.position.distance_to(_drag_start) > 8.0:
			_drag_active = true
		if _drag_active:
			_drop_index = _drop_index_at(box.get_global_transform() * event.position)
			if _overlay != null:
				_overlay.queue_redraw()
		box.accept_event()


## The per-statement field row (`LStatement.build(Table)`): `MindLogic.editor_layout`
## returns the exact Java item order (labels, "="/"linknum" separators, enum
## selects, and the colour/align/char pencils).
func _field_row(index: int, row: Dictionary) -> Control:
	var fields_box := HBoxContainer.new()
	# code-instantiated: the field row is rebuilt per statement on every list
	# rebuild (`LCanvas.StatementElem` calls `st.build(t)`).
	fields_box.add_theme_constant_override("separation", 4)
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null:
		return fields_box
	var name := str(row.get("name", "noop"))
	var values: Array = row.get("fields", [])
	# `SetRuleStatement.rebuild` normalizes `p1` to `@sharded` for team rules;
	# Java mutates the statement at build time, so the dialog mirrors it.
	var normalized: Variant = logic.call("editor_normalize", name, PackedStringArray(values))
	if normalized is PackedStringArray and normalized != PackedStringArray(values):
		values = Array(normalized)
		row["fields"] = values
		_statements[index] = row
		_modified = true
	var items: Variant = logic.call("editor_layout", name, PackedStringArray(values))
	if not (items is Array):
		return fields_box
	for entry in items:
		if not (entry is Dictionary):
			continue
		var kind := str((entry as Dictionary).get("kind", "field"))
		var field_index := int((entry as Dictionary).get("index", -1))
		var label := _field_label_text(str((entry as Dictionary).get("label", "")))
		var label_before := bool((entry as Dictionary).get("label_before", true))
		var value := ""
		if field_index >= 0 and field_index < values.size():
			value = str(values[field_index])
		match kind:
			"text":
				if not label.is_empty():
					fields_box.add_child(_field_label(label))
			"field":
				if label_before and not label.is_empty():
					fields_box.add_child(_field_label(label))
				var edit := LineEdit.new()
				edit.text = value
				edit.custom_minimum_size = Vector2(
					240 if bool((entry as Dictionary).get("wide", false)) else 90, 32
				)
				edit.tooltip_text = "%s.%s" % [name, label]
				edit.text_submitted.connect(_set_field.bind(index, field_index))
				edit.focus_exited.connect(_commit_field.bind(index, field_index, edit))
				fields_box.add_child(edit)
				if not label_before and not label.is_empty():
					fields_box.add_child(_field_label(label))
			"select":
				if label_before and not label.is_empty():
					fields_box.add_child(_field_label(label))
				var button := MindWidgets.button(_select_text(name, field_index, value))
				# `Styles.logict`.
				button.theme_type_variation = "logict"
				button.custom_minimum_size = Vector2(120, 32)
				button.tooltip_text = "%s.%s" % [name, label]
				button.pressed.connect(_open_select.bind(index, field_index, name, button))
				fields_box.add_child(button)
			"color":
				var picker := ColorPickerButton.new()
				picker.custom_minimum_size = Vector2(40, 32)
				picker.color = _parse_color(value)
				picker.color_changed.connect(_set_color.bind(index, field_index))
				fields_box.add_child(picker)
			"align":
				var align_button := MindWidgets.image_button("pencil", "logict")
				align_button.custom_minimum_size = Vector2(40, 32)
				align_button.tooltip_text = _field_label_text("align")
				align_button.pressed.connect(_open_align_select.bind(index, field_index))
				fields_box.add_child(align_button)
			"char":
				var char_button := MindWidgets.image_button("pencil", "logict")
				char_button.custom_minimum_size = Vector2(40, 32)
				char_button.tooltip_text = "%s.%s" % [name, label]
				char_button.pressed.connect(_open_char_select.bind(index, field_index))
				fields_box.add_child(char_button)
			"jump":
				var jump_button := MindWidgets.button(value)
				jump_button.theme_type_variation = "logict"
				jump_button.custom_minimum_size = Vector2(36, 32)
				jump_button.tooltip_text = "%s.%s" % [name, label]
				# `JumpButton`: press-drag-release onto a target element.
				jump_button.gui_input.connect(_jump_button_input.bind(index, jump_button))
				_jump_buttons[index] = jump_button
				fields_box.add_child(jump_button)
	return fields_box


## `bundle(desc)`: `name.token.<token>` with the raw token as the fallback.
func _field_label_text(token: String) -> String:
	if token.is_empty():
		return ""
	var key := "name.token." + token
	var resolved := _t(key)
	return token if resolved == key else resolved


func _field_label(text: String) -> Label:
	# code-instantiated: one label per field-row item (count is layout-driven).
	var label := Label.new()
	label.text = text
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	return label


## The current option's display text for a select (`LStatement.bundle(Enum)`
## then the symbol/name fallback).
func _select_text(name: String, field_index: int, value: String) -> String:
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null:
		return value
	var options: Variant = logic.call("editor_options", name, field_index)
	if options is Array:
		for option in options:
			if option is Dictionary and str((option as Dictionary).get("name", "")) == value:
				return _option_text(option)
	return value


func _option_text(option: Variant) -> String:
	var text := str((option as Dictionary).get("label", (option as Dictionary).get("name", "")))
	var key := str((option as Dictionary).get("key", ""))
	if not key.is_empty():
		var resolved := _t(key)
		if resolved != key:
			text = resolved
	return text


## `showSelect`: a popup of `logicTogglet` buttons (`LStatement.java:233-253`).
func _open_select(index: int, field_index: int, name: String, button: Button) -> void:
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null:
		return
	var options: Variant = logic.call("editor_options", name, field_index)
	if not (options is Array):
		return
	var popup := PopupPanel.new()
	# code-instantiated: one transient enum-select popup per open.
	add_child(popup)
	var grid := GridContainer.new()
	grid.columns = 4
	popup.add_child(grid)
	var current := button.text
	var unit_actions := _unit_actions_enabled()
	for option in options:
		if not (option is Dictionary):
			continue
		var option_name := str((option as Dictionary).get("name", ""))
		# `UnitControlStatement` hides build/deconstruct when the rules disable them.
		if name == "ucontrol" and not unit_actions and option_name in ["build", "deconstruct"]:
			continue
		var toggle := Button.new()
		toggle.text = _option_text(option)
		toggle.toggle_mode = true
		toggle.theme_type_variation = "logicTogglet"
		toggle.button_pressed = option_name == _current_field_value(index, field_index)
		toggle.custom_minimum_size = Vector2(60, 38)
		toggle.tooltip_text = "%s.%s" % [name.to_lower(), option_name.to_lower()]
		toggle.pressed.connect(_select_enum.bind(index, field_index, option_name, popup))
		grid.add_child(toggle)
	popup.popup_centered()


## `fieldAlignSelect` (`LStatement.java:259-268`): the nine align presets,
## set as `@<name>` (`LStatement.afterRead` normalizes them).
func _open_align_select(index: int, field_index: int) -> void:
	var popup := PopupPanel.new()
	# code-instantiated: one transient align-select popup per open.
	add_child(popup)
	var grid := GridContainer.new()
	grid.columns = 3
	popup.add_child(grid)
	var current := _current_field_value(index, field_index)
	for align in [
		"topLeft", "top", "topRight", "left", "center", "right", "bottomLeft", "bottom", "bottomRight"
	]:
		var toggle := Button.new()
		toggle.text = _field_label_text(align)
		toggle.toggle_mode = true
		toggle.theme_type_variation = "logicTogglet"
		toggle.button_pressed = current == "@" + align
		toggle.custom_minimum_size = Vector2(100, 38)
		toggle.pressed.connect(_select_enum.bind(index, field_index, "@" + align, popup))
		grid.add_child(toggle)
	popup.popup_centered()


## `PrintCharStatement`'s pencil grid (`LStatements.java:290-311`).
func _open_char_select(index: int, field_index: int) -> void:
	var popup := PopupPanel.new()
	# code-instantiated: one transient character-picker popup per open.
	add_child(popup)
	var grid := GridContainer.new()
	grid.columns = 8
	popup.add_child(grid)
	for code in range(32, 127):
		var toggle := Button.new()
		toggle.text = String.chr(code)
		toggle.theme_type_variation = "flatt"
		toggle.custom_minimum_size = Vector2(32, 32)
		toggle.pressed.connect(_select_enum.bind(index, field_index, str(code), popup))
		grid.add_child(toggle)
	popup.popup_centered()


## The `JumpButton` target picker (`LCanvas.java:551-625`): pick a statement by
## index; the drag variant lands with T9's curves.
func _open_jump_select(index: int, field_index: int) -> void:
	var popup := PopupPanel.new()
	# code-instantiated: one transient jump-target popup per open.
	add_child(popup)
	var list := VBoxContainer.new()
	popup.add_child(list)
	for target in _statements.size():
		var target_row: Dictionary = _statements[target]
		var button := MindWidgets.button(
			"%d  %s" % [target, _t("lst." + str(target_row.get("bundle", "noop")))]
		)
		button.theme_type_variation = "logict"
		button.custom_minimum_size = Vector2(220, 32)
		button.pressed.connect(_select_enum.bind(index, field_index, str(target), popup))
		list.add_child(button)
	popup.popup_centered()


func _current_field_value(index: int, field_index: int) -> String:
	if index < 0 or index >= _statements.size():
		return ""
	var values: Array = (_statements[index] as Dictionary).get("fields", [])
	return str(values[field_index]) if field_index < values.size() else ""


## The `logicUnitBuild`/`logicUnitDeconstruct` rules probe (T1's add-dialog
## filter; `UnitControlStatement.build`).
func _unit_actions_enabled() -> bool:
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null:
		return true
	var parsed: Variant = JSON.parse_string(str(host.call("logic_rules_probe")))
	if not (parsed is Dictionary):
		return true
	var info := parsed as Dictionary
	return bool(info.get("logicUnitBuild", true)) or bool(info.get("logicUnitDeconstruct", true))


## A select changed: commit, apply the Java change fixups and rebuild the
## element (`DrawStatement.rebuild`, `rebuild(table)` in each `build`).
func _select_enum(index: int, field_index: int, option: String, popup: PopupPanel) -> void:
	popup.hide()
	if not _commit_field_value(option, index, field_index):
		return
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null or index < 0 or index >= _statements.size():
		return
	var row: Dictionary = _statements[index]
	var name := str(row.get("name", "noop"))
	var values: Array = row.get("fields", [])
	var changed: Variant = logic.call("apply_type_change", name, PackedStringArray(values))
	if changed is PackedStringArray:
		var list: Array = []
		for value in (changed as PackedStringArray):
			list.append(str(value))
		row["fields"] = list
		_statements[index] = row
	_modified = true
	_rebuild_list()


## `col(table, x, res -> ...)`: the colour picker commits `%rrggbb[aa]`.
func _set_color(color: Color, index: int, field_index: int) -> void:
	var alpha := color.a >= 1.0
	var packed := "%02x%02x%02x" % [
		int(color.r * 255.0), int(color.g * 255.0), int(color.b * 255.0)
	]
	if not alpha:
		packed += "%02x" % int(color.a * 255.0)
	_commit_field_value("%" + packed, index, field_index)


func _parse_color(value: String) -> Color:
	var text := value.trim_prefix("%")
	if text.length() < 6:
		return Color.WHITE
	var r := ("0x" + text.substr(0, 2)).hex_to_int()
	var g := ("0x" + text.substr(2, 2)).hex_to_int()
	var b := ("0x" + text.substr(4, 2)).hex_to_int()
	var a := 255
	if text.length() >= 8:
		a = ("0x" + text.substr(6, 2)).hex_to_int()
	return Color8(r, g, b, a)


## `field(table, value, setter)`: `setter.get(sanitize(text))`.
func _set_field(text: String, index: int, field_index: int) -> void:
	_commit_field_value(text, index, field_index)


## Commit on focus loss (`TextField.changed` in Java runs per keystroke; the
## port commits on submit/blur so typing stays cheap).
func _commit_field(index: int, field_index: int, edit: LineEdit) -> void:
	_commit_field_value(edit.text, index, field_index)


func _commit_field_value(text: String, index: int, field_index: int) -> bool:
	if index < 0 or index >= _statements.size():
		return false
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null:
		return false
	var sanitized := str(logic.call("sanitize", text))
	var row: Dictionary = _statements[index]
	var values: Array = row.get("fields", [])
	if field_index >= values.size():
		return false
	if str(values[field_index]) == sanitized:
		return false
	values[field_index] = sanitized
	row["fields"] = values
	_statements[index] = row
	_modified = true
	return true


## `StatementElem.copy` (`LCanvas.java:516-538`): the duplicate lands after the
## element; a copied `jump`'s target bumps when it pointed below (`:520-527`).
func _copy_statement(index: int) -> void:
	if index < 0 or index >= _statements.size():
		return
	var original: Dictionary = _statements[index]
	var copied: Dictionary = original.duplicate(true)
	_statements.insert(index + 1, copied)
	if str(original.get("name", "")) == "jump":
		var dest := _jump_target(original)
		if dest > index:
			(jump_fields(original))[0] = str(dest + 1)
			original["fields"] = jump_fields(original)
			(copied.get("fields", []) as Array)[0] = str(dest + 1)
	_modified = true
	_rebuild_list()


## `StatementElem`'s delete button: dropped targets become `-1` and later
## indices shift down (Java's `saveUI` recomputes `indexOf(dest)`).
func _delete_statement(index: int) -> void:
	if index < 0 or index >= _statements.size():
		return
	_statements.remove_at(index)
	for row in _statements:
		if not (row is Dictionary) or str((row as Dictionary).get("name", "")) != "jump":
			continue
		var fields := jump_fields(row)
		if fields.is_empty():
			continue
		var dest := int(fields[0])
		if dest == index:
			fields[0] = "-1"
		elif dest > index:
			fields[0] = str(dest - 1)
		(row as Dictionary)["fields"] = fields
	_modified = true
	_rebuild_list()


## The `jump` row's `dest_index` (`-1` when unset/out of range).
func _jump_target(row: Dictionary) -> int:
	if str(row.get("name", "")) != "jump":
		return -1
	var fields: Array = row.get("fields", [])
	if fields.is_empty():
		return -1
	var dest := int(fields[0])
	return dest if dest >= 0 and dest < _statements.size() else -1


func jump_fields(row: Variant) -> Array:
	if row is Dictionary:
		var fields: Variant = (row as Dictionary).get("fields", [])
		if fields is Array and not (fields as Array).is_empty():
			return fields
	return [] as Array


## `LCanvas.DragLayout.finishLayout`: drop the dragged element at
## `insertPosition` and remap every `jump` target like Java's element refs do
## (`JumpStatement.saveUI` recomputes `indexOf(dest)`).
func _finish_drag() -> void:
	var from := _drag_index
	if from < 0 or from >= _statements.size():
		return
	var drop := clampi(_drop_index, 0, _statements.size())
	var to := drop if drop <= from else drop - 1
	if to != from:
		for row in _statements:
			if not (row is Dictionary) or str((row as Dictionary).get("name", "")) != "jump":
				continue
			var fields := jump_fields(row)
			if fields.is_empty():
				continue
			var dest := int(fields[0])
			if dest == from:
				dest = to
			elif to >= from and from < dest and dest <= to:
				dest -= 1
			elif to < from and to <= dest and dest < from:
				dest += 1
			fields[0] = str(dest)
			(row as Dictionary)["fields"] = fields
		var moved: Variant = _statements.pop_at(from)
		_statements.insert(to, moved)
		_modified = true
	_drop_index = -1
	_rebuild_list()


## The insertion index for a global pointer position (`DragLayout.layout`'s
## `insertPosition`).
func _drop_index_at(global_pos: Vector2) -> int:
	for i in _row_nodes.size():
		var rect := (_row_nodes[i] as Control).get_global_rect()
		if global_pos.y < rect.position.y + rect.size.y * 0.5:
			return i
	return _row_nodes.size()


## The statement element under a global pointer position (`canvas.hovered`).
func _row_at(global_pos: Vector2) -> int:
	for i in _row_nodes.size():
		if (_row_nodes[i] as Control).get_global_rect().has_point(global_pos):
			return i
	return -1


## `JumpButton`'s touch handlers (`LCanvas.java:571-605`): press starts the
## selection, dragging highlights the hovered element, release sets (or clears)
## the target. A press-release without movement opens the target picker.
func _jump_button_input(event: InputEvent, index: int, button: Control) -> void:
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT:
		if event.pressed:
			_jump_selecting = index
			_jump_hover = -1
			_jump_moved = false
			button.accept_event()
		else:
			if _jump_moved:
				if _jump_hover >= 0 and _jump_hover != index:
					_set_jump_target(index, _jump_hover)
				_rebuild_list()
			else:
				_open_jump_select(index, 0)
			_jump_selecting = -1
			_jump_hover = -1
			if _overlay != null:
				_overlay.queue_redraw()
			button.accept_event()
		return
	if event is InputEventMouseMotion and _jump_selecting == index:
		_jump_moved = true
		_jump_hover = _row_at(button.get_global_transform() * event.position)
		if _overlay != null:
			_overlay.queue_redraw()
		button.accept_event()


## `JumpButton`'s setter: writes the `dest_index` field and rebuilds.
func _set_jump_target(index: int, target: int) -> void:
	if index < 0 or index >= _statements.size():
		return
	var row: Dictionary = _statements[index]
	var fields: Array = row.get("fields", [])
	if fields.is_empty():
		return
	fields[0] = str(target)
	row["fields"] = fields
	_statements[index] = row
	_modified = true


## `LCanvas.DragLayout.draw` + `JumpCurve.draw`: the drop line while dragging
## and one trapezoid per `jump` from its button to the target's left edge
## (`drawCurve`'s four-point trapezoid, `Pal.place`).
func _draw_overlay() -> void:
	if _overlay == null:
		return
	var place := Color8(0x63, 0x35, 0xf8)
	if _drag_active and _drop_index >= 0 and not _row_nodes.is_empty():
		var first := (_row_nodes[0] as Control).get_global_rect()
		var y := 0.0
		if _drop_index <= 0:
			y = first.position.y - 3.0
		elif _drop_index >= _row_nodes.size():
			y = (_row_nodes[_row_nodes.size() - 1] as Control).get_global_rect().end.y + 3.0
		else:
			var above := (_row_nodes[_drop_index - 1] as Control).get_global_rect()
			var below := (_row_nodes[_drop_index] as Control).get_global_rect()
			y = (above.end.y + below.position.y) * 0.5
		var left := first.position.x
		var right := left + first.size.x
		_overlay.draw_line(
			_to_overlay(Vector2(left, y)),
			_to_overlay(Vector2(right, y)),
			Color(1, 1, 1, 0.7),
			3.0
		)
	for info in _jump_spans():
		var button: Control = _jump_buttons.get(info["jump"])
		if button == null:
			continue
		var src := _to_overlay(button.get_global_rect().get_center())
		var target_rect := (_row_nodes[int(info["target"])] as Control).get_global_rect()
		var dst := _to_overlay(
			Vector2(target_rect.position.x - 2.0, target_rect.position.y + target_rect.size.y * 0.5)
		)
		var h := 24.0 + 14.0 * float(info["lane"])
		var dy := signf(dst.y - src.y) * h * 0.5
		var points := PackedVector2Array(
			[src, Vector2(src.x + h, src.y + dy), Vector2(src.x + h, dst.y - dy), dst]
		)
		_overlay.draw_polyline(points, place, 4.0)
		_overlay.draw_rect(Rect2(dst - Vector2(4, 4), Vector2(8, 8)), place)
	if _jump_selecting >= 0 and _jump_hover >= 0 and _jump_hover < _row_nodes.size():
		var hovered := (_row_nodes[_jump_hover] as Control).get_global_rect()
		_overlay.draw_rect(
			Rect2(_to_overlay(hovered.position), hovered.size), Color(1, 1, 1, 0.25)
		)


func _to_overlay(global_pos: Vector2) -> Vector2:
	return _overlay.get_global_transform().affine_inverse() * global_pos


## The valid jump spans with greedy lane assignment so curves do not overlap
## (`LCanvas.DragLayout.setJumpHeights`).
func _jump_spans() -> Array:
	var spans: Array = []
	for i in _statements.size():
		if not (_statements[i] is Dictionary):
			continue
		var dest := _jump_target(_statements[i])
		if dest < 0 or dest == i or not _jump_buttons.has(i):
			continue
		if i >= _row_nodes.size() or dest >= _row_nodes.size():
			continue
		spans.append({"jump": i, "target": dest, "begin": mini(i, dest), "end": maxi(i, dest)})
	spans.sort_custom(func(a: Dictionary, b: Dictionary) -> bool: return int(a["begin"]) < int(b["begin"]))
	var lane_ends: Array = []
	for span in spans:
		var lane := -1
		for li in lane_ends.size():
			if int(lane_ends[li]) < int(span["begin"]):
				lane = li
				break
		if lane < 0:
			lane = lane_ends.size()
			lane_ends.append(-1)
		lane_ends[lane] = span["end"]
		span["lane"] = lane
	return spans


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


## `LogicDialog`'s `@variables` button (`:169-247`): the variables panel when
## the processor has non-constant vars, else the globals panel
## (`shouldShowVariables`); the button label follows.
func _open_variables_panel() -> void:
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null:
		return
	var state: Variant = host.call("logic_get_state", _x, _y)
	var vars: Array = []
	if state is Dictionary:
		var list: Variant = (state as Dictionary).get("vars", [])
		if list is Array:
			vars = list
	if vars.is_empty():
		_open_globals_panel()
	else:
		_open_vars_panel(vars)


## The per-variable rows (`LogicDialog.java:183-247`): name (accent), the value
## (140 wide, refreshed every 15 frames) and the type.
func _open_vars_panel(vars: Array) -> void:
	var popup := PopupPanel.new()
	# code-instantiated: the variables panel is transient and data-driven.
	add_child(popup)
	var column := VBoxContainer.new()
	column.custom_minimum_size = Vector2(420, 320)
	popup.add_child(column)
	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	column.add_child(scroll)
	var list := VBoxContainer.new()
	list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(list)
	_vars_rows.clear()
	for entry in vars:
		if not (entry is Dictionary):
			continue
		var row := HBoxContainer.new()
		var name_label := Label.new()
		name_label.text = " " + str((entry as Dictionary).get("name", "")) + " "
		name_label.add_theme_color_override("font_color", MindStyles.ACCENT)
		row.add_child(name_label)
		var value_label := Label.new()
		value_label.text = _format_var_value(entry)
		value_label.custom_minimum_size = Vector2(140, 0)
		value_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		row.add_child(value_label)
		var type_label := Label.new()
		type_label.text = " " + str((entry as Dictionary).get("type", "")) + " "
		type_label.custom_minimum_size = Vector2(120, 0)
		row.add_child(type_label)
		list.add_child(row)
		_vars_rows.append([str((entry as Dictionary).get("name", "")), value_label])
	var buttons := HBoxContainer.new()
	column.add_child(buttons)
	var globals := MindWidgets.icon_button("list", _t("@logic.globals"), "cleart")
	globals.pressed.connect(
		func() -> void:
			popup.hide()
			_open_globals_panel()
	)
	buttons.add_child(globals)
	_vars_popup = popup
	_vars_timer = 0
	popup.popup_centered()
	set_process(true)


## `LogicDialog`'s value text: objects print through the sim, numbers as
## integers within 0.00001 (`LogicDialog.java:206-215`).
func _format_var_value(entry: Variant) -> String:
	return str((entry as Dictionary).get("value", ""))


## The globals panel (`GlobalVarsDialog`): `@name` + its `lglobal.<name>`
## description.
func _open_globals_panel() -> void:
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null:
		return
	var popup := PopupPanel.new()
	# code-instantiated: the globals panel is transient and data-driven.
	add_child(popup)
	var column := VBoxContainer.new()
	column.custom_minimum_size = Vector2(520, 380)
	popup.add_child(column)
	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	column.add_child(scroll)
	var list := VBoxContainer.new()
	list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(list)
	var rows: Variant = logic.call("globals")
	if rows is Array:
		for entry in rows:
			if not (entry is Dictionary):
				continue
			var row := HBoxContainer.new()
			var name_label := Label.new()
			name_label.text = " " + str((entry as Dictionary).get("name", "")) + " "
			name_label.custom_minimum_size = Vector2(150, 0)
			row.add_child(name_label)
			var desc_label := Label.new()
			desc_label.text = _t(str((entry as Dictionary).get("description", "")))
			desc_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
			desc_label.custom_minimum_size = Vector2(320, 0)
			row.add_child(desc_label)
			list.add_child(row)
	popup.popup_centered()


## The 15-frame variables refresh while the panel is shown
## (`LogicDialog.java:204-222`).
func _process(_delta: float) -> void:
	# `DragLayout`/`JumpCurve` repaint with the scroll position every frame.
	if _overlay != null and is_visible_in_tree():
		_overlay.queue_redraw()
	if _vars_popup == null or not _vars_popup.visible:
		return
	_vars_timer += 1
	if _vars_timer < 15:
		return
	_vars_timer = 0
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null:
		return
	var state: Variant = host.call("logic_get_state", _x, _y)
	if not (state is Dictionary):
		return
	var vars: Variant = (state as Dictionary).get("vars", [])
	if not (vars is Array):
		return
	for entry in vars:
		if not (entry is Dictionary):
			continue
		var name := str((entry as Dictionary).get("name", ""))
		for row in _vars_rows:
			if row[0] == name:
				(row[1] as Label).text = _format_var_value(entry)


## `LogicDialog.showAddDialog` (`LogicDialog.java:251-341`): a searchable list of
## statements grouped by category; clicking one appends its default instance.
func _open_add_dialog() -> void:
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null:
		return
	var popup := PopupPanel.new()
	# code-instantiated: the add picker is a transient grouped list over the
	# dialog (one per open).
	add_child(popup)
	var column := VBoxContainer.new()
	column.custom_minimum_size = Vector2(560, 420)
	popup.add_child(column)
	var search := LineEdit.new()
	search.placeholder_text = _t("@players.search")
	column.add_child(search)
	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.custom_minimum_size = Vector2(0, 340)
	column.add_child(scroll)
	var list := VBoxContainer.new()
	list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(list)
	var entries := _add_entries(logic)
	var rebuild := func() -> void:
		for child in list.get_children():
			list.remove_child(child)
			child.queue_free()
		var text := search.text.to_lower()
		var current_category := ""
		var grid: GridContainer = null
		var shown := 0
		for entry in entries:
			var label: String = entry.get("label", "")
			var name: String = entry.get("name", "")
			if not text.is_empty() and not label.to_lower().contains(text) and not name.to_lower().contains(text):
				continue
			var category: String = entry.get("category", "unknown")
			if category != current_category:
				current_category = category
				var header := Label.new()
				header.text = str(entry.get("category_label", category))
				header.add_theme_color_override("font_color", CATEGORY_COLORS.get(category, CATEGORY_COLORS["unknown"]))
				list.add_child(header)
				grid = GridContainer.new()
				grid.columns = 3
				list.add_child(grid)
			if grid == null:
				continue
			var button := MindWidgets.button(label)
			button.theme_type_variation = "flatt"
			button.custom_minimum_size = Vector2(130, 50)
			button.add_theme_color_override("font_color", CATEGORY_COLORS.get(category, CATEGORY_COLORS["unknown"]))
			button.tooltip_text = _t("lst." + str(entry.get("bundle", name)))
			button.pressed.connect(_add_statement.bind(name, popup))
			(grid as GridContainer).add_child(button)
			shown += 1
		if shown == 0:
			var empty := Label.new()
			empty.text = _t("@none")
			list.add_child(empty)
	search.text_changed.connect(func(_text: String) -> void: rebuild.call())
	search.text_submitted.connect(
		func(text: String) -> void:
			var lowered := text.to_lower()
			for entry in entries:
				if str(entry.get("label", "")).to_lower().contains(lowered):
					_add_statement(str(entry.get("name", "")), popup)
					return
	)
	rebuild.call()
	popup.popup_centered()
	search.grab_focus()


## Candidate statements for the add dialog (the Java filters:
## `LogicDialog.java:296-305`).
func _add_entries(logic: Node) -> Array:
	var out: Array = []
	var privileged := _privileged
	var unit_allowed := true
	var host := get_node_or_null("/root/Spine/SimHost")
	if host != null:
		var probe := str(host.call("logic_rules_probe"))
		var parsed: Variant = JSON.parse_string(probe)
		if parsed is Dictionary:
			unit_allowed = bool((parsed as Dictionary).get("logicUnitControl", true))
	for name in logic.call("statement_names"):
		var meta: Variant = logic.call("statement_meta", str(name))
		if not (meta is Dictionary):
			continue
		var info := meta as Dictionary
		if bool(info.get("hidden", false)):
			continue
		if bool(info.get("privileged", false)) and not privileged:
			continue
		var category := str(info.get("category", "unknown"))
		if category == "unit" and not privileged and not unit_allowed:
			continue
		var bundle := str(info.get("bundle", name))
		var label := _t("lst." + bundle)
		out.append({
			"name": str(name),
			"category": category,
			"category_label": _t("lcategory." + category),
			"bundle": bundle,
			"label": label,
		})
	return out


## Adds a default instance of `name` at the end of the list
## (`canvas.addAt(canvas.statements.getChildren().size, prov.get())`).
func _add_statement(name: String, popup: PopupPanel) -> void:
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null:
		return
	var row: Variant = logic.call("default_statement_row", name)
	if not (row is Dictionary) or (row as Dictionary).is_empty():
		return
	_statements.append(row)
	_modified = true
	_rebuild_list()
	popup.hide()


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
