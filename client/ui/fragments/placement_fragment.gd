## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/PlacementFragment.java
##         (plan 14 §3.5, M4).
##
## Block/category palette with search, the command table and the hover info
## box. Static frame in `placement_fragment.tscn` (tscn-first); the category rail
## and block grid are data-driven from the `MindUi.block_catalog_json()` catalogue
## (plan 02/07/15). The hover info box renders `ui::display` rows once
## `MindHud`/the hover provider land.

extends Control

## Upstream `InputHandler.buildPlacementUI` (DesktopInput.java:545-562): the
## four icon buttons under the block pane. `tree`/`map` are campaign-only
## (`visible(() -> state.isCampaign())`).
const INPUT_BUTTONS := [
	{"icon": "paste", "tooltip": "@schematics", "dialog": "schematics"},
	{"icon": "book", "tooltip": "@database", "dialog": "database"},
	{"icon": "tree", "tooltip": "@research", "dialog": "research", "campaign_only": true},
	{"icon": "map", "tooltip": "@planetmap", "dialog": "planet", "campaign_only": true},
]

## `-1` = pick `Category.distribution` on the next rail build (`currentCategory`
## default, reset on `WorldLoadEvent`).
var _selected_category := -1
## Sim tick seen at the last reveal; a smaller tick means a new world loaded.
var _last_tick := -1
var _catalog: Dictionary = {}
var _input_buttons: Array[Button] = []
var _bar_campaign := false

@onready var panel: PanelContainer = get_node_or_null("Panel")
@onready var search: LineEdit = get_node_or_null("Panel/Layout/Search")
@onready var categories: GridContainer = get_node_or_null("Panel/Layout/Categories")
@onready var blocks: GridContainer = get_node_or_null("Panel/Layout/Left/Scroll/Blocks")
@onready var input_bar: HBoxContainer = get_node_or_null("Panel/Layout/Left/InputBar")
@onready var top_table: PanelContainer = get_node_or_null("Panel/Layout/Left/TopTable")
@onready var top_rows: VBoxContainer = get_node_or_null("Panel/Layout/Left/TopTable/Rows")

## Last rendered top-table state (`b:<block>` or `h:<kind>:<name>`); the header
## rebuilds only when it changes (`PlacementFragment.lastDisplayState`).
var _top_state := ""
## Requirement amount labels + their `(have, amount)` pairs, refreshed per frame.
var _top_amounts: Array[RichTextLabel] = []
var _top_requirements: Array[Dictionary] = []


func _ready() -> void:
	# `PlacementFragment.blockCatTable` is `full.bottom().right()` with
	# `visible(() -> ui.hudfrag.shown())`: the block pane is part of the standard
	# HUD, not a collapsed tab. The manifest bind owns the fragment root, so the
	# panel itself stays visible here and `close_palette()` remains for callers
	# that need the old collapsed behaviour.
	if panel != null:
		panel.visible = true
	if search != null:
		search.text_changed.connect(func(_value: String) -> void: _rebuild())
	_setup_top_style()
	_build_input_bar()
	# The HUD starts hidden in the menu; the catalogue is rebuilt on every reveal
	# so a sector launch or research that happened while it was hidden is
	# reflected instead of serving a stale category/block list.
	visibility_changed.connect(_ensure_loaded)
	# Category cycling (`,`/`.`) is dispatched by `MindInput`; the rail owns the
	# visible index and pushes it back.
	var input := get_node_or_null("/root/Spine/Input")
	if input != null and input.has_signal("hotkey") and not input.is_connected("hotkey", Callable(self, "_on_hotkey")):
		input.connect("hotkey", Callable(self, "_on_hotkey"))
	_refresh_input_bar()
	_ensure_loaded()


## Campaign-gated icon-row visibility, change-cached per frame (`hud.campaign`
## refreshes on the `MindHud` cadence; `state.isCampaign()` upstream), plus the
## top info table refresh (`PlacementFragment` top table `update`).
func _process(_delta: float) -> void:
	var in_campaign := _in_campaign()
	if in_campaign != _bar_campaign:
		_bar_campaign = in_campaign
		_refresh_input_bar()
	_update_top_table()


## `Tex.buttonEdge2` ninepatch frame for the top table (splits 12, margin 5).
func _setup_top_style() -> void:
	if top_table == null:
		return
	var texture: Variant = MindWidgets.icon_texture("button-edge-2")
	if not (texture is Texture2D):
		return
	var box := StyleBoxTexture.new()
	box.texture = texture
	box.texture_margin_left = 12.0
	box.texture_margin_top = 12.0
	box.texture_margin_right = 12.0
	box.texture_margin_bottom = 12.0
	box.content_margin_left = 5.0
	box.content_margin_top = 5.0
	box.content_margin_right = 5.0
	box.content_margin_bottom = 5.0
	top_table.add_theme_stylebox_override("panel", box)


## `PlacementFragment` top table: selected block header + requirements, else the
## hovered displayable. Visibility = `hasInfoBox()`.
func _update_top_table() -> void:
	if top_table == null or top_rows == null:
		return
	var host := get_node_or_null("/root/Spine/SimHost")
	var display := ""
	if host != null and host.has_method("selected_block"):
		display = str(host.call("selected_block"))
		if display == "air" or display == "remove-wall" or display == "remove-ore":
			display = ""
	var hover: Dictionary = {}
	if display.is_empty() and host != null and host.has_method("hover_info"):
		var tile: Variant = _mouse_tile()
		if tile != null:
			var info: Variant = host.call("hover_info", int(tile.x), int(tile.y))
			if info is Dictionary and not str((info as Dictionary).get("kind", "")).is_empty():
				hover = info
	if display.is_empty() and hover.is_empty():
		top_table.visible = false
		return
	top_table.visible = true
	var state := (
		"b:%s" % display
		if not display.is_empty()
		else "h:%s:%s" % [str(hover.get("kind", "")), str(hover.get("name", ""))]
	)
	if state == _top_state:
		_refresh_top_amounts()
		return
	_top_state = state
	_rebuild_top(display, hover)


## The world tile under the cursor, or `null` while any UI holds the mouse
## (`Core.scene.hasMouse(...)` guard in `hovered()`). Only `STOP` controls
## consume the mouse; the HUD fragment root is a full-rect `PASS` control, so
## `PASS` must not hide the world under it.
func _mouse_tile() -> Variant:
	var hovered := get_tree().root.gui_get_hovered_control()
	if hovered != null and hovered.mouse_filter == Control.MOUSE_FILTER_STOP:
		return null
	var camera := get_node_or_null("/root/Spine/World/Camera2D")
	if camera == null or not camera.has_method("screen_to_tile"):
		return null
	var mouse := get_viewport().get_mouse_position()
	return camera.call("screen_to_tile", mouse.x, mouse.y)


## Rebuilds the header + rows for a state change (`topTable.clear()` path).
func _rebuild_top(display: String, hover: Dictionary) -> void:
	for child in top_rows.get_children():
		child.queue_free()
	_top_amounts.clear()
	_top_requirements.clear()
	if display.is_empty():
		_build_hover_row(hover)
		return
	_build_block_header(display)
	_build_requirement_rows(display)
	_refresh_top_amounts()


## Header: 32px icon, 190px name label (`localizedName` + key combo), `?` button.
func _build_block_header(display: String) -> void:
	# code-instantiated: header cells are rebuilt per displayed block
	# (`topTable.clear()`; plan 14 §3.5).
	var header := HBoxContainer.new()
	header.add_theme_constant_override("separation", 4)
	var icon := TextureRect.new()
	icon.custom_minimum_size = Vector2(32, 32)
	icon.texture = MindWidgets.icon_texture("block-%s-ui" % display)
	if icon.texture == null:
		icon.texture = MindWidgets.icon_texture("block-%s-full" % display)
	icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	header.add_child(icon)
	var label := MindRichLabel.new()
	label.custom_minimum_size = Vector2(190, 0)
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	label.text = MindWidgets.markup("@block.%s.name" % display) + _key_combo(display)
	header.add_child(label)
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	header.add_child(spacer)
	var info := MindWidgets.button("?")
	info.name = "blockinfo"
	info.custom_minimum_size = Vector2(40, 40)
	info.tooltip_text = _bundle("database", "Database")
	info.pressed.connect(_open_content_info.bind(display))
	header.add_child(info)
	top_rows.add_child(header)


## `?` button: `ui.content.show(displayBlock)` (`ContentInfoDialog`).
func _open_content_info(content_name: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("open_dialog"):
		ui.call("open_dialog", "content", JSON.stringify({"content": content_name}))


## Requirement rows: 16px item icon, item name, `have/need` amount
## (`PlacementFragment` refresh: `[scarlet]` under half, `[accent]` under, else
## `[white]`).
func _build_requirement_rows(display: String) -> void:
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("block_requirements_json"):
		return
	var parsed: Variant = JSON.parse_string(str(host.call("block_requirements_json", display)))
	if not (parsed is Array):
		return
	for entry_variant in parsed as Array:
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		# code-instantiated: one row per block requirement (content data).
		var line := HBoxContainer.new()
		line.add_theme_constant_override("separation", 2)
		var icon := TextureRect.new()
		icon.custom_minimum_size = Vector2(16, 16)
		var item := str(entry.get("item", ""))
		icon.texture = MindWidgets.icon_texture("item-%s-ui" % item)
		icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		line.add_child(icon)
		var name_label := MindRichLabel.new()
		name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		name_label.text = MindWidgets.markup("@item.%s.name" % item)
		line.add_child(name_label)
		var amount_label := MindRichLabel.new()
		amount_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
		line.add_child(amount_label)
		top_rows.add_child(line)
		_top_amounts.append(amount_label)
		_top_requirements.append(entry)


## Refreshes requirement amounts every frame (core inventory changes live).
func _refresh_top_amounts() -> void:
	for index in _top_amounts.size():
		var entry: Dictionary = _top_requirements[index]
		var have := int(entry.get("have", 0))
		var need := int(entry.get("amount", 0))
		# `amount < stackamount / 2f ? [scarlet] : amount < stackamount ?
		# [accent] : [white]` (`Pal.remove`/`Pal.accent`).
		var color := "#ffffff"
		if float(have) < float(need) / 2.0:
			color = "#e55454"
		elif have < need:
			color = "#ffd37f"
		var label := _top_amounts[index]
		label.text = "[color=%s]%d[/color]/%d" % [color, have, need]


## Hover row: 32px icon + `Displayable` label (`Tile.display` name).
func _build_hover_row(hover: Dictionary) -> void:
	# code-instantiated: the hover row follows the cursor's target
	# (`topTable.clear()`; plan 14 §3.5).
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 4)
	var icon := TextureRect.new()
	icon.custom_minimum_size = Vector2(32, 32)
	var icon_name := str(hover.get("icon", ""))
	icon.texture = MindWidgets.icon_texture(icon_name)
	if icon.texture == null and not icon_name.is_empty():
		icon.texture = MindWidgets.icon_texture("block-%s-ui" % str(hover.get("name", "")))
	icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	row.add_child(icon)
	var label := MindRichLabel.new()
	label.custom_minimum_size = Vector2(190, 0)
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var label_key := str(hover.get("label", ""))
	if label_key.is_empty():
		label_key = "block.%s.name" % str(hover.get("name", ""))
	label.text = MindWidgets.markup("@%s" % label_key)
	row.add_child(label)
	top_rows.add_child(row)


## `PlacementFragment` key combo: category hotkey + row/column digits from the
## `block_select_01..10` binds (`placement.blockselectkeys`).
func _key_combo(display: String) -> String:
	var list: Array = _catalog.get("categories", [])
	if _selected_category < 0 or _selected_category >= list.size():
		return ""
	var category: Dictionary = list[_selected_category]
	var blocks_list: Array = category.get("blocks", [])
	var index := -1
	for i in blocks_list.size():
		if str((blocks_list[i] as Dictionary).get("name", "")) == display:
			index = i
			break
	if index < 0:
		return ""
	var binds := _block_select_displays()
	if binds.size() < 10:
		return ""
	var ordinal := int(category.get("ordinal", 0))
	var category_key: String = binds[mini(ordinal, 9)]
	var units: String = binds[index % 10]
	var combo := "%s,%s" % [category_key, units]
	if index >= 10:
		combo = "%s,%s" % [category_key, "%s,%s" % [binds[mini((index + 1) / 10 - 1, 9)], units]]
	return "[color=#9a9a9a]\nKey: [%s][/color]" % combo


## `block_select_01..10` key display strings from the input binding registry.
func _block_select_displays() -> Array[String]:
	var out: Array[String] = []
	var input := get_node_or_null("/root/Spine/Input")
	if input == null or not input.has_method("keybinds_json"):
		return out
	var parsed: Variant = JSON.parse_string(str(input.call("keybinds_json")))
	if not (parsed is Array):
		return out
	var by_name: Dictionary = {}
	for entry_variant in parsed as Array:
		if entry_variant is Dictionary:
			by_name[str((entry_variant as Dictionary).get("name", ""))] = str(
				(entry_variant as Dictionary).get("display", "")
			)
	for i in range(1, 11):
		out.append(str(by_name.get("block_select_%02d" % i, "")))
	return out


## `InputHandler.buildPlacementUI` icon row: 48px glyph buttons left-aligned
## under the pane; the dialog routing mirrors the menu's `MindUi.open_dialog`.
func _build_input_bar() -> void:
	if input_bar == null or not _input_buttons.is_empty():
		return
	for entry in INPUT_BUTTONS:
		# code-instantiated: fixed upstream icon row (four desktop buttons).
		var label := _bundle(str(entry.get("tooltip", "")), str(entry.get("icon", "")))
		var button := MindWidgets.glyph_button(str(entry.get("icon", "")), label, "clearNonei")
		button.name = "input-%s" % str(entry.get("icon", ""))
		button.tooltip_text = label
		button.custom_minimum_size = Vector2(48, 48)
		button.set_meta("campaign_only", bool(entry.get("campaign_only", false)))
		button.pressed.connect(_open_input_dialog.bind(str(entry.get("dialog", ""))))
		input_bar.add_child(button)
		_input_buttons.append(button)


## `visible(() -> state.isCampaign())` on the research/planet buttons.
func _refresh_input_bar() -> void:
	var in_campaign := _in_campaign()
	for button in _input_buttons:
		if bool(button.get_meta("campaign_only", false)):
			button.visible = in_campaign


## `state.isCampaign()` through the read-only `MindHud` facade.
func _in_campaign() -> bool:
	var hud := get_node_or_null("/root/MindHud")
	return hud != null and bool(hud.get("campaign"))


func _open_input_dialog(dialog_name: String) -> void:
	if dialog_name.is_empty():
		return
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("open_dialog"):
		ui.call("open_dialog", dialog_name, "{}")



## Opens the collapsed block-select pane (`blocksSelect` collapser expanded).
func open_palette() -> void:
	if panel != null:
		panel.visible = true
	visible = true
	_ensure_loaded()


## Collapses the block-select pane back to the HUD.
func close_palette() -> void:
	if panel != null:
		panel.visible = false
	_ensure_loaded()


func _on_hotkey(action: String) -> void:
	if action != "category_prev" and action != "category_next":
		return
	if panel == null or not panel.visible:
		open_palette()
	var count := int(_catalog.get("categories", []).size())
	if count <= 0:
		return
	var delta := -1 if action == "category_prev" else 1
	_selected_category = wrapi(_selected_category + delta, 0, count)
	_sync_category()
	_build_categories()
	_rebuild()


func _ensure_loaded() -> void:
	if not is_visible_in_tree():
		return
	# `Events.on(WorldLoadEvent) -> currentCategory = Category.distribution`.
	var host := get_node_or_null("/root/Spine/SimHost")
	if host != null and host.has_method("get_tick"):
		var tick := int(host.call("get_tick"))
		if tick < _last_tick:
			_selected_category = -1
		_last_tick = tick
	_load_catalog()


## Pulls the block catalogue from `MindUi` and paints the rail + grid.
func _load_catalog() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("block_catalog_json"):
		var parsed: Variant = JSON.parse_string(str(ui.call("block_catalog_json")))
		if parsed is Dictionary:
			_catalog = parsed
	_build_categories()
	_sync_category()
	_rebuild()


## Resolves a bundle key through `MindAssets`, falling back to `fallback` when the
## key is missing (the bundle echoes the key) or the autoload is absent.
func _bundle(key: String, fallback: String) -> String:
	if key.is_empty():
		return fallback
	var assets := get_node_or_null("/root/MindAssets")
	if assets == null:
		return fallback
	var value := str(assets.call("bundle_get", key))
	if value.is_empty() or value == key:
		return fallback
	return value


func _build_categories() -> void:
	if categories == null:
		return
	for child in categories.get_children():
		child.queue_free()
	# code-instantiated: category tabs are the data-driven block-category list
	# (plan 02); the count is runtime content, not authorable in the scene.
	var list: Array = _catalog.get("categories", [])
	if list.is_empty():
		return
	if _selected_category < 0 or _selected_category >= list.size():
		_selected_category = 0
		for index in list.size():
			if str((list[index] as Dictionary).get("name", "")) == "distribution":
				_selected_category = index
				break
	# `ButtonGroup`: exactly the active category is checked (`rebuildCategory`).
	var group := ButtonGroup.new()
	for index in list.size():
		var entry: Dictionary = list[index]
		var cat_name := str(entry.get("name", ""))
		var label := _bundle(str(entry.get("label", "")), cat_name)
		var button := MindWidgets.glyph_button(cat_name, label, "clearTogglei")
		button.name = "category-%s" % cat_name
		button.tooltip_text = label
		button.toggle_mode = true
		button.button_group = group
		button.button_pressed = index == _selected_category
		button.custom_minimum_size = Vector2(50, 50)
		button.pressed.connect(_select.bind(index))
		categories.add_child(button)


func _select(index: int) -> void:
	_selected_category = index
	_sync_category()
	_build_categories()
	_rebuild()


## Mirrors the visible category into `MindInput` so the `1`..`0` block-select
## binds pick from the same category the player sees.
func _sync_category() -> void:
	var input := get_node_or_null("/root/Spine/Input")
	if input != null and input.has_method("set_catalog_category"):
		input.call("set_catalog_category", _selected_category)


## Selects a block through `MindInput` so the placement rotation/state stay in
## sync (falls back to the sim host when the input node is absent).
func _select_block(host: Node, name: String) -> void:
	var input := get_node_or_null("/root/Spine/Input")
	if input != null and input.has_method("select_block_by_name"):
		input.call("select_block_by_name", name)
	elif host != null:
		host.call("select_block", name)


func _rebuild() -> void:
	if blocks == null:
		return
	for child in blocks.get_children():
		child.queue_free()
	# code-instantiated: block buttons come from the content catalogue filtered by
	# category/search (data-driven, plan 02/07).
	var list: Array = _catalog.get("categories", [])
	if list.is_empty() or _selected_category < 0 or _selected_category >= list.size():
		return
	var category: Dictionary = list[_selected_category]
	var query := ""
	if search != null:
		query = search.text.to_lower()
	var host := get_node_or_null("/root/Spine/SimHost")
	var selected := ""
	if host != null and host.has_method("selected_block"):
		selected = str(host.call("selected_block"))
	# `ButtonGroup`: the active block is the checked button (`button.group(group)`).
	var group := ButtonGroup.new()
	for block_variant in category.get("blocks", []):
		var block: Dictionary = block_variant
		var name := str(block.get("name", ""))
		var label := _bundle(str(block.get("localized", "")), name)
		if not query.is_empty():
			if not name.to_lower().contains(query) and not label.to_lower().contains(query):
				continue
		var button := MindWidgets.image_button_first(
			PackedStringArray(["block-%s-ui" % name, "block-%s-full" % name, name]), "selecti"
		)
		button.name = "block-%s" % name
		button.tooltip_text = label
		button.toggle_mode = true
		button.button_group = group
		button.button_pressed = name == selected
		button.custom_minimum_size = Vector2(46, 46)
		# `Button.resizeImage(iconMed)`: 46px cell, 32px centred icon.
		button.expand_icon = true
		button.add_theme_constant_override("icon_max_width", 32)
		if button.icon == null:
			button.text = label
		button.pressed.connect(func() -> void:
			_select_block(host, name))
		blocks.add_child(button)
