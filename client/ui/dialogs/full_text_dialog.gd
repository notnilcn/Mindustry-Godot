## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/FullTextDialog.java (plan 14 §3.4, M3).
##
## A scrollable plain-text dialog (`shouldPause = true`). `show_full_text` is the
## `show(String, String)` port; the title/body are bundle-resolved by the caller.
## `open_dialog("full_text", {"title": ..., "text": ...})` supplies the same pair
## through the dialog context.

extends MindDialog


func _ready() -> void:
	should_pause = true
	super._ready()
	add_close_button()


func shown() -> void:
	# Context path (`open_dialog`); empty text keeps the previous content.
	var text := str(_context.get("text", ""))
	if not text.is_empty():
		_set_content(str(_context.get("title", "")), text)


## Sets the title/body and opens the dialog (`show(titleText, text)`).
func show_full_text(dialog_title: String, text: String) -> void:
	_set_content(dialog_title, text)
	show_dialog()


## Applies the title/body pair.
func _set_content(dialog_title: String, text: String) -> void:
	title_text = dialog_title
	title_color = MindStyles.ACCENT
	_apply_title()
	if cont != null:
		for child in cont.get_children():
			cont.remove_child(child)
			child.queue_free()
		# code-instantiated: the body text is data-driven (bundle text supplied by
		# the caller); it cannot be authored in the static scene.
		var label := MindWidgets.label(text)
		label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		cont.add_child(label)
