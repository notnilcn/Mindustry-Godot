# SPDX-License-Identifier: GPL-3.0-only
#
# Headless widget check (plan 14 GDScript-widget oracle).
#
# `godot --headless --editor --quit` only parses scripts: it cannot catch a
# widget asking the atlas for a region the pack does not contain, nor a control
# whose minimum size leaves its row mostly unclickable. This script boots the
# `MindAssets` autoload scene with the real packed atlas, drives the check
# widget and asserts the indicator region resolves and the row sizes itself.
#
# Usage:
#   godot4 --headless --path client --script res://tools/ui_widgets_check.gd
#
# Exit code 0 when every assertion holds, 1 otherwise.

extends SceneTree

const CHECK_ROW_MIN_HEIGHT := 32.0
const CHECK_REGIONS := ["check-off", "check-on", "check-over", "check-on-over"]

var _failures := 0


func _check(condition: bool, message: String) -> void:
	if condition:
		print("UICHECK: ok ", message)
	else:
		printerr("UICHECK: FAIL ", message)
		_failures += 1


func _find_texture_rect(node: Node) -> TextureRect:
	for child in node.get_children():
		if child is TextureRect:
			return child
		var found := _find_texture_rect(child)
		if found != null:
			return found
	return null


func _init() -> void:
	# Project autoloads (including `MindAssets`) are instantiated by the engine
	# when the main loop starts; wait a frame before querying them.
	await process_frame
	var assets: Node = root.get_node_or_null("MindAssets")
	_check(assets != null, "MindAssets autoload is present")
	if assets == null:
		print("UICHECK: failed=", _failures)
		quit(1)
		return

	for region in CHECK_REGIONS:
		_check(assets.call("find_region", region) != null, "atlas region %s resolves" % region)

	# code-instantiated: the widget under test builds its icon+label row in code
	# (`Elems.check`); the test drives that exact path.
	var check: Control = load("res://ui/widgets/mind_check.gd").new()
	check.setup("Conveyor Placement Pathfinding", true, Callable())
	_check(
		check.custom_minimum_size.y >= CHECK_ROW_MIN_HEIGHT,
		"check row minimum height >= %s (got %s)" % [CHECK_ROW_MIN_HEIGHT, check.custom_minimum_size.y]
	)
	var indicator := _find_texture_rect(check)
	var on_texture: Texture2D = assets.call("find_region", "check-on")
	_check(indicator != null and indicator.texture == on_texture, "checked indicator uses check-on")
	check.set_checked(false)
	var off_texture: Texture2D = assets.call("find_region", "check-off")
	_check(indicator != null and indicator.texture == off_texture, "unchecked indicator uses check-off")
	check.free()

	# `SettingsMenuDialog` places each check in a 45px cell with 7px top pad
	# (upstream `.height(45f).padTop(7f)`, 52px pitch). Fill the cell instead of
	# growing it so extra table height is not absorbed into the rows.
	var table := MindTable.new()
	table.size = Vector2(500, 100)
	root.add_child(table)
	var cell_check: Control = load("res://ui/widgets/mind_check.gd").new()
	cell_check.setup("Conveyor Placement Pathfinding", true, Callable())
	table.add(cell_check).grow_x_axis().fill_y_axis().set_min_height(45.0).set_pad_top(7.0)
	table.row()
	table.sort_now()
	_check(
		is_equal_approx(cell_check.size.y, 45.0),
		"settings check cell keeps the 45px row (got %s)" % cell_check.size.y
	)
	table.free()

	print("UICHECK: failed=", _failures)
	quit(1 if _failures > 0 else 0)
