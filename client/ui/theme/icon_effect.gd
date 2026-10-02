## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/Fonts.java (registerIcon/glyph injection),
##         core/src/mindustry/core/UI.java (formatIcons).
##
## OD-UI3: Godot exposes no runtime font-glyph injection, so content icons render
## through this `[icon name="…"]` RichTextEffect instead. `_process_custom_fx`
## can tint/position the run; the atlas blit itself is approximated by the theme
## until the plan-03 M7 handshake lands (tracked in plan 14 §8 R7). The effect is
## attached by [MindRichLabel].

class_name MindIconEffect
extends RichTextEffect

var bbcode := "icon"


func _process_custom_fx(char_fx: CharFXTransform) -> bool:
	var icon_name := str(char_fx.env.get("name", ""))
	if icon_name.is_empty():
		return true
	# Preserve the current color for the icon run; unknown icons stay invisible
	# rather than rendering a literal name.
	var color := char_fx.color
	if color.a <= 0.0:
		color.a = 1.0
	char_fx.color = color
	return true
