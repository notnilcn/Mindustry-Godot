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


## `/root/Spine/MindCampaign` double for the load/save-dialog oracle: serves the
## live save listing and records the queued load and save.
class SaveCampaignStub:
	extends Node

	var loaded := ""
	var saved := ""

	func list_save_slots() -> Array:
		return [{
			"name": "alpha",
			"file": "user://saves/7.msav",
			"sector": false,
			"autosave": false,
		}]

	func load_slot(name: String) -> bool:
		loaded = name
		return true

	func save_slot(name: String) -> bool:
		saved = name
		return true


## `/root/Spine/SimHost` double for the logic-editor status oracle: only the
## permanent-status probe is needed (`LStatements.java:1483-1485`).
class StatusPermanentStub:
	extends Node

	func status_permanent(effect: String) -> bool:
		return effect == "@status-boss"


## `/root/Spine/MindLogic` double for the effects-dialog grid oracle.
class LogicPickerStub:
	extends Node

	func effect_entries() -> Array:
		return [
			{"name": "warn", "bounds": -1.0, "size": false, "rotate": false, "color": false, "data": ""},
			{"name": "explosion", "bounds": -1.0, "size": true, "rotate": false, "color": false, "data": ""},
			{"name": "trail", "bounds": -1.0, "size": true, "rotate": false, "color": true, "data": ""},
		]


## `/root/Spine/MindCampaign` double for the pause-dialog oracle: serves the
## sector summary and records an abandoned sector.
class PauseCampaignStub:
	extends Node

	var state: Dictionary = {}
	var abandoned := false

	func get_sector_state() -> Dictionary:
		return state

	func abandon_sector() -> bool:
		abandoned = true
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
	# `button("@settings.reset", Icon.refresh, tstyle, ...)` (`KeybindDialog.java:129`)
	# is an icon button: its text lives in the icon row's Label, not Button.text.
	var reset_all := 0
	for button in keybind.find_children("*", "Button", true, false):
		for label in button.find_children("*", "Label", true, false):
			if (label as Label).text == "Reset to Defaults":
				reset_all += 1
	_check(reset_all == 1, "keybind dialog renders the reset-all button (got %s)" % reset_all)
	# Axis cells are `min [red]/[] max` markup (`KeybindDialog.java:98-101`) in a
	# BBCode label: compare the parsed text.
	var label_texts: Array = []
	var key_width := -1.0
	for node in keybind.find_children("*", "", true, false):
		var text: Variant = node.get("text")
		if node is RichTextLabel:
			text = (node as RichTextLabel).get_parsed_text()
		if text is String:
			label_texts.append(text)
			if text == "A / D" and node is Control:
				key_width = (node as Control).size.x
	_check(label_texts.has("A / D"), "axis binding renders min / max (A / D)")
	_check(label_texts.has("Move X"), "binding name resolves from the bundle")
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

	# EV-0029: every link row draws its `Icon` glyph through the icon font (an
	# atlas-region lookup returns null for these names).
	var link_glyphs: Array = []
	for glyph_name in [
		"discord", "list", "trello", "book", "add", "reddit-alien",
		"itchio", "googleplay", "android", "github", "github-square", "wrench",
	]:
		link_glyphs.append(MindIcons.glyph(glyph_name))
	var icon_font: Variant = assets.call("icon_font")
	var glyphs_with_icon_font := 0
	for node in about.find_children("*", "Label", true, false):
		if link_glyphs.has(str(node.get("text"))) and node.get_theme_font("font") == icon_font:
			glyphs_with_icon_font += 1
	_check(
		glyphs_with_icon_font == 12,
		"About renders all 12 link icon glyphs in the icon font (got %s)" % glyphs_with_icon_font
	)

	# EV-0022: the locale list stacks 400x50 rows (upstream `.size(400f, 50f)`);
	# a `size(400)` cell makes every row 400x400.
	var language: Control = load("res://scenes/ui/dialogs/language_dialog.tscn").instantiate()
	root.add_child(language)
	await process_frame
	var english_size := Vector2.ZERO
	for node in language.find_children("*", "Button", true, false):
		if str(node.get("text")) == "English":
			english_size = (node as Control).size
	_check(
		is_equal_approx(english_size.x, 400.0) and english_size.y < 60.0,
		"language rows are 400x50 (got %s)" % english_size
	)
	language.free()

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

	# EV-0004: the version label mirrors `Version.combined()`, drawn in the
	# custom-build color `#fc8140aa` (`MenuFragment.java:112`); the bare host has
	# no MindPlatform, so the BuildInfo default ("custom build", build -1) is used.
	var version_label := menu.get_node_or_null("Version") as Label
	_check(
		version_label != null and version_label.text == "custom build",
		"menu version label renders the combined version (got %s)"
			% (version_label.text if version_label != null else "null")
	)
	if version_label != null:
		_check(
			version_label.get_theme_color("font_color") == Color("fc8140aa"),
			"menu version label uses the custom-build color (got %s)"
				% version_label.get_theme_color("font_color")
		)

	# EV-0054: `Load Game` lists the live saves and queues a load when a card is
	# clicked; the menu opens the dialog with no slot context.
	var save_stub := SaveCampaignStub.new()
	save_stub.name = "MindCampaign"
	spine.add_child(save_stub)
	var load_dialog: Control = load("res://scenes/ui/dialogs/load_dialog.tscn").instantiate()
	root.add_child(load_dialog)
	load_dialog.call("shown")
	var load_grid: Variant = load_dialog.get("_grid")
	var cards := 0
	if load_grid is MindTable:
		for child in (load_grid as MindTable).get_children():
			if child is Button:
				cards += 1
	_check(cards == 1, "load dialog lists the live save slot (got %s card(s))" % cards)
	if cards == 1:
		(load_grid as MindTable).get_child(0).emit_signal("pressed")
		_check(
			save_stub.loaded == "7",
			"clicking a save card queues a load of its file stem (got '%s')" % save_stub.loaded
		)
	load_dialog.free()

	# EV-0066 / PAR-0445: `SaveDialog extends LoadDialog`, so the grid lists the
	# live slots as cards and the `@save.new` button sits in the button row
	# (`SaveDialog.java:19-28`). Its prompt hands the name to
	# `MindCampaign.save_slot`.
	var save_dialog: Control = load("res://scenes/ui/dialogs/save_dialog.tscn").instantiate()
	root.add_child(save_dialog)
	save_dialog.call("shown")
	var save_grid: Variant = save_dialog.get("_grid")
	var save_rows := 0
	if save_grid is MindTable:
		for child in (save_grid as MindTable).get_children():
			if child is Button:
				save_rows += 1
	_check(
		save_rows == 1,
		"save dialog lists the live slot as a card (got %s card(s))" % save_rows
	)
	var new_row := save_dialog.find_child("new_save", true, false) as Button
	_check(
		new_row != null,
		"save dialog has the @save.new button in the button row"
	)
	if new_row != null and ui != null:
		new_row.emit_signal("pressed")
		ui.call("resolve_text_input", "evalslot")
		_check(
			save_stub.saved == "evalslot",
			"new-save prompt queues a save of the entered name (got '%s')" % save_stub.saved
		)
	save_dialog.free()
	save_stub.free()

	# EV-0061: opening the game-over dialog must not re-enter `MindUi` while
	# `open_dialog` mutably binds it (gdext `bind_mut` panic freezes the client).
	if ui != null:
		var over_dialog: Control = load("res://scenes/ui/dialogs/game_over_dialog.tscn").instantiate()
		root.add_child(over_dialog)
		ui.call("register_dialog", "game_over_check", over_dialog, false)
		ui.call("open_dialog", "game_over_check", JSON.stringify({"campaign": false}))
		await process_frame
		_check(bool(ui.call("has_dialog")), "game-over dialog opens through MindUi")
		_check(not bool(ui.call("hud_visible")), "game-over dialog hides the HUD flag")
		ui.call("close_dialog", "game_over_check")
		await process_frame
		_check(bool(ui.call("hud_visible")), "closing the game-over dialog restores the HUD flag")
		over_dialog.free()

	# EV-0063: the desktop pause branch shows the campaign `@objective` /
	# `@abandon` entries and keeps the mobile `@planetmap` off desktop.
	var pause_stub := PauseCampaignStub.new()
	pause_stub.name = "MindCampaign"
	pause_stub.state = {"campaign": true, "presetDescription": "Defend the sector."}
	spine.add_child(pause_stub)
	var paused_dialog: Control = load("res://scenes/ui/dialogs/paused_dialog.tscn").instantiate()
	root.add_child(paused_dialog)
	paused_dialog.call("shown")
	var objective := paused_dialog.find_child("objective", true, false) as Button
	var abandon := paused_dialog.find_child("abandon", true, false) as Button
	var planet := paused_dialog.find_child("planetmap", true, false) as Button
	var savegame := paused_dialog.find_child("savegame", true, false) as Button
	var loadgame := paused_dialog.find_child("loadgame", true, false) as Button
	var panel := paused_dialog.get_node_or_null("Center/Panel") as Control
	_check(
		panel != null and panel.get_combined_minimum_size().x <= panel.custom_minimum_size.x,
		"pause dialog button column does not widen the dialog panel (got %s of %s)"
				% [
					panel.get_combined_minimum_size().x if panel != null else -1.0,
					panel.custom_minimum_size.x if panel != null else -1.0,
				]
	)
	_check(
		objective != null and objective.visible,
		"pause dialog shows @objective for a described campaign sector"
	)
	_check(
		abandon != null and abandon.visible,
		"pause dialog shows @abandon in a campaign sector"
	)
	_check(
		planet != null and not planet.visible,
		"pause dialog keeps the mobile @planetmap off desktop"
	)
	_check(
		savegame != null and not savegame.visible,
		"pause dialog hides @savegame in a campaign sector"
	)
	_check(
		loadgame != null and not loadgame.visible,
		"pause dialog hides @loadgame in a campaign sector"
	)
	# EV-0063: `shown()` runs inside `MindUi.open_dialog`, which mutably binds
	# `MindUi`; a synchronous `is_mobile()` re-entry panics ("bind() failed,
	# already bound") and wedges the UI, so the mobile gate must resolve after
	# that call returns. Flip mobile preview so the branch is observable here.
	if ui != null:
		ui.call("set_mobile_preview", true)
		ui.call("register_dialog", "paused_check", paused_dialog, false)
		_check(bool(ui.call("open_dialog", "paused_check", "")), "pause dialog opens through MindUi")
		await process_frame
		_check(
			planet != null and planet.visible,
			"pause dialog resolves the mobile @planetmap after the MindUi bind returns"
		)
		ui.call("close_dialog", "paused_check")
		ui.call("set_mobile_preview", false)
	if abandon != null:
		abandon.pressed.emit()
		ui.call("resolve_confirm", true)
		_check(pause_stub.abandoned, "pause dialog abandon confirms into the live facade")
	pause_stub.state = {"campaign": false, "editor": false}
	paused_dialog.call("shown")
	_check(
		objective != null and not objective.visible,
		"pause dialog hides @objective outside a campaign sector"
	)
	_check(
		abandon != null and not abandon.visible,
		"pause dialog hides @abandon outside a campaign sector"
	)
	# EV-0066: the desktop non-campaign set exposes @savegame / @loadgame
	# (`PausedDialog.java:89-93`), and the editor branch hides them again.
	_check(
		savegame != null and savegame.visible,
		"pause dialog shows @savegame outside campaign/editor"
	)
	_check(
		loadgame != null and loadgame.visible,
		"pause dialog shows @loadgame outside campaign/editor"
	)
	pause_stub.state = {"campaign": false, "editor": true}
	paused_dialog.call("shown")
	_check(
		savegame != null and not savegame.visible,
		"pause dialog hides @savegame in the editor"
	)
	_check(
		loadgame != null and not loadgame.visible,
		"pause dialog hides @loadgame in the editor"
	)
	paused_dialog.free()
	pause_stub.free()

	# The `@objective` entry opens the full-text dialog with the preset text.
	var full_text: Control = load("res://scenes/ui/dialogs/full_text_dialog.tscn").instantiate()
	root.add_child(full_text)
	full_text.call(
		"set_context_json",
		JSON.stringify({"title": "@objective", "text": "Defend the sector."})
	)
	full_text.call("show_dialog")
	var objective_body := ""
	for node in full_text.find_children("*", "", true, false):
		if str(node.get("text")) == "Defend the sector.":
			objective_body = "Defend the sector."
	_check(
		objective_body == "Defend the sector.",
		"full-text dialog renders the objective description from context"
	)
	full_text.call("hide_dialog")
	full_text.free()

	# Prompt bodies must render BBCode: the uiscale reset prompt's translated
	# `[color=...]` markup used to render literally because the body was a plain
	# Label (EV-0034).
	var ui_root: Control = load("res://scenes/ui/ui_root.tscn").instantiate()
	root.add_child(ui_root)
	var prompt: Dictionary = ui_root.call(
		"_build_prompt", "", "[color=#ff0000ff]Red[/color] plain", false, 400.0
	)
	var prompt_body: Variant = prompt.get("body")
	_check(prompt_body is RichTextLabel, "prompt body is a BBCode label")
	var parsed := ""
	if prompt_body is RichTextLabel:
		parsed = (prompt_body as RichTextLabel).get_parsed_text()
	_check(parsed == "Red plain", "prompt body parses BBCode markup (got '%s')" % parsed)
	ui_root.free()

	# EV-0041: the campaign difficulty dialog builds each rule toggle through the
	# fluent cell API (`Cell.left()`); a missing `MindCell.left()` aborts
	# `_rebuild` before the first toggle and wedges the client under the debugger.
	var rules_dialog: Control = load("res://ui/dialogs/campaign_rules_dialog.gd").new()
	root.add_child(rules_dialog)
	rules_dialog.call("set_context_json", '{"planet":"serpulo"}')
	rules_dialog.call("shown")
	var rules_table: Variant = rules_dialog.get("_table")
	var rule_toggles := 0
	if rules_table is MindTable:
		# The toggles sit in the options box's column (`CampaignRulesDialog`
		# `cont.top().pane(inner -> ...)`), not directly under the table.
		for child in (rules_table as MindTable).find_children("*", "", true, false):
			if child is MindCheck:
				rule_toggles += 1
	_check(
		rule_toggles == 7,
		"campaign rules dialog builds all seven rule toggles (got %s)" % rule_toggles
	)
	rules_dialog.free()

	# L03 T1 (PAR-0413): `Styles.defaultSlider` — the `sliderBack` box fills the
	# whole slider row (Godot draws the track at the box's minimum height), and
	# the row is the 42 px `sliderKnob` tall (java/36-graphics.png: 500x42 rows).
	var slider_theme := MindThemeBuilder.build(assets)
	var slider_box := slider_theme.get_stylebox("slider", "defaultSlider")
	var knob_icon := slider_theme.get_icon("grabber", "defaultSlider")
	_check(
		slider_box != null and is_equal_approx(slider_box.get_minimum_size().y, 42.0),
		"defaultSlider box min height equals the 42 px row (got %s)" % (slider_box.get_minimum_size().y if slider_box else -1.0)
	)
	_check(
		knob_icon != null and knob_icon.get_height() == 42,
		"defaultSlider knob is the 42 px sliderKnob region (got %s)" % (knob_icon.get_height() if knob_icon else -1)
	)
	var probe := MindWidgets.slider(0.0, 100.0, 1.0)
	probe.theme = slider_theme
	root.add_child(probe)
	_check(
		is_equal_approx(probe.get_combined_minimum_size().y, 42.0),
		"a settings slider is 42 px tall (got %s)" % probe.get_combined_minimum_size().y
	)
	probe.free()

	# L05 T2 (PAR-0380): `ColorPicker.show(color, alpha, ...)` builds hue / sat /
	# value rows (+ alpha only when asked) at 370x44 and a hex field that drops
	# the alpha byte while opaque (`ColorPicker.java:47-167`).
	var picker: Control = load("res://scenes/ui/dialogs/color_picker.tscn").instantiate()
	root.add_child(picker)
	picker.call("set_color", Color(1.0, 0.0, 0.0, 1.0))
	picker.set("_alpha", false)
	picker.call("shown")
	var picker_sliders: Dictionary = picker.get("_sliders")
	_check(picker_sliders.size() == 3, "color picker without alpha has 3 slider rows (got %s)" % picker_sliders.size())
	var picker_hex: LineEdit = picker.get("_hex")
	_check(picker_hex != null and picker_hex.text == "ff0000", "opaque hex drops alpha (got %s)" % (picker_hex.text if picker_hex else "null"))
	picker.set("_alpha", true)
	picker.call("shown")
	_check((picker.get("_sliders") as Dictionary).size() == 4, "color picker with alpha has 4 slider rows")
	picker.free()

	# L05 T5: `ItemSelection` icons at `clamp(selectionSize, 0, 40)`
	# (`ItemSelection.java:70`): a 32 px texture shrinks to 24 for an item and
	# a 50 clamps to the 40 px cell.
	var icon_image := Image.create(32, 32, false, Image.FORMAT_RGBA8)
	var icon_texture := ImageTexture.create_from_image(icon_image)
	var item_icon := MindItemSelection.icon_rect(icon_texture, 24.0)
	_check(item_icon.get_combined_minimum_size() == Vector2(24, 24),
		"item selection icon is 24 px (got %s)" % item_icon.get_combined_minimum_size())
	var big_icon := MindItemSelection.icon_rect(icon_texture, 50.0)
	_check(big_icon.get_combined_minimum_size() == Vector2(40, 40),
		"selection icon clamps to 40 px (got %s)" % big_icon.get_combined_minimum_size())
	item_icon.free()
	big_icon.free()

	# L07 T6 (PAR-0407): the enum popups use Java's per-call-site columns
	# (`showSelect(..., cols, sizer)`), not a fixed four.
	var logic_dialog: Control = load("res://ui/dialogs/logic_dialog.gd").new()
	root.add_child(logic_dialog)
	_check(logic_dialog.call("_select_columns", "radar", 0) == 2,
		"radar target popup uses two columns")
	_check(logic_dialog.call("_select_columns", "ucontrol", 0) == 2,
		"ucontrol popup uses two columns")
	_check(logic_dialog.call("_select_columns", "setmarker", 0) == 3,
		"marker control popup uses three columns")
	_check(logic_dialog.call("_select_columns", "query", 1) == 4,
		"query type popup keeps four columns")
	_check(logic_dialog.call("_select_cell_size", "radar", 0) == Vector2(110, 50),
		"radar target cells are 110x50")
	_check(logic_dialog.call("_select_cell_size", "setmarker", 0) == Vector2(140, 50),
		"marker control cells are 140x50")
	# `ApplyStatusStatement.isPermanent` drops the `seconds` field
	# (`LStatements.java:1483-1485`); the effect is field 1, not the clear flag.
	var status_stub := StatusPermanentStub.new()
	status_stub.name = "SimHost"
	spine.add_child(status_stub)
	var status_items := [{"kind": "field", "index": 3, "label": "seconds"}]
	var status_filtered: Array = logic_dialog.call(
		"_filter_layout", "status", ["false", "@status-boss", "unit", "10"], status_items
	)
	_check(status_filtered.is_empty(),
		"permanent status hides the seconds field")
	var timed_filtered: Array = logic_dialog.call(
		"_filter_layout", "status", ["false", "@status-wet", "unit", "10"], status_items
	)
	_check(timed_filtered.size() == 1,
		"timed status keeps the seconds field")
	status_stub.free()
	logic_dialog.free()

	# `EffectsDialog` is registered and its grid lists every `LogicFx` entry.
	var effect_stub := LogicPickerStub.new()
	effect_stub.name = "MindLogic"
	spine.add_child(effect_stub)
	var effects_dialog: Control = load("res://scenes/ui/dialogs/effects_dialog.tscn").instantiate()
	root.add_child(effects_dialog)
	effects_dialog.call("shown")
	_check((effects_dialog.get("_cells") as Array).size() == 3,
		"effects dialog builds one cell per LogicFx entry (got %s)"
			% (effects_dialog.get("_cells") as Array).size())
	effects_dialog.free()
	effect_stub.free()

	_check_hint_table()

	print("UICHECK: failed=", _failures)
	quit(1 if _failures > 0 else 0)


## L05 T4 (PAR-0302): the table over every `HintsFragment.DefaultHint`
## (`HintsFragment.java:169-304`). Each desktop hint must be in the port's
## `HINTS`, read only producer-backed state, show for a fixture that satisfies
## Java's `shown` predicate and stay hidden on an empty state.
## `respawn` is `visibleMobile` and is not part of the desktop port.
func _check_hint_table() -> void:
	const JAVA_HINTS := [
		"desktopMove", "zoom", "breaking", "desktopShoot", "depositItems", "desktopPause",
		"unitControl", "unitSelectControl", "respawn", "launch", "schematicSelect",
		"conveyorPathfind", "boost", "blockInfo", "derelict", "payloadPickup", "payloadDrop",
		"waveFire", "rebuildSelect", "guardian", "cannotUpgrade", "factoryControl",
		"coreUpgrade", "serpuloCoreZone", "presetLaunch", "presetDifficulty", "coreIncinerate",
	]
	const MOBILE_ONLY := ["respawn"]
	var alive := {"player_dead": false, "serpulo": true}
	# name -> [sector state, hint state (merged over `alive`), selected block]
	var fixtures := {
		"desktopMove": [{}, {}, ""],
		"zoom": [{}, {}, ""],
		"breaking": [{"preset": "groundZero"}, {"conveyors": 6}, ""],
		"desktopShoot": [{}, {"enemies": 1}, ""],
		"depositItems": [{}, {"player_has_item": true}, ""],
		"desktopPause": [{"preset": "groundZero", "wave": 2}, {}, ""],
		"unitControl": [{}, {"team_units": 3}, ""],
		"unitSelectControl": [{}, {"team_units": 4, "fungal_pass_saved": false}, ""],
		"launch": [{"preset": "onset", "captured": true}, {}, ""],
		"schematicSelect": [{}, {"placed": ["router"]}, ""],
		"conveyorPathfind": [{}, {}, "titanium-conveyor"],
		"boost": [{}, {"player_can_boost": true}, ""],
		"blockInfo": [{}, {}, "graphite-press"],
		"derelict": [{}, {"events": ["derelictmouse"]}, ""],
		"payloadPickup": [{}, {"player_payload": 0}, ""],
		"payloadDrop": [{}, {"player_payload": 1}, ""],
		"waveFire": [{}, {"events": ["fire"]}, ""],
		"rebuildSelect": [{}, {"team_rebuild_plans": 10}, ""],
		"guardian": [{}, {"boss": {"count": 1, "first": {"armor": 4.0, "flying": false}}}, ""],
		"cannotUpgrade": [{}, {"events": ["cannotupgrade"]}, ""],
		"factoryControl": [{}, {"fabricators": 1}, ""],
		"coreUpgrade": [{"campaign": true, "planet": "serpulo"},
				{"core_block": "core-shard", "core_has_foundation_cost": true}, ""],
		"serpuloCoreZone": [{"campaign": true, "planet": "serpulo"},
				{"core_zone_present": true, "attack_mode": false}, ""],
		"presetLaunch": [{"campaign": true, "preset": "", "sector": 15}, {}, ""],
		"presetDifficulty": [{"campaign": true, "preset": ""},
				{"sector_threat": 0.5, "tar_fields_captured": false}, ""],
		"coreIncinerate": [{"campaign": true},
				{"core_block": "core-shard", "core_copper": 3990, "core_capacity": 4000}, ""],
	}
	# Every key the fixtures read must come from `MindSimHost.hint_state_json`
	# (`sim_host.rs` `hint_state_value`) or the campaign sector state.
	const PRODUCED := [
		"placed", "events", "player_dead", "player_has_item", "player_can_boost",
		"player_spawned_by_core", "player_payload", "team_units", "enemies", "conveyors",
		"core_block", "core_copper", "core_capacity", "core_has_foundation_cost", "cores",
		"fabricators", "extinguishers", "core_zone_present", "cores_destroyed", "boss",
		"team_rebuild_plans", "serpulo", "fungal_pass_saved", "tar_fields_captured",
		"sector_threat", "attack_mode",
	]
	var stub := GDScript.new()
	stub.source_code = "extends Node\nvar block := \"\"\nfunc selected_block() -> String:\n\treturn block\n"
	stub.reload()
	# Earlier checks may already have built `/root/Spine` (and a SimHost):
	# reuse it and park the real host while the stub answers `selected_block`.
	var spine := root.get_node_or_null("Spine")
	var own_spine := spine == null
	if own_spine:
		spine = Node.new()
		spine.name = "Spine"
		root.add_child(spine)
	var parked := spine.get_node_or_null("SimHost")
	if parked != null:
		parked.name = "SimHostParked"
	var host := Node.new()
	host.name = "SimHost"
	host.set_script(stub)
	spine.add_child(host)
	var hints_script: GDScript = load("res://ui/fragments/hints_fragment.gd")
	var fragment: Control = hints_script.new()
	spine.add_child(fragment)
	var port_hints: Array = hints_script.get_script_constant_map().get("HINTS", [])
	for hint_name in JAVA_HINTS:
		if hint_name in MOBILE_ONLY:
			_check(not port_hints.has(hint_name), "mobile-only hint %s stays out of the desktop HINTS" % hint_name)
			continue
		_check(port_hints.has(hint_name), "hint %s is in HINTS" % hint_name)
		_check(fixtures.has(hint_name), "hint %s has a table fixture" % hint_name)
		if not fixtures.has(hint_name):
			continue
		var row: Array = fixtures[hint_name]
		var hint_state: Dictionary = alive.duplicate()
		hint_state.merge(row[1], true)
		for key in (row[1] as Dictionary).keys():
			_check(PRODUCED.has(key), "hint %s reads produced key %s" % [hint_name, key])
		host.set("block", row[2])
		_check(bool(fragment.call("_hint_shown", hint_name, row[0], hint_state)),
			"hint %s shows for its Java predicate" % hint_name)
		host.set("block", "")
		var always: bool = hint_name == "desktopMove" or hint_name == "zoom"
		_check(always or not bool(fragment.call("_hint_shown", hint_name, {}, {})),
			"hint %s stays hidden on an empty state" % hint_name)
	# `presetLaunch` completes only on Frozen Forest (`HintsFragment.java:289-292`).
	_check(bool(fragment.call("_hint_complete", "presetLaunch", {"campaign": true, "preset": "frozenForest"}, {})),
		"presetLaunch completes on frozenForest")
	_check(not bool(fragment.call("_hint_complete", "presetLaunch", {"campaign": true, "preset": "groundZero"}, {})),
		"presetLaunch does not complete on another preset")
	fragment.free()
	host.free()
	if parked != null:
		parked.name = "SimHost"
	if own_spine:
		spine.free()

