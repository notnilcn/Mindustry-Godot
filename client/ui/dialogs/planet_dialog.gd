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
## Last `lastplanet` value written this session (T9: the settings write is a
## full read-merge-write, so only write when the planet actually changes).
var _last_planet_written: String = ""
## True while the first-run chooser is up (`_show_select`): the Java chooser is
## modal and has no close path — ESC/back must not dismiss it (PAR-0013).
var _chooser_active := false
## Dimmed planet-page footer entries shown behind the first-run chooser
## (PAR-0014): the Java chooser sits over the planet page, whose footer stays
## visible at ~10% brightness and inert (fill-parent modal blocks clicks).
var _footer_entries: Array[Button] = []
## Planet whose campaign-complete screen was already announced (control flow).
var _complete_announced := ""
## `PlanetDialog.sectorsShown` / `searchText` (top-right sector list).
var _sectors_shown := false
var _sector_search := ""
var _sector_list: VBoxContainer = null
var _sector_list_backdrop: PanelContainer = null
## Stage layers hidden while the planet page covers the screen (PAR-0146/0147):
## node path -> visibility captured when the page first covered the stage, so
## closing it restores exactly what was showing before (the live game's HUD +
## world, or the standalone menu). Empty while nothing is covered.
var _saved_layers: Dictionary = {}
## `PlanetDialog.dragged`/`clicked` pair: a left-drag orbits the globe and the
## release after a drag must not select a sector.
var _dragged := false
var _pressed_position := Vector2.ZERO
## Whether the current left press started on the globe (not the rail/lists).
var _press_on_globe := false
## Pending rename/icon targets for the sector panel's pencil and icon picker.
var _rename_target := -1
var _icon_target := -1
## `PlanetDialog.newPresets` (`PlanetDialog.java:270-288`): sector ids to
## reveal, reversed so the planet's last sector sits first; `_preset_show`
## counts ticks (Arc `Time.delta`) and `_reveal_showed` is the one-shot
## announcement flag.
var _last_sector_id := -1
var _new_presets: Array = []
var _preset_show := 0.0
var _reveal_showed := false

const MENU_GROUP_PATH := "/root/Spine/Ui/UiRoot/MenuGroup"
const MENU_BACKGROUND_PATH := "/root/Spine/Ui/MenuBackground"
const HUD_GROUP_PATH := "/root/Spine/Ui/UiRoot/HudGroup"
const WORLD_PATH := "/root/Spine/World"


func _ready() -> void:
	set_title_text("")
	should_pause = true
	full_dialog = true
	super._ready()
	_apply_full_dialog_margins()
	_add_space_backdrop()
	_build()
	_show_select()
	# `ui.showTextInput` result for the sector rename pencil (`PlanetDialog.java:
	# 1200-1206`); the load dialog keeps the same always-connected pattern.
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_signal("text_input_result") \
			and not ui.is_connected("text_input_result", _on_text_input_result):
		ui.connect("text_input_result", _on_text_input_result)


func shown() -> void:
	# `PlanetDialog.show` (`PlanetDialog.java:257`): `selected = hovered =
	# launchSector = null` — the page opens with no sector selected (no panel,
	# no band); `lookAt(getLastSector())` below only turns the camera.
	_selected_sector = -1
	# "view current planet by default" (`PlanetDialog.show`): inside a campaign
	# sector the page opens on that sector's planet, never the chooser.
	_adopt_current_sector()
	# A planet already chosen restores the planet view. Otherwise the first-run
	# chooser is shown only when no planet has a base and the player never chose
	# (`PlanetDialog.java:183` `!settings.getBool("campaignselect") &&
	# !content.planets().contains(p -> p.sectors.contains(Sector::hasBase))`);
	# a returning player opens straight on the page (PAR-0158).
	var chooser_shown := false
	if _selected_planet.is_empty():
		var based := _planets_with_base()
		if not _flag("campaignselect") and based.is_empty():
			_show_select()
			chooser_shown = true
		else:
			_show_planet(_boot_planet(based), false)
	else:
		_show_planet(_selected_planet, false)
	# `PlanetDialog.show` (`:260-288`): zoom reset, camera to the last sector
	# and the new-preset reveal collection. The chooser never reaches here.
	if not chooser_shown:
		_start_preset_reveal()
	# `PlanetDialog.java:216-222`: after a Serpulo remap the one-time "Campaign
	# Changes Notice" shows, but only when the chooser branch was not taken.
	if not chooser_shown:
		call_deferred("_maybe_show_remap_notice")


func hidden() -> void:
	_remove_sector_list()
	_set_planet_view_active(false)
	# Back/ESC returns to whatever the page covered (the running sector or the
	# menu), PAR-0147.
	_restore_stage()
	var view := _planet_view()
	if view != null:
		view.set_hover(-1)


## `PlanetDialog.show` (`PlanetDialog.java:260-288`): reset the zoom, turn the
## camera to the planet's last sector (`lookAt(getLastSector())`) and collect
## this planet's new presets (`take_new_presets`); the collected list gets the
## last sector appended and is reversed, then `_process` replays the reveal.
func _start_preset_reveal() -> void:
	_new_presets.clear()
	_preset_show = 0.0
	_reveal_showed = false
	var view := _planet_view()
	if view == null or _selected_planet.is_empty():
		return
	view.zoom = 1.0
	view.zoom_target = 1.0
	_last_sector_id = int(view.data.get("last_sector", -1))
	var last_sector := _find_sector(_selected_planet, _last_sector_id)
	if not last_sector.is_empty():
		view.look_at_tile(view.view_tile(last_sector))
	var campaign := campaign_node()
	if campaign == null or not campaign.has_method("take_new_presets"):
		return
	var collected: Variant = campaign.call("take_new_presets", _selected_planet)
	if not (collected is PackedInt32Array):
		return
	for id in collected as PackedInt32Array:
		_new_presets.append(int(id))
	if not _new_presets.is_empty():
		_new_presets.append(_last_sector_id)
		_new_presets.reverse()


## `PlanetDialog.act` reveal loop (`PlanetDialog.java:986-1003`): while
## `newPresets` is non-empty the camera slerps to the peeked sector (alpha
## 0.11) and the target zoom is 0.75; after 20 ticks the unlock announcement
## fires once (only when more than the last sector remains), and each entry is
## popped after 144 ticks (Arc `Time.delta` = 60 fps ticks).
func _process(delta: float) -> void:
	if _new_presets.is_empty() or not visible:
		return
	var view := _planet_view()
	if view == null:
		return
	_preset_show += delta * 60.0
	var to_id := int(_new_presets[_new_presets.size() - 1])
	var sector := _find_sector(_selected_planet, to_id)
	if not sector.is_empty():
		view.look_at_tile(view.view_tile(sector), 0.11)
	view.zoom_target = 0.75
	if _preset_show >= 20.0 and not _reveal_showed and _new_presets.size() > 1:
		_reveal_showed = true
		var ui := get_node_or_null("/root/MindUi")
		if ui != null:
			ui.call(
				"announce",
				"%s [accent]%s" % [MindIcons.glyph("lock-open"), _localized_sector(sector)],
				2.0
			)
	if _preset_show > 144.0:
		_new_presets.pop_back()
		_reveal_showed = false
		_preset_show = 0.0


func _build() -> void:
	_root = content_table()


## `PlanetDialog.show` (`PlanetDialog.java:244-248`): `state.rules.sector !=
## null` -> `state.planet = sector.planet`. Only the planet is adopted; Java
## leaves `selected` null (the camera turns to `getLastSector()`). Reads the
## live session through `MindCampaign.get_sector_state`.
func _adopt_current_sector() -> void:
	var campaign := campaign_node()
	if campaign == null or not campaign.has_method("get_sector_state"):
		return
	var state: Variant = campaign.call("get_sector_state")
	if not (state is Dictionary):
		return
	var sector_state: Dictionary = state
	if not bool(sector_state.get("campaign", false)):
		return
	var planet_name := str(sector_state.get("planet", ""))
	if planet_name.is_empty():
		return
	_selected_planet = planet_name
	refresh_campaign_views(planet_name)


## Names of the landable planets with at least one `Sector.hasBase` sector
## (`content.planets().contains(p -> p.sectors.contains(Sector::hasBase))`).
## The planet rows carry no base flag and the read model only projects the
## requested planet's sectors, so each landable planet's `has_base` sector rows
## are read through `campaign_views_json(planet)` without touching the shared
## cache. Only runs while no planet is chosen yet (the first open of a session).
func _planets_with_base() -> Array:
	var out: Array = []
	for planet_variant in campaign_section("planets"):
		var planet: Dictionary = planet_variant
		var planet_name := str(planet.get("name", ""))
		if planet_name.is_empty() or not bool(planet.get("has_grid", true)):
			continue
		var views := _live_campaign_views(planet_name)
		for sector_variant in views.get("sectors", []) as Array:
			var sector: Dictionary = sector_variant
			if bool(sector.get("has_base", false)):
				out.append(planet_name)
				break
	return out


## Planet the page opens on when the chooser is skipped (`PlanetDialog`
## constructor: `state.planet = settings.getString("lastplanet", "serpulo")`,
## then `show()` falls back to serpulo when that planet is not `selectable`).
## `selectable` here: a landable planet (`accessible`) or one the player has a
## base on; fall back to the first planet with a base, then serpulo (PAR-0158).
func _boot_planet(based: Array) -> String:
	var last := _text("lastplanet", "serpulo")
	var planet := _find_planet(last)
	if not planet.is_empty() and (based.has(last) or bool(planet.get("accessible", false))):
		return last
	if not based.is_empty():
		return str(based[0])
	return "serpulo"


## `Core.settings.getBool(key)` through the MindUi store (false when the
## accessor is unavailable).
func _flag(key: String) -> bool:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("settings_get_flag"):
		return bool(ui.call("settings_get_flag", key))
	return false


## `Core.settings.getString(key, default)` through the MindUi store.
func _text(key: String, default: String) -> String:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("settings_get_text"):
		return str(ui.call("settings_get_text", key, default))
	return default


## `Core.settings.put(key, value)` (bool) through the MindUi store.
func _put_flag(key: String, value: bool) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("settings_put_flag"):
		ui.call("settings_put_flag", key, value)


## `Core.settings.put(key, value)` (string) through the MindUi store.
func _put_text(key: String, value: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("settings_put_text"):
		ui.call("settings_put_text", key, value)


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


## `PlanetDialog` first-run chooser: `@campaign.select` with the two campaign
## planets as mutually exclusive toggle cards and an `@ok` button. The cards
## block (cards + caption) is vertically centered in the dialog body like the
## Java BaseDialog's centered `cont` table (cards at y=120 at 648 px height).
func _show_select() -> void:
	_remove_sector_list()
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
	# The planet rail table is `t.top().left()` (`PlanetDialog.java:732-735`):
	# pinned to the top-left corner, not centred along the top (PAR-0414, L03
	# T2). The chooser keeps the centred layout.
	if _root != null:
		_root.set_table_top(not centered)
		_root.set_table_left(not centered)


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


## `PlanetDialog` chooser OK (`PlanetDialog.java:207-213`): `state.planet =
## selected; selectSector(state.planet.getStartSector());
## settings.put("campaignselect", true)`.
func _on_select_ok() -> void:
	if _selected_planet.is_empty():
		return
	_put_flag("campaignselect", true)
	var planet := _find_planet(_selected_planet)
	if not planet.is_empty():
		_selected_sector = int(planet.get("start_sector", -1))
	_show_planet(_selected_planet)


## `PlanetDialog.java:216-222`: the one-time Serpulo remap notice
## (`hadSerpuloRemaps && settings.getBoolOnce("serpulo-remaps-notice")`).
## Deferred: `shown()` runs inside `MindUi.open_dialog`, which mutably binds
## `MindUi`; a nested `open_dialog` must wait for that call to return (PAR-0066).
func _maybe_show_remap_notice() -> void:
	var campaign := campaign_node()
	if campaign == null or not campaign.has_method("take_serpulo_remaps_notice"):
		return
	if not bool(campaign.call("take_serpulo_remaps_notice")):
		return
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	ui.call(
		"open_dialog", "full_text",
		JSON.stringify({
			"title": _t("@campaign.rework.title"),
			"text": _tm("@campaign.rework.serpulo"),
			"ok": true,
			"keep_previous": true,
		})
	)


# --- Planet view --------------------------------------------------------------

## `ResearchDialog.hidden(ui.planet::setup)`: re-runs the planet page for the
## selected planet (without re-selecting a default sector).
func refresh_page() -> void:
	_show_planet(_selected_planet, false)


## `PlanetDialog.setup`: globe + left planet/difficulty rail + bottom sector
## panel (`@sectors` readout with the in-panel action button).
func _show_planet(planet_name: String, select_default := true) -> void:
	# `PlanetDialog.java:247,902`: showing/switching a planet records it so the
	# next boot reopens on it. Writing `lastplanet` is a full settings.bin
	# read-merge-write, so only write when the value changes (T9).
	if _last_planet_written != planet_name:
		_put_text("lastplanet", planet_name)
		_last_planet_written = planet_name
	_selected_planet = planet_name
	_chooser_active = false
	# `PlanetDialog.viewPlanet` (`:893`): switching planets clears the reveal.
	_new_presets.clear()
	_clear_dimmed_footer()
	set_title_text("")
	_clear_root()
	_set_planet_view_active(true)
	_set_body_centered(false)
	refresh_campaign_views(planet_name)

	var view := _planet_view()
	if view != null:
		view.set_planet(planet_name)
		view.set_sectors(_planet_sectors(planet_name))

	# code-instantiated: the planet rail is the data-driven Planets list.
	var rail := MindTable.new()
	rail.custom_minimum_size.x = 208.0
	_root.add(rail).pad(0).set_align(0)
	_build_rail(rail, planet_name)
	_build_sector_list(planet_name)

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
	build_button_content(back, MindWidgets.glyph("left", 20), _t("@back"))
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
	build_button_content(tech_tree, MindWidgets.glyph("tree", 20), _t("@techtree"))
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


## `PlanetDialog.rebuildExpand`: when the planet has a based sector, a
## top-right `@sectorlist` toggle (with the `sectorlist.attacked` count) over a
## collapsible, searchable list of the based sectors (`rebuildList`: attacked
## first, then most played).
func _build_sector_list(planet_name: String) -> void:
	_remove_sector_list()
	var based: Array = []
	var attacked := 0
	for sector_variant in _planet_sectors(planet_name):
		var sector: Dictionary = sector_variant
		if bool(sector.get("attacked", false)):
			attacked += 1
		if bool(sector.get("has_base", false)):
			based.append(sector)
	if based.is_empty():
		return
	based.sort_custom(_sector_list_before)
	# code-instantiated: rebuilt with the planet view; content is the live
	# based-sector set. Java: `sectorList.background(Styles.black8)`.
	var backdrop := PanelContainer.new()
	backdrop.name = "SectorListBackdrop"
	var box := StyleBoxFlat.new()
	box.bg_color = MindStyles.BLACK_8
	backdrop.add_theme_stylebox_override("panel", box)
	backdrop.set_anchors_preset(Control.PRESET_TOP_RIGHT)
	backdrop.grow_horizontal = Control.GROW_DIRECTION_BEGIN
	backdrop.offset_left = -295.0
	backdrop.offset_right = -5.0
	backdrop.offset_top = 3.0
	backdrop.custom_minimum_size = Vector2(290.0, 0.0)
	add_child(backdrop)
	_sector_list_backdrop = backdrop
	_sector_list = VBoxContainer.new()
	_sector_list.name = "SectorList"
	_sector_list.add_theme_constant_override("separation", 0)
	backdrop.add_child(_sector_list)

	var toggle := Button.new()
	toggle.toggle_mode = true
	toggle.button_pressed = _sectors_shown
	toggle.custom_minimum_size = Vector2(290.0, 60.0)
	_apply_pane_button(toggle, OUTLINE_GRAY)
	var raw := _t("@sectorlist")
	if attacked > 0:
		var assets := MindWidgets.assets()
		var count := "[red]%d[]" % attacked
		var line := str(assets.call("bundle_format", "sectorlist.attacked", PackedStringArray([count]))) \
			if assets != null else count
		raw += "\n[red]\u26a0[lightgray] " + line
	var row := HBoxContainer.new()
	row.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	row.offset_left = 7.0
	row.offset_right = -7.0
	row.add_theme_constant_override("separation", 8)
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	toggle.add_child(row)
	row.add_child(MindWidgets.glyph("up-open" if _sectors_shown else "down-open", 22))
	var caption := MindWidgets.label(MindWidgets.markup(raw))
	caption.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	caption.fit_content = true
	caption.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	caption.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	caption.autowrap_mode = TextServer.AUTOWRAP_OFF
	caption.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(caption)
	toggle.pressed.connect(func() -> void:
		_sectors_shown = not _sectors_shown
		_build_sector_list(_selected_planet))
	_sector_list.add_child(toggle)
	if not _sectors_shown:
		return

	var panel := PanelContainer.new()
	panel.add_theme_stylebox_override(
		"panel", _pane_box(Color(0.0, 0.0, 0.0, 0.8), Color(0.0, 0.0, 0.0, 0.0), 0.0, 0.0)
	)
	_sector_list.add_child(panel)
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 3)
	panel.add_child(column)
	var search_row := HBoxContainer.new()
	search_row.add_child(MindWidgets.glyph("zoom", 22))
	var field := LineEdit.new()
	field.text = _sector_search
	field.custom_minimum_size = Vector2(0.0, 50.0)
	field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	search_row.add_child(field)
	column.add_child(search_row)
	var scroll := ScrollContainer.new()
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	scroll.custom_minimum_size = Vector2(0.0, 320.0)
	column.add_child(scroll)
	var entries := VBoxContainer.new()
	entries.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(entries)
	field.text_changed.connect(func(text: String) -> void:
		_sector_search = text
		_fill_sector_entries(entries, based))
	_fill_sector_entries(entries, based)


## `rebuildList` order: attacked sectors first, then by save playtime (desc).
func _sector_list_before(a: Dictionary, b: Dictionary) -> bool:
	var attacked_a := bool(a.get("attacked", false))
	var attacked_b := bool(b.get("attacked", false))
	if attacked_a != attacked_b:
		return attacked_a
	var stats_a: Dictionary = a.get("stats", {})
	var stats_b: Dictionary = b.get("stats", {})
	return int(stats_a.get("playtime_ms", 0)) > int(stats_b.get("playtime_ms", 0))


## `rebuildList.readd`: one `Styles.underlineb` row per based sector matching the
## search text (warning glyph when attacked, else the preset icon).
func _fill_sector_entries(entries: VBoxContainer, based: Array) -> void:
	for child in entries.get_children():
		child.queue_free()
	var needle := _sector_search.to_lower()
	for sector_variant in based:
		var sector: Dictionary = sector_variant
		var title := _localized_sector(sector)
		if not needle.is_empty() and not title.to_lower().contains(needle):
			continue
		var entry := Button.new()
		entry.toggle_mode = true
		entry.button_pressed = int(sector.get("id", -1)) == _selected_sector
		entry.custom_minimum_size = Vector2(0.0, 40.0)
		_apply_pane_button(entry, OUTLINE_GRAY, 3.0)
		var icon: Control = null
		var preset := str(sector.get("preset", ""))
		if bool(sector.get("attacked", false)):
			icon = MindWidgets.glyph("warn", 18, MindStyles.ACCENT)
		elif not preset.is_empty():
			var image := MindWidgets.image("sector-%s" % preset)
			image.custom_minimum_size = Vector2(20.0, 20.0)
			image.mouse_filter = Control.MOUSE_FILTER_IGNORE
			icon = image
		build_button_content(entry, icon, title)
		var sector_id := int(sector.get("id", -1))
		entry.pressed.connect(func() -> void:
			_selected_sector = sector_id
			_show_planet(_selected_planet))
		entries.add_child(entry)
	if entries.get_child_count() == 0:
		entries.add_child(MindWidgets.label(_tm("@none.found")))


func _remove_sector_list() -> void:
	if _sector_list_backdrop != null:
		_sector_list_backdrop.queue_free()
		_sector_list_backdrop = null
	if _sector_list != null:
		_sector_list.queue_free()
		_sector_list = null


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
	build_button_content(difficulty, MindWidgets.glyph("book", 20), _t("@campaign.difficulty"))
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
	build_button_content(button, icon, _localized_planet(planet))
	button.pressed.connect(func() -> void: _switch_planet(str(planet.get("name", ""))))
	return button


## `PlanetDialog.viewPlanet`: switching planets clears the selection (no sector
## panel/band until one is picked).
func _switch_planet(planet_name: String) -> void:
	if planet_name == _selected_planet:
		return
	_selected_sector = -1
	_show_planet(planet_name, false)


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
	title_row.add_theme_constant_override("separation", 4)
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
	var sector_id := int(sector.get("id", -1))
	# `PlanetDialog.java:1195-1210`: the rename pencil exists for numbered
	# sectors and presets without `requireUnlock`.
	if preset.is_empty() or not bool(sector.get("require_unlock", true)):
		var spacer := Control.new()
		spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
		title_row.add_child(spacer)
		# code-instantiated: data-driven per-sector rename pencil (`pencilSmall`).
		var pencil := MindWidgets.glyph_button("pencil", "", "clearNonei")
		pencil.custom_minimum_size = Vector2(40.0, 40.0)
		pencil.tooltip_text = _t("@sectors.rename")
		pencil.pressed.connect(func() -> void: _rename_sector(sector_id))
		title_row.add_child(pencil)
	# `PlanetDialog.java:1211-1226`: the marker icon button, always present.
	# code-instantiated: data-driven per-sector icon button.
	var icon_button := Button.new()
	icon_button.theme_type_variation = "clearNonei"
	icon_button.custom_minimum_size = Vector2(40.0, 40.0)
	icon_button.tooltip_text = _t("@sector.changeicon")
	icon_button.add_child(_sector_icon_content(sector))
	icon_button.pressed.connect(func() -> void: _open_sector_icon_dialog(sector_id))
	title_row.add_child(icon_button)

	var divider := ColorRect.new()
	divider.color = MindStyles.ACCENT
	divider.custom_minimum_size = Vector2(0.0, 3.0)
	divider.mouse_filter = Control.MOUSE_FILTER_IGNORE
	column.add_child(divider)

	# `updateSelected` order: locked objectives / threat (base-less only), the
	# state line, the `@sectors.resources` icon row, then the `@stats` button.
	var has_base := bool(sector.get("has_base", false))
	if bool(sector.get("objectives_locked", false)):
		# `isLocked` presets list the incomplete tech-node objectives instead of
		# the threat line (`updateSelected`, PlanetDialog.java:1286-1299).
		var complete_label := MindWidgets.label(_tm("@complete"))
		complete_label.custom_minimum_size = Vector2(0.0, 26.0)
		complete_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
		column.add_child(complete_label)
		for objective_variant in (sector.get("objectives", []) as Array):
			var objective: Array = objective_variant
			var kind := str(objective[0]) if objective.size() > 0 else ""
			var argument := str(objective[1]) if objective.size() > 1 else ""
			# code-instantiated: one row per incomplete objective.
			var objective_row := HBoxContainer.new()
			objective_row.add_theme_constant_override("separation", 3)
			var objective_label := MindWidgets.label(
				"> %s" % MindWidgets.markup_format(
					"@requirement.%s" % kind, [_localized_objective_arg(kind, argument)]
				)
			)
			objective_label.add_theme_color_override("default_color", Color(0.75, 0.75, 0.75))
			objective_row.add_child(objective_label)
			objective_row.add_child(
				MindWidgets.glyph("cancel", 18, Color(1.0, 0.204, 0.11))
			)
			column.add_child(objective_row)
	elif not has_base:
		var threat := str(sector.get("threat_band", "low"))
		# `displayThreat`: the hex colour replaces the accent prefix.
		var color := str(sector.get("threat_color", "ffffffff")).left(6)
		var threat_label := MindWidgets.label(
			"%s [color=#%s]%s[/color]" % [_t("@sectors.threat"), color, _t("@threat.%s" % threat)]
		)
		threat_label.custom_minimum_size = Vector2(0.0, 26.0)
		threat_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
		column.add_child(threat_label)

	for state_key in _sector_state_labels(sector):
		# Markup-bearing bundle strings (`[scarlet]Under attack!`) must go
		# through the BBCode translation, not the raw bundle (PAR-0016).
		var label := MindWidgets.label(_tm(state_key))
		label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		# Java's sector panel centres these status lines
		# (`diff/03-sector-side.png`: "Under attack!" is centred).
		label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		column.add_child(label)

	var stats: Dictionary = sector.get("stats", {})
	var resources: Array = stats.get("resources", [])
	if bool(sector.get("has_save", false)) and not resources.is_empty():
		var resource_row := HBoxContainer.new()
		resource_row.add_theme_constant_override("separation", 3)
		var spacer := Control.new()
		spacer.custom_minimum_size = Vector2(7.0, 0.0)
		resource_row.add_child(spacer)
		resource_row.add_child(MindWidgets.label(_t("@sectors.resources")))
		for resource in resources:
			var texture := _content_icon(str(resource))
			if texture == null:
				continue
			var image := TextureRect.new()
			image.texture = texture
			image.custom_minimum_size = Vector2(20.0, 20.0)
			image.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
			image.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
			image.mouse_filter = Control.MOUSE_FILTER_IGNORE
			resource_row.add_child(image)
		column.add_child(resource_row)

	if has_base:
		var stats_button := MindWidgets.button("")
		stats_button.theme_type_variation = "cleart"
		stats_button.custom_minimum_size = Vector2(0.0, 40.0)
		build_button_content(stats_button, MindWidgets.glyph("info", 20), _t("@stats"))
		stats_button.pressed.connect(func() -> void: _open_sector_stats(int(sector.get("id", -1))))
		column.add_child(stats_button)

	var action := _sector_action(sector)
	# `updateSelected`: the action button exists only when `(hasBase && look) ||
	# canSelect || preset.alwaysUnlocked` (`show_action`).
	if bool(sector.get("show_action", true)):
		var launch := MindWidgets.button("")
		launch.custom_minimum_size = Vector2(230.0, 53.0)
		launch.add_theme_stylebox_override("normal", _pane_box(Color(0.0, 0.0, 0.0, 1.0), MindStyles.ACCENT))
		launch.add_theme_stylebox_override("hover", _pane_box(Color(0.0, 0.0, 0.0, 1.0), MindStyles.ACCENT))
		launch.add_theme_stylebox_override("pressed", _pane_box(Color(0.0, 0.0, 0.0, 1.0), MindStyles.ACCENT))
		launch.add_theme_stylebox_override("disabled", _pane_box(Color(0.0, 0.0, 0.0, 1.0), MindStyles.ACCENT))
		build_button_content(
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


## `PlanetDialog.java:1211-1226`: the marker icon control inside the sector
## panel's icon button — `contentIcon.uiIcon` else the chosen accessible glyph
## else `Icon.noneSmall`.
func _sector_icon_content(sector: Dictionary) -> Control:
	var content_icon := _opt(sector, "content_icon")
	if not content_icon.is_empty():
		var texture := _content_icon(content_icon)
		if texture != null:
			# code-instantiated: per-sector marker image inside the icon button.
			var image := TextureRect.new()
			image.texture = texture
			image.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
			image.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
			image.mouse_filter = Control.MOUSE_FILTER_IGNORE
			image.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
			return image
	var icon_name := _opt(sector, "icon")
	if icon_name.is_empty():
		icon_name = "none"
	var glyph := MindWidgets.glyph(icon_name, 26)
	glyph.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	glyph.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return glyph


## `PlanetDialog.java:1200-1206`: `ui.showTextInput("@sectors.rename", "@name",
## 32, sector.name(), v -> sector.setName(v))`.
func _rename_sector(sector_id: int) -> void:
	var sector := _find_sector(_selected_planet, sector_id)
	if sector.is_empty():
		return
	_rename_target = sector_id
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_text_input"):
		ui.call(
			"show_text_input", _t("@sectors.rename"), _t("@name"), 32,
			_localized_sector(sector), false, false
		)


## `Sector.setName` result: `updateSelected` + `rebuildList` after the write.
func _on_text_input_result(text: String) -> void:
	if _rename_target < 0:
		return
	var sector_id := _rename_target
	_rename_target = -1
	if text.strip_edges().is_empty():
		return
	var campaign := campaign_node()
	if campaign == null or not campaign.has_method("set_sector_name"):
		return
	if bool(campaign.call("set_sector_name", _selected_planet, sector_id, text)):
		_show_planet(_selected_planet)


## `PlanetDialog.java:1223-1283`: the fill-parent marker picker (`Icon.none`,
## `accessibleIcons`, five `defaultContentIcons` sections) in `icon_select`'s
## sector mode.
func _open_sector_icon_dialog(sector_id: int) -> void:
	_icon_target = sector_id
	var ui := get_node_or_null("/root/MindUi")
	var root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui == null:
		return
	if root != null and root.has_method("dialog"):
		var dialog: Node = root.call("dialog", "icon_select")
		if dialog != null and dialog.has_signal("sector_icon_selected") \
				and not dialog.sector_icon_selected.is_connected(_on_sector_icon_selected):
			dialog.sector_icon_selected.connect(_on_sector_icon_selected)
	var sector := _find_sector(_selected_planet, sector_id)
	ui.call(
		"open_dialog", "icon_select",
		JSON.stringify({
			"mode": "sector",
			"icon": _opt(sector, "icon"),
			"content_icon": _opt(sector, "content_icon"),
		})
	)


## The picker's choice: Java writes one half and nulls the other; the write path
## refreshes the panel and list (`updateSelected` + `rebuildList`).
func _on_sector_icon_selected(icon_name: String, content_icon: String) -> void:
	if _icon_target < 0:
		return
	var sector_id := _icon_target
	_icon_target = -1
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("close_dialog", "icon_select")
	var campaign := campaign_node()
	if campaign == null or not campaign.has_method("set_sector_icon"):
		return
	if bool(campaign.call("set_sector_icon", _selected_planet, sector_id, icon_name, content_icon)):
		_show_planet(_selected_planet)


## Optional string field of a read-model row (`null` -> "").
func _opt(row: Dictionary, key: String) -> String:
	var value: Variant = row.get(key, null)
	return "" if value == null else str(value)


## `requirement.<kind>` argument localization: `sector.<name>.name` for
## capture/onsector, `planet.<name>.name` for onplanet, content name lookup for
## research/produce (`updateSelected` prints the name; the locked-parent `???`
## case needs the tech parent, not available here — accepted).
func _localized_objective_arg(kind: String, argument: String) -> String:
	if argument.is_empty():
		return "???"
	match kind:
		"capture", "onsector":
			return _bundle_or_name("sector.%s.name" % argument, argument)
		"onplanet":
			return _bundle_or_name("planet.%s.name" % argument, argument)
		_:
			for type_name in ["block", "item", "unit", "liquid", "status"]:
				var value := _bundle_value("%s.%s.name" % [type_name, argument])
				if not value.is_empty():
					return value
			return argument


## Bundle value or "" when the key is missing.
func _bundle_value(key: String) -> String:
	var assets := MindWidgets.assets()
	if assets == null:
		return ""
	var value := str(assets.call("bundle_get", key))
	return "" if value == key else value


## Bundle value with the raw name as fallback.
func _bundle_or_name(key: String, fallback: String) -> String:
	var value := _bundle_value(key)
	return value if not value.is_empty() else fallback


## Visible state lines in `PlanetDialog.updateSelected` order; only states with
## bundle keys get a label (`shielded` stays a map/action-tooltip marker).
func _sector_state_labels(sector: Dictionary) -> PackedStringArray:
	var out := PackedStringArray()
	if bool(sector.get("attacked", false)):
		out.append("@sectors.underattack")
	elif bool(sector.get("vulnerable", false)):
		out.append("@sectors.vulnerable")
	elif not bool(sector.get("has_base", false)) and bool(sector.get("has_enemy_base", false)):
		out.append("@sectors.enemybase")
	return out


## `UnlockableContent.uiIcon` for a resource name (`item-<name>-ui`, then the
## other `defaultContentIcons` types).
func _content_icon(content_name: String) -> Texture2D:
	for prefix in ["item", "liquid", "block", "unit", "status"]:
		var texture := MindWidgets.icon_texture("%s-%s-ui" % [prefix, content_name])
		if texture != null:
			return texture
	return null


## `PlanetDialog` `@stats`: the sector stats window (round-I I04 T10,
## `PlanetDialog.java:1026-1171`).
func _open_sector_stats(sector_id: int) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	ui.call(
		"open_dialog", "sector_stats",
		JSON.stringify({"planet": _selected_planet, "sector": sector_id})
	)


## `PlanetDialog` action label/icon/behavior for the selected sector
## (`updateSelected`): `@sectors.resume` (being played), `@sectors.go` (has a
## base), `@locked` (isLocked preset), `@sectors.nolaunchcandidate`, else
## `@sectors.launch`; disabled when locked or without a candidate.
func _sector_action(sector: Dictionary) -> Dictionary:
	var locked := bool(sector.get("objectives_locked", false))
	var no_candidate := bool(sector.get("no_candidate", false))
	var disabled := locked or no_candidate
	if _is_being_played(sector):
		return {"label": "@sectors.resume", "icon": "play", "mode": "resume", "disabled": disabled}
	if bool(sector.get("has_base", false)) or bool(sector.get("captured", false)):
		return {"label": "@sectors.go", "icon": "play", "mode": "go", "disabled": disabled}
	if locked:
		return {"label": "@locked", "icon": "lock", "mode": "launch", "disabled": true}
	if no_candidate:
		return {"label": "@sectors.nolaunchcandidate", "icon": "play", "mode": "launch", "disabled": true}
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
	_launch_via_launcher(planet_name, sector_id)


## `PlanetDialog.playSelected` look mode: the Java `findLauncher` sector (from
## the view's `launch_from`) opens the Configure Loadout dialog; a destination
## without one is a free launch - `universe.clearLoadoutInfo()` +
## `control.playSector(sector)`, no dialog (PlanetDialog.java:1394-1403).
func _launch_via_launcher(planet_name: String, sector_id: int) -> void:
	var launcher := _sector_launch_from(planet_name, sector_id)
	if launcher < 0:
		_launch(planet_name, sector_id)
		return
	_open_launch_loadout(planet_name, sector_id, launcher)


## The view's `launch_from` for a destination (`-1` = free launch).
func _sector_launch_from(planet_name: String, sector_id: int) -> int:
	for sector_variant in _planet_sectors(planet_name):
		var sector: Dictionary = sector_variant
		if int(sector.get("id", -1)) != sector_id:
			continue
		var value: Variant = sector.get("launch_from")
		return -1 if value == null else int(value)
	return -1


# --- Planet view glue ---------------------------------------------------------

func _planet_view() -> PlanetView:
	return get_node_or_null("/root/Spine/Planet/PlanetView") as PlanetView


## Toggles the globe layer and the dialog's own backdrop/dim. The globe page
## covers the whole stage (`PlanetDialog` draws its space scene over
## everything), so the menu, HUD and world are hidden while it is up and
## restored on close (`_cover_stage`/`_restore_stage`). The chooser over the
## standalone menu keeps the menu visible behind its dim.
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
	if not visible:
		# Boot `_ready` and `hidden()`: the stage is left alone (`hidden()`
		# restores it explicitly).
		return
	if active or not _saved_layers.is_empty() or _hud_visible():
		# Globe page, or anything shown over the live game: cover the stage.
		_cover_stage()
	else:
		# Chooser over the standalone menu (nothing captured): the menu stays up.
		_set_visible_at(MENU_GROUP_PATH, true)
		_set_visible_at(MENU_BACKGROUND_PATH, true)


func _hud_visible() -> bool:
	var hud := get_node_or_null(HUD_GROUP_PATH) as CanvasItem
	return hud != null and hud.visible


## Captures (once) and hides every stage layer under the dialog: the menu and
## its background, the HUD group (hints/objective text live there), the world
## and the world's own `CanvasLayer`s (the renderer's `ObjectiveMarkers` layer
## draws the objective marker/label and does not inherit `World` visibility).
func _cover_stage() -> void:
	if _saved_layers.is_empty():
		var paths: Array[String] = [MENU_GROUP_PATH, MENU_BACKGROUND_PATH, HUD_GROUP_PATH, WORLD_PATH]
		var world := get_node_or_null(WORLD_PATH)
		if world != null:
			for layer in world.find_children("*", "CanvasLayer", true, false):
				paths.append(str(layer.get_path()))
		for path in paths:
			var node := get_node_or_null(path)
			if node is CanvasItem or node is CanvasLayer:
				_saved_layers[path] = bool(node.get("visible"))
	for path: String in _saved_layers:
		_set_visible_at(path, false)


## Restores exactly the captured visibility (no-op when nothing was covered).
## `_launch` closes first and then `_enter_game` switches to the HUD, so the
## launch path still ends with the menu hidden and the HUD shown.
func _restore_stage() -> void:
	for path: String in _saved_layers:
		_set_visible_at(path, bool(_saved_layers[path]))
	_saved_layers.clear()


func _set_visible_at(path: String, value: bool) -> void:
	var node := get_node_or_null(path)
	if node is CanvasItem or node is CanvasLayer:
		node.set("visible", value)


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
		# `PlanetDialog.tap`: only `canSelect` sectors become the selection.
		if not bool(sector.get("selectable", false)):
			return
		_selected_sector = int(sector.get("id", -1))
		_show_planet(_selected_planet)
		return


## Empty-screen left clicks pick the sector under the cursor while the globe is
## up; UI regions (rail, bottom bar) are excluded. Left-drag orbits the globe
## (`PlanetDialog.java:114-138`) and the wheel zooms (`:140-147`); both are
## ignored while the new-preset reveal runs, and a release after a drag does
## not select (Java's Arc `clicked` only fires on a tap). Covered by another
## dialog (not the stack top), events are ignored like an Arc modal
## (`Dialog.hit`).
func _input(event: InputEvent) -> void:
	if not visible:
		return
	if not is_top():
		return
	if event is InputEventKey:
		_handle_reveal_escape(event as InputEventKey)
		return
	if event is InputEventMouseMotion:
		var view := _planet_view()
		if view == null or not view.visible:
			return
		var motion := event as InputEventMouseMotion
		if motion.button_mask & MOUSE_BUTTON_MASK_LEFT:
			if not _press_on_globe:
				return
			# `dragged()`: ignored while the reveal shows a preset other than
			# the planet's last sector; otherwise the drag clears the reveal
			# (`newPresets.clear()`).
			if not _new_presets.is_empty() and int(_new_presets[_new_presets.size() - 1]) != _last_sector_id:
				return
			_new_presets.clear()
			_dragged = true
			view.drag(motion.relative)
			get_viewport().set_input_as_handled()
			return
		_update_hover(view, motion.position)
		return
	if not (event is InputEventMouseButton):
		return
	var click := event as InputEventMouseButton
	if click.button_index in [MOUSE_BUTTON_WHEEL_UP, MOUSE_BUTTON_WHEEL_DOWN]:
		var wheel_view := _planet_view()
		if wheel_view == null or not wheel_view.visible:
			return
		if _over_ui(click.position):
			return
		# Arc scroll `amountY`: negative scrolling up (SDL `-wheel.y`), and
		# `zoom + amountY / 10` means up zooms in (`PlanetDialog.java:144`).
		wheel_view.scroll(-1.0 if click.button_index == MOUSE_BUTTON_WHEEL_UP else 1.0)
		get_viewport().set_input_as_handled()
		return
	if click.button_index != MOUSE_BUTTON_LEFT:
		return
	var mouse_view := _planet_view()
	if mouse_view == null or not mouse_view.visible:
		return
	if click.pressed:
		_pressed_position = click.position
		_dragged = false
		_press_on_globe = not _over_ui(click.position)
		return
	# Release: a drag must not select; a tap selects like `PlanetDialog.tap`.
	if _dragged:
		_dragged = false
		_press_on_globe = false
		return
	if not _press_on_globe:
		return
	_press_on_globe = false
	if click.position.distance_to(_pressed_position) > 4.0:
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


## `PlanetDialog.java:92-96`: Escape during the reveal clears every entry but
## the first (the planet's last sector); the dialog itself stays open.
func _handle_reveal_escape(key: InputEventKey) -> void:
	if not key.pressed or key.echo or key.keycode != KEY_ESCAPE:
		return
	if _new_presets.size() <= 1:
		return
	_new_presets.resize(1)
	_preset_show = 0.0
	_reveal_showed = false
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
	if _sector_list != null and _sector_list.get_global_rect().has_point(point):
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
	if bool(sector.get("named", false)):
		return str(sector.get("name", ""))
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
## (`PlanetDialog.playSelected` -> `LaunchLoadoutDialog`), sourced from the
## Java `findLauncher` sector. Falls through to a direct launch when the dialog
## (or its signal) is unavailable.
func _open_launch_loadout(planet_name: String, sector_id: int, launcher_id: int) -> void:
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
		"launcher": launcher_id,
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
	_launch_via_launcher(_selected_planet, sector_id)


## The loadout row for a `Schematics.all` index, or `{}`.
func _find_loadout_row(index: int) -> Dictionary:
	for loadout_variant in campaign_section("loadouts"):
		var loadout: Dictionary = loadout_variant
		if int(loadout.get("index", -1)) == index:
			return loadout
	return {}


## `PlanetDialog.playSelected` launch cost: the chosen schematic's
## requirements (`from.removeItems(loadout.requirements())`) plus the staged
## launch resources (sum per item) as `[{item, amount}]` stacks, removed from
## the launcher sector.
func _loadout_launch_stacks(row: Dictionary, resources: Dictionary) -> Array:
	var totals: Dictionary = {}
	for requirement in row.get("requirements", []):
		var item := str(requirement[0])
		totals[item] = int(totals.get(item, 0)) + int(requirement[1])
	for item in resources:
		totals[item] = int(totals.get(item, 0)) + int(resources[item])
	return _stacks_of(totals)


## `MindCampaign.launch_dialog_json` (`LaunchLoadoutDialog.update`) for the
## launcher launch; `{}` without the facade.
func _launch_plan(planet_name: String, launcher: int, sector_id: int, loadout: int) -> Dictionary:
	var campaign := campaign_node()
	if campaign == null or not campaign.has_method("launch_dialog_json"):
		return {}
	var parsed: Variant = JSON.parse_string(str(campaign.call(
		"launch_dialog_json", planet_name, launcher, sector_id, loadout
	)))
	return parsed if parsed is Dictionary else {}


## `universe.getLaunchResources()` as `{item: amount}`.
func _launch_resource_totals() -> Dictionary:
	var campaign := campaign_node()
	if campaign != null and campaign.has_method("get_launch_resource_stacks"):
		var parsed: Variant = JSON.parse_string(str(campaign.call("get_launch_resource_stacks")))
		if parsed is Dictionary:
			return parsed
	return {}


func _stacks_of(totals: Dictionary) -> Array:
	var stacks: Array = []
	for item in totals:
		stacks.append({"item": str(item), "amount": int(totals[item])})
	return stacks


## Enters the sector. `resume` uses the existing-base path
## (`Control.playSector`); `loadout >= 0` is a launcher launch with the chosen
## schematic's requirements + launch resources (`PlanetDialog.playSelected`);
## `loadout < 0` is the free launch (`universe.clearLoadoutInfo`).
func _launch(planet_name: String, sector_id: int, loadout: int = -1, resume: bool = false) -> void:
	if sector_id < 0:
		return
	sector_activated.emit(planet_name, sector_id)
	# Generate and install the sector's world into the sim host first so the
	# renderer draws terrain (`Control.playNewSector` world half).
	var sim_host := get_node_or_null("/root/Spine/SimHost")
	var campaign := campaign_node()
	# H05 T2: prime the generator with the live campaign threat/enemy-base
	# flags (`state.rules.sector` is set before `World.loadSector`).
	if campaign != null and campaign.has_method("prime_sector_load"):
		campaign.call("prime_sector_load", planet_name, sector_id)
	# H05 T4: the launcher branch computes its loadout before the world load
	# (`Schematics.placeLaunchLoadout` runs inside generation for numbered
	# sectors); the free launch primes the planet default with no resources.
	var launcher_id := -1
	var launch_resources: Dictionary = {}
	var launch_row: Dictionary = {}
	var launch_cost: Array = []
	if loadout >= 0 and not resume:
		launcher_id = _sector_launch_from(planet_name, sector_id)
		var plan := _launch_plan(planet_name, launcher_id, sector_id, loadout)
		launch_resources = plan.get("resources", {})
		if launch_resources.is_empty():
			launch_resources = _launch_resource_totals()
		launch_row = _find_loadout_row(loadout)
		launch_cost = _loadout_launch_stacks(launch_row, launch_resources)
	# `PlanetDialog.playSelected`: the chosen schematic's build cost plus the
	# staged resources is removed from the launcher sector and stocked into
	# the new core. The staged resources come from `LaunchLoadoutDialog.update`
	# (`launch_plan`) so the Max computation and capacity clamp apply.
	if sim_host != null and sim_host.has_method("set_pending_launch"):
		if loadout >= 0 and not resume:
			sim_host.call("set_pending_launch", loadout, JSON.stringify(_stacks_of(launch_resources)))
		else:
			sim_host.call("set_pending_launch", -1, "[]")
	if sim_host != null and sim_host.has_method("load_sector"):
		sim_host.call("load_sector", planet_name, sector_id)
	var started := false
	if campaign != null:
		if resume and campaign.has_method("play_sector"):
			started = bool(campaign.call("play_sector", planet_name, sector_id))
		elif loadout >= 0 and campaign.has_method("start_sector_with_loadout"):
			if launcher_id >= 0:
				campaign.call(
					"remove_sector_items",
					planet_name,
					launcher_id,
					JSON.stringify(launch_cost)
				)
			# `Schematics.placeLaunchLoadout`: the placed core holds only the
			# launch resources; the schematic requirements are its build cost
			# (paid by the launcher above), not core contents.
			var core_stacks := _stacks_of(launch_resources)
			started = bool(campaign.call(
				"start_sector_with_loadout",
				planet_name,
				sector_id,
				JSON.stringify({
					"core": str(launch_row.get("core", "")),
					"schematic": loadout,
					"stacks": core_stacks,
					"origin": launcher_id,
				})
			))
		elif campaign.has_method("start_sector"):
			# Free launch with no launcher: `universe.clearLoadoutInfo()` then
			# `control.playSector(sector)` (PAR-0085/PAR-0054).
			if campaign.has_method("clear_loadout_info"):
				campaign.call("clear_loadout_info")
			started = bool(campaign.call("start_sector", planet_name, sector_id))
	_close()
	if started:
		_enter_game()


## Hides the standalone menu and shows the in-game HUD group (`MindUiRoot`).
func _enter_game() -> void:
	# `MindUi.close_dialog` hides deferred, so `hidden()` -> `_restore_stage()`
	# runs after this: retarget the captured stage to the in-game layout.
	for path: String in _saved_layers:
		if path == MENU_GROUP_PATH or path == MENU_BACKGROUND_PATH:
			_saved_layers[path] = false
		elif path == HUD_GROUP_PATH:
			_saved_layers[path] = true
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
