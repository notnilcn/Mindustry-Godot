## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/Elems.java (`Elems.check`; plan 14 §3.3).
##
## A fancy check button (not a real checkbox): background toggles grayPanel and
## the default/over state is reflected by the theme.

class_name MindCheck
extends Button

var _listener: Callable = Callable()


## Builds the check (`Elems.check(text, checked, listener)`).
func setup(text: String, is_checked: bool, listener: Callable) -> void:
	_listener = listener
	toggle_mode = true
	button_pressed = is_checked
	theme_type_variation = "grayt"
	alignment = HORIZONTAL_ALIGNMENT_LEFT
	# code-instantiated: the icon+label row is data-driven and has no static scene
	# (plan 14 §3.3 `Elems.check`).
	var row := HBoxContainer.new()
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(row)
	var icon := TextureRect.new()
	icon.custom_minimum_size = Vector2(32, 32)
	icon.texture = MindWidgets.icon_texture("checkOn" if is_checked else "checkOff")
	icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	row.add_child(icon)
	var label := Label.new()
	label.text = text
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(label)
	if not toggled.is_connected(_on_toggled):
		toggled.connect(_on_toggled)


func set_checked(value: bool) -> void:
	button_pressed = value


func _on_toggled(pressed_state: bool) -> void:
	if _listener.is_valid():
		_listener.call(pressed_state)
