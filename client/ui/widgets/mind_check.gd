## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/Elems.java (`Elems.check`; plan 14 §3.3).
##
## A fancy check button (not a real checkbox): the background toggles grayPanel
## and the check/over drawable switches between the check-on/check-off textures,
## mirroring `Elems.check`'s dynamic image.

class_name MindCheck
extends Button

var _listener: Callable = Callable()
var _icon: TextureRect = null
var _row: HBoxContainer = null


## The icon + label row is an anchored child, which a Button does not measure
## (and Button's native `get_minimum_size` ignores a script
## `_get_minimum_size`). Raise `custom_minimum_size` to the row's minimum so a
## check cell that does not grow keeps its content width (Arc sizes
## `Elems.check`/`CheckBox` to the image + label, L01 T3).
func _sync_row_minimum() -> void:
	if _row == null or not is_instance_valid(_row):
		return
	var row_min := _row.get_combined_minimum_size()
	custom_minimum_size = Vector2(
		maxf(custom_minimum_size.x, row_min.x), maxf(custom_minimum_size.y, row_min.y)
	)


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
	row.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	row.add_theme_constant_override("separation", 8)
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(row)
	_row = row
	row.minimum_size_changed.connect(_sync_row_minimum)
	_icon = TextureRect.new()
	_icon.custom_minimum_size = Vector2(32, 32)
	_icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	_icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(_icon)
	var label := Label.new()
	label.text = text
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(label)
	_refresh_icon()
	# A `Button` is not a container, so its children do not contribute to its
	# minimum size; size it from the 32px indicator so every cell that only grows
	# the row leaves the whole row clickable.
	custom_minimum_size.y = maxf(custom_minimum_size.y, _icon.custom_minimum_size.y)
	if not toggled.is_connected(_on_toggled):
		toggled.connect(_on_toggled)
	if not mouse_entered.is_connected(_refresh_icon):
		mouse_entered.connect(_refresh_icon)
	if not mouse_exited.is_connected(_refresh_icon):
		mouse_exited.connect(_refresh_icon)


func set_checked(value: bool) -> void:
	button_pressed = value
	_refresh_icon()


func _refresh_icon() -> void:
	if _icon == null:
		return
	var region := "check-on" if button_pressed else "check-off"
	if is_inside_tree() and is_hovered():
		region = "check-on-over" if button_pressed else "check-over"
	_icon.texture = MindWidgets.icon_texture(region)


func _on_toggled(pressed_state: bool) -> void:
	_refresh_icon()
	if _listener.is_valid():
		_listener.call(pressed_state)
