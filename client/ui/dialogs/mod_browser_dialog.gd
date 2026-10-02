## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/ModBrowserDialog.java (plan 14 §3.4, M3).
##
## Mod browser: search field + result list + install path. The HTTP repository
## client is plan 20's service surface; the M3 shell provides the frame and the
## search input.

extends MindDialog

var _table: MindTable = null
var _search: LineEdit = null


func _ready() -> void:
	set_title_key("@mods.browser")
	should_pause = false
	super._ready()
	_table = content_table()
	_build()
	add_close_button()


func _build() -> void:
	_search = MindWidgets.field(_t("@search"))
	_table.add(_search).grow_x_axis().pad(4)
	_table.row()
	# code-instantiated: browser entries come from the plan-20 HTTP repository.
	_table.add(MindWidgets.label(_t("@mods.browser.none"))).pad(8)
