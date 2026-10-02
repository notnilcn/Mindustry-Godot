## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/MobileButton.java (plan 14 §3.3).
##
## Icon button with a wrapped centered label below it (`row()` semantics).

class_name MindMobileButton
extends Button

var _listener: Callable = Callable()


## Builds the icon + wrapped label (`MobileButton(icon, text, listener)`).
func setup(icon_region: String, text: String, listener: Callable) -> void:
	_listener = listener
	if not pressed.is_connected(_on_pressed):
		pressed.connect(_on_pressed)
	# code-instantiated: the icon/label row is built from data (region+text) and
	# has no static scene (plan 14 §3.3 MobileButton).
	var column := VBoxContainer.new()
	column.alignment = BoxContainer.ALIGNMENT_CENTER
	column.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(column)
	var icon := TextureRect.new()
	icon.texture = MindWidgets.icon_texture(icon_region)
	icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	icon.custom_minimum_size = Vector2(48, 48)
	column.add_child(icon)
	var label := Label.new()
	label.text = text
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	column.add_child(label)


func _on_pressed() -> void:
	if _listener.is_valid():
		_listener.call()
