## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/ColorPicker.java (`ui.picker`).
##
## `ColorPicker.show(color, alpha, consumer)` (`ColorPicker.java:27-170`): a
## 200x200 preview over `Tex.alphaBg` in a `Tex.pane` frame, then 370x44 rows
## (`padBottom(6)`) of a slider stacked over its gradient — hue (the hue
## texture), saturation, value and (when `alpha`) alpha over `Tex.alphaBgLine` —
## and a 130x40 hex field; `addCloseButton()` + `@ok` (`Icon.ok`) apply.
## L05 T2 (PAR-0380): the M3 shell's labelled plain sliders are replaced.

extends MindDialog

signal color_selected(color: Color)

const ROW_SIZE := Vector2(370.0, 44.0)

var _h := 0.0
var _s := 0.0
var _v := 1.0
var _a := 1.0
var _alpha := true
var _current := Color.WHITE
var _consumer: Callable = Callable()
var _preview: ColorRect = null
var _hex: LineEdit = null
var _sliders: Dictionary = {}
var _gradients: Array[Control] = []


func _ready() -> void:
	set_title_key("@pickcolor")
	should_pause = false
	super._ready()


## `ColorPicker.show(color, alpha, consumer)`: opens through `MindUi` so the
## dialog stack and Escape behave like every dialog.
func show_picker(color: Color, alpha: bool, consumer: Callable) -> void:
	_consumer = consumer
	_alpha = alpha
	set_color(color)
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("open_dialog"):
		ui.call("open_dialog", String(name), "")
	else:
		show_dialog()


## Sets the initial color (`current.set(color)` + `toHsv`).
func set_color(value: Color) -> void:
	_current = value
	_h = value.h * 360.0
	_s = value.s
	_v = value.v
	_a = value.a


func shown() -> void:
	_build()


func _build() -> void:
	clear_content()
	var table := content_table()
	# `t.table(Tex.pane, i -> i.stack(alphaBg, colorImage).size(200f))
	# .colspan(2).padBottom(5)`.
	var frame := PanelContainer.new()
	var box := _pane_box(Color(0, 0, 0, 0), PANE_OUTLINE_GRAY)
	# `Tex.pane` pads its content 12 px (java/picker.png: frame 228 px around the
	# 200 px preview).
	for side in [SIDE_LEFT, SIDE_TOP, SIDE_RIGHT, SIDE_BOTTOM]:
		box.set_content_margin(side, 12.0)
	frame.add_theme_stylebox_override("panel", box)
	var stack := Control.new()
	stack.custom_minimum_size = Vector2(200.0, 200.0)
	stack.add_child(_alpha_bg("alpha-bg"))
	_preview = ColorRect.new()
	_preview.set_anchors_preset(Control.PRESET_FULL_RECT)
	_preview.mouse_filter = Control.MOUSE_FILTER_IGNORE
	stack.add_child(_preview)
	frame.add_child(stack)
	table.add(frame).set_pad_bottom(5.0)
	table.row()

	_sliders.clear()
	_gradients.clear()
	table.add(_slider_row("h", 360.0, _h, null)).width(ROW_SIZE.x).height(ROW_SIZE.y).set_pad_bottom(6.0)
	table.row()
	table.add(_slider_row("s", 1.0, _s, null)).width(ROW_SIZE.x).height(ROW_SIZE.y).set_pad_bottom(6.0)
	table.row()
	table.add(_slider_row("v", 1.0, _v, null)).width(ROW_SIZE.x).height(ROW_SIZE.y).set_pad_bottom(6.0)
	table.row()
	if _alpha:
		table.add(_slider_row("a", 1.0, _a, "alpha-bg-line")).width(ROW_SIZE.x).height(ROW_SIZE.y).set_pad_bottom(6.0)
		table.row()
	# `hexField = t.field(current.toString(), ...).size(130f, 40f)` with
	# `valid(Color.valueOf)`.
	_hex = MindWidgets.field("")
	_hex.custom_minimum_size = Vector2(130.0, 40.0)
	_hex.text_changed.connect(_on_hex)
	table.add(_hex).width(130.0).height(40.0)

	clear_buttons()
	add_close_button()
	var ok := MindWidgets.icon_button("ok", _t("@ok"))
	ok.custom_minimum_size = Vector2(210.0, 64.0)
	ok.pressed.connect(_confirm)
	buttons.add_child(ok)
	_update_color(true)


## `t.stack(<gradient>, slider)`: the gradient fills the 370x44 row and the
## `defaultSlider` (transparent `sliderBack` body) draws over it.
func _slider_row(component: String, maximum: float, value: float, backdrop: Variant) -> Control:
	# code-instantiated: one stacked slider row per HSV(A) component.
	var row := Control.new()
	row.custom_minimum_size = ROW_SIZE
	if backdrop != null:
		row.add_child(_alpha_bg(str(backdrop)))
	var gradient := Control.new()
	gradient.set_anchors_preset(Control.PRESET_FULL_RECT)
	gradient.mouse_filter = Control.MOUSE_FILTER_IGNORE
	gradient.draw.connect(_draw_gradient.bind(gradient, component))
	row.add_child(gradient)
	_gradients.append(gradient)
	var slider := MindWidgets.slider(0.0, maximum, 0.0001)
	slider.set_anchors_preset(Control.PRESET_FULL_RECT)
	slider.value = value
	slider.value_changed.connect(_on_slider.bind(component))
	row.add_child(slider)
	_sliders[component] = slider
	return row


## `Fill.quad(x, y, first, x + w, y, second, ...)`: a left-to-right gradient of
## the component (hue: the 128-step hue texture).
func _draw_gradient(target: Control, component: String) -> void:
	var target_size := target.size
	if component == "h":
		var steps := 36
		for i in steps:
			var x0 := target_size.x * float(i) / steps
			var x1 := target_size.x * float(i + 1) / steps
			var c0 := Color.from_hsv(float(i) / steps, 1.0, 1.0)
			var c1 := Color.from_hsv(float(i + 1) / steps, 1.0, 1.0)
			_quad(target, x0, x1, target_size.y, c0, c1)
		return
	var first := _current
	var second := _current
	match component:
		"s":
			first = Color.from_hsv(_h / 360.0, 0.0, _v)
			second = Color.from_hsv(_h / 360.0, 1.0, _v)
		"v":
			first = Color.from_hsv(_h / 360.0, _s, 0.0)
			second = Color.from_hsv(_h / 360.0, _s, 1.0)
		"a":
			first = Color(_current.r, _current.g, _current.b, 0.0)
			second = Color(_current.r, _current.g, _current.b, 1.0)
	first.a = first.a if component == "a" else 1.0
	second.a = 1.0
	_quad(target, 0.0, target_size.x, target_size.y, first, second)


func _quad(target: Control, x0: float, x1: float, h: float, c0: Color, c1: Color) -> void:
	target.draw_polygon(
		PackedVector2Array([Vector2(x0, 0), Vector2(x1, 0), Vector2(x1, h), Vector2(x0, h)]),
		PackedColorArray([c0, c1, c1, c0])
	)


## `Tex.alphaBg` / `Tex.alphaBgLine`: the checkerboard under translucent colors.
func _alpha_bg(region: String) -> Control:
	var texture := MindWidgets.icon_texture(region)
	var rect := TextureRect.new()
	rect.set_anchors_preset(Control.PRESET_FULL_RECT)
	rect.mouse_filter = Control.MOUSE_FILTER_IGNORE
	rect.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	rect.stretch_mode = TextureRect.STRETCH_TILE
	rect.texture = texture
	return rect


func _on_slider(value: float, component: String) -> void:
	match component:
		"h":
			_h = value
		"s":
			_s = value
		"v":
			_v = value
		"a":
			_a = value
	_update_color(true)


## `Color.valueOf(current, value)` on every valid edit; sliders follow.
func _on_hex(text: String) -> void:
	var trimmed := text.strip_edges().trim_prefix("#")
	if not (trimmed.length() == 6 or trimmed.length() == 8) or not trimmed.is_valid_hex_number():
		return
	var parsed := Color.html(trimmed if trimmed.length() == 6 else trimmed)
	_h = parsed.h * 360.0
	_s = parsed.s
	_v = parsed.v
	_a = parsed.a
	for component in _sliders:
		var slider: HSlider = _sliders[component]
		slider.set_value_no_signal({"h": _h, "s": _s, "v": _v, "a": _a}[component])
	_update_color(false)


## `updateColor(updateField)`: `current.fromHsv(h, s, v); current.a = a`; the
## hex text drops the alpha byte while opaque.
func _update_color(update_field: bool) -> void:
	_current = Color.from_hsv(_h / 360.0, _s, _v, _a)
	if _preview != null:
		_preview.color = _current
	for gradient in _gradients:
		gradient.queue_redraw()
	if update_field and _hex != null:
		var value := arc_hex(_current)
		if _current.a >= 0.9999:
			value = value.substr(0, 6)
		_hex.set_block_signals(true)
		_hex.text = value
		_hex.set_block_signals(false)


## Arc `Color.toString()` / `rgba8888`: each channel is `(int)(c * 255)`
## (truncated), so a 0.5 alpha is `7f`, where Godot's `to_html` rounds to `80`.
static func arc_rgba(color: Color) -> int:
	var r := int(clampf(color.r, 0.0, 1.0) * 255.0)
	var g := int(clampf(color.g, 0.0, 1.0) * 255.0)
	var b := int(clampf(color.b, 0.0, 1.0) * 255.0)
	var a := int(clampf(color.a, 0.0, 1.0) * 255.0)
	return (r << 24) | (g << 16) | (b << 8) | a


static func arc_hex(color: Color) -> String:
	return "%08x" % arc_rgba(color)


func _confirm() -> void:
	color_selected.emit(_current)
	if _consumer.is_valid():
		_consumer.call(_current)
	_close_pressed()
