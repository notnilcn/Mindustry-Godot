## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/PlanetDialog.java (plan 14 M5).
##
## Campaign entry. Mirrors `PlanetDialog`: a first-run "Select Starting
## Campaign" chooser (`@campaign.select`, Serpulo/Erekir cards + OK) followed by
## the planet view — the 3D globe on the `Planet` layer (`PlanetView`), a left
## planet/difficulty rail, and the selected sector's panel with Threat and an
## in-panel Launch button (`@sectors.launch`). Data comes from the
## `MindUi.campaign_views()` read models plus `MindCampaign.planet_view` for the
## `PlanetGrid` topology; launching calls the `MindCampaign.start_sector`
## facade. The chooser cards draw the vendored `sprites/planets/<name>.png`
## textures (`PlanetDialog.planetTextures`).

extends MindDialog

## Emitted when a sector is launched (kept for parity with the M5 seam).
signal sector_activated(planet: String, sector: int)

const CARD_SIZE := Vector2(320.0, 300.0)
const DISC_SIZE := 170.0

var _root: MindTable = null
var _selected_planet: String = ""
var _selected_sector: int = -1
var _ok_button: Button = null
## Planet whose campaign-complete screen was already announced (control flow).
var _complete_announced := ""


func _ready() -> void:
	set_title_text("")
	should_pause = true
	full_dialog = true
	super._ready()
	_add_space_backdrop()
	_build()
	_show_select()


func shown() -> void:
	# A planet already chosen restores the planet view; otherwise the first-run
	# chooser is shown (`PlanetDialog.shown`).
	if _selected_planet.is_empty():
		_show_select()
	else:
		_show_planet(_selected_planet)


func hidden() -> void:
	_set_planet_view_active(false)


func _build() -> void:
	_root = content_table()


## Full-screen starfield behind the chooser (`sprites/space.png`; the live g3d
## backdrop is plan 16's `Planet` layer). Adds behind the base `Dim` overlay so
## the dialog dim still applies.
func _add_space_backdrop() -> void:
	if get_node_or_null("SpaceBackdrop") != null:
		return
	var assets := MindWidgets.assets()
	if assets == null:
		return
	var texture: Variant = assets.call("find_loose", "space")
	if not (texture is Texture2D):
		return
	# code-instantiated: single backdrop image; no static scene owns it yet.
	var backdrop := TextureRect.new()
	backdrop.name = "SpaceBackdrop"
	backdrop.texture = texture
	backdrop.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	backdrop.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_COVERED
	backdrop.texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR
	backdrop.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(backdrop)
	move_child(backdrop, 0)


func _clear_root() -> void:
	if _root != null:
		_root.clear_children()


# --- Select Starting Campaign -------------------------------------------------

## `PlanetDialog` first-run chooser: `@campaign.select` with the two campaign
## planets as mutually exclusive toggle cards and an `@ok` button.
func _show_select() -> void:
	set_title_text(_t("@campaign.select"))
	_clear_root()
	_set_planet_view_active(false)
	refresh_campaign_views()

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


## The two campaign starting choices in reference order (`PlanetDialog` lists
## Serpulo before Erekir; the read model's planet order is not guaranteed).
func _campaign_planets() -> Array:
	var by_name := {}
	for planet_variant in campaign_section("planets"):
		var planet: Dictionary = planet_variant
		by_name[str(planet.get("name", ""))] = planet
	var out: Array = []
	for name in ["serpulo", "erekir"]:
		if by_name.has(name):
			out.append(by_name[name])
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
	var name_label := MindWidgets.label(_localized_planet(planet))
	name_label.add_theme_color_override("default_color", MindStyles.ACCENT)
	name_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	name_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	column.add_child(name_label)
	var texture := _planet_texture(planet)
	if texture != null:
		var image := TextureRect.new()
		image.texture = texture
		image.custom_minimum_size = Vector2(DISC_SIZE, DISC_SIZE)
		image.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		image.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		image.texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR
		image.mouse_filter = Control.MOUSE_FILTER_IGNORE
		column.add_child(image)
	else:
		var disc := Panel.new()
		disc.custom_minimum_size = Vector2(DISC_SIZE, DISC_SIZE)
		disc.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
		disc.add_theme_stylebox_override("panel", _disc_style(planet))
		disc.mouse_filter = Control.MOUSE_FILTER_IGNORE
		column.add_child(disc)
	return card


## Vendored `sprites/planets/<name>.png` (`PlanetDialog.planetTextures`), or
## `null` so the card falls back to the tinted disc.
func _planet_texture(planet: Dictionary) -> Texture2D:
	var assets := MindWidgets.assets()
	if assets == null:
		return null
	var name := str(planet.get("name", ""))
	var texture: Variant = assets.call("find_loose", "planets/%s" % name)
	if texture is Texture2D:
		return texture
	return null


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

## `PlanetDialog.setup`: globe + left planet/difficulty rail + bottom sector
## panel (`@sectors` readout with the in-panel action button).
func _show_planet(planet_name: String) -> void:
	_selected_planet = planet_name
	set_title_text("")
	_clear_root()
	_set_planet_view_active(true)
	refresh_campaign_views(planet_name)

	var view := _planet_view()
	if view != null:
		view.set_planet(planet_name)

	# code-instantiated: the planet rail is the data-driven Planets list.
	var rail := MindTable.new()
	rail.custom_minimum_size.x = 208.0
	_root.add(rail).pad(12).set_align(0)
	_build_rail(rail, planet_name)

	var selected := _find_sector(planet_name, _selected_sector)
	if selected.is_empty():
		selected = _default_sector(planet_name)
		_selected_sector = int(selected.get("id", -1))
	if view != null and not selected.is_empty():
		view.focus_tile(view.view_tile(selected))
		view.set_sector_label(_localized_sector(selected))

	clear_buttons()
	var back := add_button(_t("@back"), _close, "left", 200.0)
	back.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	if buttons != null:
		buttons.add_child(_spacer())
		buttons.add_child(_build_sector_panel(selected))
		buttons.add_child(_spacer())
	var tech_tree := add_button(_t("@techtree"), _open_tech_tree, "tree", 200.0)
	tech_tree.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	# Cells added after `show_dialog` need an explicit re-sort (MinTable is manual).
	_root.call_deferred("sort_now")
	rail.call_deferred("sort_now")
	# Deferred: `shown()` runs inside `MindUi.open_dialog`, which mutably binds
	# `MindUi`; the completion opener must run after that call returns.
	call_deferred("_maybe_show_campaign_complete")


## `Control.java:177-185`: after the final sector is captured, show the campaign
## completion screen. WS2 owns capture; this polls the live read models (no new
## signal) and announces once per planet.
func _maybe_show_campaign_complete() -> void:
	var views := campaign_views()
	var last_captured := false
	for sector_variant in views.get("sectors", []):
		var sector: Dictionary = sector_variant
		if bool(sector.get("is_last", false)) and bool(sector.get("captured", false)):
			last_captured = true
			break
	if not last_captured:
		return
	var complete: Dictionary = views.get("complete", {})
	var planet := str(complete.get("planet", views.get("planet", "")))
	if planet.is_empty() or planet == _complete_announced:
		return
	var root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if root != null and root.has_method("dialog"):
		var node: Node = root.call("dialog", "campaign_complete")
		if node != null and node.has_method("is_shown") and bool(node.call("is_shown")):
			return
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	_complete_announced = planet
	ui.call("open_dialog", "campaign_complete", JSON.stringify(complete))


## Expands between the edge buttons and the centered sector panel
## (`PlanetDialog.rebuildButtons`).
func _spacer() -> Control:
	# code-instantiated: layout-only spacer, no static scene owns it.
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	return spacer


func _build_rail(rail: MindTable, planet_name: String) -> void:
	var by_name := {}
	for planet_variant in campaign_section("planets"):
		var planet: Dictionary = planet_variant
		by_name[str(planet.get("name", ""))] = planet
	for name in ["erekir", "serpulo"]:
		if not by_name.has(name):
			continue
		var planet: Dictionary = by_name[name]
		if not bool(planet.get("visible", true)):
			continue
		rail.add(_make_planet_button(planet, name == planet_name)).grow_x_axis().pad(2)
		rail.row()
	# code-instantiated: `@campaign.difficulty` opens the campaign rules dialog.
	var difficulty := MindWidgets.icon_button("book", _t("@campaign.difficulty"), "flatTogglet")
	difficulty.custom_minimum_size = Vector2(208.0, 40.0)
	difficulty.pressed.connect(_open_campaign_rules)
	rail.add(difficulty).grow_x_axis().pad(2).set_pad_top(12)
	rail.row()
	# `SectorSelectDialog` destination picker (launch-pad/accelerator flow).
	var select := MindWidgets.icon_button("map", _t("@sectors.launchselect"), "flatTogglet")
	select.custom_minimum_size = Vector2(208.0, 40.0)
	select.pressed.connect(_open_sector_select)
	rail.add(select).grow_x_axis().pad(2)
	rail.row()


func _make_planet_button(planet: Dictionary, selected: bool) -> Button:
	# code-instantiated: rail rows carry the vendored planet art plus the
	# localized name, so each entry is built at runtime.
	var button := Button.new()
	button.theme_type_variation = "flatTogglet"
	button.toggle_mode = true
	button.button_pressed = selected
	button.custom_minimum_size = Vector2(200.0, 40.0)
	var row := HBoxContainer.new()
	row.alignment = BoxContainer.ALIGNMENT_CENTER
	row.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	row.add_theme_constant_override("separation", 8)
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	button.add_child(row)
	var texture := _planet_texture(planet)
	if texture != null:
		var icon := TextureRect.new()
		icon.texture = texture
		icon.custom_minimum_size = Vector2(24.0, 24.0)
		icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
		row.add_child(icon)
	var label := MindWidgets.label(_localized_planet(planet))
	# `RichTextLabel` min width collapses under word wrap inside a bare HBox;
	# rail labels are single-line, so disable wrapping.
	label.autowrap_mode = TextServer.AUTOWRAP_OFF
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(label)
	button.pressed.connect(func() -> void: _show_planet(str(planet.get("name", ""))))
	button.ready.connect(
		func():
			button.custom_minimum_size.x = maxf(
				button.custom_minimum_size.x, row.get_combined_minimum_size().x + 20.0
			)
	)
	return button


## The selected-sector readout (`PlanetDialog.updateSelected`): flags, stats,
## threat and the state-dependent action button (`resume`/`go`/`launch`/locked).
func _build_sector_panel(sector: Dictionary) -> Control:
	# code-instantiated: bottom-center readout rebuilt per selected sector.
	var panel := PanelContainer.new()
	panel.custom_minimum_size = Vector2(240.0, 0.0)
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 4)
	panel.add_child(column)
	if sector.is_empty():
		column.add_child(MindWidgets.label(_t("@sectors.select")))
		return panel

	var title_row := HBoxContainer.new()
	title_row.alignment = BoxContainer.ALIGNMENT_CENTER
	title_row.add_theme_constant_override("separation", 6)
	column.add_child(title_row)
	var title := MindWidgets.label(_localized_sector(sector))
	title.add_theme_color_override("default_color", MindStyles.ACCENT)
	title.autowrap_mode = TextServer.AUTOWRAP_OFF
	title.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	title_row.add_child(title)
	var preset := str(sector.get("preset", ""))
	if not preset.is_empty():
		var icon := MindWidgets.image("sector-%s" % preset)
		icon.custom_minimum_size = Vector2(28.0, 28.0)
		icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
		title_row.add_child(icon)

	var divider := ColorRect.new()
	divider.color = MindStyles.ACCENT
	divider.custom_minimum_size = Vector2(0.0, 3.0)
	column.add_child(divider)

	for state_key in _sector_state_labels(sector):
		var label := MindWidgets.label(_t(state_key))
		label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		column.add_child(label)

	if bool(sector.get("has_base", false)):
		_build_sector_stats(column, sector)
	else:
		var threat := str(sector.get("threat_band", "low"))
		column.add_child(
			MindWidgets.label("%s %s" % [_t("@sectors.threat"), _t("@threat.%s" % threat)])
		)

	var action := _sector_action(sector)
	var launch := MindWidgets.icon_button(str(action.get("icon", "play")), _t(str(action.get("label", "@sectors.launch"))))
	launch.custom_minimum_size = Vector2(170.0, 54.0)
	launch.disabled = bool(action.get("disabled", false))
	var state_tooltip := PackedStringArray()
	for state_key in _sector_state_labels(sector):
		state_tooltip.append(_t(state_key))
	launch.tooltip_text = " ".join(state_tooltip)
	launch.pressed.connect(
		func() -> void: _sector_action_pressed(_selected_planet, _selected_sector, str(action.get("mode", "launch")))
	)
	column.add_child(launch)
	return panel


## Visible state lines in `PlanetDialog.updateSelected` order; only states with
## bundle keys get a label (`shielded` stays a map/action-tooltip marker).
func _sector_state_labels(sector: Dictionary) -> PackedStringArray:
	var out := PackedStringArray()
	if bool(sector.get("attacked", false)):
		out.append("@sectors.underattack")
		if bool(sector.get("frozen", false)):
			out.append("@sector.lockdown")
	elif bool(sector.get("has_base", false)) and bool(sector.get("has_enemy_base", false)):
		out.append("@sectors.vulnerable")
	elif not bool(sector.get("has_base", false)) and bool(sector.get("has_enemy_base", false)):
		out.append("@sectors.enemybase")
	return out


## `PlanetDialog.showStats` rows, rendered inline when the live view carries a
## `stats` dictionary: playtime/attempts/wave/threat plus production, export,
## import and stored items.
func _build_sector_stats(column: VBoxContainer, sector: Dictionary) -> void:
	var stats: Dictionary = sector.get("stats", {})
	var threat := str(sector.get("threat_band", "low"))
	column.add_child(
		MindWidgets.label("%s %s" % [_t("@sectors.threat"), _t("@threat.%s" % threat)])
	)
	if stats.is_empty():
		return
	if int(stats.get("playtime_ms", 0)) > 0:
		column.add_child(
			MindWidgets.label("%s %s" % [_t("@sectors.time"), format_time_ms(int(stats.get("playtime_ms", 0)))])
		)
	if int(stats.get("attempts", 0)) > 0:
		column.add_child(MindWidgets.label("%s %d" % [_t("@sectors.attempts"), int(stats.get("attempts", 0))]))
	if int(stats.get("wave", 0)) > 0:
		column.add_child(MindWidgets.label("%s %d" % [_t("@sectors.wave"), int(stats.get("wave", 0))]))
	for key in ["production", "export", "import"]:
		var line := _stat_line("@sectors.%s" % key, stats.get(key, []))
		if not line.is_empty():
			column.add_child(MindWidgets.label(line))
	var stored := _stat_line("@sectors.stored", stats.get("stored", []))
	if not stored.is_empty():
		column.add_child(MindWidgets.label(stored))


## `label item amount` line for a `[[name, value]]` stats list ("" when empty).
func _stat_line(label_key: String, values: Array) -> String:
	var parts := PackedStringArray()
	for entry in values:
		if entry is Array and (entry as Array).size() >= 2:
			parts.append("%s %s" % [str(entry[0]), str(entry[1])])
	if parts.is_empty():
		return ""
	return "%s %s" % [_t(label_key), ", ".join(parts)]


## `PlanetDialog` action label/icon/behavior for the selected sector:
## `@sectors.resume` (being played), `@sectors.go` (has a base), `@locked` or
## `@sectors.launch`.
func _sector_action(sector: Dictionary) -> Dictionary:
	if _is_being_played(sector):
		return {"label": "@sectors.resume", "icon": "play", "mode": "resume", "disabled": false}
	if bool(sector.get("has_base", false)) or bool(sector.get("captured", false)):
		return {"label": "@sectors.go", "icon": "play", "mode": "go", "disabled": false}
	if bool(sector.get("locked", false)):
		return {"label": "@locked", "icon": "lock", "mode": "launch", "disabled": true}
	return {"label": "@sectors.launch", "icon": "play", "mode": "launch", "disabled": false}


## Whether the live session is currently playing this sector
## (`Sector.isBeingPlayed`; `MindCampaign.get_sector_state`).
func _is_being_played(sector: Dictionary) -> bool:
	var campaign := campaign_node()
	if campaign == null or not campaign.has_method("get_sector_state"):
		return false
	var state: Dictionary = campaign.call("get_sector_state")
	if not bool(state.get("campaign", false)):
		return false
	return str(state.get("planet", "")) == str(sector.get("planet", "")) \
		and int(state.get("sector", -1)) == int(sector.get("id", -2))


func _sector_action_pressed(planet_name: String, sector_id: int, mode: String) -> void:
	if mode == "resume" or mode == "go":
		_launch(planet_name, sector_id, -1, true)
		return
	_open_launch_loadout(planet_name, sector_id)


# --- Planet view glue ---------------------------------------------------------

func _planet_view() -> PlanetView:
	return get_node_or_null("/root/Spine/Planet/PlanetView") as PlanetView


## Toggles the globe layer, the dialog's own backdrop/dim, and the standalone
## menu (the globe replaces the chooser backdrop and `PlanetDialog` draws over
## an empty stage).
func _set_planet_view_active(active: bool) -> void:
	var view := _planet_view()
	if view != null:
		view.set_active(active)
	var backdrop := get_node_or_null("SpaceBackdrop")
	if backdrop != null:
		backdrop.visible = not active
	var dim := get_node_or_null("Dim")
	if dim != null:
		dim.visible = not active
	var menu := get_node_or_null("/root/Spine/Ui/UiRoot/MenuGroup")
	if menu != null:
		menu.visible = not active
	var menu_background := get_node_or_null("/root/Spine/Ui/MenuBackground")
	if menu_background != null:
		menu_background.visible = not active


## The sector selected on first open: the planet's start sector, else Ground
## Zero, else the first unlocked row (`PlanetDialog.shown`).
func _default_sector(planet_name: String) -> Dictionary:
	var sectors := _planet_sectors(planet_name)
	if sectors.is_empty():
		return {}
	var view := _planet_view()
	var start := -1
	if view != null and not view.data.is_empty():
		start = int(view.data.get("start_sector", -1))
	if view != null:
		for sector_variant in sectors:
			var sector: Dictionary = sector_variant
			if view.view_tile(sector) == start:
				return sector
	for sector_variant in sectors:
		if str(sector_variant.get("preset", "")) == "groundZero":
			return sector_variant
	for sector_variant in sectors:
		if not bool(sector_variant.get("locked", true)):
			return sector_variant
	return sectors[0]


## Selects the campaign row whose remapped grid tile matches the picked hex
## (`PlanetDialog` hover hit → `selected`).
func _select_view_tile(tile: int) -> void:
	var view := _planet_view()
	if view == null:
		return
	for sector_variant in _planet_sectors(_selected_planet):
		var sector: Dictionary = sector_variant
		if view.view_tile(sector) != tile:
			continue
		if bool(sector.get("locked", false)):
			return
		_selected_sector = int(sector.get("id", -1))
		_show_planet(_selected_planet)
		return


## Empty-screen left clicks pick the sector under the cursor while the globe is
## up; UI regions (rail, bottom bar) are excluded.
func _input(event: InputEvent) -> void:
	if not visible or not (event is InputEventMouseButton):
		return
	var click := event as InputEventMouseButton
	if click.button_index != MOUSE_BUTTON_LEFT or not click.pressed:
		return
	var view := _planet_view()
	if view == null or not view.visible:
		return
	if _over_ui(click.position):
		return
	var tile := view.pick(click.position)
	if tile < 0:
		return
	_select_view_tile(tile)
	get_viewport().set_input_as_handled()


func _over_ui(point: Vector2) -> bool:
	if _root != null and _root.get_global_rect().has_point(point):
		return true
	return buttons != null and buttons.get_global_rect().has_point(point)


## Localized planet label (`planet.<name>.name`), falling back to the read model
## (which carries the lowercase content name until the bundle is bound).
func _localized_planet(planet: Dictionary) -> String:
	var name := str(planet.get("name", ""))
	var key := "planet.%s.name" % name
	var localized := _t("@%s" % key)
	if localized.is_empty() or localized == key:
		return str(planet.get("localized", name))
	return localized


## Localized sector label (`sector.<preset>.name`).
func _localized_sector(sector: Dictionary) -> String:
	var preset := str(sector.get("preset", ""))
	if not preset.is_empty():
		var key := "sector.%s.name" % preset
		var localized := _t("@%s" % key)
		if not localized.is_empty() and localized != key:
			return localized
	return str(sector.get("name", ""))


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

## Asks for a launch loadout before entering a fresh sector
## (`PlanetDialog.playSelected` -> `LaunchLoadoutDialog`). Falls through to a
## direct launch when the dialog (or its signal) is unavailable.
func _open_launch_loadout(planet_name: String, sector_id: int) -> void:
	var ui := get_node_or_null("/root/MindUi")
	var root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui == null or root == null or not root.has_method("dialog"):
		_launch(planet_name, sector_id)
		return
	var dialog: Node = root.call("dialog", "launch_loadout")
	if dialog == null or not dialog.has_signal("loadout_chosen"):
		_launch(planet_name, sector_id)
		return
	if not dialog.loadout_chosen.is_connected(_on_loadout_chosen):
		dialog.loadout_chosen.connect(_on_loadout_chosen)
	ui.call("open_dialog", "launch_loadout", JSON.stringify({
		"planet": planet_name,
		"sector": sector_id,
	}))


func _on_loadout_chosen(index: int) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("close_dialog", "launch_loadout")
	_launch(_selected_planet, _selected_sector, index)


## Opens the sector-destination picker for the current planet and routes the
## chosen sector into the launch-loadout flow (`SectorSelectDialog`).
func _open_sector_select() -> void:
	var ui := get_node_or_null("/root/MindUi")
	var root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui == null:
		return
	if root != null and root.has_method("dialog"):
		var dialog: Node = root.call("dialog", "sector_select")
		if dialog != null and dialog.has_signal("sector_chosen") and not dialog.sector_chosen.is_connected(_on_sector_select_chosen):
			dialog.sector_chosen.connect(_on_sector_select_chosen)
	ui.call("open_dialog", "sector_select", JSON.stringify({"planet": _selected_planet}))


func _on_sector_select_chosen(sector_id: int) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("close_dialog", "sector_select")
	_open_launch_loadout(_selected_planet, sector_id)


## Enters the sector. `resume` uses the existing-base path
## (`Control.playSector`; WS2 contract) and `loadout` selects the launch
## schematic (`universe.updateLoadout`; WS2 contract).
func _launch(planet_name: String, sector_id: int, loadout: int = -1, resume: bool = false) -> void:
	if sector_id < 0:
		return
	sector_activated.emit(planet_name, sector_id)
	# Generate and install the sector's world into the sim host first so the
	# renderer draws terrain (`Control.playNewSector` world half).
	var sim_host := get_node_or_null("/root/Spine/SimHost")
	if sim_host != null and sim_host.has_method("load_sector"):
		sim_host.call("load_sector", planet_name, sector_id)
	var campaign := campaign_node()
	var started := false
	if campaign != null:
		if resume and campaign.has_method("play_sector"):
			started = bool(campaign.call("play_sector", planet_name, sector_id))
		elif campaign.has_method("start_sector"):
			if loadout >= 0:
				var result: Variant = campaign.callv("start_sector", [planet_name, sector_id, loadout])
				if result == null:
					# Facade without the optional loadout parameter yet: relaunch
					# without the selection rather than refusing to enter.
					result = campaign.call("start_sector", planet_name, sector_id)
				started = bool(result)
			else:
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
		ui.call("open_dialog", "research", JSON.stringify({"planet": _selected_planet}))


func _open_campaign_rules() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("open_dialog", "campaign_rules", JSON.stringify({"planet": _selected_planet}))


## Closes through `MindUi` so the dialog stack and pause governor stay in sync.
func _close() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("close_dialog", "planet")
	else:
		hide_dialog()
