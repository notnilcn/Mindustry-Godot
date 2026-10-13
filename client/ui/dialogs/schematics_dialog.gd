## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/SchematicsDialog.java (plan 14 M5).
##
## Schematic library: search + list + detail/preview panel over the live
## `MindCampaign` schematic read models. Edit/export/delete are wired to the
## campaign facade (WS2 contract), Import reads the clipboard through
## `import_schematic`, and the icon picker reuses `icon_select_dialog`.

extends MindDialog

## Emitted when a schematic action is requested (`export`/`delete`/`edit`).
signal schematic_action(index: int, action: String)

var _field: LineEdit = null
var _list: MindTable = null
var _detail: MindTable = null
var _tags_row: HBoxContainer = null
var _tags: Array = []
var _selected_tags: Array = []
var _selected := 0
var _pending_delete := -1
var _pending_rename := -1
var _pending_icon := -1


func _ready() -> void:
	set_title_key("@schematics")
	should_pause = true
	full_dialog = true
	super._ready()
	_build()
	add_close_button()
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		if ui.has_signal("confirm_result") and not ui.is_connected("confirm_result", _on_confirm):
			ui.connect("confirm_result", _on_confirm)
		if ui.has_signal("text_input_result") and not ui.is_connected("text_input_result", _on_text_input):
			ui.connect("text_input_result", _on_text_input)


func shown() -> void:
	refresh_campaign_views()
	_load_tags()
	_rebuild_list()
	_rebuild_detail()


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


func _build() -> void:
	var root := content_table()
	var search_row := HBoxContainer.new()
	search_row.add_theme_constant_override("separation", 4)
	_field = MindWidgets.field(_t("@search"))
	_field.text_changed.connect(func(_text: String) -> void: _rebuild_list())
	search_row.add_child(_field)
	# code-instantiated: the import entry button is parameterized by the clipboard.
	var import := MindWidgets.icon_button("download", _t("@schematic.import"), "defaultt")
	import.pressed.connect(_import)
	search_row.add_child(import)
	root.add(search_row).grow_x_axis().pad(4)
	root.row()
	# `SchematicsDialog`: `@schematic.tags` label + one toggle per tag.
	_tags_row = HBoxContainer.new()
	_tags_row.add_theme_constant_override("separation", 2)
	root.add(_tags_row).grow_x_axis().pad(2)
	root.row()
	# Full-width card pane (`cols = max(screenW / 230, 1)`), then the detail
	# panel with the per-schematic actions.
	_list = MindTable.new()
	_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	root.add(_list).grow_x_axis().grow_y_axis()
	root.row()
	_detail = MindTable.new()
	_detail.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	root.add(_detail).grow_x_axis()
	_rebuild_tags()
	_rebuild_list()
	_rebuild_detail()


## Tags row (`@schematic.tags` + toggles; `showAllTags` editor deferred).
func _rebuild_tags() -> void:
	if _tags_row == null:
		return
	for child in _tags_row.get_children():
		child.queue_free()
	if _tags.is_empty():
		return
	# code-instantiated: the tag label/toggles follow the settings tag list.
	_tags_row.add_child(MindWidgets.label(_t("@schematic.tags")))
	for tag in _tags:
		var button := MindWidgets.button(str(tag))
		button.theme_type_variation = "togglet"
		button.toggle_mode = true
		button.button_pressed = _selected_tags.has(tag)
		button.pressed.connect(_toggle_tag.bind(str(tag)))
		_tags_row.add_child(button)


func _toggle_tag(tag: String) -> void:
	if _selected_tags.has(tag):
		_selected_tags.erase(tag)
	else:
		_selected_tags.append(tag)
	_rebuild_tags()
	_rebuild_list()


func _rebuild_list() -> void:
	_list.clear_children()
	var needle := _field.text.to_lower() if _field != null else ""
	var schematics := campaign_section("schematics")
	# `cols = max((int)(screenW / 230), 1)` (SchematicsDialog.java:129-150).
	var cols := maxi(int(get_viewport_rect().size.x / 230.0), 1)
	var shown := 0
	for index in schematics.size():
		var schematic: Dictionary = schematics[index]
		var name := str(schematic.get("name", ""))
		if not needle.is_empty() and not name.to_lower().contains(needle):
			continue
		var labels: Array = schematic.get("labels", [])
		var tagged := true
		for tag in _selected_tags:
			if not labels.has(tag):
				tagged = false
				break
		if not tagged:
			continue
		# code-instantiated: one SchematicImage card per library entry.
		var button := Button.new()
		button.theme_type_variation = "togglet"
		button.custom_minimum_size = Vector2(200.0, 200.0)
		button.pressed.connect(_on_card_pressed.bind(index))
		var image := MindSchematicImage.new()
		image.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		image.setup(schematic)
		button.add_child(image)
		button.mouse_entered.connect(func() -> void: image.set_hovered(true))
		button.mouse_exited.connect(func() -> void: image.set_hovered(false))
		# code-instantiated: name badge (`Styles.black3` label over the image).
		var badge := PanelContainer.new()
		var box := StyleBoxFlat.new()
		box.bg_color = MindStyles.BLACK_3
		badge.add_theme_stylebox_override("panel", box)
		badge.set_anchors_preset(Control.PRESET_BOTTOM_WIDE)
		badge.offset_top = -26.0
		badge.mouse_filter = Control.MOUSE_FILTER_IGNORE
		var label := MindWidgets.label("[center]%s[/center]" % name)
		label.mouse_filter = Control.MOUSE_FILTER_IGNORE
		badge.add_child(label)
		button.add_child(badge)
		_list.add(button).pad(4)
		shown += 1
		if shown % cols == 0:
			_list.row()
	if shown == 0:
		var key := "@none.found" if (not needle.is_empty() or not _selected_tags.is_empty()) else "@none"
		_list.add(MindWidgets.label(_t(key))).pad(8)
		_list.row()
	_list.call_deferred("sort_now")


## Card click: in game `control.input.useSchematic(s)` + hide (Java
## `SchematicsDialog.java:190-205`); in the menu it selects the detail panel.
func _on_card_pressed(index: int) -> void:
	_selected = index
	if _in_game() and _use_schematic(index):
		var ui := get_node_or_null("/root/MindUi")
		if ui != null:
			ui.call("close_dialog", "schematics")
		return
	_rebuild_list()
	_rebuild_detail()


## `state.isGame()` (the dialog is open in the running match).
func _in_game() -> bool:
	var host := get_node_or_null("/root/Spine/SimHost")
	return host != null and host.has_method("get_state") and str(host.call("get_state")) == "playing"


## `control.input.useSchematic(s)` (G08 T6b); toasts `@schematic.disabled` when
## the rules/placement state refuses (`!rules.schematicsAllowed`).
func _use_schematic(index: int) -> bool:
	var input := get_node_or_null("/root/Spine/Input")
	if input == null or not input.has_method("use_schematic"):
		return false
	var used := bool(input.call("use_schematic", index))
	if not used:
		show_toast(_t("@schematic.disabled"))
	return used


func _select(index: int) -> void:
	_selected = index
	_rebuild_list()
	_rebuild_detail()


func _rebuild_detail() -> void:
	_detail.clear_children()
	var schematics := campaign_section("schematics")
	if schematics.is_empty():
		return
	var schematic: Dictionary = schematics[clampi(_selected, 0, schematics.size() - 1)]
	_detail.add(MindWidgets.styled_label(str(schematic.get("name", "")), "techLabel")).grow_x_axis().pad(6)
	_detail.row()
	_detail.add(MindWidgets.label("%dx%d  tiles: %d" % [
		int(schematic.get("width", 0)), int(schematic.get("height", 0)),
		(schematic.get("tiles", []) as Array).size()
	])).pad(2)
	_detail.row()
	var requirements: Array = schematic.get("requirements", [])
	for requirement in requirements:
		_detail.add(MindWidgets.label("%s x%d" % [str(requirement[0]), int(requirement[1])])).pad(1)
		_detail.row()
	var buttons := HBoxContainer.new()
	# code-instantiated: the action row is parameterized by the selected schematic.
	for action in ["edit", "export", "icon", "delete"]:
		var button := MindWidgets.button(_t(_action_label(action)))
		button.pressed.connect(_on_action.bind(_selected, action))
		buttons.add_child(button)
	if bool(schematic.get("has_core", false)):
		# Core schematics are launch loadouts: open the capacity editor too.
		var configure := MindWidgets.button(_t("@configure"))
		configure.pressed.connect(_open_loadout.bind(_selected))
		buttons.add_child(configure)
	_detail.add(buttons).pad(4)


## Action -> bundle label (`SchematicsDialog` import/export/edit flows).
func _action_label(action: String) -> String:
	match action:
		"edit":
			return "@edit"
		"export":
			return "@editor.export"
		"icon":
			return "@schematic.icontag"
		"delete":
			return "@save.delete"
	return action


func _on_action(index: int, action: String) -> void:
	schematic_action.emit(index, action)
	match action:
		"edit":
			_edit(index)
		"export":
			_export(index)
		"icon":
			_open_icon_select(index)
		"delete":
			_delete(index)


# --- Facade actions (WS2 contract) -------------------------------------------

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
	_selected = int(result)
	_rebuild_list()
	_rebuild_detail()


func _delete(index: int) -> void:
	_pending_delete = index
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("show_confirm", _t("@schematic.delete.confirm"))


func _edit(index: int) -> void:
	_pending_rename = index
	var name := ""
	var schematics := campaign_section("schematics")
	if index >= 0 and index < schematics.size():
		name = str((schematics[index] as Dictionary).get("name", ""))
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("show_text_input", _t("@schematic.edit"), _t("@name"), 64, name, false, false)


## Opens the capacity editor for a core schematic (launch/schematics flow).
func _open_loadout(index: int) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	ui.call("open_dialog", "loadout", JSON.stringify({"index": index}))


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
		_selected = 0
		_rebuild_list()
		_rebuild_detail()


func _on_text_input(text: String) -> void:
	if _pending_rename < 0 or text.is_empty():
		_pending_rename = -1
		return
	var index := _pending_rename
	_pending_rename = -1
	if bool(campaign_call("rename_schematic", [index, text])):
		refresh_campaign_views()
		_rebuild_list()
		_rebuild_detail()


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
		_rebuild_detail()
