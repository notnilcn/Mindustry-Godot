## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/ItemsDisplay.java (plan 14 §3.2 scope).
##
## Launched-item row: icon + amount chips rebuilt when the item set changes.

class_name ItemsDisplay
extends HBoxContainer

var _amounts := PackedStringArray()


## Rebuilds the chips for `items` (item name -> amount).
func update_items(items: Dictionary) -> void:
	var keys := items.keys()
	keys.sort()
	var signature := PackedStringArray()
	for key in keys:
		signature.append("%s=%s" % [str(key), str(items[key])])
	if signature == _amounts:
		return
	_amounts = signature
	_rebuild(items, keys)


func _rebuild(items: Dictionary, keys: Array) -> void:
	for child in get_children():
		child.queue_free()
	for key in keys:
		# code-instantiated: one chip per used item; count is data-driven.
		var chip := HBoxContainer.new()
		var icon := TextureRect.new()
		icon.texture = MindWidgets.icon_texture("item-%s-ui" % str(key))
		icon.custom_minimum_size = Vector2(24, 24)
		icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		chip.add_child(icon)
		var amount := Label.new()
		amount.text = str(int(items[key]))
		chip.add_child(amount)
		add_child(chip)
