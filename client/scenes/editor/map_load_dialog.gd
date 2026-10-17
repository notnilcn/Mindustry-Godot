## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/MapLoadDialog.java (plan 19 M4 §3.10; L07 T3
## PAR-0432 rewrite).
##
## UI-only: `@editor.selectmap` over a scrolling grid of every map
## (`maps.all()`, custom first like the Java list), each a 250x90
## `squareTogglet` with the 64 px bordered preview and the wrapped name;
## Cancel / Load (210x64, Load disabled until a map is picked). Loading calls
## `MindEditor.begin_edit_map(path)`.

extends MindDialog

## `table.defaults().size(250f, 90f).pad(4f)`.
const BUTTON_SIZE := Vector2(250.0, 90.0)
## `new BorderImage(map.safeTexture(), 2f)` at `size(16 * 4f)`.
const PREVIEW_SIZE := 64.0

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"

var _editor: Node = null
var _selected := ""
var _load: Button = null
var _group := ButtonGroup.new()


func _ready() -> void:
	set_title_key("@editor.loadmap")
	super._ready()
	_editor = get_node_or_null(editor_path)
	clear_buttons()
	# `buttons.defaults().size(210f, 64f)`: Cancel (Icon.cancel), Load (Icon.ok).
	var cancel := add_button(_t("@cancel"), hide_dialog, "cancel", 210.0)
	cancel.custom_minimum_size.y = 64.0
	_load = add_button(_t("@load"), _load_selected, "ok", 210.0)
	_load.custom_minimum_size.y = 64.0
	_load.disabled = true


func shown() -> void:
	_selected = ""
	if _load != null:
		_load.disabled = true
	_build()


## `rebuild()` (`MapLoadDialog.java:35-71`).
func _build() -> void:
	if cont == null:
		return
	for child in cont.get_children():
		cont.remove_child(child)
		child.queue_free()
	var root := content_table()
	var maps := _maps()
	if maps.is_empty():
		root.add(MindWidgets.label(_t("@maps.none")))
		return
	root.add(MindWidgets.label(_t("@editor.selectmap")))
	root.row()
	# `ScrollPane(table)`, X scrolling off; `table.margin(10f)`.
	# code-instantiated: the map grid is data-driven (the map registry).
	var scroll := ScrollContainer.new()
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	root.add(scroll).grow_y_axis()
	var margin := MarginContainer.new()
	for side in ["left", "top", "right", "bottom"]:
		margin.add_theme_constant_override("margin_" + side, 10)
	scroll.add_child(margin)
	var grid := GridContainer.new()
	# `cols = max(width / 300, 1)`.
	grid.columns = maxi(1, int(get_viewport_rect().size.x / 300.0))
	grid.add_theme_constant_override("h_separation", 8)
	grid.add_theme_constant_override("v_separation", 8)
	margin.add_child(grid)
	for row_value in maps:
		grid.add_child(_map_button(row_value as Dictionary))
	# The grid's full width (columns x 258 + margins) fixes the pane width.
	scroll.custom_minimum_size.x = grid.columns * (BUTTON_SIZE.x + 8.0) + 12.0


## One `squareTogglet` map button: preview (padLeft 5, 64 px) + name (wrap,
## centred, padLeft 5); `margin(8f)`.
func _map_button(map: Dictionary) -> Button:
	var button := Button.new()
	button.theme_type_variation = "squareTogglet"
	button.toggle_mode = true
	button.button_group = _group
	button.custom_minimum_size = BUTTON_SIZE
	var path := str(map.get("path", ""))
	button.pressed.connect(func() -> void:
		_selected = path
		if _load != null:
			_load.disabled = _selected.is_empty())
	var row := HBoxContainer.new()
	row.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	row.offset_left = 8.0 + 5.0
	row.offset_right = -8.0
	row.offset_top = 8.0
	row.offset_bottom = -8.0
	row.add_theme_constant_override("separation", 5)
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	button.add_child(row)
	# `BorderImage(texture, 2f)`: a 2 px `Pal.gray` frame around the preview.
	var frame := PanelContainer.new()
	var border := StyleBoxFlat.new()
	border.bg_color = Color(0, 0, 0, 0)
	border.border_color = Color8(0x45, 0x45, 0x45)
	for side in ["left", "top", "right", "bottom"]:
		border.set("border_width_" + side, 2)
		border.set("content_margin_" + side, 2.0)
	frame.add_theme_stylebox_override("panel", border)
	frame.custom_minimum_size = Vector2(PREVIEW_SIZE, PREVIEW_SIZE)
	frame.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	frame.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var image := TextureRect.new()
	image.texture = _preview_texture(str(map.get("name", "")))
	image.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	image.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	image.mouse_filter = Control.MOUSE_FILTER_IGNORE
	frame.add_child(image)
	row.add_child(frame)
	var name := MindWidgets.label(str(map.get("display_name", map.get("name", ""))))
	name.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	name.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	name.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	name.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	name.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	name.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(name)
	return button


## `maps.all()`: the registry rows, custom maps first.
func _maps() -> Array:
	var preview := get_node_or_null("/root/Spine/MindPreview")
	var custom: Array = []
	var builtin: Array = []
	if preview != null and preview.has_method("maps_list"):
		var rows: Variant = preview.call("maps_list")
		if rows is Array:
			for row_value in rows:
				var row: Dictionary = row_value
				if str(row.get("name", "")).is_empty():
					continue
				if bool(row.get("custom", false)):
					custom.append(row)
				else:
					builtin.append(row)
	return custom + builtin


func _preview_texture(map_name: String) -> Texture2D:
	var preview := get_node_or_null("/root/Spine/MindPreview")
	if preview == null or not preview.has_method("texture_for_name"):
		return null
	var value: Variant = preview.call("texture_for_name", map_name)
	return value if value is Texture2D else null


func _load_selected() -> void:
	if _selected.is_empty():
		return
	if _editor != null and _editor.has_method("begin_edit_map"):
		_editor.call("begin_edit_map", _selected)
	hide_dialog()
