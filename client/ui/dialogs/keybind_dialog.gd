## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/KeybindDialog.java (plan 15 §6.1).
##
## Rebind Keys dialog: a search field, one section per binding category with a
## localized name and key column per action, a Rebind capture and a per-row
## Reset (disabled while the binding is at its default), matching upstream's
## `rebindBinds`. The battle mode + capture prompt mirror `openDialog`/`rebind`.
## The binding table, values and persistence come from `MindInput`.

extends MindDialog

## Upstream skips the per-row Reset for the menu binding.
const NO_RESET := ["menu"]
## Arc `KeyCode.getName()` handles unset as "Unset" (rendered dark gray).
const UNSET_TEXT := "Unset"
const UNSET_COLOR := Color(0.35, 0.35, 0.35)
const CATEGORY_COLOR := Color(0.5, 0.5, 0.5)
const BUTTON_SIZE := Vector2(140.0, 40.0)

var _search := ""
var _entries: Array = []
var _list: MindTable = null
var _search_field: LineEdit = null
var _capture: Control = null
var _capture_name := ""


func _ready() -> void:
	set_title_key("@keybind.title")
	should_pause = false
	# Upstream `KeybindDialog.setFillParent(true)` + `top()`.
	full_dialog = true
	super._ready()
	_build()
	add_close_button()


func _build() -> void:
	# Search row (upstream `cont.table` with Icon.zoom + field).
	var root := content_table()
	root.add_theme_constant_override("separation", 4)
	# code-instantiated: the search icon+field row is a per-dialog control with
	# no static scene.
	var search_row := HBoxContainer.new()
	search_row.add_theme_constant_override("separation", 6)
	search_row.add_child(MindWidgets.glyph("zoom", 22))
	_search_field = MindWidgets.field()
	_search_field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_search_field.text_changed.connect(_on_search_changed)
	search_row.add_child(_search_field)
	root.add(search_row).grow_x_axis()
	root.row()

	# code-instantiated: the binding list is data-driven from `MindInput` and
	# rebuilt on every search/reassign; the row count is not known at build time.
	_list = MindTable.new()
	_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	cont.add_child(_list)
	_rebuild()


func _mind_input() -> Node:
	return get_node_or_null("/root/Spine/Input")


func _on_search_changed(text: String) -> void:
	_search = text
	_rebuild()


func _load_entries() -> Array:
	var input := _mind_input()
	if input != null and input.has_method("keybinds_json"):
		var parsed: Variant = JSON.parse_string(str(input.call("keybinds_json")))
		if parsed is Array:
			return parsed
	return []


func _rebuild() -> void:
	if _list == null:
		return
	_entries = _load_entries()
	_list.clear_children()
	var last_category: Variant = null
	for entry in _entries:
		var localized := _t("@" + str(entry.get("bundle", "")))
		if not _search.is_empty() and not localized.to_lower().contains(_search.to_lower()):
			continue
		var category: Variant = entry.get("category")
		if category != null and category != last_category:
			_add_category_header(str(category))
			last_category = category
		_add_binding_row(entry, localized)
	_add_reset_all()
	_list.sort_now()
	call_deferred("_resort_tables")


func _add_category_header(category: String) -> void:
	var header := MindWidgets.label(_t("@category.%s.name" % category))
	header.add_theme_color_override("font_color", CATEGORY_COLOR)
	_list.add(header).grow_x_axis().set_colspan(4).pad(10).set_pad_bottom(4)
	_list.row()
	# code-instantiated: the section rule is a styled ColorRect (no static scene).
	var rule := ColorRect.new()
	rule.color = CATEGORY_COLOR
	rule.custom_minimum_size = Vector2(0.0, 3.0)
	rule.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_list.add(rule).grow_x_axis().set_colspan(4).pad(6).set_pad_top(0).set_pad_bottom(10)
	_list.row()


func _add_binding_row(entry: Dictionary, localized: String) -> void:
	var name := str(entry.get("name", ""))
	# code-instantiated: keybind rows are data-driven from the registry.
	var label := MindWidgets.label(localized)
	_list.add(label).grow_x_axis().set_align(3).set_pad_left(8.0).set_pad_right(40.0)

	var key := MindWidgets.label(_key_text(entry))
	key.add_theme_color_override("font_color", UNSET_COLOR if _is_unset(entry) else MindStyles.ACCENT)
	_list.add(key).fill_x_axis().set_min_width(90.0).set_align(3).set_pad_right(20.0)

	var rebind := MindWidgets.button(_t("@settings.rebind"))
	rebind.theme_type_variation = "grayt"
	rebind.custom_minimum_size = BUTTON_SIZE
	rebind.pressed.connect(_start_capture.bind(name))
	_list.add(rebind).set_min_width(BUTTON_SIZE.x).set_min_height(BUTTON_SIZE.y)

	if not NO_RESET.has(name):
		var reset := MindWidgets.button(_t("@settings.resetKey"))
		reset.theme_type_variation = "grayt"
		reset.custom_minimum_size = BUTTON_SIZE
		reset.disabled = bool(entry.get("default", false))
		reset.pressed.connect(_reset_binding.bind(name))
		_list.add(reset).set_min_width(BUTTON_SIZE.x).set_min_height(BUTTON_SIZE.y).pad(2.0).set_pad_left(4.0)
	_list.row()


func _add_reset_all() -> void:
	var reset := MindWidgets.button(_t("@settings.reset"))
	reset.theme_type_variation = "grayt"
	reset.custom_minimum_size.x = 200.0
	reset.pressed.connect(_reset_all)
	_list.add(reset).set_colspan(4).set_min_width(200.0).set_min_height(50.0).pad(4)
	_list.row()


## Upstream key cell: single key, or `min [red]/[] max` for a two-key axis.
func _key_text(entry: Dictionary) -> String:
	var negative: Variant = entry.get("negativeDisplay")
	var positive: Variant = entry.get("display")
	if bool(entry.get("axis", false)) and negative != null:
		return "%s / %s" % [str(negative), str(positive) if positive != null else UNSET_TEXT]
	return str(positive) if positive != null else UNSET_TEXT


func _is_unset(entry: Dictionary) -> bool:
	return entry.get("display") == null and entry.get("negativeDisplay") == null


func _reset_binding(name: String) -> void:
	var input := _mind_input()
	if input != null:
		input.call("reset_keybind", name)
	_rebuild()


func _reset_all() -> void:
	var input := _mind_input()
	if input != null:
		input.call("reset_keybinds")
	_rebuild()


## Opens the capture prompt (`KeybindDialog.openDialog`): the next key or mouse
## button rebinds; Back cancels and Unbind clears the binding.
func _start_capture(name: String) -> void:
	_capture_name = name
	var focus := get_viewport().gui_get_focus_owner()
	if focus != null:
		focus.release_focus()
	# code-instantiated: the capture prompt is a transient per-rebind overlay.
	_capture = Control.new()
	_capture.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_capture.mouse_filter = Control.MOUSE_FILTER_STOP
	_capture.gui_input.connect(_on_capture_gui_input)
	add_child(_capture)

	var center := CenterContainer.new()
	center.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	center.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_capture.add_child(center)

	var panel := PanelContainer.new()
	panel.theme_type_variation = "defaultDialog"
	center.add_child(panel)

	var box := VBoxContainer.new()
	box.add_theme_constant_override("separation", 12)
	panel.add_child(box)
	var prompt := MindWidgets.styled_label(_t("@keybind.press"), "techLabel")
	prompt.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	box.add_child(prompt)
	var row := HBoxContainer.new()
	row.alignment = BoxContainer.ALIGNMENT_CENTER
	box.add_child(row)
	var back := MindWidgets.button(_t("@back"))
	back.custom_minimum_size = Vector2(160.0, 50.0)
	back.pressed.connect(_end_capture)
	row.add_child(back)
	var unbind := MindWidgets.button(_t("@settings.unbindKey"))
	unbind.custom_minimum_size = Vector2(160.0, 50.0)
	unbind.pressed.connect(_unbind_capture)
	row.add_child(unbind)


func _on_capture_gui_input(event: InputEvent) -> void:
	if not (event is InputEventMouseButton) or not event.pressed:
		return
	var code := ""
	match event.button_index:
		MOUSE_BUTTON_LEFT:
			code = "mouseLeft"
		MOUSE_BUTTON_RIGHT:
			code = "mouseRight"
		MOUSE_BUTTON_MIDDLE:
			code = "mouseMiddle"
	if not code.is_empty():
		_apply_capture(code, false)


func _unhandled_input(event: InputEvent) -> void:
	if _capture == null:
		return
	if event is InputEventKey and event.pressed and not event.echo:
		_apply_capture(OS.get_keycode_string(event.keycode), true)


func _apply_capture(code: String, from_godot: bool) -> void:
	var input := _mind_input()
	if input != null:
		if from_godot:
			input.call("rebind_key", _capture_name, code)
		else:
			input.call("rebind", _capture_name, code)
	_end_capture()
	_rebuild()


func _unbind_capture() -> void:
	var input := _mind_input()
	if input != null:
		input.call("unbind_keybind", _capture_name)
	_end_capture()
	_rebuild()


func _end_capture() -> void:
	if _capture != null:
		_capture.queue_free()
		_capture = null
