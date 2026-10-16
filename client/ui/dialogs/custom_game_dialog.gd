## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/CustomGameDialog.java
##         core/src/mindustry/ui/dialogs/MapListDialog.java
##
## Custom-game map browser: `CustomGameDialog extends MapListDialog` with
## `super("@customgame", false)` — `displayType = false`, so the cards carry no
## custom/built-in source label. The grid, filters, ordering and the `@back`
## button live in `map_list_dialog.gd`; choosing a map opens `map_play`
## (`showMap` -> `MapPlayDialog.show(map)`) and emits `map_chosen`.

extends "res://ui/dialogs/map_list_dialog.gd"

## Emitted when a custom map is chosen.
signal map_chosen(name: String)


func _ready() -> void:
	set_title_key("@customgame")
	display_type = false
	super._ready()


## The read-model built-ins are a fallback for an empty registry (missing
## assets/previews).
func _fallback_rows() -> Array:
	return campaign_section("maps")


## `showMap(map)`: `dialog.show(map)` (the shared `MapPlayDialog`).
func _show_map(map: Dictionary) -> void:
	_choose_map(str(map.get("name", "")))


func _choose_map(map_name: String) -> void:
	map_chosen.emit(map_name)
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("open_dialog"):
		ui.call("open_dialog", "map_play", JSON.stringify({"map": map_name}))
