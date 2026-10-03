## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/SchematicsDialog.java (plan 14 M5).
##
## Schematic library: search + list + detail/preview panel over plan-12's
## `Schematics` registry. Import/export and `.msch` I/O are plan 04/12; the M5
## shell renders the loaded library and emits the edit/export intents.

extends MindDialog

## Emitted when a schematic action is requested (`export`/`delete`/`edit`).
signal schematic_action(index: int, action: String)

var _field: LineEdit = null
var _list: MindTable = null
var _detail: MindTable = null
var _selected := 0


func _ready() -> void:
	set_title_key("@schematics")
	should_pause = true
	full_dialog = true
	super._ready()
	_build()
	add_close_button()


func _build() -> void:
	var root := content_table()
	_field = MindWidgets.field(_t("@schematics.search"))
	_field.text_changed.connect(func(_text: String) -> void: _rebuild_list())
	root.add(_field).grow_x_axis().pad(4)
	root.row()
	var columns := HBoxContainer.new()
	columns.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_list = MindTable.new()
	_list.custom_minimum_size.x = 260
	columns.add_child(_list)
	_detail = MindTable.new()
	_detail.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	columns.add_child(_detail)
	root.add(columns).grow_x_axis().grow_y_axis()
	_rebuild_list()
	_rebuild_detail()


func _rebuild_list() -> void:
	_list.clear_children()
	var needle := _field.text.to_lower() if _field != null else ""
	var schematics := campaign_section("schematics")
	for index in schematics.size():
		var schematic: Dictionary = schematics[index]
		var name := str(schematic.get("name", ""))
		if not needle.is_empty() and not name.to_lower().contains(needle):
			continue
		# code-instantiated: schematic rows are the plan-12 Schematics registry.
		var button := MindWidgets.button(name)
		button.toggle_mode = true
		button.button_pressed = index == _selected
		button.pressed.connect(_select.bind(index))
		_list.add(button).grow_x_axis().pad(2)
		_list.row()


func _select(index: int) -> void:
	_selected = index
	_rebuild_list()
	_rebuild_detail()


func _rebuild_detail() -> void:
	_detail.clear_children()
	var schematics := campaign_section("schematics")
	if schematics.is_empty():
		return
	var schematic: Dictionary = schematics[clampi(_selected, 0, schematics.size() - 1)]
	_detail.add(MindWidgets.styled_label(str(schematic.get("name", "")), "techLabel")).grow_x_axis().pad(6)
	_detail.row()
	_detail.add(MindWidgets.label("%dx%d  tiles: %d" % [
		int(schematic.get("width", 0)), int(schematic.get("height", 0)), int(schematic.get("tiles", 0))
	])).pad(2)
	_detail.row()
	var requirements: Array = schematic.get("requirements", [])
	for requirement in requirements:
		_detail.add(MindWidgets.label("%s x%d" % [str(requirement[0]), int(requirement[1])])).pad(1)
		_detail.row()
	var buttons := HBoxContainer.new()
	# code-instantiated: the action row is parameterized by the selected schematic.
	for action in ["edit", "export", "delete"]:
		var button := MindWidgets.button(_t("@schematics.%s" % action))
		button.pressed.connect(func() -> void: schematic_action.emit(_selected, action))
		buttons.add_child(button)
	_detail.add(buttons).pad(4)
