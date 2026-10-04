## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/MapPlayDialog.java (plan 14 M5).
##
## Gamemode buttons + help panel + play/playtest actions. Gamemode help text is
## plan-12 `Gamemode`; the play flow is plan 12/19. The M5 shell renders the
## map summary and the three mode buttons and emits `play_requested`.

extends MindDialog

## Emitted with `(mode, playtest)` when the player starts a map.
signal play_requested(mode: String, playtest: bool)

## `Gamemode` names in menu order (plan 12).
const MODES := ["survival", "sandbox", "attack", "pvp", "editor"]

var _map_name := ""


func _ready() -> void:
	should_pause = false
	super._ready()
	_build()
	add_close_button()


func set_context_json(json_text: String) -> void:
	super.set_context_json(json_text)
	_map_name = str(context().get("map", ""))


func _build() -> void:
	var root := content_table()
	root.add(MindWidgets.styled_label(_map_name if not _map_name.is_empty() else "@map.play", "techLabel")).grow_x_axis().pad(6)
	root.row()
	# code-instantiated: mode buttons are the plan-12 Gamemode list.
	for mode in MODES:
		var button := MindWidgets.button(_t("@mode.%s" % mode))
		button.pressed.connect(_play.bind(mode, false))
		root.add(button).grow_x_axis().pad(2)
		root.row()
	var playtest := MindWidgets.button(_t("@playtest"))
	playtest.pressed.connect(_play.bind("survival", true))
	root.add(playtest).grow_x_axis().pad(2)


## Starts the selected map (`Logic.playMap`) and dismisses the map/custom stack
## so the game view is shown (`control.playMap(...); ui.custom.hide()`).
func _play(mode: String, playtest: bool) -> void:
	play_requested.emit(mode, playtest)
	var started := false
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign != null and campaign.has_method("start_map") and not _map_name.is_empty():
		started = bool(campaign.call("start_map", _map_name))
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
