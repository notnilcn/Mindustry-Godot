## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/BlockInventoryFragment.java
##         (plan 14 §3.5, M4).
##
## Inventory grid shown near the cursor; clicking an item withdraws one unit
## through `MindInput.transfer_item` (the `requestItem` command path). The item
## set/amounts are a read model fed by `MindInput`/the sim host.

extends Control

signal take_requested(item: String, amount: int)

var _tile := Vector2i.ZERO

@onready var grid: GridContainer = get_node_or_null("Panel/Grid")


func _ready() -> void:
	visible = false


## Opens the panel for a tile with a `{item_name: amount}` JSON read model.
func open_at(tile: Vector2i, items_json: String) -> void:
	_tile = tile
	var parsed: Variant = JSON.parse_string(items_json)
	set_items(parsed if parsed is Dictionary else {})
	visible = true


## Rebuilds the item grid from a `{item_name: amount}` read model.
func set_items(items: Dictionary) -> void:
	if grid == null:
		return
	for child in grid.get_children():
		child.queue_free()
	for item_name in items:
		# code-instantiated: inventory entries are data-driven from the building's
		# item module read model (plan 08).
		var button := Button.new()
		button.text = "%s x%d" % [str(item_name), int(items[item_name])]
		button.tooltip_text = str(item_name)
		button.pressed.connect(_take.bind(str(item_name), 1))
		grid.add_child(button)
	# code-instantiated: transient close affordance (one per open).
	var close := Button.new()
	close.text = "X"
	close.tooltip_text = "close"
	close.pressed.connect(func() -> void: visible = false)
	grid.add_child(close)


func _take(item: String, amount: int) -> void:
	var input := get_node_or_null("/root/Spine/Input")
	if input != null and input.has_method("transfer_item"):
		input.call("transfer_item", _tile.x, _tile.y, item, amount, false)
	take_requested.emit(item, amount)
