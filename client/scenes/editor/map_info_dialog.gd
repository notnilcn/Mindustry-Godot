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


## `MapInfoDialog.setup`: fields then the button grid.
func _build() -> void:
	var root := content_table()
	_add_field(root, "@editor.mapname", "name", 50, "@unknown", false)
	_add_field(root, "@editor.description", "description", 1000, "", true)
	_add_field(root, "@editor.author", "author", 50, "@unknown", false)

	# code-instantiated: route buttons are data-driven from `ROUTES`.
	var grid := MindWidgets.table()
	var column := 0
	for route_variant in ROUTES:
		var route: Dictionary = route_variant
		var wide := bool(route.get("wide", false))
		# code-instantiated: one route button per `MapInfoDialog` entry.
		var button := MindWidgets.icon_button(str(route.get("icon", "")), _t(str(route.get("key", ""))))
		button.pressed.connect(_open_route.bind(str(route.get("action", ""))))
		grid.add(button).width(460.0 if wide else 230.0).height(60.0).pad(2).set_colspan(2 if wide else 1)
		if wide:
			grid.row()
			column = 0
		else:
			column += 1
			if column % 2 == 0:
				grid.row()
	if column % 2 != 0:
		grid.row()
	root.add(grid).pad(6)


## One label + field row (Java `t.add(key); t.field(...)`).
func _add_field(root: MindTable, label_key: String, tag: String, max_length: int, placeholder: String, multiline: bool) -> void:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 8)
	var label := MindWidgets.label(_t(label_key))
	label.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	row.add_child(label)
	if multiline:
		# code-instantiated: the description is the only multi-line field.
		var area := TextEdit.new()
		area.text = str(_tags().get(tag, ""))
		area.custom_minimum_size = Vector2(400.0, 140.0)
		area.text_changed.connect(func() -> void: _set_tag(tag, area.text))
		row.add_child(area)
	else:
		var field := MindWidgets.field("")
		field.text = str(_tags().get(tag, ""))
		field.placeholder_text = _t(placeholder) if not placeholder.is_empty() else ""
		field.max_length = max_length
		field.custom_minimum_size = Vector2(400.0, 55.0)
		field.text_changed.connect(func(text: String) -> void: _set_tag(tag, text))
		row.add_child(field)
	root.add(row).pad(4)
	root.row()


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
