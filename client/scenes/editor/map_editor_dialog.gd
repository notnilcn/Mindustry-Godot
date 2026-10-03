## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/MapEditorDialog.java (plan 19 §3.10).
##
## Editor shell. UI-only: toolbar/team/brush/show toggles, palette rebuild/sort/
## search and the config collapser all read/write `/root/Spine/MindEditor`. Every
## rule decision (tool dispatch, rotation, undo/redo, palette order) lives in
## Rust; this script only lays out and forwards.

extends Control

## Path to the MindEditor facade.
@export var editor_path: NodePath = ^"/root/Spine/MindEditor"

var _editor: Node = null
var _brush_sizes: Array[int] = []
var _team_buttons: Array[Button] = []

@onready var _search: LineEdit = $RightPalette/SearchField
@onready var _selected: Label = $RightPalette/SelectedLabel
@onready var _config_panel: VBoxContainer = $RightPalette/ConfigPanel
@onready var _config_label: Label = $RightPalette/ConfigPanel/ConfigLabel
@onready var _block_list: ItemList = $RightPalette/BlockList
@onready var _team_row: HBoxContainer = $LeftTools/TeamRow
@onready var _brush: HSlider = $LeftTools/BrushSlider


func _ready() -> void:
	_editor = get_node_or_null(editor_path)
	_bind_toolbar()
	_bind_toggles()
	_build_teams()
	_build_brush()
	if _editor != null:
		_rebuild_palette()
	_refresh()


func _process(_delta: float) -> void:
	if visible:
		_refresh()


## Shows the editor dialog (MCP / menu entry point).
func show_dialog() -> void:
	visible = true
	_refresh()


## Hides the editor dialog.
func hide_dialog() -> void:
	visible = false


func _bind_toolbar() -> void:
	var tools := {
		"ZoomButton": "zoom",
		"PickButton": "pick",
		"LineButton": "line",
		"PencilButton": "pencil",
		"EraserButton": "eraser",
		"FillButton": "fill",
		"SprayButton": "spray",
	}
	for button_name in tools:
		var button: Button = $LeftTools/ToolGrid.get_node(button_name)
		var tool: String = tools[button_name]
		button.pressed.connect(func() -> void: _select_tool(tool))
	$LeftTools/ToolGrid/GridButton.pressed.connect(_toggle_grid)
	$LeftTools/ToolGrid/UndoButton.pressed.connect(func() -> void: _call("undo"))
	$LeftTools/ToolGrid/RedoButton.pressed.connect(func() -> void: _call("redo"))
	$LeftTools/ToolGrid/RotateButton.pressed.connect(func() -> void: _rotate(1))
	_search.text_changed.connect(func(_text: String) -> void: _rebuild_palette())
	_block_list.item_selected.connect(_on_block_selected)


func _bind_toggles() -> void:
	$LeftTools/ShowChecks/ShowBlocks.toggled.connect(
		func(pressed: bool) -> void: _call("set_show_buildings", pressed)
	)
	$LeftTools/ShowChecks/ShowTerrain.toggled.connect(
		func(pressed: bool) -> void: _call("set_show_terrain", pressed)
	)
	$LeftTools/ShowChecks/ShowFloor.toggled.connect(
		func(pressed: bool) -> void: _call("set_show_floor", pressed)
	)


## code-instantiated: team buttons are data-driven from the base-team registry.
func _build_teams() -> void:
	if _editor == null:
		return
	for row_value in _editor.call("palette_teams"):
		var row: Dictionary = row_value
		var name := str(row.get("name", ""))
		var button := Button.new()
		button.text = name
		var packed: int = int(row.get("color", 0xFFFFFFFF))
		button.add_theme_color_override(
			"font_color",
			Color8((packed >> 24) & 0xFF, (packed >> 16) & 0xFF, (packed >> 8) & 0xFF)
		)
		button.pressed.connect(func() -> void: _select_team(name))
		_team_row.add_child(button)
		_team_buttons.append(button)


func _build_brush() -> void:
	if _editor == null:
		return
	_brush_sizes.clear()
	for value in _editor.call("brush_sizes"):
		_brush_sizes.append(int(value))
	if _brush_sizes.is_empty():
		return
	_brush.max_value = float(_brush_sizes.size() - 1)
	_brush.value = 0.0
	_brush.value_changed.connect(_on_brush_changed)


## Rebuilds the block palette in Rust (filter + upstream sort order).
func _rebuild_palette() -> void:
	if _editor == null:
		return
	var search := _search.text if _search != null else ""
	_block_list.clear()
	for row_value in _editor.call("palette_blocks", search):
		var row: Dictionary = row_value
		var name := str(row.get("name", ""))
		var index := _block_list.add_item(name)
		_block_list.set_item_metadata(index, name)


func _on_block_selected(index: int) -> void:
	var name := str(_block_list.get_item_metadata(index))
	_call("set_draw_block", name)
	_refresh()


func _on_brush_changed(value: float) -> void:
	var index := clampi(int(value), 0, _brush_sizes.size() - 1)
	_call("set_brush_size", float(_brush_sizes[index]) / 10.0)


func _select_tool(tool: String) -> void:
	_call("set_tool", tool)
	_refresh()


func _select_team(name: String) -> void:
	_call("set_draw_team", name)
	_refresh()


func _toggle_grid() -> void:
	if _editor == null:
		return
	_call("set_grid", not bool(_editor.call("grid")))


func _rotate(delta: int) -> void:
	if _editor == null:
		return
	_call("set_rotation", int(_editor.call("rotation")) + delta)


func _refresh() -> void:
	if _editor == null:
		return
	var status: Dictionary = _editor.call("status")
	var block := str(status.get("draw_block", "-"))
	_selected.text = "selected: %s\nteam: %d  rot: %d  brush: %.1f" % [
		block,
		int(status.get("draw_team", 0)),
		int(status.get("rotation", 0)),
		float(status.get("brush_size", 1.0)),
	]
	_refresh_config(block)


func _refresh_config(block: String) -> void:
	if _editor == null:
		return
	var info: Dictionary = _editor.call("block_info", block)
	var configurable := bool(info.get("configurable", false))
	_config_panel.visible = configurable
	if configurable:
		_config_label.text = "config: %s (size %d)" % [block, int(info.get("size", 1))]


func _unhandled_input(event: InputEvent) -> void:
	if not visible or _editor == null:
		return
	if not (event is InputEventKey) or not event.pressed or event.echo:
		return
	var key: int = (event as InputEventKey).keycode
	if (event as InputEventKey).ctrl_pressed:
		match key:
			KEY_Z:
				if (event as InputEventKey).shift_pressed:
					_call("redo")
				else:
					_call("undo")
			KEY_Y:
				_call("redo")
			KEY_S:
				_call("save")
			KEY_G:
				_toggle_grid()
			KEY_1, KEY_2, KEY_3, KEY_4, KEY_5:
				_call("set_tool_mode", _number_index(key))
		accept_event()
		return
	match key:
		KEY_ESCAPE:
			pass  # Menu sheet is M4; escape is reserved.
		KEY_V:
			_select_tool("zoom")
		KEY_I:
			_select_tool("pick")
		KEY_L:
			_select_tool("line")
		KEY_B:
			_select_tool("pencil")
		KEY_E:
			_select_tool("eraser")
			_rotate(-1)
		KEY_G:
			_select_tool("fill")
		KEY_R:
			_select_tool("spray")
			_rotate(1)
		_:
			return
	accept_event()


func _number_index(key: int) -> int:
	return key - KEY_1


func _call(method: String, arg0: Variant = null, arg1: Variant = null) -> Variant:
	if _editor == null:
		return null
	if arg1 != null:
		return _editor.call(method, arg0, arg1)
	if arg0 != null:
		return _editor.call(method, arg0)
	return _editor.call(method)
