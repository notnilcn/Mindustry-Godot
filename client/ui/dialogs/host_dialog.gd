## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/HostDialog.java (plan 14 §3.4, M3).
##
## Host configuration (name + gamemode + port) and the `@host` intent. The
## dialog emits `host_requested`; `UiRoot` turns that into the
## `MindNet.create_match` call (`HostDialog.runHost`).

extends MindDialog

signal host_requested(host_name: String, mode: String)

## Gamemode keys (`@mode.*`) offered by the host dialog.
const MODES := ["@mode.survival", "@mode.sandbox", "@mode.attack", "@mode.pvp", "@mode.editor"]
## Upstream `Core.settings.getInt("port", port)` default (`HostDialog.java:57`).
const DEFAULT_PORT := 6567

var _table: MindTable = null
var _name_field: LineEdit = null
var _port_field: LineEdit = null
var _host_button: Button = null
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
	_name_field.max_length = 40
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
	# code-instantiated: the port row is a fixed composite (label + validated
	# field) mirroring `HostDialog.java:51-73`.
	var port_row := HBoxContainer.new()
	port_row.add_theme_constant_override("separation", 6)
	port_row.add_child(MindWidgets.label(_t("@server.port")))
	_port_field = MindWidgets.field()
	_port_field.text = str(DEFAULT_PORT)
	_port_field.max_length = 5
	_port_field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_port_field.text_changed.connect(_on_port_changed)
	port_row.add_child(_port_field)
	_table.add(port_row).grow_x_axis().pad(4)
	_table.row()
	# code-instantiated: the host action button is created here because the
	# validated port field gates its disabled state at runtime.
	_host_button = MindWidgets.button(_t("@host"))
	_host_button.pressed.connect(_host)
	buttons.add_child(_host_button)


func _select_mode(mode: String) -> void:
	_mode = mode


## `HostDialog.portField.valid`: an integer in `1..=65535`.
func _port_valid() -> bool:
	if _port_field == null:
		return true
	if not _port_field.text.is_valid_int():
		return false
	var port := _port_field.text.to_int()
	return port >= 1 and port <= 65535


func _on_port_changed(_text: String) -> void:
	if _host_button != null:
		_host_button.disabled = not _port_valid()


## The validated port for the host call (`HostDialog.runHost`);
## `HostParams::default()` has no port, so the dialog's default is used.
func port() -> int:
	if _port_field == null or not _port_valid():
		return DEFAULT_PORT
	return _port_field.text.to_int()


func _host() -> void:
	if not _port_valid():
		return
	var host_name := ""
	if _name_field != null:
		host_name = _name_field.text
	var ui := get_node_or_null("/root/MindUi")
	if host_name.strip_edges().is_empty():
		# `HostDialog.java:68-71`: an empty name shows `@noname` instead of hosting.
		if ui != null and ui.has_method("show_info"):
			ui.call("show_info", _t("@noname"))
		return
	host_requested.emit(host_name, _mode)
	# `HostDialog.runHost` hides the dialog once the host call is dispatched.
	if ui != null and ui.has_method("close_dialog"):
		ui.call("close_dialog", "host")
