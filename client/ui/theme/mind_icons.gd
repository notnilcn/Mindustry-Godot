## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/graphics/g3d/... and core/assets-raw/fontgen/config.json
##         (`assets/icons/icon_codes.json`), mindustry.ui.Fonts (`Icon` glyph codes).
##
## UI icon-font glyph lookup. The `icon.ttf` fontello font is keyed by the code
## points in `assets/icons/icon_codes.json`; dialogs render a glyph by assigning
## `char(code)` to a label drawn with `MindAssets.icon_font()` (`Fonts.getIcon`).
## Names are the lowercase aliases used across the UI layer (matching
## `menu_fragment.gd`'s local table), not the capitalized upstream field names.

class_name MindIcons
extends RefCounted

## Lowercase icon alias -> `icon_codes.json` code point.
const CODES := {
	"add": 59411,
	"admin": 59436,
	"arrow-note": 59444,
	"book": 59483,
	"book-open": 59393,
	"box": 59422,
	"cancel": 59413,
	"chart-bar": 59417,
	"chat": 59504,
	"copy": 59508,
	"crafting": 59440,
	"defense": 59469,
	"discord": 59405,
	"distribution": 59412,
	"down": 59397,
	"down-open": 59428,
	"download": 59513,
	"edit": 59414,
	"effect": 59475,
	"eraser": 61741,
	"exit": 59487,
	"export": 59512,
	"eye": 59534,
	"eye-off": 59535,
	"file-text": 61686,
	"filter": 61616,
	"filters": 59454,
	"folder": 59421,
	"grid": 61481,
	"hammer": 59415,
	"home": 59399,
	"host": 59485,
	"image": 59400,
	"info": 61737,
	"layers": 59455,
	"left": 59394,
	"left-open": 59429,
	"link": 59420,
	"liquid": 59484,
	"list": 59409,
	"lock": 59533,
	"lock-open": 59510,
	"logic": 59406,
	"map": 59431,
	"menu": 59532,
	"mode-attack": 59493,
	"mode-pvp": 59489,
	"mode-survival": 59499,
	"music": 59402,
	"ok": 59392,
	"paste": 59474,
	"pause": 59398,
	"pencil": 59497,
	"pick": 59511,
	"planet": 59443,
	"play": 59433,
	"players": 59506,
	"power": 59408,
	"production": 59486,
	"refresh": 59498,
	"right": 59395,
	"right-open": 59450,
	"save": 59419,
	"settings": 59516,
	"spray": 59517,
	"star": 59401,
	"steam": 59426,
	"tag": 59425,
	"tags": 59432,
	"terrain": 59492,
	"trash": 59503,
	"tree": 59509,
	"turret": 59505,
	"undo": 59445,
	"units": 59501,
	"up": 59396,
	"up-open": 59430,
	"warn": 9888,
	"waves": 59451,
	"zoom": 59530,
}


## Glyph string for an icon alias (empty when unknown).
static func glyph(name: String) -> String:
	if not CODES.has(name):
		return ""
	return char(int(CODES[name]))


## The `MindAssets` autoload (null in a bare test host).
static func _assets() -> Node:
	var loop := Engine.get_main_loop()
	if loop is SceneTree:
		return (loop as SceneTree).root.get_node_or_null("MindAssets")
	return null


## Applies the `MindAssets.icon_font()` to a label (no-op when assets are absent).
## `Control` so both `Label` (`font`) and `RichTextLabel` (`normal_font`) accept it.
static func apply_icon_font(target: Control) -> void:
	var assets := _assets()
	if assets == null:
		return
	var font: Variant = assets.call("icon_font")
	if not (font is FontFile) or font == null:
		return
	if target is RichTextLabel:
		target.add_theme_font_override("normal_font", font)
	else:
		target.add_theme_font_override("font", font)
