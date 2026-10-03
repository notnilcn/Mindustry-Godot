## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/layout/RowTreeLayout.java (plan 14 M5).
##
## Row layout used by `SectorSelectDialog`: one button per sector, ordered by
## the plan-12 grid id. Emits `sector_activated` on click.

class_name MindRowTreeLayout
extends MindTreeLayout

## Emitted with the sector grid id.
signal sector_activated(id: int)

var _entries: Array = []


func set_entries(entries: Array) -> void:
	_entries = entries
	_rebuild()


func _rebuild() -> void:
	for child in get_children():
		child.queue_free()
	# code-instantiated: sector buttons are the plan-12 Sector grid (data-driven).
	var column := VBoxContainer.new()
	column.size_flags_vertical = Control.SIZE_EXPAND_FILL
	add_child(column)
	layout_entries(_entries)
	for entry in _entries:
		var sector: Dictionary = entry
		var button := MindWidgets.button(str(sector.get("name", "")))
		button.disabled = bool(sector.get("locked", false))
		button.tooltip_text = "threat: %s" % str(sector.get("threat_band", "low"))
		button.pressed.connect(func() -> void: sector_activated.emit(int(sector.get("id", 0))))
		column.add_child(button)
