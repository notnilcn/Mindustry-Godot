## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/CampaignCompleteDialog.java
##         (plan 14 §3.4, M3).
##
## Campaign completion banner. Receives the `CampaignCompleteView` values through
## the dialog context (planet/localized/color/playtime/captures) and offers the
## `@continue` and `@menu` exits (`shouldPause = true`).

extends MindDialog

var _table: MindTable = null


func _ready() -> void:
	should_pause = true
	super._ready()
	_table = content_table()
	add_close_button()
	buttons.add_child(_make_button("@continue", _close_pressed))
	buttons.add_child(_make_button("@menu", _exit_to_menu))


func shown() -> void:
	if _table == null:
		return
	_table.clear_children()
	var view := _context
	if view.is_empty():
		view = campaign_views().get("complete", {})
	var planet := str(view.get("localized", view.get("planet", "")))
	var color := str(view.get("color", "ffd37f"))
	var planet_markup := planet
	if not planet.is_empty():
		planet_markup = "[#%s]%s[]" % [color, planet]
	# code-instantiated: the completion text is data-driven from the campaign state.
	var message := MindWidgets.label(MindWidgets.markup_format("@campaign.complete", [planet_markup]))
	message.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_table.add(message).grow_x_axis().pad(8)
	_table.row()
	var playtime := int(view.get("playtime_ms", 0))
	if playtime > 0:
		_table.add(
			MindWidgets.label(MindWidgets.markup_format("@campaign.playtime", [format_time_ms(playtime)]))
		).pad(4)
		_table.row()


## `CampaignCompleteDialog.@menu` exit; the exit-save half is the save/load
## plumbing (out of scope), so this returns to the standalone menu directly.
func _exit_to_menu() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("close_dialog", "campaign_complete")
		ui.call("close_dialog", "planet")
	var ui_root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui_root != null and ui_root.has_method("set_menu_visible"):
		ui_root.call("set_menu_visible", true)


func _make_button(text: String, callback: Callable) -> Button:
	var button := MindWidgets.button(_t(text))
	button.pressed.connect(callback)
	return button
