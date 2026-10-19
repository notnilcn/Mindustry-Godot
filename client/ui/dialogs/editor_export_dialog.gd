## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/MapEditorDialog.java (`createDialog(
## "@editor.export", ...)`, `:139-148`).
##
## UI-only: the two export routes — the `.msav` (`MapIO.writeMap`) and a PNG
## (`MapIO.writeImage`). The fallback `file_chooser` emits `file_chosen`
## one-shot (no fresh-name commit; the native path is plan 22/I07).

extends MindDialog

## `createDialog`: every entry is a 360x90 left-aligned icon button.
const BUTTON_SIZE := Vector2(360.0, 90.0)

var _editor: Node = null
## Pending `file_chosen` route (`exportfile` / `exportimage`).
var _chooser_action := ""


func _ready() -> void:
	set_title_key("@editor.export")
	should_pause = false
	super._ready()
	_editor = get_node_or_null("/root/Spine/MindEditor")
	add_close_button()


func shown() -> void:
	_build()


func _build() -> void:
	# A reused dialog node: drop the previous open's body before rebuilding
	# (`shown()` rebuilds on every open; L01 T1).
	clear_content()
	var root := content_table()
	_add_entry(root, "@editor.exportfile", "@editor.exportfile.description", "file-text",
		func() -> void: _open_chooser("exportfile", "msav"))
	_add_entry(root, "@editor.exportimage", "@editor.exportimage.description", "image",
		func() -> void: _open_chooser("exportimage", "png"))


## One `createDialog` row: icon + wrapped name + grey description.
func _add_entry(root: MindTable, name_key: String, desc_key: String, icon: String, handler: Callable) -> void:
	# code-instantiated: the export entries are data-driven (two fixed rows).
	var button := MindWidgets.button("")
	button.custom_minimum_size = BUTTON_SIZE
	button.pressed.connect(handler)
	var row := HBoxContainer.new()
	row.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	row.add_theme_constant_override("separation", 10)
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	button.add_child(row)
	var glyph := MindWidgets.glyph(icon, 26)
	glyph.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	glyph.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(glyph)
	var text := VBoxContainer.new()
	text.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	text.alignment = BoxContainer.ALIGNMENT_CENTER
	text.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(text)
	var name_label := MindWidgets.label(_t(name_key))
	name_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	text.add_child(name_label)
	var description := MindWidgets.label(_t(desc_key))
	description.add_theme_color_override("default_color", MindStyles.ARC_GRAY)
	description.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	description.mouse_filter = Control.MOUSE_FILTER_IGNORE
	text.add_child(description)
	root.add(button).width(BUTTON_SIZE.x).height(BUTTON_SIZE.y).pad(2)
	root.row()


## `FileChooser.export(editor.tags.get("name"), extension, cb)`.
func _open_chooser(action: String, extension: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("open_dialog"):
		return
	_chooser_action = action
	_connect_chooser(true)
	ui.call("open_dialog", "file_chooser", JSON.stringify({"open": false, "extensions": [extension]}))


func _connect_chooser(should_connect: bool) -> void:
	var chooser := get_node_or_null("/root/Spine/Ui/UiRoot/DialogLayer/file_chooser")
	if chooser == null or not chooser.has_signal("file_chosen"):
		return
	if should_connect and not chooser.is_connected("file_chosen", _on_file_chosen):
		chooser.connect("file_chosen", _on_file_chosen)
	elif not should_connect and chooser.is_connected("file_chosen", _on_file_chosen):
		chooser.disconnect("file_chosen", _on_file_chosen)


func _on_file_chosen(path: String) -> void:
	var action := _chooser_action
	_chooser_action = ""
	_connect_chooser(false)
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("close_dialog"):
		ui.call("close_dialog", "file_chooser")
	if _editor == null:
		return
	var ok := false
	match action:
		"exportfile":
			ok = bool(_editor.call("export_map", path))
		"exportimage":
			ok = bool(_editor.call("export_image", path))
	if action.is_empty():
		return
	if not ok:
		_notify(_t("@editor.errorsave"))
		return
	_close_pressed()


## `ui.showErrorMessage` via the shared `show_text` prompt (`@error.title`).
func _notify(text: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_text"):
		ui.call("show_text", MindWidgets.markup_text(_t("@error.title")), MindWidgets.markup_text(text))
