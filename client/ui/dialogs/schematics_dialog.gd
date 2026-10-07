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
	_rebuild_list()
	_rebuild_detail()


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
	var columns := HBoxContainer.new()
	columns.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_list = MindTable.new()
	_list.custom_minimum_size.x = 260
	columns.add_child(_list)
	_detail = MindTable.new()
	_detail.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	columns.add_child(_detail)
	root.add(columns).grow_x_axis().grow_y_axis()
	_rebuild_list()
	_rebuild_detail()


func _rebuild_list() -> void:
	_list.clear_children()
	var needle := _field.text.to_lower() if _field != null else ""
	var schematics := campaign_section("schematics")
	for index in schematics.size():
		var schematic: Dictionary = schematics[index]
		var name := str(schematic.get("name", ""))
		if not needle.is_empty() and not name.to_lower().contains(needle):
			continue
		# code-instantiated: schematic rows are the plan-12 Schematics registry.
		var button := MindWidgets.button(name)
		button.toggle_mode = true
		button.button_pressed = index == _selected
		button.pressed.connect(_select.bind(index))
		_list.add(button).grow_x_axis().pad(2)
		_list.row()


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
		int(schematic.get("width", 0)), int(schematic.get("height", 0)), int(schematic.get("tiles", 0))
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
