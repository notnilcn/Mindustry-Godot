## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/SaveDialog.java (plan 14 §3.4, M3).
##
## Save (extends `LoadDialog` upstream): slot grid + a trailing "new save" row.
## The `.msav` write flow is plan 04; the M3 shell exposes the new-save row and
## emits `save_requested(name)`.

extends MindDialog

signal save_requested(slot: String)

var _grid: MindTable = null


func _ready() -> void:
	set_title_key("@savegame")
	should_pause = false
	super._ready()
	_grid = content_table()
	add_close_button()


func shown() -> void:
	_rebuild()


func _rebuild() -> void:
	if _grid == null:
		return
	_grid.clear_children()
	# code-instantiated: the new-save row is a data-driven list entry.
	var new_button := MindWidgets.button(_t("@save.new"))
	new_button.pressed.connect(_request_new)
	_grid.add(new_button).grow_x_axis().pad(4)
	_grid.row()
	var slots: Array = _context.get("slots", [])
	for entry in slots:
		var button := MindWidgets.button(str(entry))
		button.pressed.connect(_request.bind(str(entry)))
		_grid.add(button).grow_x_axis().pad(4)
		_grid.row()


func _request_new() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	# code-instantiated: the save-name prompt is a one-off transient overlay.
	ui.call("show_text_input", _t("@save.name"), _t("@save.name"), 64, "", false, false)
	if not ui.is_connected("text_input_result", _on_name):
		ui.connect("text_input_result", _on_name)


func _on_name(text: String) -> void:
	if not text.is_empty():
		save_requested.emit(text)
		hide_dialog()


func _request(slot: String) -> void:
	save_requested.emit(slot)
	hide_dialog()
