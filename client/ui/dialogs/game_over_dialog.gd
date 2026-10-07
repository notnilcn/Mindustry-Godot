## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/GameOverDialog.java (43-140).
##
## Game-over summary. Shown by the in-game HUD watcher (`hud_fragment.gd`) when
## the campaign reports `gameOver` (loss) or a sector capture (win) through
## `MindHud`/`MindCampaign`. `GameStats` rows and the headline come from the
## dialog context; campaign continues to the planet map, other modes return to
## the standalone menu. Visibility/stack are owned by `MindUi`.

extends MindDialog

var _table: MindTable = null


func _ready() -> void:
	set_title_key("@gameover")
	should_pause = false
	super._ready()
	_table = content_table()


func shown() -> void:
	# Deferred: `shown()` runs inside `MindUi.open_dialog`, which mutably binds
	# `MindUi`; the HUD toggle must run after that call returns.
	_hud_set.call_deferred(false)
	_rebuild()


func hidden() -> void:
	# Deferred for the same reason (`MindUi.close_dialog` binds `MindUi`).
	_hud_set.call_deferred(true)


func _rebuild() -> void:
	if _table == null:
		return
	_table.clear_children()
	# code-instantiated: the headline is data-driven (campaign sector / PvP
	# winner) and the stat rows come from the GameStats context map.
	var label := MindWidgets.styled_label(_headline(), "techLabel")
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_table.add(label).grow_x_axis().pad(6)
	_table.row()
	var stats: Dictionary = _context.get("stats", {})
	for key in stats:
		_table.add(MindWidgets.label("%s: [accent]%s" % [_t("stats.%s" % str(key)), str(stats[key])])).grow_x_axis().pad(3).left()
		_table.row()
	_build_buttons()


## `GameOverDialog.rebuild` headline: campaign capture/loss, PvP winner, else
## the plain game-over title. The sector name is formatted into `sector.lost`.
func _headline() -> String:
	var sector := str(_context.get("sector", ""))
	if _is_campaign():
		if bool(_context.get("captured", false)):
			return MindWidgets.markup("@sector.curcapture")
		if not sector.is_empty():
			return MindWidgets.markup_format("@sector.lost", [sector])
		return MindWidgets.markup("@sector.lost")
	var winner := str(_context.get("winner", ""))
	if not winner.is_empty():
		return MindWidgets.markup_format("@gameover.pvp", [winner])
	return MindWidgets.markup("@gameover")


## `GameOverDialog.java:101-140`: campaign gets `@continue` (planet map), other
## modes get `@menu` (the standalone menu).
func _build_buttons() -> void:
	clear_buttons()
	if _is_campaign():
		add_button(_t("@continue"), _continue_pressed, "map", 170.0)
	else:
		add_button(_t("@menu"), _menu_pressed, "", 170.0)


func _continue_pressed() -> void:
	_close_pressed()
	var ui := _ui()
	if ui != null:
		ui.call("open_dialog", "planet", "")


## Non-campaign `@menu`: park the sim paused behind the standalone menu.
func _menu_pressed() -> void:
	_close_pressed()
	var host := get_node_or_null("/root/Spine/SimHost")
	if host != null and host.has_method("set_paused"):
		host.call("set_paused", true)
	var root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if root != null and root.has_method("set_menu_visible"):
		root.call("set_menu_visible", true)


func _is_campaign() -> bool:
	if _context.has("campaign"):
		return bool(_context.get("campaign"))
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign == null or not campaign.has_method("get_sector_state"):
		return false
	var state: Variant = campaign.call("get_sector_state")
	return state is Dictionary and bool(state.get("campaign", false))


func _hud_set(visible: bool) -> void:
	var ui := _ui()
	if ui != null:
		ui.call("hud_set_visible", visible)


func _ui() -> Node:
	return get_node_or_null("/root/MindUi")
