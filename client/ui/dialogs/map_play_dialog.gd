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
		button.pressed.connect(func() -> void: play_requested.emit(mode, false))
		root.add(button).grow_x_axis().pad(2)
		root.row()
	var playtest := MindWidgets.button(_t("@playtest"))
	playtest.pressed.connect(func() -> void: play_requested.emit("survival", true))
	root.add(playtest).grow_x_axis().pad(2)
