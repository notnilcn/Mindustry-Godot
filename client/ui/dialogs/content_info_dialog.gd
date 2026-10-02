## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/ContentInfoDialog.java
##         (plan 14 §3.4/§3.7, M3).
##
## `compute_stats()` rows (plan 02/07/12 + `ui::stat_display`) render here once
## the content read models are exposed to `mind-gdext`; the M3 shell shows the
## icon, name, description/details supplied through the dialog context and the
## "patched" indicator hook.

extends MindDialog

var _table: MindTable = null


func _ready() -> void:
	set_title_key("@info.title")
	should_pause = false
	super._ready()
	_table = content_table()
	add_close_button()


func shown() -> void:
	if _table == null:
		return
	_table.clear_children()
	var content_name := str(_context.get("content", ""))
	# code-instantiated: content stats/description are data-driven from the plan-02
	# content registry (populated when the inspector passes a content key).
	var icon := MindWidgets.image(content_name)
	icon.custom_minimum_size = Vector2(48, 48)
	_table.add(icon).size(48.0).pad(4)
	_table.add(MindWidgets.styled_label("[accent]%s" % content_name, "techLabel")).pad(4)
	_table.row()
	var description := str(_context.get("description", ""))
	if not description.is_empty():
		var body := MindWidgets.label(description)
		body.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		_table.add(body).grow_x_axis().pad(6)
		_table.row()
	var details := str(_context.get("details", ""))
	if not details.is_empty():
		_table.add(MindWidgets.label(details)).grow_x_axis().pad(6)
		_table.row()
