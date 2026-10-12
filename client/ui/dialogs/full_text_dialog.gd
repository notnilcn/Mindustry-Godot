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
	# `FullTextDialog.java`: `super("")` + `addCloseButton()` - the text floats
	# over the defaultDialog backdrop (no boxed panel), so use the full-dialog
	# style like the Java frame (r00-05).
	full_dialog = true
	super._ready()
	# `Styles.defaultDialog` (windowEmpty) dims ~0.9; the shared MindDialog dim
	# is 0.55 (same mapping as the pause dialog, PAR-0038/PAR-0148).
	var dim := get_node_or_null("Dim") as ColorRect
	if dim != null:
		dim.color = Color(0.0, 0.0, 0.0, 0.9)
	add_close_button()


func shown() -> void:
	# Context path (`open_dialog`); empty text keeps the previous content.
	var text := str(_context.get("text", ""))
	if not text.is_empty():
		_set_content(str(_context.get("title", "")), text)
	# `PlanetDialog.java:216-222`: the Campaign Changes Notice is a BaseDialog
	# with an `@ok` button (280x64, `Icon.ok`) instead of the generic close.
	if bool(_context.get("ok", false)):
		clear_buttons()
		add_button(_t("@ok"), hide_dialog, "ok", 280.0)


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
		# `PlanetDialog.java:216-222`: the notice body is centered with
		# `maxWidth(500)`.
		if bool(_context.get("ok", false)):
			label.custom_minimum_size.x = 500.0
		# `FullTextDialog.java:19`: `.labelAlign(Align.center)`.
		label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		cont.add_child(label)
