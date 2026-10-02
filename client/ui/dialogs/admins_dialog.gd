## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/AdminsDialog.java (plan 14 §3.4, M3).
##
## Admin list + unadmin. The `NetServer.admins` roster and `unAdminPlayer`
## reducer are plan 21; the M3 shell renders the roster frame and the
## `@server.admins.none` empty state.

extends MindDialog

var _rows: MindTable = null


func _ready() -> void:
	set_title_key("@server.admins")
	should_pause = false
	super._ready()
	_rows = content_table()
	add_close_button()


func shown() -> void:
	_rebuild()


func _rebuild() -> void:
	if _rows == null:
		return
	_rows.clear_children()
	var admins: Array = _context.get("admins", [])
	if admins.is_empty():
		_rows.add(MindWidgets.label(_t("@server.admins.none"))).pad(8)
		return
	for entry in admins:
		# code-instantiated: roster rows are data-driven from the plan-21 admin view.
		var row := MindWidgets.label("[lightgray]%s" % str(entry))
		_rows.add(row).grow_x_axis().pad(4)
		_rows.row()
