## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/LaunchLoadoutDialog.java (plan 14 M5).
##
## Launch-loadout picker: lists the core's valid loadouts from the live
## `MindCampaign` read models (`Schematics.getLoadouts(core)`) and lets the
## player pick one for the next launch. The capacity grid/item stepping is
## `LoadoutDialog` (opened through the `@resources` button); the confirm button
## emits `loadout_chosen(index)` which the planet dialog forwards to
## `MindCampaign.start_sector(planet, sector, index)`.

extends MindDialog

## Emitted with the chosen schematic index (`Schematics.all` index).
signal loadout_chosen(index: int)

var _list: MindTable = null
var _selected := -1
var _capacity := 0


func _ready() -> void:
	set_title_key("@configure")
	should_pause = false
	super._ready()
	_build()


func shown() -> void:
	refresh_campaign_views(str(_context.get("planet", "")))
	if _selected < 0:
		for loadout_variant in campaign_section("loadouts"):
			var loadout: Dictionary = loadout_variant
			if bool(loadout.get("is_default", false)):
				_selected = int(loadout.get("index", -1))
				break
	_rebuild()


func _build() -> void:
	_list = content_table()
	_rebuild()


## The default core for the selected planet (`PlanetDef.default_core`), read from
## the live loadout rows instead of the planet name (`_selected_core` bug).
func _selected_core() -> String:
	for loadout_variant in campaign_section("loadouts"):
		var loadout: Dictionary = loadout_variant
		var core := str(loadout.get("core", ""))
		if not core.is_empty():
			return core
	return ""


func _rebuild() -> void:
	_list.clear_children()
	var loadouts := campaign_section("loadouts")
	_capacity = int(campaign_views().get("launch_capacity", 0))
	if loadouts.is_empty():
		_list.add(MindWidgets.label(_t("@none"))).pad(8)
		_list.row()
		return
	for index in loadouts.size():
		var loadout: Dictionary = loadouts[index]
		# code-instantiated: loadout rows come from plan-12 Schematics.getLoadouts.
		var row := HBoxContainer.new()
		row.add_theme_constant_override("separation", 6)
		var button := MindWidgets.button(str(loadout.get("name", "")))
		button.toggle_mode = true
		button.button_pressed = int(loadout.get("index", -1)) == _selected
		button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		button.pressed.connect(_select.bind(int(loadout.get("index", -1))))
		row.add_child(button)
		var total := _requirement_total(loadout)
		var over := _capacity > 0 and total > _capacity
		row.add_child(MindWidgets.label("%d%s" % [total, "/%d" % _capacity if _capacity > 0 else ""]))
		if over:
			row.add_child(MindWidgets.label(_t("@sector.missingresources")))
		_list.add(row).grow_x_axis().pad(2)
		_list.row()
	if _capacity > 0:
		_list.add(MindWidgets.label(MindWidgets.markup_format("@launch.capacity", [_capacity]))).pad(4)
		_list.row()
	clear_buttons()
	add_button(_t("@launch.text"), _confirm, "ok", 200.0)
	var resources := add_button(_t("@resources"), _open_loadout, "edit", 200.0)
	resources.disabled = _selected < 0
	# `clear_buttons` also dropped the close button built in `_ready`; re-add it.
	add_close_button()


func _requirement_total(loadout: Dictionary) -> int:
	var total := 0
	for requirement in loadout.get("requirements", []):
		total += int(requirement[1])
	return total


func _select(index: int) -> void:
	_selected = index
	_rebuild()


func _confirm() -> void:
	if _selected < 0:
		return
	loadout_chosen.emit(_selected)


## `LaunchLoadoutDialog.@resources`: opens the capacity editor seeded with the
## live launch resources.
func _open_loadout() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	ui.call("open_dialog", "loadout", JSON.stringify({
		"planet": str(_context.get("planet", "")),
		"sector": int(_context.get("sector", -1)),
		"capacity": _capacity,
		"index": _selected,
		"core": _selected_core(),
	}))
