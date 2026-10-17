## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/EditorMapsDialog.java (`showMap`,
## `:100-163`): the map preview beside the name/author/description column, then
## `@editor.openin` and `@delete` (disabled for built-in maps).

extends MindDialog

## `showMap`: `Core.graphics.isPortrait() ? 160f : 300f`.
const PREVIEW_SIZE := Vector2(300.0, 300.0)
## `mind_core::maps::UNKNOWN`: the `Map.tag` fallback `MindPreview.maps_list`
## puts in a row whose tag is missing/blank (`Map.hasTag` false).
const MISSING_TAG := "unknown"

var _name := ""
var _row: Dictionary = {}
## A `@delete` confirm is awaiting an answer.
var _pending_delete := false


func _ready() -> void:
	set_title_key("@editor.mapinfo")
	should_pause = false
	super._ready()
	add_close_button()
	_connect_confirm()


func set_context_json(json_text: String) -> void:
	super.set_context_json(json_text)
	_name = str(context().get("map", ""))


func shown() -> void:
	if _name.is_empty():
		_name = str(context().get("map", ""))
	_build()


func _build() -> void:
	# Java builds a fresh dialog per `showMap` (`EditorMapsDialog.java:101`);
	# this node is reused, so drop the previous open's body (L01 T1: the second
	# open stacked a second copy under the first).
	clear_content()
	var root := content_table()
	_row = _row_for(_name)
	# `EditorMapsDialog.showMap` (`ui/dialogs/EditorMapsDialog.java:100-163`): a
	# 2×2 grid — the preview and the `Styles.black` info pane are each
	# `mapsize` (300) squares, and the two action buttons `fillX()` their own
	# column, so the content is 600 px wide and centred by `BaseDialog`.
	const MAPSIZE := 300.0
	var preview := get_node_or_null("/root/Spine/MindPreview")
	var texture: Texture2D = null
	if preview != null and preview.has_method("texture_for_name"):
		texture = preview.call("texture_for_name", _name)
	if texture != null:
		# code-instantiated: the map preview texture comes from the registry.
		var image := MindBorderImage.new()
		image.texture = texture
		image.thickness = 3.0
		image.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		root.add(image).width(MAPSIZE).height(MAPSIZE)
	else:
		# Keep the grid's first column even without a preview texture.
		# code-instantiated: layout placeholder for the missing preview.
		root.add(Control.new()).width(MAPSIZE).height(MAPSIZE)
	# code-instantiated: the info pane's rows come from the map's tags.
	var info := PanelContainer.new()
	info.theme_type_variation = "black"
	var info_layout := VBoxContainer.new()
	info_layout.add_theme_constant_override("separation", 2)
	info_layout.alignment = BoxContainer.ALIGNMENT_BEGIN
	info.add_child(info_layout)
	_add_info(info_layout, "@editor.mapname", _name)
	var author := _tag("author")
	if author.is_empty():
		# `showMap`: built-ins with no author tag show "Anuke"; custom maps
		# show `map.author()` = `Map.tag` -> `@unknown`.
		author = "Anuke" if not bool(_row.get("custom", false)) else _t("@unknown")
	_add_info(info_layout, "@editor.author", author)
	# `if(!map.tags.get("description", "").isEmpty())`: no description row at
	# all for a map without one.
	var description := _tag("description")
	if not description.is_empty():
		_add_info(info_layout, "@editor.description", description)
	root.add(info).width(MAPSIZE).height(MAPSIZE)
	root.row()

	# `table.button("@editor.openin", Icon.export, ...).fillX().height(54f)`.
	var open_button := MindWidgets.icon_button("export", _t("@editor.openin"))
	open_button.custom_minimum_size.y = 54.0
	open_button.pressed.connect(_open_in_editor)
	root.add(open_button).width(MAPSIZE).height(54.0)
	# `table.button("@delete", Icon.trash, ...).disabled(!map.custom)`.
	var delete_button := MindWidgets.icon_button("trash", _t("@delete"))
	delete_button.custom_minimum_size.y = 54.0
	delete_button.disabled = not bool(_row.get("custom", false))
	delete_button.pressed.connect(_confirm_delete)
	root.add(delete_button).width(MAPSIZE).height(54.0)
	root.row()


## A row tag as the raw `map.tags.get(name, "")` (blank when missing).
func _tag(name: String) -> String:
	var value := str(_row.get(name, "")).strip_edges()
	return "" if value == MISSING_TAG else value


func _add_info(parent: VBoxContainer, label_key: String, value: String) -> void:
	var label := MindWidgets.label(_t(label_key))
	label.add_theme_color_override("default_color", Color.GRAY)
	parent.add_child(label)
	var text := MindWidgets.label(value)
	text.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	parent.add_child(text)


## `Vars.ui.editor.beginEditMap(map.file); dialog.hide(); hide();`.
func _open_in_editor() -> void:
	var path := str(_row.get("path", ""))
	var maps_dialog := _dialog("editor_maps")
	if maps_dialog != null and maps_dialog.has_method("_open_in_editor"):
		maps_dialog.call("_open_in_editor", path)
	_close_pressed()


## `ui.showConfirm("@confirm", Core.bundle.format("map.delete", map.name()), ...)`.
func _confirm_delete() -> void:
	_pending_delete = true
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_confirm"):
		ui.call("show_confirm", MindWidgets.markup_text(_fmt("@map.delete", [_name])))


func _on_confirm(confirmed: bool) -> void:
	if not _pending_delete:
		return
	_pending_delete = false
	if not confirmed:
		return
	var editor := get_node_or_null("/root/Spine/MindEditor")
	if editor == null or not bool(editor.call("delete_map", _name)):
		_notify("@editor.errorload")
		return
	var preview := get_node_or_null("/root/Spine/MindPreview")
	if preview != null and preview.has_method("refresh"):
		preview.call("refresh")
	var maps_dialog := _dialog("editor_maps")
	if maps_dialog != null and maps_dialog.has_method("_rebuild"):
		maps_dialog.call("_rebuild")
	_close_pressed()


func _connect_confirm() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	if ui.has_signal("confirm_result") and not ui.is_connected("confirm_result", _on_confirm):
		ui.connect("confirm_result", _on_confirm)


## The registry row for a map name (`MindPreview.maps_list`).
func _row_for(name: String) -> Dictionary:
	var preview := get_node_or_null("/root/Spine/MindPreview")
	if preview == null or not preview.has_method("maps_list"):
		return {}
	for row_variant in preview.call("maps_list"):
		var row: Dictionary = row_variant
		if str(row.get("name", "")) == name:
			return row
	return {}


func _dialog(name: String) -> Node:
	var ui_root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui_root == null or not ui_root.has_method("dialog"):
		return null
	return ui_root.call("dialog", name)


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
