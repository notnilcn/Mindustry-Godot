## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/ResearchDialog.java (plan 14 M5).
##
## Tech-tree roots selector + node table over the plan-12 `TechStore` runtime.
## The interactive node graph (zoom/pan, tree layouts) is built from
## `client/ui/layout/*.gd`; the M5 shell lists nodes with unlock/requirement
## state and emits `research_requested` for the spend flow.

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
		if _root_name.is_empty():
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
		var row := MindWidgets.button(content)
		row.theme_type_variation = "flatBordert"
		row.disabled = unlocked or not ready
		row.tooltip_text = _requirements_text(node)
		row.pressed.connect(_confirm.bind(content))
		_nodes.add(row).grow_x_axis().pad(2)
		_nodes.row()


func _requirements_text(node: Dictionary) -> String:
	var parts := PackedStringArray()
	var requirements: Array = node.get("requirements", [])
	for requirement in requirements:
		parts.append("%s x%d" % [str(requirement[0]), int(requirement[1])])
	return ", ".join(parts) if not parts.is_empty() else "ready"


func _confirm(content: String) -> void:
	research_requested.emit(content)
