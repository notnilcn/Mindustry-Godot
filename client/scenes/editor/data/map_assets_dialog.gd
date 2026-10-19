## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/data/MapAssetsDialog.java plus
## `AssetView`, `MapPatchesView`, `MapContentView`, `MapBundlesView`,
## `MapImagesView` and `MapAudioView` (plan 19 M6 §3.12 / PAR-0394).
##
## UI-only: the type strip (menu + six originals' tabs + current-type label +
## guide), the search row, the per-type list/grid body and the per-type action
## buttons. The records and every mutation live in the `MindMods` owner
## (`state.data`, `mind_core::mods::assets::ModDataManager`); the dialog calls
## `MindEditor.assets()` / `data_*` and the owner applies the decorators.
##
## Java's five view classes are the `_build_*` functions below; each mirrors its
## `AssetView.build` body. Deviations recorded on PAR-0394: generated content
## icons are not packed yet (the content icon cell uses the type glyph), the
## per-asset `warnings` list is not stored by `ModDataManager` (content errors
## are shown as one dialog-level warning), and no pixel-perfect frame hunting.

extends MindDialog

## `DataAssetType.all` in ordinal order (the tab identity).
const TYPES := ["patch", "content", "bundle", "image", "sound", "music"]
## `DataAssetType.localized()` bundle keys.
const TYPE_KEYS := {
	"patch": "@asset.patch",
	"content": "@asset.content",
	"bundle": "@asset.bundle",
	"image": "@asset.image",
	"sound": "@asset.sound",
	"music": "@asset.music",
}
## `MapAssetsDialog.typeIcons` (`Icon.fileCode`, `box`, `fileText`, `image`,
## `volumeUp`, `music`).
const TYPE_ICONS := {
	"patch": "file-code",
	"content": "box",
	"bundle": "file-text",
	"image": "image",
	"sound": "volume-up",
	"music": "music",
}
## `MapContentView.contentIcons` (`Icon.box/liquid/units/distribution/planet/
## drizzle/power`; `drizzle` has no MindIcons alias, `effect` stands in).
const CONTENT_ICONS := {
	"item": "box",
	"liquid": "liquid",
	"unit": "units",
	"block": "distribution",
	"planet": "planet",
	"weather": "effect",
	"status": "power",
}
## `ContentAsset.loadableContent` (the add-content type row).
const CONTENT_TYPES := ["item", "block", "liquid", "status", "unit", "weather"]
## `MapAssetsDialog` row geometry: `h = 50f`, `mobile ? 390f : 450f`.
const ROW_HEIGHT := 50.0
const ROW_WIDTH := 450.0
const ROW_WIDTH_MOBILE := 390.0
const IMAGE_CELL := 200.0
## `iconMed` (32 px) in the icon-only cells.
const ICON_SIZE := 32.0
## `Vars.patchesGuideURL`.
const GUIDE_URL := "https://mindustrygame.github.io/wiki/datapatches/"
## Java `unsafeNamePattern` (`MapContentView`).
const UNSAFE_NAME := "[\\[\\]{}`!@#$%^&*();:,]"

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"

var _editor: Node = null
var _type := "patch"
var _search := ""
var _rows: Array = []
var _type_buttons: Dictionary = {}
var _type_label: MindLabel = null
var _search_field: LineEdit = null
var _list: VBoxContainer = null
## Pending `ui.showConfirm` handler (`confirm_result` is a global signal).
var _pending_confirm := Callable()
## Last playing data audio row name (music pause icon).
var _playing_audio := ""
## Image path awaiting the rename prompt result.
var _renaming_image := ""


func _ready() -> void:
	# `super("")`: the title is not displayed.
	set_title_text("")
	should_pause = false
	cont_top = true
	super._ready()
	_editor = get_node_or_null(editor_path)
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_signal("confirm_result") \
			and not ui.is_connected("confirm_result", _on_confirm):
		ui.connect("confirm_result", _on_confirm)


## `shown`: reset search, then `changeType(patch)`.
func shown() -> void:
	_search = ""
	if _search_field != null:
		_search_field.text = ""
	refetch()
	_build()


## `hidden`: any item added through the dialog needs its arrays fixed and the
## editor's block selection rebuilt (`DataPatcher.fixContentArrays`). The owner
## fixes arrays on every action; the port palette is rebuilt per query
## (`MindEditor.palette_blocks`), so no explicit selection rebuild is needed.
func hidden() -> void:
	pass


## Pulls the current record set from the owner (`state.data`).
func refetch() -> void:
	_rows = []
	if _editor != null and _editor.has_method("assets"):
		var value: Variant = _editor.call("assets")
		if value is Array:
			_rows = value


func _build() -> void:
	clear_content()
	if cont == null:
		return
	var root := content_table()

	# `cont.add(types).growX().left().row()`.
	var types := HBoxContainer.new()
	types.add_theme_constant_override("separation", 4)
	types.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	root.add(types).grow_x_axis()
	root.row()

	# `types.button(Icon.menu, Styles.graySquarei, this::showEditMenu)`.
	var menu := MindWidgets.icon_button("menu", "", "graySquarei")
	menu.custom_minimum_size = Vector2(50.0, 50.0)
	menu.tooltip_text = _t("@edit.menu")
	menu.pressed.connect(_show_edit_menu)
	types.add_child(menu)
	types.add_child(_divider())

	_type_buttons.clear()
	for type in TYPES:
		# `types.button(typeIcons[type.ordinal()], Styles.grayTogglei, ...)
		#   .checked(b -> currentType == type)`.
		var button := MindWidgets.glyph_button(str(TYPE_ICONS[type]), _t(str(TYPE_KEYS[type])),
			"grayTogglei")
		button.toggle_mode = true
		button.button_pressed = _type == type
		button.custom_minimum_size = Vector2(50.0, 50.0)
		button.tooltip_text = _t(str(TYPE_KEYS[type]))
		button.pressed.connect(_change_type.bind(type))
		types.add_child(button)
		_type_buttons[type] = button
	types.add_child(_divider())

	# `types.table(Styles.grayPanel, t -> t.label(() -> currentType.localized()))`.
	var panel := PanelContainer.new()
	panel.add_theme_stylebox_override("panel", MindThemeBuilder.flat(MindStyles.GRAY_PANEL))
	panel.custom_minimum_size.y = 50.0
	_type_label = MindWidgets.label(_t(str(TYPE_KEYS[_type])))
	_type_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	panel.add_child(_type_label)
	types.add_child(panel)

	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	types.add_child(spacer)

	# `types.button("@asset.guide", Icon.link, Styles.grayt, ...).size(200, 50)`.
	var guide := MindWidgets.icon_button("link", _t("@asset.guide"), "grayt")
	guide.custom_minimum_size = Vector2(200.0, 50.0)
	guide.pressed.connect(func() -> void: OS.shell_open(GUIDE_URL))
	types.add_child(guide)

	# `cont.table(search -> image + field).growX().row()`.
	var search := HBoxContainer.new()
	search.add_theme_constant_override("separation", 4)
	search.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	root.add(search).grow_x_axis()
	root.row()
	var zoom := MindWidgets.image("zoom")
	zoom.custom_minimum_size = Vector2(30.0, 30.0)
	zoom.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	search.add_child(zoom)
	_search_field = MindWidgets.field(_t("@search"))
	_search_field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_search_field.text = _search
	_search_field.text_changed.connect(_on_search)
	search.add_child(_search_field)

	# `cont.pane(t -> list = t).grow().top()`.
	# code-instantiated: the record list is data-driven (`rebuild`).
	var scroll := ScrollContainer.new()
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	# A nested pane inside the dialog body needs a minimum height or the outer
	# scroll collapses it (`cont.pane(...).grow()` in Java).
	scroll.custom_minimum_size = Vector2(0.0, 300.0)
	root.add(scroll).grow_x_axis().grow_y_axis()
	root.row()
	_list = VBoxContainer.new()
	_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_list.add_theme_constant_override("separation", 4)
	scroll.add_child(_list)

	_change_type(_type)


## `changeType(type)`: swap the action buttons, then `rebuild()`.
func _change_type(type: String) -> void:
	_type = type
	if _type_label != null:
		_type_label.text = _t(str(TYPE_KEYS.get(type, "")))
	for key in _type_buttons.keys():
		var button: Button = _type_buttons[key]
		button.button_pressed = key == type

	clear_buttons()
	match type:
		"patch":
			add_button(_t("@add"), _add_patch, "add", 190.0)
		"content":
			add_button(_t("@add"), _add_content, "add", 190.0)
		"bundle":
			add_button(_t("@add"), _add_bundle, "add", 190.0)
		"image":
			add_button(_t("@add"), _add_image, "add", 190.0)
			add_button(_t("@edit"), _show_edit_menu, "menu", 190.0)
		"sound", "music":
			add_button(_t("@add"), _add_audio.bind(type), "add", 190.0)
	add_close_button()

	# Switching to the content view reloads content + sprites (`changeType`).
	if type == "content" and _editor != null:
		_editor.call("data_reload_content", false)
		refetch()
	_rebuild()


## `rebuild()`: clear the pane and run the current `AssetView.build`.
func _rebuild() -> void:
	if _list == null:
		return
	for child in _list.get_children():
		_list.remove_child(child)
		child.queue_free()
	match _type:
		"patch":
			_build_patches()
		"content":
			_build_content()
		"bundle":
			_build_bundles()
		"image":
			_build_images()
		"sound", "music":
			_build_audio(_type)


func _on_search(text: String) -> void:
	# `t.length() > 0 ? t.toLowerCase() : null`.
	_search = text.to_lower() if text.length() > 0 else ""
	_rebuild()


# --- helpers ---------------------------------------------------------------


func _divider() -> ColorRect:
	# `types.image(Tex.whiteui, Pal.accent).size(4f, 50f)`.
	var line := ColorRect.new()
	line.color = MindStyles.ACCENT
	line.custom_minimum_size = Vector2(4.0, 50.0)
	return line


func _rows_of(kind: String) -> Array:
	var out: Array = []
	for value in _rows:
		if value is Dictionary and str((value as Dictionary).get("kind", "")) == kind:
			out.append(value)
	return out


func _matches(row: Dictionary, key: String) -> bool:
	if _search.is_empty():
		return true
	return str(row.get(key, "")).to_lower().contains(_search)


func _row_width() -> float:
	return ROW_WIDTH_MOBILE if _portrait() else ROW_WIDTH


func _portrait() -> bool:
	var viewport_size := get_viewport_rect().size
	return viewport_size.y > viewport_size.x


## A 50×50 icon button (`list.button(Icon.x, Styles.graySquarei, iconMed, ...)`).
func _square(icon_name: String, tooltip: String, handler: Callable) -> Button:
	var button := MindWidgets.icon_button(icon_name, "", "graySquarei")
	button.custom_minimum_size = Vector2(ROW_HEIGHT, ROW_HEIGHT)
	button.tooltip_text = tooltip
	button.pressed.connect(handler)
	return button


## Empty cell for Java's `list.add().size(h)` warning placeholder.
func _empty_cell() -> Control:
	var cell := Control.new()
	cell.custom_minimum_size = Vector2(ROW_HEIGHT, ROW_HEIGHT)
	return cell


## The two-line entry button (`[accent]name\n[lightgray]sub`).
func _entry_button(row: Dictionary, title: String, subtitle: String, width: float,
		handler: Callable) -> Button:
	var button := MindWidgets.button("")
	button.theme_type_variation = "grayt"
	button.custom_minimum_size = Vector2(width, ROW_HEIGHT)
	button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var markup := "[accent]%s" % title
	if not subtitle.is_empty():
		markup += "\n[lightgray]%s" % subtitle
	var label := MindWidgets.label(MindWidgets.markup_text(markup))
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_LEFT
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	label.offset_left = 10.0
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	button.add_child(label)
	button.tooltip_text = str(row.get("path", ""))
	button.pressed.connect(handler)
	return button


func _show_text(title: String, text_body: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_text"):
		ui.call("show_text", title, text_body)
	else:
		show_toast(text_body)


func _error(message: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_text"):
		ui.call("show_text", _t("@error.title"), message)
	else:
		push_warning(message)


## `ui.showConfirm` + one pending handler (`confirm_result`).
func _confirm(message: String, handler: Callable) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("show_confirm"):
		handler.call()
		return
	_pending_confirm = handler
	ui.call("show_confirm", message)


func _on_confirm(confirmed: bool) -> void:
	var handler := _pending_confirm
	_pending_confirm = Callable()
	if confirmed and handler.is_valid():
		handler.call()


## Owner error codes -> bundle keys (`DataManager` dialog messages).
const ERROR_KEYS := {
	"exists": "@asset.content.exists",
	"badname": "@asset.content.badname",
	"badtype": "@asset.content.type",
	"invalid JSON": "@patch.importerror",
}


## Runs an owner action; on failure shows its message, on success refetches.
func _apply(result: Variant, success: Callable = Callable()) -> void:
	var value: Dictionary = result if result is Dictionary else {}
	if not bool(value.get("ok", false)):
		var message := str(value.get("error", ""))
		if ERROR_KEYS.has(message):
			message = _t(str(ERROR_KEYS[message]))
		_error(message)
		return
	refetch()
	if success.is_valid():
		success.call()
	_rebuild()


func _open_file(label: String, filters: PackedStringArray, multiple: bool,
		handler: Callable) -> void:
	# code-instantiated: `FileChooser.open` is a runtime native dialog.
	var dialog := FileDialog.new()
	dialog.title = label
	dialog.file_mode = (
		FileDialog.FILE_MODE_OPEN_FILES if multiple else FileDialog.FILE_MODE_OPEN_FILE
	)
	dialog.access = FileDialog.ACCESS_FILESYSTEM
	for filter in filters:
		dialog.add_filter(filter, "Files")
	dialog.file_selected.connect(func(path: String) -> void:
		dialog.queue_free()
		handler.call([path]))
	dialog.files_selected.connect(func(paths: PackedStringArray) -> void:
		dialog.queue_free()
		handler.call(_to_array(paths)))
	dialog.canceled.connect(dialog.queue_free)
	add_child(dialog)
	dialog.popup_centered_ratio(0.6)


func _save_file(label: String, file_name: String, filter: String, handler: Callable) -> void:
	# code-instantiated: `FileChooser.export` is a runtime native dialog.
	var dialog := FileDialog.new()
	dialog.title = label
	dialog.file_mode = FileDialog.FILE_MODE_SAVE_FILE
	dialog.access = FileDialog.ACCESS_FILESYSTEM
	dialog.current_file = file_name
	dialog.add_filter(filter, "Files")
	dialog.file_selected.connect(func(path: String) -> void:
		dialog.queue_free()
		handler.call(path))
	dialog.canceled.connect(dialog.queue_free)
	add_child(dialog)
	dialog.popup_centered_ratio(0.6)


## `showImport(handler)`: clipboard + file choices (`MapPatchesView`).
func _import_choices(handler: Callable, filters: PackedStringArray) -> void:
	# code-instantiated: Java's `BaseDialog("@editor.import")` choice menu.
	var dialog := AcceptDialog.new()
	dialog.title = _t("@editor.import")
	var box := VBoxContainer.new()
	box.add_theme_constant_override("separation", 6)
	box.custom_minimum_size = Vector2(300.0, 120.0)
	var clipboard := MindWidgets.icon_button("copy", _t("@load.clipboard"))
	clipboard.disabled = DisplayServer.clipboard_get().is_empty()
	clipboard.pressed.connect(func() -> void:
		var text := DisplayServer.clipboard_get()
		dialog.queue_free()
		if not text.is_empty():
			handler.call(text))
	box.add_child(clipboard)
	var file := MindWidgets.icon_button("download", _t("@import.file"))
	file.pressed.connect(func() -> void:
		dialog.queue_free()
		_open_file(_t("@import.file"), filters, false,
			func(paths: Array) -> void: handler.call(_read_text(str(paths[0])))))
	box.add_child(file)
	dialog.add_child(box)
	dialog.get_ok_button().visible = false
	add_child(dialog)
	dialog.popup_centered(Vector2i(340, 180))


func _read_text(path: String) -> String:
	return FileAccess.get_file_as_string(path)


func _to_array(values: PackedStringArray) -> Array:
	var out: Array = []
	for value in values:
		out.append(value)
	return out


## `MapAssetsDialog.showEditMenu` (`@asset.importzip`/`exportzip`/`clearall`).
func _show_edit_menu() -> void:
	# code-instantiated: Java builds this `BaseDialog("@edit.menu")` per open.
	var dialog := AcceptDialog.new()
	dialog.title = _t("@edit.menu")
	var box := VBoxContainer.new()
	box.add_theme_constant_override("separation", 6)
	box.custom_minimum_size = Vector2(300.0, 170.0)
	var import_zip := MindWidgets.icon_button("download", _t("@asset.importzip"))
	import_zip.pressed.connect(func() -> void:
		dialog.queue_free()
		_open_file(_t("@asset.importzip"), PackedStringArray(["*.zip"]), false,
			func(paths: Array) -> void:
				_apply(_editor.call("data_import_zip", str(paths[0])))))
	box.add_child(import_zip)
	var export_zip := MindWidgets.icon_button("upload", _t("@asset.exportzip"))
	export_zip.pressed.connect(func() -> void:
		dialog.queue_free()
		_save_file(_t("@asset.exportzip"), "assets.zip", "*.zip",
			func(path: String) -> void:
				if not bool(_editor.call("data_export_zip", path)):
					_error(_t("@asset.exportzip"))))
	box.add_child(export_zip)
	var clear_all := MindWidgets.icon_button("trash", _t("@asset.clearall"))
	clear_all.pressed.connect(func() -> void:
		dialog.queue_free()
		_confirm(_t("@asset.clearall.confirm"), func() -> void:
			_apply(_editor.call("data_clear"))))
	box.add_child(clear_all)
	dialog.add_child(box)
	dialog.get_ok_button().visible = false
	add_child(dialog)
	dialog.popup_centered(Vector2i(340, 230))


# --- MapPatchesView --------------------------------------------------------


func _build_patches() -> void:
	var rows := _rows_of("patch")
	var shown_count := 0
	for value in rows:
		var row: Dictionary = value
		if not _matches(row, "name"):
			continue
		shown_count += 1
		_list.add_child(_patch_row(row))
	if shown_count == 0:
		_list.add_child(MindWidgets.label(MindWidgets.markup("@patch.none")))


func _patch_row(row: Dictionary) -> Control:
	var index := int(row.get("index", 0))
	var patch_name := str(row.get("name", ""))
	var json := str(row.get("data", ""))
	var label := "<unnamed>" if patch_name.is_empty() else patch_name
	var fields := _count_fields(_parse_json(json))
	var box := HBoxContainer.new()
	box.add_theme_constant_override("separation", 4)
	box.alignment = BoxContainer.ALIGNMENT_CENTER
	box.add_child(_empty_cell())
	box.add_child(_entry_button(row, label, _t("@patch.fields").format([fields]),
		_row_width(), func() -> void:
			_show_text(_t("@patch.set").format([label]), json)))
	box.add_child(_square("copy", _t("@copy.clipboard"), func() -> void:
		DisplayServer.clipboard_set(json)
		show_toast(_t("@copied"))))
	box.add_child(_square("refresh", _t("@editor.import"), func() -> void:
		_import_choices(func(text: String) -> void:
			_apply(_editor.call("data_patch_replace", index, text)), _text_filters())))
	box.add_child(_square("trash", _t("@patch.delete.confirm"), func() -> void:
		_confirm(_t("@patch.delete.confirm"), func() -> void:
			_apply(_editor.call("data_patch_remove", index)))))
	return box


func _add_patch() -> void:
	_import_choices(func(text: String) -> void:
		_apply(_editor.call("data_patch_add", text)), _text_filters())


func _text_filters() -> PackedStringArray:
	return PackedStringArray(["*.json", "*.hjson", "*.json5"])


## `MapPatchesView.countFields`: leaf count of the patch JSON, at least 1.
func _count_fields(value: Variant) -> int:
	if value is Dictionary or value is Array:
		var sum := 0
		var children: Array = value.values() if value is Dictionary else value
		for child in children:
			sum += _count_fields(child)
		return maxi(sum, 1)
	return 1


func _parse_json(text: String) -> Variant:
	var parsed: Variant = JSON.parse_string(text)
	return parsed if parsed != null else {}


# --- MapContentView --------------------------------------------------------


func _build_content() -> void:
	var rows := _rows_of("content")
	var shown_count := 0
	var last_type := ""
	for value in rows:
		var row: Dictionary = value
		if not _matches(row, "name"):
			continue
		shown_count += 1
		var content_type := str(row.get("contentType", ""))
		if content_type != last_type:
			last_type = content_type
			_list.add_child(_content_header(content_type))
		_list.add_child(_content_row(row, content_type))
	if shown_count == 0:
		_list.add_child(MindWidgets.label(MindWidgets.markup("@none.found")))


func _content_header(content_type: String) -> Control:
	# `list.table(t -> icon + "@content.<type>" + whiteui rule).colspan(6)`.
	var box := HBoxContainer.new()
	box.add_theme_constant_override("separation", 8)
	box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var glyph := MindWidgets.glyph(str(CONTENT_ICONS.get(content_type, "box")), 22,
		Color.LIGHT_GRAY)
	box.add_child(glyph)
	var label := MindWidgets.label(_t("@content.%s" % content_type))
	label.add_theme_color_override("font_color", Color.LIGHT_GRAY)
	box.add_child(label)
	# code-instantiated: the rule under the type header (Java `image().growX()`).
	var rule := ColorRect.new()
	rule.color = Color.LIGHT_GRAY
	rule.custom_minimum_size = Vector2(0.0, 4.0)
	rule.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	box.add_child(rule)
	return box


func _content_row(row: Dictionary, content_type: String) -> Control:
	var content_name := str(row.get("name", ""))
	var json := str(row.get("data", ""))
	var box := HBoxContainer.new()
	box.add_theme_constant_override("separation", 4)
	box.alignment = BoxContainer.ALIGNMENT_CENTER
	box.add_child(_empty_cell())

	# `content.content instanceof UnlockableContent ? uiIcon : contentIcons.get(type)`.
	var icon := MindWidgets.icon_button(str(CONTENT_ICONS.get(content_type, "box")), "",
		"graySquarei")
	icon.custom_minimum_size = Vector2(ROW_HEIGHT, ROW_HEIGHT)
	icon.disabled = true
	box.add_child(icon)

	box.add_child(_entry_button(row, content_name, _t("@content.%s" % content_type), _row_width(),
		func() -> void: _show_text(content_name, json)))
	box.add_child(_square("refresh", _t("@editor.import"), func() -> void:
		_import_choices(func(text: String) -> void:
			# Java overwrites `content.data`; replace then re-add.
			var removed: Dictionary = _editor.call("data_asset_delete", "content", content_name)
			if not bool(removed.get("ok", false)):
				_error(str(removed.get("error", "")))
				return
			_apply(_editor.call("data_content_add", content_name, content_type, text)), _text_filters())))
	box.add_child(_square("export", _t("@export.file"), func() -> void:
		_save_file(_t("@editor.export"), content_name + ".json", "*.json",
			func(path: String) -> void: FileAccess.open(path, FileAccess.WRITE).store_string(json))))
	box.add_child(_square("trash", _t("@asset.delete.confirm"), func() -> void:
		_confirm(_t("@asset.delete.confirm"), func() -> void:
			_apply(_editor.call("data_asset_delete", "content", content_name)))))
	return box


## `MapContentView.showImport`: name + type row, then clipboard/file.
func _add_content() -> void:
	# code-instantiated: Java's `BaseDialog("@asset.content.add")` name/type form.
	var dialog := AcceptDialog.new()
	dialog.title = _t("@asset.content.add")
	var box := VBoxContainer.new()
	box.add_theme_constant_override("separation", 6)
	box.custom_minimum_size = Vector2(420.0, 190.0)
	var name_row := HBoxContainer.new()
	name_row.add_child(MindWidgets.label(_t("@name")))
	var name_field := MindWidgets.field("")
	name_field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	name_field.max_length = 64
	name_row.add_child(name_field)
	box.add_child(name_row)
	var type_row := HBoxContainer.new()
	type_row.add_child(MindWidgets.label(_t("@asset.content.type")))
	# GDScript lambdas capture locals by value: keep the live selection in a
	# dictionary the toggle lambdas mutate.
	var selection := {"type": "item"}
	var type_buttons: Array[Button] = []
	for content_type in CONTENT_TYPES:
		var button := MindWidgets.glyph_button(str(CONTENT_ICONS.get(content_type, "box")),
			_t("@content.%s" % content_type), "squareTogglei")
		button.toggle_mode = true
		button.button_pressed = content_type == selection["type"]
		button.custom_minimum_size = Vector2(50.0, 50.0)
		button.tooltip_text = _t("@content.%s" % content_type)
		button.pressed.connect(func() -> void:
			selection["type"] = content_type
			for other in type_buttons:
				other.button_pressed = other == button)
		type_buttons.append(button)
		type_row.add_child(button)
	box.add_child(type_row)
	dialog.add_child(box)
	dialog.get_ok_button().visible = false
	var actions := HBoxContainer.new()
	actions.add_theme_constant_override("separation", 6)
	var cancel := MindWidgets.icon_button("cancel", _t("@cancel"))
	cancel.pressed.connect(dialog.queue_free)
	actions.add_child(cancel)
	var import_file := MindWidgets.icon_button("file-text", _t("@asset.content.import.file"))
	import_file.pressed.connect(func() -> void:
		var chosen: String = selection["type"]
		dialog.queue_free()
		_open_file(_t("@asset.content.import.file"), _text_filters(), false,
			func(paths: Array) -> void:
				_apply(_editor.call("data_content_add", name_field.text, chosen,
					_read_text(str(paths[0]))))))
	actions.add_child(import_file)
	var import_clipboard := MindWidgets.icon_button("copy", _t("@asset.content.import.clipboard"))
	import_clipboard.pressed.connect(func() -> void:
		var chosen: String = selection["type"]
		dialog.queue_free()
		_apply(_editor.call("data_content_add", name_field.text, chosen,
			DisplayServer.clipboard_get())))
	actions.add_child(import_clipboard)
	dialog.add_child(actions)
	add_child(dialog)
	dialog.popup_centered(Vector2i(500, 260))


# --- MapBundlesView --------------------------------------------------------


func _build_bundles() -> void:
	var rows := _rows_of("bundle")
	var shown_count := 0
	for value in rows:
		var row: Dictionary = value
		if not _matches(row, "name"):
			continue
		shown_count += 1
		_list.add_child(_bundle_row(row))
	if shown_count == 0:
		_list.add_child(MindWidgets.label(MindWidgets.markup("@patch.none")))


func _bundle_row(row: Dictionary) -> Control:
	var bundle_name := str(row.get("name", ""))
	var path := str(row.get("path", ""))
	var text := str(row.get("data", ""))
	var lines := int(row.get("size", 0))
	var box := HBoxContainer.new()
	box.add_theme_constant_override("separation", 4)
	box.alignment = BoxContainer.ALIGNMENT_CENTER
	box.add_child(_empty_cell())
	box.add_child(_entry_button(row, bundle_name, _t("@bundle.lines").format([lines]),
		_row_width(), func() -> void: _show_text(bundle_name, text)))
	box.add_child(_square("refresh", _t("@editor.import"), func() -> void:
		_import_choices(func(updated: String) -> void:
			_apply(_editor.call("data_bundle_set", path, updated)),
			PackedStringArray(["*.properties", "*.txt"]))))
	box.add_child(_square("export", _t("@export.file"), func() -> void:
		_save_file(_t("@editor.export"), bundle_name, "*.properties",
			func(target: String) -> void: FileAccess.open(target, FileAccess.WRITE).store_string(text))))
	box.add_child(_square("trash", _t("@asset.delete.confirm"), func() -> void:
		_confirm(_t("@asset.delete.confirm"), func() -> void:
			_apply(_editor.call("data_asset_delete", "bundle", path)))))
	return box


func _add_bundle() -> void:
	_open_file(_t("@add"), PackedStringArray(["*.properties", "*.txt"]), true,
		func(paths: Array) -> void:
			for path in paths:
				var result: Dictionary = _editor.call("data_file_add", "bundle", str(path))
				if not bool(result.get("ok", false)):
					_error(str(result.get("error", "")))
			refetch()
			_rebuild())


# --- MapImagesView ---------------------------------------------------------


func _build_images() -> void:
	# code-instantiated: the image grid is record-driven; columns follow Java.
	var columns := maxi(1, int((get_viewport_rect().size.x - 20.0) * 0.9 / (IMAGE_CELL + 14.0)))
	var grid := GridContainer.new()
	grid.columns = columns
	grid.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	grid.add_theme_constant_override("h_separation", 4)
	grid.add_theme_constant_override("v_separation", 4)
	_list.add_child(grid)
	var count := 0
	for value in _rows_of("image"):
		var row: Dictionary = value
		if bool(row.get("generated", false)):
			continue
		if not _matches(row, "path"):
			continue
		count += 1
		grid.add_child(_image_cell(row))
	if count == 0:
		var empty := MindWidgets.label(MindWidgets.markup("@none.found"))
		empty.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		_list.add_child(empty)


func _image_cell(row: Dictionary) -> Control:
	var image_name := str(row.get("name", ""))
	var path := str(row.get("path", ""))
	var region := str(row.get("region", ""))
	var found := bool(row.get("found", false))
	var width := int(row.get("width", 1))
	var height := int(row.get("height", 1))
	var file_size := int(row.get("size", 0))

	# code-instantiated: one `Styles.grayPanel` card per image record.
	var panel := PanelContainer.new()
	panel.add_theme_stylebox_override("panel", MindThemeBuilder.flat(MindStyles.GRAY_PANEL))
	panel.custom_minimum_size = Vector2(IMAGE_CELL, IMAGE_CELL)
	var box := VBoxContainer.new()
	box.add_theme_constant_override("separation", 2)
	panel.add_child(box)

	# `t.add(new BorderImage(region, 4f)).scaling(Scaling.fit).size(size - 10f)`.
	if found:
		# code-instantiated: the preview texture is a runtime `dp-` region.
		var preview := TextureRect.new()
		preview.custom_minimum_size = Vector2(IMAGE_CELL - 10.0, IMAGE_CELL - 60.0)
		preview.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		preview.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		var assets := MindWidgets.assets()
		if assets != null:
			var texture: Variant = assets.call("find_region", region)
			if texture is Texture2D:
				preview.texture = texture
		box.add_child(preview)
	else:
		# `Core.bundle.get("asset.image.packfailed")` placeholder cell.
		var missing := MindWidgets.label("⚠")
		missing.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		missing.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
		missing.custom_minimum_size = Vector2(IMAGE_CELL - 10.0, IMAGE_CELL - 60.0)
		missing.tooltip_text = _t("@asset.image.packfailed")
		box.add_child(missing)

	var label := MindWidgets.label(("" if found else "[red]⚠[] ") + image_name)
	label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	label.tooltip_text = "%s\n%s" % [region, path]
	label.mouse_filter = Control.MOUSE_FILTER_PASS
	label.gui_input.connect(func(event: InputEvent) -> void:
		if event is InputEventMouseButton and event.pressed:
			_show_image_info(image_name, path, region, found, width, height, file_size))
	box.add_child(label)
	var dims := MindWidgets.label("%dx%d" % [width, height])
	dims.add_theme_color_override("font_color", Color.LIGHT_GRAY)
	box.add_child(dims)

	var actions := HBoxContainer.new()
	actions.add_theme_constant_override("separation", 0)
	var cell := (IMAGE_CELL - 10.0) / 4.0
	var pencil := MindWidgets.icon_button("pencil", "", "emptyi")
	pencil.custom_minimum_size = Vector2(cell, cell)
	pencil.tooltip_text = _t("@save.rename")
	pencil.pressed.connect(func() -> void: _rename_image(path))
	actions.add_child(pencil)
	var info := MindWidgets.icon_button("info", "", "emptyi")
	info.custom_minimum_size = Vector2(cell, cell)
	info.tooltip_text = _t("@asset.image.info")
	info.pressed.connect(func() -> void:
		_show_image_info(image_name, path, region, found, width, height, file_size))
	actions.add_child(info)
	var export := MindWidgets.icon_button("export", "", "emptyi")
	export.custom_minimum_size = Vector2(cell, cell)
	export.tooltip_text = _t("@export.file")
	export.pressed.connect(func() -> void:
		_save_file(_t("@editor.export"), image_name + ".png", "*.png",
			func(target: String) -> void:
				if not bool(_editor.call("data_asset_export", "image", path, target)):
					_error(_t("@asset.broken"))))
	actions.add_child(export)
	var trash := MindWidgets.icon_button("trash", "", "emptyi")
	trash.custom_minimum_size = Vector2(cell, cell)
	trash.tooltip_text = _t("@asset.image.delete.confirm")
	trash.pressed.connect(func() -> void:
		_confirm(_t("@asset.image.delete.confirm"), func() -> void:
			_apply(_editor.call("data_asset_delete", "image", image_name))))
	actions.add_child(trash)
	box.add_child(actions)
	return panel


func _show_image_info(image_name: String, path: String, region: String, found: bool,
		width: int, height: int, file_size: int) -> void:
	var env := path.contains("blocks/environment/")
	var lines := PackedStringArray()
	if not found:
		lines.append(_t("@asset.image.packfailed"))
		if env:
			lines.append(_t("@asset.image.packfailed.env"))
	lines.append(_t("@asset.image.name").format([image_name]))
	lines.append(_t("@asset.image.region.name").format([region]))
	lines.append(_t("@asset.image.path").format([path]))
	lines.append(_t("@asset.image.env").format([str(env)]))
	lines.append(_t("@asset.image.size").format([width, height]))
	lines.append(_t("@asset.image.filesize").format([file_size]))
	_show_text(_t("@asset.image.info"), "\n".join(lines))


func _add_image() -> void:
	_open_file(_t("@add"), PackedStringArray(["*.png", "*.jpg", "*.jpeg"]), true,
		func(paths: Array) -> void:
			for path in paths:
				var result: Dictionary = _editor.call("data_file_add", "image", str(path))
				if not bool(result.get("ok", false)):
					_error(str(result.get("error", "")))
			refetch()
			_rebuild())


## `MapImagesView` pencil (`ui.showTextInput("@save.rename", "@patch.path", ...)`).
func _rename_image(path: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("show_text_input"):
		return
	if ui.has_signal("text_input_result") \
			and not ui.is_connected("text_input_result", _on_image_rename):
		ui.connect("text_input_result", _on_image_rename)
	_renaming_image = path
	ui.call("show_text_input", _t("@save.rename"), _t("@patch.path"), 1000, path, false, false)



func _on_image_rename(text: String) -> void:
	var old := _renaming_image
	_renaming_image = ""
	if old.is_empty() or text.is_empty() or text == old:
		return
	_apply(_editor.call("data_image_rename", old, text))


# --- MapAudioView ----------------------------------------------------------


func _build_audio(kind: String) -> void:
	var rows := _rows_of(kind)
	var shown_count := 0
	for value in rows:
		var row: Dictionary = value
		if not _matches(row, "name"):
			continue
		shown_count += 1
		_list.add_child(_audio_row(row, kind))
	if shown_count == 0:
		_list.add_child(MindWidgets.label(MindWidgets.markup("@none.found")))


func _audio_row(row: Dictionary, kind: String) -> Control:
	var audio_name := str(row.get("name", ""))
	var path := str(row.get("path", ""))
	var music := kind == "music"
	var stream_name := _audio_stream_name(row, music)
	var box := HBoxContainer.new()
	box.add_theme_constant_override("separation", 4)
	box.alignment = BoxContainer.ALIGNMENT_CENTER

	var play := MindWidgets.icon_button(
		"pause" if _playing_audio == stream_name else "play", "", "graySquarei")
	play.custom_minimum_size = Vector2(ROW_HEIGHT, ROW_HEIGHT)
	play.tooltip_text = _t("@play")
	play.pressed.connect(func() -> void: _play_audio(stream_name, music, play))
	box.add_child(play)

	# `in.add("[accent]" + asset.name)` + `UI.formatTime(audioSource.getLength()*60)`.
	var panel := PanelContainer.new()
	panel.add_theme_stylebox_override("panel", MindThemeBuilder.flat(MindStyles.GRAY_PANEL))
	panel.custom_minimum_size = Vector2(_row_width(), ROW_HEIGHT)
	panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var lines := VBoxContainer.new()
	lines.add_theme_constant_override("separation", 0)
	var title := MindWidgets.label(MindWidgets.markup_text("[accent]%s" % audio_name))
	title.horizontal_alignment = HORIZONTAL_ALIGNMENT_LEFT
	lines.add_child(title)
	var seconds := _audio_length(stream_name, music)
	var length := MindWidgets.label("[lightgray][[%s]" % _format_seconds(seconds))
	length.add_theme_color_override("font_color", Color.LIGHT_GRAY)
	lines.add_child(length)
	panel.add_child(lines)
	box.add_child(panel)

	box.add_child(_square("export", _t("@export.file"), func() -> void:
		var extension := "*.ogg" if path.to_lower().ends_with(".ogg") else "*.mp3"
		_save_file(_t("@editor.export"), audio_name, extension,
			func(target: String) -> void:
				if not bool(_editor.call("data_asset_export", kind, path, target)):
					_error(_t("@asset.broken")))))
	box.add_child(_square("trash", _t("@asset.delete.confirm"), func() -> void:
		_confirm(_t("@asset.delete.confirm"), func() -> void:
			_apply(_editor.call("data_asset_delete", kind, audio_name)))))
	return box


## `DataAudioLoader.prefix + asset.name.replace(' ', '_')` (`dp-`).
func _audio_stream_name(row: Dictionary, _music: bool) -> String:
	var asset_name := str(row.get("name", "")).replace(" ", "_")
	return "dp-" + asset_name.to_lower()


func _audio_length(stream_name: String, music: bool) -> float:
	var assets := MindWidgets.assets()
	if assets == null:
		return 0.0
	var method := "music_stream" if music else "sound_stream"
	var stream: Variant = assets.call(method, stream_name)
	if stream is AudioStream:
		return (stream as AudioStream).get_length()
	return 0.0


func _format_seconds(seconds: float) -> String:
	var total := int(seconds)
	@warning_ignore("integer_division")
	return "%d:%02d" % [total / 60, total % 60]


func _play_audio(stream_name: String, music: bool, button: Button) -> void:
	var audio := get_node_or_null("/root/Spine/MindAudio")
	if audio == null or not audio.has_method("play_data_audio"):
		return
	if bool(audio.call("play_data_audio", stream_name, music)):
		_playing_audio = stream_name
		if button != null:
			button.icon = MindWidgets.icon_texture("pause")


func _add_audio(kind: String) -> void:
	_open_file(_t("@add"), PackedStringArray(["*.ogg", "*.mp3"]), true,
		func(paths: Array) -> void:
			for path in paths:
				var result: Dictionary = _editor.call("data_file_add", kind, str(path))
				if not bool(result.get("ok", false)):
					_error(str(result.get("error", "")))
			refetch()
			_rebuild())
