## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/PlanetDialog.java (plan 14 M5).
##
## Planet list + selected-planet sector grid. Data comes from `MindUi`'s M5
## campaign read models (plan 12 `Campaign`/`Planet`/`Sector`); the g3d planet
## view/interface renderer is plan 16, so the M5 shell renders the sector cards.

extends MindDialog

## Emitted when a sector card is activated (launch/select flow).
signal sector_activated(planet: String, sector: int)

var _list: MindTable = null
var _detail: MindTable = null
var _planet_index := 0


func _ready() -> void:
	set_title_key("@planets")
	should_pause = true
	full_dialog = true
	super._ready()
	_build()
	add_close_button()


func _build() -> void:
	var columns := HBoxContainer.new()
	columns.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_table_add(columns)
	# code-instantiated: the planet rail is the data-driven Planets list.
	_list = MindTable.new()
	_list.custom_minimum_size.x = 190
	columns.add_child(_list)
	_detail = MindTable.new()
	_detail.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	columns.add_child(_detail)
	_rebuild_list()
	_rebuild_detail()


func _table_add(node: Control) -> void:
	var table := content_table()
	table.add(node).grow_x_axis().grow_y_axis()


func _rebuild_list() -> void:
	_list.clear_children()
	var planets := campaign_section("planets")
	for index in planets.size():
		var planet: Dictionary = planets[index]
		var label := str(planet.get("localized", planet.get("name", "")))
		# code-instantiated: planet cards come from the plan-12 Planet runtime.
		var button := MindWidgets.button(label)
		button.toggle_mode = true
		button.button_pressed = index == _planet_index
		button.disabled = not bool(planet.get("visible", true))
		button.pressed.connect(_select_planet.bind(index))
		_list.add(button).grow_x_axis().pad(2)
		_list.row()


func _select_planet(index: int) -> void:
	_planet_index = index
	_rebuild_list()
	_rebuild_detail()


func _rebuild_detail() -> void:
	_detail.clear_children()
	var planets := campaign_section("planets")
	if planets.is_empty():
		_detail.add(MindWidgets.label("@none")).pad(8)
		return
	var planet: Dictionary = planets[clampi(_planet_index, 0, planets.size() - 1)]
	_detail.add(MindWidgets.styled_label(str(planet.get("localized", "")), "techLabel")).grow_x_axis().pad(6)
	_detail.row()
	_detail.add(MindWidgets.label("sectors: %d  captured: %d" % [
		int(planet.get("sector_count", 0)), int(planet.get("sectors_captured", 0))
	])).grow_x_axis().pad(2)
	_detail.row()
	var sectors := campaign_section("sectors")
	if str(planet.get("name", "")) != str(campaign_views().get("planet", "")):
		return
	# code-instantiated: sector cards are the data-driven Sector grid.
	var grid := MindTable.new()
	for sector_variant in sectors:
		var sector: Dictionary = sector_variant
		var card := MindWidgets.button(str(sector.get("name", "")))
		card.theme_type_variation = "flatBordert"
		card.disabled = bool(sector.get("locked", false))
		card.tooltip_text = "threat: %s" % str(sector.get("threat_band", "low"))
		card.pressed.connect(_activate.bind(str(planet.get("name", "")), int(sector.get("id", 0))))
		grid.add(card).pad(2)
		grid.row()
	_detail.add(grid).grow_x_axis()


func _activate(planet: String, sector: int) -> void:
	sector_activated.emit(planet, sector)
