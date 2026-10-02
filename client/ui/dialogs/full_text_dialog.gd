## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/FullTextDialog.java (plan 14 §3.4, M3).
##
## A scrollable plain-text dialog (`shouldPause = true`). `show_full_text` is the
## `show(String, String)` port; the title/body are bundle-resolved by the caller.

extends MindDialog


func _ready() -> void:
	should_pause = true
	super._ready()
	add_close_button()


## Sets the title/body and opens the dialog (`show(titleText, text)`).
func show_full_text(dialog_title: String, text: String) -> void:
	title_text = dialog_title
	title_color = MindStyles.ACCENT
	_apply_title()
	if cont != null:
		cont.clear_children()
		# code-instantiated: the body text is data-driven (bundle text supplied by
		# the caller); it cannot be authored in the static scene.
		var label := MindWidgets.label(text)
		label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		cont.add_child(label)
	show_dialog()
