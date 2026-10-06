## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/SectorSelectDialog.java (plan 14 M5).
##
## Sector picker for campaign launch/sector-select mode. Filters the live
## `MindCampaign` sector rows to the current planet's authored, unlockable
## presets with a search field (`SectorSelectDialog.matches`) and emits
## `sector_chosen(id)` for the launch flow.

extends MindDialog

## Emitted with the chosen sector index (`-1` = cancel/default).
signal sector_chosen(sector: int)

var _layout: MindRowTreeLayout = null
var _search: LineEdit = null
var _planet := ""


func _ready() -> void:
	should_pause = false
	full_dialog = true
	super._ready()
	_build()
	add_close_button(0.0)


func shown() -> void:
	_planet = str(_context.get("planet", ""))
	refresh_campaign_views(_planet)
	_rebuild()


func _build() -> void:
	var root := content_table()
	root.add(MindWidgets.styled_label(_t("@sectors.launchselect"), "techLabel")).grow_x_axis().pad(6)
	root.row()
	# code-instantiated: the search row is a single data-driven filter field.
	var search_row := HBoxContainer.new()
	_search = MindWidgets.field(_t("@search"))
	_search.text_changed.connect(func(_value: String) -> void: _rebuild())
	search_row.add_child(_search)
	root.add(search_row).grow_x_axis().pad(4)
	root.row()
	# code-instantiated: sector placement is computed from the plan-12 grid via
	# the shared MindRowTreeLayout; cells are data-driven.
	_layout = MindRowTreeLayout.new()
	_layout.size_flags_vertical = Control.SIZE_EXPAND_FILL
	root.add(_layout).grow_x_axis().grow_y_axis()
	_layout.sector_activated.connect(func(index: int) -> void: sector_chosen.emit(index))


## `SectorSelectDialog.matches`: the current planet's unlockable (authored)
## sectors matching the search text.
func _rebuild() -> void:
	if _layout == null:
		return
	var entries: Array[Dictionary] = []
	var needle := ""
	if _search != null:
		needle = _search.text.to_lower()
	for sector_variant in campaign_section("sectors"):
		var sector: Dictionary = sector_variant
		if str(sector.get("planet", "")) != _planet:
			continue
		if str(sector.get("preset", "")).is_empty():
			continue
		var name := str(sector.get("name", ""))
		if not needle.is_empty() and not name.to_lower().contains(needle):
			continue
		entries.append(sector)
	_layout.set_entries(entries)
