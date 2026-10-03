## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/SectorSelectDialog.java (plan 14 M5).
##
## Sector picker for campaign launch/sector-select mode. Uses the shared
## `client/ui/layout/*.gd` tree layouts over plan-12 `Planet`/`Sector` runtime
## data; the M5 shell renders a row layout with the legend/description column.

extends MindDialog

## Emitted with the chosen sector index (`-1` = cancel/default).
signal sector_chosen(sector: int)

var _layout: MindRowTreeLayout = null


func _ready() -> void:
	should_pause = false
	full_dialog = true
	super._ready()
	_build()
	add_close_button(0.0)


func _build() -> void:
	var root := content_table()
	root.add(MindWidgets.styled_label(_t("@sectors"), "techLabel")).grow_x_axis().pad(6)
	root.row()
	# code-instantiated: sector placement is computed from the plan-12 grid via
	# the shared MindRowTreeLayout; cells are data-driven.
	_layout = MindRowTreeLayout.new()
	_layout.size_flags_vertical = Control.SIZE_EXPAND_FILL
	root.add(_layout).grow_x_axis().grow_y_axis()
	var entries: Array[Dictionary] = []
	for sector_variant in campaign_section("sectors"):
		entries.append(sector_variant as Dictionary)
	_layout.set_entries(entries)
	_layout.sector_activated.connect(func(index: int) -> void: sector_chosen.emit(index))
