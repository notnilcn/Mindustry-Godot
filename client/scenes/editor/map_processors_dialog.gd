## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/MapProcessorsDialog.java
##
## UI-only: the world-processor list editor. The scan, tag/icon reads and
## writes come from `MindEditor` (`processors`, `add_processor`,
## `rename_processor`, `set_processor_icon`, `delete_processor`); the name
## button opens the shared logic editor in its editor mode
## (`LogicBuild.showEditDialog(true)`).

extends MindDialog

## `LogicBlock.maxNameLength` (`LogicBlock.java:39`).
const MAX_NAME_LENGTH := 32
## Row height (`float h = 50f`).
const ROW_HEIGHT := 50.0
## Name button width (`Vars.mobile ? 390f : 450f`).
const NAME_WIDTH := 450.0

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"

var _editor: Node = null
var _list: VBoxContainer = null
var _search := ""
var _search_field: LineEdit = null
## `MapProcessorsDialog.processors` rows (`{x, y, tag, icon_tag}`).
var _entries: Array = []
var _pending_rename := Vector2i(-1, -1)
var _pending_icon := Vector2i(-1, -1)
var _pending_delete := Vector2i(-1, -1)


func _ready() -> void:
	# `cont.top()` (`MapProcessorsDialog.java:58`): the body sticks to the top (L01 T2).
	cont_top = true
	set_title_key("@editor.worldprocessors")
	should_pause = false
	super._ready()
	_editor = get_node_or_null(editor_path)
	clear_buttons()
	# `addCloseButton()` then `buttons.button("@add", Icon.add, ...)`
	# (`MapProcessorsDialog.java:32-33`): Back is the first button.
	add_close_button()
	add_button(_t("@add"), _add_processor, "add", 210.0)
	_connect_prompts()


## `shown(this::setup)`: rescan every world-processor center, then rebuild.
func shown() -> void:
	_entries = _processors()
	_build()


func _build() -> void:
	if cont == null:
		return
	for child in cont.get_children():
		cont.remove_child(child)
		child.queue_free()
	var root := content_table()

	# `cont.table(s -> { s.image(Icon.zoom); s.field(...).growX(); }).width(440f)`.
	# code-instantiated: the zoom glyph + search field row.
	var search_row := HBoxContainer.new()
	search_row.add_theme_constant_override("separation", 8)
	search_row.custom_minimum_size.x = 440.0
	search_row.add_child(MindWidgets.glyph("zoom", 20))
	_search_field = MindWidgets.field(_t("@players.search"))
	_search_field.text = _search
	_search_field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_search_field.text_changed.connect(func(text: String) -> void:
		_search = text.to_lower()
		_rebuild())
	search_row.add_child(_search_field)
	root.add(search_row).pad(4)
	root.row()

	# `cont.pane(t -> list = t)`.
	# code-instantiated: the processor rows are scan data, not scene-authored.
	var scroll := ScrollContainer.new()
	scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	scroll.custom_minimum_size = Vector2(680.0, 300.0)
	root.add(scroll).grow_x_axis().grow_y_axis()
	_list = VBoxContainer.new()
	# `cont.pane(t -> list = t)` has no grow (`MapProcessorsDialog.java:67-69`):
	# the list is content-sized and centred (Arc cell default), so the
	# `@editor.worldprocessors.none` block sits mid-dialog, lines left-aligned.
	_list.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	# A ScrollContainer pins a non-filling child to x=0, so a full-width row
	# centres the list (top-aligned: `cont.top()`).
	# code-instantiated: the centring wrapper of the scan-driven list.
	var center_row := HBoxContainer.new()
	center_row.alignment = BoxContainer.ALIGNMENT_CENTER
	center_row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(center_row)
	center_row.add_child(_list)
	_rebuild()


## `rebuild()`: the filtered processor rows (or `@editor.worldprocessors.none`).
func _rebuild() -> void:
	if _list == null:
		return
	for child in _list.get_children():
		_list.remove_child(child)
		child.queue_free()
	if _entries.is_empty():
		# `list.add("@editor.worldprocessors.none")`: the bundle text carries
		# `[lightgray]` markup plus an icon token.
		_list.add_child(MindWidgets.label(MindWidgets.markup("@editor.worldprocessors.none")))
		return
	for entry_value in _entries:
		var entry: Dictionary = entry_value
		var tag := str(entry.get("tag", ""))
		# `text.isEmpty() || (log.tag != null && log.tag.toLowerCase().contains(text))`.
		if not _search.is_empty() and not tag.to_lower().contains(_search):
			continue
		_list.add_child(_row(entry))


## One row: icon select, name (opens the logic editor), rename, delete.
func _row(entry: Dictionary) -> Control:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 4)
	row.custom_minimum_size.y = ROW_HEIGHT + 8.0

	# `t.button(log.iconTag == 0 ? Styles.none : ..., Styles.graySquarei, ...)`.
	# code-instantiated: the icon cell holds a glyph char or nothing.
	var icon_button := Button.new()
	icon_button.theme_type_variation = "graySquarei"
	icon_button.custom_minimum_size = Vector2(ROW_HEIGHT, ROW_HEIGHT)
	var icon_code := int(entry.get("icon_tag", 0))
	if icon_code > 0 and icon_code <= 0x10FFFF:
		var glyph := MindWidgets.glyph("", 26)
		glyph.text = String.chr(icon_code)
		glyph.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		glyph.mouse_filter = Control.MOUSE_FILTER_IGNORE
		icon_button.add_child(glyph)
	icon_button.pressed.connect(_open_icon_select.bind(entry))
	row.add_child(icon_button)

	# `t.button((tag == null ? "<no name>\n" : "[accent]" + tag + "\n") + "[lightgray][[x, y]", ...)`.
	# code-instantiated: the wide name cell carries the two-line markup.
	var name_button := MindWidgets.button("")
	name_button.theme_type_variation = "grayt"
	name_button.custom_minimum_size = Vector2(NAME_WIDTH, ROW_HEIGHT)
	var name_label := MindWidgets.label(_entry_markup(entry))
	name_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_LEFT
	name_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	name_label.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	name_label.offset_left = 10.0
	name_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	name_button.add_child(name_label)
	name_button.pressed.connect(_edit_processor.bind(entry))
	row.add_child(name_button)

	# `t.button(Icon.pencil, Styles.graySquarei, ...)` — rename.
	row.add_child(_square_icon("edit", "@editor.worldprocessors.editname", func() -> void:
		_rename_processor(entry)))

	# `t.button(Icon.trash, ...)` — delete with confirm.
	row.add_child(_square_icon("cancel", "@editor.worldprocessors.delete.confirm", func() -> void:
		_confirm_delete(entry)))
	return row


## `t.button(Icon.x, Styles.graySquarei, iconMed, ...)` cells.
func _square_icon(icon: String, tooltip_key: String, handler: Callable) -> Button:
	var button := MindWidgets.icon_button(icon, "", "graySquarei")
	button.custom_minimum_size = Vector2(ROW_HEIGHT, ROW_HEIGHT)
	button.tooltip_text = _t(tooltip_key)
	button.pressed.connect(handler)
	return button


## `(log.tag == null ? "<no name>\n" : "[accent]" + log.tag + "\n") + "[lightgray][[x, y]"`.
func _entry_markup(entry: Dictionary) -> String:
	var tag := str(entry.get("tag", ""))
	var head := "<no name>" if tag.is_empty() else "[accent]%s" % tag
	return MindWidgets.markup_text("%s\n[lightgray][[%d, %d]" % [
		head, int(entry.get("x", 0)), int(entry.get("y", 0)),
	])


## `buttons.button("@add", Icon.add, ...)`: first non-synthetic tile becomes a
## world-processor, else `@editor.worldprocessors.nospace`.
func _add_processor() -> void:
	if _editor == null or not bool(_editor.call("add_processor")):
		_notify("@editor.worldprocessors.nospace")
		return
	_entries = _processors()
	_rebuild()


## `MapProcessorsDialog.setup`: the editor scans its world for processors.
func _processors() -> Array:
	if _editor != null and _editor.has_method("processors"):
		var value: Variant = _editor.call("processors")
		if value is Array:
			return value
	return []


## `log.showEditDialog(true)`: the shared logic editor over the editor tile.
func _edit_processor(entry: Dictionary) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("open_dialog"):
		return
	ui.call(
		"open_dialog",
		"logic",
		JSON.stringify({
			"editor": true,
			"x": int(entry.get("x", 0)),
			"y": int(entry.get("y", 0)),
		})
	)


## `ui.showTextInput("", "@editor.name", LogicBlock.maxNameLength, ...)`.
func _rename_processor(entry: Dictionary) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("show_text_input"):
		return
	_pending_rename = Vector2i(int(entry.get("x", 0)), int(entry.get("y", 0)))
	ui.call(
		"show_text_input",
		"",
		_t("@editor.name"),
		MAX_NAME_LENGTH,
		str(entry.get("tag", "")),
		false,
		false
	)


## `iconSelect.show(ic -> log.iconTag = ic; rebuild())`.
func _open_icon_select(entry: Dictionary) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("open_dialog"):
		return
	_pending_icon = Vector2i(int(entry.get("x", 0)), int(entry.get("y", 0)))
	_connect_icon_select()
	ui.call("open_dialog", "icon_select", "{}")


## `ui.showConfirm("@editor.worldprocessors.delete.confirm", ...)`.
func _confirm_delete(entry: Dictionary) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("show_confirm"):
		return
	_pending_delete = Vector2i(int(entry.get("x", 0)), int(entry.get("y", 0)))
	ui.call("show_confirm", _t("@editor.worldprocessors.delete.confirm"))


func _connect_prompts() -> void:
	_connect_icon_select()
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	if ui.has_signal("text_input_result") \
			and not ui.is_connected("text_input_result", _on_text_input):
		ui.connect("text_input_result", _on_text_input)
	if ui.has_signal("confirm_result") and not ui.is_connected("confirm_result", _on_confirm):
		ui.connect("confirm_result", _on_confirm)


func _connect_icon_select() -> void:
	var ui_root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui_root == null or not ui_root.has_method("dialog"):
		return
	var dialog: Node = ui_root.call("dialog", "icon_select")
	if dialog != null and dialog.has_signal("icon_selected") \
			and not dialog.is_connected("icon_selected", _on_icon_selected):
		dialog.icon_selected.connect(_on_icon_selected)


func _on_text_input(text: String) -> void:
	var target := _pending_rename
	_pending_rename = Vector2i(-1, -1)
	if target.x < 0 or text.is_empty():
		return
	if _editor != null and bool(_editor.call("rename_processor", target.x, target.y, text)):
		_entries = _processors()
		_rebuild()


func _on_confirm(confirmed: bool) -> void:
	var target := _pending_delete
	_pending_delete = Vector2i(-1, -1)
	if not confirmed or target.x < 0:
		return
	if _editor != null and bool(_editor.call("delete_processor", target.x, target.y)):
		_entries = _processors()
		_rebuild()


func _on_icon_selected(region_name: String) -> void:
	var target := _pending_icon
	_pending_icon = Vector2i(-1, -1)
	if target.x < 0:
		return
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("close_dialog", "icon_select")
	var code := _icon_code(region_name)
	if _editor != null and bool(_editor.call("set_processor_icon", target.x, target.y, code)):
		_entries = _processors()
		_rebuild()


## `Iconc.codes.get(key)`: the icon select emits a region name; the stored
## `iconTag` is its PUA char (`IconSelectDialog`'s `consumer.get(Iconc.codes.get(key))`).
func _icon_code(region_name: String) -> int:
	var assets := get_node_or_null("/root/MindAssets")
	if assets != null and assets.has_method("unicode_str"):
		var text := str(assets.call("unicode_str", region_name))
		if not text.is_empty():
			return text.unicode_at(0)
	var glyph := MindIcons.glyph(region_name)
	return glyph.unicode_at(0) if not glyph.is_empty() else 0


func _notify(key: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("toast"):
		ui.call("toast", _t(key), "error")
