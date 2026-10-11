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

const CARD_SIZE := Vector2(320.0, 320.0)
const DISC_SIZE := 170.0
## `Tex.pane` border in the reference frames (#454545).
const OUTLINE_GRAY := Color(0.27, 0.27, 0.27)
## Java card outline (`Styles.togglet`, sampled from the reference frame pair).
const CARD_OUTLINE := Color(0.27, 0.27, 0.27)

var _root: MindTable = null
var _selected_planet: String = ""
var _selected_sector: int = -1
var _ok_button: Button = null
## True while the first-run chooser is up (`_show_select`): the Java chooser is
## modal and has no close path — ESC/back must not dismiss it (PAR-0013).
var _chooser_active := false
## Dimmed planet-page footer entries shown behind the first-run chooser
## (PAR-0014): the Java chooser sits over the planet page, whose footer stays
## visible at ~10% brightness and inert (fill-parent modal blocks clicks).
var _footer_entries: Array[Button] = []
## Planet whose campaign-complete screen was already announced (control flow).
var _complete_announced := ""


func _ready() -> void:
	set_title_text("")
	should_pause = true
	full_dialog = true
	super._ready()
	_apply_full_dialog_margins()
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
	var view := _planet_view()
	if view != null:
		view.set_hover(-1)


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


## `Styles.fullDialog` (`windowEmpty`) margins; the reference frame puts the rail
## pane at (3, 3), the action row at x=5/x=1146 and 6 px above the bottom edge.
func _apply_full_dialog_margins() -> void:
	var panel := get_node_or_null("Center/Panel") as PanelContainer
	if panel == null:
		return
	var box := StyleBoxEmpty.new()
	box.content_margin_left = 3.0
	box.content_margin_right = 5.0
	box.content_margin_top = 1.0
	box.content_margin_bottom = 6.0
	panel.add_theme_stylebox_override("panel", box)


## `Tex.pane`/flat button box: black fill with a flat border (Java rail rows and
## the action-row buttons sampled #454545; the checked rail row takes
## `Tex.buttonDown`'s `(125, 64, 127)` outline).
func _pane_box(fill: Color, border: Color, width: float = 4.0, radius: float = 2.0, bottom_width: float = -1.0) -> StyleBoxFlat:
	var box := StyleBoxFlat.new()
	box.bg_color = fill
	box.border_width_left = int(width)
	box.border_width_top = int(width)
	box.border_width_right = int(width)
	box.border_width_bottom = int(bottom_width if bottom_width >= 0.0 else width)
	box.border_color = border
	box.corner_radius_top_left = int(radius)
	box.corner_radius_top_right = int(radius)
	box.corner_radius_bottom_left = int(radius)
	box.corner_radius_bottom_right = int(radius)
	box.content_margin_left = 2.0
	box.content_margin_right = 2.0
	box.content_margin_top = 2.0
	box.content_margin_bottom = 2.0
	return box


## Applies the pane look to one button (`Styles.flatTogglet` over `Tex.pane`).
## The unchecked rail pane draws no bottom edge (the next pane's border closes
## the stack, sampled from the reference frame).
func _apply_pane_button(button: Button, border: Color, bottom_border: float = 4.0) -> void:
	var fill := Color(0.0, 0.0, 0.0, 1.0)
	button.add_theme_stylebox_override("normal", _pane_box(fill, border, 4.0, 2.0, bottom_border))
	button.add_theme_stylebox_override("hover", _pane_box(fill, border, 4.0, 2.0, bottom_border))
	button.add_theme_stylebox_override("pressed", _pane_box(fill, border))
	button.add_theme_stylebox_override("disabled", _pane_box(fill, border, 4.0, 2.0, bottom_border))


# --- Select Starting Campaign -------------------------------------------------

## `PlanetDialog` first-run chooser: `@campaign.select` with the two campaign
## planets as mutually exclusive toggle cards and an `@ok` button. The cards
## block (cards + caption) is vertically centered in the dialog body like the
## Java BaseDialog's centered `cont` table (cards at y=120 at 648 px height).
func _show_select() -> void:
	set_title_key("@campaign.select")
	_clear_root()
	_set_planet_view_active(false)
	_set_body_centered(true)
	_chooser_active = true
	refresh_campaign_views()

	var choices := _campaign_planets()
	var row := HBoxContainer.new()
	row.alignment = BoxContainer.ALIGNMENT_CENTER
	row.add_theme_constant_override("separation", 6)
	# Java title glyphs start at y=6 with the fullDialog top margin at 0; a 4 px
	# top pad (body is centered) lands the unselected cards at Java's y=120 and
	# the selected state at y=90.
	_root.add(row).grow_x_axis().set_align(0).set_pad_top(4)
	_root.row()

	# Java's BaseDialog title renders at ~23 px glyphs (blocky font); the port's
	# default 18 px title is visibly smaller. 21 matches the reference width
	# (254 px) with glyph height within 2 px.
	if title_label != null:
		title_label.add_theme_font_size_override("font_size", 21)

	var group := ButtonGroup.new()
	group.allow_unpress = true
	var caption := MindWidgets.label(_tm("@campaign.none"))
	caption.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	caption.custom_minimum_size.x = 440.0
	caption.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	# Java `Styles.outlineLabel` line advance is 21 px at the reference frames;
	# size 20 advances 27, so the -8 separation lands a 19 px pitch: the
	# unselected two-line caption shrinks to 46 px and the five-line planet
	# blurb to 106, which keeps the centered block at Java's card y=120
	# (unselected) and y=90 (selected).
	caption.add_theme_font_size_override("normal_font_size", 20)
	caption.add_theme_constant_override("line_separation", -8)

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

	# `max_width(440)` mirrors the Java `.width(440f).wrap()` caption cell: the
	# label wraps at 440 and centers (a grown full-width cell would wrap at the
	# dialog width and lay the long planet blurbs on single lines).
	_root.add(caption).grow_x_axis().set_max_width(440.0).set_pad_top(2)
	_root.row()

	clear_buttons()
	_build_dimmed_footer()
	# Java: a single `@ok` `.size(300f, 64f)`; the dialog's `@back` was a
	# Godot-only addition (the Java corner `@back`/`@techtree` belong to the
	# planet page behind the modal — PAR-0013/0014).
	_ok_button = add_button(_t("@ok"), _on_select_ok, "ok")
	_ok_button.custom_minimum_size = Vector2(300.0, 64.0)
	_apply_ok_style(_ok_button)
	_reflow_ok_row(_ok_button)
	_ok_button.disabled = _selected_planet.is_empty()


## Java `Button` draws the icon at the left edge and the label centered in the
## remaining width; the port's shared icon row centers icon+label as one group.
func _reflow_ok_row(button: Button) -> void:
	if button.get_child_count() == 0:
		return
	var row := button.get_child(0) as HBoxContainer
	if row == null or row.get_child_count() != 2:
		return
	# Java's check glyph renders ~36 px; the shared row uses the 18 px default.
	var icon := row.get_child(0)
	if icon is Label:
		icon.add_theme_font_size_override("font_size", 36)
	row.alignment = BoxContainer.ALIGNMENT_BEGIN
	var left := Control.new()
	left.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	left.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(left)
	row.move_child(left, 1)
	var right := Control.new()
	right.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	right.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(right)


## The first-run chooser is not escapable in Java (PAR-0013): the BaseDialog
## overlay consumes ESC and no `closeOnBack` listener is registered, so the
## only exit is choosing a planet and pressing OK. The planet page behind it
## keeps the normal ESC path.
func blocks_escape() -> bool:
	return _chooser_active


## The Java chooser sits over the planet page: the page's footer Back/Tech Tree
## entries stay visible at ~10% brightness behind the modal and stay inert
## (fill-parent modal blocks clicks — PAR-0013). Only the footer is mirrored
## here; the rail/globe behind are PAR-0015. Positions are the Java-measured
## boxes at 1152x648: Back (5,589,200,54), Tech Tree (947,589,200,54).
func _build_dimmed_footer() -> void:
	_clear_dimmed_footer()
	_footer_entries.append(_footer_entry("@back", "left", true))
	_footer_entries.append(_footer_entry("@techtree", "tree", false))


func _clear_dimmed_footer() -> void:
	for button in _footer_entries:
		if is_instance_valid(button):
			button.queue_free()
	_footer_entries.clear()


func _footer_entry(text_key: String, icon: String, left: bool) -> Button:
	# code-instantiated: dimmed copies of the planet page's footer entries.
	var button := MindWidgets.icon_button(icon, _t(text_key))
	button.custom_minimum_size = Vector2(200.0, 54.0)
	button.set_anchors_preset(
		Control.PRESET_BOTTOM_LEFT if left else Control.PRESET_BOTTOM_RIGHT
	)
	button.offset_left = 5.0 if left else -205.0
	button.offset_right = 205.0 if left else -5.0
	button.offset_top = -59.0
	button.offset_bottom = -5.0
	button.modulate = Color(0.1, 0.1, 0.1)
	button.mouse_filter = Control.MOUSE_FILTER_IGNORE
	button.focus_mode = Control.FOCUS_NONE
	add_child(button)
	# Above the `Dim` overlay, below the centered panel content.
	move_child(button, 2)
	return button


## Java centers the `cont` table between the title and the button bar; the port
## body is a ScrollContainer, so disable its vertical scroll (which stretches
## the content column) and center the column for the chooser alone.
func _set_body_centered(centered: bool) -> void:
	if body != null:
		body.vertical_scroll_mode = (
			ScrollContainer.SCROLL_MODE_DISABLED if centered else ScrollContainer.SCROLL_MODE_AUTO
		)
	if cont != null:
		cont.alignment = (
			BoxContainer.ALIGNMENT_CENTER if centered else BoxContainer.ALIGNMENT_BEGIN
		)
		# A ScrollContainer only stretches its child when the child expands;
		# without this the content column keeps its minimum height and the
		# alignment has no free space to center in.
		cont.size_flags_vertical = (
			Control.SIZE_EXPAND_FILL if centered else Control.SIZE_FILL
		)


## Java `Styles.button` OK states at the reference frames: enabled = black fill
## with a 4 px #454545 outline (white icon/text); disabled = flat #252525 with
## grey text. Scoped to the chooser button; a theme-wide arc-Button style is a
## separate surface.
func _apply_ok_style(button: Button) -> void:
	for state in ["normal", "hover", "pressed"]:
		var box := StyleBoxFlat.new()
		box.bg_color = Color.BLACK
		box.border_width_left = 4
		box.border_width_top = 4
		box.border_width_right = 4
		box.border_width_bottom = 4
		box.border_color = CARD_OUTLINE
		box.corner_radius_top_left = 6
		box.corner_radius_top_right = 6
		box.corner_radius_bottom_left = 6
		box.corner_radius_bottom_right = 6
		box.content_margin_left = 12.0
		box.content_margin_right = 12.0
		button.add_theme_stylebox_override(state, box)
	var disabled := StyleBoxFlat.new()
	disabled.bg_color = Color(0.145, 0.145, 0.145)
	disabled.corner_radius_top_left = 6
	disabled.corner_radius_top_right = 6
	disabled.corner_radius_bottom_left = 6
	disabled.corner_radius_bottom_right = 6
	disabled.content_margin_left = 12.0
	disabled.content_margin_right = 12.0
	button.add_theme_stylebox_override("disabled", disabled)


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
	_apply_card_style(card)
	var texture := _planet_texture(planet)
	if texture != null:
		# Java: `.image(...).grow().scaling(Scaling.fit)` below the name row.
		# Fit box measured from the reference frame pair: (card+5, card+31)
		# 310x277 -> art 277x277 at (card+5+16, card+31); the name row overlaps
		# the texture's transparent top margin.
		var image := TextureRect.new()
		image.texture = texture
		image.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		image.offset_left = 5.0
		image.offset_top = 31.0
		image.offset_right = -5.0
		image.offset_bottom = -12.0
		image.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		image.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		image.texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR
		image.mouse_filter = Control.MOUSE_FILTER_IGNORE
		card.add_child(image)
	var column := VBoxContainer.new()
	column.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	# Name row only: glyphs at card+14 (Java 134..150 for a card at 120).
	column.offset_left = 5.0
	column.offset_top = 8.0
	column.offset_right = -5.0
	column.offset_bottom = -8.0
	column.alignment = BoxContainer.ALIGNMENT_BEGIN
	column.mouse_filter = Control.MOUSE_FILTER_IGNORE
	card.add_child(column)
	var name_label := MindWidgets.label(_localized_planet(planet))
	name_label.add_theme_color_override("default_color", MindStyles.ACCENT)
	name_label.add_theme_font_size_override("normal_font_size", 21)
	name_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	name_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	column.add_child(name_label)
	if texture == null:
		var disc := Panel.new()
		disc.custom_minimum_size = Vector2(DISC_SIZE, DISC_SIZE)
		disc.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
		disc.add_theme_stylebox_override("panel", _disc_style(planet))
		disc.mouse_filter = Control.MOUSE_FILTER_IGNORE
		column.add_child(disc)
	return card


## `Styles.togglet` card states sampled from the Java reference frames: the
## outlines are 4 px; hover takes the accent, the checked state white (arc
## `Button` checked drawable), the disabled state the normal outline. The M0
## theme's flat placeholders do not carry this.
func _apply_card_style(card: Button) -> void:
	card.add_theme_stylebox_override("normal", _card_box(CARD_OUTLINE))
	card.add_theme_stylebox_override("hover", _card_box(MindStyles.ACCENT))
	card.add_theme_stylebox_override("pressed", _card_box(Color.WHITE))
	card.add_theme_stylebox_override("hover_pressed", _card_box(Color.WHITE))
	card.add_theme_stylebox_override("disabled", _card_box(CARD_OUTLINE))


func _card_box(border: Color) -> StyleBoxFlat:
	var box := StyleBoxFlat.new()
	box.bg_color = Color.BLACK
	box.border_width_left = 4
	box.border_width_top = 4
	box.border_width_right = 4
	box.border_width_bottom = 4
	box.border_color = border
	box.corner_radius_top_left = 4
	box.corner_radius_top_right = 4
	box.corner_radius_bottom_left = 4
	box.corner_radius_bottom_right = 4
	box.content_margin_left = 5.0
	box.content_margin_right = 5.0
	box.content_margin_top = 6.0
	box.content_margin_bottom = 5.0
	return box


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
func _show_planet(planet_name: String, select_default := true) -> void:
	_selected_planet = planet_name
	_chooser_active = false
	_clear_dimmed_footer()
	set_title_text("")
	_clear_root()
	_set_planet_view_active(true)
	_set_body_centered(false)
	refresh_campaign_views(planet_name)

	var view := _planet_view()
	if view != null:
		view.set_planet(planet_name)

	# code-instantiated: the planet rail is the data-driven Planets list.
	var rail := MindTable.new()
	rail.custom_minimum_size.x = 208.0
	_root.add(rail).pad(0).set_align(0)
	_build_rail(rail, planet_name)

	var selected := _find_sector(planet_name, _selected_sector)
	if selected.is_empty() and select_default:
		selected = _default_sector(planet_name)
		_selected_sector = int(selected.get("id", -1))
	if view != null:
		if selected.is_empty():
			# `PlanetDialog.viewPlanet`: a planet switch clears the selection
			# (no panel, no band) and keeps the camera.
			view.clear_selection()
		else:
			view.focus_tile(view.view_tile(selected))
			view.set_sector_icon(str(selected.get("preset", "")))

	clear_buttons()
	# `PlanetDialog` action row: `@back`/`@techtree` are `button(text, icon)`
	# cells — icon at the left margin, label centered in what remains; the
	# reference row starts 2 px right of the rail's pane edge.
	if buttons != null:
		var lead := Control.new()
		lead.custom_minimum_size.x = 2.0
		buttons.add_child(lead)
		buttons.move_child(lead, 0)
	var back := MindWidgets.button("")
	back.custom_minimum_size = Vector2(200.0, 54.0)
	back.size_flags_vertical = Control.SIZE_SHRINK_END
	_apply_pane_button(back, OUTLINE_GRAY)
	_build_rail_content(back, MindWidgets.glyph("left", 20), _t("@back"))
	back.pressed.connect(_close)
	buttons.add_child(back)
	buttons.add_child(_spacer())
	if not selected.is_empty():
		buttons.add_child(_build_sector_panel(selected))
	buttons.add_child(_spacer())
	var tech_tree := MindWidgets.button("")
	tech_tree.custom_minimum_size = Vector2(200.0, 54.0)
	tech_tree.size_flags_vertical = Control.SIZE_SHRINK_END
	_apply_pane_button(tech_tree, OUTLINE_GRAY)
	_build_rail_content(tech_tree, MindWidgets.glyph("tree", 20), _t("@techtree"))
	tech_tree.pressed.connect(_open_tech_tree)
	buttons.add_child(tech_tree)
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
	var first_planet := true
	for name in ["erekir", "serpulo"]:
		if not by_name.has(name):
			continue
		var planet: Dictionary = by_name[name]
		if not bool(planet.get("visible", true)):
			continue
		var cell := rail.add(_make_planet_button(planet, name == planet_name)).grow_x_axis()
		cell.set_pad_left(0).set_pad_right(0).set_pad_bottom(0)
		cell.set_pad_top(2 if first_planet else 0)
		first_planet = false
		rail.row()
	# code-instantiated: `@campaign.difficulty` opens the campaign rules dialog.
	var difficulty := Button.new()
	difficulty.theme_type_variation = "flatTogglet"
	difficulty.custom_minimum_size = Vector2(208.0, 40.0)
	_apply_pane_button(difficulty, OUTLINE_GRAY)
	_build_rail_content(difficulty, MindWidgets.glyph("book", 20), _t("@campaign.difficulty"))
	difficulty.pressed.connect(_open_campaign_rules)
	rail.add(difficulty).grow_x_axis().set_pad_left(0).set_pad_right(0) \
		.set_pad_top(13).set_pad_bottom(2)
	rail.row()
	# `@sectors.launchselect` is launch-mode info text, not a rail entry
	# (`PlanetDialog.java:708-714`); the `SectorSelectDialog` seam stays in
	# `_open_sector_select` for the launch-pad flow.


func _make_planet_button(planet: Dictionary, selected: bool) -> Button:
	# code-instantiated: rail rows carry the vendored planet art plus the
	# localized name, so each entry is built at runtime.
	var button := Button.new()
	button.theme_type_variation = "flatTogglet"
	button.toggle_mode = true
	button.button_pressed = selected
	button.custom_minimum_size = Vector2(208.0, 44.0)
	_apply_pane_button(button, OUTLINE_GRAY, 0.0)
	# Checked outline: `Tex.buttonDown`'s accent border tinted by the planet's
	# `iconColor` (sampled: Serpulo (125,64,127) = accent x 7d4dff; Erekir
	# (255,121,51) = accent x ff9266).
	var planet_color := Color.from_string(str(planet.get("color", "ffffff")), Color.WHITE)
	var checked := Color(
		MindStyles.ACCENT.r * planet_color.r,
		MindStyles.ACCENT.g * planet_color.g,
		MindStyles.ACCENT.b * planet_color.b
	)
	button.add_theme_stylebox_override("pressed", _pane_box(Color.BLACK, checked))
	button.add_theme_stylebox_override("hover_pressed", _pane_box(Color.BLACK, checked))
	var icon: Control = null
	var texture := _planet_texture(planet)
	if texture != null:
		var image := TextureRect.new()
		image.texture = texture
		image.custom_minimum_size = Vector2(20.0, 20.0)
		image.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		image.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		image.mouse_filter = Control.MOUSE_FILTER_IGNORE
		icon = image
	_build_rail_content(button, icon, _localized_planet(planet))
	button.pressed.connect(func() -> void: _switch_planet(str(planet.get("name", ""))))
	return button


## `PlanetDialog.viewPlanet`: switching planets clears the selection (no sector
## panel/band until one is picked).
func _switch_planet(planet_name: String) -> void:
	if planet_name == _selected_planet:
		return
	_selected_sector = -1
	_show_planet(planet_name, false)


## `planetTable.button(name, icon, Styles.flatTogglet)`: the icon sits at the
## left margin (10 px in Java, ~13 px to its pixel row) and the label centers in
## the space that remains to the right of it.
func _build_rail_content(button: Button, icon: Control, label_text: String) -> void:
	# code-instantiated: rail content rows are data-driven per planet/preset.
	var row := HBoxContainer.new()
	row.alignment = BoxContainer.ALIGNMENT_BEGIN
	row.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	row.offset_left = 13.0
	row.offset_right = -8.0
	row.add_theme_constant_override("separation", 8)
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	button.add_child(row)
	if icon != null:
		row.add_child(icon)
	var label := MindWidgets.label(label_text)
	# `RichTextLabel` min width collapses under word wrap inside a bare HBox;
	# rail labels are single-line, so disable wrapping.
	label.autowrap_mode = TextServer.AUTOWRAP_OFF
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(label)


## The selected-sector readout (`PlanetDialog.updateSelected`): flags, stats,
## threat and the state-dependent action button (`resume`/`go`/`launch`/locked).
func _build_sector_panel(sector: Dictionary) -> Control:
	# code-instantiated: bottom-center readout rebuilt per selected sector.
	# Java builds it as a `Styles.black6` table (230 px wide, flush with the
	# action row) with a gold `@sectors.launch` accented button.
	var panel := PanelContainer.new()
	panel.custom_minimum_size = Vector2(230.0, 0.0)
	panel.add_theme_stylebox_override(
		"panel", _pane_box(Color(0.0, 0.0, 0.0, 0.6), Color(0.0, 0.0, 0.0, 0.0), 0.0, 0.0)
	)
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 0)
	panel.add_child(column)
	if sector.is_empty():
		column.add_child(MindWidgets.label(_t("@sectors.select")))
		return panel

	var title_row := HBoxContainer.new()
	title_row.alignment = BoxContainer.ALIGNMENT_CENTER
	title_row.add_theme_constant_override("separation", 6)
	# Java: panel top 514 -> title glyphs 528-552 -> divider 557 (`Styles.black6`
	# table with a 40 px title cell); heights sampled from the reference frame.
	title_row.custom_minimum_size = Vector2(0.0, 41.0)
	column.add_child(title_row)
	var title := MindWidgets.label(_localized_sector(sector))
	title.add_theme_color_override("default_color", MindStyles.ACCENT)
	title.autowrap_mode = TextServer.AUTOWRAP_OFF
	title.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	title_row.add_child(title)
	var preset := str(sector.get("preset", ""))
	if not preset.is_empty():
		var icon := MindWidgets.image("sector-%s" % preset)
		icon.custom_minimum_size = Vector2(20.0, 20.0)
		icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
		title_row.add_child(icon)

	var divider := ColorRect.new()
	divider.color = MindStyles.ACCENT
	divider.custom_minimum_size = Vector2(0.0, 3.0)
	divider.mouse_filter = Control.MOUSE_FILTER_IGNORE
	column.add_child(divider)

	for state_key in _sector_state_labels(sector):
		var label := MindWidgets.label(_t(state_key))
		label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		column.add_child(label)

	if bool(sector.get("has_base", false)):
		_build_sector_stats(column, sector)
	else:
		var threat := str(sector.get("threat_band", "low"))
		var threat_label := MindWidgets.label(
			"%s %s" % [_t("@sectors.threat"), _t("@threat.%s" % threat)]
		)
		threat_label.custom_minimum_size = Vector2(0.0, 26.0)
		threat_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
		column.add_child(threat_label)

	var action := _sector_action(sector)
	var launch := MindWidgets.button("")
	launch.custom_minimum_size = Vector2(230.0, 53.0)
	launch.add_theme_stylebox_override("normal", _pane_box(Color(0.0, 0.0, 0.0, 1.0), MindStyles.ACCENT))
	launch.add_theme_stylebox_override("hover", _pane_box(Color(0.0, 0.0, 0.0, 1.0), MindStyles.ACCENT))
	launch.add_theme_stylebox_override("pressed", _pane_box(Color(0.0, 0.0, 0.0, 1.0), MindStyles.ACCENT))
	launch.add_theme_stylebox_override("disabled", _pane_box(Color(0.0, 0.0, 0.0, 1.0), MindStyles.ACCENT))
	_build_rail_content(
		launch,
		MindWidgets.glyph(str(action.get("icon", "play")), 20),
		_t(str(action.get("label", "@sectors.launch")))
	)
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
	if not visible:
		return
	if event is InputEventMouseMotion:
		var view := _planet_view()
		if view != null and view.visible:
			_update_hover(view, (event as InputEventMouseMotion).position)
		return
	if not (event is InputEventMouseButton):
		return
	var click := event as InputEventMouseButton
	if click.button_index != MOUSE_BUTTON_LEFT or not click.pressed:
		return
	var mouse_view := _planet_view()
	if mouse_view == null or not mouse_view.visible:
		return
	if _over_ui(click.position):
		return
	var tile := mouse_view.pick(click.position)
	if tile < 0:
		return
	_select_view_tile(tile)
	# The rebuild clears the hover; Java keeps `hovered` on the sector under the
	# pointer after the click (`selected = hovered`), so re-apply it.
	_update_hover(mouse_view, click.position)
	get_viewport().set_input_as_handled()


## `PlanetDialog` hover: the sector under the cursor fades the grid in around it
## and draws the `[ name ]` label (`hoverLabel`); locked sectors draw none here.
func _update_hover(view: PlanetView, point: Vector2) -> void:
	var tile := -1
	if not _over_ui(point):
		tile = view.pick(point)
	if tile == view.hover_tile:
		return
	var name_text := ""
	if tile >= 0:
		for sector_variant in _planet_sectors(_selected_planet):
			var sector: Dictionary = sector_variant
			if view.view_tile(sector) == tile and not bool(sector.get("locked", false)):
				name_text = _localized_sector(sector)
				break
	view.set_sector_label(name_text)
	view.set_hover(tile)


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
			# `MindCampaign.start_sector(planet, sector)` takes no loadout
			# parameter, and `start_sector_with_loadout` expects item-stack JSON
			# the dialog does not build yet. Launch without the selection instead
			# of calling a 3-arg `start_sector` (that `callv` errored with
			# "function has 2 parameters" and tripped break-on-error, pausing the
			# game even though the fallback then started the sector).
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
