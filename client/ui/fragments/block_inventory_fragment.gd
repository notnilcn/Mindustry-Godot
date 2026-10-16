## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/BlockInventoryFragment.java
##         (plan 14 §3.5, M4; G03 T13 presentation pass).
##
## `Tex.inventory` panel at the building's top-right: a 3-column grid of 40x40
## item icons with the live amount bottom-left, tapping takes the whole stack,
## holding takes one every 20 ticks, the panel follows the building and closes
## when it is gone, empty for 120 ticks, or tapped again.

extends Control

## `BlockInventoryFragment.holdWithdraw` (ticks per held withdraw).
const HOLD_WITHDRAW := 20.0
## `BlockInventoryFragment.holdShrink` (ticks before an absent item's cell goes).
const HOLD_SHRINK := 120.0
## Cell size (`size(8 * 5)`), grid columns (`cols = 3`).
const CELL := 40.0
const COLUMNS := 3
## Live re-read cadence (seconds).
const REFRESH_INTERVAL := 0.1

signal take_requested(item: String, amount: int)

var _tile := Vector2i(-1, -1)
## Item name → `[control, label, amount]`.
var _cells: Dictionary = {}
## Item names in the current display order.
var _order: Array[String] = []
## Ticks the inventory has been empty (`emptyTime`).
var _empty_time := 0.0
## Item name → ticks absent (`holdShrink` tracking).
var _absent: Dictionary = {}
var _accum := 0.0
## Held cell state (`_holding`, `_hold_time`, `_held`).
var _holding := ""
var _hold_time := 0.0
var _held := false

@onready var panel: PanelContainer = get_node_or_null("Panel")
@onready var grid: GridContainer = get_node_or_null("Panel/Grid")


func _ready() -> void:
	visible = false
	_setup_style()
	if grid != null:
		grid.columns = COLUMNS
		grid.add_theme_constant_override("h_separation", 4)
		grid.add_theme_constant_override("v_separation", 4)


## `Tex.inventory` ninepatch (margins 4) behind the grid.
func _setup_style() -> void:
	if panel == null:
		return
	var assets := MindWidgets.assets()
	if assets == null or not assets.has_method("find_region"):
		return
	var texture: Variant = assets.call("find_region", "inventory")
	if not (texture is Texture2D):
		return
	var box := StyleBoxTexture.new()
	box.texture = texture
	box.texture_margin_left = 4.0
	box.texture_margin_top = 4.0
	box.texture_margin_right = 4.0
	box.texture_margin_bottom = 4.0
	box.content_margin_left = 4.0
	box.content_margin_top = 4.0
	box.content_margin_right = 4.0
	box.content_margin_bottom = 4.0
	panel.add_theme_stylebox_override("panel", box)


## Opens the panel for a tile with a `{item_name: amount}` JSON read model;
## tapping the same building again closes it (`showFor`).
func open_at(tile: Vector2i, items_json: String) -> void:
	if visible and tile == _tile:
		hide_panel()
		return
	_tile = tile
	_empty_time = 0.0
	_absent.clear()
	var parsed: Variant = JSON.parse_string(items_json)
	var items: Dictionary = parsed if parsed is Dictionary else {}
	if items.is_empty():
		hide_panel()
		return
	set_items(items)
	visible = true


## Rebuilds the item grid from a `{item_name: amount}` read model.
func set_items(items: Dictionary) -> void:
	if grid == null:
		return
	for child in grid.get_children():
		child.queue_free()
	_cells.clear()
	_order.clear()
	for item_name in items:
		var name := str(item_name)
		_add_cell(name, int(items[item_name]))
	visible = not items.is_empty()


## Hides the panel (tapped again, empty for 120 ticks, or the building is gone).
func hide_panel() -> void:
	visible = false
	_tile = Vector2i(-1, -1)
	_cells.clear()
	_order.clear()
	_holding = ""
	_hold_time = 0.0
	_held = false


func _process(delta: float) -> void:
	if not visible:
		return
	_position_panel()
	_hold_time += delta * 60.0 if not _holding.is_empty() else 0.0
	if not _holding.is_empty() and _hold_time >= HOLD_WITHDRAW:
		_take(_holding, 1)
		_hold_time = 0.0
		_held = true
	_accum += delta
	if _accum < REFRESH_INTERVAL:
		return
	_accum = 0.0
	_refresh()


## `BlockInventoryFragment.update`: follows the building's top-right corner.
## The panel hugs its `Tex.inventory` table (`table.pack()`), not the scene's
## 260x200 stub — Java draws a compact box around the 3-column 40 px grid.
func _position_panel() -> void:
	var camera := get_node_or_null("/root/Spine/World/Camera2D")
	if camera == null or not camera.has_method("world_to_screen"):
		return
	var size := 1
	var host := get_node_or_null("/root/Spine/SimHost")
	if host != null and host.has_method("block_size"):
		size = maxi(1, int(host.call("block_size", _tile.x, _tile.y)))
	var corner := Vector2(
		float(_tile.x) * 8.0 + float(size) * 4.0, float(_tile.y) * 8.0 + float(size) * 4.0
	)
	position = camera.call("world_to_screen", corner.x, corner.y)
	if panel != null:
		# `table.pack()`: the fragment is exactly the inventory table
		# (the scene stub is 260x200, which stretched the ninepatch).
		self.size = panel.get_combined_minimum_size()
		pivot_offset = self.size / 2.0


## Live amounts every 100 ms; hide when the building is gone, the inventory has
## been empty for 120 ticks, or rebuild when an item appears/disappears.
func _refresh() -> void:
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("building_items_json"):
		return
	var tile: Variant = host.call("tile_debug", _tile.x, _tile.y)
	if tile is Dictionary and not bool((tile as Dictionary).get("has_build", false)):
		hide_panel()
		return
	var parsed: Variant = JSON.parse_string(
		str(host.call("building_items_json", _tile.x, _tile.y))
	)
	var items: Dictionary = parsed if parsed is Dictionary else {}
	var changed := false
	for item_name in items:
		var name := str(item_name)
		var amount := int(items[item_name])
		if _cells.has(name):
			_cells[name][2] = amount
			(_cells[name][1] as Label).text = _round(float(amount))
			_absent.erase(name)
		else:
			_add_cell(name, amount)
			changed = true
	for name in _order.duplicate():
		if items.has(name):
			continue
		_absent[name] = float(_absent.get(name, 0.0)) + REFRESH_INTERVAL * 60.0
		if float(_absent[name]) >= HOLD_SHRINK:
			changed = true
	if changed:
		set_items(items)
	if items.is_empty():
		_empty_time += REFRESH_INTERVAL * 60.0
		if _empty_time >= HOLD_SHRINK:
			hide_panel()
	else:
		_empty_time = 0.0


## `size(8 * 5)` cell: item icon (`item-<name>-ui`) with the amount bottom-left.
func _add_cell(item_name: String, amount: int) -> void:
	# code-instantiated: one cell per inventory item (count driven by the item
	# module read model; G03 T13).
	var cell := Control.new()
	cell.custom_minimum_size = Vector2(CELL, CELL)
	cell.mouse_filter = Control.MOUSE_FILTER_STOP
	cell.gui_input.connect(_on_cell_input.bind(item_name))
	var icon := TextureRect.new()
	icon.set_anchors_preset(Control.PRESET_FULL_RECT)
	icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	icon.texture = MindWidgets.icon_texture("item-%s-ui" % item_name)
	if icon.texture == null:
		icon.texture = MindWidgets.icon_texture("item-%s" % item_name)
	cell.add_child(icon)
	var label := Label.new()
	label.set_anchors_preset(Control.PRESET_BOTTOM_LEFT)
	label.text = _round(float(amount))
	cell.add_child(label)
	grid.add_child(cell)
	_cells[item_name] = [cell, label, amount]
	_order.append(item_name)


func _on_cell_input(event: InputEvent, item_name: String) -> void:
	if not (event is InputEventMouseButton):
		return
	var button := event as InputEventMouseButton
	if button.button_index != MOUSE_BUTTON_LEFT:
		return
	if button.pressed:
		_holding = item_name
		_hold_time = 0.0
		_held = false
	else:
		if not _held:
			_take(item_name, int(_cells.get(item_name, [null, null, 0])[2]))
		_holding = ""
		_hold_time = 0.0


func _take(item: String, amount: int) -> void:
	var input := get_node_or_null("/root/Spine/Input")
	if input != null and input.has_method("transfer_item"):
		input.call("transfer_item", _tile.x, _tile.y, item, amount, false)
	take_requested.emit(item, amount)


## `UI.formatAmount`-style short count: `1.2k` / `3mil` (`unit.thousands` /
## `unit.millions` bundle values).
func _round(amount: float) -> String:
	if amount >= 1_000_000.0:
		return "%d%s" % [int(amount / 1_000_000.0), _bundle("unit.millions", "mil")]
	if amount >= 1_000.0:
		return "%d%s" % [int(amount / 1_000.0), _bundle("unit.thousands", "k")]
	return str(int(amount))


func _bundle(key: String, fallback: String) -> String:
	var assets := get_node_or_null("/root/MindAssets")
	if assets == null or not assets.has_method("bundle_get"):
		return fallback
	var value := str(assets.call("bundle_get", key))
	return value if not value.is_empty() and value != key else fallback
