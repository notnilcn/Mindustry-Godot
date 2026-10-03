## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/CustomGameDialog.java (plan 14 M5).
##
## Custom-game map list (extends the shared `map_list_dialog` shell). The map
## registry is plan 06/19; the M5 shell lists the M5 map rows from `MindUi`'s
## campaign read models and emits `map_chosen` for the play flow.

extends MindDialog

## Emitted when a custom map is chosen.
signal map_chosen(name: String)

var _list: MindTable = null
var _search := ""


func _ready() -> void:
	set_title_key("@customgame")
	should_pause = false
	full_dialog = true
	super._ready()
	_build()
	add_close_button()


func _build() -> void:
	var root := content_table()
	var field := MindWidgets.field(_t("@search"))
	field.text_changed.connect(func(text: String) -> void:
		_search = text.to_lower()
		_rebuild())
	root.add(field).grow_x_axis().pad(4)
	root.row()
	_list = MindTable.new()
	root.add(_list).grow_x_axis().grow_y_axis()
	_rebuild()


func _rebuild() -> void:
	_list.clear_children()
	for row_variant in campaign_section("maps"):
		var row: Dictionary = row_variant
		var name := str(row.get("name", ""))
		if not _search.is_empty() and not name.to_lower().contains(_search):
			continue
		# code-instantiated: map rows come from the plan-06 Maps registry.
		var button := MindWidgets.button(name)
		button.pressed.connect(func() -> void: map_chosen.emit(name))
		_list.add(button).grow_x_axis().pad(2)
		_list.row()
