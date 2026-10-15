## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/LoadDialog.java
##
## "Load Game" dialog: a search row (field + gamemode filter toggles), a grid of
## save cards, and an "Import Save" button. The listing comes from the live
## `Saves` store (`MindCampaign.list_save_slots`), with `_context.slots` taking
## precedence when supplied; with no slots the grid renders the empty state
## (`@save.none`). Clicking a card body emits `slot_selected(name)`, queues the
## slot load on the live facade, and closes.

extends MindDialog

## Emitted when a save card body is clicked (not one of its icon buttons).
signal slot_selected(slot: String)

## Gamemode filter toggles (`Gamemode.all`; sandbox uses the terrain glyph).
const FILTERS := [
	{"mode": "survival", "icon": "mode-survival"},
	{"mode": "attack", "icon": "mode-attack"},
	{"mode": "pvp", "icon": "mode-pvp"},
	{"mode": "sandbox", "icon": "terrain"},
]

var _field: LineEdit = null
var _grid: MindTable = null
## Gamemode names whose saves are hidden from the grid (`hidden` upstream).
var _hidden: Dictionary = {}
## Slot rows for the current open (`Saves.getSaveSlots` snapshot).
var _slots: Array = []
## Registered map names (`SaveMeta` map lookup), filled lazily.
var _map_names: Dictionary = {}
## Pending card-action prompt targets (`confirm_result`/`text_input_result`).
var _pending_delete := ""
var _pending_rename := ""
## Pending export slot stem (`file_chosen`).
var _pending_export := ""


func _ready() -> void:
	set_title_key("@loadgame")
	should_pause = false
	full_dialog = true
	super._ready()
	_build()
	_connect_prompts()
	_connect_chooser()


## `LoadDialog` `modifyButton`'s prompts answer through the shared `MindUi`
## signals (`show_confirm`/`show_text_input`).
func _connect_prompts() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	if ui.has_signal("confirm_result") and not ui.is_connected("confirm_result", _on_confirm):
		ui.connect("confirm_result", _on_confirm)
	if ui.has_signal("text_input_result") and not ui.is_connected("text_input_result", _on_text_input):
		ui.connect("text_input_result", _on_text_input)


func _connect_chooser() -> void:
	var chooser := get_node_or_null("/root/Spine/Ui/UiRoot/DialogLayer/file_chooser")
	if chooser == null or not chooser.has_signal("file_chosen"):
		return
	if not chooser.is_connected("file_chosen", _on_file_chosen):
		chooser.connect("file_chosen", _on_file_chosen)


func shown() -> void:
	# Upstream `shown` resets the search string and re-reads the live slot list.
	if _field != null and not _field.text.is_empty():
		_field.text = ""
	_refresh_slots()
	_rebuild_list()


## Context slots (MCP/test injection) win; otherwise the live `Saves` listing.
func _refresh_slots() -> void:
	var injected: Variant = context().get("slots", null)
	if injected is Array:
		_slots = injected
		return
	var campaign := campaign_node()
	if campaign == null or not campaign.has_method("list_save_slots"):
		_slots = []
		return
	var listed: Variant = campaign.call("list_save_slots")
	_slots = listed if listed is Array else []


func _build() -> void:
	var root := content_table()

	# code-instantiated: the search row is data-driven from the gamemode list.
	var search_row := HBoxContainer.new()
	search_row.add_theme_constant_override("separation", 6)
	# code-instantiated: the zoom glyph is an icon-font run with no static scene.
	var zoom := MindWidgets.glyph("zoom", 22)
	search_row.add_child(zoom)
	_field = MindWidgets.field(_t("@save.search"))
	_field.max_length = 50
	_field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_field.text_changed.connect(func(_value: String) -> void: _rebuild_list())
	search_row.add_child(_field)
	_add_filters(search_row)
	root.add(search_row).grow_x_axis().pad(4)
	root.row()

	# code-instantiated: the save grid is a data-driven list of save slots.
	_grid = MindTable.new()
	root.add(_grid).grow_x_axis().grow_y_axis().pad(4)

	# Bottom row: Back (`left`) and `@save.import` (`add`).
	add_button(_t("@back"), _close, "left")
	add_button(_t("@save.import"), _open_import, "add")

	_rebuild_list()


func _add_filters(row: HBoxContainer) -> void:
	# code-instantiated: filter toggles track the local hidden-gamemode set.
	for filter in FILTERS:
		var mode := str(filter.get("mode", ""))
		var button := MindWidgets.icon_button(str(filter.get("icon", "")), "", "emptyTogglei")
		button.toggle_mode = true
		button.button_pressed = true
		button.tooltip_text = _t("@mode.%s.name" % mode)
		button.pressed.connect(_toggle_filter.bind(mode))
		row.add_child(button)


func _toggle_filter(mode: String) -> void:
	if _hidden.has(mode):
		_hidden.erase(mode)
	else:
		_hidden[mode] = true
	_rebuild_list()


func _rebuild_list() -> void:
	if _grid == null:
		return
	_grid.clear_children()
	var slots: Array = _slots
	var needle := _field.text.strip_edges().to_lower() if _field != null else ""
	var columns := maxi(int(get_viewport_rect().size.x / 470.0), 1)
	var any := false
	var count := 0
	for entry in slots:
		if not (entry is Dictionary):
			continue
		var slot: Dictionary = entry
		# `SaveSlot.isHidden`: sector saves are hidden from the load list.
		if bool(slot.get("sector", false)):
			continue
		var name := str(slot.get("name", ""))
		if not needle.is_empty() and not name.to_lower().contains(needle):
			continue
		var mode := str(slot.get("mode", ""))
		if not mode.is_empty() and _hidden.has(mode):
			continue
		any = true
		_grid.add(_build_card(slot)).grow_x_axis().pad(4).set_min_width(300.0)
		count += 1
		if count % columns == 0:
			_grid.row()
	if not any:
		# code-instantiated: the empty-state text is a data-driven grid cell.
		var none := MindWidgets.label(_tm("@save.none"))
		none.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		_grid.add(none).grow_x_axis().grow_y_axis().pad(16).set_align(0)
	# Nested grids miss the deferred sort; place now and again next frame.
	_grid.sort_now()
	_grid.call_deferred("sort_now")


func _build_card(slot: Dictionary) -> Button:
	var name := str(slot.get("name", ""))
	# code-instantiated: save cards are data-driven from the save-slot listing.
	var card := MindWidgets.button("")
	card.theme_type_variation = "grayt"
	# Five meta lines (map, mode/wave, autosave, playtime, date) + the title row.
	card.custom_minimum_size = Vector2(400, 196)
	card.pressed.connect(_select.bind(slot))

	# code-instantiated: card body follows the slot data; ignores clicks so the
	# card button itself handles selection.
	var content := VBoxContainer.new()
	content.mouse_filter = Control.MOUSE_FILTER_IGNORE
	content.add_theme_constant_override("separation", 4)
	card.add_child(content)
	content.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)

	# code-instantiated: per-card title/action row (data-driven icon buttons).
	var title := HBoxContainer.new()
	title.mouse_filter = Control.MOUSE_FILTER_IGNORE
	content.add_child(title)
	var name_label := MindWidgets.styled_label(name, "techLabel")
	name_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	name_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	title.add_child(name_label)
	# `modifyButton`: `Icon.save` autosave toggle, `Icon.trash` delete,
	# `Icon.pencil` rename, `Icon.export` (`LoadDialog.java:105-145`).
	var autosave := _icon_action("save", "emptyTogglei", bool(slot.get("autosave", false)))
	autosave.toggled.connect(func(pressed: bool) -> void: _set_slot_autosave(slot, pressed))
	title.add_child(autosave)
	var delete_button := _icon_action("trash", "emptyi", false)
	delete_button.pressed.connect(func() -> void: _delete_slot(slot))
	title.add_child(delete_button)
	var rename_button := _icon_action("pencil", "emptyi", false)
	rename_button.pressed.connect(func() -> void: _rename_slot(slot))
	title.add_child(rename_button)
	var export_button := _icon_action("export", "emptyi", false)
	export_button.pressed.connect(func() -> void: _export_slot(slot))
	title.add_child(export_button)

	# code-instantiated: preview/meta row is per-slot presentation data.
	var body := HBoxContainer.new()
	body.mouse_filter = Control.MOUSE_FILTER_IGNORE
	body.add_theme_constant_override("separation", 8)
	body.size_flags_vertical = Control.SIZE_EXPAND_FILL
	content.add_child(body)

	# `Saves.SaveSlot.previewTexture()`: the saved `<previews>/save_slot_<stem>.png`
	# (`Saves.java:359-370`); the `nomap` glyph stays until one exists.
	var texture := _save_preview_texture(slot)
	if texture != null:
		# code-instantiated: save preview textures come from the live slot store.
		var image := MindBorderImage.new()
		image.texture = texture
		image.thickness = 3.0
		image.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		image.custom_minimum_size = Vector2(160, 100)
		image.mouse_filter = Control.MOUSE_FILTER_IGNORE
		body.add_child(image)
	else:
		# code-instantiated: `nomap` placeholder glyph is an icon-font run.
		var preview := Panel.new()
		preview.mouse_filter = Control.MOUSE_FILTER_IGNORE
		preview.custom_minimum_size = Vector2(160, 100)
		body.add_child(preview)
		var map_icon := MindWidgets.glyph("map", 40)
		map_icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
		preview.add_child(map_icon)
		map_icon.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)

	# code-instantiated: metadata column lines come from the slot dictionary.
	var meta := VBoxContainer.new()
	meta.mouse_filter = Control.MOUSE_FILTER_IGNORE
	meta.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	body.add_child(meta)
	for line in _meta_lines(slot):
		var label := MindWidgets.label(line)
		label.mouse_filter = Control.MOUSE_FILTER_IGNORE
		label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		meta.add_child(label)

	return card


## `SaveMeta.map`: the registered map whose name matches the save's
## `mapname`, else `@unknown`.
func _resolve_map(map_name: String) -> String:
	if map_name.is_empty():
		return _t("@unknown")
	if _map_names.is_empty():
		var preview := get_node_or_null("/root/Spine/MindPreview")
		if preview != null and preview.has_method("maps_list"):
			for row in preview.call("maps_list"):
				_map_names[str((row as Dictionary).get("name", ""))] = true
	return map_name if _map_names.has(map_name) else _t("@unknown")


## `Strings.formatMillis`: `H:MM:SS`.
func _format_millis(millis: int) -> String:
	var sign := "-" if millis < 0 else ""
	millis = absi(millis)
	return "%s%d:%02d:%02d" % [sign, millis / 3600000, (millis % 3600000) / 60000, (millis % 60000) / 1000]


## `Saves.SaveSlot.previewTexture()` for a slot row (`save_preview_texture`).
func _save_preview_texture(slot: Dictionary) -> Texture2D:
	var file := str(slot.get("file", ""))
	var stem := file.get_file().get_basename()
	if stem.is_empty():
		stem = str(slot.get("name", ""))
	if stem.is_empty():
		return null
	var campaign := campaign_node()
	if campaign == null or not campaign.has_method("save_preview_texture"):
		return null
	var value: Variant = campaign.call("save_preview_texture", stem)
	return value if value is Texture2D else null


## `SaveSlot.getDate` (`DateFormat.getDateTimeInstance()`, en-US medium:
## `Oct 12, 2026, 12:54:33 AM`) in the local time zone.
func _format_date(timestamp_ms: int) -> String:
	var bias := int(Time.get_time_zone_from_system().get("bias", 0)) * 60
	var dt := Time.get_datetime_dict_from_unix_time(timestamp_ms / 1000 + bias)
	var months := ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
	var hour := int(dt.hour) % 12
	if hour == 0:
		hour = 12
	return "%s %d, %d, %d:%02d:%02d %s" % [
		months[int(dt.month) - 1], int(dt.day), int(dt.year), hour, int(dt.minute), int(dt.second),
		"PM" if int(dt.hour) >= 12 else "AM",
	]


func _icon_action(icon_name: String, style: String, is_pressed: bool) -> Button:
	# code-instantiated: card action buttons are per-slot (no static scene).
	var button := MindWidgets.icon_button(icon_name, "", style)
	button.custom_minimum_size = Vector2(34, 34)
	if style == "emptyTogglei":
		button.toggle_mode = true
		button.button_pressed = is_pressed
	return button


func _meta_lines(slot: Dictionary) -> Array:
	var map_name := str(slot.get("map", ""))
	if map_name.is_empty():
		map_name = _resolve_map(str(slot.get("map_name", "")))
	var mode := str(slot.get("mode", ""))
	if not mode.is_empty():
		mode = _t("@mode.%s.name" % mode)
	var wave := int(slot.get("wave", 0))
	var autosave := bool(slot.get("autosave", false))
	var playtime := str(slot.get("playtime", ""))
	if slot.has("playtime_ms"):
		playtime = _format_millis(int(slot.get("playtime_ms", 0)))
	var date := str(slot.get("date", ""))
	if date.is_empty() and int(slot.get("timestamp_ms", 0)) > 0:
		date = _format_date(int(slot.get("timestamp_ms", 0)))
	var lines := [
		_fmt("@save.map", [map_name]),
		"%s /%s" % [mode, _fmt("@save.wave", [wave])],
		_fmt("@save.autosave", [_t("@on" if autosave else "@off")]),
		_fmt("@save.playtime", [playtime]),
	]
	if not date.is_empty():
		lines.append(date)
	return lines


func _select(slot: Dictionary) -> void:
	slot_selected.emit(str(slot.get("name", "")))
	_load_slot(slot)
	_close()


## `LoadDialog.runLoadSave`: hands the slot to the live facade, which queues the
## load at the next IO boundary (`MindCampaign.load_slot`).
func _load_slot(slot: Dictionary) -> void:
	var file := str(slot.get("file", ""))
	var stem := file.get_file().get_basename()
	if stem.is_empty():
		stem = str(slot.get("name", ""))
	if stem.is_empty():
		return
	campaign_call("load_slot", [stem])


## `LoadDialog.addSetup`'s `@save.import`: `FileChooser.open("msav")` with the
## `.msav` filter; the chosen file goes through `MindCampaign.import_save`.
func _open_import() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	_connect_chooser()
	ui.call("open_dialog", "file_chooser", JSON.stringify({"open": true, "extensions": ["msav"]}))


func _on_file_chosen(path: String) -> void:
	if not _pending_export.is_empty():
		var stem := _pending_export
		_pending_export = ""
		_close_chooser()
		if not bool(campaign_call("export_save_slot", [stem, path])):
			_notify(_t("@editor.errorsave"))
		return
	_close_chooser()
	# `SaveIO.isSaveValid` -> `getMeta` sector check -> `Saves.importSave`
	# (`LoadDialog.java:198-216`); errors keep the dialog open.
	var result := str(campaign_call("import_save", [path]))
	match result:
		"":
			_refresh_slots()
			_rebuild_list()
		"invalid":
			_notify(_t("@save.import.invalid"))
		"nocampaign":
			_notify(_t("@save.nocampaign"))
		_:
			var message := result.trim_prefix("fail:")
			_notify(_fmt("@save.import.fail", [message]))
	_rebuild_list()


func _close_chooser() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("close_dialog"):
		ui.call("close_dialog", "file_chooser")


## The file stem used by the main `save_slot`/`load_slot` calls.
func _slot_stem(slot: Dictionary) -> String:
	var file := str(slot.get("file", ""))
	var stem := file.get_file().get_basename()
	if stem.is_empty():
		stem = str(slot.get("name", ""))
	return stem


## `Icon.save`: `slot.setAutosave(!slot.isAutosave())`.
func _set_slot_autosave(slot: Dictionary, on: bool) -> void:
	campaign_call("set_slot_autosave", [_slot_stem(slot), on])


## `Icon.trash`: `ui.showConfirm("@confirm", "@save.delete.confirm", ...)`.
func _delete_slot(slot: Dictionary) -> void:
	_pending_delete = _slot_stem(slot)
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_confirm"):
		ui.call("show_confirm", _t("@save.delete.confirm"))


## `Icon.pencil`: `ui.showTextInput("@save.rename", "@save.rename.text", ...)`.
func _rename_slot(slot: Dictionary) -> void:
	_pending_rename = _slot_stem(slot)
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_text_input"):
		ui.call("show_text_input", _t("@save.rename"), _t("@save.rename.text"), 64, str(slot.get("name", "")), false, false)


## `Icon.export`: `FileChooser.export(slot.getName(), "msav", slot::exportFile)`;
## a save with embedded assets must be loaded first (`@save.export.needsload`).
func _export_slot(slot: Dictionary) -> void:
	if bool(slot.get("external_assets", false)):
		_notify(_t("@save.export.needsload"))
		return
	_pending_export = _slot_stem(slot)
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("open_dialog"):
		return
	_connect_chooser()
	ui.call("open_dialog", "file_chooser", JSON.stringify({"open": false, "extensions": ["msav"]}))


func _on_confirm(confirmed: bool) -> void:
	if not confirmed or _pending_delete.is_empty():
		_pending_delete = ""
		return
	var stem := _pending_delete
	_pending_delete = ""
	if bool(campaign_call("delete_save_slot", [stem])):
		_refresh_slots()
		_rebuild_list()


func _on_text_input(text: String) -> void:
	if _pending_rename.is_empty() or text.strip_edges().is_empty():
		_pending_rename = ""
		return
	var stem := _pending_rename
	_pending_rename = ""
	if bool(campaign_call("rename_save_slot", [stem, text])):
		_refresh_slots()
		_rebuild_list()


## `ui.showErrorMessage` via the shared `show_text` prompt (`@error.title`).
func _notify(text: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_text"):
		ui.call("show_text", MindWidgets.markup_text(_t("@error.title")), MindWidgets.markup_text(text))


func _close() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("close_dialog"):
		ui.call("close_dialog", "load")
	else:
		hide_dialog()


## Bundle lookup with `{0}`-style substitution (`Bundle.format`).
func _fmt(key: String, args: Array) -> String:
	var result := _t(key)
	for index in args.size():
		result = result.replace("{%d}" % index, str(args[index]))
	return result
