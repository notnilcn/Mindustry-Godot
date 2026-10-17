## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/WaveInfoDialog.java (plan 19 M5 §3.9).
##
## UI-only: the wave group editor (search, unit filter, sort menu, per-group
## expansion with every `SpawnGroup` field, guardian/team/spawn rows, the
## clipboard menu, randomize) plus the WaveGraph. The group JSON is validated by
## `MindEditor.set_waves_json`.

extends MindDialog

## `SpawnGroup.NEVER` (`i32::MAX`).
const NEVER := 2147483647
## `WaveInfoDialog.setup`: the left editor column.
const PANEL_WIDTH := 390.0
## `WaveInfoDialog.showUnits`: three picker entries per row.
const PICKER_COLUMNS := 3
## `spawner.getSpawns()` display cap (`i >= 20`).
const SPAWN_CAP := 20

enum Sort { BEGIN, HEALTH, TYPE }

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"

var _editor: Node = null
var _list: VBoxContainer = null
var _graph: Control = null
## Unit names in content-id order for the `type` sort keys.
var _type_order: Array = []
## Working group list (`WaveInfoDialog.groups`); written back on every edit.
var _groups: Array = []
## Expanded group index (`expandedGroup`), -1 for none.
var _expanded := -1
## 0-based wave search (`search`), -1 for none.
var _search := -1
## Unit-type filter (`filterType`), "" for none.
var _filter_type := ""
var _sort: Sort = Sort.BEGIN
var _reverse := false
## Pending `list_select` target (`filter`/`unit:<i>`/`effect:<i>`/`spawn:<i>`/`team:<i>`).
var _picker_target := ""
## Inline panels (`@edit.menu` / `@waves.sort`).
var _edit_panel: VBoxContainer = null
var _sort_panel: VBoxContainer = null


func _ready() -> void:
	set_title_key("@waves.title")
	should_pause = false
	super._ready()
	_editor = get_node_or_null(editor_path)
	clear_buttons()
	# `addCloseButton()` first, then `@edit.menu` (Icon.edit, 250x64) and
	# `waves.random` (Icon.refresh, width 200) (`WaveInfoDialog.java:48-95`).
	add_close_button()
	add_button(_t("@edit.menu"), _toggle_edit_panel, "edit", 250.0)
	add_button(_t("@waves.random"), _randomize, "refresh", 200.0)
	_connect_picker()


func shown() -> void:
	_groups = _read_groups()
	_expanded = -1
	# The graph reads the tag: write the working list (vanilla fallback
	# included) so it matches `graph.groups = groups` (`updateWaves`).
	_commit()
	_build()


func _build() -> void:
	if cont != null:
		for child in cont.get_children():
			cont.remove_child(child)
			child.queue_free()
	var root := content_table()
	var row := HBoxContainer.new()
	row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.size_flags_vertical = Control.SIZE_EXPAND_FILL
	root.add(row).grow_x_axis().grow_y_axis()

	# Left column: search, list, add/sort (`setup()`).
	var left := VBoxContainer.new()
	left.custom_minimum_size.x = PANEL_WIDTH
	left.size_flags_horizontal = Control.SIZE_FILL
	left.size_flags_vertical = Control.SIZE_EXPAND_FILL
	row.add_child(left)
	var search := HBoxContainer.new()
	search.add_child(MindWidgets.glyph("zoom", 20))
	var search_field := MindWidgets.field(_t("@waves.search"))
	search_field.max_length = 8
	search_field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	search_field.text_changed.connect(func(text: String) -> void:
		_search = (int(text) - 1) if text.is_valid_int() else -1
		_rebuild())
	search.add_child(search_field)
	# `imageUp = filterType != null ? filterType.uiIcon : Icon.filter`, 46 px.
	var filter_button := MindWidgets.icon_button("filter", "", "emptyi")
	filter_button.custom_minimum_size = Vector2(46.0, 46.0)
	filter_button.tooltip_text = _t("@waves.filter")
	filter_button.pressed.connect(_show_unit_filter)
	search.add_child(filter_button)
	left.add_child(search)

	var scroll := ScrollContainer.new()
	scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.custom_minimum_size = Vector2(PANEL_WIDTH - 20.0, 260.0)
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	left.add_child(scroll)
	_list = VBoxContainer.new()
	_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(_list)

	var actions := HBoxContainer.new()
	var add_button := MindWidgets.button(_t("@add"))
	add_button.custom_minimum_size.y = 70.0
	add_button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	add_button.pressed.connect(_show_unit_add)
	actions.add_child(add_button)
	# `t.button(Icon.filter, ...)`: default `ImageButton` (Tex.button frame),
	# `size(64f, 70f).padLeft(6f)` (`WaveInfoDialog.java:121-139`).
	var sort_button := MindWidgets.icon_button("filter", "", "defaulti")
	sort_button.custom_minimum_size = Vector2(64.0, 70.0)
	for slot in [["normal", "button"], ["hover", "button-over"], ["pressed", "button-down"]]:
		sort_button.add_theme_stylebox_override(slot[0], MindThemeBuilder._button_nine_patch(slot[1]))
	actions.add_theme_constant_override("separation", 6)
	sort_button.pressed.connect(_toggle_sort_panel)
	actions.add_child(sort_button)
	left.add_child(actions)

	# Inline `@edit.menu` panel (`MapInfoDialog`/`WaveInfoDialog` button row).
	_edit_panel = VBoxContainer.new()
	_edit_panel.visible = false
	left.add_child(_edit_panel)
	_build_edit_panel()
	# Inline `@waves.sort` panel.
	_sort_panel = VBoxContainer.new()
	_sort_panel.visible = false
	left.add_child(_sort_panel)
	_build_sort_panel()

	# Wave graph (`cont.add(graph)`).
	var graph_scene: PackedScene = load("res://scenes/editor/wave_graph.tscn")
	if graph_scene != null:
		# code-instantiated: the graph is a slide-out child of this dialog only.
		_graph = graph_scene.instantiate()
		_graph.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		_graph.size_flags_vertical = Control.SIZE_EXPAND_FILL
		_graph.custom_minimum_size = Vector2(240.0, 260.0)
		row.add_child(_graph)

	_rebuild()


## `WaveInfoDialog`'s `@edit.menu`: copy / load / clear / reset.
func _build_edit_panel() -> void:
	# code-instantiated: the edit menu is a fixed four-action strip.
	var copy := MindWidgets.button(_t("@copy.clipboard"))
	copy.pressed.connect(func() -> void:
		DisplayServer.clipboard_set(_editor.call("waves_json"))
		_notify_info(_t("@waves.copied")))
	_edit_panel.add_child(copy)
	var load_button := MindWidgets.button(_t("@load.clipboard"))
	load_button.pressed.connect(func() -> void:
		if not bool(_editor.call("set_waves_json", DisplayServer.clipboard_get())):
			_notify(_t("@waves.invalid"))
		else:
			_groups = _read_groups()
			_edit_panel.visible = false
			_rebuild())
	_edit_panel.add_child(load_button)
	var clear := MindWidgets.button(_t("@clear"))
	clear.pressed.connect(func() -> void:
		_groups = []
		_commit()
		_edit_panel.visible = false
		_rebuild())
	_edit_panel.add_child(clear)
	var reset := MindWidgets.button(_t("@settings.reset"))
	reset.pressed.connect(func() -> void:
		var parsed: Variant = JSON.parse_string(str(_editor.call("default_waves_json")))
		_groups = parsed if parsed is Array else []
		_commit()
		_edit_panel.visible = false
		_rebuild())
	_edit_panel.add_child(reset)


## `WaveInfoDialog`'s sort dialog (`@waves.sort.<s>` toggles + `@waves.sort.reverse`).
func _build_sort_panel() -> void:
	# code-instantiated: the sort panel is a fixed three-toggle strip.
	var sort_row := HBoxContainer.new()
	for sort_value in [Sort.BEGIN, Sort.HEALTH, Sort.TYPE]:
		var button := MindWidgets.button(_t("@waves.sort.%s" % _sort_name(sort_value)))
		button.theme_type_variation = "flatTogglet"
		button.toggle_mode = true
		button.button_pressed = _sort == sort_value
		button.pressed.connect(func() -> void:
			_sort = sort_value
			_sort_panel.visible = false
			_rebuild())
		sort_row.add_child(button)
	_sort_panel.add_child(sort_row)
	var reverse := MindWidgets.check(_t("@waves.sort.reverse"), _reverse, func(pressed: bool) -> void:
		_reverse = pressed
		_rebuild())
	_sort_panel.add_child(reverse)


func _sort_name(sort_value: int) -> String:
	match sort_value:
		Sort.HEALTH:
			return "health"
		Sort.TYPE:
			return "type"
		_:
			return "begin"


func _toggle_sort_panel() -> void:
	_sort_panel.visible = not _sort_panel.visible
	_edit_panel.visible = false


## `buttons.button("@edit.menu", Icon.edit, ...)`: the copy/load/clear/reset strip.
func _toggle_edit_panel() -> void:
	if _edit_panel == null:
		return
	_edit_panel.visible = not _edit_panel.visible
	_sort_panel.visible = false


## `buttons.button(Core.bundle.get("waves.random"), Icon.refresh, ...)`.
func _randomize() -> void:
	if _editor == null or not bool(_editor.call("randomize_waves")):
		return
	_groups = _read_groups()
	_expanded = -1
	_rebuild()


## `buildGroups`: sorts, filters and rebuilds every group card.
func _rebuild() -> void:
	if _list == null:
		return
	for child in _list.get_children():
		_list.remove_child(child)
		child.queue_free()
	_sort_groups()
	var any := false
	for index in _groups.size():
		var group: Dictionary = _groups[index] if _groups[index] is Dictionary else {}
		if _search >= 0 and _spawned_at(group, _search) <= 0:
			continue
		if not _filter_type.is_empty() and str(group.get("type", "")) != _filter_type:
			continue
		any = true
		_list.add_child(_group_card(group, index))
	if _groups.is_empty():
		_list.add_child(MindWidgets.label(_t("@waves.none")))
	elif not any:
		_list.add_child(MindWidgets.label(_t("@none.found")))
	if _graph != null:
		_graph.call("refresh")


## `groups.sort(Structs.comps(comparingFloat(sort.sort), comparingFloat(sort.secondary)))`.
func _sort_groups() -> void:
	# `type.id` order, read once per sort (not per comparison).
	_type_order = _unit_names()
	_groups.sort_custom(func(a: Variant, b: Variant) -> bool:
		var group_a: Dictionary = a if a is Dictionary else {}
		var group_b: Dictionary = b if b is Dictionary else {}
		var key_a := _sort_key(group_a, _sort)
		var key_b := _sort_key(group_b, _sort)
		if key_a == key_b:
			# `Sort.secondary`: `begin` ties break on `type.id`, the others
			# on `begin` (`WaveInfoDialog.java:432-447`).
			if _sort == Sort.BEGIN:
				return _sort_key(group_a, Sort.TYPE) < _sort_key(group_b, Sort.TYPE)
			return float(group_a.get("begin", 0)) < float(group_b.get("begin", 0))
		return key_a < key_b)
	if _reverse:
		_groups.reverse()


func _sort_key(group: Dictionary, sort_value: int) -> float:
	match sort_value:
		Sort.HEALTH:
			var name := str(group.get("type", ""))
			var health: Dictionary = _unit_health()
			return float(health.get(name, 0.0))
		Sort.TYPE:
			var order: Array = _type_order if not _type_order.is_empty() else _unit_names()
			return float(order.find(str(group.get("type", ""))))
		_:
			return float(group.get("begin", 0))


## `SpawnGroup.getSpawned(wave)`: whether the group spawns on `wave`.
func _spawned_at(group: Dictionary, wave: int) -> int:
	var begin := int(group.get("begin", 0))
	var end := int(group.get("end", NEVER))
	var spacing := maxi(1, int(group.get("spacing", 1)))
	if wave < begin or wave > end:
		return 0
	if (wave - begin) % spacing != 0:
		return 0
	return maxi(1, int(group.get("amount", 1)))


## One group card (`table.table(Tex.button, t -> ...)`).
func _group_card(group: Dictionary, index: int) -> Control:
	# `table.table(Tex.button, t -> t.button(b -> ..., () -> expand))`
	# (`WaveInfoDialog.java:170-206`): the whole header row is the expand
	# button; icon + accent name (ellipsis, 110) + spacer + light-gray wave +
	# the four 46px `emptyi` icon buttons.
	var card := PanelContainer.new()
	card.add_theme_stylebox_override("panel", MindThemeBuilder._button_nine_patch("button"))
	var box := VBoxContainer.new()
	card.add_child(box)
	var header := Button.new()
	header.theme_type_variation = "nonet"
	header.custom_minimum_size.y = 46.0 - 12.0
	header.pressed.connect(func() -> void:
		_expanded = -1 if _expanded == index else index
		_rebuild())
	box.add_child(header)
	var row := HBoxContainer.new()
	row.set_anchors_preset(Control.PRESET_FULL_RECT)
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_theme_constant_override("separation", 0)
	header.add_child(row)
	var type := str(group.get("type", "?"))
	var icon := TextureRect.new()
	icon.texture = MindWidgets.icon_texture("unit-%s-ui" % type)
	icon.custom_minimum_size = Vector2(32.0, 32.0)
	icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	icon.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(icon)
	var gap := Control.new()
	gap.custom_minimum_size.x = 3.0
	gap.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(gap)
	var name_label := MindWidgets.label(_unit_name(type))
	name_label.add_theme_color_override("font_color", MindStyles.ACCENT)
	name_label.custom_minimum_size.x = 110.0
	name_label.clip_contents = true
	name_label.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	name_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(name_label)
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(spacer)
	var begin := MindWidgets.label(str(int(group.get("begin", 0)) + 1))
	begin.add_theme_color_override("font_color", Color.LIGHT_GRAY)
	begin.custom_minimum_size.x = 45.0
	begin.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	begin.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(begin)
	row.add_child(_small_icon("copy", "@editor.copy", func() -> void: _copy_group(index)))
	var effect := str(group.get("effect", ""))
	row.add_child(_small_icon("logic" if effect.is_empty() else "warn",
		effect if not effect.is_empty() else _t("@none"),
		func() -> void: _show_effect_picker(index)))
	row.add_child(_small_icon("units", "@stat.unittype", func() -> void: _show_unit_picker(index)))
	row.add_child(_small_icon("cancel", "@waves.remove", func() -> void: _remove_group(index)))
	if _expanded == index:
		box.add_child(_group_fields(group, index))
	return card


## `group.type.localizedName` (`unit.<name>.name`).
func _unit_name(type: String) -> String:
	var key := "@unit.%s.name" % type
	var value := _t(key)
	return type if value == key or value.is_empty() else value


func _group_fields(group: Dictionary, index: int) -> Control:
	var fields := VBoxContainer.new()
	# `@waves.to` range + begin (`t.field("" + (group.begin + 1))`).
	var range_row := HBoxContainer.new()
	range_row.add_child(_int_field(str(int(group.get("begin", 0)) + 1), 100.0, func(value: int) -> void:
		group["begin"] = value - 1
		_commit()))
	range_row.add_child(MindWidgets.label(_t("@waves.to")))
	var end := int(group.get("end", NEVER))
	range_row.add_child(_int_field("" if end == NEVER else str(end + 1), 100.0, func(value: int) -> void:
		group["end"] = value - 1 if value > 0 else NEVER
		_commit()))
	fields.add_child(range_row)
	# `@waves.every ... @waves.waves`.
	var spacing_row := HBoxContainer.new()
	spacing_row.add_child(MindWidgets.label(_t("@waves.every")))
	spacing_row.add_child(_int_field(str(int(group.get("spacing", 1))), 100.0, func(value: int) -> void:
		if value > 0:
			group["spacing"] = value
			_commit()))
	spacing_row.add_child(MindWidgets.label(_t("@waves.waves")))
	fields.add_child(spacing_row)
	# amount + scaling (`1f / unitScaling` shown, Java's fixed(2)).
	var amount_row := HBoxContainer.new()
	amount_row.add_child(_int_field(str(int(group.get("amount", 1))), 80.0, func(value: int) -> void:
		group["amount"] = value
		_commit()))
	var scaling := float(group.get("scaling", NEVER))
	var per_spawn := 0.0 if scaling <= 0.0 else 1.0 / scaling
	amount_row.add_child(MindWidgets.label(" + "))
	amount_row.add_child(_float_field("%.2f" % per_spawn, 80.0, func(value: float) -> void:
		if value > 0.0:
			group["scaling"] = 1.0 / value
			_commit()))
	amount_row.add_child(MindWidgets.label(_t("@waves.perspawn")))
	fields.add_child(amount_row)
	# `@waves.max`.
	var max_row := HBoxContainer.new()
	max_row.add_child(_int_field(str(int(group.get("max", 40))), 80.0, func(value: int) -> void:
		group["max"] = value
		_commit()))
	max_row.add_child(MindWidgets.label(_t("@waves.max")))
	fields.add_child(max_row)
	# `@waves.shields` + shieldScaling.
	var shield_row := HBoxContainer.new()
	shield_row.add_child(_int_field(str(int(group.get("shields", 0))), 80.0, func(value: int) -> void:
		group["shields"] = value
		_commit()))
	shield_row.add_child(MindWidgets.label(" + "))
	shield_row.add_child(_int_field(str(int(group.get("shieldScaling", 0))), 80.0, func(value: int) -> void:
		group["shieldScaling"] = value
		_commit()))
	shield_row.add_child(MindWidgets.label(_t("@waves.shields")))
	fields.add_child(shield_row)
	# `@waves.guardian` (`group.effect == StatusEffects.boss`).
	fields.add_child(MindWidgets.check(_t("@waves.guardian"), effect_is_boss(group), func(pressed: bool) -> void:
		if pressed:
			group["effect"] = "boss"
		else:
			group.erase("effect")
		_commit()
		_rebuild()))
	# `@waves.team` row (`MapObjectivesDialog.showTeamSelect`).
	var team_row := HBoxContainer.new()
	team_row.add_child(MindWidgets.label(_t("@waves.team")))
	var team_button := MindWidgets.button(_team_label(group))
	team_button.pressed.connect(func() -> void: _show_team_picker(index))
	team_row.add_child(team_button)
	fields.add_child(team_row)
	# `@waves.spawn` row (packed `Point2` position).
	var spawn_row := HBoxContainer.new()
	spawn_row.add_child(MindWidgets.label(_t("@waves.spawn")))
	var spawn_button := MindWidgets.button(_spawn_label(group))
	spawn_button.pressed.connect(func() -> void: _show_spawn_picker(index))
	spawn_row.add_child(spawn_button)
	fields.add_child(spawn_row)
	return fields


func _effect_is_boss(group: Dictionary) -> bool:
	return str(group.get("effect", "")) == "boss"


## Java `effect_is_boss` helper (`group.effect == StatusEffects.boss`).
func effect_is_boss(group: Dictionary) -> bool:
	return _effect_is_boss(group)


func _small_icon(icon: String, tooltip: String, handler: Callable) -> Button:
	var button := MindWidgets.icon_button(icon, "", "emptyi")
	# `size(46f)` with `pad(-6)`: a 34 px slot; `Icon.*Small` glyphs draw at
	# the small icon size (~20 px), not the 30 px default.
	button.custom_minimum_size = Vector2(34.0, 34.0)
	button.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	for child in button.find_children("*", "Label", true, false):
		(child as Label).add_theme_font_size_override("font_size", 20)
	button.tooltip_text = tooltip
	button.pressed.connect(handler)
	return button


func _int_field(value: String, width: float, setter: Callable) -> LineEdit:
	var field := MindWidgets.field("")
	field.text = value
	field.max_length = 10
	field.custom_minimum_size.x = width
	field.text_changed.connect(func(text: String) -> void:
		if text.is_valid_int():
			setter.call(int(text)))
	return field


func _float_field(value: String, width: float, setter: Callable) -> LineEdit:
	var field := MindWidgets.field("")
	field.text = value
	field.max_length = 10
	field.custom_minimum_size.x = width
	field.text_changed.connect(func(text: String) -> void:
		if text.is_valid_float():
			setter.call(float(text)))
	return field


## `groups.insert(groups.indexOf(group) + 1, group.copy())`.
func _copy_group(index: int) -> void:
	if index < 0 or index >= _groups.size():
		return
	var copy: Variant = _groups[index].duplicate(true) if _groups[index] is Dictionary else {}
	_groups.insert(index + 1, copy)
	_expanded = index + 1
	_commit()
	_rebuild()


func _remove_group(index: int) -> void:
	if index < 0 or index >= _groups.size():
		return
	_groups.remove_at(index)
	_expanded = -1
	_commit()
	_rebuild()


## `showUnits(cons, reset)`: the unit picker (`filter`/add/unit swap).
func _show_unit_filter() -> void:
	_open_picker("@waves.filter", _unit_names(), "@settings.resetKey", "filter")


func _show_unit_add() -> void:
	_open_picker("", _unit_names(), "", "add")


func _show_unit_picker(index: int) -> void:
	_open_picker("", _unit_names(), "", "unit:%d" % index)


## `showEffects(group)`: the status-effect picker.
func _show_effect_picker(index: int) -> void:
	_open_picker("", _effect_names(), "@settings.resetKey", "effect:%d" % index)


## `MapObjectivesDialog.showTeamSelect(true, ...)`: the base-team picker.
func _show_team_picker(index: int) -> void:
	var names: Array = []
	for row_value in _editor.call("palette_teams"):
		var team_row: Dictionary = row_value
		names.append(str(team_row.get("name", "")))
	_open_picker("", names, "@settings.resetKey", "team:%d" % index)


## The spawn picker (`spawner.getSpawns()`: first 20 packed positions + all).
func _show_spawn_picker(index: int) -> void:
	var parsed: Variant = JSON.parse_string(str(_editor.call("spawn_points_json")))
	var names: Array = []
	if parsed is Array:
		for packed in parsed:
			if names.size() >= SPAWN_CAP:
				break
			var value := int(packed)
			var x := (value << 16) >> 16
			var y := value >> 16
			names.append("%d, %d" % [x, y])
	_open_picker("@waves.spawn.select", names, "@waves.spawn.all", "spawn:%d" % index)


func _open_picker(title: String, items: Array, reset_key: String, target: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("open_dialog"):
		return
	_picker_target = target
	# The sibling dialog registry may not be bound yet when `_ready` runs;
	# connect lazily on every open.
	_connect_picker()
	ui.call("open_dialog", "list_select", JSON.stringify({
		"title": title,
		"items": items,
		"reset": reset_key,
	}))


func _connect_picker() -> void:
	var picker := _dialog("list_select")
	if picker == null or not picker.has_signal("selected"):
		return
	if not picker.is_connected("selected", _on_picked):
		picker.connect("selected", _on_picked)


func _on_picked(name: String) -> void:
	var target := _picker_target
	_picker_target = ""
	var parts := target.split(":")
	match parts[0]:
		"filter":
			_filter_type = name
			_rebuild()
		"add":
			if not name.is_empty():
				_groups.append({"type": name})
				_expanded = _groups.size() - 1
				_commit()
				_rebuild()
		"unit":
			var unit_index := int(parts[1]) if parts.size() > 1 else -1
			if unit_index >= 0 and unit_index < _groups.size() and not name.is_empty():
				_groups[unit_index]["type"] = name
				_commit()
				_rebuild()
		"effect":
			var effect_index := int(parts[1]) if parts.size() > 1 else -1
			if effect_index >= 0 and effect_index < _groups.size():
				if name.is_empty():
					_groups[effect_index].erase("effect")
				else:
					_groups[effect_index]["effect"] = name
				_commit()
				_rebuild()
		"spawn":
			var spawn_index := int(parts[1]) if parts.size() > 1 else -1
			if spawn_index >= 0 and spawn_index < _groups.size():
				if name.is_empty():
					_groups[spawn_index]["spawn"] = -1
				else:
					var coords := name.split(",")
					if coords.size() == 2:
						var x := int(coords[0].strip_edges())
						var y := int(coords[1].strip_edges())
						_groups[spawn_index]["spawn"] = ((y << 16) | (x & 0xFFFF))
				_commit()
				_rebuild()
		"team":
			var team_index := int(parts[1]) if parts.size() > 1 else -1
			if team_index >= 0 and team_index < _groups.size():
				if name.is_empty():
					_groups[team_index].erase("team")
				else:
					_groups[team_index]["team"] = _team_id(name)
				_commit()
				_rebuild()
		_:
			pass


func _team_id(name: String) -> int:
	var index := 0
	for row_value in _editor.call("palette_teams"):
		var team_row: Dictionary = row_value
		if str(team_row.get("name", "")) == name:
			return index
		index += 1
	return -1


func _team_label(group: Dictionary) -> String:
	if not group.has("team"):
		return _t("@none")
	var id := int(group.get("team", -1))
	var index := 0
	for row_value in _editor.call("palette_teams"):
		var team_row: Dictionary = row_value
		if index == id:
			return str(team_row.get("name", ""))
		index += 1
	return str(id)


func _spawn_label(group: Dictionary) -> String:
	var packed := int(group.get("spawn", -1))
	if packed == -1:
		return _t("@waves.spawn.all")
	var x := (packed << 16) >> 16
	var y := packed >> 16
	return "%d, %d" % [x, y]


func _unit_names() -> Array:
	var parsed: Variant = JSON.parse_string(str(_editor.call("wave_units_json")))
	var names: Array = []
	if parsed is Array:
		for row_value in parsed:
			var row: Dictionary = row_value
			names.append(str(row.get("name", "")))
	return names


func _unit_health() -> Dictionary:
	var parsed: Variant = JSON.parse_string(str(_editor.call("wave_units_json")))
	var health: Dictionary = {}
	if parsed is Array:
		for row_value in parsed:
			var row: Dictionary = row_value
			health[str(row.get("name", ""))] = float(row.get("health", 0.0))
	return health


func _effect_names() -> Array:
	var parsed: Variant = JSON.parse_string(str(_editor.call("wave_effects_json")))
	return parsed if parsed is Array else []


func _read_groups() -> Array:
	if _editor == null:
		return []
	var parsed: Variant = JSON.parse_string(str(_editor.call("waves_json")))
	if parsed is Array and not (parsed as Array).is_empty():
		return parsed
	# `state.rules.spawns.isEmpty() ? waves.get()`: the vanilla table shows when
	# the map carries no spawns tag.
	var fallback: Variant = JSON.parse_string(str(_editor.call("default_waves_json")))
	return fallback if fallback is Array else []


## Writes the working list back (`hidden(() -> state.rules.spawns = groups)`,
## applied per edit so the tag and the graph stay live).
func _commit() -> void:
	if _editor != null and _editor.has_method("set_waves_json"):
		_editor.call("set_waves_json", JSON.stringify(_groups))
	if _graph != null:
		_graph.call("refresh")


func _dialog(name: String) -> Node:
	var ui_root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui_root == null or not ui_root.has_method("dialog"):
		return null
	return ui_root.call("dialog", name)


func _notify(key: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_info"):
		ui.call("show_info", _t(key))


func _notify_info(text: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_info"):
		ui.call("show_info", text)
