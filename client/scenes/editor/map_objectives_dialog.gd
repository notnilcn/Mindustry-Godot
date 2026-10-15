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
## `MapObjectives.allMarkerTypes` descriptors (`[{tag, fields}]`).
var _marker_classes: Array = []
var _objectives: Array = []
var _selected := -1
## The objective being edited in the class picker (`-1` = append).
var _pending_class_index := -1
## Set while the `@clear` confirm is pending (`@settings.clear.confirm`).
var _pending_clear := false
## The objective created by `@add` waiting for a canvas click (`canvas.query`).
var _pending_query: Dictionary = {}
## The color picker's answer target (`Color` interpreter).
var _pending_color_set: Callable = Callable()
## The list picker's answer target (content/team pickers).
var _pending_pick_set: Callable = Callable()

## `LStatement.aligns` / `nameToAlign` (`Arc Align` bit values).
const ALIGN_ORDER := [
	"topLeft", "top", "topRight", "left", "center", "right",
	"bottomLeft", "bottom", "bottomRight",
]
const ALIGN_NAMES := {
	"center": 1, "top": 2, "bottom": 4, "left": 8, "right": 16,
	"topLeft": 10, "topRight": 18, "bottomLeft": 12, "bottomRight": 20,
}


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
	_marker_classes = _read_marker_classes()
	_objectives = _read_objectives()
	_rebuild_layout()
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
		if _canvas.has_signal("node_moved"):
			_canvas.node_moved.connect(_on_node_moved)
		if _canvas.has_signal("query_placed"):
			_canvas.query_placed.connect(_on_query_placed)
		row.add_child(_canvas)
		_canvas.call("set_selected", _selected)

	# code-instantiated: the field panel is driven by the class descriptors.
	var scroll := ScrollContainer.new()
	scroll.custom_minimum_size.x = PANEL_WIDTH
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	row.add_child(scroll)
	_panel = VBoxContainer.new()
	_panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(_panel)
	_rebuild_panel()


## `@add`: the objective type picker (`getProvider(MapObjective.class)`); the
## picked class becomes the canvas query cursor (`canvas::query`).
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
		_pending_class_index = -1
		_commit()
		_build()
		return
	_pending_class_index = -1
	# `@add` -> `canvas::query`: click the canvas to place it (`placeQuery`).
	_pending_query = created
	if _canvas != null:
		_canvas.call("set_querying", true)
		_notify("@editor.objectives")


## `placeQuery`: `tilemap.createTile(queryX, queryY, query)` + append.
func _on_query_placed(x: int, y: int) -> void:
	_pending_query["editorPos"] = _pack_pos(x, y)
	_objectives.append(_pending_query)
	_pending_query = {}
	_selected = _objectives.size() - 1
	if _canvas != null:
		_canvas.call("set_querying", false)
	_commit()
	_build()


## `ObjectiveTile.Mover.touchUp` -> `moveTile`: write the snapped position.
func _on_node_moved(index: int, x: int, y: int) -> void:
	if index < 0 or index >= _objectives.size() or not (_objectives[index] is Dictionary):
		return
	_objectives[index]["editorPos"] = _pack_pos(x, y)
	_commit()


func _pack_pos(x: int, y: int) -> int:
	var value := (x & 0xFFFF) | ((y & 0xFFFF) << 16)
	if value >= 2147483648:
		value -= 4294967296
	return value


func _has_pos(editor_pos: int) -> bool:
	return editor_pos != _pack_pos(-999, -999)


func _node_x(editor_pos: int) -> int:
	var x := (editor_pos >> 16) & 0xFFFF
	return x - 65536 if x > 32767 else x


func _node_y(editor_pos: int) -> int:
	var y := editor_pos & 0xFFFF
	return y - 65536 if y > 32767 else y


## `rebuildObjectives`: programmatic objectives (editorX == -999) get a grid
## layout before the canvas draws them (`MapObjectivesDialog.java:601-631`).
func _rebuild_layout() -> void:
	var any_unplaced := false
	for objective in _objectives:
		if objective is Dictionary and not _has_pos(int(objective.get("editorPos", 0))):
			any_unplaced = true
			break
	if not any_unplaced:
		return
	var width := 5 + 2
	var len := _objectives.size() * width
	var columns := _objectives.size()
	var rows := 1
	if len > 100:
		rows = len / 100
		columns = 100 / width
	var index := 0
	for y in rows:
		for x in columns:
			if index >= _objectives.size():
				break
			var objective: Dictionary = _objectives[index] \
				if _objectives[index] is Dictionary else {}
			objective["editorPos"] = _pack_pos(x * width, y)
			index += 1
	if index > 0:
		_commit()


func _select_objective(index: int) -> void:
	if index < 0 or index >= _objectives.size():
		return
	_selected = index
	if _canvas != null:
		_canvas.call("set_selected", index)
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
##
## Java renders one interpreter per field kind; `Seq` fields own their header
## (the name is drawn inside), so they dispatch before the label row.
func _field_row(objective: Dictionary, tag: String, field: Dictionary) -> Control:
	var name := str(field.get("name", ""))
	var kind := str(field.get("kind", ""))
	var flags: Array = field.get("flags", [])
	if name == "class" or name == "editorPos":
		return null
	if name == "parents" or flags.has("synthetic"):
		return null
	var getter := func() -> Variant: return objective.get(name, _default_of(field))
	var setter := func(value: Variant) -> void:
		objective[name] = value
		_commit()
	if kind.begins_with("seq:"):
		# Java's Seq interpreter writes `name:` in its own accent header.
		return _seq_editor(name, getter, setter, kind.trim_prefix("seq:"), flags)
	var box := VBoxContainer.new()
	box.add_child(MindWidgets.label("%s:" % name))
	box.add_child(_make_editor(getter, setter, kind, flags))
	return box


## The generic field interpreter (`MapObjectivesDialog`'s interpreter table):
## builds exactly one control for the value behind `get_value`/`set_value`.
func _make_editor(get_value: Callable, set_value: Callable, kind: String, flags: Array) -> Control:
	var value: Variant = get_value.call()
	if kind == "string":
		if flags.has("logiccode"):
			# `@LogicCode`: Java renders only the pencil button
			# (`ui.logic.show(code, null, true, set)`).
			var pencil := MindWidgets.icon_button("pencil", "", "emptyi")
			pencil.custom_minimum_size = Vector2(34.0, 34.0)
			pencil.pressed.connect(_open_logic_dialog.bind(get_value, set_value))
			return pencil
		if flags.has("multiline"):
			var area := TextEdit.new()
			area.text = str(value) if value != null else ""
			area.custom_minimum_size.y = 100.0
			area.text_changed.connect(func() -> void:
				set_value.call(area.text))
			return area
		var edit := MindWidgets.field("")
		edit.text = str(value) if value != null else ""
		edit.text_changed.connect(func(text: String) -> void:
			set_value.call(text))
		return edit
	if kind == "bool":
		return MindWidgets.check("", bool(value), func(pressed: bool) -> void:
			set_value.call(pressed))
	if kind == "indexbool":
		# `@IndexBool int`: checked means `!= -1`; writes `1`/`-1`.
		var enabled := int(value) != -1
		return MindWidgets.check("", enabled, func(pressed: bool) -> void:
			set_value.call(1 if pressed else -1))
	if kind == "byte" or kind == "int" or kind == "float":
		var scale := 1.0
		if flags.has("second"):
			scale = 60.0
		elif flags.has("tilepos"):
			scale = 8.0
		return _number_value_field(kind == "float", scale, get_value, set_value)
	if kind.begins_with("content:"):
		var button := MindWidgets.button(str(value))
		button.pressed.connect(func() -> void:
			_open_content_picker(kind.trim_prefix("content:"), set_value))
		return button
	if kind == "team":
		var button := MindWidgets.button(_team_name(int(value)))
		button.pressed.connect(func() -> void:
			_open_team_picker(set_value))
		return button
	if kind == "color":
		return _color_editor(get_value, set_value)
	if kind == "vec2i" or kind == "vec2f":
		var pair: Dictionary = value if value is Dictionary else {"x": 0, "y": 0}
		var row := HBoxContainer.new()
		var is_float := kind == "vec2f"
		var scale := 8.0 if flags.has("tilepos") else 1.0
		row.add_child(_number_value_field(is_float, scale,
			func() -> Variant: return pair.get("x", 0),
			func(number: Variant) -> void:
				pair["x"] = number if is_float else int(number)
				set_value.call(pair)))
		row.add_child(_number_value_field(is_float, scale,
			func() -> Variant: return pair.get("y", 0),
			func(number: Variant) -> void:
				pair["y"] = number if is_float else int(number)
				set_value.call(pair)))
		return row
	if kind == "alignment":
		return _align_editor(get_value, set_value, flags)
	if kind == "labelflag":
		return _label_flag_editor(get_value, set_value)
	if kind == "texture":
		return _texture_editor(get_value, set_value)
	if kind == "marker":
		return _marker_editor(get_value, set_value)
	if kind == "verts":
		# Java's Vertices interpreter edits one row of x/y/color/u/v per
		# vertex; the port lists the raw array (documented deviation).
		return MindWidgets.label(str(value))
	# `map:` (no upstream field uses it) and `objective` references.
	return MindWidgets.label(str(value))


## One element of a `Seq` field: the element header carries the
## remover/indexer buttons Java passes into the element interpreter.
func _element_row(get_value: Callable, set_value: Callable, kind: String, flags: Array,
		remover: Callable, indexer: Callable) -> Control:
	var box := VBoxContainer.new()
	var header := HBoxContainer.new()
	header.add_theme_constant_override("separation", 4)
	if remover.is_valid():
		header.add_child(_small_button("cancel", remover))
	if indexer.is_valid():
		header.add_child(_small_button("up", func() -> void: indexer.call(true)))
		header.add_child(_small_button("down", func() -> void: indexer.call(false)))
	if header.get_child_count() > 0:
		box.add_child(header)
	box.add_child(_make_editor(get_value, set_value, kind, flags))
	return box


## `Seq` interpreter (`:335-402`): name header + add / per-element editors.
func _seq_editor(name: String, get_arr: Callable, set_arr: Callable,
		inner_kind: String, flags: Array) -> Control:
	var box := VBoxContainer.new()
	var header := HBoxContainer.new()
	header.add_theme_constant_override("separation", 4)
	if not name.is_empty():
		var label := MindWidgets.label("%s:" % name)
		label.add_theme_color_override("font_color", Color("ffd37f"))
		header.add_child(label)
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	header.add_child(spacer)
	spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
	header.add_child(_small_button("add", func() -> void: _seq_add(get_arr, set_arr, inner_kind)))
	box.add_child(header)
	var arr: Array = get_arr.call()
	for index in arr.size():
		var i := index
		var element_get := func() -> Variant: return arr[i]
		var element_set := func(value: Variant) -> void:
			arr[i] = value
			set_arr.call(arr)
		var remove := func() -> void:
			arr.remove_at(i)
			set_arr.call(arr)
			_rebuild_panel()
		var move := func(up: bool) -> void:
			if up and i > 0:
				var temporary: Variant = arr[i]
				arr[i] = arr[i - 1]
				arr[i - 1] = temporary
			elif not up and i < arr.size() - 1:
				var temporary: Variant = arr[i]
				arr[i] = arr[i + 1]
				arr[i + 1] = temporary
			else:
				return
			set_arr.call(arr)
			_rebuild_panel()
		var indexer := move if not flags.has("unordered") else Callable()
		box.add_child(_element_row(element_get, element_set, inner_kind, flags, remove, indexer))
	return box


## The `Seq` `@add` button: `getProvider(element).get` then append + rebuild.
func _seq_add(get_arr: Callable, set_arr: Callable, inner_kind: String) -> void:
	var arr: Array = get_arr.call()
	if inner_kind == "marker":
		_open_marker_picker(func(marker: Variant) -> void:
			arr.append(marker)
			set_arr.call(arr)
			_rebuild_panel())
		return
	arr.append(_provider_default_of(inner_kind))
	set_arr.call(arr)
	_rebuild_panel()


func _provider_default_of(kind: String) -> Variant:
	if kind.begins_with("content:"):
		return "copper"
	match kind:
		"string":
			return ""
		"bool":
			return false
		"byte", "int":
			return 0
		"float":
			return 0.0
		"vec2i":
			return {"x": 0, "y": 0}
		"vec2f":
			return {"x": 0.0, "y": 0.0}
		"team":
			return 1
		"color":
			return "ffd37fff"
		"labelflag":
			return 3
		"alignment":
			return 1
		"texture":
			return {"string": "white"}
		"marker":
			return {}
	return null


## `Color` interpreter (`:151-163`): a color swatch opening `ui.picker`.
func _color_editor(get_value: Callable, set_value: Callable) -> Control:
	var button := Button.new()
	button.custom_minimum_size = Vector2(50.0, 50.0)
	var swatch := ColorRect.new()
	swatch.color = _color_from_hex(str(get_value.call()))
	swatch.set_anchors_preset(Control.PRESET_FULL_RECT)
	swatch.mouse_filter = Control.MOUSE_FILTER_IGNORE
	button.add_child(swatch)
	button.pressed.connect(func() -> void:
		_open_color_picker(str(get_value.call()), set_value))
	return button


func _color_from_hex(hex: String) -> Color:
	if hex.length() == 8:
		return Color8(
			hex.substr(0, 2).hex_to_int(),
			hex.substr(2, 2).hex_to_int(),
			hex.substr(4, 2).hex_to_int(),
			hex.substr(6, 2).hex_to_int())
	return Color("ffd37f")


func _open_color_picker(current: String, setter: Callable) -> void:
	var ui := get_node_or_null("/root/MindUi")
	var ui_root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui == null or not ui.has_method("open_dialog") or ui_root == null \
			or not ui_root.has_method("dialog"):
		return
	var picker: Node = ui_root.call("dialog", "picker")
	if picker == null or not picker.has_signal("color_selected"):
		return
	_pending_color_set = setter
	if picker.has_method("set_color"):
		picker.call("set_color", _color_from_hex(current))
	if picker.is_connected("color_selected", _on_color_picked):
		picker.disconnect("color_selected", _on_color_picked)
	picker.connect("color_selected", _on_color_picked)
	ui.call("open_dialog", "picker", "")


func _on_color_picked(color: Color) -> void:
	if not _pending_color_set.is_valid():
		return
	var setter := _pending_color_set
	_pending_color_set = Callable()
	setter.call("%02x%02x%02x%02x" % [
		int(color.r * 255.0), int(color.g * 255.0), int(color.b * 255.0), int(color.a * 255.0),
	])
	_rebuild_panel()


## `@LabelFlag byte` (`:309-325`): the two `WorldLabel` flag checks.
func _label_flag_editor(get_value: Callable, set_value: Callable) -> Control:
	var box := VBoxContainer.new()
	var value := int(get_value.call())
	for bit in [[1, "@marker.background"], [2, "@marker.outline"]]:
		box.add_child(MindWidgets.check(_t(str(bit[1])), (value & int(bit[0])) == int(bit[0]),
			func(pressed: bool) -> void:
				var current := int(get_value.call())
				set_value.call((current | int(bit[0])) if pressed else (current & ~int(bit[0])))))
	return box


## `@Alignment int` (`:290-297`): the `LStatement.showAlignSelect` presets.
func _align_editor(get_value: Callable, set_value: Callable, flags: Array) -> Control:
	var button := MindWidgets.button(_align_name(int(get_value.call())))
	button.pressed.connect(func() -> void:
		_open_align_picker(get_value, set_value, flags))
	return button


func _align_name(align: int) -> String:
	for name in ALIGN_ORDER:
		if ALIGN_NAMES[name] == align:
			return name
	return "center"


func _open_align_picker(get_value: Callable, set_value: Callable, flags: Array) -> void:
	var popup := PopupPanel.new()
	# code-instantiated: one transient align-select popup per open.
	add_child(popup)
	var grid := GridContainer.new()
	grid.columns = 3
	popup.add_child(grid)
	var current := int(get_value.call())
	for name in ALIGN_ORDER:
		var value: int = ALIGN_NAMES[name]
		if flags.has("align-nover") and value != 1 and value != 8 and value != 16:
			continue
		if flags.has("align-nohor") and value != 1 and value != 2 and value != 4:
			continue
		var choice := Button.new()
		choice.text = name
		choice.custom_minimum_size = Vector2(100.0, 38.0)
		choice.theme_type_variation = "flatt"
		if value == current:
			choice.theme_type_variation = "flatTogglet"
		choice.pressed.connect(func() -> void:
			set_value.call(value)
			popup.hide())
		grid.add_child(choice)
	popup.popup_centered()


## `TextureHolder` interpreter (`:299-302`): the string-backed name.
func _texture_editor(get_value: Callable, set_value: Callable) -> Control:
	var value: Variant = get_value.call()
	var holder: Dictionary = value if value is Dictionary else {}
	var edit := MindWidgets.field("")
	edit.text = str(holder.get("string", ""))
	edit.text_changed.connect(func(text: String) -> void:
		set_value.call({"string": text}))
	return edit


## `defaultInterpreter()` for one `ObjectiveMarker` element (`:417-468`).
func _marker_editor(get_value: Callable, set_value: Callable) -> Control:
	var box := VBoxContainer.new()
	var value: Variant = get_value.call()
	var marker: Dictionary = value if value is Dictionary else {}
	var entry := _marker_class_entry(str(marker.get("class", "")))
	for value_row in entry.get("fields", []):
		var field: Dictionary = value_row
		if not _is_visible_field(field):
			continue
		var name := str(field.get("name", ""))
		var row := HBoxContainer.new()
		row.add_theme_constant_override("separation", 4)
		row.add_child(MindWidgets.label("%s:" % name))
		var field_get := func() -> Variant: return marker.get(name, _default_of(field))
		var field_set := func(new_value: Variant) -> void:
			marker[name] = new_value
			set_value.call(marker)
		row.add_child(_make_editor(field_get, field_set,
			str(field.get("kind", "")), field.get("flags", [])))
		box.add_child(row)
	return box


## The marker picker (`getProvider(ObjectiveMarker.class)`, `:236-256`).
func _open_marker_picker(setter: Callable) -> void:
	if _marker_classes.is_empty():
		return
	# code-instantiated: the marker type picker has no static scene.
	var overlay := ColorRect.new()
	overlay.name = "MarkerPicker"
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
	for value in _marker_classes:
		var entry: Dictionary = value
		var tag := str(entry.get("tag", ""))
		var button := MindWidgets.button(_marker_type_name(tag))
		button.theme_type_variation = "flatt"
		button.alignment = HORIZONTAL_ALIGNMENT_LEFT
		button.custom_minimum_size = Vector2(195.0, 56.0)
		button.pressed.connect(func() -> void:
			var parsed: Variant = JSON.parse_string(str(_editor.call("marker_default_json", tag)))
			if parsed is Dictionary:
				setter.call(parsed)
			overlay.queue_free())
		grid.add_child(button)
	var back := MindWidgets.button(_t("@back"))
	back.pressed.connect(overlay.queue_free)
	box.add_child(back)


func _marker_type_name(tag: String) -> String:
	# `ObjectiveMarker.typeName()`: `bundle.get("marker.<lower>.name", className)`;
	# upstream has no `marker.light.name`, so it falls back to the class name.
	var key := "marker.%s.name" % tag.to_lower()
	var assets := MindWidgets.assets()
	if assets != null and assets.has_method("bundle_get"):
		var value := str(assets.call("bundle_get", key))
		if value != key:
			return value
	return tag


func _marker_class_entry(tag: String) -> Dictionary:
	for value in _marker_classes:
		var entry: Dictionary = value
		if str(entry.get("tag", "")) == tag:
			return entry
	return {}


## `ui.logic.show(code, null, true, set)` for `@LogicCode` fields.
func _open_logic_dialog(get_value: Callable, set_value: Callable) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("open_dialog"):
		return
	var logic := _dialog("logic")
	if logic != null and logic.has_signal("code_edited"):
		if logic.is_connected("code_edited", _on_logic_code_edited):
			logic.disconnect("code_edited", _on_logic_code_edited)
		logic.connect("code_edited", _on_logic_code_edited.bind(set_value))
	ui.call("open_dialog", "logic", JSON.stringify({
		"filter": true,
		"privileged": true,
		"code": str(get_value.call()),
	}))


func _on_logic_code_edited(code: String, setter: Callable) -> void:
	setter.call(code)


## `@Second` x60 / `@TilePos` x8 display scaling (`interpret_number`); the
## display re-syncs from the model on focus loss (Java's `field.update`).
func _number_value_field(is_float: bool, scale: float, get_value: Callable, set_value: Callable) -> LineEdit:
	var edit := MindWidgets.field("")
	edit.custom_minimum_size.x = 120.0
	var sync := func() -> void:
		var raw := float(get_value.call())
		edit.text = ("%f" % (raw / scale)) if is_float else str(int(raw / scale))
	sync.call()
	edit.text_changed.connect(func(text: String) -> void:
		if text.is_empty() or text == "-":
			return
		if is_float and text.is_valid_float():
			set_value.call(float(text) * scale)
		elif not is_float and text.is_valid_int():
			set_value.call(float(text) * scale))
	edit.focus_exited.connect(func() -> void: sync.call())
	return edit


## Hidden/synthetic fields stay out of the panel (`FieldFlags`).
func _is_visible_field(field: Dictionary) -> bool:
	var flags: Array = field.get("flags", [])
	return not flags.has("hidden") and not flags.has("synthetic")


func _default_of(field: Dictionary) -> Variant:
	# `provider_default` values are materialized by `objective_default_json`;
	# fields missing there fall back per kind.
	var kind := str(field.get("kind", ""))
	if kind.begins_with("content:"):
		return "copper"
	if kind == "team":
		return 1
	if kind == "indexbool":
		return 1
	if kind == "labelflag":
		return 3
	if kind == "alignment":
		return 1
	if kind == "texture":
		return {"string": "white"}
	if kind == "color":
		return "ffd37fff"
	if kind == "vec2i":
		return {"x": 0, "y": 0}
	if kind == "vec2f":
		return {"x": 0.0, "y": 0.0}
	if kind == "marker":
		return {}
	if kind == "verts":
		return []
	if kind.begins_with("seq:") or kind.begins_with("map:"):
		return []
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
func _open_content_picker(content_type_name: String, setter: Callable) -> void:
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("content_list"):
		return
	# `content_list` is a `PackedStringArray` (the `is Array` guard never fired).
	var listed: Variant = host.call("content_list", content_type_name)
	var items: Array = []
	if listed is PackedStringArray:
		for name in listed:
			items.append(str(name))
	elif listed is Array:
		items = listed
	else:
		return
	if items.is_empty():
		return
	_open_list_picker(items, setter)


## `showTeamSelect`: the base-team picker (`palette_teams`).
func _open_team_picker(setter: Callable) -> void:
	var names: Array = []
	for row_value in get_node_or_null("/root/Spine/MindEditor").call("palette_teams"):
		var team_row: Dictionary = row_value
		names.append(str(team_row.get("name", "")))
	_open_list_picker(names, func(name: String) -> void:
		setter.call(_team_id(name)))


## `list_select` + the `selected` signal (`showContentSelect`/`showTeamSelect`).
func _open_list_picker(items: Array, on_selected: Callable) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("open_dialog"):
		return
	var ui_root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui_root == null or not ui_root.has_method("dialog"):
		return
	var picker: Node = ui_root.call("dialog", "list_select")
	if picker == null or not picker.has_signal("selected"):
		return
	_pending_pick_set = on_selected
	if picker.is_connected("selected", _on_picked):
		picker.disconnect("selected", _on_picked)
	picker.connect("selected", _on_picked)
	ui.call("open_dialog", "list_select", JSON.stringify({
		"title": "",
		"items": items,
		"reset": "",
	}))


func _on_picked(name: String) -> void:
	if name.is_empty() or not _pending_pick_set.is_valid():
		return
	var setter := _pending_pick_set
	_pending_pick_set = Callable()
	setter.call(name)
	_rebuild_panel()


## `/root/Spine/Ui/UiRoot.dialog(name)` (`MapProcessorsDialog` route).
func _dialog(name: String) -> Node:
	var ui_root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui_root == null or not ui_root.has_method("dialog"):
		return null
	return ui_root.call("dialog", name)


func _team_id(name: String) -> int:
	for row_value in get_node_or_null("/root/Spine/MindEditor").call("palette_teams"):
		var team_row: Dictionary = row_value
		if str(team_row.get("name", "")) == name:
			return int(team_row.get("id", -1))
	return -1


## `MapObjectivesDialog.doInput` (`:556-588`): Ctrl+C copies the objective under
## the cursor, Ctrl+V pastes the clipboard at the cursor's tile.
func _input(event: InputEvent) -> void:
	if not visible or _canvas == null or not is_top():
		return
	var key := event as InputEventKey
	if key == null or not key.pressed or key.echo or not key.ctrl_pressed:
		return
	# Let focused text fields keep their own copy/paste.
	var focus := get_viewport().gui_get_focus_owner()
	if focus is LineEdit or focus is TextEdit:
		return
	if key.keycode == KEY_C:
		_copy_under_cursor()
		get_viewport().set_input_as_handled()
	elif key.keycode == KEY_V:
		_paste_at_cursor()
		get_viewport().set_input_as_handled()


func _copy_under_cursor() -> void:
	var mouse: Vector2 = _canvas.call("cursor_position")
	var index := int(_canvas.call("node_index_at", mouse))
	if index < 0 or index >= _objectives.size():
		return
	DisplayServer.clipboard_set(JSON.stringify(_objectives[index]))
	_notify("@copied")


func _paste_at_cursor() -> void:
	var parsed: Variant = JSON.parse_string(DisplayServer.clipboard_get())
	if not (parsed is Dictionary) or not (parsed as Dictionary).has("class"):
		return
	var cell: Vector2i = _canvas.call("cursor_cell")
	if not bool(_canvas.call("cell_valid", cell.x, cell.y)):
		return
	(parsed as Dictionary)["editorPos"] = _pack_pos(cell.x, cell.y)
	_objectives.append(parsed)
	_selected = _objectives.size() - 1
	_commit()
	_build()


func _read_classes() -> Array:
	if _editor == null or not _editor.has_method("objective_classes_json"):
		return []
	var parsed: Variant = JSON.parse_string(str(_editor.call("objective_classes_json")))
	return parsed if parsed is Array else []


func _read_marker_classes() -> Array:
	if _editor == null or not _editor.has_method("marker_classes_json"):
		return []
	var parsed: Variant = JSON.parse_string(str(_editor.call("marker_classes_json")))
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
