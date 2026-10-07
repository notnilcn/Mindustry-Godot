## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/CustomGameDialog.java
##         core/src/mindustry/ui/dialogs/MapListDialog.java
##
## Custom-game map browser (`MapListDialog`, `displayType = true`): a search row
## over a grid of map tiles. Each tile shows a mode-icon row, the map name, a
## divider, a preview placeholder and the map type. The rows come from the live
## `MindPreview` registry plus the read-model built-ins; choosing a map opens
## `map_play` and emits `map_chosen`.

extends MindDialog

## Emitted when a custom map is chosen.
signal map_chosen(name: String)

const TILE_SIZE := Vector2(200.0, 214.0)
const NAME_WIDTH := 182.0
const PREVIEW_SIZE := Vector2(180.0, 110.0)
const COLUMN_WIDTH := 230.0

var _column: VBoxContainer = null
var _field: LineEdit = null
var _grid: MindTable = null
var _filter_panel: Control = null
var _search := ""
var _show_custom := true
var _show_builtin := true
var _show_modded := true


func _ready() -> void:
	set_title_key("@customgame")
	should_pause = false
	full_dialog = true
	super._ready()
	_build()


## `MapListDialog.shown` rebuilds the map grid so refreshed maps appear.
func shown() -> void:
	_refresh_registry()
	_rebuild()


## Reloads the live `MindPreview` registry (editor-saved custom maps).
func _refresh_registry() -> void:
	var preview := get_node_or_null("/root/Spine/MindPreview")
	if preview != null and preview.has_method("refresh"):
		preview.call("refresh")


## Merges the live registry rows (`MindPreview.maps_list`) with the read-model
## built-ins, keeping the first row per name (custom maps win a name clash).
func _maps() -> Array:
	var rows: Array = []
	var seen := {}
	var preview := get_node_or_null("/root/Spine/MindPreview")
	if preview != null and preview.has_method("maps_list"):
		var maps: Variant = preview.call("maps_list")
		if maps is Array:
			for map_variant in maps:
				var map: Dictionary = map_variant
				var name := str(map.get("name", ""))
				if name.is_empty():
					continue
				rows.append(map)
				seen[name.to_lower()] = true
	for row_variant in campaign_section("maps"):
		var row: Dictionary = row_variant
		var name := str(row.get("name", ""))
		if name.is_empty() or seen.has(name.to_lower()):
			continue
		rows.append(row)
	return rows


func _build() -> void:
	var root := content_table()
	# code-instantiated: dialog body is a fixed column (search + filter panel + map grid).
	_column = VBoxContainer.new()
	_column.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_column.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_column.add_theme_constant_override("separation", 6)
	root.add(_column).grow_x_axis().grow_y_axis()

	_build_search()

	_filter_panel = _build_filter_panel()
	_filter_panel.visible = false
	_column.add_child(_filter_panel)

	# code-instantiated: tile count and layout are driven by the Maps registry.
	_grid = MindTable.new()
	_grid.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_grid.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_column.add_child(_grid)

	clear_buttons()
	add_button(_t("@back"), _back, "left", 210.0)
	_rebuild()


func _build_search() -> void:
	# code-instantiated: the search row is a fixed composite (zoom glyph + field + filter).
	var search := HBoxContainer.new()
	search.add_theme_constant_override("separation", 6)
	search.add_child(MindWidgets.glyph("zoom", 20))
	_field = MindWidgets.field(_t("@editor.search"))
	_field.max_length = 50
	_field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_field.text_changed.connect(func(text: String) -> void:
		_search = text.to_lower()
		_rebuild())
	search.add_child(_field)
	var filter_button := MindWidgets.icon_button("filter", "", "emptyi")
	filter_button.tooltip_text = _t("@editor.filters")
	filter_button.pressed.connect(_toggle_filters)
	search.add_child(filter_button)
	_column.add_child(search)


func _build_filter_panel() -> Control:
	# code-instantiated: the filter panel is a fixed custom/builtin/modded toggle set.
	var panel := PanelContainer.new()
	panel.theme_type_variation = "smallPane"
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 6)
	panel.add_child(row)
	_add_filter_toggle(row, "@custom", _show_custom, func(value: bool) -> void: _show_custom = value)
	_add_filter_toggle(row, "@builtin", _show_builtin, func(value: bool) -> void: _show_builtin = value)
	_add_filter_toggle(row, "@modded", _show_modded, func(value: bool) -> void: _show_modded = value)
	return panel


func _add_filter_toggle(row: HBoxContainer, key: String, initial: bool, setter: Callable) -> void:
	# code-instantiated: filter toggles are data-driven (one per filter key).
	var button := MindWidgets.button(_t(key))
	button.theme_type_variation = "flatTogglet"
	button.toggle_mode = true
	button.button_pressed = initial
	button.toggled.connect(func(pressed: bool) -> void:
		setter.call(pressed)
		_rebuild())
	row.add_child(button)


func _toggle_filters() -> void:
	if _filter_panel != null:
		_filter_panel.visible = not _filter_panel.visible


func _rebuild() -> void:
	if _grid == null:
		return
	_grid.clear_children()
	var maxwidth := maxi(1, int(get_viewport_rect().size.x / COLUMN_WIDTH))
	var index := 0
	var shown_any := false
	for map_variant in _maps():
		var map: Dictionary = map_variant
		if not _passes_filter(map):
			continue
		shown_any = true
		if index > 0 and index % maxwidth == 0:
			_grid.row()
		_grid.add(_build_map_tile(map)).pad(8)
		index += 1
	if not shown_any:
		_grid.add(MindWidgets.label(_tm("@maps.none"))).pad(8)
	# Grids nested in another container miss the deferred sort; place now and
	# again next frame once the VBox has assigned the grid its final size.
	_grid.sort_now()
	_grid.call_deferred("sort_now")


func _passes_filter(map: Dictionary) -> bool:
	var custom := bool(map.get("custom", false))
	if custom and not _show_custom:
		return false
	if not custom and not _show_builtin:
		return false
	if bool(map.get("mod", false)) and not _show_modded:
		return false
	if _search.is_empty():
		return true
	var name := str(map.get("name", "")).to_lower()
	var author := str(map.get("author", "")).to_lower()
	return name.contains(_search) or author.contains(_search)


func _build_map_tile(map: Dictionary) -> Button:
	# code-instantiated: map tiles are data-driven from the Maps registry.
	var map_name := str(map.get("name", ""))
	var tile := MindWidgets.button("")
	tile.theme_type_variation = "grayt"
	tile.custom_minimum_size = TILE_SIZE
	tile.clip_contents = true
	tile.pressed.connect(func() -> void: _choose_map(map_name))

	var column := VBoxContainer.new()
	column.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	column.alignment = BoxContainer.ALIGNMENT_CENTER
	column.mouse_filter = Control.MOUSE_FILTER_IGNORE
	column.add_theme_constant_override("separation", 4)
	tile.add_child(column)

	# code-instantiated: the mode-icon row reflects the map's valid Gamemodes.
	var modes := HBoxContainer.new()
	modes.alignment = BoxContainer.ALIGNMENT_CENTER
	modes.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var mode_glyph := MindWidgets.glyph("mode-survival", 16)
	mode_glyph.mouse_filter = Control.MOUSE_FILTER_IGNORE
	modes.add_child(mode_glyph)
	column.add_child(modes)

	# code-instantiated: the name needs native ellipsis trimming the rich label lacks.
	var name_label := Label.new()
	name_label.text = map_name
	name_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	name_label.clip_text = true
	name_label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	name_label.custom_minimum_size.x = NAME_WIDTH
	name_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	column.add_child(name_label)

	# code-instantiated: divider between the map name and the preview placeholder.
	var divider := HSeparator.new()
	divider.mouse_filter = Control.MOUSE_FILTER_IGNORE
	column.add_child(divider)

	# code-instantiated: the preview is a placeholder until map textures exist.
	var preview := PanelContainer.new()
	preview.theme_type_variation = "smallPane"
	preview.custom_minimum_size = PREVIEW_SIZE
	preview.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	preview.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var preview_glyph := MindWidgets.glyph("map", 48)
	preview_glyph.mouse_filter = Control.MOUSE_FILTER_IGNORE
	preview.add_child(preview_glyph)
	column.add_child(preview)

	var type_label := MindWidgets.label(_t("@custom") if bool(map.get("custom", false)) else _t("@builtin"))
	type_label.add_theme_color_override("default_color", Color.GRAY)
	type_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	type_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	column.add_child(type_label)

	return tile


func _choose_map(map_name: String) -> void:
	map_chosen.emit(map_name)
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("open_dialog"):
		ui.call("open_dialog", "map_play", JSON.stringify({"map": map_name}))


## Closes through `MindUi` so the dialog stack and pause governor stay in sync.
func _back() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("close_dialog"):
		ui.call("close_dialog", "custom")
	else:
		hide_dialog()
