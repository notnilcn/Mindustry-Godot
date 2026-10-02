## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: plan 14 §5 M1 verification (dev-only widget gallery).
##
## Instantiates one of each M1 widget for in-engine/MCP inspection
## (`/root/Spine/Ui/UiRoot/DialogLayer/widget_gallery`).

extends MindDialog


func _ready() -> void:
	title_text = "Widget Gallery"
	super._ready()
	_build()


func _build() -> void:
	if cont == null:
		return
	# code-instantiated: the gallery is a dev harness that adds one of each widget;
	# its contents are intentionally data/code-driven (plan 14 §5 M1).
	var grid := MindTable.new()
	grid.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	cont.add_child(grid)

	grid.add(MindWidgets.bar("Bar", MindStyles.ACCENT, 0.5)).size(200, 24).pad(2)
	grid.add(MindWidgets.warning_bar()).size(200, 24).pad(2)
	grid.row()

	grid.add(MindWidgets.grid_image(8, 4)).size(200, 64).pad(2)
	grid.add(MindWidgets.check("Check", true)).pad(2)
	grid.row()

	grid.add(MindWidgets.items_display()).pad(2)
	grid.add(MindWidgets.core_items_display()).pad(2)
	grid.row()

	grid.add(MindWidgets.label("Label")).pad(2)
	grid.add(MindWidgets.button("Button")).pad(2)

	add_close_button()
