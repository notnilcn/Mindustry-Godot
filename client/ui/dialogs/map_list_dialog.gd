## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/MapListDialog.java
##         core/src/mindustry/game/Gamemode.java
##         core/src/mindustry/ui/BorderImage.java
##
## Shared map browser behind `CustomGameDialog` (`displayType = false`) and
## `EditorMapsDialog` (`displayType = true`). `setup` builds a fixed search row
## (zoom glyph, field, filter button) over the scrolling map grid; the button
## row holds `@back` (210x64) plus the subclass `buildButtons`. `rebuildMaps`
## gathers custom/built-in/modded maps, sorts them by the persisted
## `MapViewSettings.priority` (default `recent`), drops maps filtered out by
## gamemode/search, and lays out `Core.graphics.getWidth() / 230` cards per
## row. Each card (`Styles.grayt`, width 200, margin 9, bottom-aligned) is the
## icon row of every valid `Gamemode` with a `mode<Name>Small` icon, the
## ellipsized name, a `Pal.gray` rule, the fitted 180 px preview and (when
## `displayType`) the map source label.
##
## Subclasses set the title and `display_type` before `super._ready()`, and
## override `_build_buttons` / `_show_map`.

extends MindDialog

## `MapListDialog.settings` persistence key (`Core.settings.getJson`).
const SETTINGS_KEY := "editormapviewsettings"
## `rebuildMaps`: `float mapsize = 200f`.
const MAP_SIZE := 200.0
## `Core.graphics.getWidth() / Scl.scl(230)` columns.
const COLUMN_WIDTH := 230.0
## `button.margin(9)`.
const CARD_MARGIN := 9.0
## `Gamemode.all` in declaration order (`Gamemode.java:9-43`).
const GAMEMODES := ["survival", "sandbox", "attack", "pvp", "editor"]
## Gamemodes whose `mode<Name>` / `mode<Name>Small` icon exists in the atlas
## (`assets-raw/icons/mode-{survival,attack,pvp}.png`); `MapListDialog` skips
## modes whose icon `Core.atlas.isFound` rejects (sandbox, editor).
const MODE_ICONS := {"survival": "mode-survival", "attack": "mode-attack", "pvp": "mode-pvp"}
## `MapPriority.all` in declaration order with the filter-dialog icons
## (`Icon.fileText, Icon.players, Icon.hammer, Icon.play`).
const PRIORITIES := ["name", "custom", "builtin", "recent"]
const PRIORITY_ICONS := ["file-text", "players", "hammer", "play"]
## `Pal.gray`.
const PAL_GRAY := Color(0.271, 0.271, 0.271)
## `Pal.darkestGray` (`Styles.grayPanel`, the `grayt` up state).
const PAL_DARKEST_GRAY := Color(0.1, 0.1, 0.1)

## `MapListDialog.displayType`: show the custom/built-in source label.
var display_type := false

## `MapViewSettings` defaults (`MapListDialog.java:58-75`).
var _settings := {
	"showBuiltIn": true,
	"showCustom": true,
	"showModded": true,
	"searchAuthor": false,
	"searchDescription": false,
	"searchModname": false,
	"priority": "recent",
}
## `MapListDialog.modes`: the gamemode filter (instance state, not persisted).
var _mode_filter: Array[String] = []
## `searchString` (lowercased; empty = null).
var _search := ""
var _field: LineEdit = null
var _grid: MindTable = null
var _filter_panel: Control = null
var _priority_buttons: Array[Button] = []
## `MapViewSettings` flag -> its filter toggle (re-synced on settings reload).
var _setting_buttons: Dictionary = {}
## Per-map `Gamemode.valid` cache (`MindPreview.map_modes`), keyed by path.
var _modes_cache: Dictionary = {}
## Coalesces `previews_changed` batches into one grid rebuild per frame.
var _preview_refresh_queued := false
## `Strings.stripColors` tag matcher (built on first search).
var _color_tags: RegEx = null


func _ready() -> void:
	should_pause = false
	full_dialog = true
	super._ready()
	_load_settings()
	_build()


## `shown(this::setup)`: reload the registry, reset the search and rebuild.
func shown() -> void:
	_load_settings()
	_refresh_registry()
	# `setup`: `searchString = null` and a fresh (empty) search field.
	_search = ""
	if _field != null:
		_field.text = ""
	_rebuild()
	# `MapListDialog.show`: desktop focuses the search field
	# (`Core.scene.setKeyboardFocus(searchField)`, MapListDialog.java:367-369), so
	# the field shows a caret instead of its `@editor.search` message text.
	if _field != null:
		_field.grab_focus.call_deferred()


## `buildButtons()` hook (`EditorMapsDialog` adds New Map / Import Map).
func _build_buttons() -> void:
	pass


## `showMap(map)` hook.
func _show_map(_map: Dictionary) -> void:
	pass


## Rows used when the live registry is empty (subclass fallback).
func _fallback_rows() -> Array:
	return []


func _build() -> void:
	_build_search()
	# code-instantiated: tile count and layout are driven by the Maps registry.
	_grid = MindTable.new()
	_grid.mouse_filter = Control.MOUSE_FILTER_PASS
	var root := content_table()
	# `cont.add(pane).padLeft(28f)` + `mapTable.marginRight(12f)`; the table is
	# centred in the pane (`Table` default alignment), so expand (not fill).
	root.add(_grid).expand_x_axis().set_pad_left(28.0).set_pad_right(12.0)

	clear_buttons()
	# `setup`: `buttons.button("@back", Icon.left, this::hide).size(210f, 64f)`
	# first, then `buildButtons()`.
	add_pane_button(_t("@back"), "left", _close_pressed)
	_build_buttons()
	_rebuild()


## `search.image(Icon.zoom)`, `search.field(...).maxTextLength(50).growX()`,
## `search.button(Icon.filter, Styles.emptyi, this::showMapFilters)`. The row
## sits above the scrolling pane (`cont.add(search).growX(); cont.row();
## cont.add(pane)`), so it is placed outside the scroll body.
func _build_search() -> void:
	# code-instantiated: the search row is a fixed composite (zoom glyph + field + filter).
	var search := HBoxContainer.new()
	search.name = "Search"
	search.add_theme_constant_override("separation", 6)
	search.add_child(MindWidgets.glyph("zoom", 24))
	_field = MindWidgets.field(_t("@editor.search"))
	_field.max_length = 50
	_field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_field.text_changed.connect(func(text: String) -> void:
		_search = text.to_lower()
		_rebuild())
	search.add_child(_field)
	var filter_button := MindWidgets.icon_button("filter", "", "emptyi")
	filter_button.tooltip_text = _t("@editor.filters")
	filter_button.custom_minimum_size = Vector2(40.0, 40.0)
	filter_button.pressed.connect(_toggle_filters)
	search.add_child(filter_button)
	_filter_panel = _build_filter_panel()
	_filter_panel.visible = false
	var layout := get_node_or_null("Center/Panel/Layout") as Container
	if layout != null and body != null:
		layout.add_child(search)
		layout.move_child(search, body.get_index())
		layout.add_child(_filter_panel)
		layout.move_child(_filter_panel, body.get_index())
	else:
		var root := content_table()
		root.add(search).grow_x_axis()
		root.row()
		root.add(_filter_panel).grow_x_axis()


# --- Filters (`showMapFilters`) ------------------------------------------------


## `showMapFilters`: gamemode + priority toggles, the map type toggles and the
## search-in toggles. Upstream opens them in a `BaseDialog`; the port keeps
## them in an inline panel under the search row. The planet filter is not
## ported (the registry rows carry no `rules.planet`).
func _build_filter_panel() -> Control:
	# code-instantiated: the filter panel is a fixed upstream toggle set.
	var panel := PanelContainer.new()
	panel.name = "Filters"
	panel.theme_type_variation = "smallPane"
	var menu := VBoxContainer.new()
	menu.add_theme_constant_override("separation", 6)
	panel.add_child(menu)

	var tabs := HBoxContainer.new()
	tabs.alignment = BoxContainer.ALIGNMENT_CENTER
	tabs.add_theme_constant_override("separation", 10)
	menu.add_child(tabs)

	# `t.add("@editor.filters.mode")` + one `Styles.emptyTogglei` 60 px toggle
	# per gamemode with an icon.
	var mode_box := _filter_box(tabs, "@editor.filters.mode")
	for mode in GAMEMODES:
		if not MODE_ICONS.has(mode):
			continue
		var mode_button := MindWidgets.glyph_button(str(MODE_ICONS[mode]), _t("@mode.%s.name" % mode), "emptyTogglei")
		mode_button.toggle_mode = true
		mode_button.custom_minimum_size = Vector2(60.0, 60.0)
		mode_button.button_pressed = _mode_filter.has(mode)
		mode_button.tooltip_text = _t("@mode.%s.name" % mode)
		mode_button.toggled.connect(func(pressed: bool) -> void:
			if pressed and not _mode_filter.has(mode):
				_mode_filter.append(mode)
			elif not pressed:
				_mode_filter.erase(mode)
			_rebuild())
		mode_box.add_child(mode_button)

	# `t.add("@editor.filters.priorities")`: one checked-when-current toggle per
	# `MapPriority`.
	var priority_box := _filter_box(tabs, "@editor.filters.priorities")
	_priority_buttons.clear()
	for index in PRIORITIES.size():
		var priority: String = PRIORITIES[index]
		var priority_button := MindWidgets.glyph_button(PRIORITY_ICONS[index], priority, "emptyTogglei")
		priority_button.toggle_mode = true
		priority_button.custom_minimum_size = Vector2(60.0, 60.0)
		priority_button.tooltip_text = _t("@editor.filters.prioritize.%s" % priority)
		priority_button.pressed.connect(func() -> void:
			_settings["priority"] = priority
			_sync_priority_buttons()
			_save_settings()
			_rebuild())
		priority_box.add_child(priority_button)
		_priority_buttons.append(priority_button)
	_sync_priority_buttons()

	# `menu.add("@editor.filters.type")` + custom/builtin/modded (150x60).
	var type_box := _filter_box(menu, "@editor.filters.type")
	_add_setting_toggle(type_box, "@custom", "showCustom")
	_add_setting_toggle(type_box, "@builtin", "showBuiltIn")
	_add_setting_toggle(type_box, "@modded", "showModded")
	# `menu.add("@editor.filters.search")` + author/description/modname.
	var search_box := _filter_box(menu, "@editor.filters.search")
	_add_setting_toggle(search_box, "@editor.filters.author", "searchAuthor")
	_add_setting_toggle(search_box, "@editor.filters.description", "searchDescription")
	_add_setting_toggle(search_box, "@editor.filters.modname", "searchModname")
	return panel


## A titled `Tex.button` box (`t.add(title).row(); t.table(Tex.button, ...)`);
## returns the row the toggles go into.
func _filter_box(parent: Container, title_key: String) -> HBoxContainer:
	# code-instantiated: filter group boxes are part of the fixed filter panel.
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 6)
	column.add_child(MindWidgets.label(_t(title_key)))
	var box := PanelContainer.new()
	box.add_theme_stylebox_override("panel", _pane_box(Color.BLACK, PANE_OUTLINE_GRAY))
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 0)
	box.add_child(row)
	column.add_child(box)
	parent.add_child(column)
	return row


## A `Styles.flatTogglet` 150x60 toggle bound to one `MapViewSettings` flag.
func _add_setting_toggle(row: HBoxContainer, key: String, setting: String) -> void:
	# code-instantiated: filter toggles are data-driven (one per settings flag).
	var button := MindWidgets.button(_t(key))
	button.theme_type_variation = "flatTogglet"
	button.toggle_mode = true
	button.custom_minimum_size = Vector2(150.0, 60.0)
	button.button_pressed = bool(_settings.get(setting, false))
	button.toggled.connect(func(pressed: bool) -> void:
		_settings[setting] = pressed
		_save_settings()
		_rebuild())
	row.add_child(button)
	_setting_buttons[setting] = button


func _sync_priority_buttons() -> void:
	for index in _priority_buttons.size():
		_priority_buttons[index].set_pressed_no_signal(PRIORITIES[index] == str(_settings.get("priority", "recent")))


func _toggle_filters() -> void:
	if _filter_panel != null:
		_filter_panel.visible = not _filter_panel.visible


## `Core.settings.getJson("editormapviewsettings", ...)`: shared by both map
## dialogs, so it is re-read on every `setup`.
func _load_settings() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("settings_get_text"):
		return
	var text := str(ui.call("settings_get_text", SETTINGS_KEY, ""))
	if text.strip_edges().is_empty():
		return
	var parsed: Variant = JSON.parse_string(text)
	if not (parsed is Dictionary):
		return
	var stored: Dictionary = parsed
	for key in _settings.keys():
		if stored.has(key):
			_settings[key] = stored[key]
	if not PRIORITIES.has(str(_settings.get("priority", ""))):
		_settings["priority"] = "recent"
	_sync_priority_buttons()
	for setting in _setting_buttons.keys():
		var toggle: Button = _setting_buttons[setting]
		toggle.set_pressed_no_signal(bool(_settings.get(setting, false)))


## `MapViewSettings.save` (`activeDialog.hidden(settings::save)`).
func _save_settings() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("settings_put_text"):
		ui.call("settings_put_text", SETTINGS_KEY, JSON.stringify(_settings))


# --- Registry -----------------------------------------------------------------


## Reloads the live `MindPreview` registry (editor-saved custom maps) and asks
## it for the missing previews (`Maps.loadPreviews` + `createAllPreviews`);
## batches land through `previews_changed`.
func _refresh_registry() -> void:
	_modes_cache.clear()
	var preview := _preview_node()
	if preview == null:
		return
	if preview.has_method("refresh"):
		preview.call("refresh")
	_connect_previews(preview)
	if preview.has_method("generate_previews"):
		preview.call("generate_previews")


func _preview_node() -> Node:
	return get_node_or_null("/root/Spine/MindPreview")


func _connect_previews(preview: Node) -> void:
	if not preview.has_signal("previews_changed"):
		return
	if not preview.is_connected("previews_changed", Callable(self, "_on_previews_changed")):
		preview.connect("previews_changed", Callable(self, "_on_previews_changed"))


## A generated preview batch landed (`Maps.createAllPreviews`); its scan facts
## (`Map.spawns`/`teams`) now exist, so drop the mode cache and refresh the
## tiles once per frame while batches keep arriving.
func _on_previews_changed() -> void:
	if _preview_refresh_queued:
		return
	_preview_refresh_queued = true
	call_deferred("_rebuild_after_previews")


func _rebuild_after_previews() -> void:
	_preview_refresh_queued = false
	_modes_cache.clear()
	_rebuild()


## The live registry rows (`MindPreview.maps_list`), else the subclass fallback.
func _maps() -> Array:
	var rows: Array = []
	var preview := _preview_node()
	if preview != null and preview.has_method("maps_list"):
		var maps: Variant = preview.call("maps_list")
		if maps is Array:
			for map_variant in maps:
				var map: Dictionary = map_variant
				if not str(map.get("name", "")).is_empty():
					rows.append(map)
	if rows.is_empty():
		for row_variant in _fallback_rows():
			var row: Dictionary = row_variant
			if not str(row.get("name", "")).is_empty():
				rows.append(row)
	return rows


## `maps.customMaps()` + `defaultMaps()` + `moddedMaps()` (each list filtered
## by its `MapViewSettings` flag), `distinct()`, then
## `sort(settings.priority.comparator)`.
func _sorted_maps() -> Array:
	var customs: Array = []
	var builtins: Array = []
	var modded: Array = []
	for row_variant in _maps():
		var row: Dictionary = row_variant
		if bool(row.get("custom", false)):
			customs.append(row)
		elif bool(row.get("mod", false)):
			modded.append(row)
		else:
			builtins.append(row)
	var entries: Array = []
	if bool(_settings.get("showCustom", true)):
		entries.append_array(customs)
	if bool(_settings.get("showBuiltIn", true)):
		entries.append_array(builtins)
	if bool(_settings.get("showModded", true)):
		entries.append_array(modded)
	# `Seq.sort` is stable (TimSort); carry the list index as the final key.
	var keyed: Array = []
	for index in entries.size():
		var row: Dictionary = entries[index]
		keyed.append({
			"row": row,
			"index": index,
			"name": str(row.get("name", "")),
			"custom": bool(row.get("custom", false)),
			"modified": _last_modified(row),
		})
	var priority := str(_settings.get("priority", "recent"))
	keyed.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		return _compare(priority, a, b) < 0)
	var out: Array = []
	for entry in keyed:
		out.append(entry["row"])
	return out


## `MapPriority` comparators (`MapListDialog.java:44-50`); a negative result
## orders `a` first. `String.compareTo` is a code-point compare.
static func _compare(priority: String, a: Dictionary, b: Dictionary) -> int:
	var result := 0
	match priority:
		"name":
			result = _cmp_str(a["name"], b["name"])
		"custom":
			# `comparingBool(m -> !m.custom)` then `comparingLong(-lastModified)`.
			result = _cmp_int(0 if a["custom"] else 1, 0 if b["custom"] else 1)
			if result == 0:
				result = _cmp_int(-int(a["modified"]), -int(b["modified"]))
		"builtin":
			result = _cmp_int(1 if a["custom"] else 0, 1 if b["custom"] else 0)
			if result == 0:
				result = _cmp_int(-int(a["modified"]), -int(b["modified"]))
		_:
			# `recent`: `comparingLong(-lastModified)` then `comparing(Map::name)`.
			result = _cmp_int(-int(a["modified"]), -int(b["modified"]))
			if result == 0:
				result = _cmp_str(a["name"], b["name"])
	if result == 0:
		result = _cmp_int(int(a["index"]), int(b["index"]))
	return result


static func _cmp_int(a: int, b: int) -> int:
	return -1 if a < b else (1 if a > b else 0)


static func _cmp_str(a: String, b: String) -> int:
	return -1 if a < b else (1 if a > b else 0)


## `map.file.lastModified()` in milliseconds (`MapPriority.recent`'s primary
## key; Java truncates the sub-millisecond digits, `MapListDialog.java:48`).
## Built-in maps report 0 like Java's classpath-backed `Fi`s; the value comes
## from the registry row (`MindPreview.maps_list`), not a second-precision
## `FileAccess` read.
static func _last_modified(row: Dictionary) -> int:
	return int(row.get("lastModified", 0))


## `Gamemode.valid(map)` (`Gamemode.java`): survival needs `spawns > 0`,
## attack/pvp need `teams.size > 1`, sandbox/editor are always valid. The
## spawn/team facts come from the preview scan (`MindPreview.map_modes`); a
## map whose preview has not been generated yet is not scanned synchronously
## here (the batch that writes it triggers a rebuild).
func _map_modes(map: Dictionary) -> Dictionary:
	var key := str(map.get("path", map.get("name", "")))
	if _modes_cache.has(key):
		return _modes_cache[key]
	var modes := {}
	var preview := _preview_node()
	if preview != null and preview.has_method("map_modes") and bool(map.get("preview", true)):
		var value: Variant = preview.call("map_modes", str(map.get("name", "")))
		if value is Dictionary:
			modes = value
	_modes_cache[key] = modes
	return modes


func _mode_valid(map: Dictionary, mode: String) -> bool:
	if mode == "sandbox" or mode == "editor":
		return true
	return bool(_map_modes(map).get(mode, false))


# --- Grid (`rebuildMaps`) -------------------------------------------------------


func _rebuild() -> void:
	if _grid == null:
		return
	_grid.clear_children()
	var maxwidth := maxi(1, int(get_viewport_rect().size.x / COLUMN_WIDTH))
	var index := 0
	var shown_any := false
	for map_variant in _sorted_maps():
		var map: Dictionary = map_variant
		if not _passes_filter(map):
			continue
		shown_any = true
		if index > 0 and index % maxwidth == 0:
			_grid.row()
		# `mapTable.button("", Styles.grayt, ...).width(mapsize).bottom().pad(8)`
		_grid.add(_build_map_tile(map)).width(MAP_SIZE).bottom().pad(8)
		index += 1
	if not shown_any:
		_grid.add(MindWidgets.label(_tm("@maps.none"))).pad(8)


## The gamemode filter (`invalid |= !mode.valid(map)`) and the search over
## `plainName` (+ author/description/mod name per `MapViewSettings`).
func _passes_filter(map: Dictionary) -> bool:
	for mode in _mode_filter:
		if not _mode_valid(map, mode):
			return false
	if _search.is_empty():
		return true
	if _plain(str(map.get("name", ""))).contains(_search):
		return true
	if bool(_settings.get("searchAuthor", false)) and _plain(str(map.get("author", ""))).contains(_search):
		return true
	if bool(_settings.get("searchDescription", false)) and _plain(str(map.get("description", ""))).contains(_search):
		return true
	if bool(_settings.get("searchModname", false)) and _plain(str(map.get("mod_name", ""))).contains(_search):
		return true
	return false


## `Strings.stripColors(text).toLowerCase()` (`Map.plainName`): drops `[]`,
## `[#rrggbb]` and lowercase colour-name tags.
func _plain(text: String) -> String:
	if _color_tags == null:
		_color_tags = RegEx.create_from_string("\\[(#[0-9a-fA-F]{3,8}|[a-z]*)\\]")
	return _color_tags.sub(text, "", true).to_lower()


## One `rebuildMaps` card (`MapListDialog.java:165-205`).
func _build_map_tile(map: Dictionary) -> Button:
	var map_name := str(map.get("name", ""))
	# code-instantiated: map tiles are data-driven from the Maps registry.
	var tile := MindWidgets.button("")
	tile.theme_type_variation = "grayt"
	_apply_grayt(tile)
	tile.custom_minimum_size = Vector2(MAP_SIZE, 0.0)
	tile.clip_contents = true
	tile.pressed.connect(func() -> void: _show_map(map))

	# code-instantiated: `button.margin(9)` inset around the card column.
	var inset := MarginContainer.new()
	inset.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	inset.mouse_filter = Control.MOUSE_FILTER_IGNORE
	for side in ["left", "right", "top", "bottom"]:
		inset.add_theme_constant_override("margin_" + side, int(CARD_MARGIN))
	tile.add_child(inset)

	# code-instantiated: the card column (`button.bottom()`: content at the bottom).
	var column := VBoxContainer.new()
	column.alignment = BoxContainer.ALIGNMENT_END
	column.mouse_filter = Control.MOUSE_FILTER_IGNORE
	column.add_theme_constant_override("separation", 0)
	inset.add_child(column)

	column.add_child(_build_mode_row(map))

	# `button.add(map.name()).width(mapsize - 18f).center().get().setEllipsis(true)`
	# code-instantiated: the name needs native ellipsis trimming the rich label lacks.
	var name_label := Label.new()
	name_label.text = map_name
	name_label.clip_text = true
	name_label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	name_label.custom_minimum_size.x = MAP_SIZE - 18.0
	name_label.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	name_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	column.add_child(name_label)

	# `button.image().growX().pad(4).color(Pal.gray)`: the 3 px `whiteui` rule.
	# code-instantiated: divider between the map name and the preview.
	var rule_pad := MarginContainer.new()
	rule_pad.mouse_filter = Control.MOUSE_FILTER_IGNORE
	for side in ["left", "right", "top", "bottom"]:
		rule_pad.add_theme_constant_override("margin_" + side, 4)
	var rule := ColorRect.new()
	rule.color = PAL_GRAY
	rule.custom_minimum_size = Vector2(0.0, 3.0)
	rule.mouse_filter = Control.MOUSE_FILTER_IGNORE
	rule_pad.add_child(rule)
	column.add_child(rule_pad)

	# `button.stack(new Image(map.safeTexture()).setScaling(Scaling.fit),
	# new BorderImage(map.safeTexture()).setScaling(Scaling.fit)).size(mapsize - 20f)`.
	column.add_child(_build_preview(map_name, MAP_SIZE - 20.0, 4.0))

	if display_type:
		# `button.add(map.custom ? "@custom" : map.workshop ? "@workshop" :
		# map.mod != null ? "[lightgray]" + modName : "@builtin").color(Color.gray).padTop(3f)`
		# code-instantiated: the source label row is data-driven per map.
		var type_label := Label.new()
		type_label.text = _source_label(map)
		type_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		type_label.add_theme_color_override("font_color", Color.GRAY)
		type_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
		# code-instantiated: `padTop(3f)` spacer above the source label.
		var spacer := Control.new()
		spacer.custom_minimum_size = Vector2(0.0, 3.0)
		spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
		column.add_child(spacer)
		column.add_child(type_label)

	# A `Button` is not a container: size the card to its column once the
	# theme fonts resolve (the card height is the content height, like Arc's
	# `TextButton` pref size).
	tile.ready.connect(func() -> void:
		tile.custom_minimum_size.y = inset.get_combined_minimum_size().y)
	return tile


## `button.table(t -> { t.left(); for(Gamemode mode : Gamemode.all) ... })`:
## a 16 px icon (pad 4) per valid mode whose icon exists, or one empty 16 px
## cell when none.
func _build_mode_row(map: Dictionary) -> Control:
	# code-instantiated: the mode-icon row reflects the map's valid Gamemodes.
	var pad := MarginContainer.new()
	pad.mouse_filter = Control.MOUSE_FILTER_IGNORE
	pad.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	for side in ["left", "right", "top", "bottom"]:
		pad.add_theme_constant_override("margin_" + side, 4)
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 8)
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	pad.add_child(row)
	for mode in GAMEMODES:
		if not MODE_ICONS.has(mode) or not _mode_valid(map, mode):
			continue
		row.add_child(_icon_cell(str(MODE_ICONS[mode]), 16.0))
	if row.get_child_count() == 0:
		# code-instantiated: `t.add().size(16f).pad(4f)` placeholder cell.
		var empty := Control.new()
		empty.custom_minimum_size = Vector2(16.0, 16.0)
		empty.mouse_filter = Control.MOUSE_FILTER_IGNORE
		row.add_child(empty)
	return pad


## A fixed-size icon cell (`t.image(icon).size(16f)`) holding an icon-font glyph.
static func _icon_cell(icon_name: String, extent: float) -> Control:
	# code-instantiated: icon cells are data-driven (one per valid gamemode).
	var cell := Control.new()
	cell.custom_minimum_size = Vector2(extent, extent)
	cell.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var glyph := MindWidgets.glyph(icon_name, int(extent))
	glyph.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	glyph.grow_horizontal = Control.GROW_DIRECTION_BOTH
	glyph.grow_vertical = Control.GROW_DIRECTION_BOTH
	glyph.mouse_filter = Control.MOUSE_FILTER_IGNORE
	cell.add_child(glyph)
	return cell


## The fitted map preview (`Scaling.fit` inside a `size x size` cell with a
## `BorderImage` stroke), or the `nomap` placeholder until the batched
## generation lands (`map.safeTexture()`).
func _build_preview(map_name: String, extent: float, thickness: float) -> Control:
	var texture := _preview_texture(map_name)
	if texture != null:
		# code-instantiated: the preview texture comes from the map registry.
		var image := MindBorderImage.new()
		image.texture = texture
		image.thickness = thickness
		image.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		image.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		image.custom_minimum_size = Vector2(extent, extent)
		image.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
		image.mouse_filter = Control.MOUSE_FILTER_IGNORE
		return image
	# code-instantiated: the `nomap` placeholder until map textures exist.
	var placeholder := PanelContainer.new()
	placeholder.theme_type_variation = "smallPane"
	placeholder.custom_minimum_size = Vector2(extent, extent)
	placeholder.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	placeholder.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var glyph := MindWidgets.glyph("map", 48)
	glyph.mouse_filter = Control.MOUSE_FILTER_IGNORE
	placeholder.add_child(glyph)
	return placeholder


## `map.safeTexture()` (`MapPreviewLoader` + the cached `<previews>/<stem>_v2.png`;
## the port bumps the name to `_v3` to regenerate pre-T3 colour caches).
func _preview_texture(map_name: String) -> Texture2D:
	var preview := _preview_node()
	if preview == null or not preview.has_method("texture_for_name"):
		return null
	var value: Variant = preview.call("texture_for_name", map_name)
	return value if value is Texture2D else null


func _source_label(map: Dictionary) -> String:
	if bool(map.get("custom", false)):
		return _t("@custom")
	if bool(map.get("workshop", false)):
		return _t("@workshop")
	if bool(map.get("mod", false)):
		return str(map.get("mod_name", ""))
	return _t("@builtin")


## `Styles.grayt` (`Styles.java:157-165`): `up = grayPanel` (`Pal.darkestGray`),
## `over`/`down = flatOver` (#454545), `disabled = grayPanelDark`.
static func _apply_grayt(button: Button) -> void:
	button.add_theme_stylebox_override("normal", _solid(PAL_DARKEST_GRAY))
	button.add_theme_stylebox_override("hover", _solid(PAL_GRAY))
	button.add_theme_stylebox_override("pressed", _solid(PAL_GRAY))
	button.add_theme_stylebox_override("hover_pressed", _solid(PAL_GRAY))
	button.add_theme_stylebox_override("focus", StyleBoxEmpty.new())


static func _solid(color: Color) -> StyleBoxFlat:
	var box := StyleBoxFlat.new()
	box.bg_color = color
	return box


## `buttons.button(text, icon, listener).size(210f, 64f)` over the default
## `Tex.button` pane (`BaseDialog.addCloseButton` look).
func add_pane_button(text_value: String, icon_name: String, listener: Callable) -> Button:
	# code-instantiated: the dialog's button row is data-driven per dialog.
	var button := MindWidgets.button("")
	button.custom_minimum_size = Vector2(210.0, 64.0)
	apply_pane_button(button, PANE_OUTLINE_GRAY)
	build_button_content(button, MindWidgets.glyph(icon_name, 24), text_value)
	if listener.is_valid():
		button.pressed.connect(listener)
	if buttons != null:
		buttons.add_child(button)
	return button
