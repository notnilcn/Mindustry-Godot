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


## `/root/Spine/Input` double for the keybind-dialog oracle (the real node is a
## game-scene child that the `--script` main loop does not instantiate).
class KeybindInputStub:
	extends Node

	func keybinds_json() -> String:
		return JSON.stringify([
			{
				"name": "move_x",
				"category": "general",
				"axis": true,
				"bundle": "keybind.move_x.name",
				"value": "d",
				"display": "D",
				"negativeDisplay": "A",
				"default": true,
			},
			{
				"name": "select",
				"category": "general",
				"axis": false,
				"bundle": "keybind.select.name",
				"value": "q",
				"display": "Q",
				"negativeDisplay": null,
				"default": true,
			},
		])

	func rebind(_name: String, _code: String) -> bool:
		return true

	func rebind_key(_name: String, _text: String) -> bool:
		return true

	func reset_keybind(_name: String) -> bool:
		return true

	func reset_keybinds() -> void:
		pass

	func unbind_keybind(_name: String) -> bool:
		return true


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

	# A dialog's Back button must close through `MindUi`, not just hide the node:
	# the stack entry keeps the pause governor and the menu modal guard latched
	# (upstream `BaseDialog.hide` leaves the UI interactive).
	var ui: Node = root.get_node_or_null("MindUi")
	_check(ui != null, "MindUi autoload is present")
	if ui != null:
		var dialog: Control = load("res://ui/mind_dialog.gd").new()
		dialog.name = "ui_widgets_check_dialog"
		root.add_child(dialog)
		var buttons := VBoxContainer.new()
		dialog.add_child(buttons)
		dialog.buttons = buttons
		ui.call("register_dialog", String(dialog.name), dialog, false)
		ui.call("open_dialog", String(dialog.name), "")
		_check(bool(ui.call("has_dialog")), "open_dialog pushes the dialog onto the MindUi stack")
		dialog.add_close_button().pressed.emit()
		_check(not bool(ui.call("has_dialog")), "Back button pops the dialog off the MindUi stack")
		_check(not dialog.visible, "Back button hides the dialog node")

	# The Controls dialog must render the binding registry (names, key columns,
	# Rebind/Reset), not the category-header stub (EV-0023).
	var spine := Node.new()
	spine.name = "Spine"
	root.add_child(spine)
	var stub := KeybindInputStub.new()
	stub.name = "Input"
	spine.add_child(stub)
	var keybind: Control = load("res://scenes/ui/dialogs/keybind_dialog.tscn").instantiate()
	root.add_child(keybind)
	var button_texts: Array = []
	for button in keybind.find_children("*", "Button", true, false):
		button_texts.append(button.text)
	_check(
		button_texts.count("Rebind") == 2,
		"keybind dialog renders one Rebind per binding (got %s)" % button_texts.count("Rebind")
	)
	_check(
		button_texts.count("Reset to Defaults") == 1,
		"keybind dialog renders the reset-all button"
	)
	var label_texts: Array = []
	for node in keybind.find_children("*", "", true, false):
		var text: Variant = node.get("text")
		if text is String:
			label_texts.append(text)
	_check(label_texts.has("A / D"), "axis binding renders min / max (A / D)")
	_check(label_texts.has("Move X"), "binding name resolves from the bundle")

	print("UICHECK: failed=", _failures)
	quit(1 if _failures > 0 else 0)
