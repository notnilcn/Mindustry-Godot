## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/ItemsDisplay.java (the research dialog's
##         `@globalitems` pool; ResearchDialog.java:111-113 stacks it top-left).
##
## `Tex.button` panel with the `@globalitems` collapser: amount, 24 px item icon
## and light-gray name instead of the plain chip row. The collapsed state
## persists across rebuilds (Java `collapsed` field); used items flash accent
## (`shine` map keyed by item name).

class_name MindResearchItemsDisplay
extends PanelContainer

## Whether the list is collapsed (Java `ItemsDisplay.collapsed`).
var collapsed := false
## Maximum list height (view height - 120); set by the host dialog.
var max_height := 400.0

var _toggle: Button = null
var _toggle_glyph: Label = null
var _scroll: ScrollContainer = null
var _list: VBoxContainer = null
static var _panel_style: StyleBoxTexture = null


func _init() -> void:
	# code-instantiated: the button frame is built here so the panel matches
	# `table(Tex.button, ...)` without a scene.
	if _panel_style == null:
		_panel_style = StyleBoxTexture.new()
		_panel_style.texture = MindWidgets.icon_texture("button")
		_panel_style.texture_margin_left = 12.0
		_panel_style.texture_margin_top = 12.0
		_panel_style.texture_margin_right = 12.0
		_panel_style.texture_margin_bottom = 12.0
	add_theme_stylebox_override("panel", _panel_style)
	mouse_filter = Control.MOUSE_FILTER_STOP


func _ready() -> void:
	# code-instantiated: toggle + collapser column (`ItemsDisplay.rebuild`).
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 4)
	add_child(column)
	# code-instantiated: the `@globalitems` collapser button.
	_toggle = Button.new()
	_toggle.theme_type_variation = "flatTogglet"
	_toggle.toggle_mode = true
	_toggle.custom_minimum_size = Vector2(200.0, 40.0)
	_toggle.pressed.connect(_on_toggle_pressed)
	column.add_child(_toggle)
	_update_toggle_content()
	# code-instantiated: the collapser list (max height set by the host).
	_scroll = ScrollContainer.new()
	_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	column.add_child(_scroll)
	# code-instantiated: one row per pooled item.
	_list = VBoxContainer.new()
	_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_scroll.add_child(_list)
	_update_visibility()


func _on_toggle_pressed() -> void:
	collapsed = not collapsed
	_update_visibility()
	_update_toggle_content()


## Rebuilds the rows; `items` is `research_tree_json.items` (item-id order) and
## `shine` marks item names whose amount just changed (flash accent -> white).
func rebuild(items: Array, shine: Dictionary = {}) -> void:
	if _list == null:
		return
	for child in _list.get_children():
		child.queue_free()
	for row_variant in items:
		var row_data: Dictionary = row_variant
		var amount := int(row_data.get("amount", 0))
		if amount <= 0:
			continue
		var item_name := str(row_data.get("item", ""))
		# code-instantiated: one row per item with a positive pool amount.
		var row := HBoxContainer.new()
		row.add_theme_constant_override("separation", 4)
		var amount_label := MindWidgets.label(str(row_data.get("text", str(amount))))
		if shine.has(item_name):
			# `Actions.color(Pal.accent -> white, 0.75s, Interp.fade)`.
			amount_label.modulate = MindStyles.ACCENT
			var tween := amount_label.create_tween()
			tween.tween_property(amount_label, "modulate", Color.WHITE, 0.75)
		row.add_child(amount_label)
		# code-instantiated: item icon follows the pool row.
		var icon := TextureRect.new()
		icon.texture = MindWidgets.icon_texture("item-%s-ui" % item_name)
		icon.custom_minimum_size = Vector2(24.0, 24.0)
		icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		row.add_child(icon)
		var name_label := MindWidgets.label(str(row_data.get("name", item_name)))
		name_label.add_theme_color_override("default_color", Color(0.75, 0.75, 0.75))
		row.add_child(name_label)
		_list.add_child(row)
	_update_size()


func _update_toggle_content() -> void:
	for child in _toggle.get_children():
		child.queue_free()
	# code-instantiated: glyph + `@globalitems` label row.
	var row := HBoxContainer.new()
	row.alignment = BoxContainer.ALIGNMENT_BEGIN
	row.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	row.add_theme_constant_override("separation", 8)
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_toggle.add_child(row)
	_toggle_glyph = MindWidgets.glyph("down-open", 20)
	row.add_child(_toggle_glyph)
	var label := MindWidgets.label(MindWidgets.markup("@globalitems"))
	label.add_theme_color_override("default_color", MindStyles.ACCENT)
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(label)


func _update_visibility() -> void:
	if _scroll != null:
		_scroll.visible = not collapsed
	if _toggle != null:
		_toggle.button_pressed = collapsed
	if _toggle_glyph != null:
		# Java: up-open while collapsed, down-open while expanded.
		_toggle_glyph.text = MindIcons.glyph("up-open" if collapsed else "down-open")


func _update_size() -> void:
	if _scroll == null or _list == null:
		return
	var content := _list.get_combined_minimum_size().y
	_scroll.custom_minimum_size.y = minf(content, max_height)
