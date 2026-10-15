## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/SchematicsDialog.java (plan 14 M5).
##
## Schematic library per `SchematicsDialog`: search field, `Tags:` row (tag
## toggles + edit pencil), and one 200 px card per schematic with a 50 px
## action row (info/export/edit/delete), the `SchematicImage` preview and the
## top `black3` name badge. The Back + Import buttons live in the bottom row.
## Cards in a match start placement (`control.input.useSchematic`); in the menu
## the info button opens `schematic_info`.
##
## `Schematics.all` hides the vanilla launch loadouts (`schematic_views`), and
## every action addresses the view's real `index` field.

extends MindDialog

## Emitted when a schematic action is requested (`export`/`delete`/`edit`).
signal schematic_action(index: int, action: String)

## `SchematicsDialog.ignoreSymbols` (`Pattern.compile` of the symbol class).
const IGNORE_SYMBOLS := "[`~!@#$%^&*()\\-_=+{}|;:'\",<.>/?]"

## `SchematicsDialog.tagh` (tag button height).
const TAG_HEIGHT := 42.0

var _field: LineEdit = null
var _list: MindTable = null
var _tags_row: HBoxContainer = null
var _tags: Array = []
var _selected_tags: Array = []
## First visible schematic's view index (`firstSchematic`; Enter uses it).
var _first_index := -1
var _pending_delete := -1
var _pending_rename := -1
var _pending_icon := -1
## True while the tag text prompt is open (`showAllTags` pencil half).
var _pending_tag := false


func _ready() -> void:
	set_title_key("@schematics")
	should_pause = true
	full_dialog = true
	super._ready()
	_build()
	add_close_button()
	# `buttons.button("@schematic.import", Icon.download, this::showImport)`:
	# the import button follows Back in the bottom row.
	var import := MindWidgets.button("")
	import.custom_minimum_size = Vector2(200.0, 54.0)
	apply_pane_button(import, PANE_OUTLINE_GRAY)
	build_button_content(import, MindWidgets.glyph("download", 20), _t("@schematic.import"))
	import.pressed.connect(_import)
	if buttons != null:
		buttons.add_child(import)
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		if ui.has_signal("confirm_result") and not ui.is_connected("confirm_result", _on_confirm):
			ui.connect("confirm_result", _on_confirm)
		if ui.has_signal("text_input_result") and not ui.is_connected("text_input_result", _on_text_input):
			ui.connect("text_input_result", _on_text_input)


func shown() -> void:
	refresh_campaign_views()
	_load_tags()
	# `shown(() -> { searchField.setText(search = ""); setup(); })`.
	if _field != null:
		_field.text = ""
	_rebuild_tags()
	_rebuild_list()


## `SchematicsDialog` tags (`Core.settings` JSON `schematic-tags`).
func _load_tags() -> void:
	_tags.clear()
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("settings_get_text"):
		return
	var parsed: Variant = JSON.parse_string(str(ui.call("settings_get_text", "schematic-tags", "[]")))
	if parsed is Array:
		for tag in parsed:
			if not str(tag).is_empty() and not _tags.has(str(tag)):
				_tags.append(str(tag))


func _save_tags() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("settings_put_text"):
		ui.call("settings_put_text", "schematic-tags", JSON.stringify(_tags))


func _build() -> void:
	var root := content_table()
	# `cont.table(s -> { s.image(Icon.zoom); s.add(searchField).growX() })`.
	var search_row := HBoxContainer.new()
	search_row.add_theme_constant_override("separation", 4)
	search_row.add_child(MindWidgets.glyph("zoom", 22))
	_field = MindWidgets.field(_t("@schematic.search"))
	_field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	# `searchField.changed(() -> { search = searchField.getText(); rebuildPane.run(); })`.
	_field.text_changed.connect(func(_text: String) -> void: _rebuild_list())
	# `Binding.chat` with the field focused -> use/inspect the first match.
	_field.text_submitted.connect(func(_text: String) -> void: _search_submit())
	search_row.add_child(_field)
	root.add(search_row).grow_x_axis().pad(4)
	root.row()
	# `cont.table(in -> { in.left(); in.add("@schematic.tags"); pane(toggles); button(Icon.pencilSmall, showAllTags) })`.
	_tags_row = HBoxContainer.new()
	_tags_row.add_theme_constant_override("separation", 2)
	root.add(_tags_row).grow_x_axis().pad(2)
	root.row()
	# `cont.pane(t -> ...)` with `cols = max(screenW / 230, 1)`.
	_list = MindTable.new()
	_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	root.add(_list).grow_x_axis().grow_y_axis()
	root.row()
	_rebuild_tags()
	_rebuild_list()


## `rebuildTags` (`SchematicsDialog.java:86-116`): `@schematic.tags` label, one
## toggle per saved tag (checked while selected) and the edit pencil.
func _rebuild_tags() -> void:
	if _tags_row == null:
		return
	for child in _tags_row.get_children():
		child.queue_free()
	# code-instantiated: the tag label/toggles follow the settings tag list.
	_tags_row.add_child(MindWidgets.label(_t("@schematic.tags")))
	for tag in _tags:
		var button := MindWidgets.button(str(tag))
		button.theme_type_variation = "togglet"
		button.toggle_mode = true
		button.button_pressed = _selected_tags.has(tag)
		button.custom_minimum_size = Vector2(0, TAG_HEIGHT)
		button.pressed.connect(_toggle_tag.bind(str(tag)))
		_tags_row.add_child(button)
	var pencil := MindWidgets.glyph_button("pencil", _t("@schematic.edittags"), "emptyi")
	pencil.custom_minimum_size = Vector2(TAG_HEIGHT, TAG_HEIGHT)
	pencil.pressed.connect(_edit_tags)
	_tags_row.add_child(pencil)


func _toggle_tag(tag: String) -> void:
	if _selected_tags.has(tag):
		_selected_tags.erase(tag)
	else:
		_selected_tags.append(tag)
	_rebuild_list()


## `showAllTags` pencil: the port adds a tag through the shared text prompt
## (Java opens a per-tag editor dialog; the toggles themselves are shared).
func _edit_tags() -> void:
	_pending_tag = true
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("show_text_input", _t("@schematic.edittags"), _t("@schematic.tags"), 24, "", false, false)


## `rebuildPane` (`SchematicsDialog.java:118-224`).
func _rebuild_list() -> void:
	if _list == null:
		return
	_list.clear_children()
	var search := _field.text if _field != null else ""
	var needle := _strip_symbols(search.to_lower())
	var schematics := campaign_section("schematics")
	# `cols = max((int)(screenW / 230), 1)`.
	var cols := maxi(int(get_viewport_rect().size.x / 230.0), 1)
	var shown := 0
	_first_index = -1
	for row_variant in schematics:
		var schematic: Dictionary = row_variant
		# `selectedTags.any() && !s.labels.containsAll(selectedTags)`.
		var labels: Array = schematic.get("labels", [])
		var tagged := true
		for tag in _selected_tags:
			if not labels.has(tag):
				tagged = false
				break
		if not tagged:
			continue
		# `!search.isEmpty() && !ignoreSymbols(name).contains(searchString)`.
		if not needle.is_empty() and not _strip_symbols(str(schematic.get("name", "")).to_lower()).contains(needle):
			continue
		if _first_index < 0:
			_first_index = int(schematic.get("index", -1))
		_list.add(_build_card(schematic)).pad(4)
		shown += 1
		if shown % cols == 0:
			_list.row()
	if shown == 0:
		# `@none.found` when filtering, plain `@none` (light gray) otherwise.
		if not needle.is_empty() or not _selected_tags.is_empty():
			_list.add(MindWidgets.label(_t("@none.found")))
		else:
			var none := MindWidgets.label(_t("@none"))
			none.modulate = Color.LIGHT_GRAY
			_list.add(none)
		_list.row()
	_list.call_deferred("sort_now")


## One card (`SchematicsDialog.java:150-205`): a flat button with the `pane`
## background holding the 50 px action row and the 200 px image stack.
func _build_card(schematic: Dictionary) -> Button:
	var index := int(schematic.get("index", -1))
	# code-instantiated: one card per filtered schematic row.
	var button := Button.new()
	button.theme_type_variation = "flati"
	button.custom_minimum_size = Vector2(200.0, 250.0)
	var pane := _pane_box(Color(0, 0, 0, 1), PANE_OUTLINE_GRAY)
	button.add_theme_stylebox_override("normal", pane)
	button.add_theme_stylebox_override("hover", pane)
	button.add_theme_stylebox_override("pressed", pane)
	button.pressed.connect(_on_card_pressed.bind(index))
	var column := VBoxContainer.new()
	column.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	column.add_theme_constant_override("separation", 0)
	column.mouse_filter = Control.MOUSE_FILTER_IGNORE
	# `b.table(buttons -> { buttons.left(); buttons.defaults().size(50f); ... }).growX().height(50f)`.
	var actions := HBoxContainer.new()
	actions.add_theme_constant_override("separation", 0)
	actions.mouse_filter = Control.MOUSE_FILTER_IGNORE
	for action in [
		{"icon": "info", "tooltip": "@info.title", "action": "info"},
		{"icon": "upload", "tooltip": "@editor.export", "action": "export"},
		{"icon": "pencil", "tooltip": "@schematic.edit", "action": "edit"},
		{"icon": "trash", "tooltip": "@save.delete", "action": "delete"},
	]:
		# code-instantiated: fixed upstream action row (four card buttons).
		var action_button := MindWidgets.glyph_button(
			str(action.get("icon", "")), _t(str(action.get("tooltip", ""))), "emptyi"
		)
		action_button.custom_minimum_size = Vector2(50.0, 50.0)
		action_button.pressed.connect(_on_card_action.bind(index, str(action.get("action", ""))))
		actions.add_child(action_button)
	column.add_child(actions)
	# `b.stack(new SchematicImage(s), new Table(n -> { n.top(); black3 badge })).size(200f)`.
	var stack := Control.new()
	stack.custom_minimum_size = Vector2(200.0, 200.0)
	stack.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var image := MindSchematicImage.new()
	image.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	image.setup(schematic)
	stack.add_child(image)
	var badge := PanelContainer.new()
	var box := StyleBoxFlat.new()
	box.bg_color = MindStyles.BLACK_3
	box.content_margin_left = 4.0
	box.content_margin_right = 4.0
	badge.add_theme_stylebox_override("panel", box)
	badge.set_anchors_preset(Control.PRESET_TOP_WIDE)
	badge.mouse_filter = Control.MOUSE_FILTER_IGNORE
	# `label.setEllipsis(true)`, `maxWidth(200f - 8f)`, centred, top.
	var name_label := Label.new()
	name_label.text = str(schematic.get("name", ""))
	name_label.custom_minimum_size = Vector2(192.0, 0)
	name_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	name_label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	name_label.clip_text = true
	name_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	badge.add_child(name_label)
	stack.add_child(badge)
	column.add_child(stack)
	button.add_child(column)
	button.mouse_entered.connect(func() -> void: image.set_hovered(true))
	button.mouse_exited.connect(func() -> void: image.set_hovered(false))
	return button


## Card press (`SchematicsDialog.java:190-205`): menu -> info; in game ->
## `control.input.useSchematic(s)` + hide (or `@schematic.disabled`).
func _on_card_pressed(index: int) -> void:
	if index < 0:
		return
	if not _in_game():
		_open_info(index)
		return
	if not _use_schematic(index):
		show_toast(_t("@schematic.disabled"))
		return
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("close_dialog", "schematics")


## Card action row press: never falls through as a card press (the buttons stop
## the event; Java `sel[0].childrenPressed()`).
func _on_card_action(index: int, action: String) -> void:
	match action:
		"info":
			_open_info(index)
		"export":
			_export(index)
		"edit":
			_edit(index)
		"delete":
			_delete(index)
	schematic_action.emit(index, action)


## Enter in the search field (`Binding.chat`): in game use the first match, in
## the menu open its info (`SchematicsDialog.java:122-133`).
func _search_submit() -> void:
	if _first_index < 0:
		return
	if not _in_game():
		_open_info(_first_index)
		return
	if not _use_schematic(_first_index):
		show_toast(_t("@schematic.disabled"))
		return
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("close_dialog", "schematics")


## `showInfo(schematic)` -> the `schematic_info` dialog.
func _open_info(index: int) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("open_dialog"):
		ui.call("open_dialog", "schematic_info", JSON.stringify({"index": index}))


## `state.isGame()` (the dialog is open in the running match). `should_pause`
## turns the port's phase into `paused`, which is still a game
## (`custom_rules_dialog` uses the same `!= "menu"` test).
func _in_game() -> bool:
	var host := get_node_or_null("/root/Spine/SimHost")
	return host != null and host.has_method("get_state") and str(host.call("get_state")) != "menu"


## `control.input.useSchematic(s)` (G08 T6b).
func _use_schematic(index: int) -> bool:
	var input := get_node_or_null("/root/Spine/Input")
	if input == null or not input.has_method("use_schematic"):
		return false
	return bool(input.call("use_schematic", index))


## `SchematicsDialog.ignoreSymbols`: strip the symbol class from a name.
func _strip_symbols(value: String) -> String:
	var regex := RegEx.new()
	if regex.compile(IGNORE_SYMBOLS) != OK:
		return value
	return regex.sub(value, "", true)


# --- Facade actions ----------------------------------------------------------

## `SchematicsDialog.showExport` -> `@copy.clipboard` half.
func _export(index: int) -> void:
	var result: Variant = campaign_call("export_schematic", [index])
	var text := str(result) if result != null else ""
	if text.is_empty():
		show_toast(_t("@none"))
		return
	DisplayServer.clipboard_set(text)
	show_toast(_t("@copied"))


## `SchematicsDialog.showImport` clipboard half.
func _import() -> void:
	var text := DisplayServer.clipboard_get()
	if text.is_empty():
		show_toast(_t("@none"))
		return
	var result: Variant = campaign_call("import_schematic", [text])
	if result == null or int(result) < 0:
		show_toast(_t("@none"))
		return
	refresh_campaign_views()
	_rebuild_list()


## `@schematic.delete.confirm`; mod-provided schematics show the removal note
## (`mod.item.remove`) instead of the confirm.
func _delete(index: int) -> void:
	var schematics := campaign_section("schematics")
	var mod_name := ""
	for row_variant in schematics:
		var row: Dictionary = row_variant
		if int(row.get("index", -1)) == index:
			mod_name = str(row.get("mod_name", ""))
			break
	if not mod_name.is_empty():
		show_toast(MindWidgets.markup_format("@mod.item.remove", [mod_name]))
		return
	_pending_delete = index
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("show_confirm", _t("@schematic.delete.confirm"))


func _edit(index: int) -> void:
	_pending_rename = index
	var name := ""
	var schematics := campaign_section("schematics")
	for row_variant in schematics:
		var row: Dictionary = row_variant
		if int(row.get("index", -1)) == index:
			name = str(row.get("name", ""))
			break
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("show_text_input", _t("@schematic.edit"), _t("@name"), 64, name, false, false)


## `SchematicsDialog` icon tag picker (`IconSelectDialog`).
func _open_icon_select(index: int) -> void:
	_pending_icon = index
	var ui := get_node_or_null("/root/MindUi")
	var root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui == null:
		return
	if root != null and root.has_method("dialog"):
		var dialog: Node = root.call("dialog", "icon_select")
		if dialog != null and dialog.has_signal("icon_selected") and not dialog.icon_selected.is_connected(_on_icon_selected):
			dialog.icon_selected.connect(_on_icon_selected)
	ui.call("open_dialog", "icon_select", JSON.stringify({"index": index}))


func _on_confirm(confirmed: bool) -> void:
	if not confirmed or _pending_delete < 0:
		_pending_delete = -1
		return
	var index := _pending_delete
	_pending_delete = -1
	if bool(campaign_call("delete_schematic", [index])):
		refresh_campaign_views()
		_rebuild_list()


func _on_text_input(text: String) -> void:
	if _pending_tag:
		_pending_tag = false
		if not text.is_empty() and not _tags.has(text):
			_tags.append(text)
			_save_tags()
			_rebuild_tags()
			_rebuild_list()
		return
	if _pending_rename < 0:
		return
	if text.is_empty():
		_pending_rename = -1
		return
	var index := _pending_rename
	_pending_rename = -1
	if bool(campaign_call("rename_schematic", [index, text])):
		refresh_campaign_views()
		_rebuild_list()


func _on_icon_selected(region_name: String) -> void:
	if _pending_icon < 0:
		return
	var index := _pending_icon
	_pending_icon = -1
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("close_dialog", "icon_select")
	if bool(campaign_call("set_schematic_icon", [index, region_name])):
		refresh_campaign_views()
		_rebuild_list()
