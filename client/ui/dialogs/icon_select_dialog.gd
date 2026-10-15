## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/IconSelectDialog.java (fill-parent, no
## title), core/src/mindustry/Vars.java (`accessibleIcons`,
## `defaultContentIcons`), core/src/mindustry/ui/dialogs/PlanetDialog.java
## :1223-1283 (the sector marker picker: none button, accessible icons, five
## content sections with a 3 px accent rule).
##
## Two modes. `icon` (default; schematics tags): the vanilla `accessibleIcons`
## grid plus content icons, selecting emits `icon_selected(region_name)`.
## `sector` (planet page): selecting emits
## `sector_icon_selected(icon, content_icon)` with the Java `Iconc` field name
## or a content name — the unused half is empty, both empty is `Icon.none`.

extends MindDialog

signal icon_selected(region_name: String)
signal sector_icon_selected(icon: String, content_icon: String)

## `Vars.accessibleIcons` — order and Java field names are the stored ABI
## (`SectorInfo.icon` keeps the field name; the display maps through
## `MindIcons`).
const ACCESSIBLE_ICONS := [
	"effect", "power", "logic", "units", "liquid", "production", "defense",
	"turret", "distribution", "crafting", "settings", "cancel", "zoom", "ok",
	"star", "home", "pencil", "up", "down", "left", "right", "hammer",
	"warning", "tree", "admin", "map", "modePvp", "terrain", "modeSurvival",
	"commandRally", "commandAttack",
]

## `Vars.defaultContentIcons`.
const CONTENT_TYPES := ["item", "liquid", "block", "unit", "status"]

## `min(20, Core.graphics.getWidth() / Scl.scl(52))` (1152 px viewport -> 20).
const COLUMNS := 20

var _mode := "icon"
var _checked_icon := ""
var _checked_content := ""
var _grid: MindTable = null
## `database_json` entries grouped by content type (content order per type).
var _rows: Dictionary = {}


func _ready() -> void:
	should_pause = false
	super._ready()
	add_close_button(210.0)


func shown() -> void:
	var ctx := context()
	_mode = str(ctx.get("mode", "icon"))
	_checked_icon = _opt(ctx, "icon")
	_checked_content = _opt(ctx, "content_icon")
	_load_rows()
	_rebuild()


## `MindCampaign.database_json` entries grouped by type: hidden content is
## already dropped by the model; locked entries are filtered at build time like
## `u.unlocked()` (`PlanetDialog.java:1263`).
func _load_rows() -> void:
	_rows = {}
	var view: Dictionary = campaign_json("database_json")
	for category_variant in view.get("categories", []):
		var category: Dictionary = category_variant
		for tag_variant in category.get("tags", []):
			var tag: Dictionary = tag_variant
			for entry_variant in tag.get("entries", []):
				var entry: Dictionary = entry_variant
				var type_name := str(entry.get("content_type", ""))
				if not CONTENT_TYPES.has(type_name):
					continue
				var list: Array = _rows.get(type_name, [])
				list.append(entry)
				_rows[type_name] = list


## `IconSelectDialog` grid: 48 px buttons, `cols = min(20, width/52)`, the none
## button, the accessible icons, then each `defaultContentIcons` type behind a
## 3 px accent rule. Java renders it in a `cont.pane`, so the grid scrolls.
func _rebuild() -> void:
	if _grid == null:
		var table := content_table()
		# code-instantiated: the icon grid is a scrolling pane sized by content
		# rows (`IconSelectDialog.java:26-31` `cont.pane`), not scene state.
		var scroll := MindScroll.new()
		scroll.custom_minimum_size = Vector2(1000.0, 420.0)
		scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
		_grid = MindTable.new()
		_grid.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		scroll.add_child(_grid)
		table.add(scroll).grow_x_axis().grow_y_axis().pad(4)
		table.row()
	_grid.clear_children()
	var sector_mode := _mode == "sector"
	var none_checked := _checked_icon.is_empty() and _checked_content.is_empty()
	var none_button: Button
	if sector_mode:
		none_button = _toggle_button(
			MindWidgets.glyph("none", 28), none_checked, func() -> void: _choose_sector("", "")
		)
	else:
		none_button = _toggle_button(
			MindWidgets.glyph("none", 28), none_checked, func() -> void: _choose_icon("")
		)
	_grid.add(none_button).size(48.0)

	var index := 1
	for icon_name in ACCESSIBLE_ICONS:
		var control := MindWidgets.glyph(icon_name, 28)
		var checked := str(icon_name) == _checked_icon
		var button: Button
		if sector_mode:
			button = _toggle_button(control, checked, func() -> void: _choose_sector(icon_name, ""))
		else:
			button = _toggle_button(control, checked, func() -> void: _choose_icon(icon_name))
		_grid.add(button).size(48.0)
		index += 1
		if index % COLUMNS == 0:
			_grid.row()

	for type_name in CONTENT_TYPES:
		_grid.row()
		var rule := ColorRect.new()
		rule.color = MindStyles.ACCENT
		rule.custom_minimum_size = Vector2(0.0, 3.0)
		rule.mouse_filter = Control.MOUSE_FILTER_IGNORE
		_grid.add(rule).grow_x_axis().set_colspan(COLUMNS)
		_grid.row()
		index = 0
		for entry_variant in _rows.get(type_name, []):
			var entry: Dictionary = entry_variant
			if not bool(entry.get("unlocked", false)):
				continue
			var entry_name := str(entry.get("name", ""))
			var entry_button := _content_button(entry, entry_name == _checked_content)
			_grid.add(entry_button).size(48.0)
			index += 1
			if index % COLUMNS == 0:
				_grid.row()


## One 48 px `squareTogglei` cell around the given content control
## (`PlanetDialog.java:1236-1256`).
func _toggle_button(content: Control, checked: bool, on_pressed: Callable) -> Button:
	# code-instantiated: one data-driven toggle per accessible icon.
	var button := Button.new()
	button.theme_type_variation = "squareTogglei"
	button.toggle_mode = true
	button.button_pressed = checked
	button.custom_minimum_size = Vector2(48.0, 48.0)
	content.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	content.mouse_filter = Control.MOUSE_FILTER_IGNORE
	button.add_child(content)
	button.pressed.connect(on_pressed)
	return button


## One content entry: `uiIcon` via the `<type>-<name>-ui` / `-full` region chain
## (`UnlockableContent.uiIcon`; `PlanetDialog.java:1265-1275`).
func _content_button(entry: Dictionary, checked: bool) -> Button:
	var entry_name := str(entry.get("name", ""))
	var type_name := str(entry.get("content_type", ""))
	# code-instantiated: one data-driven toggle per unlocked content entry.
	var button := MindWidgets.image_button_first(
		PackedStringArray(
			[
				"%s-%s-ui" % [type_name, entry_name],
				"%s-%s-full" % [type_name, entry_name],
				entry_name,
			]
		),
		"squareTogglei"
	)
	button.toggle_mode = true
	button.button_pressed = checked
	button.custom_minimum_size = Vector2(48.0, 48.0)
	button.expand_icon = true
	button.add_theme_constant_override("icon_max_width", 32)
	button.tooltip_text = _t("@%s.%s.name" % [type_name, entry_name])
	button.pressed.connect(func() -> void: _choose_sector("", entry_name))
	return button


## Vanilla mode (schematics tags): the picked accessible icon name.
func _choose_icon(icon_name: String) -> void:
	if _mode == "sector":
		_choose_sector(icon_name, "")
		return
	icon_selected.emit(icon_name)
	hide_dialog()


## Sector mode: the picked `Iconc` field name or content name; the unused half
## is empty (`PlanetDialog.java:1238-1281` nulls the other).
func _choose_sector(icon_name: String, content_name: String) -> void:
	if _mode != "sector":
		if icon_name.is_empty():
			_choose_icon(icon_name)
		return
	sector_icon_selected.emit(icon_name, content_name)


## Optional string field of a context/read-model dictionary (`null` -> "").
func _opt(row: Dictionary, key: String) -> String:
	var value: Variant = row.get(key, null)
	return "" if value == null else str(value)
