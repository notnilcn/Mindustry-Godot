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
## Java `HostDialog.java:22-34`: name max 40; `name`/`color-0` shared settings.
const NAME_SETTING := "name"
const COLOR_SETTING := "color-0"
const MAX_NAME_LENGTH := 40

var _table: MindTable = null
var _name_field: LineEdit = null
var _port_field: LineEdit = null
var _host_button: Button = null
var _mode := "@mode.survival"
var _color := Color.WHITE
var _swatch_tint: ColorRect = null


func _ready() -> void:
	set_title_key("@hostserver")
	should_pause = false
	super._ready()
	_build()
	add_close_button()


func _build() -> void:
	_table = content_table()
	_table.add_theme_constant_override("separation", 6)
	# code-instantiated: the name + colour row (`HostDialog.java:22-34`); the
	# 300 px row width and final paddings land in I08 T12.
	var name_row := HBoxContainer.new()
	name_row.add_theme_constant_override("separation", 6)
	name_row.add_child(MindWidgets.label(_t("@name")))
	_name_field = MindWidgets.field(_t("@name"))
	_name_field.max_length = MAX_NAME_LENGTH
	# `HostDialog.java:24`: the field opens with the remembered name.
	_name_field.text = _stored_name()
	_name_field.text_changed.connect(_store_name)
	_name_field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	name_row.add_child(_name_field)
	# code-instantiated: the square colour swatch is tinted at runtime by the
	# colour chosen in the `picker` dialog (`HostDialog.java:28-34`).
	var swatch := Button.new()
	swatch.theme_type_variation = "squarei"
	swatch.custom_minimum_size = Vector2(54, 54)
	_swatch_tint = ColorRect.new()
	_color = _stored_color()
	_swatch_tint.color = _color
	_swatch_tint.mouse_filter = Control.MOUSE_FILTER_IGNORE
	swatch.add_child(_swatch_tint)
	_swatch_tint.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	swatch.pressed.connect(_open_picker)
	name_row.add_child(swatch)
	_table.add(name_row).grow_x_axis().pad(4)
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


func _open_picker() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	ui.call("open_dialog", "picker", "{}")
	var picker := get_tree().root.find_child("picker", true, false)
	if picker != null and picker.has_signal("color_selected") and not picker.is_connected("color_selected", _on_color_selected):
		picker.connect("color_selected", _on_color_selected)


func _on_color_selected(color: Color) -> void:
	_store_color(color)


## `Core.settings.getString("name")` (`HostDialog.java:24`).
func _stored_name() -> String:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("settings_get_text"):
		return ""
	return str(ui.call("settings_get_text", NAME_SETTING, ""))


## `Core.settings.getInt("color-0")` as `Color.rgba8888()`; unset (0) keeps the
## palette default (`HostDialog.java:31`).
func _stored_color() -> Color:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("settings_get_int"):
		return Color.WHITE
	var rgba := int(ui.call("settings_get_int", COLOR_SETTING, 0))
	if rgba == 0:
		return Color.WHITE
	return Color8((rgba >> 24) & 0xFF, (rgba >> 16) & 0xFF, (rgba >> 8) & 0xFF, rgba & 0xFF)


## `HostDialog.java:24-27`: every name edit updates `player.name` and the
## `name` setting immediately.
func _store_name(text: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("settings_put_text"):
		ui.call("settings_put_text", NAME_SETTING, text)
	_push_identity()


## `HostDialog.java:31-33`: picking a colour updates `player.color` and the
## `color-0` setting immediately.
func _store_color(color: Color) -> void:
	_color = color
	if _swatch_tint != null:
		_swatch_tint.color = color
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("settings_put_int"):
		ui.call("settings_put_int", COLOR_SETTING, color.to_rgba32())
	_push_identity()


## Pushes `player.name`/`player.color` to the relay identity reducers; the
## facade queues them and sends on connect (I08 T2).
func _push_identity() -> void:
	if _name_field == null:
		return
	var net := get_node_or_null("/root/Spine/MindNet")
	if net != null and net.has_method("set_player_identity"):
		net.call("set_player_identity", _name_field.text, _color.to_rgba32())


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
	_push_identity()
	host_requested.emit(host_name, _mode)
	# `HostDialog.runHost` hides the dialog once the host call is dispatched.
	if ui != null and ui.has_method("close_dialog"):
		ui.call("close_dialog", "host")
