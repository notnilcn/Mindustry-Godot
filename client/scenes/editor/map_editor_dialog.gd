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

## `build()`: tool button -> `EditorTool` name (`ui.getIcon(tool.name())`).
const TOOL_BUTTONS := {
	"ZoomButton": "zoom",
	"PickButton": "pick",
	"LineButton": "line",
	"PencilButton": "pencil",
	"EraserButton": "eraser",
	"FillButton": "fill",
	"SprayButton": "spray",
}
## `build()`: `float size = mobile ? 50f : 58f`.
const TOOL_SIZE := 58.0
## `rebuildBlockSelection`: `blockSelection.add(button).size(50f)`,
## `button.resizeImage(8 * 4f)`.
const BLOCK_BUTTON_SIZE := 50.0
const BLOCK_ICON_SIZE := 32.0
## `search.image(Icon.zoom)`: `Fonts.icon` glyphs are 30 px (`Fonts.java:78-82`).
const SEARCH_ICON_SIZE := 30
## `Tex.underline` padding (the 9-patch's 12 px pads) around the block name.
const UNDERLINE_PAD := 12.0
## Arc's line box is 2 px taller than Godot's for the same font (21 vs 19 px),
## and Arc centres the glyphs in it: java/18-editorview.png has the name table
## at y=44..89 ("Stone" cap top at 62, rule 85-88), a 45 px row of
## 12 + 21 + 12. Godot's 19 px line gets 14/12 to land the same pixels (the
## search field above is the same 45 px `Styles.defaultField` row, themed in
## `MindThemeBuilder`).
const UNDERLINE_PAD_TOP := 14.0
const UNDERLINE_PAD_BOTTOM := 12.0
## `scroll-knob-vertical-thin` width (`Styles.smallPane`).
const SMALL_PANE_BAR := 12.0
## `_last_block` value that never matches a block name (forces a refresh).
const NO_BLOCK := "<none>"

var _editor: Node = null
var _brush_sizes: Array[int] = []
## Team id -> its `clearNoneTogglei` button (`teamgroup`).
var _team_buttons: Dictionary = {}
## Block name -> its palette button (`blockSelection`).
var _block_buttons: Dictionary = {}
## Tool name -> the `M<n>` mode label stacked on its button.
var _mode_labels: Dictionary = {}
## Tool name -> last seen alternate mode (`EditorTool.mode`; Rust exposes the
## active tool's mode, and a mode only changes while its tool is active).
var _tool_modes: Dictionary = {}
var _undo_glyph: Label = null
var _redo_glyph: Label = null
var _rotate_glyph: Label = null
var _last_block := NO_BLOCK
## A `MindUi` confirm for `@editor.unsaved` is awaiting an answer.
var _pending_exit := false

@onready var _tool_grid: GridContainer = $LeftTools/ToolGrid
@onready var _search: LineEdit = $RightPalette/SearchPad/SearchRow/SearchField
@onready var _selected: Label = $RightPalette/SelectedPanel/SelectedLabel
@onready var _config_panel: VBoxContainer = $RightPalette/ConfigPanel
@onready var _config_label: Label = $RightPalette/ConfigPanel/ConfigLabel
@onready var _block_grid: GridContainer = $RightPalette/BlockScroll/BlockGrid
@onready var _team_grid: GridContainer = $LeftTools/TeamGrid
@onready var _brush: HSlider = $LeftTools/BrushStack/BrushSlider


func _ready() -> void:
	_editor = get_node_or_null(editor_path)
	_apply_theme()
	_apply_labels()
	_bind_toolbar()
	_bind_toggles()
	_build_teams()
	_build_brush()
	if _editor != null:
		_rebuild_palette()
	_refresh()
	_connect_confirm()


## The shell is a sibling of `UiRoot` (not under it), so it takes the shared
## skin explicitly (`MapEditorDialog` is a `Dialog` on the same `Scene` skin).
## `UiRoot._enter_tree` builds the theme before any `_ready` runs.
func _apply_theme() -> void:
	var ui_root := _ui_root() as Control
	if ui_root != null and ui_root.theme != null:
		theme = ui_root.theme


## Bundle text and icon-font glyphs for the statically declared controls.
func _apply_labels() -> void:
	_set_glyph($LeftTools/ToolGrid/MenuButton, "menu", "menu")
	_set_glyph($LeftTools/ToolGrid/GridButton, "grid", "grid")
	_undo_glyph = _set_glyph($LeftTools/ToolGrid/UndoButton, "undo", "undo")
	_redo_glyph = _set_glyph($LeftTools/ToolGrid/RedoButton, "redo", "redo")
	_rotate_glyph = _set_glyph($LeftTools/ToolGrid/RotateButton, "right", "rotate")
	for button_name: String in TOOL_BUTTONS:
		var tool: String = TOOL_BUTTONS[button_name]
		var button: Button = _tool_grid.get_node(button_name)
		_set_glyph(button, tool, tool)
		_mode_labels[tool] = _add_mode_label(button)
	# `search.image(Icon.zoom).padRight(8)` (MapEditorDialog.java:812): the
	# icon at its natural size (`Fonts.icon` is a 30 px font).
	var search_icon: Label = $RightPalette/SearchPad/SearchRow/SearchIcon
	search_icon.text = MindIcons.glyph("zoom")
	MindIcons.apply_icon_font(search_icon)
	search_icon.add_theme_font_size_override("font_size", SEARCH_ICON_SIZE)
	_search.placeholder_text = _t("@players.search")
	_apply_block_panel_styles()
	($LeftTools/BrushStack/BrushLabel as Label).text = _t("@editor.brush")
	($LeftTools/ChecksPad/ShowChecks/ShowBlocks as Button).text = _t("@editor.showblocks")
	($LeftTools/ChecksPad/ShowChecks/ShowTerrain as Button).text = _t("@editor.showterrain")
	($LeftTools/ChecksPad/ShowChecks/ShowFloor as Button).text = _t("@editor.showfloor")
	_set_icon_text($LeftTools/CenterButton, "move", _t("@editor.center"))


## `addBlockSelection` (MapEditorDialog.java:797-832) drawables the theme has
## no type for:
## - `cont.table(Tex.underline, extra -> extra.labelWrap(...).width(200f)
##   .center()).growX()`: the selected-block name sits in a full-width
##   `underline` table (12 px padding, 4 px #454545 bottom rule); the 200 px
##   label is centred but its text is left-aligned (Arc `Label` default).
## - `new ScrollPane(blockSelection, Styles.smallPane)`: `vScroll = clear`, the
##   knob the 12 px `scroll-knob-vertical-thin` region (`Styles.java:368-371`).
##   Arc lays the bar beside the 6x50 px grid, so the pane is 312 px wide.
func _apply_block_panel_styles() -> void:
	var underline := MindWidgets.icon_texture("underline")
	if underline != null:
		var box := StyleBoxTexture.new()
		box.texture = underline
		box.set_texture_margin_all(12.0)
		box.set_content_margin_all(UNDERLINE_PAD)
		box.content_margin_top = UNDERLINE_PAD_TOP
		box.content_margin_bottom = UNDERLINE_PAD_BOTTOM
		($RightPalette/SelectedPanel as PanelContainer).add_theme_stylebox_override("panel", box)
	var bar := ($RightPalette/BlockScroll as ScrollContainer).get_v_scroll_bar()
	var track := StyleBoxEmpty.new()
	track.content_margin_left = SMALL_PANE_BAR * 0.5
	track.content_margin_right = SMALL_PANE_BAR * 0.5
	bar.add_theme_stylebox_override("scroll", track)
	bar.add_theme_stylebox_override("scroll_focus", track)
	var thin := MindWidgets.icon_texture("scroll-knob-vertical-thin")
	if thin != null:
		var knob := StyleBoxTexture.new()
		knob.texture = thin
		for slot in ["grabber", "grabber_highlight", "grabber_pressed"]:
			bar.add_theme_stylebox_override(slot, knob)


## `mid.button("@editor.center", Icon.move, Styles.flatt, view::center)
## .margin(9f)` (MapEditorDialog.java:720-722): an image text button — the icon
## at the left margin and the label centred in the rest (L03 T3, PAR-0421).
## code-instantiated: icon-font glyphs need `MindAssets.icon_font()` at runtime.
func _set_icon_text(button: Button, icon_name: String, text_value: String) -> void:
	button.text = text_value
	if MindIcons.glyph(icon_name).is_empty():
		return
	button.text = ""
	var row := HBoxContainer.new()
	row.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	row.offset_left = 9.0
	row.offset_right = -9.0
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var glyph := MindWidgets.glyph(icon_name, 28)
	glyph.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(glyph)
	var label := Label.new()
	label.text = text_value
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(label)
	button.add_child(row)


## code-instantiated: icon-font glyphs need `MindAssets.icon_font()` at runtime.
## Returns the glyph label (null when the glyph is unknown: text fallback).
func _set_glyph(button: Button, icon_name: String, fallback: String) -> Label:
	button.text = ""
	if MindIcons.glyph(icon_name).is_empty():
		button.text = fallback
		return null
	var glyph := MindWidgets.glyph(icon_name, 28)
	glyph.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	glyph.mouse_filter = Control.MOUSE_FILTER_IGNORE
	button.add_child(glyph)
	return glyph


## code-instantiated: `tools.stack(button, mode)`: the `M<n>` alt-mode label in
## `Pal.remove`, bottom-right, untouchable.
func _add_mode_label(button: Button) -> Label:
	var label := Label.new()
	label.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	label.vertical_alignment = VERTICAL_ALIGNMENT_BOTTOM
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	label.add_theme_color_override("font_color", MindStyles.REMOVE)
	label.add_theme_font_size_override("font_size", 14)
	button.add_child(label)
	return label


## `MapEditorDialog.tryExit` answers through the shared `MindUi` confirm signal.
func _connect_confirm() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	if ui.has_signal("confirm_result") and not ui.is_connected("confirm_result", _on_confirm):
		ui.connect("confirm_result", _on_confirm)


func _process(_delta: float) -> void:
	if visible:
		_refresh()


## Shows the editor dialog (MCP / menu entry point). The shell is a
## full-screen dialog below the `UiRoot` layers: `UiRoot.set_editor_open` hides
## the standalone menu (`MenuFragment` `group.visible(() -> !ui.editor.isShown())`)
## and the in-game HUD, closes the config/inventory panels and lets editor
## dialogs/prompts take the pointer above the shell.
func show_dialog() -> void:
	visible = true
	_refresh()
	var ui_root := _ui_root()
	if ui_root != null and ui_root.has_method("set_editor_open"):
		ui_root.call("set_editor_open", true)


## Hides the editor dialog (`hidden(...)`); the caller shows the next surface.
func hide_dialog() -> void:
	visible = false
	_drop_temporary_input()
	var ui_root := _ui_root()
	if ui_root != null and ui_root.has_method("set_editor_open"):
		ui_root.call("set_editor_open", false)


func _ui_root() -> Node:
	return get_node_or_null("/root/Spine/Ui/UiRoot")


## True while a `MindUi` dialog or prompt is up over the shell: `MapEditorDialog`
## only runs `doInput()` while it `hasKeyboard()` (it is the top dialog), and
## `MapView` stops key panning under a modal.
func has_modal() -> bool:
	var ui_root := _ui_root()
	return ui_root != null and ui_root.has_method("has_modal") and bool(ui_root.call("has_modal"))


## Releases any in-progress stroke / temporary tool in the view.
func _drop_temporary_input() -> void:
	var view := get_node_or_null(^"MapView")
	if view != null and view.has_method("cancel_input"):
		view.call("cancel_input")


## `MapEditorDialog.build()` (`:606`): the first tool-grid button is
## `Icon.menu` -> `menu.show()`. Escape does the same (`doInput` `:750-754`);
## it does not exit the editor.
func _open_menu() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("open_dialog"):
		ui.call("open_dialog", "editor_menu", "")


## Closing the editor returns to the main menu: `tryExit`'s confirm runs
## `logic.reset(); hide()` and nothing reopens the maps dialog (it was hidden by
## `EditorMapsDialog`'s New Map / `@editor.openin` before `ui.editor.show()`),
## so `MenuFragment`'s `group.visible(() -> !ui.editor.isShown())` brings the
## menu back. Every open dialog on top (the editor menu) is closed too
## (`menu.hide()` after `tryExit()` in the `@quit` button).
func _exit_to_menu() -> void:
	hide_dialog()
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("close_dialog"):
		ui.call("close_dialog", "editor_menu")
	# `logic.reset(); hide()` (`MapEditorDialog.java:792-793`).
	var ui_root := _ui_root()
	if ui_root != null and ui_root.has_method("return_to_menu"):
		ui_root.call("return_to_menu")


## `MapEditorDialog.tryExit` (`MapEditorDialog.java:789-795`): always
## `ui.showConfirm("@confirm", "@editor.unsaved", ...)`; it never checks
## `editor.saved`.
func _confirm_exit() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("show_confirm"):
		_exit_now()
		return
	_pending_exit = true
	ui.call("show_confirm", MindWidgets.markup_text(_t("@editor.unsaved")))


## The accept path: `logic.reset(); hide()` (`MindEditor.try_exit`).
func _exit_now() -> void:
	if _editor != null:
		_editor.call("try_exit")
	_exit_to_menu()


func _on_confirm(confirmed: bool) -> void:
	if not _pending_exit:
		return
	_pending_exit = false
	if confirmed:
		_exit_now()


func _t(key: String) -> String:
	var resolved := key.trim_prefix("@")
	var assets := MindWidgets.assets()
	if assets == null:
		return resolved
	return str(assets.call("bundle_get", resolved))


func _bind_toolbar() -> void:
	$LeftTools/ToolGrid/MenuButton.pressed.connect(_open_menu)
	for button_name: String in TOOL_BUTTONS:
		var button: Button = _tool_grid.get_node(button_name)
		var tool: String = TOOL_BUTTONS[button_name]
		button.pressed.connect(func() -> void: _select_tool(tool))
	$LeftTools/ToolGrid/GridButton.pressed.connect(_toggle_grid)
	$LeftTools/ToolGrid/UndoButton.pressed.connect(func() -> void: _call("undo"))
	$LeftTools/ToolGrid/RedoButton.pressed.connect(func() -> void: _call("redo"))
	# `editor.rotation = (editor.rotation + 1) % 4`.
	$LeftTools/ToolGrid/RotateButton.pressed.connect(func() -> void: _rotate(1))
	# `mid.button("@editor.center", Icon.move, Styles.flatt, view::center)`.
	$LeftTools/CenterButton.pressed.connect(func() -> void: _call("map_view_center"))
	_search.text_changed.connect(func(_text: String) -> void: _rebuild_palette())


func _bind_toggles() -> void:
	$LeftTools/ChecksPad/ShowChecks/ShowBlocks.toggled.connect(
		func(pressed: bool) -> void: _call("set_show_buildings", pressed)
	)
	$LeftTools/ChecksPad/ShowChecks/ShowTerrain.toggled.connect(
		func(pressed: bool) -> void: _call("set_show_terrain", pressed)
	)
	$LeftTools/ChecksPad/ShowChecks/ShowFloor.toggled.connect(
		func(pressed: bool) -> void: _call("set_show_floor", pressed)
	)


## `for(Team team : Team.baseTeams)`: `ImageButton(Tex.whiteui,
## Styles.clearNoneTogglei)`, `margin(4f)`, the image tinted `team.color`,
## three per row.
## code-instantiated: team buttons are data-driven from the base-team registry.
func _build_teams() -> void:
	if _editor == null:
		return
	for row_value in _editor.call("palette_teams"):
		var row: Dictionary = row_value
		var id := int(row.get("id", 0))
		var team_name := str(row.get("name", ""))
		var button := Button.new()
		button.theme_type_variation = "clearNoneTogglei"
		button.toggle_mode = true
		button.custom_minimum_size = Vector2(TOOL_SIZE, TOOL_SIZE)
		var swatch := ColorRect.new()
		swatch.set_anchors_preset(Control.PRESET_FULL_RECT)
		swatch.offset_left = 4.0
		swatch.offset_top = 4.0
		swatch.offset_right = -4.0
		swatch.offset_bottom = -4.0
		swatch.mouse_filter = Control.MOUSE_FILTER_IGNORE
		var packed: int = int(row.get("color", 0xFFFFFFFF))
		swatch.color = Color8((packed >> 24) & 0xFF, (packed >> 16) & 0xFF, (packed >> 8) & 0xFF)
		button.add_child(swatch)
		button.pressed.connect(func() -> void: _select_team(team_name))
		_team_grid.add_child(button)
		_team_buttons[id] = button


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


## `rebuildBlockSelection(searchText)`: the Rust palette order (core first,
## natural before synthetic, ores last, then id), filtered by the localized name
## (`block.localizedName.toLowerCase().contains(search)`); blocks without a UI
## icon region are skipped (`!Core.atlas.isFound(block.uiIcon)`). Six 50px
## `clearNoneTogglei` buttons per row, a 32px icon and the localized name as
## tooltip.
## code-instantiated: one button per palette block (content-driven count).
func _rebuild_palette() -> void:
	if _editor == null:
		return
	for child in _block_grid.get_children():
		_block_grid.remove_child(child)
		child.queue_free()
	_block_buttons.clear()
	_last_block = NO_BLOCK
	var search := ""
	if _search != null:
		search = " ".join(_search.text.strip_edges().split(" ", false)).to_lower()
	for row_value in _editor.call("palette_blocks", ""):
		var row: Dictionary = row_value
		var block_key := str(row.get("name", ""))
		var localized := _block_name(block_key)
		if not search.is_empty() and not localized.to_lower().contains(search):
			continue
		var texture := MindWidgets.icon_texture("block-%s-ui" % block_key)
		if texture == null:
			continue
		var button := Button.new()
		button.theme_type_variation = "clearNoneTogglei"
		button.toggle_mode = true
		button.custom_minimum_size = Vector2(BLOCK_BUTTON_SIZE, BLOCK_BUTTON_SIZE)
		button.tooltip_text = localized
		var icon := TextureRect.new()
		icon.texture = texture
		icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
		icon.set_anchors_preset(Control.PRESET_CENTER)
		var half := BLOCK_ICON_SIZE / 2.0
		icon.offset_left = -half
		icon.offset_top = -half
		icon.offset_right = half
		icon.offset_bottom = half
		button.add_child(icon)
		button.pressed.connect(func() -> void: _on_block_selected(block_key))
		_block_grid.add_child(button)
		_block_buttons[block_key] = button


## `block.localizedName`: `Core.bundle.get("block." + name + ".name", name)`.
func _block_name(block_key: String) -> String:
	if block_key.is_empty():
		return block_key
	var key := "block.%s.name" % block_key
	var resolved := _t("@" + key)
	return block_key if resolved == key or resolved.is_empty() else resolved


func _on_block_selected(block_key: String) -> void:
	_call("set_draw_block", block_key)
	_refresh()


func _on_brush_changed(value: float) -> void:
	var index := clampi(int(value), 0, _brush_sizes.size() - 1)
	_call("set_brush_size", float(_brush_sizes[index]) / 10.0)


func _select_tool(tool: String) -> void:
	_call("set_tool", tool)
	_refresh()


func _select_team(team_name: String) -> void:
	_call("set_draw_team", team_name)
	_refresh()


func _toggle_grid() -> void:
	if _editor == null:
		return
	_call("set_grid", not bool(_editor.call("grid")))


func _rotate(delta: int) -> void:
	if _editor == null:
		return
	_call("set_rotation", int(_editor.call("rotation")) + delta)


## The per-frame `update(...)` hooks of `build()`: tool/grid/team/block
## `setChecked`, undo/redo `setDisabled` + gray image, the rotate image turned
## `rotation * 90`, the tool `M<n>` labels and the selected block's name.
func _refresh() -> void:
	if _editor == null:
		return
	var status: Dictionary = _editor.call("status")
	var tool := str(status.get("tool", ""))
	_tool_modes[tool] = int(status.get("tool_mode", -1))
	for button_name: String in TOOL_BUTTONS:
		var tool_name: String = TOOL_BUTTONS[button_name]
		var button: Button = _tool_grid.get_node(button_name)
		button.set_pressed_no_signal(tool_name == tool)
		var label: Label = _mode_labels.get(tool_name)
		if label != null:
			var mode := int(_tool_modes.get(tool_name, -1))
			label.text = "" if mode == -1 else "M%d " % (mode + 1)
	var grid_button: Button = $LeftTools/ToolGrid/GridButton
	grid_button.set_pressed_no_signal(bool(status.get("grid", false)))
	_refresh_undo($LeftTools/ToolGrid/UndoButton, _undo_glyph, bool(status.get("can_undo", false)))
	_refresh_undo($LeftTools/ToolGrid/RedoButton, _redo_glyph, bool(status.get("can_redo", false)))
	if _rotate_glyph != null:
		# Arc turns counter-clockwise (y-up); Godot's y-down turns clockwise.
		_rotate_glyph.pivot_offset = _rotate_glyph.size / 2.0
		_rotate_glyph.rotation_degrees = -90.0 * float(int(status.get("rotation", 0)))
	var team := int(status.get("draw_team", -1))
	for id: int in _team_buttons:
		var team_button: Button = _team_buttons[id]
		team_button.set_pressed_no_signal(id == team)
	var block := str(status.get("draw_block", ""))
	if block != _last_block:
		_last_block = block
		for block_name: String in _block_buttons:
			var block_button: Button = _block_buttons[block_name]
			block_button.set_pressed_no_signal(block_name == block)
		# `extra.labelWrap(() -> editor.drawBlock.localizedName)`.
		_selected.text = _block_name(block)
	_refresh_config(block)


## `undo.setDisabled(() -> !editor.canUndo())` + the gray image tint.
func _refresh_undo(button: Button, glyph: Label, enabled: bool) -> void:
	button.disabled = not enabled
	if glyph != null:
		glyph.modulate = Color.WHITE if enabled else MindStyles.ARC_GRAY


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
	# `update(() -> { if(hasKeyboard()) doInput(); })`: dialogs/prompts on top
	# own the keys (UiRoot's Escape closes them).
	if has_modal():
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
		get_viewport().set_input_as_handled()
		accept_event()
		return
	match key:
		KEY_ESCAPE:
			# `MapEditorDialog.doInput`: Escape opens the menu; it does not exit.
			_open_menu()
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
	# `MapEditorDialog.doInput` consumes its keys; UiRoot's Escape handler would
	# otherwise open the pause menu behind the editor shell.
	get_viewport().set_input_as_handled()
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
