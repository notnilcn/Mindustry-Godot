## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/CampaignCompleteDialog.java
##         (plan 14 §3.4, M3).
##
## Campaign completion banner. The planet/playtime data is plan 12; the M3 shell
## takes the planet name/color and playtime from the dialog context and offers
## the menu/continue exits (`shouldPause = true`).

extends MindDialog

var _table: MindTable = null


func _ready() -> void:
	should_pause = true
	super._ready()
	_table = content_table()
	add_close_button()
	buttons.add_child(_make_button("@continue", hide_dialog))


func shown() -> void:
	if _table == null:
		return
	_table.clear_children()
	var planet := str(_context.get("planet", ""))
	var color := str(_context.get("color", "ffd37f"))
	var message := "campaign.complete"
	if not planet.is_empty():
		message = "[#%s]%s[]" % [color, planet]
	# code-instantiated: the completion text is data-driven from the campaign state.
	_table.add(MindWidgets.label(message)).grow_x_axis().pad(8)
	_table.row()
	var playtime := str(_context.get("playtime", ""))
	if not playtime.is_empty():
		_table.add(MindWidgets.label("campaign.playtime %s" % playtime)).pad(4)
		_table.row()


func _make_button(text: String, callback: Callable) -> Button:
	var button := MindWidgets.button(_t(text))
	button.pressed.connect(callback)
	return button
