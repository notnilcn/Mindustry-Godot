## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/DatabaseDialog.java (plan 14 §3.4, M3).
##
## Core Database: search field, planet tabs (all/Serpulo/Erekir) and the
## `databaseCategory`/`databaseTag` content grid over the live
## `MindCampaign.database_json` model. Locked entries render the lock glyph;
## unlocked entries open `ContentInfoDialog`.

extends MindDialog

## `Planets.sun` content name (`DatabaseDialog.tab`'s all-content default).
const ALL_TAB := "sun"

var _tabs: HBoxContainer = null
var _grid: VBoxContainer = null
var _search: LineEdit = null
var _view: Dictionary = {}
var _tab := ALL_TAB


func _ready() -> void:
	set_title_key("@database")
	should_pause = true
	super._ready()
	_build()
	add_close_button()


func shown() -> void:
	_load_view()


func _build() -> void:
	var table := content_table()

	_search = MindWidgets.field(_t("@search"))
	_search.text_changed.connect(func(_value: String) -> void: _rebuild())
	table.add(_search).grow_x_axis().pad(4)
	table.row()

	# code-instantiated: the tab rail is the data-driven `checkTabList` planets.
	_tabs = HBoxContainer.new()
	_tabs.alignment = BoxContainer.ALIGNMENT_CENTER
	table.add(_tabs).grow_x_axis().pad(4)
	table.row()

	# code-instantiated: the category/tag grid is live content, not scene state.
	_grid = VBoxContainer.new()
	_grid.add_theme_constant_override("separation", 4)
	_grid.size_flags_vertical = Control.SIZE_EXPAND_FILL
	table.add(_grid).grow_x_axis().grow_y_axis().pad(4)
	_load_view()


## Reads the live database model from `MindCampaign` (settings-backed unlock
## state) and repaints the tab rail + grid.
func _load_view() -> void:
	_view = campaign_json("database_json")
	if _view.is_empty():
		_view = {"tabs": [], "categories": []}
	var tabs: Array = _view.get("tabs", [])
	if not tabs.is_empty():
		var names: Array = tabs.map(func(tab: Dictionary) -> String: return str(tab.get("name", "")))
		if not names.has(_tab):
			_tab = str((tabs[0] as Dictionary).get("name", ALL_TAB))
	_build_tabs()
	_rebuild()


func _build_tabs() -> void:
	if _tabs == null:
		return
	for child in _tabs.get_children():
		_tabs.remove_child(child)
		child.queue_free()
	# code-instantiated: one tab button per planet in the database model.
	var group := ButtonGroup.new()
	var tabs: Array = _view.get("tabs", [])
	for index in tabs.size():
		var tab: Dictionary = tabs[index]
		var name := str(tab.get("name", ""))
		var is_all := bool(tab.get("is_all", false))
		var label := _tab_label(tab, is_all)
		var button := Button.new()
		button.name = "tab-%s" % name
		button.theme_type_variation = "flatTogglet"
		button.text = label
		button.tooltip_text = label
		button.toggle_mode = true
		button.button_group = group
		button.button_pressed = name == _tab
		button.custom_minimum_size = Vector2(90, 40)
		if not is_all:
			button.add_theme_color_override("font_color", _hex_color(str(tab.get("color", ""))))
		button.pressed.connect(_select_tab.bind(name))
		_tabs.add_child(button)


func _select_tab(name: String) -> void:
	_tab = name
	_build_tabs()
	_rebuild()


func _rebuild() -> void:
	if _grid == null:
		return
	for child in _grid.get_children():
		_grid.remove_child(child)
		child.queue_free()
	var query := ""
	if _search != null:
		query = _search.text.to_lower()
	for category_variant in _view.get("categories", []):
		var category: Dictionary = category_variant
		var rows := _tag_rows(category, query)
		if rows.is_empty():
			continue
		_grid.add_child(
			MindWidgets.styled_label(_t("@%s" % str(category.get("label", ""))), "techLabel")
		)
		_grid.add_child(_divider(MindStyles.ACCENT))
		for row_variant in rows:
			var row: Dictionary = row_variant
			var tag_name := str(row.get("name", ""))
			if tag_name != "default":
				var tag_label := MindWidgets.label(_t("@%s" % str(row.get("label", ""))))
				tag_label.modulate = Color(0.5, 0.5, 0.5)
				_grid.add_child(tag_label)
				_grid.add_child(_divider(Color(0.5, 0.5, 0.5)))
			# code-instantiated: one flow row per `databaseTag` grid section.
			var flow := HFlowContainer.new()
			flow.name = "grid-%s-%s" % [str(category.get("name", "")), tag_name]
			flow.add_theme_constant_override("h_separation", 4)
			flow.add_theme_constant_override("v_separation", 4)
			flow.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			for entry_variant in row.get("entries", []):
				flow.add_child(_entry_button(entry_variant))
			_grid.add_child(flow)


## Non-empty tag rows for the active tab and search query (`DatabaseDialog`
## filters category/tag entries before dropping empty groups).
func _tag_rows(category: Dictionary, query: String) -> Array:
	var rows: Array = []
	for tag_variant in category.get("tags", []):
		var tag: Dictionary = tag_variant
		var entries: Array = []
		for entry_variant in tag.get("entries", []):
			var entry: Dictionary = entry_variant
			if not _entry_in_tab(entry):
				continue
			if not query.is_empty() and not _entry_matches(entry, query):
				continue
			entries.append(entry)
		if not entries.is_empty():
			rows.append({
				"name": str(tag.get("name", "")),
				"label": str(tag.get("label", "")),
				"entries": entries,
			})
	return rows


## `tab == Planets.sun || allDatabaseTabs || databaseTabs.contains(tab)`.
func _entry_in_tab(entry: Dictionary) -> bool:
	if _tab == ALL_TAB or bool(entry.get("all_tabs", false)):
		return true
	return (entry.get("tabs", []) as Array).has(_tab)


func _entry_matches(entry: Dictionary, query: String) -> bool:
	if str(entry.get("name", "")).to_lower().contains(query):
		return true
	return _entry_label(entry).to_lower().contains(query)


## One grid cell: the content icon when unlocked (`ui.content.show`), else the
## gray lock glyph (`new Image(Icon.lock, Pal.gray)`).
func _entry_button(entry_variant: Variant) -> Button:
	var entry: Dictionary = entry_variant
	var entry_name := str(entry.get("name", ""))
	var unlocked := bool(entry.get("unlocked", false))
	var button: Button
	if unlocked:
		var type_name := str(entry.get("content_type", ""))
		button = MindWidgets.image_button_first(
			PackedStringArray(
				[
					"%s-%s-ui" % [type_name, entry_name],
					"%s-%s-full" % [type_name, entry_name],
					entry_name,
				]
			),
			"selecti"
		)
		button.expand_icon = true
		button.add_theme_constant_override("icon_max_width", 32)
		if button.icon == null:
			button.text = _entry_label(entry)
		button.pressed.connect(open_content_info.bind(entry_name))
	else:
		button = Button.new()
		button.theme_type_variation = "selecti"
		var lock := MindWidgets.glyph("lock", 24, Color(0.5, 0.5, 0.5))
		lock.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		lock.mouse_filter = Control.MOUSE_FILTER_IGNORE
		button.add_child(lock)
	button.name = "content-%s" % entry_name
	button.custom_minimum_size = Vector2(46, 46)
	button.set_meta("unlocked", unlocked)
	if unlocked:
		button.tooltip_text = _entry_label(entry)
	return button


## Section divider (`all.image().height(3)` with the given color).
func _divider(color: Color) -> ColorRect:
	# code-instantiated: one divider per runtime section row.
	var line := ColorRect.new()
	line.color = color
	line.custom_minimum_size = Vector2(0, 3)
	line.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	return line


## Tab label: `@all` for the sun tab, else the localized planet name.
func _tab_label(tab: Dictionary, is_all: bool) -> String:
	if is_all:
		return _t("@all")
	return _bundle(str(tab.get("localized", "")), str(tab.get("name", "")))


## Entry label: bundle key when the model sent one (`<type>.<name>.name`).
func _entry_label(entry: Dictionary) -> String:
	return _bundle(str(entry.get("localized", "")), str(entry.get("name", "")))


## Bundle lookup with the key echoed back when assets are absent (`Bundle.get`).
func _bundle(key: String, fallback: String) -> String:
	if key.is_empty():
		return fallback
	var assets := MindWidgets.assets()
	if assets == null:
		return fallback
	var value := str(assets.call("bundle_get", key))
	if value.is_empty() or value == key:
		return fallback
	return value


## `rrggbb` planet icon color -> Godot color (white when absent/malformed).
func _hex_color(value: String) -> Color:
	if value.length() != 6 or not value.is_valid_hex_number(false):
		return Color.WHITE
	return Color(value)
