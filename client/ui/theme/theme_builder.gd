## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/Styles.java (plan 14 §3.3).
##
## Builds the single runtime [Theme] from `styles_manifest.json`: `default*`
## names become type defaults, the rest become theme type variations keyed by the
## Java field name so MSUI `style: "grayt"` and GDScript `theme_type_variation`
## both resolve. Ninepatch atlas margins replace the M0 flat placeholders at M1
## (plan 14 §7d/R7).

class_name MindThemeBuilder
extends RefCounted

const STYLES_MANIFEST := "res://ui/styles_manifest.json"


## Builds the UI theme (`AssetsReadyEvent`). `assets` may be null in a test host.
static func build(assets: Node) -> Theme:
	var theme := Theme.new()
	theme.default_font_size = 18
	if assets != null:
		var font: Variant = assets.call("default_font")
		if font is FontFile:
			theme.default_font = font
	_apply_base_styles(theme)
	_apply_manifest(theme, load_manifest())
	return theme


## Loads `styles_manifest.json` (`{}` when absent).
static func load_manifest() -> Dictionary:
	if not FileAccess.file_exists(STYLES_MANIFEST):
		return {}
	var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(STYLES_MANIFEST))
	return parsed if parsed is Dictionary else {}


## Boot check: every manifest name resolves to a theme type (plan 14 §7a.3).
static func verify(theme: Theme, manifest: Dictionary) -> bool:
	var ok := true
	for group in MindStyles.GROUP_BASE_TYPES.keys():
		for name in manifest.get(group, []):
			if not theme.get_type_list().has(str(name)):
				push_warning("[ui] style not registered: %s" % str(name))
				ok = false
	return ok


static func _apply_base_styles(theme: Theme) -> void:
	theme.set_stylebox("panel", "PanelContainer", flat(MindStyles.GRAY_PANEL))
	theme.set_stylebox("normal", "Button", flat(Color(0.18, 0.20, 0.24, 1.0)))
	theme.set_stylebox("hover", "Button", flat(Color(0.24, 0.27, 0.32, 1.0)))
	theme.set_stylebox("pressed", "Button", flat(Color(0.13, 0.15, 0.18, 1.0)))
	theme.set_stylebox("disabled", "Button", flat(Color(0.12, 0.13, 0.15, 1.0)))
	theme.set_color("font_color", "Button", Color(0.92, 0.92, 0.92))
	theme.set_color("font_hover_color", "Button", Color.WHITE)
	theme.set_color("font_disabled_color", "Button", Color(0.5, 0.5, 0.5))
	theme.set_color("default_color", "RichTextLabel", Color(0.92, 0.92, 0.92))
	theme.set_color("font_color", "Label", Color(0.92, 0.92, 0.92))
	theme.set_stylebox("panel", "Panel", flat(MindStyles.GRAY_PANEL_DARK))


static func _apply_manifest(theme: Theme, manifest: Dictionary) -> void:
	if manifest.is_empty():
		return
	for group in MindStyles.GROUP_BASE_TYPES.keys():
		var base: String = MindStyles.GROUP_BASE_TYPES[group]
		for name_value in manifest.get(group, []):
			var name := str(name_value)
			theme.add_type(name)
			theme.set_type_variation(name, base)
			_style_variation(theme, base, name)


static func _style_variation(theme: Theme, base: String, name: String) -> void:
	match base:
		"Button":
			if name == "nonet":
				# `Styles.nonet`: no background/border at all (hint skip).
				theme.set_stylebox("normal", name, StyleBoxEmpty.new())
				theme.set_stylebox("hover", name, StyleBoxEmpty.new())
				theme.set_stylebox("pressed", name, StyleBoxEmpty.new())
				theme.set_stylebox("disabled", name, StyleBoxEmpty.new())
			elif name == "defaultb" or name == "defaultt":
				# `Styles.defaultb`/`defaultt` (`Styles.java:118-125,133-141`):
				# `Tex.button` (black body, grey border) with `buttonOver` on
				# hover and `buttonDown` (white border) while pressed;
				# `buttonDisabled` for the disabled state. The 9-patch's 12 px
				# splits are also the content padding, so the icon cell starts
				# 12 px in like Java's `Button` background padding.
				theme.set_stylebox("normal", name, _button_nine_patch("button"))
				theme.set_stylebox("hover", name, _button_nine_patch("button-over"))
				theme.set_stylebox("pressed", name, _button_nine_patch("button-down"))
				theme.set_stylebox("hover_pressed", name, _button_nine_patch("button-down"))
				theme.set_stylebox("disabled", name, _button_nine_patch("button-disabled"))
				theme.set_color("font_color", name, Color.WHITE)
				theme.set_color("font_hover_color", name, Color.WHITE)
				theme.set_color("font_pressed_color", name, Color.WHITE)
				theme.set_color("font_disabled_color", name, Color.GRAY)
			elif name == "selecti":
				# `Styles.selecti` (Styles.java:283-286): `checked = buttonSelect`
				# (the accent 9-patch frame), `up = none`. The checked state is
				# the accent frame around the cell; the cell itself stays
				# transparent at rest and on hover.
				theme.set_stylebox("normal", name, StyleBoxEmpty.new())
				theme.set_stylebox("hover", name, StyleBoxEmpty.new())
				theme.set_stylebox("disabled", name, StyleBoxEmpty.new())
				var checked := _checked_frame()
				theme.set_stylebox("pressed", name, checked)
				theme.set_stylebox("hover_pressed", name, checked)
			elif name == "clearTogglei":
				# `Styles.clearTogglei` (Styles.java:345-350): `up = black6`,
				# `over = flatOver` (#454545), `down = checked = flatDown` (the
				# accent-bordered `flat-down-base` 9-patch). The checked frame is
				# what marks the selected `PlacementFragment` category.
				theme.set_stylebox("normal", name, flat(MindStyles.BLACK_6))
				theme.set_stylebox("hover", name, flat(Color8(0x45, 0x45, 0x45)))
				theme.set_stylebox("disabled", name, flat(MindStyles.BLACK_6))
				var flat_down := _flat_down_frame()
				theme.set_stylebox("pressed", name, flat_down)
				theme.set_stylebox("hover_pressed", name, flat_down)
			elif name == "clearNonei" or name == "clearNoneTogglei":
				# `Styles.clearNonei` (Styles.java:316): `up = none` (fully
				# transparent at rest), hover/pressed carry the flat highlight
				# boxes; used by the HUD input row and the research dialog.
				theme.set_stylebox("normal", name, StyleBoxEmpty.new())
				theme.set_stylebox("hover", name, flat(Color(0.24, 0.27, 0.32, 0.85)))
				theme.set_stylebox("pressed", name, flat(Color(0.13, 0.15, 0.18, 0.85)))
				theme.set_stylebox("disabled", name, StyleBoxEmpty.new())
			else:
				theme.set_stylebox("normal", name, flat(Color(0.18, 0.20, 0.24, 1.0)))
				theme.set_stylebox("hover", name, flat(Color(0.24, 0.27, 0.32, 1.0)))
				theme.set_stylebox("pressed", name, flat(Color(0.13, 0.15, 0.18, 1.0)))
			theme.set_color("font_color", name, Color(0.92, 0.92, 0.92))
		"PanelContainer":
			if name == "fullDialog":
				# `Styles.fullDialog` uses `windowEmpty`: a full-viewport frame
				# over the stage background (the `Dim` overlay + backdrop), not an
				# opaque sheet. Content floats over the space backdrop.
				var empty := StyleBoxEmpty.new()
				empty.content_margin_left = 8.0
				empty.content_margin_right = 8.0
				# The Java title text starts at y=6 in the reference frame (no
				# top inset above the title row).
				empty.content_margin_top = 0.0
				# Java's full dialogs leave 8 px below the button row
				# (`PlanetDialog` OK at y=640 of 648).
				empty.content_margin_bottom = 8.0
				theme.set_stylebox("panel", name, empty)
			elif name == "none":
				theme.set_stylebox("panel", name, StyleBoxEmpty.new())
			else:
				# Named drawables carry their Java alpha (`Styles.black`,
				# `black3`, `black5`, `black6`, `black8`, `black9`); the rest of
				# the manifest group keeps the pane colour.
				theme.set_stylebox("panel", name, flat(_drawable_color(name)))
		"RichTextLabel":
			theme.set_color("default_color", name, Color(0.92, 0.92, 0.92))
		"HSlider":
			theme.set_stylebox("slider", name, flat(Color(0.12, 0.13, 0.15, 1.0)))
			theme.set_stylebox("grabber_area", name, flat(MindStyles.ACCENT))
		"LineEdit":
			theme.set_stylebox("normal", name, flat(Color(0.08, 0.09, 0.11, 1.0)))
			theme.set_color("font_color", name, Color(0.92, 0.92, 0.92))
		"CheckBox":
			theme.set_color("font_color", name, Color(0.92, 0.92, 0.92))
		"Tree":
			theme.set_stylebox("panel", name, flat(MindStyles.GRAY_PANEL_DARK))
		_:
			pass


## `Tex.button*` nine-patch (`button`, `button-over`, `button-down`,
## `button-disabled`): 12 px texture margins on all four edges (the pack
## `splits`). `StyleBoxTexture`'s content margin follows the texture margin, so
## the button's content starts 12 px in — the same padding Arc applies from the
## background drawable.
static func _button_nine_patch(region: String) -> StyleBox:
	var texture := MindWidgets.icon_texture(region)
	if texture == null:
		return flat(Color(0.08, 0.08, 0.10, 1.0))
	var box := StyleBoxTexture.new()
	box.texture = texture
	box.texture_margin_left = 12.0
	box.texture_margin_top = 12.0
	box.texture_margin_right = 12.0
	box.texture_margin_bottom = 12.0
	return box


## `Tex.buttonSelect` ninepatch (accent border, splits 4) for the
## `Styles.selecti` checked state; falls back to the flat pressed box when the
## atlas region is unavailable.
static func _checked_frame() -> StyleBox:
	var texture := MindWidgets.icon_texture("button-select")
	if texture == null:
		return flat(Color(0.13, 0.15, 0.18, 1.0))
	var box := StyleBoxTexture.new()
	box.texture = texture
	box.texture_margin_left = 4.0
	box.texture_margin_top = 4.0
	box.texture_margin_right = 4.0
	box.texture_margin_bottom = 4.0
	return box


## `Styles.flatDown` (`Styles.createFlatDown`, Styles.java:479-495): the
## `flat-down-base` 9-patch (splits 12; 4px `Pal.accent` border around a black
## fill) with zero padding. Falls back to an accent-bordered flat box when the
## atlas region is unavailable.
static func _flat_down_frame() -> StyleBox:
	var texture := MindWidgets.icon_texture("flat-down-base")
	if texture == null:
		var box := flat(Color.BLACK)
		box.border_color = MindStyles.ACCENT
		box.set_border_width_all(4)
		box.set_content_margin_all(0.0)
		return box
	var frame := StyleBoxTexture.new()
	frame.texture = texture
	frame.texture_margin_left = 12.0
	frame.texture_margin_top = 12.0
	frame.texture_margin_right = 12.0
	frame.texture_margin_bottom = 12.0
	frame.set_content_margin_all(0.0)
	return frame


## Flat stylebox with the standard content margins (ninepatch replaces at M1).
static func flat(color: Color) -> StyleBoxFlat:
	var box := StyleBoxFlat.new()
	box.bg_color = color
	box.content_margin_left = 8.0
	box.content_margin_right = 8.0
	box.content_margin_top = 4.0
	box.content_margin_bottom = 4.0
	box.corner_radius_top_left = 3
	box.corner_radius_top_right = 3
	box.corner_radius_bottom_left = 3
	box.corner_radius_bottom_right = 3
	return box


## `Styles` drawable colour: the named translucent blacks keep their Java
## alpha; every other drawable stays the pane colour.
static func _drawable_color(name: String) -> Color:
	match name:
		"black":
			return MindStyles.BLACK
		"black3":
			return MindStyles.BLACK_3
		"black5":
			return MindStyles.BLACK_5
		"black6":
			return MindStyles.BLACK_6
		"black8":
			return MindStyles.BLACK_8
		"black9":
			return MindStyles.BLACK_9
		_:
			return MindStyles.GRAY_PANEL
