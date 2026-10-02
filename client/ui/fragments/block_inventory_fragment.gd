## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/BlockInventoryFragment.java
##         (plan 14 §3.5, M4).
##
## Inventory grid shown near the cursor; clicking an item takes/withdraws it via
## the plan-21 `Call.takeItems` relay. The item set/amounts are a read model; the
## M4 shell renders the frame and the take-intent signal.

extends Control

signal take_requested(item: String, amount: int)

@onready var grid: GridContainer = get_node_or_null("Panel/Grid")


func _ready() -> void:
	visible = false


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


func _take(item: String, amount: int) -> void:
	take_requested.emit(item, amount)
