## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: arc.scene.ui.* widget constructors (plan 14 §3.3).
##
## Static factory helpers so dialogs never hand-build themed widgets. Every
## helper applies the manifest type variation (theme_type_variation) expected by
## `MindThemeBuilder`; text/icon lookup goes through the `MindAssets` autoload.

class_name MindWidgets
extends RefCounted


static func table() -> MindTable:
	return MindTable.new()


static func stack() -> MindStack:
	return MindStack.new()


static func scroll() -> MindScroll:
	return MindScroll.new()


static func label(text_value: String = "") -> MindLabel:
	var result := MindLabel.new()
	result.text = text_value
	return result


static func styled_label(text_value: String, style: String) -> MindLabel:
	var result := label(text_value)
	if not style.is_empty():
		result.theme_type_variation = style
	return result


static func button(text_value: String) -> Button:
	var result := Button.new()
	result.text = text_value
	result.theme_type_variation = "defaultt"
	return result


static func image_button(region_name: String, style: String = "defaulti") -> Button:
	var result := Button.new()
	result.icon = icon_texture(region_name)
	result.theme_type_variation = style
	return result


static func image(region_name: String) -> TextureRect:
	var result := TextureRect.new()
	result.texture = icon_texture(region_name)
	result.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	result.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	return result


static func check(text_value: String = "") -> CheckBox:
	var result := CheckBox.new()
	result.text = text_value
	result.theme_type_variation = "defaultCheck"
	return result


static func field(placeholder: String = "") -> LineEdit:
	var result := LineEdit.new()
	result.placeholder_text = placeholder
	result.theme_type_variation = "defaultField"
	return result


static func slider(minimum: float, maximum: float, step: float) -> HSlider:
	var result := HSlider.new()
	result.min_value = minimum
	result.max_value = maximum
	result.step = step
	result.theme_type_variation = "defaultSlider"
	return result


static func space(width: float, height: float) -> Control:
	# code-instantiated: spacers are sizing-only layout cells with no static scene.
	var result := Control.new()
	result.custom_minimum_size = Vector2(width, height)
	return result


## Resolves an atlas region through the `MindAssets` autoload (null when absent).
static func icon_texture(region_name: String) -> Texture2D:
	var assets := assets()
	if assets == null:
		return null
	var texture: Variant = assets.call("find_region", region_name)
	return texture as Texture2D


## The `MindAssets` autoload (plan 03 M5), or null in a bare test host.
static func assets() -> Node:
	var loop := Engine.get_main_loop()
	if loop is SceneTree:
		return (loop as SceneTree).root.get_node_or_null("MindAssets")
	return null
