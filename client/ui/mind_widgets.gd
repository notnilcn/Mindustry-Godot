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


## Image button resolved from the first available region in `region_names`
## (`core.atlas.find` fallback chain; e.g. `block-x-ui` then `block-x-full`).
static func image_button_first(region_names: PackedStringArray, style: String = "defaulti") -> Button:
	var result := Button.new()
	result.theme_type_variation = style
	for region_name in region_names:
		var texture := icon_texture(region_name)
		if texture != null:
			result.icon = texture
			break
	return result


## An icon-font-glyph toggle button (`PlacementFragment` category rail); falls
## back to `fallback_text` when the glyph is unknown.
static func glyph_button(
	icon_name: String, fallback_text: String, style: String = "clearTogglei"
) -> Button:
	var result := Button.new()
	result.theme_type_variation = style
	var glyph_text := MindIcons.glyph(icon_name)
	if glyph_text.is_empty():
		result.text = fallback_text
		return result
	# code-instantiated: centered glyph label inside the themed toggle button.
	var icon := glyph(icon_name, 28)
	icon.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	result.add_child(icon)
	return result


## An action button with an icon-font glyph followed by a label, mirroring
## upstream `TextButton` + `Icon.*` drawables. Falls back to plain text when the
## glyph is unknown or assets are absent.
static func icon_button(icon_name: String, text_value: String, style: String = "defaultt") -> Button:
	var result := Button.new()
	result.theme_type_variation = style
	var glyph_text := MindIcons.glyph(icon_name)
	if glyph_text.is_empty():
		result.text = text_value
		return result
	# code-instantiated: data-driven icon+label row inside the themed button.
	var row := HBoxContainer.new()
	row.alignment = BoxContainer.ALIGNMENT_CENTER
	row.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	row.add_theme_constant_override("separation", 6)
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	result.add_child(row)
	var icon := Label.new()
	icon.text = glyph_text
	MindIcons.apply_icon_font(icon)
	icon.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(icon)
	var label := Label.new()
	label.text = text_value
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	# Apply the default font up front so the row reports a real minimum width
	# (the theme font is not available before the node enters the tree).
	var assets := assets()
	if assets != null:
		var default_font: Variant = assets.call("default_font")
		if default_font is FontFile and default_font != null:
			label.add_theme_font_override("font", default_font)
	label.add_theme_font_size_override("font_size", 18)
	row.add_child(label)
	# A `Button` is not a container, so children do not contribute to its minimum
	# size. Theme metrics only resolve once the button is inside the tree, so
	# measure the row and apply its minimum on `ready` (measuring at construction
	# returns a zero-height label); size it from the icon+label row so labels are
	# not clipped.
	result.ready.connect(
		func():
			var row_min := row.get_combined_minimum_size()
			result.custom_minimum_size.x = maxf(result.custom_minimum_size.x, row_min.x + 20.0)
			result.custom_minimum_size.y = maxf(result.custom_minimum_size.y, row_min.y)
	)
	return result


## A standalone icon-glyph label (section headers, rail entries).
static func glyph(icon_name: String, font_size: int = 22, color: Color = Color.WHITE) -> Label:
	# code-instantiated: icon runs use the icon font, which no themed label carries.
	var result := Label.new()
	result.text = MindIcons.glyph(icon_name)
	MindIcons.apply_icon_font(result)
	result.add_theme_font_size_override("font_size", font_size)
	result.add_theme_color_override("font_color", color)
	result.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	result.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	return result


## A bundle string translated to Godot BBCode via `MindAssets.bundle_markup`
## (`[accent]` colors, `:icon:` tokens, `\n`). Falls back to the bare key.
static func markup(key: String) -> String:
	var assets := assets()
	if assets == null:
		return key.trim_prefix("@")
	return str(assets.call("bundle_markup", key.trim_prefix("@")))


## `Core.bundle.format` + markup translation (see [method markup]).
static func markup_format(key: String, args: Array) -> String:
	var assets := assets()
	if assets == null:
		return key.trim_prefix("@")
	return str(assets.call("bundle_markup_format", key.trim_prefix("@"), args))


static func image(region_name: String) -> TextureRect:
	var result := TextureRect.new()
	result.texture = icon_texture(region_name)
	result.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	result.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
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


static func bar(bar_name: String, color: Color, fraction: float) -> MindBar:
	var result := MindBar.new()
	result.setup(bar_name, color, fraction)
	return result


static func warning_bar() -> MindWarningBar:
	return MindWarningBar.new()


static func grid_image(w: int, h: int) -> MindGridImage:
	return MindGridImage.new(w, h)


static func border_image(region_name: String) -> MindBorderImage:
	var result := MindBorderImage.new()
	result.texture = icon_texture(region_name)
	return result


static func check(text_value: String, is_checked: bool, listener: Callable = Callable()) -> MindCheck:
	var result := MindCheck.new()
	result.setup(text_value, is_checked, listener)
	return result


static func mobile_button(icon_region: String, text_value: String, listener: Callable) -> MindMobileButton:
	var result := MindMobileButton.new()
	result.setup(icon_region, text_value, listener)
	return result


static func req_image(image: Control, valid: Callable) -> MindReqImage:
	var result := MindReqImage.new()
	result.setup(image, valid)
	return result


static func items_display() -> ItemsDisplay:
	return ItemsDisplay.new()


static func core_items_display() -> CoreItemsDisplay:
	return CoreItemsDisplay.new()


## Tween helpers (`arc.scene.actions.Actions` equivalents).
static func fade_in(node: CanvasItem, duration: float = 0.1) -> void:
	node.modulate.a = 0.0
	var tween := node.create_tween()
	tween.tween_property(node, "modulate:a", 1.0, duration)


static func fade_out(node: CanvasItem, duration: float = 0.1) -> void:
	var tween := node.create_tween()
	tween.tween_property(node, "modulate:a", 0.0, duration)


static func remove_after(node: Node, delay: float) -> void:
	var tween := node.create_tween()
	tween.tween_interval(delay)
	tween.tween_callback(node.queue_free)


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
