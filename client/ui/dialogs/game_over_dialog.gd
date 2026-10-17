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
	# `GameOverDialog` (`GameOverDialog.java:25-30`): `super("@gameover")` then
	# `titleTable.remove()` — the content headline *is* the dialog's only title,
	# so no title row (and no accent rule) is built.
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
## `cont.table(...).center().minWidth(370).maxSize(600f, 550f).grow()`
## (`GameOverDialog.java:102`): the stats block is a centred 600 px column.
const BLOCK_WIDTH := 600.0
## `Pal.darkestGray` (`graphics/Pal.java:61`): the colour every row animates to.
const DARKEST_GRAY := Color(0.1, 0.1, 0.1)


func _rebuild() -> void:
	if _table == null:
		return
	_table.clear_children()
	# code-instantiated: the headline and stat rows are data-driven
	# (campaign sector / PvP winner) and come from the `game_stats_json` context.
	var block := VBoxContainer.new()
	block.custom_minimum_size.x = BLOCK_WIDTH
	block.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	block.add_theme_constant_override("separation", 0)
	# No `grow_x_axis` on the cell: the block keeps its own 600 px width and the
	# table's default align centres it (Java's `.center()`).
	_table.add(block)
	_table.row()
	block.add_child(_padded(_headline_label(), 6.0))

	var stats: Dictionary = _context.get("stats", {})
	var pane_holder := _padded(_stats_pane(), 12.0)
	block.add_child(pane_holder)
	var pane := pane_holder.get_child(0) as PanelContainer
	var rows := pane.get_child(0) as VBoxContainer
	# `if(state.rules.waves) addStat(...)` (`GameOverDialog.java:79`): only the
	# wave row is gated on waves; every other row always shows.
	for index in STAT_ROWS.size():
		if str(STAT_ROWS[index][1]) == "wave" and not bool(stats.get("waves", false)):
			continue
		_add_stat_row(rows, stats, STAT_ROWS[index][0], str(STAT_ROWS[index][1]), index)
	# `if(control.saves.getCurrent() != null)` (`GameOverDialog.java:92-96`): the
	# Time Played row only exists while a save slot is current.
	var playtime := _current_playtime_ms()
	if playtime >= 0:
		rows.add_child(_playtime_row(playtime))
	_build_buttons()


## `cont.pane(p -> ... p.margin(13f).setBackground(Styles.black3))`: the stats
## pane, `Tex.black3` over the dialog backdrop with a 13 px inner margin.
func _stats_pane() -> PanelContainer:
	# code-instantiated: the pane's rows come from the stats context.
	var pane := PanelContainer.new()
	var box := StyleBoxFlat.new()
	box.bg_color = MindStyles.BLACK_3
	box.content_margin_left = 13.0
	box.content_margin_right = 13.0
	box.content_margin_top = 13.0
	box.content_margin_bottom = 13.0
	pane.add_theme_stylebox_override("panel", box)
	var rows := VBoxContainer.new()
	rows.add_theme_constant_override("separation", 0)
	pane.add_child(rows)
	return pane


## A child wrapped in a uniform margin (`Cell.pad`).
func _padded(child: Control, margin: float) -> MarginContainer:
	# code-instantiated: padding wrapper for a data-built cell.
	var holder := MarginContainer.new()
	holder.add_theme_constant_override("margin_left", int(margin))
	holder.add_theme_constant_override("margin_right", int(margin))
	holder.add_theme_constant_override("margin_top", int(margin))
	holder.add_theme_constant_override("margin_bottom", int(margin))
	holder.add_child(child)
	return holder


func _headline_label() -> MindLabel:
	var label := MindWidgets.styled_label(_headline(), "techLabel")
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	return label


## One `StatLabel` (`GameOverDialog.java:143-184`): a 50 px `Tex.whiteui` bar
## tinted `Pal.accent`, name left / value right, scaled in from x=0 after
## `0.05 * i` s while the tint animates to `Pal.darkestGray` over 0.3 s
## (`pow3Out`); the value then counts up from 0 over 60 frames — linearly below
## 10, `Interp.slowFast` (= `pow2In` = `a * a`, `Interp.java:30`) above.
func _add_stat_row(
	parent: VBoxContainer, stats: Dictionary, key: String, field: String, index: int
) -> void:
	# code-instantiated: one row per GameStats entry (T16).
	var panel := PanelContainer.new()
	panel.custom_minimum_size = Vector2(0, 50)
	var box := StyleBoxFlat.new()
	box.bg_color = MindStyles.ACCENT
	box.content_margin_left = 5.0
	box.content_margin_right = 5.0
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
	parent.add_child(_padded(panel, 5.0))
	var target := float(stats.get(field, 0))
	var delay := 0.05 * float(index)
	# Scale + colour: `parallel(scaleTo(1, 1, 0.3f, pow3Out), color(...))`.
	var intro := create_tween()
	intro.tween_interval(delay)
	intro.set_trans(Tween.TRANS_CUBIC).set_ease(Tween.EASE_OUT)
	intro.tween_property(panel, "scale:x", 1.0, 0.3)
	intro.parallel().tween_method(
		func(t: float) -> void: box.bg_color = MindStyles.ACCENT.lerp(DARKEST_GRAY, t),
		0.0,
		1.0,
		0.3
	)
	# Count-up: `sequence(delay(0.3f), run(...))` after the same initial delay.
	var counter := create_tween()
	counter.tween_interval(delay + 0.3)
	if target >= 10.0:
		counter.set_trans(Tween.TRANS_QUAD).set_ease(Tween.EASE_IN)
	else:
		counter.set_trans(Tween.TRANS_LINEAR).set_ease(Tween.EASE_IN_OUT)
	counter.tween_method(
		func(value: float) -> void: value_label.text = str(int(value)),
		0.0,
		target,
		1.0
	)


## `pt.add(new FLabel(Core.bundle.get("stats.playtime"))).left().pad(5).growX()`
## + `"[accent]" + getPlayTime()` right (`GameOverDialog.java:91-97`).
func _playtime_row(millis: int) -> Control:
	# code-instantiated: the row follows the current save slot's playtime.
	var row := HBoxContainer.new()
	var name_label := MindWidgets.styled_label(_t("@stats.playtime"), "outlineLabel")
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_child(name_label)
	var value_label := MindWidgets.styled_label(
		MindWidgets.markup("[accent]" + _format_playtime(millis)), "outlineLabel"
	)
	value_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	row.add_child(value_label)
	return _padded(row, 5.0)


## `Strings.formatMillis` (`Saves.SaveSlot.getPlayTime`,
## `arc/util/Strings.java:814-827`): hours unpadded, minutes/seconds two digits
## ("0:05:24"), the same shape as the reference frame.
func _format_playtime(millis: int) -> String:
	var sign := "-" if millis < 0 else ""
	var value := absi(millis)
	return "%s%d:%02d:%02d" % [
		sign, value / 3600000, (value % 3600000) / 60000, (value % 60000) / 1000
	]


## `SaveSlot.getPlayTime()` through the campaign facade; `-1` when no slot is
## current (Java hides the row then).
func _current_playtime_ms() -> int:
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign == null or not campaign.has_method("current_save_playtime_ms"):
		return -1
	return int(campaign.call("current_save_playtime_ms"))


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
## modes get `@menu` (the standalone menu); both are 60 px tall.
func _build_buttons() -> void:
	clear_buttons()
	if _is_campaign():
		add_button(_t("@continue"), _continue_pressed, "map", 170.0).custom_minimum_size.y = 60.0
	else:
		add_button(_t("@menu"), _menu_pressed, "", 140.0).custom_minimum_size.y = 60.0


func _continue_pressed() -> void:
	_close_pressed()
	var ui := _ui()
	if ui != null:
		ui.call("open_dialog", "planet", "")


## Non-campaign `@menu` (`GameOverDialog.java:134-138`):
## `if(!ui.paused.checkPlaytest()) logic.reset();` and the standalone menu.
func _menu_pressed() -> void:
	_close_pressed()
	# `if(!ui.paused.checkPlaytest()) logic.reset();` (`GameOverDialog.java:134-139`):
	# a playtest that ended in game over returns to the editor.
	var ui_root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui_root != null and ui_root.has_method("check_playtest") and bool(ui_root.call("check_playtest")):
		return
	if ui_root != null and ui_root.has_method("return_to_menu"):
		ui_root.call("return_to_menu")


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
