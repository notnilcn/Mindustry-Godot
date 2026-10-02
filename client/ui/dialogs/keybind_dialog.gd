## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/KeybindDialog.java (plan 14 §3.4, M3).
##
## Grouped keybind list + reset. The binding model (`Binding.all`, capture
## dialog, defaults) is plan 15's; the M3 shell renders the category frame and
## the `@settings.controls` title.

extends MindDialog

## Upstream categories (`KeybindDialog.findGroup`): input category ids.
const GROUPS := ["@category.general", "@category.unit", "@category.block", "@category.view"]


func _ready() -> void:
	set_title_key("@settings.controls")
	should_pause = false
	super._ready()
	_build()
	add_close_button()


func _build() -> void:
	var table := content_table()
	table.add_theme_constant_override("separation", 6)
	for group in GROUPS:
		# code-instantiated: group rows are data-driven from the binding catalogue
		# (plan 15); M3 renders the category headers.
		var header := MindWidgets.styled_label(group, "techLabel")
		table.add(header).grow_x_axis().pad(4)
		table.row()
