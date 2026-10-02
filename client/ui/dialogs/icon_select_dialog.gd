## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/IconSelectDialog.java,
##         core/src/mindustry/Vars.java (`accessibleIcons`, plan 14 §3.4, M3).
##
## Icon grid for server/team customization: the vanilla `accessibleIcons` set
## plus content icons. Selecting emits `icon_selected(region_name)`; the locked
## filter and content-icon set are extended when plan 02/03 enumerate content.

extends MindDialog

signal icon_selected(region_name: String)

## `Vars.accessibleIcons` — order is the upstream ABI.
const ACCESSIBLE_ICONS := [
	"effect", "power", "logic", "units", "liquid", "production", "defense",
	"turret", "distribution", "crafting", "settings", "cancel", "zoom", "ok",
	"star", "home", "pencil", "up", "down", "left", "right", "hammer",
	"warning", "tree", "admin", "map", "modePvp", "terrain", "modeSurvival",
	"commandRally", "commandAttack",
]

## Columns cap mirrors `min(20, width / Scl.scl(52))` (fixed for the shell).
const COLUMNS := 8


func _ready() -> void:
	set_title_key("@icons")
	should_pause = false
	super._ready()
	_build()


func _build() -> void:
	var grid := content_table()
	grid.add_theme_constant_override("separation", 4)
	var index := 0
	for icon_name in ACCESSIBLE_ICONS:
		# code-instantiated: icon entries are the data-driven accessibleIcons set.
		var button := MindWidgets.image_button(icon_name, "flati")
		button.custom_minimum_size = Vector2(48, 48)
		button.pressed.connect(_choose.bind(icon_name))
		grid.add(button).size(48.0)
		index += 1
		if index % COLUMNS == 0:
			grid.row()
	add_close_button()


func _choose(region_name: String) -> void:
	icon_selected.emit(region_name)
	hide_dialog()
