## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/MapInfoDialog.java (plan 19 M4 §3.10).
##
## UI-only: the map-info hub — name/description/author fields writing into
## `MindEditor.tags`, then the seven routes: rules (`custom_rules` with
## `show_rule_edit`), waves, objectives, generation, locales, world processors
## and custom assets.

extends MindDialog

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"

## `MapInfoDialog.setup` route grid: label key, icon alias, target dialog;
## `wide` spans both columns (`Icon.fileCode`'s 460px row).
const ROUTES := [
	{"key": "@editor.rules", "icon": "list", "action": "rules"},
	{"key": "@editor.waves", "icon": "units", "action": "wave_info"},
	{"key": "@editor.objectives", "icon": "info", "action": "map_objectives"},
	{"key": "@editor.generation", "icon": "terrain", "action": "map_generate"},
	{"key": "@editor.locales", "icon": "file-text", "action": "map_locales"},
	{"key": "@editor.worldprocessors", "icon": "logic", "action": "map_processors"},
	{"key": "@asset.title", "icon": "tags", "action": "map_assets", "wide": true},
]

var _editor: Node = null


func _ready() -> void:
	set_title_key("@editor.mapinfo")
	should_pause = false
	super._ready()
	_editor = get_node_or_null(editor_path)
	add_close_button()


func shown() -> void:
	_build()


## `MapInfoDialog.setup` (`MapInfoDialog.java:33-126`): `cont.pane(t -> ...)`, a
## two-column table (label `padRight(8).left()`, field 400 wide, `defaults()
## .padTop(15)`), then the route buttons inside one `Tex.button` frame
## (`r.defaults().width(230f).height(60f)`, `Styles.flatt`, `marginLeft(10f)`),
## `colspan(2).center()`. The body scrolls when it is taller than the screen, so
## the last row sits below the fold like Java (L03 T3, PAR-0429).
func _build() -> void:
	var root := content_table()
	# code-instantiated: the field table is the dialog's data-driven body.
	var table := MindTable.new()
	table.defaults().set_pad_top(15.0)
	root.add(table)
	_add_field(table, "@editor.mapname", "name", 50, "@unknown", false)
	_add_field(table, "@editor.description", "description", 1000, "", true)
	_add_field(table, "@editor.author", "author", 50, "@unknown", false)

	# code-instantiated: `t.table(Tex.button, r -> ...)` frame around the routes.
	var frame := PanelContainer.new()
	var box := _pane_box(Color.BLACK, PANE_OUTLINE_GRAY)
	for side in [SIDE_LEFT, SIDE_TOP, SIDE_RIGHT, SIDE_BOTTOM]:
		box.set_content_margin(side, 12.0)
	frame.add_theme_stylebox_override("panel", box)
	var grid := MindWidgets.table()
	frame.add_child(grid)
	var column := 0
	for route_variant in ROUTES:
		var route: Dictionary = route_variant
		var wide := bool(route.get("wide", false))
		# code-instantiated: one route button per `MapInfoDialog` entry.
		var button := MindWidgets.icon_button(str(route.get("icon", "")), _t(str(route.get("key", ""))), "flatt")
		_inset_icon_row(button, 10.0)
		button.pressed.connect(_open_route.bind(str(route.get("action", ""))))
		grid.add(button).width(460.0 if wide else 230.0).height(60.0).set_colspan(2 if wide else 1)
		if wide:
			grid.row()
			column = 0
		else:
			column += 1
			if column % 2 == 0:
				grid.row()
	if column % 2 != 0:
		grid.row()
	table.add(frame).set_colspan(2)
	table.row()


## `.marginLeft(10f)`: the icon row starts 10 px in from the button edge.
func _inset_icon_row(button: Button, margin: float) -> void:
	for child in button.get_children():
		if child is HBoxContainer:
			(child as HBoxContainer).offset_left = margin


## One label + field row (`t.add(key).padRight(8).left(); t.field(...)
## .size(400, 55)` / `t.area(..., Styles.areaField, ...).size(400, 140)`).
func _add_field(table: MindTable, label_key: String, tag: String, max_length: int, placeholder: String, multiline: bool) -> void:
	var label := MindWidgets.label(_t(label_key))
	table.add(label).left().set_pad_right(8.0)
	if multiline:
		# code-instantiated: the description is the only multi-line field.
		# Arc `TextArea` wraps at the field width and has no horizontal scroll;
		# `Styles.areaField` is the same `underline` background as the fields.
		var area := TextEdit.new()
		area.text = str(_tags().get(tag, ""))
		area.wrap_mode = TextEdit.LINE_WRAPPING_BOUNDARY
		area.scroll_fit_content_height = false
		area.custom_minimum_size = Vector2(400.0, 140.0)
		area.text_changed.connect(func() -> void:
			if area.text.length() > max_length:
				area.text = area.text.substr(0, max_length)
			_set_tag(tag, area.text))
		table.add(area).width(400.0).height(140.0)
	else:
		var field := MindWidgets.field(_t(placeholder) if not placeholder.is_empty() else "")
		field.text = str(_tags().get(tag, ""))
		field.max_length = max_length
		field.custom_minimum_size = Vector2(400.0, 55.0)
		field.text_changed.connect(func(text: String) -> void: _set_tag(tag, text))
		table.add(field).width(400.0).height(55.0)
	table.row()


func _tags() -> Dictionary:
	if _editor != null and _editor.has_method("tags"):
		var value: Variant = _editor.call("tags")
		if value is Dictionary:
			return value
	return {}


func _set_tag(key: String, value: String) -> void:
	if _editor != null and _editor.has_method("set_tag"):
		_editor.call("set_tag", key, value)


## `MapInfoDialog.setup` routes: `rules` edits `state.rules` via the
## Custom Rules dialog (`showRuleEditRule = true`); the rest open their dialog.
func _open_route(action: String) -> void:
	if action == "rules":
		_open_rules()
		return
	if action == "map_generate":
		# From map info the filters are written to the `genfilters` tag
		# (`MapGenerateDialog(false)`), not applied to the editor.
		var generate := _dialog("map_generate")
		if generate != null:
			generate.set("applied", false)
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("open_dialog"):
		ui.call("open_dialog", action, "")
	_close_pressed()


## `ruleInfo.show(Vars.state.rules, () -> Vars.state.rules = new Rules())`:
## the working rules live in the editor's `rules` tag.
func _open_rules() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("open_dialog"):
		return
	var current: Variant = JSON.parse_string(str(_tags().get("rules", "{}")))
	if not (current is Dictionary):
		current = {}
	var reset: Variant = {}
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign != null and campaign.has_method("get_rules_json"):
		reset = JSON.parse_string(str(campaign.call("get_rules_json")))
	if not (reset is Dictionary):
		reset = {}
	ui.call(
		"open_dialog",
		"custom_rules",
		JSON.stringify({"rules": current, "reset": reset, "show_rule_edit": true})
	)
	var rules_dialog := _dialog("custom_rules")
	if rules_dialog != null and rules_dialog.has_signal("rules_edited") \
			and not rules_dialog.is_connected("rules_edited", _on_rules_edited):
		rules_dialog.connect("rules_edited", _on_rules_edited)
	_close_pressed()


func _on_rules_edited(json: String) -> void:
	_set_tag("rules", json)


func _dialog(name: String) -> Node:
	var ui_root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui_root == null or not ui_root.has_method("dialog"):
		return null
	return ui_root.call("dialog", name)
