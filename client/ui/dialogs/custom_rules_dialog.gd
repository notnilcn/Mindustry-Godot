## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/CustomRulesDialog.java
##         (plan 14 §3.4, M3).
##
## Custom game rules editor. The full `Rules` model is plan 12 and the editor
## row builders (`check/number/text/team`) land with M5; the M3 shell provides
## the category rail and the rule table host + close button.

extends MindDialog

## Category keys from `CustomRulesDialog.buildCategories`.
const CATEGORIES := ["@rules.title", "@rules.environment", "@rules.waves", "@rules.units", "@rules.build", "@rules.enemy", "@rules.planet"]

var _table: MindTable = null
var _selected := 0


func _ready() -> void:
	set_title_key("@mode.custom")
	should_pause = false
	super._ready()
	_table = content_table()
	add_close_button()
	_build()


func _build() -> void:
	var rail := HBoxContainer.new()
	for index in CATEGORIES.size():
		# code-instantiated: category tabs are data-driven from the Rules model.
		var button := Button.new()
		button.text = _t(CATEGORIES[index])
		button.theme_type_variation = "flatTogglet"
		button.toggle_mode = true
		button.button_pressed = index == 0
		button.pressed.connect(_select.bind(index))
		rail.add_child(button)
	_table.add(rail).grow_x_axis().pad(4)
	_table.row()
	_rebuild()


func _select(index: int) -> void:
	_selected = index
	_rebuild()


func _rebuild() -> void:
	# code-instantiated: rule rows come from the plan-12 Rules model (M5 builders).
	var header := MindWidgets.styled_label(CATEGORIES[_selected], "techLabel")
	_table.add(header).grow_x_axis().pad(6)
	_table.row()
