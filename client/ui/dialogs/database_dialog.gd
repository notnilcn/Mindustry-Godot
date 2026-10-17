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
## `Vars.iconMed` (grid cell size).
const ICON_MED := 32.0
## `Pal.gray` (#454545): tag headers and lock icons.
const PAL_GRAY := Color(0x45 / 255.0, 0x45 / 255.0, 0x45 / 255.0, 1.0)

var _tabs: GridContainer = null
var _grid: VBoxContainer = null
var _search: LineEdit = null
var _view: Dictionary = {}
var _tab := ALL_TAB


func _ready() -> void:
	# `cont.top()` (`DatabaseDialog.java:57`): the body sticks to the top (L01 T2).
	cont_top = true
	set_title_key("@database")
	should_pause = true
	super._ready()
	# Java `BaseDialog`: the `cont.pane(all)` grid fills the viewport.
	fill_panel()
	_build()
	add_close_button()
	# `onResize(this::rebuild)`: the grid column count follows the width.
	var viewport := get_viewport()
	if viewport != null and not viewport.size_changed.is_connected(_rebuild):
		viewport.size_changed.connect(_rebuild)


func shown() -> void:
	_load_view()


func _build() -> void:
	var table := content_table()
	# `cont.top()` with the pane at its natural width: the search row
	# (`fillX`) spans the grid's width rather than the whole viewport.
	table.size_flags_horizontal = Control.SIZE_SHRINK_CENTER

	# `cont.table(s -> { s.image(Icon.zoom).padRight(8); search = s.field(..).growX();
	# search.setMessageText("@players.search") }).fillX().padBottom(4)`.
	# code-instantiated: the search row is part of the dialog body built in code.
	var search_row := HBoxContainer.new()
	search_row.add_theme_constant_override("separation", 8)
	search_row.add_child(MindWidgets.glyph("zoom", 22))
	_search = MindWidgets.field(_t("@players.search"))
	_search.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_search.text_changed.connect(func(_value: String) -> void: _rebuild())
	search_row.add_child(_search)
	table.add(search_row).grow_x_axis().set_pad_bottom(4)
	table.row()

	# code-instantiated: the tab rail is the data-driven `checkTabList` planets
	# (`t.button(..., Styles.clearNoneTogglei, iconMed, ...).size(50f)`, 10 per
	# row).
	_tabs = GridContainer.new()
	_tabs.columns = 10
	_tabs.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	_tabs.add_theme_constant_override("h_separation", 4)
	table.add(_tabs).pad(4)
	table.row()

	# `all.margin(20).marginTop(0f).marginRight(30f)` inside `cont.pane(all)`:
	# the grid keeps its natural width (`cols` icon cells) and sits centred
	# instead of stretching across the viewport.
	# code-instantiated: the category/tag grid is live content, not scene state.
	var margin := MarginContainer.new()
	margin.add_theme_constant_override("margin_left", 20)
	margin.add_theme_constant_override("margin_right", 30)
	margin.add_theme_constant_override("margin_top", 0)
	margin.add_theme_constant_override("margin_bottom", 20)
	margin.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	_grid = VBoxContainer.new()
	_grid.add_theme_constant_override("separation", 0)
	_grid.size_flags_vertical = Control.SIZE_EXPAND_FILL
	margin.add_child(_grid)
	table.add(margin).grow_y_axis()
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
		# `DatabaseDialog.shown`: in a campaign the active tab follows the
		# played planet (`state.getPlanet()`) when the tab list has it.
		var current_planet := ""
		var campaign := campaign_node()
		if campaign != null and campaign.has_method("get_sector_state"):
			var state: Dictionary = campaign.call("get_sector_state")
			if bool(state.get("campaign", false)):
				current_planet = str(state.get("planet", ""))
		if not current_planet.is_empty() and names.has(current_planet):
			_tab = current_planet
		elif not names.has(_tab):
			_tab = str((tabs[0] as Dictionary).get("name", ALL_TAB))
	_build_tabs()
	_rebuild()


func _build_tabs() -> void:
	if _tabs == null:
		return
	for child in _tabs.get_children():
		_tabs.remove_child(child)
		child.queue_free()
	# code-instantiated: one 50 px icon toggle per planet tab — `Icon.eyeSmall`
	# for the sun, else the planet's icon tinted `p.iconColor`
	# (`t.button(..., Styles.clearNoneTogglei, iconMed, ...)`).
	var group := ButtonGroup.new()
	var tabs: Array = _view.get("tabs", [])
	for index in tabs.size():
		var tab: Dictionary = tabs[index]
		var name := str(tab.get("name", ""))
		var is_all := bool(tab.get("is_all", false))
		var label := _tab_label(tab, is_all)
		var color := Color.WHITE if is_all else _hex_color(str(tab.get("color", "")))
		var icon_name := str(tab.get("icon", ""))
		var button := Button.new()
		button.name = "tab-%s" % name
		button.theme_type_variation = "clearNoneTogglei"
		button.tooltip_text = label
		button.toggle_mode = true
		button.button_group = group
		button.button_pressed = name == _tab
		button.custom_minimum_size = Vector2(50, 50)
		_apply_clear_none_toggle(button)
		if MindIcons.glyph(icon_name).is_empty():
			button.text = label
		else:
			var glyph := MindWidgets.glyph(icon_name, 32, color)
			glyph.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
			glyph.mouse_filter = Control.MOUSE_FILTER_IGNORE
			button.add_child(glyph)
		button.pressed.connect(_select_tab.bind(name))
		_tabs.add_child(button)


## `Styles.clearNoneTogglei` (`Styles.java:351-356`): `up = none`,
## `over = flatOver` (#454545), `down`/`checked = flatDown` (the
## `flat-down-base` 9-patch: black centre in a 4 px `Pal.accent` frame), so the
## selected tab carries the accent border.
func _apply_clear_none_toggle(button: Button) -> void:
	var over := StyleBoxFlat.new()
	over.bg_color = Color(0x45 / 255.0, 0x45 / 255.0, 0x45 / 255.0, 1.0)
	var checked := StyleBoxFlat.new()
	checked.bg_color = Color.BLACK
	checked.border_color = MindStyles.ACCENT
	checked.set_border_width_all(4)
	button.add_theme_stylebox_override("normal", StyleBoxEmpty.new())
	button.add_theme_stylebox_override("hover", over)
	button.add_theme_stylebox_override("pressed", checked)
	button.add_theme_stylebox_override("hover_pressed", checked)
	button.add_theme_stylebox_override("focus", StyleBoxEmpty.new())


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
	# `cols = clamp((graphics.getWidth() - Scl.scl(30)) / Scl.scl(32 + 12), 1, 22)`.
	var cols := clampi(int((get_viewport_rect().size.x - 30.0) / 44.0), 1, 22)
	var has_result := false
	for category_variant in _view.get("categories", []):
		var category: Dictionary = category_variant
		var rows := _tag_rows(category, query)
		if rows.is_empty():
			continue
		has_result = true
		# `all.add("@database-category." + name).growX().left().color(Pal.accent)`.
		var category_label := MindWidgets.label(_t("@%s" % str(category.get("label", ""))))
		category_label.modulate = MindStyles.ACCENT
		_grid.add_child(category_label)
		# `all.image().pad(5).padLeft(0).padRight(0).height(3).color(Pal.accent).growX()`.
		_grid.add_child(_padded(_divider(MindStyles.ACCENT), 0, 5, 0, 5))
		# `all.table(sub -> ...).growX().left().padBottom(10)`.
		# code-instantiated: one section per category with live tag rows.
		var sub := VBoxContainer.new()
		sub.add_theme_constant_override("separation", 0)
		for row_variant in rows:
			var row: Dictionary = row_variant
			var tag_name := str(row.get("name", ""))
			if tag_name != "default":
				# `sub.table(tag -> { tag.add("@database-tag." + name).left().color(Pal.gray);
				# tag.image().growX().pad(5).height(3).color(Pal.gray) }).pad(4, 8, 4, 8).growX()`.
				# code-instantiated: tag header rows follow the live tag list.
				var header := HBoxContainer.new()
				header.add_theme_constant_override("separation", 0)
				var tag_label := MindWidgets.label(_t("@%s" % str(row.get("label", ""))))
				tag_label.modulate = PAL_GRAY
				tag_label.autowrap_mode = TextServer.AUTOWRAP_OFF
				tag_label.fit_content = true
				header.add_child(tag_label)
				var line := _padded(_divider(PAL_GRAY), 5, 5, 5, 5)
				line.size_flags_horizontal = Control.SIZE_EXPAND_FILL
				line.size_flags_vertical = Control.SIZE_SHRINK_CENTER
				header.add_child(line)
				sub.add_child(_padded(header, 8, 4, 8, 4))
			# `sub.table(list -> ...)`: `cols` cells of `iconMed` padded 3, the
			# row topped up with clear cells (`for k < cols - count`).
			# code-instantiated: one grid per `databaseTag` section.
			var list := GridContainer.new()
			list.name = "grid-%s-%s" % [str(category.get("name", "")), tag_name]
			list.columns = cols
			list.add_theme_constant_override("h_separation", 6)
			list.add_theme_constant_override("v_separation", 6)
			var entries: Array = row.get("entries", [])
			for entry_variant in entries:
				list.add_child(_entry_button(entry_variant))
			for _k in maxi(cols - entries.size(), 0):
				# code-instantiated: clear filler cells keep the grid `cols` wide.
				var filler := Control.new()
				filler.custom_minimum_size = Vector2(ICON_MED, ICON_MED)
				filler.mouse_filter = Control.MOUSE_FILTER_IGNORE
				list.add_child(filler)
			sub.add_child(_padded(list, 3, 3, 3, 3))
		_grid.add_child(_padded(sub, 0, 0, 0, 10))
	if not has_result:
		_grid.add_child(MindWidgets.label(_t("@none.found")))


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
		button.add_theme_constant_override("icon_max_width", int(ICON_MED))
		# `new Image(uiIcon, Color.lightGray)` lerping to white while hovered.
		button.add_theme_color_override("icon_normal_color", Color.LIGHT_GRAY)
		button.add_theme_color_override("icon_hover_color", Color.WHITE)
		button.add_theme_color_override("icon_pressed_color", Color.WHITE)
		button.add_theme_color_override("icon_hover_pressed_color", Color.WHITE)
		button.add_theme_color_override("icon_focus_color", Color.LIGHT_GRAY)
		button.add_theme_stylebox_override("normal", StyleBoxEmpty.new())
		button.add_theme_stylebox_override("hover", StyleBoxEmpty.new())
		button.add_theme_stylebox_override("pressed", StyleBoxEmpty.new())
		button.add_theme_stylebox_override("hover_pressed", StyleBoxEmpty.new())
		button.add_theme_stylebox_override("focus", StyleBoxEmpty.new())
		if button.icon == null:
			button.text = _entry_label(entry)
		button.pressed.connect(open_content_info.bind(entry_name))
	else:
		button = Button.new()
		button.theme_type_variation = "selecti"
		# `new Image(Icon.lock, Pal.gray)`.
		var lock := MindWidgets.glyph("lock", int(ICON_MED * 0.75), PAL_GRAY)
		lock.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		lock.mouse_filter = Control.MOUSE_FILTER_IGNORE
		button.add_child(lock)
	if bool(entry.get("banned", false)):
		# `new Image(Icon.cancel, Tmp.c1.set(Color.scarlet).a(0.6f))` over the
		# content icon when in a game with the block banned.
		var scarlet := Color("ff341c")
		scarlet.a = 0.6
		var cross := MindWidgets.glyph("cancel", 32, scarlet)
		cross.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		cross.mouse_filter = Control.MOUSE_FILTER_IGNORE
		button.add_child(cross)
	button.name = "content-%s" % entry_name
	# `.size(iconMed).pad(3)` (the pad is the grid separation).
	button.custom_minimum_size = Vector2(ICON_MED, ICON_MED)
	button.set_meta("unlocked", unlocked)
	if unlocked:
		button.tooltip_text = _entry_label(entry)
	return button


## Wraps `control` in a margin box (Arc cell pads: left, top, right, bottom).
func _padded(control: Control, left: int, top: int, right: int, bottom: int) -> MarginContainer:
	# code-instantiated: cell padding for a runtime section row.
	var box := MarginContainer.new()
	box.add_theme_constant_override("margin_left", left)
	box.add_theme_constant_override("margin_top", top)
	box.add_theme_constant_override("margin_right", right)
	box.add_theme_constant_override("margin_bottom", bottom)
	box.mouse_filter = Control.MOUSE_FILTER_IGNORE
	box.add_child(control)
	return box


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
