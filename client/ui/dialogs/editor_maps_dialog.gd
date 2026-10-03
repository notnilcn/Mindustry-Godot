## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/EditorMapsDialog.java (plan 14 M5).
##
## Editor map browser shell: import/export/open map actions. The editor model is
## plan 19 and the map I/O plan 04/06; the M5 shell lists the committed map rows
## and emits the intent signals for the owning systems.

extends MindDialog

## Emitted for an editor map action (`import`/`export`/`open`).
signal editor_map_action(action: String, name: String)

var _list: MindTable = null


func _ready() -> void:
	set_title_key("@editor.maps")
	should_pause = false
	full_dialog = true
	super._ready()
	_build()
	add_close_button()


func _build() -> void:
	var root := content_table()
	var actions := HBoxContainer.new()
	for action in ["import", "export"]:
		# code-instantiated: the action row is a fixed editor action set.
		var button := MindWidgets.button(_t("@editor.%s" % action))
		button.pressed.connect(func() -> void: editor_map_action.emit(action, ""))
		actions.add_child(button)
	root.add(actions).grow_x_axis().pad(4)
	root.row()
	_list = MindTable.new()
	root.add(_list).grow_x_axis().grow_y_axis()
	_rebuild()


func _rebuild() -> void:
	_list.clear_children()
	for row_variant in campaign_section("maps"):
		var row: Dictionary = row_variant
		var name := str(row.get("name", ""))
		# code-instantiated: editor map rows come from the plan-06 Maps registry.
		var button := MindWidgets.button(name)
		button.pressed.connect(func() -> void: editor_map_action.emit("open", name))
		_list.add(button).grow_x_axis().pad(2)
		_list.row()
