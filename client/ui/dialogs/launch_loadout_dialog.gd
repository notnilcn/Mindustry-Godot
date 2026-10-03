## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/LaunchLoadoutDialog.java (plan 14 M5).
##
## Launch-loadout picker: lists the core's valid loadouts (plan-12 `Schematics`
## `getLoadouts(core)`) and lets the player pick one for the next launch. The
## capacity grid/item stepping is `LoadoutDialog`; this dialog only selects.

extends MindDialog

## Emitted with the chosen schematic index.
signal loadout_chosen(index: int)

var _list: MindTable = null


func _ready() -> void:
	set_title_key("@launch.loadout")
	should_pause = false
	super._ready()
	_build()
	add_close_button()


func _build() -> void:
	_list = content_table()
	_rebuild()


func _selected_core() -> String:
	var views := campaign_views()
	return str(views.get("planet", ""))


func _rebuild() -> void:
	_list.clear_children()
	var loadouts := campaign_section("loadouts")
	if loadouts.is_empty():
		_list.add(MindWidgets.label("@launch.noloadout")).pad(8)
		return
	for index in loadouts.size():
		var loadout: Dictionary = loadouts[index]
		# code-instantiated: loadout rows come from plan-12 Schematics.getLoadouts.
		var label := str(loadout.get("name", ""))
		if bool(loadout.get("is_default", false)):
			label = "%s %s" % [label, _t("@default")]
		var button := MindWidgets.button(label)
		button.pressed.connect(func() -> void: loadout_chosen.emit(int(loadout.get("index", 0))))
		_list.add(button).grow_x_axis().pad(2)
		_list.row()
