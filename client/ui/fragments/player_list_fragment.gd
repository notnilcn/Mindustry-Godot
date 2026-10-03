## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/PlayerListFragment.java (plan 14 M7).
##
## Player-list shell: search field + row host + bans/admins/close actions. Rows
## come from `MindUi.player_list_json()` (empty until plan 21 supplies live
## views); row actions go through `MindUi.player_action`, which is the documented
## plan-21 relay seam and never fakes a server action (task rule).

extends Control

signal close_requested

var _content: VBoxContainer = null
var _search := ""


func _ready() -> void:
	set_anchors_preset(Control.PRESET_FULL_RECT)
	visible = false
	_build()


func _build() -> void:
	# code-instantiated: the playerlist panel is a runtime overlay whose rows are
	# server-driven, so its structure is built once in code (plan 14 §3.10).
	var center := CenterContainer.new()
	center.set_anchors_preset(Control.PRESET_FULL_RECT)
	add_child(center)
	var panel := PanelContainer.new()
	panel.theme_type_variation = "defaultDialog"
	panel.custom_minimum_size = Vector2(360, 420)
	center.add_child(panel)
	var column := VBoxContainer.new()
	panel.add_child(column)
	var field := MindWidgets.field(_t("@players.search"))
	field.text_changed.connect(func(text: String) -> void:
		_search = text.to_lower()
		rebuild())
	column.add_child(field)
	_content = VBoxContainer.new()
	_content.size_flags_vertical = Control.SIZE_EXPAND_FILL
	column.add_child(_content)
	var menu := HBoxContainer.new()
	for action in ["server.bans", "server.admins", "close"]:
		var button := MindWidgets.button(_t("@%s" % action))
		button.pressed.connect(_on_menu.bind(action))
		menu.add_child(button)
	column.add_child(menu)
	rebuild()


func rebuild() -> void:
	if _content == null:
		return
	for child in _content.get_children():
		child.queue_free()
	var ui := get_node_or_null("/root/MindUi")
	var rows: Array = []
	if ui != null and ui.has_method("player_list_json"):
		var parsed: Variant = JSON.parse_string(str(ui.call("player_list_json")))
		if parsed is Array:
			rows = parsed
	for row_variant in rows:
		var row: Dictionary = row_variant
		var name := str(row.get("name", ""))
		if not _search.is_empty() and not name.to_lower().contains(_search):
			continue
		# code-instantiated: player rows are server-driven (plan 21 relay views).
		var button := MindWidgets.button(name)
		button.pressed.connect(_on_player.bind(name))
		_content.add_child(button)


func _on_player(name: String) -> void:
	# code-instantiated: per-player action menu is data-driven by the relay view.
	_player_action(name, "spectate")


func _player_action(name: String, action: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("player_action"):
		ui.call("player_action", name, action)


func _on_menu(action: String) -> void:
	match action:
		"server.bans":
			_open("@server.bans")
		"server.admins":
			_open("@server.admins")
		_:
			visible = false
			close_requested.emit()


func _open(dialog: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("open_dialog", dialog.trim_prefix("@"), "")


func toggle() -> void:
	visible = not visible
	if visible:
		rebuild()


func shown() -> bool:
	return visible


func _t(key: String) -> String:
	var assets := MindWidgets.assets()
	if assets == null:
		return key.trim_prefix("@")
	return str(assets.call("bundle_get", key.trim_prefix("@")))
