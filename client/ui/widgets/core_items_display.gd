## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/CoreItemsDisplay.java (plan 14 §3.2 scope).
##
## Core item table: per item an `iconSmall` image and a `UI.formatAmount`
## label (`minWidth(52)`, `padRight(3)` each), four items per row, in content
## order. The cells are rebuilt only when the shown item set changes; the
## amounts refresh every update (Java binds the label to the live core).
##
## Deviation: Java keeps every item ever seen (`usedItems`) until `resetUsed`
## on `ResetEvent`; the port has no reset hook here, so it shows the items the
## core currently holds (`core_items_json` omits zero amounts).

class_name CoreItemsDisplay
extends GridContainer

## Four items per row, two cells (image + label) per item (`++i % 4 == 0`).
const ITEMS_PER_ROW := 4
## `Vars.iconSmall` (`8 * 3`).
const ICON_SIZE := 24.0
## `label(...).minWidth(52f)`.
const LABEL_MIN_WIDTH := 52.0
## `padRight(3)` on both cells.
const PAD_RIGHT := 3

## Shown item names in content order (the `core_items_json` key order).
var _order := PackedStringArray()
## Item name -> amount label.
var _labels: Dictionary = {}


func _ready() -> void:
	columns = ITEMS_PER_ROW * 2
	add_theme_constant_override("h_separation", PAD_RIGHT)
	add_theme_constant_override("v_separation", 0)


## Whether any item is shown (`usedItems.size > 0` gates the background).
func has_items() -> bool:
	return not _order.is_empty()


## Updates the table for `items` (item name -> amount, content order).
func update_items(items: Dictionary) -> void:
	var names := PackedStringArray()
	for key in items.keys():
		names.append(str(key))
	if names != _order:
		_order = names
		_rebuild()
	for item_name in _order:
		var label: RichTextLabel = _labels.get(item_name)
		if label != null:
			label.text = _format_amount(int(items.get(item_name, 0)))


func _rebuild() -> void:
	_labels.clear()
	for child in get_children():
		remove_child(child)
		child.queue_free()
	for item_name in _order:
		# code-instantiated: one image + label pair per shown item; the count
		# is driven by the live core inventory.
		var icon := TextureRect.new()
		icon.texture = MindWidgets.icon_texture("item-%s-ui" % item_name)
		icon.custom_minimum_size = Vector2(ICON_SIZE, ICON_SIZE)
		icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		icon.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		icon.tooltip_text = _bundle("item.%s.name" % item_name, item_name)
		add_child(icon)
		var amount := RichTextLabel.new()
		amount.bbcode_enabled = true
		amount.fit_content = true
		amount.scroll_active = false
		amount.autowrap_mode = TextServer.AUTOWRAP_OFF
		amount.custom_minimum_size = Vector2(LABEL_MIN_WIDTH, 0.0)
		amount.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		amount.mouse_filter = Control.MOUSE_FILTER_IGNORE
		amount.add_theme_color_override("default_color", Color.WHITE)
		add_child(amount)
		_labels[item_name] = amount


## `UI.formatAmount(long)` (`UI.java:716-734`) with `[gray]` unit suffixes
## (`unit.billions`/`unit.millions`/`unit.thousands`).
func _format_amount(number: int) -> String:
	var mag := absi(number)
	var prefix := "-" if number < 0 else ""
	if mag >= 1_000_000_000:
		return "%s%.1f%s" % [prefix, mag / 1_000_000_000.0, _unit("unit.billions", "B")]
	if mag >= 1_000_000:
		return "%s%.1f%s" % [prefix, mag / 1_000_000.0, _unit("unit.millions", "M")]
	if mag >= 10_000:
		# Java `number / 1000` (integer division, sign kept by the quotient).
		return "%d%s" % [int(float(number) / 1000.0), _unit("unit.thousands", "k")]
	if mag >= 1000:
		return "%s%.1f%s" % [prefix, mag / 1000.0, _unit("unit.thousands", "k")]
	return str(number)


func _unit(key: String, fallback: String) -> String:
	return "[color=#7f7f7f]%s[/color]" % _bundle(key, fallback)


func _bundle(key: String, fallback: String) -> String:
	var assets := get_node_or_null("/root/MindAssets")
	if assets == null or not assets.has_method("bundle_get"):
		return fallback
	var value := str(assets.call("bundle_get", key))
	return value if not value.is_empty() and value != key else fallback
