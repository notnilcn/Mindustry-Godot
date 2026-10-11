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
## refreshes on the `MindHud` cadence; `state.isCampaign()` upstream).
func _process(_delta: float) -> void:
	var in_campaign := _in_campaign()
	if in_campaign != _bar_campaign:
		_bar_campaign = in_campaign
		_refresh_input_bar()


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
