## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/ColorPicker.java (plan 14 §3.4, M3).
##
## HSV + alpha picker. Upstream draws saturation/value pads; the M3 shell maps
## the same four components to sliders over a live preview and emits
## `color_selected` on OK. The saturation/value pad (`color_picker_pad.tscn`) is
## tracked in the plan changelog.

extends MindDialog

signal color_selected(color: Color)

var _h := 0.0
var _s := 0.0
var _v := 1.0
var _a := 1.0
var _current := Color.WHITE
var _preview: ColorRect = null


func _ready() -> void:
	set_title_key("@pickcolor")
	should_pause = false
	super._ready()
	_build()


## Sets the initial color (upstream `ColorPicker.setColor`).
func set_color(value: Color) -> void:
	_current = value
	_h = value.h
	_s = value.s
	_v = value.v
	_a = value.a
	_refresh()


func _build() -> void:
	var table := content_table()
	table.add_theme_constant_override("separation", 6)

	_preview = ColorRect.new()
	_preview.custom_minimum_size = Vector2(200, 48)
	_preview.color = _current
	table.add(_preview).grow_x_axis().height(48.0).pad(6)
	table.row()

	table.add(_component_slider("@pickcolor.h", 0.0, 360.0, _h * 360.0, func(v: float) -> void:
		_h = v / 360.0
		_refresh())).grow_x_axis()
	table.row()
	table.add(_component_slider("@pickcolor.s", 0.0, 1.0, _s, func(v: float) -> void:
		_s = v
		_refresh())).grow_x_axis()
	table.row()
	table.add(_component_slider("@pickcolor.v", 0.0, 1.0, _v, func(v: float) -> void:
		_v = v
		_refresh())).grow_x_axis()
	table.row()
	table.add(_component_slider("@pickcolor.a", 0.0, 1.0, _a, func(v: float) -> void:
		_a = v
		_refresh())).grow_x_axis()

	add_close_button()
	var ok := MindWidgets.button(_t("@ok"))
	ok.pressed.connect(_confirm)
	buttons.add_child(ok)


func _component_slider(label_key: String, minimum: float, maximum: float, value: float, on_change: Callable) -> Control:
	var row := VBoxContainer.new()
	row.add_child(MindWidgets.label(_t(label_key)))
	var slider := MindWidgets.slider(minimum, maximum, 0.01)
	slider.value = value
	slider.value_changed.connect(on_change)
	row.add_child(slider)
	return row


func _refresh() -> void:
	_current = Color.from_hsv(_h, _s, _v, _a)
	if _preview != null:
		_preview.color = _current


func _confirm() -> void:
	color_selected.emit(_current)
	hide_dialog()
