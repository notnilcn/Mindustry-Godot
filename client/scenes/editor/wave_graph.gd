## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/WaveGraph.java (plan 19 M5 §3.9/§2.3.7;
## L07 T3 line-graph port, PAR-0432).
##
## Presentation-only: the per-wave series, the axis maxima (`nextStep`) and the
## participating units come from Rust (`MindEditor.wave_graph(from, to)`); this
## Control draws the `Tex.pane` table of `WaveGraph`: the line plot (pan, wheel
## zoom, hover column), the unit legend with its hide toggles, and the
## counts/totals/health mode row.
extends PanelContainer

enum Mode { COUNTS, TOTALS, HEALTH }

const MODE_NAMES := ["counts", "totals", "health"]
## `defaultSpace = Scl.scl(40f)`: the column spacing at zoom 1.
const DEFAULT_SPACE := 40.0
## `Pal.health`.
const HEALTH_COLOR := Color("ff341c")
const STROKE := 3.0

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"

var _editor: Node = null
var _mode := Mode.COUNTS
var _pan := 0.0
var _zoom := 1.0
var _from := 0
var _to := 20
var _last_from := -1
var _last_to := -1
## `rebuild()` state: the Rust series for `_from.._to`.
var _data: Dictionary = {}
## `hidden`: unit names whose lines are hidden.
var _hidden: Dictionary = {}
var _plot: Control = null
var _colors: HBoxContainer = null
var _hide_button: Button = null
var _legend_buttons: Dictionary = {}
var _mode_buttons: Array[Button] = []
var _dragging := false


func _ready() -> void:
	_editor = get_node_or_null(editor_path)
	# `background(Tex.pane)`.
	add_theme_stylebox_override("panel", MindThemeBuilder._nine_patch("pane", 12.0))
	# code-instantiated: the WaveGraph table rows (plot, legend, modes).
	var rows := VBoxContainer.new()
	rows.add_theme_constant_override("separation", 0)
	add_child(rows)
	var plot_margin := MarginContainer.new()
	# `.pad(4).padBottom(10).grow()`.
	for side in ["left", "top", "right"]:
		plot_margin.add_theme_constant_override("margin_" + side, 4)
	plot_margin.add_theme_constant_override("margin_bottom", 10)
	plot_margin.size_flags_vertical = Control.SIZE_EXPAND_FILL
	rows.add_child(plot_margin)
	_plot = Control.new()
	_plot.clip_contents = true
	_plot.mouse_filter = Control.MOUSE_FILTER_STOP
	_plot.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_plot.draw.connect(_draw_plot)
	_plot.gui_input.connect(_plot_input)
	_plot.mouse_exited.connect(_plot.queue_redraw)
	plot_margin.add_child(_plot)
	# `table(t -> colors = t).growX()`.
	_colors = HBoxContainer.new()
	_colors.add_theme_constant_override("separation", 0)
	rows.add_child(_colors)
	# `t.left()` + one `fullTogglet` 130x35 button per `Mode`.
	var modes := HBoxContainer.new()
	modes.add_theme_constant_override("separation", 0)
	rows.add_child(modes)
	for index in MODE_NAMES.size():
		var button := MindWidgets.button(_bundle("wavemode." + MODE_NAMES[index]))
		button.theme_type_variation = "fullTogglet"
		button.toggle_mode = true
		button.button_pressed = index == _mode
		button.custom_minimum_size = Vector2(130.0, 35.0)
		button.pressed.connect(set_mode.bind(index))
		modes.add_child(button)
		_mode_buttons.append(button)


## Forces `rebuild()` on the next draw (the dialog's groups changed).
func refresh() -> void:
	_last_from = -1
	_last_to = -1
	if _plot != null:
		_plot.queue_redraw()


func set_mode(mode: int) -> void:
	_mode = mode as Mode
	for index in _mode_buttons.size():
		_mode_buttons[index].set_pressed_no_signal(index == _mode)
	if _plot != null:
		_plot.queue_redraw()


## `scrolled`: `zoom -= scroll * 2 / 10 * zoom`; `ElementGestureListener.pan`:
## `pan -= deltaX / zoom`.
func _plot_input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		var button := event as InputEventMouseButton
		if button.pressed and (button.button_index == MOUSE_BUTTON_WHEEL_DOWN \
				or button.button_index == MOUSE_BUTTON_WHEEL_UP):
			var scroll := 1.0 if button.button_index == MOUSE_BUTTON_WHEEL_DOWN else -1.0
			_zoom = clampf(_zoom - scroll * 2.0 / 10.0 * _zoom, 0.5, 40.0)
			_plot.accept_event()
		elif button.button_index == MOUSE_BUTTON_LEFT:
			_dragging = button.pressed
	elif event is InputEventMouseMotion:
		if _dragging:
			_pan -= (event as InputEventMouseMotion).relative.x / _zoom
	_plot.queue_redraw()


## `WaveGraph.rect(...)`, in Godot's y-down plot space (`y` = bottom - Java y).
func _draw_plot() -> void:
	var width := _plot.size.x
	var height := _plot.size.y
	var font := get_theme_default_font()
	var font_size := get_theme_default_font_size()
	# `lay.setText(font, "1"); fh = lay.height` (cap height).
	var fh := font.get_ascent(font_size) * 0.7
	var spacing := _zoom * DEFAULT_SPACE
	_pan = maxf(_pan, (width / 2.0) / _zoom - DEFAULT_SPACE)
	var offset_y := 22.0 + fh + 5.0
	var graph_x := -_pan * _zoom + width / 2.0
	var graph_h := height - offset_y
	var base_y := graph_h
	var left := (0.0 - graph_x) / spacing
	var right := (width - graph_x) / spacing
	_from = int(left) - 1
	_to = int(right) + 1
	if _last_from != _from or _last_to != _to:
		_rebuild()
	_last_from = _from
	_last_to = _to

	var series: Array = _data.get("series", [])
	var units: Array = _data.get("units", [])
	var max_y_values: PackedInt64Array = _data.get("max_y", PackedInt64Array([1, 1, 1]))
	var max_y := float(maxi(1, int(max_y_values[_mode]) if _mode < max_y_values.size() else 1))
	var unit_count := maxf(1.0, float(_data.get("unit_count", 1)))
	var visible := _visible_units(units)

	var mouse := _plot.get_local_mouse_position()
	var inside := Rect2(Vector2.ZERO, _plot.size).has_point(mouse)
	var selcol := roundi((mouse.x - graph_x - _from * spacing) / spacing) if inside else -1
	if selcol + _from <= -1:
		selcol = -1

	var columns := series.size()
	match _mode:
		Mode.COUNTS:
			for unit_index in visible:
				var unit: Dictionary = units[unit_index]
				var points := PackedVector2Array()
				for i in columns:
					var row: PackedInt64Array = series[i]
					points.append(Vector2(graph_x + (i + _from) * spacing,
						base_y - float(row[unit_index]) * graph_h / max_y))
				_end_line(points, _unit_color(unit, unit_count))
		Mode.TOTALS, Mode.HEALTH:
			var points := PackedVector2Array()
			for i in columns:
				var row: PackedInt64Array = series[i]
				var sum := 0.0
				for unit_index in visible:
					var weight := 1.0
					if _mode == Mode.HEALTH:
						weight = float((units[unit_index] as Dictionary).get("health", 0.0))
					sum += weight * float(row[unit_index])
				points.append(Vector2(graph_x + (i + _from) * spacing, base_y - sum * graph_h / max_y))
			_end_line(points, MindStyles.ACCENT if _mode == Mode.TOTALS else HEALTH_COLOR)

	if selcol >= 0 and selcol < columns:
		var column_x := (selcol + _from) * spacing + graph_x - spacing / 2.0
		_plot.draw_rect(Rect2(column_x, 0.0, spacing, graph_h), Color(1.0, 0.0, 0.0, 0.2))
		# `countStr`: `type.emoji() + " " + amount` per line, font scale 1.5.
		var big := int(font_size * 1.5)
		var line_h := font.get_height(big)
		var y := 5.0
		var row: PackedInt64Array = series[selcol]
		for unit_index in visible:
			var amount := int(row[unit_index])
			if amount <= 0:
				continue
			var unit: Dictionary = units[unit_index]
			var icon := MindWidgets.icon_texture("unit-%s-ui" % str(unit.get("name", "")))
			var x := column_x + 5.0
			if icon != null:
				var icon_size := font.get_ascent(big)
				_plot.draw_texture_rect(icon, Rect2(x, y + (line_h - icon_size) / 2.0, icon_size, icon_size), false)
				x += icon_size + font.get_string_size(" ", HORIZONTAL_ALIGNMENT_LEFT, -1, big).x
			_plot.draw_string_outline(font, Vector2(x, y + font.get_ascent(big)), str(amount),
				HORIZONTAL_ALIGNMENT_LEFT, -1, big, 3, Color.BLACK)
			_plot.draw_string(font, Vector2(x, y + font.get_ascent(big)), str(amount),
				HORIZONTAL_ALIGNMENT_LEFT, -1, big, Color.WHITE)
			y += line_h

	# Horizontal marks: `totalMarks = clamp(maxY, 1, 10)`, faint light-gray
	# lines with the value at the left edge.
	var total_marks := clampf(max_y, 1.0, 10.0)
	var mark_space := maxi(1, ceili(max_y / total_marks))
	var mark := 0
	while mark < int(max_y):
		var cy := base_y - mark * graph_h / max_y
		_plot.draw_line(Vector2(0.0, cy), Vector2(width, cy), Color(Color.LIGHT_GRAY, 0.1), STROKE)
		_plot.draw_string(font, Vector2(0.0, cy + fh / 2.0), str(mark), HORIZONTAL_ALIGNMENT_LEFT,
			-1, font_size, Color.WHITE)
		mark += mark_space

	# Wave ticks along the bottom (`cy = y + fh`, `len = 4`), the hovered
	# column's wave number under its tick.
	var tick_y := height - fh
	for i in columns:
		var cx := graph_x + spacing * (i + _from)
		if cx >= 0.0 and cx <= width:
			_plot.draw_line(Vector2(cx, tick_y), Vector2(cx, tick_y - 4.0), Color.LIGHT_GRAY, STROKE)
		if i == selcol:
			var text := str(i + _from + 1)
			var text_w := font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size).x
			_plot.draw_string(font, Vector2(cx - text_w / 2.0, tick_y + 2.0 + fh), text,
				HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, Color.LIGHT_GRAY)


## `endLine()`: 3 px segments with round joints, a bigger dot on the last point.
func _end_line(points: PackedVector2Array, color: Color) -> void:
	if points.is_empty():
		return
	for i in points.size() - 1:
		_plot.draw_line(points[i], points[i + 1], color, STROKE)
		_plot.draw_circle(points[i], STROKE / 2.0, color)
	_plot.draw_circle(points[points.size() - 1], STROKE, color)


## `used` minus `hidden`, as unit indices into the Rust `units` array.
func _visible_units(units: Array) -> Array[int]:
	var out: Array[int] = []
	for index in units.size():
		if not _hidden.has(str((units[index] as Dictionary).get("name", ""))):
			out.append(index)
	return out


## `color(type)`: `fromHsv(type.id / units.size * 360, 0.7, 1)`.
func _unit_color(unit: Dictionary, unit_count: float) -> Color:
	return Color.from_hsv(float(unit.get("id", 0)) / unit_count, 0.7, 1.0)


## `rebuild()`: refetch the window and rebuild the legend.
func _rebuild() -> void:
	_data = {}
	if _editor != null and _editor.has_method("wave_graph"):
		var value: Variant = _editor.call("wave_graph", _from, _to)
		if value is Dictionary:
			_data = value
	_rebuild_legend()


## `colors`: the `@waves.units.hide`/`show` button (flatt, 130x32) and a
## horizontal pane of `fullTogglet` toggles (colour swatch + unit icon).
## code-instantiated: one legend toggle per participating unit type.
func _rebuild_legend() -> void:
	if _colors == null:
		return
	for child in _colors.get_children():
		_colors.remove_child(child)
		child.queue_free()
	_legend_buttons.clear()
	var units: Array = _data.get("units", [])
	var unit_count := maxf(1.0, float(_data.get("unit_count", 1)))
	_hide_button = MindWidgets.button("")
	_hide_button.theme_type_variation = "flatt"
	_hide_button.custom_minimum_size = Vector2(130.0, 32.0)
	_hide_button.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	_hide_button.pressed.connect(_toggle_all.bind(units))
	_colors.add_child(_hide_button)
	var scroll := ScrollContainer.new()
	scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_colors.add_child(scroll)
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 0)
	scroll.add_child(row)
	for unit_value in units:
		var unit: Dictionary = unit_value
		var name := str(unit.get("name", ""))
		var button := Button.new()
		button.theme_type_variation = "fullTogglet"
		button.toggle_mode = true
		var content := HBoxContainer.new()
		content.mouse_filter = Control.MOUSE_FILTER_IGNORE
		content.add_theme_constant_override("separation", 0)
		# `b.image().size(32f)` has no drawable, so Arc draws nothing there:
		# the colour only tints an empty image. Keep the 32 px slot, invisible.
		var swatch := ColorRect.new()
		swatch.self_modulate = Color(1, 1, 1, 0)
		swatch.custom_minimum_size = Vector2(32.0, 32.0)
		swatch.mouse_filter = Control.MOUSE_FILTER_IGNORE
		swatch.set_meta("color", _unit_color(unit, unit_count))
		content.add_child(swatch)
		var icon := TextureRect.new()
		icon.texture = MindWidgets.icon_texture("unit-%s-ui" % name)
		icon.custom_minimum_size = Vector2(32.0, 32.0)
		icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
		content.add_child(icon)
		# `.padRight(20)` after the icon.
		var pad := Control.new()
		pad.custom_minimum_size.x = 20.0
		pad.mouse_filter = Control.MOUSE_FILTER_IGNORE
		content.add_child(pad)
		button.add_child(content)
		button.custom_minimum_size = Vector2(84.0, 32.0)
		button.pressed.connect(_toggle_unit.bind(name))
		row.add_child(button)
		_legend_buttons[name] = [button, swatch, icon]
	_sync_legend(units)


func _toggle_unit(name: String) -> void:
	if _hidden.has(name):
		_hidden.erase(name)
	else:
		_hidden[name] = true
	_sync_legend(_data.get("units", []))
	_plot.queue_redraw()


## `hidden.size == used.size ? clear : addAll(used)`.
func _toggle_all(units: Array) -> void:
	if _all_hidden(units):
		_hidden.clear()
	else:
		for unit_value in units:
			_hidden[str((unit_value as Dictionary).get("name", ""))] = true
	_sync_legend(units)
	_plot.queue_redraw()


func _all_hidden(units: Array) -> bool:
	for unit_value in units:
		if not _hidden.has(str((unit_value as Dictionary).get("name", ""))):
			return false
	return not units.is_empty()


## The legend `update` lambdas: checked = hidden; the swatch halves its colour
## and the icon greys out; the hide button reads show/hide.
func _sync_legend(units: Array) -> void:
	if _hide_button != null:
		_hide_button.text = _bundle("waves.units.show" if _all_hidden(units) else "waves.units.hide")
	for name in _legend_buttons:
		var parts: Array = _legend_buttons[name]
		var hidden := _hidden.has(name)
		(parts[0] as Button).set_pressed_no_signal(hidden)
		var color: Color = (parts[1] as ColorRect).get_meta("color")
		(parts[1] as ColorRect).color = Color(color.r * 0.5, color.g * 0.5, color.b * 0.5, color.a * 0.5) \
			if hidden else color
		(parts[2] as TextureRect).modulate = Color.GRAY if hidden else Color.WHITE


func _bundle(key: String) -> String:
	var assets := get_node_or_null("/root/MindAssets")
	if assets == null:
		return key
	var value := str(assets.call("bundle_get", key))
	return key if value.is_empty() else value
