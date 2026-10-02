## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/GameOverDialog.java (plan 14 §3.4, M3).
##
## Game-over summary. `GameStats` and the campaign continue path are plan 12;
## the M3 shell reads the winner and stat rows from the dialog context, hides the
## HUD while shown (`hudfrag.shown = false`) and offers the menu/continue exits.

extends MindDialog

var _table: MindTable = null
var _on_menu: Callable = Callable()


func _ready() -> void:
	set_title_key("@gameover")
	should_pause = false
	super._ready()
	_table = content_table()
	buttons.add_child(_make_button("@menu", _menu))


func shown() -> void:
	_hud_set(false)
	_rebuild()


func hidden() -> void:
	_hud_set(true)


func _rebuild() -> void:
	if _table == null:
		return
	_table.clear_children()
	var winner := str(_context.get("winner", ""))
	var headline := str(_context.get("headline", ""))
	if headline.is_empty():
		headline = "@sector.lost" if not winner.is_empty() else "@gameover"
	# code-instantiated: the headline is data-driven (campaign sector / PvP winner).
	var label := MindWidgets.styled_label(headline, "techLabel")
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_table.add(label).grow_x_axis().pad(6)
	_table.row()
	var stats: Dictionary = _context.get("stats", {})
	for key in stats:
		# code-instantiated: stat rows come from plan-12 GameStats.
		_table.add(MindWidgets.label("%s: [accent]%s" % [_t("stats.%s" % str(key)), str(stats[key])])).grow_x_axis().pad(3).left()
		_table.row()


func _menu() -> void:
	hide_dialog()
	_on_menu.call()


func _make_button(text: String, callback: Callable) -> Button:
	# code-instantiated: the exit button is bound to the caller's flow.
	var button := MindWidgets.button(_t(text))
	button.pressed.connect(callback)
	return button


func _hud_set(visible: bool) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("hud_set_visible", visible)
