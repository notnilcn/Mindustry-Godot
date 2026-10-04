## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/PlanetDialog.java (plan 14 M5).
##
## Campaign entry. Mirrors `PlanetDialog`: a first-run "Select Starting
## Campaign" chooser (`@campaign.select`, Serpulo/Erekir cards + OK) followed by
## the planet view — a planet rail, the active planet's sector grid, the selected
## sector's threat readout and a Launch button (`@sectors.launch`). Data comes
## from the `MindUi.campaign_views()` read models; launching calls the
## `MindCampaign.start_sector` facade. The g3d planet renderer is plan 16, so the
## planet is drawn as a tinted disc placeholder.

extends MindDialog

## Emitted when a sector is launched (kept for parity with the M5 seam).
signal sector_activated(planet: String, sector: int)

const CARD_SIZE := Vector2(320.0, 300.0)
const DISC_SIZE := 170.0

var _root: MindTable = null
var _selected_planet: String = ""
var _selected_sector: int = -1
var _ok_button: Button = null


func _ready() -> void:
	set_title_text("")
	should_pause = true
	full_dialog = true
	super._ready()
	_build()
	_show_select()


func shown() -> void:
	# A planet already chosen restores the planet view; otherwise the first-run
	# chooser is shown (`PlanetDialog.shown`).
	if _selected_planet.is_empty():
		_show_select()
	else:
		_show_planet(_selected_planet)


func _build() -> void:
	_root = content_table()


func _clear_root() -> void:
	if _root != null:
		_root.clear_children()


# --- Select Starting Campaign -------------------------------------------------

## `PlanetDialog` first-run chooser: `@campaign.select` with the two campaign
## planets as mutually exclusive toggle cards and an `@ok` button.
func _show_select() -> void:
	set_title_text(_t("@campaign.select"))
	_clear_root()

	var choices := _campaign_planets()
	var row := HBoxContainer.new()
	row.alignment = BoxContainer.ALIGNMENT_CENTER
	row.add_theme_constant_override("separation", 8)
	_root.add(row).grow_x_axis().set_align(0).pad(6)
	_root.row()

	var group := ButtonGroup.new()
	group.allow_unpress = true
	var caption := MindWidgets.label(_tm("@campaign.none"))
	caption.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	caption.custom_minimum_size.x = 440.0
	caption.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER

	for planet_variant in choices:
		var planet: Dictionary = planet_variant
		# code-instantiated: planet cards come from the plan-12 Planets runtime.
		var card := _make_planet_card(planet)
		card.button_group = group
		card.toggled.connect(func(pressed: bool) -> void:
			if not pressed:
				return
			_selected_planet = str(planet.get("name", ""))
			caption.text = _tm("@campaign.%s" % _selected_planet)
			if _ok_button != null:
				_ok_button.disabled = false)
		card.button_pressed = str(planet.get("name", "")) == _selected_planet
		row.add_child(card)

	_root.add(caption).grow_x_axis().set_align(0).pad(8)
	_root.row()

	clear_buttons()
	_ok_button = add_button(_t("@ok"), _on_select_ok, "ok", 300.0)
	_ok_button.disabled = _selected_planet.is_empty()
	add_button(_t("@back"), _close, "left")


## The two campaign starting choices (`PlanetDialog` uses Serpulo/Erekir).
func _campaign_planets() -> Array:
	var out: Array = []
	for planet_variant in campaign_section("planets"):
		var planet: Dictionary = planet_variant
		var name := str(planet.get("name", ""))
		if name == "serpulo" or name == "erekir":
			out.append(planet)
	return out


func _make_planet_card(planet: Dictionary) -> Button:
	# code-instantiated: planet card layout is data-driven (no static scene).
	var card := MindWidgets.button("")
	card.toggle_mode = true
	card.custom_minimum_size = CARD_SIZE
	var column := VBoxContainer.new()
	column.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	column.alignment = BoxContainer.ALIGNMENT_CENTER
	column.mouse_filter = Control.MOUSE_FILTER_IGNORE
	card.add_child(column)
	var name_label := MindWidgets.label(str(planet.get("localized", planet.get("name", ""))))
	name_label.add_theme_color_override("default_color", MindStyles.ACCENT)
	name_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	name_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	column.add_child(name_label)
	var disc := Panel.new()
	disc.custom_minimum_size = Vector2(DISC_SIZE, DISC_SIZE)
	disc.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	disc.add_theme_stylebox_override("panel", _disc_style(planet))
	disc.mouse_filter = Control.MOUSE_FILTER_IGNORE
	column.add_child(disc)
	return card


func _disc_style(planet: Dictionary) -> StyleBoxFlat:
	var box := StyleBoxFlat.new()
	box.bg_color = Color.from_string(str(planet.get("color", "ffffff")), Color.WHITE)
	var radius := int(DISC_SIZE * 0.5)
	box.corner_radius_top_left = radius
	box.corner_radius_top_right = radius
	box.corner_radius_bottom_left = radius
	box.corner_radius_bottom_right = radius
	box.border_width_left = 2
	box.border_width_right = 2
	box.border_width_top = 2
	box.border_width_bottom = 2
	box.border_color = MindStyles.ACCENT
	return box


func _on_select_ok() -> void:
	if _selected_planet.is_empty():
		return
	_show_planet(_selected_planet)


# --- Planet view --------------------------------------------------------------

## `PlanetDialog.setup`: planet rail + sector grid + selected-sector panel.
func _show_planet(planet_name: String) -> void:
	_selected_planet = planet_name
	set_title_text("")
	_clear_root()

	var columns := HBoxContainer.new()
	columns.size_flags_vertical = Control.SIZE_EXPAND_FILL
	columns.add_theme_constant_override("separation", 8)
	_root.add(columns).grow_x_axis().grow_y_axis()
	_root.row()

	# code-instantiated: the planet rail is the data-driven Planets list.
	var rail := MindTable.new()
	rail.custom_minimum_size.x = 200.0
	columns.add_child(rail)
	_build_rail(rail, planet_name)

	var detail := VBoxContainer.new()
	detail.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	columns.add_child(detail)
	_build_sector_view(detail, planet_name)

	var selected := _find_sector(planet_name, _selected_sector)
	if selected.is_empty():
		var sectors := _planet_sectors(planet_name)
		if not sectors.is_empty():
			var first: Dictionary = sectors[0]
			selected = first
			_selected_sector = int(first.get("id", -1))
	_build_sector_panel(selected, planet_name)

	clear_buttons()
	var has_sector := not selected.is_empty() and not bool(selected.get("locked", false))
	if has_sector:
		var label := _t("@sectors.resume") if bool(selected.get("has_base", false)) else _t("@sectors.launch")
		add_button(label, func() -> void: _launch(planet_name, int(selected.get("id", -1))), "play", 200.0)
	add_button(_t("@back"), _close, "left")
	add_button(_t("@techtree"), _open_tech_tree, "tree", 200.0)


func _build_rail(rail: MindTable, planet_name: String) -> void:
	for planet_variant in campaign_section("planets"):
		var planet: Dictionary = planet_variant
		var name := str(planet.get("name", ""))
		if not bool(planet.get("visible", true)):
			continue
		# code-instantiated: planet rail entries come from the Planets runtime.
		var button := MindWidgets.button(str(planet.get("localized", name)))
		button.toggle_mode = true
		button.button_pressed = name == planet_name
		button.pressed.connect(func() -> void: _show_planet(name))
		rail.add(button).grow_x_axis().pad(2)
		rail.row()


func _build_sector_view(detail: VBoxContainer, planet_name: String) -> void:
	var sectors := _planet_sectors(planet_name)
	if sectors.is_empty():
		detail.add_child(_wrap_label(_tm("@sectors.select")))
		return
	# code-instantiated: sector cards are the data-driven Sector grid.
	var grid := MindTable.new()
	grid.add_theme_constant_override("separation", 4)
	var index := 0
	for sector_variant in sectors:
		var sector: Dictionary = sector_variant
		var card := MindWidgets.button(str(sector.get("name", "")))
		card.theme_type_variation = "flatBordert"
		card.disabled = bool(sector.get("locked", false))
		card.custom_minimum_size = Vector2(150.0, 54.0)
		card.pressed.connect(func() -> void:
			_selected_sector = int(sector.get("id", -1))
			_show_planet(planet_name))
		grid.add(card).pad(2)
		index += 1
		if index % 3 == 0:
			grid.row()
	detail.add_child(grid)
	# Nested grids miss the deferred sort; place now and again next frame.
	grid.sort_now()
	grid.call_deferred("sort_now")


## The selected-sector readout (`PlanetDialog.updateSelected`): name + threat.
func _build_sector_panel(sector: Dictionary, _planet_name: String) -> void:
	if sector.is_empty():
		_root.add(MindWidgets.label("@none")).grow_x_axis().pad(8)
		_root.row()
		return
	_root.add(MindWidgets.label(str(sector.get("name", "")))).grow_x_axis().pad(4)
	_root.row()
	var threat := str(sector.get("threat_band", "low"))
	_root.add(MindWidgets.label("%s%s" % [_t("@sectors.threat"), _t("@threat.%s" % threat)])).grow_x_axis().pad(2)
	_root.row()


func _wrap_label(markup_text: String) -> Control:
	# code-instantiated: markup carries BBCode and needs a rich label.
	var label := MindRichLabel.new()
	label.text = markup_text
	label.fit_content = true
	return label


## Sectors for a planet. The read model only projects the active planet's
## sector grid; for the others synthesize the start sector from the planet row
## so the start location is still selectable (`Planet.getStartSector`).
func _planet_sectors(planet_name: String) -> Array:
	if planet_name == str(campaign_views().get("planet", "")):
		return campaign_section("sectors")
	var planet := _find_planet(planet_name)
	if planet.is_empty():
		return []
	return [{
		"id": int(planet.get("start_sector", -1)),
		"name": str(planet.get("localized", planet_name)),
		"planet": planet_name,
		"threat_band": "low",
		"locked": false,
		"has_base": false,
	}]


func _find_planet(planet_name: String) -> Dictionary:
	for planet_variant in campaign_section("planets"):
		var planet: Dictionary = planet_variant
		if str(planet.get("name", "")) == planet_name:
			return planet
	return {}


func _find_sector(planet_name: String, sector_id: int) -> Dictionary:
	for sector_variant in _planet_sectors(planet_name):
		var sector: Dictionary = sector_variant
		if int(sector.get("id", -1)) == sector_id:
			return sector
	return {}


# --- Actions ------------------------------------------------------------------

func _launch(planet_name: String, sector_id: int) -> void:
	if sector_id < 0:
		return
	sector_activated.emit(planet_name, sector_id)
	var started := false
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign != null and campaign.has_method("start_sector"):
		started = bool(campaign.call("start_sector", planet_name, sector_id))
	_close()
	if started:
		_enter_game()


## Hides the standalone menu and shows the in-game HUD group (`MindUiRoot`).
func _enter_game() -> void:
	var ui_root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui_root != null and ui_root.has_method("set_menu_visible"):
		ui_root.call("set_menu_visible", false)


func _open_tech_tree() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("open_dialog", "research", "{}")


## Closes through `MindUi` so the dialog stack and pause governor stay in sync.
func _close() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("close_dialog", "planet")
	else:
		hide_dialog()
