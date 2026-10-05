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
				empty.content_margin_top = 4.0
				empty.content_margin_bottom = 4.0
				theme.set_stylebox("panel", name, empty)
			else:
				theme.set_stylebox("panel", name, flat(MindStyles.GRAY_PANEL))
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
