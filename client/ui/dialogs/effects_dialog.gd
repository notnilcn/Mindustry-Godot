## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/EffectsDialog.java
##
## `ui.effects` — the `LogicFx` effect picker used by `EffectStatement`. A
## scrolling grid of 280 px cells (`EffectsDialog.setup`), each an animated
## preview over the effect's `name.token.<name>` label. Selecting a cell emits
## `effect_selected` and closes the dialog.
##
## Deviation: the Java `EffectCell` renders the live effect through `Core.batch`
## and sizes its cell from `calculateSize(entry)`. The port has no UI-side
## effect renderer, so the cell draws Mindustry's animated HSV wash over the
## alpha checker and leaves the particle preview to a later effect-render seam.

extends MindDialog

## Emitted with the picked `LogicFx` entry name (`EffectsDialog.show(listener)`).
signal effect_selected(name: String)

const CELL_SIZE := 280.0
const CELL_GAP := 12.0

## `LogicFx.entries()` rows from `MindLogic.effect_entries()`.
var _entries: Array = []
## Built preview cells (widget-check seam).
var _cells: Array = []


func _ready() -> void:
	should_pause = false
	super._ready()
	add_close_button(210.0)


func shown() -> void:
	set_title_text("Effects")
	_entries = []
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic != null and logic.has_method("effect_entries"):
		var rows: Variant = logic.call("effect_entries")
		if rows is Array:
			_entries = rows
	_build()


func hidden() -> void:
	_cells.clear()


## `EffectsDialog.setup`: `cols = max(1, width / (size + 12))`, a scrollable
## grid of 280 px cells.
func _build() -> void:
	if cont == null:
		return
	for child in cont.get_children():
		cont.remove_child(child)
		child.queue_free()
	_cells.clear()
	var width := get_viewport_rect().size.x
	var columns := maxi(1, int(width / (CELL_SIZE + CELL_GAP)))
	# code-instantiated: the effect grid is data-driven (`LogicFx.entries()`).
	var scroll := ScrollContainer.new()
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	scroll.custom_minimum_size = Vector2(minf(width * 0.9, 1200.0), 520.0)
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	cont.add_child(scroll)
	# code-instantiated: one cell per LogicFx entry.
	var grid := GridContainer.new()
	grid.columns = columns
	grid.add_theme_constant_override("h_separation", int(CELL_GAP))
	grid.add_theme_constant_override("v_separation", int(CELL_GAP))
	scroll.add_child(grid)
	for row_value in _entries:
		if not (row_value is Dictionary):
			continue
		var row: Dictionary = row_value
		var cell := _effect_cell(str(row.get("name", "")))
		grid.add_child(cell)
		_cells.append(cell)
	if _entries.is_empty():
		grid.add_child(MindWidgets.label(_t("@none.found")))


## One `EffectsDialog.EffectCell`: a clickable 280 px stack with the animated
## preview and the bottom-left outlined name.
func _effect_cell(name: String) -> Control:
	# code-instantiated: one preview cell per LogicFx entry.
	var button := Button.new()
	button.custom_minimum_size = Vector2(CELL_SIZE, CELL_SIZE)
	button.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	button.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	button.focus_mode = Control.FOCUS_NONE
	for state in ["normal", "hover", "pressed", "disabled", "focus", "hover_pressed"]:
		button.add_theme_stylebox_override(state, StyleBoxEmpty.new())
	var preview := EffectPreview.new()
	preview.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	preview.mouse_filter = Control.MOUSE_FILTER_IGNORE
	button.add_child(preview)
	var label := _outline_label(name)
	label.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_LEFT)
	label.offset_left = 6.0
	label.offset_bottom = -4.0
	label.offset_top = -28.0
	label.offset_right = CELL_SIZE - 6.0
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	button.add_child(label)
	button.tooltip_text = name
	button.pressed.connect(_pick.bind(name))
	return button


## `LStatement.bundle(entry.name)` (`name.token.<name>` fallback: the name).
func _outline_label(name: String) -> Label:
	# code-instantiated: one label per effect cell.
	var label := Label.new()
	var key := "name.token." + name
	var text := _t(key)
	label.text = text if text != key else name
	label.add_theme_color_override("font_color", Color.WHITE)
	label.add_theme_color_override("font_outline_color", Color.BLACK)
	label.add_theme_constant_override("outline_size", 4)
	return label


func _pick(name: String) -> void:
	effect_selected.emit(name)
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("close_dialog"):
		if bool(ui.call("close_dialog", "effects")):
			return
	hide_dialog()


## `EffectsDialog.EffectCell.draw`: `Tex.alphaBg` under
## `color.fromHsv((Time.globalTime * 2f) % 360f, 1f, 1f)` and a black border.
class EffectPreview:
	extends Control

	var _time := 0.0

	func _process(delta: float) -> void:
		_time = fmod(_time + delta, 1000.0)
		queue_redraw()

	func _draw() -> void:
		# `Tex.alphaBg`: a 16 px dark/light checker.
		var cell := 16.0
		var y := 0.0
		var row := 0
		while y < size.y:
			var x := 0.0
			var column := row
			while x < size.x:
				var tint := Color(0.35, 0.35, 0.35, 0.55) if (column % 2) == 0 \
					else Color(0.22, 0.22, 0.22, 0.55)
				draw_rect(Rect2(x, y, cell, cell), tint)
				x += cell
				column += 1
			y += cell
			row += 1
		# `color.fromHsv((Time.globalTime * 2f) % 360f, 1f, 1f)`.
		var hue := fmod(Time.get_ticks_msec() * 0.002, 1.0)
		var wash := Color.from_hsv(hue, 1.0, 1.0, 0.85)
		var centre := size * 0.5
		var radius := minf(size.x, size.y) * 0.28
		draw_circle(centre, radius, wash)
		draw_circle(centre, radius * 0.55, Color(1, 1, 1, 0.35))
		draw_rect(Rect2(Vector2.ZERO, size), Color.BLACK, false, 3.0)
