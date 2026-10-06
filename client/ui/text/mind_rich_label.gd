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


func _ready() -> void:
	_add_icon_effect()


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
