## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/CoreItemsDisplay.java (plan 14 §3.2 scope).
##
## Core item grid (4 columns), rebuilt only when the used-item set changes.

class_name CoreItemsDisplay
extends GridContainer

const COLUMNS := 4

var _last_signature := ""


func _ready() -> void:
	columns = COLUMNS


## Rebuilds the grid for `items` (item name -> amount).
func update_items(items: Dictionary) -> void:
	var keys := items.keys()
	keys.sort()
	var signature := PackedStringArray()
	for key in keys:
		signature.append("%s" % str(key))
	var joined := "|".join(signature)
	if joined == _last_signature:
		return
	_last_signature = joined
	for child in get_children():
		child.queue_free()
	for key in keys:
		# code-instantiated: one cell per used item; count is data-driven.
		var cell := HBoxContainer.new()
		var icon := TextureRect.new()
		icon.texture = MindWidgets.icon_texture("item-%s-ui" % str(key))
		icon.custom_minimum_size = Vector2(28, 28)
		icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		cell.add_child(icon)
		var amount := Label.new()
		amount.text = str(int(items[key]))
		cell.add_child(amount)
		add_child(cell)
