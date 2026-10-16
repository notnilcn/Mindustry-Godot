## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/world/blocks/units/UnitFactory.java:292-340
##         (`buildConfiguration` command row) and
##         core/src/mindustry/world/blocks/units/Reconstructor.java:215-240
##         (J05 T2).
##
## The unit-command icon row Java builds under a factory's plan grid (40 px
## `clearNoneTogglei` cells, `columns = clamp(units.size, 2, selectionColumns)`
## and a 4 px `Pal.gray` separator) or as the reconstructor's whole panel
## (5 columns of 44 px cells over `Styles.black6`).
##
## A click configures the command (`configure(item)`); the checked cell follows
## `command == item || (command == null && unit.defaultCommand == item)`
## (`UnitFactory.java:314`). Java's click listener never clears the config (the
## group's min-check-0 uncheck is re-checked by the next `update`); a
## reconstructor click additionally closes the panel (`deselect()`, `:230`),
## which the fragment applies from `close_select`.

class_name MindItemCommandRow
extends PanelContainer

## Emitted with the command name on a checked click.
signal picked(command: String)

## `Pal.gray` (`Pal.java:82`) — the factory's separator strip.
const SEPARATOR_COLOR := Color(0x45 / 255.0, 0x45 / 255.0, 0x45 / 255.0)
## `commands.image(Tex.whiteui, Pal.gray).height(4f)` (`UnitFactory.java:307`).
const SEPARATOR_HEIGHT := 4.0
## Java `Icon` field names → `MindIcons` keys where the port's alias table has
## no entry (`UnitCommand.getIcon()` returns the `Icon` drawable).
const ICON_ALIASES := {
	"downOpen": "down-open",
	"upOpen": "up-open",
	"rightOpen": "right-open",
	"leftOpen": "left-open",
}

var _close_select := false


## Whether the fragment closes the panel after a pick
## (`Reconstructor.java:230` `deselect()`).
func close_select() -> bool:
	return _close_select


## Builds the row from the `config_spec` `commands` object.
func build(spec: Dictionary) -> void:
	_close_select = bool(spec.get("close_select", false))
	if bool(spec.get("background", true)):
		var style := StyleBoxFlat.new()
		style.bg_color = MindStyles.BLACK_6
		style.set_content_margin_all(0.0)
		add_theme_stylebox_override("panel", style)
	else:
		add_theme_stylebox_override("panel", StyleBoxEmpty.new())
	# code-instantiated: one toggle per command from the `config_spec` JSON
	# (runtime content; the count is driven by the produced unit type).
	var grid := MindTable.new()
	add_child(grid)
	var columns := maxi(1, int(spec.get("columns", 5)))
	var size := float(spec.get("size", 40.0))
	var group := ButtonGroup.new()
	group.allow_unpress = true
	if bool(spec.get("separator", false)):
		var bar := ColorRect.new()
		bar.color = SEPARATOR_COLOR
		bar.custom_minimum_size = Vector2(0, SEPARATOR_HEIGHT)
		bar.mouse_filter = Control.MOUSE_FILTER_IGNORE
		var cell := grid.add(bar).grow_x_axis()
		cell.colspan = columns
		grid.row()
	var options: Array = spec.get("options", []) if spec.get("options", []) is Array else []
	var index := 0
	for option in options:
		if not (option is Dictionary):
			continue
		var button := _make_button(option, size, group)
		grid.add(button).size(size)
		index += 1
		if index % columns == 0:
			grid.row()
	# `if(list.size < columns)`: empty 40 px cells pad the row
	# (`UnitFactory.java:322-326`).
	if options.size() < columns:
		for _i in range(columns - options.size()):
			var filler := Control.new()
			filler.mouse_filter = Control.MOUSE_FILTER_IGNORE
			grid.add(filler).size(size)
	# `MindTable` layout needs an explicit pass after a runtime rebuild
	# (`mind_table.gd sort_now`), repeated once deferred like the dialogs do.
	grid.sort_now()
	grid.call_deferred("sort_now")


## `UnitFactory.java:311-315`: a size-box `clearNoneTogglei` toggle with the
## command's `Icon` glyph and the localized command name as its tooltip.
func _make_button(option: Dictionary, size: float, group: ButtonGroup) -> Button:
	var name := str(option.get("name", ""))
	var icon_name := str(option.get("icon", ""))
	if not ICON_ALIASES.get(icon_name, "").is_empty():
		icon_name = str(ICON_ALIASES[icon_name])
	var button := MindWidgets.glyph_button(icon_name, "", "clearNoneTogglei")
	button.name = "command-%s" % name
	button.toggle_mode = true
	button.button_group = group
	button.custom_minimum_size = Vector2(size, size)
	button.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	button.tooltip_text = _localized(str(option.get("label", "")), name)
	MindItemSelection.apply_clear_none_toggle(button)
	for child in button.get_children():
		if child is Label:
			# Java draws the `Icon` drawable at the button's image size
			# (`imageSize = size`), so the glyph fills the cell.
			(child as Label).add_theme_font_size_override("font_size", int(size))
	if bool(option.get("checked", false)):
		button.set_pressed_no_signal(true)
	# Java's click listener always configures the command (an uncheck is
	# re-checked by the next `update`), so only the checked transition reports.
	button.toggled.connect(
		func(on: bool) -> void:
			if on:
				picked.emit(name)
	)
	return button


## Resolves a bundle key through `MindAssets` with a raw fallback.
func _localized(key: String, fallback: String) -> String:
	var resolved := key.trim_prefix("@")
	if resolved.is_empty():
		return fallback
	var assets := get_node_or_null("/root/MindAssets")
	if assets == null:
		return fallback
	var value := str(assets.call("bundle_get", resolved))
	if value.is_empty() or value == resolved:
		return fallback
	return value
