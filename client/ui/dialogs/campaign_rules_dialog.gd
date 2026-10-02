## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/CampaignRulesDialog.java
##         (plan 14 §3.4, M3).
##
## Difficulty + campaign rule toggles. `CampaignRules` is plan 12; the M3 shell
## renders the difficulty buttons and the vanilla toggle list, reading/writing
## through the dialog context.

extends MindDialog

signal rule_changed(key: String, value: Variant)

## `Difficulty.all` order (plan 12).
const DIFFICULTIES := ["easy", "normal", "hard"]
## Vanilla campaign toggles (`@rules.*`) in source order.
const RULES := ["invasions", "fog", "hidespawns", "randomwaveai", "pauseDisabled"]

var _table: MindTable = null


func _ready() -> void:
	set_title_key("@campaign.difficulty")
	should_pause = false
	super._ready()
	_table = content_table()
	add_close_button()


func shown() -> void:
	if _table == null:
		return
	_table.clear_children()
	# code-instantiated: difficulty buttons are the data-driven Difficulty list.
	var diff_row := HBoxContainer.new()
	var difficulty := str(_context.get("difficulty", "normal"))
	for name in DIFFICULTIES:
		var button := Button.new()
		button.text = _t("@difficulty.%s" % name)
		button.theme_type_variation = "flatTogglet"
		button.toggle_mode = true
		button.button_pressed = name == difficulty
		button.pressed.connect(_set_difficulty.bind(name))
		diff_row.add_child(button)
	_table.add(diff_row).grow_x_axis().pad(4)
	_table.row()
	var rules: Dictionary = _context.get("rules", {})
	for key in RULES:
		# code-instantiated: rule toggles come from the plan-12 CampaignRules set.
		var check := MindWidgets.check("@rules.%s" % key, bool(rules.get(key, false)), _toggle.bind(key))
		_table.add(check).grow_x_axis().pad(2).left()
		_table.row()


func _set_difficulty(name: String) -> void:
	rule_changed.emit("difficulty", name)


func _toggle(key: String, value: bool) -> void:
	rule_changed.emit(key, value)
