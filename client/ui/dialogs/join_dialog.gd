## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/JoinDialog.java (plan 14 §3.4, M3).
##
## Direct-connect + server list. The server-list fetch/parse and the
## `NetClient`/`mind-stdb` connect flow are plans 01/21/22; the M3 shell provides
## the address field, connect button, server list host and version-mismatch hook.

extends MindDialog

signal connect_requested(address: String)

var _table: MindTable = null
var _address: LineEdit = null
var _servers: MindTable = null


func _ready() -> void:
	set_title_key("@joingame")
	should_pause = false
	super._ready()
	_build()
	add_close_button()


func _build() -> void:
	_table = content_table()
	_table.add_theme_constant_override("separation", 6)

	var row := HBoxContainer.new()
	_address = MindWidgets.field(_t("@server.address"))
	_address.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_child(_address)
	var connect := MindWidgets.button(_t("@connect"))
	connect.pressed.connect(_connect)
	row.add_child(connect)
	_table.add(row).grow_x_axis().pad(4)
	_table.row()

	# code-instantiated: the discovered/remembered server list is data-driven.
	_servers = MindTable.new()
	_servers.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_table.add(_servers).grow_x_axis().grow_y_axis().pad(4)


func shown() -> void:
	_rebuild_servers()


func _rebuild_servers() -> void:
	if _servers == null:
		return
	_servers.clear_children()
	var servers: Array = _context.get("servers", [])
	if servers.is_empty():
		_servers.add(MindWidgets.label(_t("@servers.none"))).pad(8)
		return
	for entry in servers:
		# code-instantiated: server rows come from the plan-21 server list.
		var button := MindWidgets.button(str(entry))
		button.pressed.connect(_connect_to.bind(str(entry)))
		_servers.add(button).grow_x_axis().pad(3)
		_servers.row()


func _connect() -> void:
	var address := ""
	if _address != null:
		address = _address.text.strip_edges()
	if not address.is_empty():
		connect_requested.emit(address)


func _connect_to(address: String) -> void:
	connect_requested.emit(address)
