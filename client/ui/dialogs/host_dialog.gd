## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/HostDialog.java (plan 14 §3.4, M3).
##
## Host configuration (name + gamemode) and launch. The host/launch flow is
## plans 12/21; the M3 shell provides the name field, mode buttons and the
## `host_requested` signal.

extends MindDialog

signal host_requested(host_name: String, mode: String)

## Gamemode keys (`@mode.*`) offered by the host dialog.
const MODES := ["@mode.survival", "@mode.sandbox", "@mode.attack", "@mode.pvp", "@mode.editor"]

var _table: MindTable = null
var _name_field: LineEdit = null
var _mode := "@mode.survival"


func _ready() -> void:
	set_title_key("@hostserver")
	should_pause = false
	super._ready()
	_build()
	add_close_button()


func _build() -> void:
	_table = content_table()
	_table.add_theme_constant_override("separation", 6)
	_name_field = MindWidgets.field(_t("@name"))
	_table.add(_name_field).grow_x_axis().pad(4)
	_table.row()
	var row := HBoxContainer.new()
	for mode in MODES:
		# code-instantiated: gamemode buttons are the data-driven mode list.
		var button := Button.new()
		button.text = _t(mode)
		button.theme_type_variation = "flatTogglet"
		button.toggle_mode = true
		button.button_pressed = mode == _mode
		button.pressed.connect(_select_mode.bind(mode))
		row.add_child(button)
	_table.add(row).grow_x_axis().pad(4)
	_table.row()
	var host := MindWidgets.button(_t("@host"))
	host.pressed.connect(_host)
	buttons.add_child(host)


func _select_mode(mode: String) -> void:
	_mode = mode


func _host() -> void:
	var host_name := ""
	if _name_field != null:
		host_name = _name_field.text
	host_requested.emit(host_name, _mode)
