## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/TraceDialog.java (plan 14 §3.4, M3).
##
## Player trace info with per-field copy buttons. `TraceInfo` is plan 21's; the
## M3 shell reads the fields from the dialog context and copies to clipboard.

extends MindDialog

var _table: MindTable = null


func _ready() -> void:
	set_title_key("@trace")
	should_pause = false
	super._ready()
	_table = content_table()
	add_close_button()


func shown() -> void:
	if _table == null:
		return
	_table.clear_children()
	var player_name := str(_context.get("player", ""))
	var info: Dictionary = _context.get("info", {})
	_add_copy_row("trace.playername", {"name": player_name}, player_name)
	_add_copy_row("trace.ip", info, str(info.get("ip", "")))
	_add_copy_row("trace.language", info, str(info.get("locale", "")))
	_add_copy_row("trace.id", info, str(info.get("uuid", "")))
	_add_scalar("trace.modclient", info.get("modded", false))
	_add_scalar("trace.mobile", info.get("mobile", false))
	_add_scalar("trace.times.joined", info.get("timesJoined", 0))
	_add_scalar("trace.times.kicked", info.get("timesKicked", 0))


func _add_copy_row(_key: String, _info: Dictionary, value: String) -> void:
	# code-instantiated: trace fields are data-driven from the plan-21 TraceInfo.
	var row := HBoxContainer.new()
	var button := MindWidgets.button(_t("@copy"))
	button.pressed.connect(_copy.bind(value))
	row.add_child(button)
	row.add_child(MindWidgets.label(value))
	_table.add(row).grow_x_axis().pad(2)
	_table.row()


func _add_scalar(key: String, value: Variant) -> void:
	_table.add(MindWidgets.label("%s: %s" % [_t(key), str(value)])).grow_x_axis().pad(2)
	_table.row()


func _copy(value: String) -> void:
	DisplayServer.clipboard_set(value)
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("show_info", _t("@copied"))
