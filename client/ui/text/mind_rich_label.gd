## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: arc.scene.ui.Label + mindustry.core.UI markup translation.
##
## BBCode `RichTextLabel` with the `[icon]` effect attached. Text is expected to
## be pre-translated by `mind-core::ui::text::render_markup` (plan 14 §3.7); this
## class never parses Mindustry markup itself.

class_name MindRichLabel
extends RichTextLabel


func _init() -> void:
	bbcode_enabled = true
	# Arc `Label` does not wrap unless `setWrap(true)` (plan 14 §3.7). Godot's
	# `RichTextLabel` defaults to word-smart wrapping, whose minimum width is
	# hard-coded to 1px and whose minimum height is the content height at that
	# width, so every label inside a row/table cell collapses. Callers opt into
	# wrapping explicitly by assigning `autowrap_mode`.
	autowrap_mode = TextServer.AUTOWRAP_OFF
	fit_content = true
	scroll_active = false
	# The `[icon]` effect must be attached before the first text assignment:
	# widgets assign `.text` before the label enters the tree, and a later
	# `custom_effects` change does not re-parse the shaped text (PAR-0016 -
	# `[icon name="…"]` rendered literally for every pre-tree assignment).
	_add_icon_effect()


func _ready() -> void:
	_add_icon_effect()
	_mirror_font_color()


## Arc `Label.setColor` / `.color(...)` callers port as the `Label`
## `font_color` override, but a `RichTextLabel` draws with `default_color`:
## mirror the override so `label.add_theme_color_override("font_color", c)`
## colours this label too (L07 T3: the Banned Blocks pane titles stayed white).
func _notification(what: int) -> void:
	if what == NOTIFICATION_THEME_CHANGED:
		_mirror_font_color()


func _mirror_font_color() -> void:
	if not has_theme_color_override("font_color"):
		return
	var color := get_theme_color("font_color")
	if has_theme_color_override("default_color") and get_theme_color("default_color") == color:
		return
	add_theme_color_override("default_color", color)


## Assigns already-translated BBCode text.
func set_markup_text(value: String) -> void:
	text = value


func _add_icon_effect() -> void:
	for effect in custom_effects:
		if effect is MindIconEffect:
			return
	# code-instantiated: one shared effect instance per label, created lazily when
	# the label joins the tree (there is no scene-declared RichTextEffect resource).
	custom_effects.append(MindIconEffect.new())
