## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/MapObjectivesDialog.java
##
## The objectives editor: the canvas (node graph) plus the field editor panel
## for the selected objective. The field editors follow the Java reflection
## interpreters for the scalar kinds (`MapObjectivesDialog.java:44-210`):
## string (multiline -> area), boolean, byte/int, float (`@Second` x60,
## `@TilePos` x8), content pickers, team pick, `Vec2` pairs. The class/field
## table comes from `MindEditor.objective_classes_json`; edits write through
## `set_objectives_json`.

extends MindDialog

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"
@export var canvas_scene: PackedScene

## Panel width for the selected objective's fields (desktop side panel).
const PANEL_WIDTH := 380.0
## Field kinds with a scalar editor; the rest are hidden in this slice.
const SCALAR_KINDS := ["string", "bool", "byte", "int", "float"]

var _editor: Node = null
var _canvas: Control = null
var _panel: VBoxContainer = null
## `[{tag, fields: [{name, kind, flags}]}]`.
var _classes: Array = []
var _objectives: Array = []
var _selected := -1
## Pending picker target (`class` / `class:<i>` / `content:<i>:<field>` / `team:<i>:<field>`).
var _picker_target := ""
## The objective being edited in the class picker (`-1` = append).
var _pending_class_index := -1
## Set while the `@clear` confirm is pending (`@settings.clear.confirm`).
var _pending_clear := false


func _ready() -> void:
	set_title_key("@editor.objectives")
	should_pause = false
	super._ready()
	_editor = get_node_or_null(editor_path)
	clear_buttons()
	# `buttons.button("@back", Icon.left, ...)`, `@add`, `@edit.menu` (`:494-496`).
	add_close_button()
	add_button(_t("@add"), _add_objective, "add", 160.0)
	add_button(_t("@edit.menu"), _edit_menu, "edit", 160.0)
	_connect_confirm()


func shown() -> void:
	_classes = _read_classes()
	_objectives = _read_objectives()
	_selected = -1
	_build()


func _build() -> void:
	if cont == null:
		return
	for child in cont.get_children():
		cont.remove_child(child)
		child.queue_free()
	var root := content_table()
	var row := HBoxContainer.new()
	row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.size_flags_vertical = Control.SIZE_EXPAND_FILL
	row.add_theme_constant_override("separation", 10)
	root.add(row).grow_x_axis().grow_y_axis().pad(4)

	# `canvas = new MapObjectivesCanvas()` (the Java dialog is canvas-first).
	if canvas_scene != null:
		# code-instantiated: the canvas scene is shared by this dialog only.
		_canvas = canvas_scene.instantiate()
		_canvas.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		_canvas.size_flags_vertical = Control.SIZE_EXPAND_FILL
		_canvas.custom_minimum_size = Vector2(480.0, 320.0)
		if _canvas.has_signal("node_selected"):
			_canvas.node_selected.connect(_select_objective)
		row.add_child(_canvas)

	# code-instantiated: the field panel is driven by the class descriptors.
	var scroll := ScrollContainer.new()
	scroll.custom_minimum_size.x = PANEL_WIDTH
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	row.add_child(scroll)
	_panel = VBoxContainer.new()
	_panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(_panel)
	_rebuild_panel()


## `@add`: the objective type picker (`getProvider(MapObjective.class)`).
func _add_objective() -> void:
	_pending_class_index = -1
	_open_class_picker()


## `@edit.menu`: copy/load/clear (`:496-522`).
func _edit_menu() -> void:
	# code-instantiated: per-action popup panels have no static scene.
	var overlay := ColorRect.new()
	overlay.name = "EditMenu"
	overlay.color = Color(0, 0, 0, 0.8)
	overlay.set_anchors_preset(Control.PRESET_FULL_RECT)
	overlay.mouse_filter = Control.MOUSE_FILTER_STOP
	add_child(overlay)
	var center := CenterContainer.new()
	center.set_anchors_preset(Control.PRESET_FULL_RECT)
	center.mouse_filter = Control.MOUSE_FILTER_IGNORE
	overlay.add_child(center)
	var panel := PanelContainer.new()
	panel.theme_type_variation = "defaultDialog"
	center.add_child(panel)
	var box := VBoxContainer.new()
	panel.add_child(box)
	var copy := MindWidgets.icon_button("copy", _t("@copy.clipboard"))
	copy.disabled = _objectives.is_empty()
	copy.pressed.connect(func() -> void:
		DisplayServer.clipboard_set(JSON.stringify(_objectives))
		_notify("@copied")
		overlay.queue_free())
	box.add_child(copy)
	var load_button := MindWidgets.icon_button("download", _t("@load.clipboard"))
	load_button.pressed.connect(func() -> void:
		var parsed: Variant = JSON.parse_string(DisplayServer.clipboard_get())
		if not (parsed is Array):
			_notify("@waves.invalid")
		else:
			_objectives = parsed
			_commit()
			_selected = -1
			_build()
		overlay.queue_free())
	box.add_child(load_button)
	var clear := MindWidgets.icon_button("cancel", _t("@clear"))
	clear.pressed.connect(func() -> void:
		_pending_clear = true
		overlay.queue_free()
		var ui := get_node_or_null("/root/MindUi")
		if ui != null and ui.has_method("show_confirm"):
			ui.call("show_confirm", "%s\n%s" % [_t("@confirm"), _t("@settings.clear.confirm")])
		)
	box.add_child(clear)
	var back := MindWidgets.button(_t("@back"))
	back.pressed.connect(overlay.queue_free)
	box.add_child(back)


## The class picker (`:214-240`): all `MapObjectives.allObjectiveTypes`.
func _open_class_picker() -> void:
	if _classes.is_empty():
		return
	# code-instantiated: per-action popup panels have no static scene.
	var overlay := ColorRect.new()
	overlay.name = "ClassPicker"
	overlay.color = Color(0, 0, 0, 0.8)
	overlay.set_anchors_preset(Control.PRESET_FULL_RECT)
	overlay.mouse_filter = Control.MOUSE_FILTER_STOP
	add_child(overlay)
	var center := CenterContainer.new()
	center.set_anchors_preset(Control.PRESET_FULL_RECT)
	center.mouse_filter = Control.MOUSE_FILTER_IGNORE
	overlay.add_child(center)
	var panel := PanelContainer.new()
	panel.theme_type_variation = "defaultDialog"
	center.add_child(panel)
	var box := VBoxContainer.new()
	panel.add_child(box)
	box.add_child(MindWidgets.label(_t("@add")))
	var grid := GridContainer.new()
	grid.columns = 3
	grid.add_theme_constant_override("h_separation", 4)
	grid.add_theme_constant_override("v_separation", 4)
	box.add_child(grid)
	for value in _classes:
		var entry: Dictionary = value
		var tag := str(entry.get("tag", ""))
		var button := MindWidgets.button(_objective_type_name(tag))
		button.theme_type_variation = "flatt"
		button.alignment = HORIZONTAL_ALIGNMENT_LEFT
		button.custom_minimum_size = Vector2(195.0, 56.0)
		button.pressed.connect(func() -> void: _pick_class(tag, overlay))
		grid.add_child(button)
	var back := MindWidgets.button(_t("@back"))
	back.pressed.connect(overlay.queue_free)
	box.add_child(back)


## `cons.get(obj)` then hide: append or replace the objective.
func _pick_class(tag: String, overlay: Control) -> void:
	overlay.queue_free()
	var created: Variant = JSON.parse_string(str(_editor.call("objective_default_json", tag)))
	if not (created is Dictionary):
		return
	if _pending_class_index >= 0 and _pending_class_index < _objectives.size():
		# Keep the canvas position when changing class.
		var old: Dictionary = _objectives[_pending_class_index] \
			if _objectives[_pending_class_index] is Dictionary else {}
		if old.has("editorPos"):
			created["editorPos"] = old["editorPos"]
		_objectives[_pending_class_index] = created
		_selected = _pending_class_index
	else:
		_objectives.append(created)
		_selected = _objectives.size() - 1
	_pending_class_index = -1
	_commit()
	_build()


func _select_objective(index: int) -> void:
	if index < 0 or index >= _objectives.size():
		return
	_selected = index
	_rebuild_panel()


## The selected objective's editor (type, position actions, field rows).
func _rebuild_panel() -> void:
	if _panel == null:
		return
	for child in _panel.get_children():
		_panel.remove_child(child)
		child.queue_free()
	if _selected < 0 or _selected >= _objectives.size():
		_panel.add_child(MindWidgets.label(_t("@editor.objectives")))
		return
	var objective: Dictionary = _objectives[_selected] if _objectives[_selected] is Dictionary else {}
	var tag := str(objective.get("class", ""))
	var header := HBoxContainer.new()
	header.add_theme_constant_override("separation", 4)
	var type_button := MindWidgets.button(_objective_type_name(tag))
	type_button.theme_type_variation = "flatt"
	type_button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	type_button.pressed.connect(func() -> void:
		_pending_class_index = _selected
		_open_class_picker())
	header.add_child(type_button)
	header.add_child(_small_button("up", func() -> void: _move(-1)))
	header.add_child(_small_button("down", func() -> void: _move(1)))
	header.add_child(_small_button("cancel", func() -> void: _remove_selected()))
	_panel.add_child(header)
	for value in _class_entry(tag).get("fields", []):
		var field: Dictionary = value
		if not _is_visible_field(field):
			continue
		var row := _field_row(objective, tag, field)
		if row != null:
			_panel.add_child(row)


## `name(cont, name, remover, indexer)` + the type interpreter row.
func _field_row(objective: Dictionary, tag: String, field: Dictionary) -> Control:
	var name := str(field.get("name", ""))
	var kind := str(field.get("kind", ""))
	var flags: Array = field.get("flags", [])
	if name == "class" or name == "editorPos":
		return null
	if name == "parents" or flags.has("synthetic"):
		return null
	var box := VBoxContainer.new()
	box.add_child(MindWidgets.label(name))
	var value: Variant = objective.get(name, _default_of(field))
	if kind == "string":
		if flags.has("multiline"):
			var area := TextEdit.new()
			area.text = str(value) if value != null else ""
			area.custom_minimum_size.y = 100.0
			area.text_changed.connect(func() -> void:
				_objectives[_selected][name] = area.text
				_commit())
			box.add_child(area)
		else:
			var edit := MindWidgets.field("")
			edit.text = str(value) if value != null else ""
			edit.text_changed.connect(func(text: String) -> void:
				_objectives[_selected][name] = text
				_commit())
			box.add_child(edit)
	elif kind == "bool":
		var check := MindWidgets.check("", bool(value), func(pressed: bool) -> void:
			_objectives[_selected][name] = pressed
			_commit())
		box.add_child(check)
	elif kind == "byte" or kind == "int":
		var edit := _number_field(value, flags, false, func(number: float) -> void:
			_objectives[_selected][name] = int(number)
			_commit())
		box.add_child(edit)
	elif kind == "float":
		var edit := _number_field(value, flags, true, func(number: float) -> void:
			_objectives[_selected][name] = number
			_commit())
		box.add_child(edit)
	elif kind.begins_with("content:"):
		var button := MindWidgets.button(str(value))
		button.pressed.connect(func() -> void:
			_picker_target = "content:%d:%s" % [_selected, name]
			_open_content_picker(kind.trim_prefix("content:")))
		box.add_child(button)
	elif kind == "team":
		var button := MindWidgets.button(_team_name(int(value)))
		button.pressed.connect(func() -> void:
			_picker_target = "team:%d:%s" % [_selected, name]
			_open_team_picker())
		box.add_child(button)
	elif kind == "vec2i" or kind == "vec2f":
		var pair: Dictionary = value if value is Dictionary else {"x": 0, "y": 0}
		var row := HBoxContainer.new()
		row.add_child(_number_field(pair.get("x", 0), flags, kind == "vec2f",
			func(number: float) -> void:
				pair["x"] = number if kind == "vec2f" else int(number)
				_objectives[_selected][name] = pair
				_commit()))
		row.add_child(_number_field(pair.get("y", 0), flags, kind == "vec2f",
			func(number: float) -> void:
				pair["y"] = number if kind == "vec2f" else int(number)
				_objectives[_selected][name] = pair
				_commit()))
		box.add_child(row)
	else:
		# `objective` references, `Seq`/`Map` fields: listed but not editable here.
		box.add_child(MindWidgets.label(str(value)))
	return box


## `@Second` x60 / `@TilePos` x8 display scaling (`interpret_number`).
func _number_field(value: Variant, flags: Array, is_float: bool, setter: Callable) -> LineEdit:
	var scale := 1.0
	if flags.has("second"):
		scale = 60.0
	elif flags.has("tilepos"):
		scale = 8.0
	var raw := float(value) if value is float or value is int else 0.0
	var edit := MindWidgets.field("")
	edit.custom_minimum_size.x = 120.0
	edit.text = ("%f" % (raw / scale)) if is_float else str(int(raw / scale))
	edit.text_changed.connect(func(text: String) -> void:
		if text.is_empty() or text == "-":
			return
		var number := float(text) * scale
		if is_float and text.is_valid_float():
			setter.call(number)
		elif not is_float and text.is_valid_int():
			setter.call(number))
	return edit


## Hidden/synthetic fields stay out of the panel (`FieldFlags`).
func _is_visible_field(field: Dictionary) -> bool:
	var flags: Array = field.get("flags", [])
	return not flags.has("hidden") and not flags.has("synthetic")


func _default_of(field: Dictionary) -> Variant:
	# `provider_default` values are materialized by `objective_default_json`;
	# fields missing there (content/team/vec2) fall back per kind.
	var kind := str(field.get("kind", ""))
	if kind.begins_with("content:"):
		return "copper"
	if kind == "team":
		return 1
	if kind == "vec2i":
		return {"x": 0, "y": 0}
	if kind == "vec2f":
		return {"x": 0.0, "y": 0.0}
	return null


func _class_entry(tag: String) -> Dictionary:
	for value in _classes:
		var entry: Dictionary = value
		if str(entry.get("tag", "")) == tag:
			return entry
	return {}


func _objective_type_name(tag: String) -> String:
	return _t("@objective.%s.name" % tag.to_lower())


func _team_name(id: int) -> String:
	for row_value in get_node_or_null("/root/Spine/MindEditor").call("palette_teams"):
		var team_row: Dictionary = row_value
		if int(team_row.get("id", -1)) == id:
			return str(team_row.get("name", ""))
	return str(id)


func _small_button(icon: String, handler: Callable) -> Button:
	var button := MindWidgets.icon_button(icon, "", "emptyi")
	button.custom_minimum_size = Vector2(34.0, 34.0)
	button.pressed.connect(handler)
	return button


func _move(direction: int) -> void:
	var target := _selected + direction
	if _selected < 0 or target < 0 or target >= _objectives.size():
		return
	var temporary: Variant = _objectives[_selected]
	_objectives[_selected] = _objectives[target]
	_objectives[target] = temporary
	_selected = target
	_commit()
	_build()


func _remove_selected() -> void:
	if _selected < 0 or _selected >= _objectives.size():
		return
	_objectives.remove_at(_selected)
	_selected = -1
	_commit()
	_build()


## The content picker (`showContentSelect`): `list_select` over the type's names.
func _open_content_picker(content_type_name: String) -> void:
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("content_list"):
		return
	var items: Variant = host.call("content_list", content_type_name)
	if not (items is Array):
		return
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	_connect_picker()
	ui.call("open_dialog", "list_select", JSON.stringify({
		"title": "",
		"items": items,
		"reset": "",
	}))


## `showTeamSelect`: the base-team picker (`palette_teams`).
func _open_team_picker() -> void:
	var names: Array = []
	for row_value in get_node_or_null("/root/Spine/MindEditor").call("palette_teams"):
		var team_row: Dictionary = row_value
		names.append(str(team_row.get("name", "")))
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	_connect_picker()
	ui.call("open_dialog", "list_select", JSON.stringify({
		"title": "",
		"items": names,
		"reset": "",
	}))


func _connect_picker() -> void:
	var ui_root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui_root == null or not ui_root.has_method("dialog"):
		return
	var picker: Node = ui_root.call("dialog", "list_select")
	if picker != null and picker.has_signal("selected") \
			and not picker.is_connected("selected", _on_picked):
		picker.selected.connect(_on_picked)


func _on_picked(name: String) -> void:
	if name.is_empty() or _picker_target.is_empty():
		return
	var parts := _picker_target.split(":")
	_picker_target = ""
	var index := int(parts[1]) if parts.size() > 1 else -1
	var field := parts[2] if parts.size() > 2 else ""
	if index < 0 or index >= _objectives.size():
		return
	if parts[0] == "content":
		_objectives[index][field] = name
	elif parts[0] == "team":
		_objectives[index][field] = _team_id(name)
	_commit()
	_rebuild_panel()


func _team_id(name: String) -> int:
	for row_value in get_node_or_null("/root/Spine/MindEditor").call("palette_teams"):
		var team_row: Dictionary = row_value
		if str(team_row.get("name", "")) == name:
			return int(team_row.get("id", -1))
	return -1


func _read_classes() -> Array:
	if _editor == null or not _editor.has_method("objective_classes_json"):
		return []
	var parsed: Variant = JSON.parse_string(str(_editor.call("objective_classes_json")))
	return parsed if parsed is Array else []


func _read_objectives() -> Array:
	if _editor == null or not _editor.has_method("objectives_json"):
		return []
	var parsed: Variant = JSON.parse_string(str(_editor.call("objectives_json")))
	return parsed if parsed is Array else []


func _commit() -> void:
	if _editor != null and _editor.has_method("set_objectives_json"):
		_editor.call("set_objectives_json", JSON.stringify(_objectives))


func _connect_confirm() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_signal("confirm_result") \
			and not ui.is_connected("confirm_result", _on_confirm):
		ui.connect("confirm_result", _on_confirm)


func _on_confirm(confirmed: bool) -> void:
	if not _pending_clear:
		return
	_pending_clear = false
	if confirmed:
		_objectives = []
		_selected = -1
		_commit()
		_build()


func _notify(key: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_info"):
		ui.call("show_info", _t(key))
