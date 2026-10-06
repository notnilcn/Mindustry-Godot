## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/ResearchDialog.java (plan 14 M5).
##
## Tech-tree roots selector + node table over the plan-12 `TechStore` runtime.
## Rows carry live unlock/ready state and requirement progress from the
## `MindCampaign` read models; the spend flow calls the live `research` facade
## through `research_requested` (the M5 stub left the signal unconnected), and
## each node exposes the content-info button (`ResearchDialog.java:668`).

extends MindDialog

## Emitted when a researchable node is confirmed.
signal research_requested(content: String)

var _roots: MindTable = null
var _nodes: MindTable = null
var _root_name := ""


func _ready() -> void:
	set_title_key("@research")
	should_pause = true
	full_dialog = true
	super._ready()
	_build()
	add_close_button()
	if not research_requested.is_connected(_on_research_requested):
		research_requested.connect(_on_research_requested)


func shown() -> void:
	# Live refresh per open (the fixture rows would otherwise freeze at boot).
	refresh_campaign_views(str(_context.get("planet", "")))
	_rebuild_roots()
	_rebuild_nodes()


func _build() -> void:
	var columns := HBoxContainer.new()
	columns.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_table_add(columns)
	# code-instantiated: the root rail is the data-driven TechTree.roots list.
	_roots = MindTable.new()
	_roots.custom_minimum_size.x = 180
	columns.add_child(_roots)
	_nodes = MindTable.new()
	_nodes.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	columns.add_child(_nodes)
	_rebuild_roots()
	_rebuild_nodes()


func _table_add(node: Control) -> void:
	content_table().add(node).grow_x_axis().grow_y_axis()


func _rebuild_roots() -> void:
	_roots.clear_children()
	var nodes := campaign_section("research")
	var seen := {}
	for node_variant in nodes:
		var node: Dictionary = node_variant
		var root := str(node.get("root", ""))
		if root.is_empty() or seen.has(root):
			continue
		seen[root] = true
		if _root_name.is_empty() or not seen.has(_root_name):
			_root_name = root
		# code-instantiated: root entries are the data-driven TechNode roots.
		var button := MindWidgets.button(_t("techtree.%s" % root))
		button.toggle_mode = true
		button.button_pressed = root == _root_name
		button.pressed.connect(_select_root.bind(root))
		_roots.add(button).grow_x_axis().pad(2)
		_roots.row()


func _select_root(root: String) -> void:
	_root_name = root
	_rebuild_roots()
	_rebuild_nodes()


func _rebuild_nodes() -> void:
	_nodes.clear_children()
	var nodes := campaign_section("research")
	for node_variant in nodes:
		var node: Dictionary = node_variant
		if str(node.get("root", "")) != _root_name:
			continue
		var content := str(node.get("content", ""))
		var unlocked := bool(node.get("unlocked", false))
		var ready := bool(node.get("dependencies_met", false)) and bool(node.get("requirements_met", false))
		# code-instantiated: node rows are the data-driven TechStore.nodes.
		var row := HBoxContainer.new()
		row.add_theme_constant_override("separation", 4)
		var info := MindWidgets.image_button_first(PackedStringArray(["info"]), "clearNonei")
		info.custom_minimum_size = Vector2(34.0, 34.0)
		info.tooltip_text = _t("@info.title")
		info.pressed.connect(func() -> void: open_content_info(content))
		row.add_child(info)
		var button := MindWidgets.button(_localized_content(content))
		button.theme_type_variation = "flatBordert"
		button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		button.disabled = unlocked or not ready
		button.tooltip_text = _requirements_text(node)
		button.pressed.connect(_confirm.bind(content))
		row.add_child(button)
		if unlocked:
			row.add_child(MindWidgets.label("unlocked"))
		elif ready:
			row.add_child(MindWidgets.label("ready"))
		_nodes.add(row).grow_x_axis().pad(2)
		_nodes.row()


## Bundle label for a tech node's content (`<type>.<name>.name`); the type is
## not part of `ResearchView`, so try the unlockable content types in order.
func _localized_content(content: String) -> String:
	var assets := MindWidgets.assets()
	if assets == null:
		return content
	for type_name in ["block", "item", "unit", "liquid", "status"]:
		var key := "%s.%s.name" % [type_name, content]
		var value := str(assets.call("bundle_get", key))
		if not value.is_empty() and value != key:
			return value
	return content


func _requirements_text(node: Dictionary) -> String:
	var parts := PackedStringArray()
	var requirements: Array = node.get("requirements", [])
	var finished: Array = node.get("finished", [])
	for index in requirements.size():
		var requirement = requirements[index]
		var done := 0
		if index < finished.size():
			done = int(finished[index][1])
		parts.append("%s %d/%d" % [str(requirement[0]), done, int(requirement[1])])
	return ", ".join(parts) if not parts.is_empty() else "ready"


func _confirm(content: String) -> void:
	research_requested.emit(content)


## Spends on the live campaign node and re-reads the tech state so unlock/ready
## rows update after a successful purchase.
func _on_research_requested(content: String) -> void:
	var campaign := campaign_node()
	if campaign == null or not campaign.has_method("research"):
		return
	if not bool(campaign.call("research", content)):
		return
	refresh_campaign_views(str(_context.get("planet", "")))
	_rebuild_nodes()
	show_toast(_t("@unlocked"))
