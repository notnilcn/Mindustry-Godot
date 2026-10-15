## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/MapPlayDialog.java (plan 14 M5).
##
## Gamemode selection + Customize + play/playtest actions. The selected mode,
## optional custom rules JSON (context `rules`) and the playtest flag are handed
## to `MindCampaign.start_map_mode` (falling back to `set_rules_json` +
## `start_map`); the play flow is plan 12. `Gamemode.editor` is hidden upstream.

extends MindDialog

## `Gamemode` names in menu order (plan 12).
const MODES := ["survival", "sandbox", "attack", "pvp", "editor"]

var _map_name := ""
var _mode := "survival"
var _rules_json := ""
var _mode_buttons: Array[Button] = []
var _button_modes: Array[String] = []


func _ready() -> void:
	should_pause = false
	super._ready()
	_build()
	add_close_button()


func set_context_json(json_text: String) -> void:
	super.set_context_json(json_text)
	_map_name = str(context().get("map", ""))
	_rules_json = str(context().get("rules", ""))
	var requested := str(context().get("mode", ""))
	if MODES.has(requested):
		_select_mode(requested)


func _build() -> void:
	var root := content_table()
	root.add(MindWidgets.styled_label(_map_name if not _map_name.is_empty() else "@map.play", "techLabel")).grow_x_axis().pad(6)
	root.row()
	root.add(MindWidgets.label(_t("@level.mode"))).grow_x_axis().pad(4)
	root.row()
	# code-instantiated: mode buttons are the plan-12 Gamemode list (hidden modes skipped).
	for mode in MODES:
		if mode == "editor":
			continue
		var button := MindWidgets.button(_t("@mode.%s.name" % mode))
		button.toggle_mode = true
		button.button_pressed = mode == _mode
		button.pressed.connect(_select_mode.bind(mode))
		root.add(button).grow_x_axis().pad(2)
		root.row()
		_mode_buttons.append(button)
		_button_modes.append(mode)
	var customize := MindWidgets.button(_t("@customize"))
	customize.pressed.connect(_open_rules)
	root.add(customize).grow_x_axis().pad(2)
	root.row()
	var playtest := MindWidgets.button(_t("@editor.playtest"))
	playtest.pressed.connect(func() -> void: _play(_mode, true))
	root.add(playtest).grow_x_axis().pad(2)
	add_button(_t("@play"), func() -> void: _play(_mode, false), "play", 210.0)


func _select_mode(mode: String) -> void:
	_mode = mode
	for index in _mode_buttons.size():
		_mode_buttons[index].button_pressed = _button_modes[index] == mode


## `CustomRulesDialog`: edits `rules`, then `playMap(map, rules, playtesting)`.
func _open_rules() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("open_dialog"):
		ui.call(
			"open_dialog",
			"custom_rules",
			JSON.stringify({"map": _map_name, "mode": _mode, "rules": _rules_json})
		)


## `control.playMap(map, rules, playtesting)` + `ui.custom.hide()`: starts the
## selected map with the chosen gamemode/rules and shows the game view.
func _play(mode: String, playtest: bool) -> void:
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	var started := false
	if campaign != null and not _map_name.is_empty():
		# `start_map_mode` loads the world, applies the gamemode + custom rules
		# and starts the session in one call (`Control.playMap`).
		started = bool(campaign.call("start_map_mode", _map_name, mode, playtest, _rules_json))
	_close_stack()
	if started:
		var ui_root := get_node_or_null("/root/Spine/Ui/UiRoot")
		if ui_root != null and ui_root.has_method("set_menu_visible"):
			ui_root.call("set_menu_visible", false)


func _close_stack() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		hide_dialog()
		return
	ui.call("close_dialog", "map_play")
	ui.call("close_dialog", "custom")
