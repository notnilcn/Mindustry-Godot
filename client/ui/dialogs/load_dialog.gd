## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/LoadDialog.java
##
## "Load Game" dialog: a search row (field + gamemode filter toggles), a grid of
## save cards, and an "Import Save" button. The save-slot listing is supplied
## through `_context.slots`; with no slots the grid renders the empty state
## (`@save.none`). Clicking a card body emits `slot_selected(name)` and closes.

extends MindDialog

## Emitted when a save card body is clicked (not one of its icon buttons).
signal slot_selected(slot: String)

## Gamemode filter toggles (`Gamemode.all`; sandbox uses the terrain glyph).
const FILTERS := [
	{"mode": "survival", "icon": "mode-survival"},
	{"mode": "attack", "icon": "mode-attack"},
	{"mode": "pvp", "icon": "mode-pvp"},
	{"mode": "sandbox", "icon": "terrain"},
]

var _field: LineEdit = null
var _grid: MindTable = null
## Gamemode names whose saves are hidden from the grid (`hidden` upstream).
var _hidden: Dictionary = {}


func _ready() -> void:
	set_title_key("@loadgame")
	should_pause = false
	full_dialog = true
	super._ready()
	_build()


func shown() -> void:
	# Upstream `shown` resets the search string and rebuilds the grid.
	if _field != null and not _field.text.is_empty():
		_field.text = ""
	_rebuild_list()


func _build() -> void:
	var root := content_table()

	# code-instantiated: the search row is data-driven from the gamemode list.
	var search_row := HBoxContainer.new()
	search_row.add_theme_constant_override("separation", 6)
	# code-instantiated: the zoom glyph is an icon-font run with no static scene.
	var zoom := MindWidgets.glyph("zoom", 22)
	search_row.add_child(zoom)
	_field = MindWidgets.field(_t("@save.search"))
	_field.max_length = 50
	_field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_field.text_changed.connect(func(_value: String) -> void: _rebuild_list())
	search_row.add_child(_field)
	_add_filters(search_row)
	root.add(search_row).grow_x_axis().pad(4)
	root.row()

	# code-instantiated: the save grid is a data-driven list of save slots.
	_grid = MindTable.new()
	root.add(_grid).grow_x_axis().grow_y_axis().pad(4)

	# Bottom row: Back (`left`) and `@save.import` (`add`).
	add_button(_t("@back"), _close, "left")
	add_button(_t("@save.import"), _open_import, "add")

	_rebuild_list()


func _add_filters(row: HBoxContainer) -> void:
	# code-instantiated: filter toggles track the local hidden-gamemode set.
	for filter in FILTERS:
		var mode := str(filter.get("mode", ""))
		var button := MindWidgets.icon_button(str(filter.get("icon", "")), "", "emptyTogglei")
		button.toggle_mode = true
		button.button_pressed = true
		button.tooltip_text = _t("@mode.%s.name" % mode)
		button.pressed.connect(_toggle_filter.bind(mode))
		row.add_child(button)


func _toggle_filter(mode: String) -> void:
	if _hidden.has(mode):
		_hidden.erase(mode)
	else:
		_hidden[mode] = true
	_rebuild_list()


func _rebuild_list() -> void:
	if _grid == null:
		return
	_grid.clear_children()
	var slots: Array = context().get("slots", [])
	var needle := _field.text.strip_edges().to_lower() if _field != null else ""
	var columns := maxi(int(get_viewport_rect().size.x / 470.0), 1)
	var any := false
	var count := 0
	for entry in slots:
		if not (entry is Dictionary):
			continue
		var slot: Dictionary = entry
		var name := str(slot.get("name", ""))
		if not needle.is_empty() and not name.to_lower().contains(needle):
			continue
		var mode := str(slot.get("mode", ""))
		if not mode.is_empty() and _hidden.has(mode):
			continue
		any = true
		_grid.add(_build_card(slot)).grow_x_axis().pad(4).set_min_width(300.0)
		count += 1
		if count % columns == 0:
			_grid.row()
	if not any:
		# code-instantiated: the empty-state text is a data-driven grid cell.
		var none := MindWidgets.label(_tm("@save.none"))
		none.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		_grid.add(none).grow_x_axis().grow_y_axis().pad(16).set_align(0)
	# Nested grids miss the deferred sort; place now and again next frame.
	_grid.sort_now()
	_grid.call_deferred("sort_now")


func _build_card(slot: Dictionary) -> Button:
	var name := str(slot.get("name", ""))
	# code-instantiated: save cards are data-driven from the save-slot listing.
	var card := MindWidgets.button("")
	card.theme_type_variation = "grayt"
	card.custom_minimum_size = Vector2(400, 160)
	card.pressed.connect(_select.bind(name))

	# code-instantiated: card body follows the slot data; ignores clicks so the
	# card button itself handles selection.
	var content := VBoxContainer.new()
	content.mouse_filter = Control.MOUSE_FILTER_IGNORE
	content.add_theme_constant_override("separation", 4)
	card.add_child(content)
	content.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)

	# code-instantiated: per-card title/action row (data-driven icon buttons).
	var title := HBoxContainer.new()
	title.mouse_filter = Control.MOUSE_FILTER_IGNORE
	content.add_child(title)
	var name_label := MindWidgets.styled_label(name, "techLabel")
	name_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	name_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	title.add_child(name_label)
	title.add_child(_icon_action("save", "emptyTogglei", bool(slot.get("autosave", false))))
	title.add_child(_icon_action("trash", "emptyi", false))
	title.add_child(_icon_action("pencil", "emptyi", false))
	title.add_child(_icon_action("export", "emptyi", false))

	# code-instantiated: preview/meta row is per-slot presentation data.
	var body := HBoxContainer.new()
	body.mouse_filter = Control.MOUSE_FILTER_IGNORE
	body.add_theme_constant_override("separation", 8)
	body.size_flags_vertical = Control.SIZE_EXPAND_FILL
	content.add_child(body)

	# code-instantiated: preview placeholder (no save texture is exposed yet).
	var preview := Panel.new()
	preview.mouse_filter = Control.MOUSE_FILTER_IGNORE
	preview.custom_minimum_size = Vector2(160, 100)
	body.add_child(preview)
	# code-instantiated: `nomap` placeholder glyph is an icon-font run.
	var map_icon := MindWidgets.glyph("map", 40)
	map_icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	preview.add_child(map_icon)
	map_icon.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)

	# code-instantiated: metadata column lines come from the slot dictionary.
	var meta := VBoxContainer.new()
	meta.mouse_filter = Control.MOUSE_FILTER_IGNORE
	meta.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	body.add_child(meta)
	for line in _meta_lines(slot):
		var label := MindWidgets.label(line)
		label.mouse_filter = Control.MOUSE_FILTER_IGNORE
		label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		meta.add_child(label)

	return card


func _icon_action(icon_name: String, style: String, is_pressed: bool) -> Button:
	# code-instantiated: card action buttons are per-slot (no static scene).
	var button := MindWidgets.icon_button(icon_name, "", style)
	button.custom_minimum_size = Vector2(34, 34)
	if style == "emptyTogglei":
		button.toggle_mode = true
		button.button_pressed = is_pressed
	return button


func _meta_lines(slot: Dictionary) -> Array:
	var map_name := str(slot.get("map", _t("@unknown")))
	var mode := str(slot.get("mode", ""))
	if not mode.is_empty():
		mode = _t("@mode.%s.name" % mode)
	var wave := int(slot.get("wave", 0))
	var autosave := bool(slot.get("autosave", false))
	var playtime := str(slot.get("playtime_ms", slot.get("playtime", "")))
	var date := str(slot.get("date", ""))
	var lines := [
		_fmt("@save.map", [map_name]),
		"%s /%s" % [mode, _fmt("@save.wave", [wave])],
		_fmt("@save.autosave", [_t("@on" if autosave else "@off")]),
		_fmt("@save.playtime", [playtime]),
	]
	if not date.is_empty():
		lines.append(date)
	return lines


func _select(slot: String) -> void:
	slot_selected.emit(slot)
	_close()


func _open_import() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	# Upstream `FileChooser.open("msav")`; the chooser owns the `.msav` flow.
	ui.call("open_dialog", "file_chooser", JSON.stringify({"open": true}))


func _close() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("close_dialog"):
		ui.call("close_dialog", "load")
	else:
		hide_dialog()


## Bundle lookup with `{0}`-style substitution (`Bundle.format`).
func _fmt(key: String, args: Array) -> String:
	var result := _t(key)
	for index in args.size():
		result = result.replace("{%d}" % index, str(args[index]))
	return result
