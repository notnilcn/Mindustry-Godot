## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/EditorMapsDialog.java (plan 14 M5).
##
## Editor map browser (`EditorMapsDialog extends MapListDialog`, `displayType = true`;
## grid, filters and ordering in `map_list_dialog.gd`): `@back`,
## `@editor.newmap` + `@editor.importmap` in the button row, a search row over
## the T5 preview grid, and per-map clicks opening `editor_map` (preview +
## name/author/description + open/delete).

extends "res://ui/dialogs/map_list_dialog.gd"

## `MapEditorDialog`/`ui.editor.show()` default size for `New Map`.
const NEW_MAP_SIZE := 200

## Pending `file_chosen` route (`import`); empty when none is active.
var _chooser_action := ""
## A `file_chosen` handler is connected.
var _chooser_connected := false
## A New Map text prompt is awaiting an answer.
var _pending_new_map := false
## A pending import path awaiting the overwrite confirm.
var _pending_import := ""


func _ready() -> void:
	set_title_key("@maps")
	display_type = true
	super._ready()
	_connect_prompts()


## `buildButtons`: `@editor.newmap` (`Icon.add`) then `@editor.importmap`
## (`Icon.upload`), both 210x64, after the shared `@back`.
func _build_buttons() -> void:
	add_pane_button(_t("@editor.newmap"), "add", _new_map)
	add_pane_button(_t("@editor.importmap"), "upload", _open_import)


func _show_map(map: Dictionary) -> void:
	_open_map(str(map.get("name", "")))


## `EditorMapsDialog.showMap`: the per-map dialog (preview + actions).
func _open_map(map_name: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("open_dialog"):
		ui.call("open_dialog", "editor_map", JSON.stringify({"map": map_name}))


## `buildButtons`: `ui.showTextInput("@editor.newmap", "@editor.mapname", "", ...)`;
## a taken name shows `@editor.exists`, else `ui.editor.show()` on a fresh
## 200x200 map with the `name` tag (`EditorMapsDialog.java:27-42`).
func _new_map() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("show_text_input"):
		return
	_pending_new_map = true
	ui.call("show_text_input", _t("@editor.newmap"), _t("@editor.mapname"), 50, "", false, false)


func _on_text_input(text: String) -> void:
	if not _pending_new_map:
		return
	_pending_new_map = false
	var name := text.strip_edges()
	if name.is_empty():
		return
	if not _row_for(name).is_empty():
		_notify(_t("@editor.exists"))
		return
	var editor := get_node_or_null("/root/Spine/MindEditor")
	if editor == null or not bool(editor.call("begin_new", NEW_MAP_SIZE, NEW_MAP_SIZE)):
		_notify("@editor.errorload")
		return
	editor.call("set_tag", "name", name)
	_close_pressed()
	var shell := get_node_or_null("/root/Spine/Ui/EditorDialog")
	if shell != null and shell.has_method("show_dialog"):
		shell.call("show_dialog")


## `buildButtons`: `@editor.importmap` -> `FileChooser.open(mapExtension)`.
func _open_import() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("open_dialog"):
		return
	_chooser_action = "import"
	_connect_chooser(true)
	ui.call("open_dialog", "file_chooser", JSON.stringify({"open": true, "extensions": ["msav"]}))


func _connect_chooser(connect: bool) -> void:
	var chooser := get_node_or_null("/root/Spine/Ui/UiRoot/DialogLayer/file_chooser")
	if chooser == null or not chooser.has_signal("file_chosen"):
		return
	if connect and not chooser.is_connected("file_chosen", _on_file_chosen):
		chooser.connect("file_chosen", _on_file_chosen)
		_chooser_connected = true
	elif not connect and chooser.is_connected("file_chosen", _on_file_chosen):
		chooser.disconnect("file_chosen", _on_file_chosen)
		_chooser_connected = false


## `EditorMapsDialog.tryImportMap` (`:55-98`): image -> `@editor.errorimage`;
## a built-in name -> `editor.import.exists`; a custom conflict -> the
## `editor.overwrite.confirm` confirm, then re-import with `overwrite`.
func _on_file_chosen(path: String) -> void:
	if _chooser_action.is_empty():
		return
	_chooser_action = ""
	_connect_chooser(false)
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("close_dialog"):
		ui.call("close_dialog", "file_chooser")
	_pending_import = path
	_import(false)


func _import(overwrite: bool) -> void:
	var editor := get_node_or_null("/root/Spine/MindEditor")
	if editor == null:
		_pending_import = ""
		return
	var result := str(editor.call("import_map", _pending_import, overwrite))
	if result.is_empty():
		_pending_import = ""
		_refresh_registry()
		_rebuild()
		return
	if result.begins_with("overwrite:"):
		_confirm_overwrite(result.trim_prefix("overwrite:"))
		return
	_pending_import = ""
	if result.begins_with("exists:"):
		_notify(_fmt("@editor.import.exists", [result.trim_prefix("exists:")]))
	elif result == "image":
		_notify("@editor.errorimage")
	else:
		_notify("@editor.errorload")


func _confirm_overwrite(name: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_confirm"):
		ui.call("show_confirm", MindWidgets.markup_text(_fmt("@editor.overwrite.confirm", [name])))


func _on_confirm(confirmed: bool) -> void:
	if _pending_import.is_empty():
		return
	if not confirmed:
		_pending_import = ""
		return
	_import(true)


## `showMap`'s `@editor.openin`: load the map into the editor and show the shell.
func _open_in_editor(path: String) -> void:
	if path.is_empty():
		_notify("@editor.errorload")
		return
	var editor := get_node_or_null("/root/Spine/MindEditor")
	if editor == null or not bool(editor.call("begin_edit_map", path)):
		_notify("@editor.errorload")
		return
	_close_pressed()
	var shell := get_node_or_null("/root/Spine/Ui/EditorDialog")
	if shell != null and shell.has_method("show_dialog"):
		shell.call("show_dialog")


func _row_for(name: String) -> Dictionary:
	for row_variant in _maps():
		var row: Dictionary = row_variant
		if str(row.get("name", "")) == name:
			return row
	return {}


func _connect_prompts() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	if ui.has_signal("text_input_result") and not ui.is_connected("text_input_result", _on_text_input):
		ui.connect("text_input_result", _on_text_input)
	if ui.has_signal("confirm_result") and not ui.is_connected("confirm_result", _on_confirm):
		ui.connect("confirm_result", _on_confirm)


## Bundle lookup with `{0}`-style substitution (`Bundle.format`).
func _fmt(key: String, args: Array) -> String:
	var result := _t(key)
	for index in args.size():
		result = result.replace("{%d}" % index, str(args[index]))
	return result


func _notify(key: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_info"):
		ui.call("show_info", _t(key))
