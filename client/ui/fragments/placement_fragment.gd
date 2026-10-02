## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/PlacementFragment.java
##         (plan 14 §3.5, M4).
##
## Block/category palette with search, the command table and the hover info
## box. Static frame in `placement_fragment.tscn` (tscn-first); the category rail
## and block grid are data-driven (plan 02/07/15) and built by this script. The
## hover info box renders `ui::display` rows once `MindHud`/the hover provider
## land.

extends Control

var _selected_category := 0

@onready var search: LineEdit = get_node_or_null("Panel/Layout/Search")
@onready var categories: HBoxContainer = get_node_or_null("Panel/Layout/Categories")
@onready var blocks: GridContainer = get_node_or_null("Panel/Layout/Blocks")


func _ready() -> void:
	if search != null:
		search.text_changed.connect(func(_value: String) -> void: _rebuild())
	_build_categories()
	_rebuild()


func _build_categories() -> void:
	if categories == null:
		return
	# code-instantiated: category tabs are the data-driven block-category list
	# (plan 02); the count is runtime content, not authorable in the scene.
	var names := ["@category.turret", "@category.production", "@category.distribution", "@category.defense"]
	for index in names.size():
		var button := Button.new()
		button.text = names[index]
		button.theme_type_variation = "flatTogglet"
		button.toggle_mode = true
		button.button_pressed = index == 0
		button.pressed.connect(_select.bind(index))
		categories.add_child(button)


func _select(index: int) -> void:
	_selected_category = index
	_rebuild()


func _rebuild() -> void:
	if blocks == null:
		return
	for child in blocks.get_children():
		child.queue_free()
	# code-instantiated: block buttons come from the plan-02 registry filtered by
	# category/search (data-driven; placeholder until the content bridge lands).
