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
	# Arc has no keyboard focus frame (`Element` draws none); Godot's default
	# focus stylebox shows up on mouse-driven buttons (the planet page's Go
	# button after a click, `godot/03-sector.png`), so every button style gets an
	# empty one — here for the unvaried base and in `_style_variation` for each
	# manifest variation.
	theme.set_stylebox("focus", "Button", StyleBoxEmpty.new())
	theme.set_color("font_color", "Button", Color(0.92, 0.92, 0.92))
	theme.set_color("font_hover_color", "Button", Color.WHITE)
	theme.set_color("font_disabled_color", "Button", Color(0.5, 0.5, 0.5))
	theme.set_color("default_color", "RichTextLabel", Color(0.92, 0.92, 0.92))
	theme.set_color("font_color", "Label", Color(0.92, 0.92, 0.92))
	theme.set_stylebox("panel", "Panel", flat(MindStyles.GRAY_PANEL_DARK))
	_apply_scroll_bar(theme)
	# `Styles.areaField` (`ui/Styles.java:424-433`): a `TextArea` on the same
	# `underline` 9-patch as the fields, no box, no focus frame (L03 T3: the
	# editor Map Info description, java/ed-info.png).
	var area_box := _nine_patch("underline", 12.0)
	theme.set_stylebox("normal", "TextEdit", area_box)
	theme.set_stylebox("read_only", "TextEdit", _nine_patch("underline-disabled", 12.0))
	theme.set_stylebox("focus", "TextEdit", StyleBoxEmpty.new())
	theme.set_color("font_color", "TextEdit", Color.WHITE)
	theme.set_color("background_color", "TextEdit", Color(0, 0, 0, 0))


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
				theme.set_color("font_disabled_color", name, MindStyles.ARC_GRAY)
			elif name == "grayt":
				# `Styles.grayt` (Styles.java:157-165): `up = grayPanel`,
				# `over`/`down = flatOver` (#454545), `disabled = grayPanelDark`
				# — the save-card body style and `Elems.check` rows. All four are
				# `whiteui` tints: square corners.
				theme.set_stylebox("normal", name, _square(flat(MindStyles.GRAY_PANEL)))
				theme.set_stylebox("hover", name, _square(flat(Color8(0x45, 0x45, 0x45))))
				theme.set_stylebox("pressed", name, _square(flat(Color8(0x45, 0x45, 0x45))))
				theme.set_stylebox("disabled", name, _square(flat(MindStyles.GRAY_PANEL_DARK)))
			elif name == "flati":
				# `Styles.flati` (Styles.java:306-310): `up = black`, `over` /
				# `down = flatOver`; no disabled drawable (Arc keeps `up`). The
				# editor's menu/undo/redo/rotate cells (L03 T3, PAR-0421).
				theme.set_stylebox("normal", name, flat(Color.BLACK))
				theme.set_stylebox("hover", name, flat(Color8(0x45, 0x45, 0x45)))
				theme.set_stylebox("pressed", name, flat(Color8(0x45, 0x45, 0x45)))
				theme.set_stylebox("disabled", name, flat(Color.BLACK))
			elif name == "squareTogglei":
				# `Styles.squareTogglei` (Styles.java:324-329): `up = black`,
				# `over = flatOver`, `down`/`checked = flatDown` (the accent
				# frame) — the editor tool/grid cells, the selected tool framed.
				theme.set_stylebox("normal", name, flat(Color.BLACK))
				theme.set_stylebox("hover", name, flat(Color8(0x45, 0x45, 0x45)))
				theme.set_stylebox("pressed", name, _flat_down_frame())
				theme.set_stylebox("hover_pressed", name, _flat_down_frame())
				theme.set_stylebox("disabled", name, flat(Color.BLACK))
			elif name == "cleari":
				# `Styles.cleari` (Styles.java:337-344): `up`/`disabled = black6`,
				# `over = flatOver`, `down = flatDown`; image white, light gray
				# when disabled.
				theme.set_stylebox("normal", name, flat(MindStyles.BLACK_6))
				theme.set_stylebox("hover", name, flat(Color8(0x45, 0x45, 0x45)))
				theme.set_stylebox("pressed", name, _flat_down_frame())
				theme.set_stylebox("disabled", name, flat(MindStyles.BLACK_6))
				theme.set_color("icon_normal_color", name, Color.WHITE)
				theme.set_color("icon_disabled_color", name, MindStyles.ARC_LIGHT_GRAY)
			elif name == "flatt":
				# `Styles.flatt` (Styles.java:147-155): `up = black`,
				# `over`/`down = flatOver`.
				theme.set_stylebox("normal", name, flat(Color.BLACK))
				theme.set_stylebox("hover", name, flat(Color8(0x45, 0x45, 0x45)))
				theme.set_stylebox("pressed", name, flat(Color8(0x45, 0x45, 0x45)))
				theme.set_stylebox("disabled", name, flat(MindStyles.GRAY_PANEL_DARK))
			elif name == "flatTogglet":
				# `Styles.flatTogglet` (Styles.java:190-199): `up`/`disabled =
				# black`, `over = flatOver`, `down`/`checked = flatDown` (accent
				# frame); gray disabled text.
				theme.set_stylebox("normal", name, flat(Color.BLACK))
				theme.set_stylebox("hover", name, flat(Color8(0x45, 0x45, 0x45)))
				theme.set_stylebox("pressed", name, _flat_down_frame())
				theme.set_stylebox("hover_pressed", name, _flat_down_frame())
				theme.set_stylebox("disabled", name, flat(Color.BLACK))
				theme.set_color("font_disabled_color", name, MindStyles.ARC_GRAY)
			elif name == "squareTogglet":
				# `Styles.squareTogglet` (Styles.java:250-259): `up = pane`,
				# `over`/`down`/`checked = flatOver`, `disabled = black`.
				var over := flat(Color8(0x45, 0x45, 0x45))
				theme.set_stylebox("normal", name, _nine_patch("pane", 12.0))
				theme.set_stylebox("hover", name, over)
				theme.set_stylebox("pressed", name, over)
				theme.set_stylebox("hover_pressed", name, over)
				theme.set_stylebox("disabled", name, flat(Color.BLACK))
				theme.set_color("font_disabled_color", name, MindStyles.ARC_GRAY)
			elif name == "fullTogglet":
				# `Styles.fullTogglet` (Styles.java:240-249): `up`/`disabled =
				# black`, `over`/`down`/`checked = flatOver`; gray disabled text.
				var over := flat(Color8(0x45, 0x45, 0x45))
				theme.set_stylebox("normal", name, flat(Color.BLACK))
				theme.set_stylebox("hover", name, over)
				theme.set_stylebox("pressed", name, over)
				theme.set_stylebox("hover_pressed", name, over)
				theme.set_stylebox("disabled", name, flat(Color.BLACK))
				theme.set_color("font_disabled_color", name, MindStyles.ARC_GRAY)
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
				# `black6`/`flatOver` are `whiteui` tints: square, no rounding, so
				# the stacked category cells tile without corner gaps.
				theme.set_stylebox("normal", name, _square(flat(MindStyles.BLACK_6)))
				theme.set_stylebox("hover", name, _square(flat(Color8(0x45, 0x45, 0x45))))
				theme.set_stylebox("disabled", name, _square(flat(MindStyles.BLACK_6)))
				var flat_down := _flat_down_frame()
				theme.set_stylebox("pressed", name, flat_down)
				theme.set_stylebox("hover_pressed", name, flat_down)
			elif name == "emptyi":
				# `Styles.emptyi` (Styles.java:272-276): no background at all; the
				# icon is white, lightGray on hover and accent on down.
				theme.set_stylebox("normal", name, StyleBoxEmpty.new())
				theme.set_stylebox("hover", name, StyleBoxEmpty.new())
				theme.set_stylebox("pressed", name, StyleBoxEmpty.new())
				theme.set_stylebox("disabled", name, StyleBoxEmpty.new())
				theme.set_color("icon_normal_color", name, Color.WHITE)
				theme.set_color("icon_hover_color", name, Color(0.69, 0.69, 0.69))
				theme.set_color("icon_pressed_color", name, MindStyles.ACCENT)
				theme.set_color("icon_disabled_color", name, Color.WHITE)
			elif name == "emptyTogglei":
				# `Styles.emptyTogglei` (Styles.java:277-282): no background; the
				# icon is gray at rest, white while checked/down and darkGray when
				# disabled (the save-slot autosave toggle).
				theme.set_stylebox("normal", name, StyleBoxEmpty.new())
				theme.set_stylebox("hover", name, StyleBoxEmpty.new())
				theme.set_stylebox("pressed", name, StyleBoxEmpty.new())
				theme.set_stylebox("disabled", name, StyleBoxEmpty.new())
				theme.set_color("icon_normal_color", name, Color(0.498, 0.498, 0.498))
				theme.set_color("icon_hover_color", name, Color(0.498, 0.498, 0.498))
				theme.set_color("icon_pressed_color", name, Color.WHITE)
				theme.set_color("icon_disabled_color", name, Color(0.247, 0.247, 0.247))
			elif name == "clearNonei" or name == "clearNoneTogglei":
				# `Styles.clearNonei`/`clearNoneTogglei` (Styles.java:310-327):
				# `up = none`, `over = flatOver`, `down`/`checked = flatDown` (the
				# accent-bordered `flat-down-base` frame). The editor's block
				# palette relies on the checked frame to mark the selected entry
				# (`MapEditorDialog.java:872`).
				theme.set_stylebox("normal", name, StyleBoxEmpty.new())
				theme.set_stylebox("hover", name, flat(Color8(0x45, 0x45, 0x45)))
				var none_down := _flat_down_frame()
				theme.set_stylebox("pressed", name, none_down)
				theme.set_stylebox("hover_pressed", name, none_down)
				theme.set_stylebox("disabled", name, StyleBoxEmpty.new())
			else:
				theme.set_stylebox("normal", name, flat(Color(0.18, 0.20, 0.24, 1.0)))
				theme.set_stylebox("hover", name, flat(Color(0.24, 0.27, 0.32, 1.0)))
				theme.set_stylebox("pressed", name, flat(Color(0.13, 0.15, 0.18, 1.0)))
			# No keyboard focus frame (Arc draws none): the empty focus box keeps
			# Godot from painting one on clicked buttons.
			theme.set_stylebox("focus", name, StyleBoxEmpty.new())
			theme.set_color("font_color", name, Color(0.92, 0.92, 0.92))
		"PanelContainer":
			if name == "fullDialog" or name == "defaultDialog":
				# `Styles.defaultDialog`/`fullDialog` (`Styles.java:458-469`):
				# `background = windowEmpty` — no visible box; the dialog floats
				# over its stage backdrop (`black9`/`black`). `fullDialog` leaves
				# 8 px below the button row (`PlanetDialog` OK at y=640 of 648).
				var empty := StyleBoxEmpty.new()
				empty.content_margin_left = 8.0
				empty.content_margin_right = 8.0
				# Top inset of the `windowEmpty` background (L01 T2, measured in
				# runs/20261017-L-twin): titled dialogs draw their title text at
				# y=10 (java/26-pause.png, java/09-custom.png) and the untitled
				# Game Over its headline at y=19 (java/44b.png, `titleTable.remove()`
				# + `cont` pad 3 + cell pad 6). 7 px puts both within 3 px.
				empty.content_margin_top = 7.0
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
			if name == "outlineLabel":
				# `Styles.outlineLabel` (`ui/Styles.java:385-388`): white
				# `Fonts.outline`, the default font rasterized with a 2 px
				# `Color.darkGray` (#3f3f3f) border (`ui/Fonts.java:247-249,
				# 264-271`). Godot's glyph stroke radius is `outline_size / 4` px
				# (measured: 4 drew a 1 px ring), so 8 gives Java's 2 px border.
				theme.set_color("default_color", name, Color.WHITE)
				theme.set_color("font_outline_color", name, Color8(0x3f, 0x3f, 0x3f))
				theme.set_constant("outline_size", name, 8)
		"HSlider":
			if name == "defaultSlider":
				# `Styles.defaultSlider` (`Styles.java:374-378`): the
				# `sliderBack` 9-patch track (4 px splits) with the
				# `sliderKnob`/`sliderKnobOver`/`sliderKnobDown` grabbers
				# (`Tex.sliderKnob*`). Java draws no filled progress area, so
				# both `grabber_area` boxes stay empty. A disabled slider keeps
				# the plain knob like Arc (its `SliderStyle` has no disabled
				# knob).
				# Godot's `HSlider` draws the track at the box's *minimum* height,
				# centred. Arc's row box fills the whole 42 px slider (the
				# `sliderKnob` height): java/36-graphics.png shows each row as a
				# 500x42 `sliderBack` box (4 px #454545 border, black fill) with
				# the full-height knob inside (L03 T1, PAR-0413). A 21 px vertical
				# content margin makes the box's minimum height 42.
				var back := _nine_patch("slider-back", 4.0, 4.0)
				back.content_margin_top = SLIDER_ROW_HEIGHT * 0.5
				back.content_margin_bottom = SLIDER_ROW_HEIGHT * 0.5
				theme.set_stylebox("slider", name, back)
				theme.set_stylebox("grabber_area", name, StyleBoxEmpty.new())
				theme.set_stylebox("grabber_area_highlight", name, StyleBoxEmpty.new())
				var knob := MindWidgets.icon_texture("slider-knob")
				if knob != null:
					theme.set_icon("grabber", name, knob)
					theme.set_icon("grabber_disabled", name, knob)
				var knob_over := MindWidgets.icon_texture("slider-knob-over")
				if knob_over != null:
					theme.set_icon("grabber_highlight", name, knob_over)
			else:
				theme.set_stylebox("slider", name, flat(Color(0.12, 0.13, 0.15, 1.0)))
				theme.set_stylebox("grabber_area", name, flat(MindStyles.ACCENT))
		"LineEdit":
			# `Styles.defaultField` (`ui/Styles.java:398-409`): `background =
			# underline` — the `underline` 9-patch (transparent body, 4 px #454545
			# bottom rule, 12 px splits and padding on every edge), so a field is
			# a bare text line ~48 px tall, not a box. L01 T3 measured
			# java/custom.png: the search underline at y=87-90 and the map cards
			# from y=105 (the boxed 40 px port field put them 9 px higher).
			# `disabledBackground = underlineDisabled`; Arc draws no focus frame.
			# Arc's field height (`TextField.getPrefHeight`: background padding +
			# the font's line height) is 8 px taller than Godot's line box with
			# the same 12 px padding, so the vertical padding is 16.
			for state in [["normal", "underline"], ["read_only", "underline-disabled"]]:
				var field_box := _nine_patch(str(state[1]), 12.0)
				field_box.content_margin_top = 16.0
				field_box.content_margin_bottom = 16.0
				theme.set_stylebox(str(state[0]), name, field_box)
			theme.set_stylebox("focus", name, StyleBoxEmpty.new())
			theme.set_color("font_color", name, Color.WHITE)
			theme.set_color("font_placeholder_color", name, MindStyles.ARC_GRAY)
		"CheckBox":
			# `Styles.defaultCheck` (`ui/Styles.java:446-456`): the round
			# `checkOn`/`checkOff` textures with their disabled variants
			# (Godot's default square indicators otherwise). Godot's CheckBox
			# has no per-hover *icons* (Java's `checkOnOver`/`checkOver`), so the
			# hover state keeps the plain indicator.
			for state in [
				["checked", "check-on"],
				["unchecked", "check-off"],
				["checked_disabled", "check-on-disabled"],
				["unchecked_disabled", "check-disabled"],
			]:
				var texture := MindWidgets.icon_texture(str(state[1]))
				if texture != null:
					theme.set_icon(str(state[0]), name, texture)
			theme.set_color("font_color", name, Color(0.92, 0.92, 0.92))
			# Arc `CheckBox` draws no background in any state (`CheckBoxStyle`
			# has only the check drawables); Godot's CheckBox inherits the base
			# Button boxes and painted checked rows grey (L03 T3: the editor's
			# Show Blocks/Terrain/Floor rows, java/editor-view.png).
			for state in ["normal", "hover", "pressed", "hover_pressed", "disabled", "focus"]:
				theme.set_stylebox(state, name, StyleBoxEmpty.new())
		"Tree":
			theme.set_stylebox("panel", name, flat(MindStyles.GRAY_PANEL_DARK))
		_:
			pass


## `Styles.defaultPane` (`ui/Styles.java:358-361`): `vScroll = scroll` (a 24 px
## 60%-black 9-patch track) and `vScrollKnob = scrollKnobVerticalBlack` (a
## 24x40 #454545 region stretched to the knob bounds); no hover/press variants.
## Arc lays the pane's area out beside the 24 px bar, so the bar width moves the
## content: L01 T3 measured the Custom Game cards at x=39 in java/custom.png vs
## x=48 with Godot's default 8 px bar.
const SCROLL_BAR_WIDTH := 24.0
## `sliderKnob` height (29x42 region): the Arc slider's pref height and the
## height its `sliderBack` box fills.
const SLIDER_ROW_HEIGHT := 42.0


static func _apply_scroll_bar(theme: Theme) -> void:
	var track := _nine_patch("scroll", 6.0)
	track.content_margin_left = SCROLL_BAR_WIDTH * 0.5
	track.content_margin_right = SCROLL_BAR_WIDTH * 0.5
	track.content_margin_top = 0.0
	track.content_margin_bottom = 0.0
	var knob: StyleBox = flat(Color8(0x45, 0x45, 0x45))
	var knob_texture := MindWidgets.icon_texture("scroll-knob-vertical-black")
	if knob_texture != null:
		var knob_box := StyleBoxTexture.new()
		knob_box.texture = knob_texture
		knob = knob_box
	knob.content_margin_left = SCROLL_BAR_WIDTH * 0.5
	knob.content_margin_right = SCROLL_BAR_WIDTH * 0.5
	for slot in ["scroll", "scroll_focus"]:
		theme.set_stylebox(slot, "VScrollBar", track)
	for slot in ["grabber", "grabber_highlight", "grabber_pressed"]:
		theme.set_stylebox(slot, "VScrollBar", knob)


## A packed 9-patch region with uniform texture margins (the pack `splits`).
## `StyleBoxTexture`'s content margin follows the texture margin, so pass
## `content_margin` when a control measures the box's minimum size instead of
## its content (Godot's `HSlider` sizes its track from `get_minimum_size()`).
static func _nine_patch(region: String, margin: float, content_margin: float = -1.0) -> StyleBox:
	var texture := MindWidgets.icon_texture(region)
	if texture == null:
		return flat(Color(0.08, 0.08, 0.10, 1.0))
	var box := StyleBoxTexture.new()
	box.texture = texture
	box.texture_margin_left = margin
	box.texture_margin_top = margin
	box.texture_margin_right = margin
	box.texture_margin_bottom = margin
	if content_margin >= 0.0:
		box.content_margin_left = content_margin
		box.content_margin_top = content_margin
		box.content_margin_right = content_margin
		box.content_margin_bottom = content_margin
	return box


## `Tex.button*` nine-patch (`button`, `button-over`, `button-down`,
## `button-disabled`): 12 px texture margins on all four edges (the pack
## `splits`). `StyleBoxTexture`'s content margin follows the texture margin, so
## the button's content starts 12 px in — the same padding Arc applies from the
## background drawable.
static func _button_nine_patch(region: String) -> StyleBox:
	return _nine_patch(region, 12.0)


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


## `whiteui` tint drawable: a flat box with square corners.
static func _square(box: StyleBoxFlat) -> StyleBoxFlat:
	box.set_corner_radius_all(0)
	return box


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
