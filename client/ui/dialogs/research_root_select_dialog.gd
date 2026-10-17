## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/ResearchDialog.java (the
## `BaseDialog("@techtree.select")` the title-table button opens).
##
## One 300x60 `flatTogglet` icon+name row per selectable tech-tree root
## (`TechTree.roots` minus locked roots that require unlock); the choice is
## emitted as `root_selected(name)` and the research dialog rebuilds its tree.

extends MindDialog

signal root_selected(root_name: String)

## `iconMed` (`Scl.scl(32f)`).
const ICON_SIZE := 32.0


func _ready() -> void:
	set_title_key("@techtree.select")
	should_pause = false
	super._ready()
	add_close_button(210.0)


func shown() -> void:
	_rebuild()


func _rebuild() -> void:
	# A reused dialog node: drop the previous open's body before rebuilding
	# (`shown()` rebuilds on every open; L01 T1).
	clear_content()
	var table := content_table()
	table.add_theme_constant_override("separation", 0)
	# Java centers the 300-wide `Tex.button` table inside the pane.
	table.custom_minimum_size.x = 300.0
	table.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	# Java `in.defaults().width(300f).height(60f)`.
	var selected := str(_context.get("selected", ""))
	for root_variant in (_context.get("roots", []) as Array):
		var entry: Dictionary = root_variant
		# code-instantiated: root rows come from the tree JSON roots list.
		var row := Button.new()
		row.theme_type_variation = "flatTogglet"
		row.toggle_mode = true
		row.button_pressed = (
			bool(entry.get("selected", false))
			or str(entry.get("name", "")) == selected
		)
		row.custom_minimum_size = Vector2(300.0, 60.0)
		# code-instantiated: row content (icon + name) is data-driven.
		var content := HBoxContainer.new()
		content.alignment = BoxContainer.ALIGNMENT_BEGIN
		content.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		content.add_theme_constant_override("separation", 8)
		content.mouse_filter = Control.MOUSE_FILTER_IGNORE
		row.add_child(content)
		# Java `.marginLeft(12f)`.
		var margin := Control.new()
		margin.custom_minimum_size = Vector2(12.0, 0.0)
		margin.mouse_filter = Control.MOUSE_FILTER_IGNORE
		content.add_child(margin)
		# code-instantiated: root icon follows the tree data (atlas region or the
		# `terrain` glyph for a sector root).
		var icon := MindWidgets.content_icon(
			str(entry.get("icon", "")), str(entry.get("type", "")), ICON_SIZE
		)
		if icon == null:
			icon = Control.new()
		icon.custom_minimum_size = Vector2(ICON_SIZE, ICON_SIZE)
		icon.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
		content.add_child(icon)
		var label := MindWidgets.label(str(entry.get("title", "")))
		label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
		label.mouse_filter = Control.MOUSE_FILTER_IGNORE
		content.add_child(label)
		row.pressed.connect(_choose.bind(str(entry.get("name", ""))))
		table.add(row).width(300.0).height(60.0)
		table.row()


func _choose(root_name: String) -> void:
	if root_name.is_empty():
		return
	root_selected.emit(root_name)
