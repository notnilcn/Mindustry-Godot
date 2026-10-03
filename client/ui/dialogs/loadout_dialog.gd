## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/LoadoutDialog.java (plan 14 M5).
##
## Core loadout editor: capacity grid + requirement list over plan-12's
## `Schematics`/`ItemSeq`. The interactive item stepping hooks plan-08's
## `ItemSeq`, so the M5 shell renders the capacity/requirement summary and
## emits `loadout_changed` when a loadout is selected.

extends MindDialog

## Emitted when the active loadout changes.
signal loadout_changed(index: int)

var _summary: MindTable = null
var _capacity := 0


func _ready() -> void:
	set_title_key("@loadout")
	should_pause = false
	full_dialog = true
	super._ready()
	_build()
	add_close_button()


func _build() -> void:
	_summary = content_table()
	_rebuild()


func _rebuild() -> void:
	_summary.clear_children()
	var loadouts := campaign_section("loadouts")
	if loadouts.is_empty():
		return
	for index in loadouts.size():
		var loadout: Dictionary = loadouts[index]
		_capacity = maxi(_capacity, int(loadout.get("tiles", 0)))
		var header := MindWidgets.styled_label(str(loadout.get("name", "")), "techLabel")
		_summary.add(header).grow_x_axis().pad(4)
		_summary.row()
		# code-instantiated: requirement rows are the plan-12 loadout ItemSeq.
		for requirement in loadout.get("requirements", []):
			_summary.add(MindWidgets.label("%s x%d" % [str(requirement[0]), int(requirement[1])])).pad(1)
			_summary.row()
		var choose := MindWidgets.button(_t("@loadout.select"))
		choose.pressed.connect(func() -> void: loadout_changed.emit(int(loadout.get("index", 0))))
		_summary.add(choose).pad(2)
		_summary.row()
