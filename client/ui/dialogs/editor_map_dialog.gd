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
## `t.margin(6)` around the info rows (`EditorMapsDialog.java:113`).
const INFO_MARGIN := 6.0
## `t.defaults().padTop(10)` between caption rows.
const INFO_ROW_PAD := 10.0
## `.padTop(2)` above each value.
const INFO_VALUE_PAD := 2.0
## Wrapped-line pitch: Arc's `Fonts.def` lines are 20 px apart, `MindLabel`'s
## 19 px (java/17b-capital.png: description lines at y=263, 283, 303, ...).
const ARC_LINE_EXTRA := 1
## An Arc label's box is one more pixel than its line pitch (21 px for one
## line; the caption rows sit at y=133, 155, 186, 209 in java/17b-capital.png).
const ARC_LABEL_TAIL := 1.0
## Arc's cap offset inside a label box minus `MindLabel`'s (5 px vs 3 px).
const ARC_CAP_LEAD := 2.0
## `table.button(...).fillX().height(54f)` (`EditorMapsDialog.java:149,163`).
const BUTTON_HEIGHT := 54.0

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
		# `new BorderImage(map.safeTexture())`: the default `thickness = 4f`
		# (`BorderImage.java:16`, `EditorMapsDialog.java:108`).
		image.thickness = 4.0
		# `map.safeTexture()` is an Arc `Texture` with the default
		# `TextureFilter.nearest` (Arc `GLTexture.java:18-19`): crisp pixels.
		# `MindBorderImage._ready` resets `texture_filter` from `force_nearest`.
		image.force_nearest = true
		image.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		root.add(image).width(MAPSIZE).height(MAPSIZE)
	else:
		# Keep the grid's first column even without a preview texture.
		# code-instantiated: layout placeholder for the missing preview.
		root.add(Control.new()).width(MAPSIZE).height(MAPSIZE)
	# `table.table(Styles.black, desc -> { desc.top(); Table t = new Table();
	# t.margin(6); desc.add(new ScrollPane(t)).grow(); t.top();
	# t.defaults().padTop(10).left(); ... }).height(mapsize).width(mapsize)`
	# (`EditorMapsDialog.java:110-134`): the rows scroll inside the fixed
	# 300x300 pane, so a long description (Capital Ship Crash Site) never
	# pushes the action buttons off the dialog.
	# code-instantiated: the info pane's rows come from the map's tags.
	var info := PanelContainer.new()
	# `Styles.black` is a tinted `whiteui` region: no padding, square corners
	# (the themed `black` pane box pads 8/4 px), so `t.margin(6)` is the only
	# inset around the rows.
	var black := StyleBoxFlat.new()
	black.bg_color = Color.BLACK
	info.add_theme_stylebox_override("panel", black)
	info.clip_contents = true
	# code-instantiated: `new ScrollPane(t)` (default pane style, no X scroll).
	var pane := ScrollContainer.new()
	pane.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	pane.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	pane.size_flags_vertical = Control.SIZE_EXPAND_FILL
	info.add_child(pane)
	# code-instantiated: `t.margin(6)`.
	var margin := MarginContainer.new()
	margin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	for side in ["left", "top", "bottom"]:
		margin.add_theme_constant_override("margin_" + side, int(INFO_MARGIN))
	# Arc's `GlyphLayout` wraps a word only when its glyphs overflow; Godot's
	# line breaker also counts the space after it, so a word ending within
	# a space's advance of the edge drops to the next line ("that a
	# gargantuan capital" fits Java's 264 px, not Godot's). Hand the right
	# margin to the text box: ink still ends inside the 264 px column.
	margin.add_theme_constant_override("margin_right", 0)
	pane.add_child(margin)
	var info_layout := VBoxContainer.new()
	info_layout.add_theme_constant_override("separation", 0)
	info_layout.alignment = BoxContainer.ALIGNMENT_BEGIN
	info_layout.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	margin.add_child(info_layout)
	# `t.add("@editor.mapname")...padTop(0)`: the first row has no top pad.
	# Every Arc label draws its cap 2 px lower in its box than a `MindLabel`
	# (java/17b-capital.png: pane top 122, "Map Name:" cap 133), so the whole
	# column starts [constant ARC_CAP_LEAD] lower.
	_add_info_gap(info_layout, ARC_CAP_LEAD)
	_add_info(info_layout, "@editor.mapname", _name, 0.0)
	var author := _tag("author")
	if author.is_empty():
		# `showMap`: built-ins with no author tag show "Anuke"; custom maps
		# show `map.author()` = `Map.tag` -> `@unknown`.
		author = "Anuke" if not bool(_row.get("custom", false)) else _t("@unknown")
	_add_info(info_layout, "@editor.author", author, INFO_ROW_PAD)
	# `if(!map.tags.get("description", "").isEmpty())`: no description row at
	# all for a map without one.
	var description := _tag("description")
	if not description.is_empty():
		_add_info(info_layout, "@editor.description", description, INFO_ROW_PAD)
	root.add(info).width(MAPSIZE).height(MAPSIZE)
	root.row()

	# `table.button("@editor.openin", Icon.export, ...).fillX().height(54f)`.
	var open_button := MindWidgets.icon_button("export", _t("@editor.openin"))
	_fix_button_height(open_button)
	open_button.pressed.connect(_open_in_editor)
	root.add(open_button).width(MAPSIZE).height(BUTTON_HEIGHT)
	# `table.button("@delete", Icon.trash, ...).disabled(!map.custom)`.
	var delete_button := MindWidgets.icon_button("trash", _t("@delete"))
	_fix_button_height(delete_button)
	delete_button.disabled = not bool(_row.get("custom", false))
	delete_button.pressed.connect(_confirm_delete)
	root.add(delete_button).width(MAPSIZE).height(BUTTON_HEIGHT)
	root.row()


## `.height(54f)`: Arc's cell size wins over the icon row's pref height
## (`MindWidgets.icon_button` grows the button to its 31 px glyph + 24 px
## padding = 55 px on `ready`; this handler runs after it).
func _fix_button_height(button: Button) -> void:
	button.custom_minimum_size.y = BUTTON_HEIGHT
	button.ready.connect(func() -> void: button.custom_minimum_size.y = BUTTON_HEIGHT)


## A row tag as the raw `map.tags.get(name, "")` (blank when missing).
func _tag(name: String) -> String:
	var value := str(_row.get(name, "")).strip_edges()
	return "" if value == MISSING_TAG else value


## One caption + value pair: `t.add(key).padRight(10).color(Color.gray)`
## (`padTop(10)` from the defaults, 0 for the first) then
## `t.add(value).growX().wrap().padTop(2)` (`EditorMapsDialog.java:121-133`).
func _add_info(parent: VBoxContainer, label_key: String, value: String, pad_top: float) -> void:
	_add_info_gap(parent, pad_top)
	var label := MindWidgets.label(_t(label_key))
	label.add_theme_color_override("default_color", MindStyles.ARC_GRAY)
	label.add_theme_constant_override("line_separation", ARC_LINE_EXTRA)
	parent.add_child(label)
	_add_info_gap(parent, ARC_LABEL_TAIL + INFO_VALUE_PAD)
	var text := MindWidgets.label(value)
	text.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	text.add_theme_constant_override("line_separation", ARC_LINE_EXTRA)
	parent.add_child(text)
	_add_info_gap(parent, ARC_LABEL_TAIL)


func _add_info_gap(parent: VBoxContainer, height: float) -> void:
	if height <= 0.0:
		return
	# code-instantiated: a cell `padTop` between two runtime-built rows.
	var gap := Control.new()
	gap.custom_minimum_size = Vector2(0.0, height)
	gap.mouse_filter = Control.MOUSE_FILTER_IGNORE
	parent.add_child(gap)


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
