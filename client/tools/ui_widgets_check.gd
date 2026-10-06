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
	var key_width := -1.0
	for node in keybind.find_children("*", "", true, false):
		if node.get("text") == "A / D" and node is Control:
			key_width = (node as Control).size.x
	_check(key_width > 1.0, "key label renders with a real width (got %s)" % key_width)

	# The About dialog titles must resolve through the Links fallback, not render
	# the raw `link.*.title` keys (EV-0006).
	var about: Control = load("res://scenes/ui/dialogs/about_dialog.tscn").instantiate()
	root.add_child(about)
	var about_texts: Array = []
	for node in about.find_children("*", "", true, false):
		var text: Variant = node.get("text")
		if text is String:
			about_texts.append(text)
	_check(about_texts.has("Discord"), "About renders the Discord title")
	_check(about_texts.has("Trello"), "About renders the Trello title")
	_check(not about_texts.has("link.trello.title"), "About does not render raw title keys")
	for title in [
		"Discord", "Changelog", "Trello", "Wiki", "Suggestions", "Reddit",
		"Itch.io", "Google play", "F droid", "Github", "Dev builds", "Bug",
	]:
		_check(about_texts.has(title), "About renders the %s link" % title)
	_check(about_texts.has("Credits"), "About renders the Credits button")

	# EV-0007: the Credits dialog lists the packed contributors and translates
	# the `credits.text` blurb.
	var credits: Control = load("res://scenes/ui/dialogs/credits_dialog.tscn").instantiate()
	root.add_child(credits)
	var credits_texts: Array = []
	for node in credits.find_children("*", "", true, false):
		var text: Variant = node.get("text")
		if text is String:
			credits_texts.append(text)
	_check(credits_texts.has("redloong9527"), "credits dialog lists contributors")
	_check(not credits_texts.has("credits.text"), "credits dialog translates credits.text")
	# Contributor rows must render at their text width, not the 1px minimum a
	# word-wrapped `RichTextLabel` reports (EV-0030).
	await process_frame
	var contributor_width := -1.0
	for node in credits.find_children("*", "RichTextLabel", true, false):
		if str(node.get("text")) == "redloong9527":
			contributor_width = (node as Control).size.x
	_check(
		contributor_width > 1.0,
		"credits contributor label renders its text width (got %s)" % contributor_width
	)

	# Slider value labels sit in a settings row HBox after the expanding title;
	# they must keep their text width instead of collapsing to 1px (EV-0020).
	var value_header := HBoxContainer.new()
	value_header.size = Vector2(420, 40)
	root.add_child(value_header)
	var value_label: Control = load("res://ui/text/mind_rich_label.gd").new()
	value_label.text = "100%"
	value_header.add_child(value_label)
	await process_frame
	_check(
		value_label.size.x > 1.0,
		"slider value label renders its text width (got %s)" % value_label.size.x
	)
	value_header.free()

	# A menu leaf button must dismiss the open submenu before running its action
	# (upstream MenuFragment.buttons). (EV-0008)
	var menu: Control = load("res://scenes/ui/fragments/menu_fragment.tscn").instantiate()
	root.add_child(menu)
	var source := Button.new()
	var leaf := Button.new()
	menu.call("_show_submenu", [{"text": "@about.button", "dialog": "about"}], source)
	var submenu := menu.get("_submenu") as Control
	_check(submenu != null and submenu.visible, "menu submenu opens")
	menu.call("_on_menu_button", {"dialog": "about"}, leaf)
	_check(
		submenu != null and not submenu.visible,
		"opening a submenu dialog dismisses the submenu"
	)
	_check(menu.get("_active_button") == null, "dismissed submenu clears the active button")

	print("UICHECK: failed=", _failures)
	quit(1 if _failures > 0 else 0)
