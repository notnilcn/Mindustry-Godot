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


## `GameOverDialog.rebuild`: headline, then the `Styles.black3` stats pane with
## one 50 px accent row per stat in Java order, each sliding in after
## `0.05 * i` s with the value counting up (`StatLabel`).
const STAT_ROWS: Array[Array] = [
	["stats.wave", "wave"],
	["stats.unitsCreated", "unitsCreated"],
	["stats.enemiesDestroyed", "enemiesDestroyed"],
	["stats.built", "built"],
	["stats.destroyed", "destroyed"],
	["stats.deconstructed", "deconstructed"],
]


func _rebuild() -> void:
	if _table == null:
		return
	_table.clear_children()
	# code-instantiated: the headline is data-driven (campaign sector / PvP
	# winner) and the stat rows come from the `game_stats_json` context.
	var label := MindWidgets.styled_label(_headline(), "techLabel")
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_table.add(label).grow_x_axis().pad(6)
	_table.row()
	var stats: Dictionary = _context.get("stats", {})
	if bool(stats.get("waves", false)):
		for index in STAT_ROWS.size():
			_add_stat_row(stats, STAT_ROWS[index][0], str(STAT_ROWS[index][1]), index)
	_build_buttons()


## One `StatLabel`: accent background 50 px tall, name left, value right; the
## row's scale.x animates 0->1 over 0.3 s `pow3Out` after `0.05 * i` s.
func _add_stat_row(stats: Dictionary, key: String, field: String, index: int) -> void:
	# code-instantiated: one row per GameStats entry (T16).
	var panel := PanelContainer.new()
	panel.custom_minimum_size = Vector2(0, 50)
	var box := StyleBoxFlat.new()
	box.bg_color = Color("ffd37f")
	box.content_margin_left = 8.0
	box.content_margin_right = 8.0
	panel.add_theme_stylebox_override("panel", box)
	panel.pivot_offset = Vector2.ZERO
	panel.scale.x = 0.0
	var row := HBoxContainer.new()
	var name_label := MindWidgets.styled_label(_t("@" + key), "outlineLabel")
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_child(name_label)
	var value_label := MindWidgets.styled_label("0", "outlineLabel")
	value_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	row.add_child(value_label)
	panel.add_child(row)
	_table.add(panel).grow_x_axis().pad(2)
	_table.row()
	var target := float(stats.get(field, 0))
	var tween := create_tween()
	tween.tween_interval(0.05 * float(index))
	tween.set_trans(Tween.TRANS_CUBIC).set_ease(Tween.EASE_OUT)
	tween.tween_property(panel, "scale:x", 1.0, 0.3)
	tween.parallel().tween_method(
		func(value: float) -> void: value_label.text = str(int(value)),
		0.0,
		target,
		0.3
	)


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
		add_button(_t("@menu"), _menu_pressed, "", 140.0)


func _continue_pressed() -> void:
	_close_pressed()
	var ui := _ui()
	if ui != null:
		ui.call("open_dialog", "planet", "")


## Non-campaign `@menu`: park the sim paused behind the standalone menu.
func _menu_pressed() -> void:
	_close_pressed()
	# `if(!ui.paused.checkPlaytest()) logic.reset();` (`GameOverDialog.java:134-139`):
	# a playtest that ended in game over returns to the editor.
	var ui_root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui_root != null and ui_root.has_method("check_playtest") and bool(ui_root.call("check_playtest")):
		return
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
