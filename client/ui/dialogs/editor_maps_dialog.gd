## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/EditorMapsDialog.java (plan 14 M5).
##
## Editor map browser: the live `MindPreview` map registry plus the read-model
## built-ins, with import/export routed through the file chooser and row clicks
## opening the statically-instanced map editor (`ui.editor.show()` upstream).
## The editor model is plan 19; this shell only lists maps and forwards intents.

extends MindDialog

## Emitted for an editor map action (`import`/`export`/`open`).
signal editor_map_action(action: String, name: String)

var _list: MindTable = null
## Pending file-chooser request (`import`/`export`); empty when none is active.
var _chooser_action := ""


func _ready() -> void:
	set_title_key("@editor.maps")
	should_pause = false
	full_dialog = true
	super._ready()
	_build()
	add_close_button()
	_connect_file_chooser()


func _build() -> void:
	var root := content_table()
	var actions := HBoxContainer.new()
	# code-instantiated: the action row is a fixed editor action set.
	var import_button := MindWidgets.button(_t("@editor.import"))
	import_button.pressed.connect(_open_import)
	actions.add_child(import_button)
	var export_button := MindWidgets.button(_t("@editor.export"))
	export_button.pressed.connect(_open_export)
	actions.add_child(export_button)
	root.add(actions).grow_x_axis().pad(4)
	root.row()
	_list = MindTable.new()
	root.add(_list).grow_x_axis().grow_y_axis()
	_rebuild()


## `MapListDialog.shown` rebuilds the grid so refreshed maps appear. A reveal
## after the file chooser means the chooser closed without a pick (cancel).
func shown() -> void:
	_chooser_action = ""
	_refresh_registry()
	_rebuild()


func _refresh_registry() -> void:
	var preview := get_node_or_null("/root/Spine/MindPreview")
	if preview != null and preview.has_method("refresh"):
		preview.call("refresh")


## The live registry rows (`MindPreview.maps_list`). The read-model built-ins
## are a fallback for an empty registry (missing assets/previews).
func _maps() -> Array:
	var rows: Array = []
	var preview := get_node_or_null("/root/Spine/MindPreview")
	if preview != null and preview.has_method("maps_list"):
		var maps: Variant = preview.call("maps_list")
		if maps is Array:
			for map_variant in maps:
				var map: Dictionary = map_variant
				if not str(map.get("name", "")).is_empty():
					rows.append(map)
	if not rows.is_empty():
		return rows
	for row_variant in campaign_section("maps"):
		var row: Dictionary = row_variant
		if not str(row.get("name", "")).is_empty():
			rows.append(row)
	return rows


func _rebuild() -> void:
	if _list == null:
		return
	_list.clear_children()
	var rows := _maps()
	if rows.is_empty():
		_list.add(MindWidgets.label(_tm("@maps.none"))).pad(8)
		return
	for row_variant in rows:
		var row: Dictionary = row_variant
		var name := str(row.get("name", ""))
		# code-instantiated: editor map rows come from the plan-06 Maps registry.
		var button := MindWidgets.button(name)
		button.pressed.connect(_open_map.bind(row))
		_list.add(button).grow_x_axis().pad(2)
		_list.row()


## `EditorMapsDialog.showMap` → "Open In Editor": loads the map into
## `/root/Spine/MindEditor` and shows the statically-instanced editor shell.
func _open_map(row: Dictionary) -> void:
	var name := str(row.get("name", ""))
	editor_map_action.emit("open", name)
	var path := str(row.get("path", ""))
	if path.is_empty():
		path = _builtin_path(name)
	if path.is_empty():
		_notify("@editor.errorload")
		return
	_open_in_editor(path)


func _open_import() -> void:
	_chooser_action = "import"
	_open_chooser(true)


func _open_export() -> void:
	var editor := get_node_or_null("/root/Spine/MindEditor")
	if editor == null:
		return
	var tags: Variant = editor.call("tags")
	if not (tags is Dictionary) or str(tags.get("name", "")).strip_edges().is_empty():
		_notify("@editor.save.noname")
		return
	_chooser_action = "export"
	_open_chooser(false)


func _open_chooser(is_open: bool) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("open_dialog"):
		return
	ui.call(
		"open_dialog",
		"file_chooser",
		JSON.stringify({"open": is_open, "extensions": ["msav"]})
	)


func _connect_file_chooser() -> void:
	var chooser := get_node_or_null("/root/Spine/Ui/UiRoot/DialogLayer/file_chooser")
	if chooser == null or not chooser.has_signal("file_chosen"):
		return
	if not chooser.is_connected("file_chosen", _on_file_chosen):
		chooser.connect("file_chosen", _on_file_chosen)


func _on_file_chosen(path: String) -> void:
	if _chooser_action.is_empty():
		return
	var action := _chooser_action
	_chooser_action = ""
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("close_dialog", "file_chooser")
	if action == "import":
		_open_in_editor(path)
		return
	var editor := get_node_or_null("/root/Spine/MindEditor")
	if editor == null or not bool(editor.call("export_map", path)):
		_notify("@editor.errorsave")
		return
	_refresh_registry()
	_rebuild()


func _open_in_editor(path: String) -> void:
	var editor := get_node_or_null("/root/Spine/MindEditor")
	if editor == null or not bool(editor.call("begin_edit_map", path)):
		_notify("@editor.errorload")
		return
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("close_dialog", "editor_maps")
	var dialog := get_node_or_null("/root/Spine/Ui/EditorDialog")
	if dialog != null and dialog.has_method("show_dialog"):
		dialog.call("show_dialog")


## Built-in maps live at `<assets>/maps/default/<name>.msav` (`MapSources`);
## `res://` cannot address the repo-root asset tree, so the native path is used.
func _builtin_path(name: String) -> String:
	var assets := MindWidgets.assets()
	if assets == null or not assets.has_method("assets_dir"):
		return ""
	return str(assets.call("assets_dir")).path_join("maps/default").path_join("%s.msav" % name)


func _notify(key: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_info"):
		ui.call("show_info", _t(key))
