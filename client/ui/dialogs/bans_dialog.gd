## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/BansDialog.java (plan 14 §3.4, M3).
##
## Ban list + unban. `NetServer.admins.getBanned()` and `unbanPlayerID` are
## plan 21; the M3 shell renders the list frame and `@server.bans.none`.

extends MindDialog

var _rows: MindTable = null


func _ready() -> void:
	set_title_key("@server.bans")
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
	var banned: Array = _context.get("banned", [])
	if banned.is_empty():
		_rows.add(MindWidgets.label(_t("@server.bans.none"))).pad(8)
		return
	for entry in banned:
		# code-instantiated: ban rows are data-driven from the plan-21 admin view.
		_rows.add(MindWidgets.label("[lightgray]%s" % str(entry))).grow_x_axis().pad(4)
		_rows.row()
