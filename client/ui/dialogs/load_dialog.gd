## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/LoadDialog.java (plan 14 §3.4, M3).
##
## Save-slot grid. The slot listing/meta/delete flow is plan 04's `MindIo`; the
## M3 shell renders the slot frame and emits `slot_selected(slot)`.

extends MindDialog

signal slot_selected(slot: String)

var _grid: MindTable = null


func _ready() -> void:
	set_title_key("@loadgame")
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
	var slots: Array = _context.get("slots", [])
	if slots.is_empty():
		_grid.add(MindWidgets.label(_t("@save.none"))).pad(8)
		return
	for entry in slots:
		# code-instantiated: slot rows are data-driven from the plan-04 save listing.
		var button := MindWidgets.button(str(entry))
		button.pressed.connect(_select.bind(str(entry)))
		_grid.add(button).grow_x_axis().pad(4)
		_grid.row()


func _select(slot: String) -> void:
	slot_selected.emit(slot)
	hide_dialog()
