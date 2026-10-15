## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/EditorMapsDialog.java (plan 14 M5).
##
## Editor map browser (`MapListDialog`, `displayType = true`):
## `@editor.newmap` + `@editor.importmap` in the button row, a search row over
## the T5 preview grid, and per-map clicks opening `editor_map` (preview +
## name/author/description + open/delete).

extends MindDialog

## `MapListDialog` tile metrics (shared with `custom_game_dialog`).
const TILE_SIZE := Vector2(200.0, 288.0)
const NAME_WIDTH := 182.0
const PREVIEW_SIZE := Vector2(180.0, 180.0)
const COLUMN_WIDTH := 230.0
## `MapEditorDialog`/`ui.editor.show()` default size for `New Map`.
const NEW_MAP_SIZE := 200

var _field: LineEdit = null
var _grid: MindTable = null
var _search := ""
## Pending `file_chosen` route (`import`); empty when none is active.
var _chooser_action := ""
## A `file_chosen` handler is connected.
var _chooser_connected := false
## A New Map text prompt is awaiting an answer.
var _pending_new_map := false
## A pending import path awaiting the overwrite confirm.
var _pending_import := ""
## Coalesces `previews_changed` batches into one rebuild per frame.
var _preview_refresh_queued := false


func _ready() -> void:
	set_title_key("@maps")
	should_pause = false
	full_dialog = true
	super._ready()
	_build()
	clear_buttons()
	# `buildButtons`: `@editor.newmap` (210x64) and `@editor.importmap`.
	add_button(_t("@editor.newmap"), _new_map, "add", 210.0)
	add_button(_t("@editor.importmap"), _open_import, "upload", 210.0)
	add_close_button()
	_connect_prompts()


## `MapListDialog.shown` rebuilds the grid so refreshed maps appear.
func shown() -> void:
	_refresh_registry()
	_rebuild()


func _build() -> void:
	var root := content_table()
	# code-instantiated: the search row is a fixed composite (zoom glyph + field).
	var search := HBoxContainer.new()
	search.add_theme_constant_override("separation", 6)
	search.add_child(MindWidgets.glyph("zoom", 20))
	_field = MindWidgets.field(_t("@editor.search"))
	_field.max_length = 50
	_field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_field.text_changed.connect(func(text: String) -> void:
		_search = text.to_lower()
		_rebuild())
	search.add_child(_field)
	root.add(search).grow_x_axis().pad(4)
	root.row()
	# code-instantiated: tile count and layout are driven by the map registry.
	_grid = MindTable.new()
	_grid.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_grid.size_flags_vertical = Control.SIZE_EXPAND_FILL
	root.add(_grid).grow_x_axis().grow_y_axis()
	_rebuild()


## Reloads the live `MindPreview` registry and asks for missing previews.
func _refresh_registry() -> void:
	var preview := get_node_or_null("/root/Spine/MindPreview")
	if preview == null:
		return
	if preview.has_method("refresh"):
		preview.call("refresh")
	_connect_previews(preview)
	if preview.has_method("generate_previews"):
		preview.call("generate_previews")


func _connect_previews(preview: Node) -> void:
	if not preview.has_signal("previews_changed"):
		return
	if not preview.is_connected("previews_changed", Callable(self, "_on_previews_changed")):
		preview.connect("previews_changed", Callable(self, "_on_previews_changed"))


func _on_previews_changed() -> void:
	if _preview_refresh_queued:
		return
	_preview_refresh_queued = true
	call_deferred("_rebuild_after_previews")


func _rebuild_after_previews() -> void:
	_preview_refresh_queued = false
	_rebuild()


## The live registry rows (`MindPreview.maps_list`).
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
	return rows


func _rebuild() -> void:
	if _grid == null:
		return
	_grid.clear_children()
	var maxwidth := maxi(1, int(get_viewport_rect().size.x / COLUMN_WIDTH))
	var index := 0
	var shown_any := false
	for map_variant in _maps():
		var map: Dictionary = map_variant
		if not _passes_filter(map):
			continue
		shown_any = true
		if index > 0 and index % maxwidth == 0:
			_grid.row()
		_grid.add(_build_map_tile(map)).pad(8)
		index += 1
	if not shown_any:
		_grid.add(MindWidgets.label(_tm("@maps.none"))).pad(8)
	_grid.sort_now()
	_grid.call_deferred("sort_now")


func _passes_filter(map: Dictionary) -> bool:
	if _search.is_empty():
		return true
	var name := str(map.get("name", "")).to_lower()
	var author := str(map.get("author", "")).to_lower()
	return name.contains(_search) or author.contains(_search)


## `MapListDialog.rebuildMaps` tile: name, divider, preview, source label
## (shared shape with `custom_game_dialog`).
func _build_map_tile(map: Dictionary) -> Button:
	# code-instantiated: map tiles are data-driven from the Maps registry.
	var map_name := str(map.get("name", ""))
	var tile := MindWidgets.button("")
	tile.theme_type_variation = "grayt"
	tile.custom_minimum_size = TILE_SIZE
	tile.clip_contents = true
	tile.pressed.connect(func() -> void: _open_map(map_name))

	var column := VBoxContainer.new()
	column.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	column.alignment = BoxContainer.ALIGNMENT_CENTER
	column.mouse_filter = Control.MOUSE_FILTER_IGNORE
	column.add_theme_constant_override("separation", 4)
	tile.add_child(column)

	# code-instantiated: the name needs native ellipsis trimming.
	var name_label := Label.new()
	name_label.text = map_name
	name_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	name_label.clip_text = true
	name_label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	name_label.custom_minimum_size.x = NAME_WIDTH
	name_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	column.add_child(name_label)

	# code-instantiated: divider between the map name and the preview.
	var divider := HSeparator.new()
	divider.mouse_filter = Control.MOUSE_FILTER_IGNORE
	column.add_child(divider)

	var texture := _preview_texture(map_name)
	if texture != null:
		# code-instantiated: the preview texture comes from the map registry.
		var image := MindBorderImage.new()
		image.texture = texture
		image.thickness = 3.0
		image.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		image.custom_minimum_size = PREVIEW_SIZE
		image.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
		image.mouse_filter = Control.MOUSE_FILTER_IGNORE
		column.add_child(image)
	else:
		# code-instantiated: the `nomap` placeholder until generation lands.
		var preview := PanelContainer.new()
		preview.theme_type_variation = "smallPane"
		preview.custom_minimum_size = PREVIEW_SIZE
		preview.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
		preview.mouse_filter = Control.MOUSE_FILTER_IGNORE
		var glyph := MindWidgets.glyph("map", 48)
		glyph.mouse_filter = Control.MOUSE_FILTER_IGNORE
		preview.add_child(glyph)
		column.add_child(preview)

	# `displayType`: `@custom` / `@builtin` source label.
	var type_label := MindWidgets.label(_t("@custom") if bool(map.get("custom", false)) else _t("@builtin"))
	type_label.add_theme_color_override("default_color", Color.GRAY)
	type_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	type_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	column.add_child(type_label)

	return tile


func _preview_texture(map_name: String) -> Texture2D:
	var preview := get_node_or_null("/root/Spine/MindPreview")
	if preview == null or not preview.has_method("texture_for_name"):
		return null
	var value: Variant = preview.call("texture_for_name", map_name)
	return value if value is Texture2D else null


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
