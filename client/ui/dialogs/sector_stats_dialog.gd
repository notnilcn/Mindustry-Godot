## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/PlanetDialog.java:1026-1171
## (`showStats`, `displayItems`, `abandonSectorConfirm`).
##
## The planet page's Stats window: the preset description, playtime/attempts/
## wave/threat, resource icons, the lockdown notice, production/export/import
## rate rows (3 cells per row, `@unit.perminute`) and the stored item pool
## (4 cells per row), plus the Abandon action when the shown sector has a base.
## Data comes from the sector row of the live `campaign_views` model.

extends MindDialog

const ACCENT := "#ffd37f"
## `Pal.lightGray` (`Color.lightGray` in Arc: 0.5-ish; sampled 0.62).
const LIGHT_GRAY := Color(0.62, 0.62, 0.62)

var _planet := ""
var _sector_id := -1
var _row: Dictionary = {}
var _root: MindTable = null
var _pending_abandon := false


func _ready() -> void:
	should_pause = false
	super._ready()
	# `new BaseDialog(sector.name())` fills the viewport (`setFillParent(true)`)
	# instead of floating as a boxed panel.
	fill_panel()
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_signal("confirm_result") \
			and not ui.is_connected("confirm_result", _on_confirm):
		ui.connect("confirm_result", _on_confirm)


func shown() -> void:
	var ctx := context()
	_planet = str(ctx.get("planet", ""))
	_sector_id = int(ctx.get("sector", -1))
	refresh_campaign_views(_planet)
	_row = _find_row()
	_pending_abandon = false
	_build()


## `PlanetDialog.showStats`: description, time/attempts/wave/threat, resources,
## lockdown, rate sections and the stored pool, in this order.
func _build() -> void:
	if _root == null:
		# `dialog.cont.pane(c -> ...)`: the dialog body already scrolls, so the
		# stats table sits directly in it, centred at its natural width (no
		# boxed inner pane).
		_root = content_table()
		_root.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	_root.clear_children()
	clear_buttons()
	set_title_text(_localized_name())

	var stats: Dictionary = _row.get("stats", {})
	var description_key := _opt(stats, "preset_description_key")
	if not description_key.is_empty():
		var description := MindWidgets.label(_tm("@%s" % description_key))
		description.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		description.custom_minimum_size.x = 420.0
		_root.add(description).grow_x_axis().set_pad_bottom(5)
		_root.row()

	_value_line("@sectors.time", _format_millis(int(stats.get("playtime_ms", 0))))
	var attempts := int(stats.get("attempts", 0))
	if attempts > 0:
		_value_line("@sectors.attempts", str(attempts))
	var wave := int(stats.get("wave", 0))
	if wave > 0:
		_value_line("@sectors.wave", str(wave))
	if bool(_row.get("attacked", false)) or not bool(_row.get("has_base", false)):
		_value_line("@sectors.threat", _t("@threat.%s" % str(_row.get("threat_band", "low"))))

	var resources: Array = stats.get("resources", [])
	if bool(_row.get("has_save", false)) and not resources.is_empty():
		_root.add(MindWidgets.label(_t("@sectors.resources"))).left()
		_root.row()
		var resource_row := HBoxContainer.new()
		resource_row.add_theme_constant_override("separation", 3)
		for resource in resources:
			var texture := _content_icon(str(resource))
			if texture == null:
				continue
			# code-instantiated: one 24 px icon per known resource.
			var image := TextureRect.new()
			image.texture = texture
			image.custom_minimum_size = Vector2(24.0, 24.0)
			image.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
			image.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
			image.mouse_filter = Control.MOUSE_FILTER_IGNORE
			resource_row.add_child(image)
		_root.add(resource_row).set_pad_left(10).left()
		_root.row()

	if bool(stats.get("lockdown", false)):
		# `c.add(UI.formatIcons(bundle.get("sector.lockdown"))).wrap().fillX()`:
		# `[red]:warning:[accent] Sector currently under attack\n...`.
		var lockdown := MindWidgets.label(_icon_glyph_markup(_tm("@sector.lockdown")))
		lockdown.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		lockdown.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		_root.add(lockdown).grow_x_axis().set_pad_bottom(10)
		_root.row()
		_apply_icon_fallback(lockdown)

	_stat_section("@sectors.production", stats.get("production", []), [])
	var destination: Variant = stats.get("destination")
	var export_trailing: Array = []
	if destination is Dictionary:
		export_trailing.append(destination)
	_stat_section("@sectors.export", stats.get("export", []), export_trailing)
	if bool(_row.get("has_base", false)):
		_stat_section("@sectors.import", stats.get("import", []), stats.get("import_sources", []))

	var stored: Array = stats.get("stored", [])
	if bool(_row.get("has_base", false)) and not stored.is_empty():
		_root.add(MindWidgets.label(_t("@sectors.stored"))).left()
		_root.row()
		# code-instantiated: the stored pool is a runtime-sized 4-column grid.
		var stored_grid := MindTable.new()
		var index := 0
		for entry in stored:
			if not (entry is Array) or (entry as Array).size() < 2:
				continue
			stored_grid.add(_stored_cell(str(entry[0]), int(entry[1])))
			index += 1
			if index % 4 == 0:
				stored_grid.row()
		_root.add(stored_grid).set_pad_left(10).left()
		_root.row()

	add_close_button()
	# `abandonSectorConfirm` button, shown for sectors with a base.
	if bool(_row.get("has_base", false)):
		var abandon := MindWidgets.icon_button("cancel", _t("@sector.abandon"), "cleart")
		abandon.pressed.connect(_confirm_abandon)
		buttons.add_child(abandon)


## `UI.formatIcons` for icon-font tokens (`:warning:`): `render_markup` emits
## empty `[icon name="…"][/icon]` spans (OD-UI3) that draw nothing, so the
## lockdown line lost its warning sign. Icon-font names become their code point
## (drawn through the icon-font fallback, [method _apply_icon_fallback]);
## other spans are left as they are.
func _icon_glyph_markup(markup: String) -> String:
	var regex := RegEx.create_from_string("\\[icon name=\"([^\"]+)\"\\]\\[/icon\\]")
	var out := ""
	var cursor := 0
	for found in regex.search_all(markup):
		out += markup.substr(cursor, found.get_start() - cursor)
		var glyph := MindIcons.glyph(found.get_string(1))
		out += glyph if not glyph.is_empty() else found.get_string()
		cursor = found.get_end()
	return out + markup.substr(cursor)


## Gives a label the `MindAssets.icon_font()` as a fallback of its current
## text font so icon code points render inline (Arc injects the icon glyphs
## into every UI font, `Fonts.java`). Runs once the label is in the tree so the
## themed font resolves.
func _apply_icon_fallback(label: RichTextLabel) -> void:
	var assets := MindWidgets.assets()
	if assets == null or not label.is_inside_tree():
		return
	var icon_font: Variant = assets.call("icon_font")
	if not (icon_font is Font):
		return
	var base := label.get_theme_font("normal_font")
	if base == null:
		return
	# code-instantiated: per-label font wrapper adding the icon-font fallback.
	var variation := FontVariation.new()
	variation.base_font = base
	var fallbacks: Array[Font] = [icon_font as Font]
	variation.fallbacks = fallbacks
	label.add_theme_font_override("normal_font", variation)


## `name [accent]value` line (`c.add(Core.bundle.get(...) + " [accent]" + ...)`).
func _value_line(label_key: String, value: String) -> void:
	var label := MindWidgets.label("%s [color=%s]%s[/color]" % [_t(label_key), ACCENT, value])
	_root.add(label).left().set_pad_bottom(5)
	_root.row()


## `displayItems(c, stats, name, builder)`: 3 `icon total unit.perminute` cells
## per row behind a `lightGray` label, plus the trailing destination/source
## lines (`Iconc.rightOpen` + sector name).
func _stat_section(label_key: String, rows: Array, trailing: Array) -> void:
	var cells: Array = []
	for entry in rows:
		if entry is Array and (entry as Array).size() >= 2:
			cells.append(entry)
	if cells.is_empty() and trailing.is_empty():
		return
	_root.add(MindWidgets.label(_t(label_key))).left()
	_root.row()
	if not cells.is_empty():
		# code-instantiated: runtime-sized 3-column rate grid.
		var grid := MindTable.new()
		var index := 0
		for cell in cells:
			var cell_row := HBoxContainer.new()
			cell_row.add_theme_constant_override("separation", 3)
			var texture := _content_icon(str(cell[0]))
			if texture != null:
				# code-instantiated: one icon per rate row.
				var image := TextureRect.new()
				image.texture = texture
				image.custom_minimum_size = Vector2(24.0, 24.0)
				image.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
				image.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
				image.mouse_filter = Control.MOUSE_FILTER_IGNORE
				cell_row.add_child(image)
			var label := MindWidgets.label("%d %s" % [int(cell[1]), _t("@unit.perminute")])
			label.add_theme_color_override("default_color", LIGHT_GRAY)
			cell_row.add_child(label)
			grid.add(cell_row)
			index += 1
			if index % 3 == 0:
				grid.row()
		_root.add(grid).set_pad_left(10).left()
		_root.row()
	for source_variant in trailing:
		if not (source_variant is Dictionary):
			continue
		var source: Dictionary = source_variant
		var prefix := MindIcons.glyph("right-open")
		var icon_text := ""
		var content_icon := _opt(source, "content_icon")
		var icon_name := _opt(source, "icon")
		if not content_icon.is_empty():
			icon_text = ""  # content rows keep the text prefix only
		elif not icon_name.is_empty():
			icon_text = MindIcons.glyph(icon_name)
		var line := "%s %s%s%s" % [prefix, icon_text, "" if icon_text.is_empty() else " ", str(source.get("name", ""))]
		_root.add(MindWidgets.label(line)).set_pad_left(10).left()
		_root.row()


## One stored cell (`res.image(uiIcon).padRight(3); res.add(formatAmount)`).
func _stored_cell(content_name: String, amount: int) -> Control:
	# code-instantiated: one per stored item row.
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 3)
	var texture := _content_icon(content_name)
	if texture != null:
		var image := TextureRect.new()
		image.texture = texture
		image.custom_minimum_size = Vector2(24.0, 24.0)
		image.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		image.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		image.mouse_filter = Control.MOUSE_FILTER_IGNORE
		row.add_child(image)
	var label := MindWidgets.label(str(maxi(amount, 0)))
	label.add_theme_color_override("default_color", LIGHT_GRAY)
	row.add_child(label)
	return row


## `abandonSectorConfirm`: shared confirm prompt, then the off-play record
## clear (`MindCampaign.abandon_sector_at`) and the planet page refresh
## (`updateSelected` + `rebuildList`).
func _confirm_abandon() -> void:
	_pending_abandon = true
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_confirm"):
		ui.call("show_confirm", _t("@sector.abandon.confirm"))


func _on_confirm(confirmed: bool) -> void:
	if not _pending_abandon:
		return
	_pending_abandon = false
	if not confirmed:
		return
	var campaign := campaign_node()
	var played := _is_played_sector(campaign)
	if campaign != null and campaign.has_method("abandon_sector_at"):
		campaign.call("abandon_sector_at", _planet, _sector_id)
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("close_dialog", "sector_stats")
		# `sector.isBeingPlayed()` -> `hide()` (PlanetDialog.java:1153-1154):
		# the cores die 7 ticks later and the game-over path plays.
		if played:
			ui.call("close_dialog", "planet")
			return
	var root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if root != null:
		var planet_dialog: Node = root.call("dialog", "planet")
		if planet_dialog != null and planet_dialog.has_method("refresh_page"):
			planet_dialog.call("refresh_page")


## `Sector.isBeingPlayed` for this dialog's sector (`get_sector_state`).
func _is_played_sector(campaign: Node) -> bool:
	if campaign == null or not campaign.has_method("get_sector_state"):
		return false
	var state: Dictionary = campaign.call("get_sector_state")
	return bool(state.get("campaign", false)) \
		and str(state.get("planet", "")) == _planet \
		and int(state.get("sector", -1)) == _sector_id


## The live `campaign_views` sector row this dialog was opened for.
func _find_row() -> Dictionary:
	for row_variant in campaign_section("sectors"):
		var row: Dictionary = row_variant
		if int(row.get("id", -1)) == _sector_id and str(row.get("planet", "")) == _planet:
			return row
	return {}


## `BaseDialog(sector.name())`: custom name wins, then the localized preset.
func _localized_name() -> String:
	if bool(_row.get("named", false)):
		return str(_row.get("name", ""))
	var preset := str(_row.get("preset", ""))
	if not preset.is_empty():
		var key := "sector.%s.name" % preset
		var localized := _t("@%s" % key)
		if not localized.is_empty() and localized != key:
			return localized
	return str(_row.get("name", ""))


## `UnlockableContent.uiIcon` region chain (item first; stats rows are items).
func _content_icon(content_name: String) -> Texture2D:
	for prefix in ["item", "liquid", "block", "unit", "status"]:
		var texture := MindWidgets.icon_texture("%s-%s-ui" % [prefix, content_name])
		if texture != null:
			return texture
	return null


## Optional string field of the read-model row (`null` -> "").
func _opt(row: Dictionary, key: String) -> String:
	var value: Variant = row.get(key, null)
	return "" if value == null else str(value)


## `Strings.formatMillis` (`ARC/.../Strings.java:813`): `h:mm:ss`, hours
## unpadded, minutes/seconds two digits (`SaveSlot.getPlayTime`).
func _format_millis(millis: int) -> String:
	var sign_prefix := "-" if millis < 0 else ""
	millis = absi(millis)
	@warning_ignore("integer_division")
	return "%s%d:%02d:%02d" % [
		sign_prefix, millis / 3600000, (millis % 3600000) / 60000, (millis % 60000) / 1000
	]
