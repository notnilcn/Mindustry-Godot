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

var _selected_category := 0
var _catalog: Dictionary = {}
var _loaded := false

@onready var search: LineEdit = get_node_or_null("Panel/Layout/Search")
@onready var categories: HBoxContainer = get_node_or_null("Panel/Layout/Categories")
@onready var blocks: GridContainer = get_node_or_null("Panel/Layout/Scroll/Blocks")


func _ready() -> void:
	if search != null:
		search.text_changed.connect(func(_value: String) -> void: _rebuild())
	# The HUD starts hidden in the menu; build the (content-booting) catalogue on
	# first reveal instead of during boot.
	visibility_changed.connect(_ensure_loaded)
	_ensure_loaded()


func _ensure_loaded() -> void:
	if _loaded or not is_visible_in_tree():
		return
	_loaded = true
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
	if _selected_category >= list.size():
		_selected_category = 0
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
