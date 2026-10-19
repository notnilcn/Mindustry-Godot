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
## upstream `TextButton` + `Icon.*` drawables (`Table.button(text, image,
## runnable)`, `Table.java:536-550`): the icon is the first cell at the glyph's
## natural size (`image.imageSize()`; `Fonts.icon` is a 30 px font,
## `Fonts.java:78-82`) and the label grows over the remaining width, centring
## its text (`TextButton.java:30-34`). Falls back to plain text when the glyph
## is unknown or assets are absent.
static func icon_button(
	icon_name: String,
	text_value: String,
	style: String = "defaultt",
	row_alignment: int = BoxContainer.ALIGNMENT_BEGIN
) -> Button:
	var result := Button.new()
	result.theme_type_variation = style
	var glyph_text := MindIcons.glyph(icon_name)
	if glyph_text.is_empty():
		result.text = text_value
		return result
	# code-instantiated: data-driven icon+label row inside the themed button.
	var icon_only := text_value.is_empty()
	var row := HBoxContainer.new()
	# An icon-only entry is Java's `button(Icon.x, style, size, ...)` image
	# button (icon centred); a text button keeps the icon cell at the left.
	row.alignment = (
		BoxContainer.ALIGNMENT_CENTER if icon_only else row_alignment
	)
	row.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	row.add_theme_constant_override("separation", 0)
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	result.add_child(row)
	# The icon cell: the glyph at the icon font's 30 px size fills its own cell
	# (Java does not reserve `iconMed` for text buttons).
	var icon := glyph(icon_name, 30)
	icon.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(icon)
	if icon_only:
		result.ready.connect(
			func():
				var row_min := row.get_combined_minimum_size()
				result.custom_minimum_size = Vector2(
					maxf(result.custom_minimum_size.x, row_min.x),
					maxf(result.custom_minimum_size.y, row_min.y)
				)
		)
		return result
	var label := Label.new()
	label.text = text_value
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
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
	# The text is a child Label, so it never sees the button's disabled state:
	# Arc `TextButtonStyle.disabledFontColor` (gray for `defaultt`, Styles.java
	# :138) only reaches it through this hook. `BaseButton.set_disabled`
	# redraws the button; the override changes only on a state flip.
	result.draw.connect(
		func():
			if result.disabled == bool(result.get_meta("_shown_disabled", false)):
				return
			result.set_meta("_shown_disabled", result.disabled)
			if result.disabled:
				label.add_theme_color_override("font_color", result.get_theme_color("font_disabled_color"))
			else:
				label.remove_theme_color_override("font_color")
	)
	# Arc `Button` is a `Table` whose background drawable pads its cells
	# (`Table.setBackground` -> `getLeftWidth()`...; `Tex.button`'s 9-patch
	# splits), so the icon cell starts inside the border and the label centres in
	# the padded width. A Godot `Button` does not lay out children, so inset the
	# row by the style box's content margins (12 px for `defaultt`).
	inset_to_content(result, row)
	result.theme_changed.connect(inset_to_content.bind(result, row))
	# A `Button` is not a container, so children do not contribute to its minimum
	# size. Theme metrics only resolve once the button is inside the tree, so
	# measure the row and apply its minimum on `ready` (measuring at construction
	# returns a zero-height label); size it from the padded icon+label row so
	# labels are not clipped (Arc `Table.getPrefWidth` adds the padding).
	result.ready.connect(
		func():
			inset_to_content(result, row)
			var row_min := row.get_combined_minimum_size()
			var box := result.get_theme_stylebox("normal")
			var pad := Vector2.ZERO
			if box != null:
				pad = Vector2(
					box.get_margin(SIDE_LEFT) + box.get_margin(SIDE_RIGHT),
					box.get_margin(SIDE_TOP) + box.get_margin(SIDE_BOTTOM)
				)
			result.custom_minimum_size.x = maxf(result.custom_minimum_size.x, row_min.x + pad.x)
			result.custom_minimum_size.y = maxf(result.custom_minimum_size.y, row_min.y + pad.y)
	)
	return result


## Insets a full-rect content `row` inside `button` by the button's `normal`
## style box content margins: Arc's background padding (`Table.setBackground`
## pads by the drawable's left/right/top/bottom widths). The row grows from its
## centre when its content is wider than the padded area, like an Arc `Table`
## centring an over-wide cell run (`Table.layout`, `align = center`).
static func inset_to_content(button: Control, row: Control) -> void:
	if not is_instance_valid(button) or not is_instance_valid(row):
		return
	row.grow_horizontal = Control.GROW_DIRECTION_BOTH
	row.grow_vertical = Control.GROW_DIRECTION_BOTH
	# The themed variation (`defaultt`, ...) only resolves inside the tree;
	# `ready`/`theme_changed` re-apply once it does.
	if not button.is_inside_tree():
		return
	var box := button.get_theme_stylebox("normal")
	if box == null:
		return
	row.offset_left = box.get_margin(SIDE_LEFT)
	row.offset_top = box.get_margin(SIDE_TOP)
	row.offset_right = -box.get_margin(SIDE_RIGHT)
	row.offset_bottom = -box.get_margin(SIDE_BOTTOM)


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


## `render_markup` for a raw markup string that is not a bundle key (H05 T10a
## content-stat cells and content descriptions).
static func markup_text(raw: String) -> String:
	var assets := assets()
	if assets == null:
		return raw
	return str(assets.call("markup_text", raw))


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
	# Arc draws `messageText` only while the field is unfocused (TextField.java:340);
	# Godot keeps the placeholder over a focused empty field. The search
	# dialogs focus their field on show (L01 T3), so hide it while focused.
	result.focus_entered.connect(func() -> void: result.placeholder_text = "")
	result.focus_exited.connect(func() -> void: result.placeholder_text = placeholder)
	return result


static func slider(minimum: float, maximum: float, step: float) -> HSlider:
	var result := HSlider.new()
	result.min_value = minimum
	result.max_value = maximum
	result.step = step
	result.theme_type_variation = "defaultSlider"
	# Arc `Slider` has no `scrolled` handler: in Java the wheel falls through to
	# the surrounding `ScrollPane` and never moves a slider. Godot sliders are
	# `scrollable` by default, which adjusted the value under the pointer while
	# the pane scrolled (J10 T2 item 3).
	result.scrollable = false
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


## Content icon control (`content.uiIcon`): the packed `sector-<name>-ui` /
## `item-<name>-ui` region, else the `terrain` icon-font glyph for sector
## presets (`SectorPreset.java:132-134`: `Core.atlas.find("sector-" + name,
## Icon.terrain.getRegion())`), else `null` (callers draw `Icon.lock`).
static func content_icon(icon_region: String, type_name: String, size: float) -> Control:
	var texture := icon_texture(icon_region)
	if texture != null:
		# code-instantiated: content icons are data-driven (atlas region lookup).
		var image := TextureRect.new()
		image.texture = texture
		image.custom_minimum_size = Vector2(size, size)
		# A `TextureRect`'s minimum size is its texture size; ignore the source
		# size so the requested cell wins (`b.imageDraw(...).size(iconMed)`).
		image.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		image.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		return image
	if type_name == "sector" or type_name == "sectorpreset":
		# code-instantiated: the sector fallback glyph is data-driven.
		# `SectorPreset.uiIcon` falls back to `Icon.terrain` when the
		# `sector-<name>` region is missing (`SectorPreset.java:134`).
		return glyph("terrain", int(size), Color.WHITE)
	return null


## The `MindAssets` autoload (plan 03 M5), or null in a bare test host.
static func assets() -> Node:
	var loop := Engine.get_main_loop()
	if loop is SceneTree:
		return (loop as SceneTree).root.get_node_or_null("MindAssets")
	return null
