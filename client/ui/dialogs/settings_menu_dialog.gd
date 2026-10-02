## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/SettingsMenuDialog.java
##         (plan 14 §3.4, M3).
##
## Category column + settings table. The `Setting` row model and persistence live
## in plan 04's `SettingsStore`; the M3 shell provides the category rail
## (`game/graphics/sound/dev/main/data`) and the table host. `shouldPause = true`
## matches upstream.

extends MindDialog

## Category order/keys from `SettingsMenuDialog.SettingsCategory`.
const CATEGORIES := [
	{"key": "@settings.game", "icon": "settings"},
	{"key": "@settings.graphics", "icon": "terrain"},
	{"key": "@settings.sound", "icon": "effect"},
	{"key": "@settings.dev", "icon": "logic"},
	{"key": "@settings.main", "icon": "home"},
	{"key": "@settings.data", "icon": "map"},
]

var _table_host: MindTable = null
var _selected := 0


func _ready() -> void:
	set_title_key("@settings")
	should_pause = true
	super._ready()
	_build()
	add_close_button()


func _build() -> void:
	var root := content_table()
	root.add_theme_constant_override("separation", 8)
	var columns := HBoxContainer.new()
	columns.size_flags_vertical = Control.SIZE_EXPAND_FILL
	root.add(columns).grow_x_axis().grow_y_axis()

	# code-instantiated: the category rail is the data-driven SettingsCategory list.
	var rail := VBoxContainer.new()
	for index in CATEGORIES.size():
		var entry: Dictionary = CATEGORIES[index]
		var button := MindWidgets.image_button(str(entry.icon), "flati")
		button.text = _t(str(entry.key))
		button.expand_icon = true
		button.toggle_mode = true
		button.button_pressed = index == 0
		button.pressed.connect(_select_category.bind(index))
		rail.add_child(button)
	columns.add_child(rail)

	_table_host = MindTable.new()
	_table_host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	columns.add_child(_table_host)
	_rebuild()


func _select_category(index: int) -> void:
	_selected = index
	_rebuild()


func _rebuild() -> void:
	if _table_host == null:
		return
	_table_host.clear_children()
	var category: Dictionary = CATEGORIES[_selected]
	# code-instantiated: setting rows come from the plan-04 SettingsStore; M3
	# renders the selected category header until the store is wired.
	_table_host.add(MindWidgets.styled_label(str(category.key), "techLabel")).grow_x_axis().pad(6)
