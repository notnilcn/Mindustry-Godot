## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/PaletteDialog.java,
##         core/src/mindustry/Vars.java (`playerColors`, plan 14 §3.4, M3).
##
## A 4-column grid of the 16 vanilla player colors; selecting one emits
## `color_selected` (used by the editor/team-color flows owned by plan 19).

extends MindDialog

signal color_selected(color: Color)

## `Vars.playerColors` — names/values are the upstream ABI.
const PLAYER_COLORS := [
	"82759a", "c0c1c5", "ffffff", "7d2953",
	"ff074e", "ff072a", "ff76a6", "a95238",
	"ffa108", "feeb2c", "ffcaa8", "008551",
	"00e339", "423c7b", "4b5ef1", "2cabfe",
]


func _ready() -> void:
	set_title_key("@color")
	should_pause = false
	super._ready()
	_build()


func _build() -> void:
	var grid := content_table()
	grid.add_theme_constant_override("separation", 4)
	var col := 0
	for hex in PLAYER_COLORS:
		var color := Color(hex)
		# code-instantiated: palette entries come from the playerColors data table.
		var swatch := Button.new()
		swatch.theme_type_variation = "squareTogglei"
		swatch.custom_minimum_size = Vector2(48, 48)
		swatch.modulate = color
		swatch.pressed.connect(_choose.bind(color))
		grid.add(swatch).size(48.0)
		col += 1
		if col % 4 == 0:
			grid.row()


func _choose(color: Color) -> void:
	color_selected.emit(color)
	hide_dialog()
