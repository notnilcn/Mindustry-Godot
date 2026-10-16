## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/PlacementFragment.java
##         (plan 14 §3.5, M4).
##
## Block/category palette with search, the command table and the hover info
## box. Static frame in `placement_fragment.tscn` (tscn-first); the category rail
## and block grid are data-driven from the `MindUi.block_catalog_json()` catalogue
## (plan 02/07/15). The hover info box renders `ui::display` rows once
## `MindHud`/the hover provider land.

extends Control

## Upstream `InputHandler.buildPlacementUI` (DesktopInput.java:545-562): the
## four icon buttons under the block pane. `tree`/`map` are campaign-only
## (`visible(() -> state.isCampaign())`).
const INPUT_BUTTONS := [
	{"icon": "paste", "tooltip": "@schematics", "dialog": "schematics"},
	{"icon": "book", "tooltip": "@database", "dialog": "database"},
	{"icon": "tree", "tooltip": "@research", "dialog": "research", "campaign_only": true},
	{"icon": "map", "tooltip": "@planetmap", "dialog": "planet", "campaign_only": true},
]

## `-1` = pick `Category.distribution` on the next rail build (`currentCategory`
## default, reset on `WorldLoadEvent`).
var _selected_category := -1
## Sim tick seen at the last reveal; a smaller tick means a new world loaded.
var _last_tick := -1
var _catalog: Dictionary = {}
var _input_buttons: Array[Button] = []
var _bar_campaign := false

@onready var panel: PanelContainer = get_node_or_null("Panel")
@onready var search: LineEdit = get_node_or_null("Panel/Frame/Layout/Search")
@onready var categories: GridContainer = get_node_or_null("Panel/Frame/Layout/Categories")
@onready var blocks: GridContainer = get_node_or_null("Panel/Frame/Layout/Left/Scroll/Blocks")
@onready var input_bar: HBoxContainer = get_node_or_null("Panel/Frame/Layout/Left/InputBar")
@onready var top_table: PanelContainer = get_node_or_null("Panel/Frame/TopTable")
@onready var top_rows: VBoxContainer = get_node_or_null("Panel/Frame/TopTable/Rows")
@onready var top_divider: ColorRect = get_node_or_null("Panel/Frame/TopDivider")
@onready var command_panel: PanelContainer = get_node_or_null("Panel/Frame/CommandPanel")
@onready var command_rows: VBoxContainer = get_node_or_null("Panel/Frame/CommandPanel/Rows")
@onready var layout: HBoxContainer = get_node_or_null("Panel/Frame/Layout")

## Last rendered top-table state (`b:<block>` or `h:<kind>:<name>`); the header
## rebuilds only when it changes (`PlacementFragment.lastDisplayState`).
var _top_state := ""
## One-shot warning when the `block_select_01..10` binds are not all displayable
## (the key combo line is silently dropped otherwise).
var _warned_block_select := false
## Last block selection mirrored into the palette's checked cell (T4).
var _last_selected := ""
var _tint_tick := 0
## Block name -> cell button for the current grid. Node *names* are not a
## stable key: a double `_rebuild` in one frame (boot + world load) makes Godot
## rename the new children to its auto-generated `@Button@N` names, so the
## checked cell is resolved through this map instead.
var _block_buttons: Dictionary = {}
## Requirement amount labels + their `(have, amount)` pairs, refreshed per frame.
var _top_amounts: Array[RichTextLabel] = []
var _top_requirements: Array[Dictionary] = []
## Last live re-read of the requirement `have` counts (msec).
var _top_refreshed_ms := 0
const TOP_REFRESH_MS := 250
## Hovered building tile (T6); `-1` while not over a building.
var _hover_tile := Vector2i(-1, -1)
## Live `Block.setBars` rows: `[[row_dict, MindBar], ...]` (T6).
var _hover_bars: Array = []
## Command-mode panel state (`PlacementFragment.commandTable`; H03 T3).
var _command_mode := false
var _command_json := ""
var _command_dirty := true
var _command_refresh_ms := 0
## `PlacementFragment` command-table re-read cadence (msec).
const COMMAND_REFRESH_MS := 250


func _ready() -> void:
	# `PlacementFragment.blockCatTable` is `full.bottom().right()` with
	# `visible(() -> ui.hudfrag.shown())`: the block pane is part of the standard
	# HUD, not a collapsed tab. The manifest bind owns the fragment root, so the
	# panel itself stays visible here and `close_palette()` remains for callers
	# that need the old collapsed behaviour.
	if panel != null:
		panel.visible = true
	if search != null:
		search.text_changed.connect(func(_value: String) -> void: _rebuild())
	_setup_top_style()
	_setup_command_style()
	_build_input_bar()
	# The HUD starts hidden in the menu; the catalogue is rebuilt on every reveal
	# so a sector launch or research that happened while it was hidden is
	# reflected instead of serving a stale category/block list.
	visibility_changed.connect(_ensure_loaded)
	# Category cycling (`,`/`.`) is dispatched by `MindInput`; the rail owns the
	# visible index and pushes it back.
	var input := get_node_or_null("/root/Spine/Input")
	if input != null and input.has_signal("hotkey") and not input.is_connected("hotkey", Callable(self, "_on_hotkey")):
		input.connect("hotkey", Callable(self, "_on_hotkey"))
	if (
		input != null
		and input.has_signal("selection_changed")
		and not input.is_connected("selection_changed", Callable(self, "_on_selection_changed"))
	):
		input.connect("selection_changed", Callable(self, "_on_selection_changed"))
	# `WorldLoadEvent` / `UnlockEvent`-with-a-block rebuild the palette
	# (`PlacementFragment.java:78-96`); both posts are deferred like Java's
	# `Core.app.post`.
	var host := get_node_or_null("/root/Spine/SimHost")
	if (
		host != null
		and host.has_signal("world_loaded")
		and not host.is_connected("world_loaded", Callable(self, "_on_world_loaded"))
	):
		host.connect("world_loaded", Callable(self, "_on_world_loaded"), CONNECT_DEFERRED)
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if (
		campaign != null
		and campaign.has_signal("block_unlocked")
		and not campaign.is_connected("block_unlocked", Callable(self, "_on_block_unlocked"))
	):
		campaign.connect(
			"block_unlocked", Callable(self, "_on_block_unlocked"), CONNECT_DEFERRED
		)
	_refresh_input_bar()
	_ensure_loaded()


## `PlacementFragment` commandTable update (`PlacementFragment.java:613-630`):
## the selection re-renders the unit/command/stance rows; block mode restores
## the block table (`mainStack` swap at `:443-453`).
func _on_selection_changed(_units_json: String) -> void:
	_command_dirty = true


## `Events.on(WorldLoadEvent)` (`PlacementFragment.java:78-84`, posted): the
## category resets to the distribution fallback, `control.input.block = null`,
## the per-category memory clears (`selectedBlocks`) and the catalogue re-reads
## even while the HUD is hidden.
func _on_world_loaded() -> void:
	_selected_category = -1
	_selected_blocks.clear()
	_select_block_by_name("air")
	_load_catalog()


## `Events.on(UnlockEvent)` when the content is a `Block`
## (`PlacementFragment.java:92-96`): re-read the catalogue so the new block
## appears in its category.
func _on_block_unlocked() -> void:
	_load_catalog()


## Campaign-gated icon-row visibility, change-cached per frame (`hud.campaign`
## refreshes on the `MindHud` cadence; `state.isCampaign()` upstream), plus the
## top info table refresh (`PlacementFragment` top table `update`).
func _process(_delta: float) -> void:
	var in_campaign := _in_campaign()
	if in_campaign != _bar_campaign:
		_bar_campaign = in_campaign
		_refresh_input_bar()
	_update_top_table()
	_update_selection_highlight()
	_update_command_panel()
	# `PlacementFragment.java:313-323` recolours every cell in a per-frame
	# `button.update`; the port batches the seam (`palette_tints_json`) on the
	# HUD poll cadence instead (10 frames) to keep the eval-free frame cheap.
	_tint_tick += 1
	if _tint_tick >= TINT_INTERVAL:
		_tint_tick = 0
		_update_palette_tints()


## Java `Color.gray` / `Color.darkGray` (`arc/graphics/Color.java:14-15`).
const TINT_WHITE := Color(1.0, 1.0, 1.0)
const TINT_GRAY := Color(0.498, 0.498, 0.498)
const TINT_DARK_GRAY := Color(0.247, 0.247, 0.247)
## Palette tint refresh cadence in frames (Java updates every frame).
const TINT_INTERVAL := 10


## `button.update` block half: white when the block is placeable, affordable
## (infinite resources or the player's core holds every scaled requirement) and
## the player unit is a builder, `Color.gray` otherwise, `Color.darkGray` when
## `Block.isPlaceable()` is false (`PlacementFragment.java:313-323`). Only the
## icon is tinted (`icon_*_color`), so the checked `selecti` accent frame keeps
## its colour like Java's ImageButton background.
func _update_palette_tints() -> void:
	if _block_buttons.is_empty():
		return
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("palette_tints_json"):
		return
	var parsed: Variant = JSON.parse_string(str(host.call("palette_tints_json")))
	if not (parsed is Dictionary):
		return
	var tints: Dictionary = parsed
	for name_variant in _block_buttons.keys():
		var button := _block_buttons[name_variant] as Button
		if button == null or not is_instance_valid(button):
			continue
		var color := TINT_GRAY
		match int(tints.get(str(name_variant), 1)):
			0:
				color = TINT_WHITE
			2:
				color = TINT_DARK_GRAY
			_:
				color = TINT_GRAY
		if button.get_theme_color("icon_normal_color") == color:
			continue
		for state in ["icon_normal_color", "icon_hover_color", "icon_pressed_color", "icon_disabled_color"]:
			button.add_theme_color_override(state, color)


## `PlacementFragment` per-frame `button.update`: the checked cell follows
## `control.input.block` even when the selection changed outside a palette
## click (`select_block_by_name`, hotkeys) — PlacementFragment.java:300-330.
func _update_selection_highlight() -> void:
	if blocks == null:
		return
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("selected_block"):
		return
	var selected := str(host.call("selected_block"))
	if selected == _last_selected:
		return
	_last_selected = selected
	var active := _block_buttons.get(selected) as BaseButton
	for button_variant in _block_buttons.values():
		var button := button_variant as BaseButton
		if button == null or not is_instance_valid(button):
			continue
		# `setPressedNoSignal`: the ButtonGroup must not re-fire `pressed`
		# (which would re-select through `_select_block`).
		button.set_pressed_no_signal(button == active)


## `Tex.buttonEdge2` ninepatch frame for the top table (splits 12, margin 5).
func _setup_top_style() -> void:
	if top_table == null:
		return
	var texture: Variant = MindWidgets.icon_texture("button-edge-2")
	if not (texture is Texture2D):
		return
	var box := StyleBoxTexture.new()
	box.texture = texture
	box.texture_margin_left = 12.0
	box.texture_margin_top = 12.0
	box.texture_margin_right = 12.0
	box.texture_margin_bottom = 12.0
	box.content_margin_left = 5.0
	box.content_margin_top = 5.0
	box.content_margin_right = 5.0
	box.content_margin_bottom = 5.0
	top_table.add_theme_stylebox_override("panel", box)


## `Tex.pane2` ninepatch for the command table (`commandTable = new
## Table(Tex.pane2)`, `PlacementFragment.java:444`; `pane-2` splits 12).
func _setup_command_style() -> void:
	if command_panel == null:
		return
	var texture: Variant = MindWidgets.icon_texture("pane-2")
	if not (texture is Texture2D):
		return
	var box := StyleBoxTexture.new()
	box.texture = texture
	box.texture_margin_left = 12.0
	box.texture_margin_top = 12.0
	box.texture_margin_right = 12.0
	box.texture_margin_bottom = 12.0
	command_panel.add_theme_stylebox_override("panel", box)


## `control.input.commandMode ? commandTable : blockCatTable`
## (`PlacementFragment.java:443-453`): swaps the block palette for the command
## table and re-reads `command_panel_json` at most every `COMMAND_REFRESH_MS`
## (or immediately after a selection change).
func _update_command_panel() -> void:
	if command_panel == null:
		return
	var input := get_node_or_null("/root/Spine/Input")
	var mode := false
	if input != null and input.has_method("command_mode"):
		mode = bool(input.call("command_mode"))
	if mode != _command_mode:
		_command_mode = mode
		if layout != null:
			layout.visible = not mode
		command_panel.visible = mode
		_command_dirty = true
		if mode and layout != null:
			# Java forces the command table to the block table width
			# (`commandTable.getWidth()` constraint, `:446`).
			command_panel.custom_minimum_size.x = layout.size.x
	if not mode:
		return
	var now := Time.get_ticks_msec()
	if not _command_dirty and now - _command_refresh_ms < COMMAND_REFRESH_MS:
		return
	_command_refresh_ms = now
	if input == null or not input.has_method("selection_json"):
		return
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("command_panel_json"):
		return
	var json := str(host.call("command_panel_json", str(input.call("selection_json"))))
	if json == _command_json and not _command_dirty:
		return
	_command_json = json
	_command_dirty = false
	_rebuild_command_panel(JSON.parse_string(json))


## `rebuildCommand` (`PlacementFragment.java:492-610`): title, accent rule,
## unit-type grid, command row and stance row from the probe JSON.
func _rebuild_command_panel(view: Variant) -> void:
	if command_rows == null:
		return
	for child in command_rows.get_children():
		child.queue_free()
	# code-instantiated: rows follow the live selection (type counts, command
	# and stance unions); the count is driven by the selection, never authored.
	if not (view is Dictionary):
		return
	var data: Dictionary = view
	var title := MindWidgets.label(MindWidgets.markup("@commandmode.name"))
	title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	command_rows.add_child(title)
	# `image().color(Pal.accent).growX().pad(20f).padTop(0f).padBottom(4f)`.
	var rule_margin := MarginContainer.new()
	rule_margin.add_theme_constant_override("margin_left", 20)
	rule_margin.add_theme_constant_override("margin_right", 20)
	rule_margin.add_theme_constant_override("margin_bottom", 4)
	var rule := ColorRect.new()
	rule.custom_minimum_size = Vector2(0, 4)
	rule.color = Color("ffd37f")
	rule.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	rule_margin.add_child(rule)
	command_rows.add_child(rule_margin)
	var types: Array = data.get("types", [])
	if types.is_empty():
		# `commandmode.nounits`, light grey, centred, pad 6 (`:611`).
		var none := MindWidgets.label(MindWidgets.markup("@commandmode.nounits"))
		none.modulate = Color(0.75, 0.75, 0.75)
		none.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		none.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		var pad := MarginContainer.new()
		pad.add_theme_constant_override("margin_left", 6)
		pad.add_theme_constant_override("margin_right", 6)
		pad.add_theme_constant_override("margin_top", 6)
		pad.add_theme_constant_override("margin_bottom", 6)
		pad.add_child(none)
		command_rows.add_child(pad)
		return
	var type_grid := GridContainer.new()
	type_grid.columns = 7
	for entry in types:
		if entry is Dictionary:
			type_grid.add_child(_command_type_cell(entry))
	command_rows.add_child(type_grid)
	var commands: Array = data.get("commands", [])
	var stances: Array = data.get("stances", [])
	if not commands.is_empty():
		command_rows.add_child(_command_button_grid(commands, true))
	if not stances.is_empty():
		if not commands.is_empty():
			# `image().color(Pal.gray).height(3).pad(7)` separator (`:592-594`).
			var sep_margin := MarginContainer.new()
			sep_margin.add_theme_constant_override("margin_left", 7)
			sep_margin.add_theme_constant_override("margin_right", 7)
			sep_margin.add_theme_constant_override("margin_top", 7)
			sep_margin.add_theme_constant_override("margin_bottom", 7)
			var separator := ColorRect.new()
			separator.custom_minimum_size = Vector2(0, 3)
			separator.color = Color("454545")
			separator.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			sep_margin.add_child(separator)
			command_rows.add_child(sep_margin)
		command_rows.add_child(_command_button_grid(stances, false))


## `StatValues.stack(type, count)` cell (`PlacementFragment.java:530-553`):
## 32px unit icon with the count at the bottom-right; left click keeps the
## type, right click drops it.
func _command_type_cell(entry: Dictionary) -> Control:
	var name := str(entry.get("name", ""))
	var count := int(entry.get("count", 0))
	var button := Button.new()
	button.custom_minimum_size = Vector2(42, 42)
	button.tooltip_text = _bundle("unit.%s.name" % name, name)
	button.icon = MindWidgets.icon_texture("unit-%s-ui" % name)
	button.expand_icon = true
	button.add_theme_constant_override("icon_max_width", 32)
	button.gui_input.connect(_on_command_type_input.bind(int(entry.get("type", -1))))
	# code-instantiated: the stack amount follows the live selection count.
	var amount := MindWidgets.label(str(count))
	amount.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_RIGHT)
	amount.mouse_filter = Control.MOUSE_FILTER_IGNORE
	button.add_child(amount)
	return button


## `b.clicked(KeyCode.mouseLeft/mouseRight)` (`PlacementFragment.java:537-548`).
func _on_command_type_input(event: InputEvent, type_id: int) -> void:
	if not (event is InputEventMouseButton):
		return
	var mouse := event as InputEventMouseButton
	if not mouse.pressed or type_id < 0:
		return
	var input := get_node_or_null("/root/Spine/Input")
	if input == null:
		return
	if mouse.button_index == MOUSE_BUTTON_LEFT and input.has_method("select_only_type"):
		input.call("select_only_type", type_id)
	elif mouse.button_index == MOUSE_BUTTON_RIGHT and input.has_method("deselect_type"):
		input.call("deselect_type", type_id)


## Command buttons (`:579-581`, 50x50, checked when any selected unit runs it)
## or stance buttons (`:602-603`, toggled).
func _command_button_grid(entries: Array, is_command: bool) -> GridContainer:
	var grid := GridContainer.new()
	grid.columns = 6
	for entry_variant in entries:
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		var name := str(entry.get("name", ""))
		var active := bool(entry.get("active", false))
		var label := _bundle(str(entry.get("key", "")), name)
		# `Icon.icons.get(command.icon, Icon.cancel)`: the Java field name maps
		# to the port's kebab-case alias; unknown icons fall back.
		# code-instantiated: one button per command/stance in the live union.
		var button := MindWidgets.glyph_button(_icon_alias(str(entry.get("icon", ""))), "?", "clearNoneTogglei")
		button.toggle_mode = true
		button.set_pressed_no_signal(active)
		button.custom_minimum_size = Vector2(50, 50)
		button.tooltip_text = label
		if is_command:
			button.pressed.connect(_set_command.bind(name))
		else:
			button.pressed.connect(_toggle_stance.bind(name, active))
		grid.add_child(button)
	return grid


## `Call.setUnitCommand(player, ids, command)` (`:579-581`).
func _set_command(name: String) -> void:
	var input := get_node_or_null("/root/Spine/Input")
	if input != null and input.has_method("set_unit_command"):
		input.call("set_unit_command", name)


## `Call.setUnitStance(player, ids, stance, !activeStances.get(...))` (`:602-603`).
func _toggle_stance(name: String, active: bool) -> void:
	var input := get_node_or_null("/root/Spine/Input")
	if input != null and input.has_method("set_unit_stance_named"):
		input.call("set_unit_stance_named", name, not active)


## Java `Icon` field name -> port glyph alias (`downOpen` -> `down-open`); empty
## when the port has no such glyph (`Icon.icons.get(..., Icon.cancel)`).
func _icon_alias(name: String) -> String:
	var alias := name.to_snake_case().replace("_", "-")
	if MindIcons.glyph(alias).is_empty():
		return "cancel"
	return alias


## `PlacementFragment` top table: selected block header + requirements, else the
## hovered displayable. Visibility = `hasInfoBox()`.
func _update_top_table() -> void:
	if top_table == null or top_rows == null:
		return
	var host := get_node_or_null("/root/Spine/SimHost")
	var display := ""
	if host != null and host.has_method("selected_block"):
		display = str(host.call("selected_block"))
		if display == "air" or display == "remove-wall" or display == "remove-ore":
			display = ""
	var hover: Dictionary = {}
	if display.is_empty() and host != null and host.has_method("hover_info_world"):
		# `PlacementFragment.hovered`: the world unit test uses the cursor's
		# world position (`Core.input.mouseWorldX/Y`), not the tile centre (T5).
		var world: Variant = _mouse_world()
		if world != null:
			var info: Variant = host.call("hover_info_world", world.x, world.y)
			if info is Dictionary and not str((info as Dictionary).get("kind", "")).is_empty():
				hover = info
	elif display.is_empty() and host != null and host.has_method("hover_info"):
		var tile: Variant = _mouse_tile()
		if tile != null:
			var info: Variant = host.call("hover_info", int(tile.x), int(tile.y))
			if info is Dictionary and not str((info as Dictionary).get("kind", "")).is_empty():
				hover = info
	# T6: the bar read needs the hovered tile (the hover dict carries no coords).
	_hover_tile = Vector2i(-1, -1)
	if not hover.is_empty() and str(hover.get("kind", "")) == "building":
		var tile: Variant = _mouse_tile()
		if tile != null:
			_hover_tile = tile
	if display.is_empty() and hover.is_empty():
		_set_top_visible(false)
		return
	_set_top_visible(true)
	var state := (
		"b:%s" % display
		if not display.is_empty()
		else
			"h:%s:%s:%d,%d"
			% [
				str(hover.get("kind", "")),
				str(hover.get("name", "")),
				_hover_tile.x,
				_hover_tile.y,
			]
	)
	if state == _top_state:
		if not display.is_empty():
			_refresh_top_amounts(display)
		elif not _hover_bars.is_empty():
			var now := Time.get_ticks_msec()
			if now - _top_refreshed_ms >= TOP_REFRESH_MS:
				_top_refreshed_ms = now
				_refresh_hover_bars()
		return
	_top_state = state
	_rebuild_top(display, hover)


## `visible(this::hasInfoBox)` on the top table and its `Pal.gray` rule.
func _set_top_visible(shown: bool) -> void:
	top_table.visible = shown
	if top_divider != null:
		top_divider.visible = shown


## The world tile under the cursor, or `null` while any UI holds the mouse
## (`Core.scene.hasMouse(...)` guard in `hovered()`). Only `STOP` controls
## consume the mouse; the HUD fragment root is a full-rect `PASS` control, so
## `PASS` must not hide the world under it.
func _mouse_tile() -> Variant:
	var hovered := get_tree().root.gui_get_hovered_control()
	if hovered != null and hovered.mouse_filter == Control.MOUSE_FILTER_STOP:
		return null
	var camera := get_node_or_null("/root/Spine/World/Camera2D")
	if camera == null or not camera.has_method("screen_to_tile"):
		return null
	var mouse := get_viewport().get_mouse_position()
	return camera.call("screen_to_tile", mouse.x, mouse.y)


## The cursor in world pixels (`Core.input.mouseWorldX/Y`), or `null` while any
## UI holds the mouse (same guard as `_mouse_tile`). T5: the hover unit test
## must use this point, not the tile centre.
func _mouse_world() -> Variant:
	var hovered := get_tree().root.gui_get_hovered_control()
	if hovered != null and hovered.mouse_filter == Control.MOUSE_FILTER_STOP:
		return null
	var camera := get_node_or_null("/root/Spine/World/Camera2D")
	if camera == null or not camera.has_method("screen_to_world"):
		return null
	var mouse := get_viewport().get_mouse_position()
	return camera.call("screen_to_world", mouse.x, mouse.y)


## Rebuilds the header + rows for a state change (`topTable.clear()` path).
func _rebuild_top(display: String, hover: Dictionary) -> void:
	for child in top_rows.get_children():
		child.queue_free()
	_top_amounts.clear()
	_top_requirements.clear()
	_hover_bars.clear()
	if display.is_empty():
		_build_hover_row(hover)
		return
	_build_block_header(display)
	_build_requirement_rows(display)
	_refresh_top_amounts()


## Header: 32px icon, 190px name label (`localizedName` + key combo), `?` button.
func _build_block_header(display: String) -> void:
	# code-instantiated: header cells are rebuilt per displayed block
	# (`topTable.clear()`; plan 14 §3.5).
	var header := HBoxContainer.new()
	header.add_theme_constant_override("separation", 4)
	var icon := TextureRect.new()
	icon.custom_minimum_size = Vector2(32, 32)
	# `.size(8 * 4)` + `Scaling.fit`: never the texture's native size.
	icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	icon.texture = MindWidgets.icon_texture("block-%s-ui" % display)
	if icon.texture == null:
		icon.texture = MindWidgets.icon_texture("block-%s-full" % display)
	icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	header.add_child(icon)
	var label := MindRichLabel.new()
	# Java `labelWrap(...).left().width(190f).padLeft(5)` (PlacementFragment
	# .java:371-395): the 190px label must fit both lines (name + key combo)
	# instead of clipping them; only the spacer expands.
	label.fit_content = true
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	label.custom_minimum_size = Vector2(190, 0)
	label.size_flags_horizontal = Control.SIZE_FILL
	label.text = MindWidgets.markup("@block.%s.name" % display) + _key_combo(display)
	header.add_child(label)
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	header.add_child(spacer)
	var info := MindWidgets.button("?")
	info.name = "blockinfo"
	info.custom_minimum_size = Vector2(40, 40)
	info.tooltip_text = _bundle("database", "Database")
	info.pressed.connect(_open_content_info.bind(display))
	header.add_child(info)
	top_rows.add_child(header)


## `?` button: `ui.content.show(displayBlock)` (`ContentInfoDialog`).
func _open_content_info(content_name: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("open_dialog"):
		ui.call("open_dialog", "content", JSON.stringify({"content": content_name}))


## Requirement rows: 16px item icon, item name, `have/need` amount
## (`PlacementFragment` refresh: `[scarlet]` under half, `[accent]` under, else
## `[white]`).
func _build_requirement_rows(display: String) -> void:
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("block_requirements_json"):
		return
	var parsed: Variant = JSON.parse_string(str(host.call("block_requirements_json", display)))
	if not (parsed is Array):
		return
	for entry_variant in parsed as Array:
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		# code-instantiated: one row per block requirement (content data).
		var line := HBoxContainer.new()
		line.add_theme_constant_override("separation", 2)
		var icon := TextureRect.new()
		icon.custom_minimum_size = Vector2(16, 16)
		icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		var item := str(entry.get("item", ""))
		icon.texture = MindWidgets.icon_texture("item-%s-ui" % item)
		icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		line.add_child(icon)
		var name_label := MindRichLabel.new()
		name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		name_label.text = MindWidgets.markup("@item.%s.name" % item)
		line.add_child(name_label)
		var amount_label := MindRichLabel.new()
		amount_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
		line.add_child(amount_label)
		top_rows.add_child(line)
		_top_amounts.append(amount_label)
		_top_requirements.append(entry)


## Refreshes requirement amounts (core inventory changes live; the Java label
## is a per-frame provider). Re-reads `have` from the sim at most every
## `TOP_REFRESH` seconds; `display` empty keeps the cached rows.
func _refresh_top_amounts(display: String = "") -> void:
	var now := Time.get_ticks_msec()
	if not display.is_empty() and now - _top_refreshed_ms >= TOP_REFRESH_MS:
		_top_refreshed_ms = now
		var host := get_node_or_null("/root/Spine/SimHost")
		if host != null and host.has_method("block_requirements_json"):
			var parsed: Variant = JSON.parse_string(str(host.call("block_requirements_json", display)))
			if parsed is Array and (parsed as Array).size() == _top_requirements.size():
				for index in _top_requirements.size():
					if (parsed as Array)[index] is Dictionary:
						_top_requirements[index] = (parsed as Array)[index]
	for index in _top_amounts.size():
		var entry: Dictionary = _top_requirements[index]
		var label := _top_amounts[index]
		# `PlacementFragment.java:407-416`: the sim host formats the Java markup
		# (`*/90` under infinite resources or without a core, else
		# `[scarlet|accent|white]amount[white]/need`).
		var raw := str(entry.get("label", ""))
		if not raw.is_empty():
			if label.get_meta("raw", "") != raw:
				label.set_meta("raw", raw)
				label.text = MindWidgets.markup_text(raw)
			continue
		var have := int(entry.get("have", 0))
		var need := int(entry.get("amount", 0))
		# `amount < stackamount / 2f ? [scarlet] : amount < stackamount ?
		# [accent] : [white]` (`Pal.remove`/`Pal.accent`).
		var color := "#ffffff"
		if float(have) < float(need) / 2.0:
			color = "#e55454"
		elif have < need:
			color = "#ffd37f"
		label.text = "[color=%s]%d[/color]/%d" % [color, have, need]


## Hover row: 32px icon + `Displayable` label (`Tile.display` name).
func _build_hover_row(hover: Dictionary) -> void:
	# code-instantiated: the hover row follows the cursor's target
	# (`topTable.clear()`; plan 14 §3.5).
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 4)
	var icon := TextureRect.new()
	icon.custom_minimum_size = Vector2(32, 32)
	# `.size(8 * 4)` + `Scaling.fit`: never the texture's native size.
	icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	var icon_name := str(hover.get("icon", ""))
	icon.texture = MindWidgets.icon_texture(icon_name)
	if icon.texture == null and not icon_name.is_empty():
		icon.texture = MindWidgets.icon_texture("block-%s-ui" % str(hover.get("name", "")))
	icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	row.add_child(icon)
	var label := MindRichLabel.new()
	label.custom_minimum_size = Vector2(190, 0)
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var label_key := str(hover.get("label", ""))
	if label_key.is_empty():
		label_key = "block.%s.name" % str(hover.get("name", ""))
	label.text = MindWidgets.markup("@%s" % label_key)
	row.add_child(label)
	top_rows.add_child(row)
	# T6: `BuildingComp.display`'s bar table (only for the player's buildings;
	# `building_bars_json` gates the team).
	if str(hover.get("kind", "")) == "building" and _hover_tile.x >= 0:
		_rebuild_hover_bars()


## `Block.setBars` rows from `building_bars_json` (`Block.java:709-760`): one
## 18px `MindBar` per row inside a 4px margin.
func _rebuild_hover_bars() -> void:
	_hover_bars.clear()
	var rows: Variant = _hover_bar_rows()
	if not (rows is Array):
		return
	for entry in rows:
		if not (entry is Dictionary):
			continue
		# code-instantiated: one bar per `Block.setBars` entry (T6).
		var margin := MarginContainer.new()
		margin.add_theme_constant_override("margin_left", 4)
		margin.add_theme_constant_override("margin_right", 4)
		margin.add_theme_constant_override("margin_top", 4)
		margin.add_theme_constant_override("margin_bottom", 4)
		var bar := MindBar.new()
		bar.custom_minimum_size = Vector2(0, 18)
		bar.setup(
			_bar_text(entry), _bar_color(entry), float((entry as Dictionary).get("value", 0.0))
		)
		margin.add_child(bar)
		top_rows.add_child(margin)
		_hover_bars.append([entry, bar])


## Live re-read of the bar rows (`TOP_REFRESH_MS`): fractions every tick, text
## when the formatted argument changed.
func _refresh_hover_bars() -> void:
	var rows: Variant = _hover_bar_rows()
	if not (rows is Array) or rows.size() != _hover_bars.size():
		for pair in _hover_bars:
			(pair[1] as Node).queue_free()
		_hover_bars.clear()
		_rebuild_hover_bars()
		return
	for i in rows.size():
		var entry: Dictionary = rows[i]
		var pair: Array = _hover_bars[i]
		var bar: MindBar = pair[1]
		bar.fraction = float(entry.get("value", 0.0))
		if str(entry.get("label_arg", "")) != str((pair[0] as Dictionary).get("label_arg", "")):
			bar.setup(_bar_text(entry), _bar_color(entry), float(entry.get("value", 0.0)))
			_hover_bars[i] = [entry, bar]


func _hover_bar_rows() -> Variant:
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("building_bars_json") or _hover_tile.x < 0:
		return null
	return JSON.parse_string(
		str(host.call("building_bars_json", _hover_tile.x, _hover_tile.y))
	)


func _bar_text(entry: Dictionary) -> String:
	var key := str(entry.get("label_key", ""))
	var arg: Variant = entry.get("label_arg", null)
	if arg != null:
		return MindWidgets.markup(_bundle(key, key).replace("{0}", str(arg)))
	return MindWidgets.markup("@%s" % key)


func _bar_color(entry: Dictionary) -> Color:
	return Color(str(entry.get("color", "ffffffff")))


## `PlacementFragment` key combo: category hotkey + row/column digits from the
## `block_select_01..10` binds (`placement.blockselectkeys`).
func _key_combo(display: String) -> String:
	var list: Array = _catalog.get("categories", [])
	if _selected_category < 0 or _selected_category >= list.size():
		return ""
	var category: Dictionary = list[_selected_category]
	var blocks_list: Array = category.get("blocks", [])
	var index := -1
	for i in blocks_list.size():
		if str((blocks_list[i] as Dictionary).get("name", "")) == display:
			index = i
			break
	if index < 0:
		return ""
	var binds := _block_select_displays()
	var displayable := 0
	for bind in binds:
		if not bind.is_empty():
			displayable += 1
	if displayable < 10:
		if not _warned_block_select:
			_warned_block_select = true
			push_warning(
				"placement: block_select_01..10 keybinds incomplete (%d/10 displayable); key combo line omitted"
				% displayable
			)
		return ""
	var ordinal := int(category.get("ordinal", 0))
	var category_key: String = binds[mini(ordinal, 9)]
	# `Core.bundle.format("placement.blockselectkeys", categoryKey)` +
	# `(i < 10 ? "" : blockSelect[(i + 1) / 10 - 1] + ",") + blockSelect[i % 10]
	# + "]"` (PlacementFragment.java:371-395).
	var fmt := _bundle("placement.blockselectkeys", "\n[lightgray]Key: [{0},")
	var combo := fmt.replace("{0}", category_key)
	if index >= 10:
		combo += "%s," % binds[mini((index + 1) / 10 - 1, 9)]
	combo += "%s]" % binds[index % 10]
	return MindWidgets.markup(combo)


## `block_select_01..10` key display strings from the input binding registry.
func _block_select_displays() -> Array[String]:
	var out: Array[String] = []
	var input := get_node_or_null("/root/Spine/Input")
	if input == null or not input.has_method("keybinds_json"):
		return out
	var parsed: Variant = JSON.parse_string(str(input.call("keybinds_json")))
	if not (parsed is Array):
		return out
	var by_name: Dictionary = {}
	for entry_variant in parsed as Array:
		if entry_variant is Dictionary:
			by_name[str((entry_variant as Dictionary).get("name", ""))] = str(
				(entry_variant as Dictionary).get("display", "")
			)
	for i in range(1, 11):
		out.append(str(by_name.get("block_select_%02d" % i, "")))
	return out


## `InputHandler.buildPlacementUI` icon row: 48px glyph buttons left-aligned
## under the pane; the dialog routing mirrors the menu's `MindUi.open_dialog`.
func _build_input_bar() -> void:
	if input_bar == null or not _input_buttons.is_empty():
		return
	for entry in INPUT_BUTTONS:
		# code-instantiated: fixed upstream icon row (four desktop buttons).
		var label := _bundle(str(entry.get("tooltip", "")), str(entry.get("icon", "")))
		var button := MindWidgets.glyph_button(str(entry.get("icon", "")), label, "clearNonei")
		button.name = "input-%s" % str(entry.get("icon", ""))
		button.tooltip_text = label
		button.custom_minimum_size = Vector2(48, 48)
		button.set_meta("campaign_only", bool(entry.get("campaign_only", false)))
		button.pressed.connect(_open_input_dialog.bind(str(entry.get("dialog", ""))))
		input_bar.add_child(button)
		_input_buttons.append(button)


## `visible(() -> state.isCampaign())` on the research/planet buttons.
func _refresh_input_bar() -> void:
	var in_campaign := _in_campaign()
	for button in _input_buttons:
		if bool(button.get_meta("campaign_only", false)):
			button.visible = in_campaign


## `state.isCampaign()` through the read-only `MindHud` facade.
func _in_campaign() -> bool:
	var hud := get_node_or_null("/root/MindHud")
	return hud != null and bool(hud.get("campaign"))


func _open_input_dialog(dialog_name: String) -> void:
	if dialog_name.is_empty():
		return
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("open_dialog"):
		ui.call("open_dialog", dialog_name, "{}")



## Opens the collapsed block-select pane (`blocksSelect` collapser expanded).
func open_palette() -> void:
	if panel != null:
		panel.visible = true
	visible = true
	_ensure_loaded()


## Collapses the block-select pane back to the HUD.
func close_palette() -> void:
	if panel != null:
		panel.visible = false
	_ensure_loaded()


## `blockSelect[0..9]` action names -> the Java `Category.all` digit index
## (`PlacementFragment.java:60-74`).
const DIGIT_ACTIONS := {
	"block_select_01": 0, "block_select_02": 1, "block_select_03": 2,
	"block_select_04": 3, "block_select_05": 4, "block_select_06": 5,
	"block_select_07": 6, "block_select_08": 7, "block_select_09": 8,
	"block_select_10": 9,
}

## `blockSelect[10..13]` arrow actions -> the `gridUpdate` direction index.
const ARROW_ACTIONS := {
	"block_select_left": 10, "block_select_right": 11,
	"block_select_up": 12, "block_select_down": 13,
}

## `Time.timeSinceMillis(blockSelectSeqMillis) > 400` combo window.
const BLOCK_SELECT_COMBO_MS := 400

## `PlacementFragment.blockSelectEnd`/`blockSelectSeq`/`blockSelectSeqMillis`
## (`:55-57`); the window is wall-clock.
var _block_select_end := true
var _block_select_seq := 0
var _block_select_ms := 0
## `PlacementFragment.selectedBlocks`: category name -> remembered block name.
var _selected_blocks: Dictionary = {}


## `PlacementFragment.gridUpdate` (`:177-273`) hotkey half. The digits, arrows
## and category cycles are dispatched as `MindInput` hotkeys (the Java fragment
## owns them; `DesktopInput` only routes the binding edges here) and run only
## outside command mode (`:745`).
func _on_hotkey(action: String) -> void:
	if not _command_mode:
		var digit := int(DIGIT_ACTIONS.get(action, -1))
		if digit >= 0:
			_block_select_digit(digit)
			return
		var arrow := int(ARROW_ACTIONS.get(action, -1))
		if arrow >= 0:
			_block_select_arrow(arrow)
			return
		if action == "category_prev" or action == "category_next":
			_category_step(-1 if action == "category_prev" else 1)
			return
	if action == "pick":
		# `updatePick`'s `currentCategory = input.block.category` (`:166-168`).
		_sync_category_to_selected()


## `gridUpdate` digits (`PlacementFragment.java:208-236`): the first number of a
## combo picks `Category.all[i]`, the second/third pick the block index
## (`X,1,0` == `X,0`).
func _block_select_digit(digit: int) -> void:
	var now := Time.get_ticks_msec()
	if _block_select_end or now - _block_select_ms > BLOCK_SELECT_COMBO_MS:
		var index := _category_index_by_ordinal(digit)
		if index < 0:
			# Empty category (`!getUnlockedByCategory(Category.all[i]).isEmpty()`).
			return
		# `if(input.block != null) input.block = getSelectedBlock(currentCategory)`.
		_select_category(index, not _block_selected_name().is_empty())
		_block_select_end = false
		_block_select_seq = 0
		_block_select_ms = now
		return
	var target := digit
	if _block_select_seq == 0:
		_block_select_seq = digit + 1
	else:
		target += (_block_select_seq - (0 if digit != 9 else 1)) * 10
		_block_select_end = true
	var entry := _current_entry()
	var names := _block_names(entry)
	if target >= names.size():
		return
	# `selectedBlocks.put(currentCategory, input.block)`.
	_selected_blocks[str(entry.get("name", ""))] = names[target]
	_select_block_by_name(names[target])
	_block_select_ms = Time.get_ticks_msec()


## `gridUpdate` arrows (`PlacementFragment.java:189-207`): the grid position of
## the current selection in the category's (unlock-filtered) list.
func _block_select_arrow(arrow: int) -> void:
	var entry := _current_entry()
	var names := _block_names(entry)
	var count := names.size()
	if count <= 0:
		return
	var at := names.find(_block_selected_name())
	if at < 0:
		return
	var target := at
	match arrow:
		10:
			target = (at - 1 + count) % count
		11:
			target = (at + 1) % count
		12:
			target = at - 4 if at > 3 else count - count % 4 + at
			if target >= count:
				target -= 4
		13:
			target = at + 4 if at < count - 4 else at % 4
	_selected_blocks[str(entry.get("name", ""))] = names[target]
	_select_block_by_name(names[target])


## `PlacementFragment.categoryPrev/Next` (`:240-258`): cycle the non-empty
## categories and select `getSelectedBlock(new)` even with no prior pick.
func _category_step(delta: int) -> void:
	var list: Array = _catalog.get("categories", [])
	if list.is_empty():
		return
	if panel == null or not panel.visible:
		open_palette()
	_selected_category = wrapi(_selected_category + delta, 0, list.size())
	_build_categories()
	_rebuild()
	var remembered := _get_selected_block(_current_entry())
	if not remembered.is_empty():
		_select_block_by_name(remembered)


## `updatePick`'s category switch for a successful middle-click pick: the rail
## follows the picked block's category (`:166-168`).
func _sync_category_to_selected() -> void:
	var name := _block_selected_name()
	if name.is_empty():
		return
	var list: Array = _catalog.get("categories", [])
	for index in list.size():
		var entry: Dictionary = list[index]
		if not _block_names(entry).has(name):
			continue
		_selected_category = index
		_selected_blocks[str(entry.get("name", ""))] = name
		_build_categories()
		_rebuild()
		return


## `control.input.block` as reported by the sim host (`""` when none).
func _block_selected_name() -> String:
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("selected_block"):
		return ""
	var name := str(host.call("selected_block"))
	return "" if name == "air" else name


## The live catalogue entry for `_selected_category`.
func _current_entry() -> Dictionary:
	var list: Array = _catalog.get("categories", [])
	if list.is_empty() or _selected_category < 0 or _selected_category >= list.size():
		return {}
	return list[_selected_category] as Dictionary


## Article names of one catalogue entry, in content order
## (`getUnlockedByCategory` order).
func _block_names(entry: Dictionary) -> PackedStringArray:
	var out := PackedStringArray()
	for block_variant in entry.get("blocks", []):
		out.append(str((block_variant as Dictionary).get("name", "")))
	return out


## Catalogue index of the entry with `Category.ordinal() == ordinal` (`-1` when
## the category is empty and therefore absent).
func _category_index_by_ordinal(ordinal: int) -> int:
	var list: Array = _catalog.get("categories", [])
	for index in list.size():
		if int((list[index] as Dictionary).get("ordinal", -1)) == ordinal:
			return index
	return -1


## `PlacementFragment.getSelectedBlock` (`:775-777`): the remembered block of
## the category, else its first block (the catalogue is unlock-filtered, so the
## head is the first unlocked block).
func _get_selected_block(entry: Dictionary) -> String:
	if entry.is_empty():
		return ""
	var name := str(entry.get("name", ""))
	if _selected_blocks.has(name):
		return str(_selected_blocks[name])
	var names := _block_names(entry)
	return names[0] if not names.is_empty() else ""


## Selects a block by content name through `MindInput` (`control.input.block`).
func _select_block_by_name(name: String) -> void:
	if name.is_empty():
		return
	_select_block(get_node_or_null("/root/Spine/SimHost"), name)


## Switches the visible category and re-selects its remembered block
## (rail click `:730-736`, digit first number `:213-218`).
func _select_category(index: int, select_block: bool) -> void:
	_selected_category = index
	_build_categories()
	_rebuild()
	if not select_block:
		return
	var remembered := _get_selected_block(_current_entry())
	if not remembered.is_empty():
		_select_block_by_name(remembered)


func _ensure_loaded() -> void:
	if not is_visible_in_tree():
		return
	# `Events.on(WorldLoadEvent) -> currentCategory = Category.distribution`.
	var host := get_node_or_null("/root/Spine/SimHost")
	if host != null and host.has_method("get_tick"):
		var tick := int(host.call("get_tick"))
		if tick < _last_tick:
			_selected_category = -1
		_last_tick = tick
	_load_catalog()


## Pulls the block catalogue from `MindUi` and paints the rail + grid.
func _load_catalog() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("block_catalog_json"):
		var parsed: Variant = JSON.parse_string(str(ui.call("block_catalog_json")))
		if parsed is Dictionary:
			_catalog = parsed
	_build_categories()
	_rebuild()


## Resolves a bundle key through `MindAssets`, falling back to `fallback` when the
## key is missing (the bundle echoes the key) or the autoload is absent.
func _bundle(key: String, fallback: String) -> String:
	if key.is_empty():
		return fallback
	var assets := get_node_or_null("/root/MindAssets")
	if assets == null:
		return fallback
	var value := str(assets.call("bundle_get", key))
	if value.is_empty() or value == key:
		return fallback
	return value


func _build_categories() -> void:
	if categories == null:
		return
	for child in categories.get_children():
		child.queue_free()
	# code-instantiated: category tabs are the data-driven block-category list
	# (plan 02); the count is runtime content, not authorable in the scene.
	var list: Array = _catalog.get("categories", [])
	if list.is_empty():
		return
	if _selected_category < 0 or _selected_category >= list.size():
		_selected_category = 0
		for index in list.size():
			if str((list[index] as Dictionary).get("name", "")) == "distribution":
				_selected_category = index
				break
	# `ButtonGroup`: exactly the active category is checked (`rebuildCategory`).
	var group := ButtonGroup.new()
	for index in list.size():
		var entry: Dictionary = list[index]
		var cat_name := str(entry.get("name", ""))
		var label := _bundle(str(entry.get("label", "")), cat_name)
		var button := MindWidgets.glyph_button(cat_name, label, "clearTogglei")
		button.name = "category-%s" % cat_name
		button.tooltip_text = label
		button.toggle_mode = true
		button.button_group = group
		button.button_pressed = index == _selected_category
		button.custom_minimum_size = Vector2(50, 50)
		button.pressed.connect(_select.bind(index))
		categories.add_child(button)


## Rail click (`PlacementFragment.java:730-736`): switch the category and, when
## a block is already picked, re-select that category's remembered block.
func _select(index: int) -> void:
	_select_category(index, not _block_selected_name().is_empty())


## Selects a block through `MindInput` so the placement rotation/state stay in
## sync (falls back to the sim host when the input node is absent).
func _select_block(host: Node, name: String) -> void:
	var input := get_node_or_null("/root/Spine/Input")
	if input != null and input.has_method("select_block_by_name"):
		input.call("select_block_by_name", name)
	elif host != null:
		host.call("select_block", name)


func _rebuild() -> void:
	if blocks == null:
		return
	for child in blocks.get_children():
		child.queue_free()
	_block_buttons.clear()
	# A rebuild re-creates the cells; force the next frame's highlight pass to
	# re-apply instead of trusting the cached selection.
	_last_selected = "\u0000"
	# code-instantiated: block buttons come from the content catalogue filtered by
	# category/search (data-driven, plan 02/07).
	var list: Array = _catalog.get("categories", [])
	if list.is_empty() or _selected_category < 0 or _selected_category >= list.size():
		return
	var category: Dictionary = list[_selected_category]
	var query := ""
	if search != null:
		query = search.text.to_lower()
	var host := get_node_or_null("/root/Spine/SimHost")
	var selected := ""
	if host != null and host.has_method("selected_block"):
		selected = str(host.call("selected_block"))
	# `ButtonGroup`: the active block is the checked button (`button.group(group)`).
	var group := ButtonGroup.new()
	for block_variant in category.get("blocks", []):
		var block: Dictionary = block_variant
		var name := str(block.get("name", ""))
		var label := _bundle(str(block.get("localized", "")), name)
		if not query.is_empty():
			if not name.to_lower().contains(query) and not label.to_lower().contains(query):
				continue
		var button := MindWidgets.image_button_first(
			PackedStringArray(["block-%s-ui" % name, "block-%s-full" % name, name]), "selecti"
		)
		button.name = "block-%s" % name
		button.tooltip_text = label
		button.toggle_mode = true
		button.button_group = group
		button.button_pressed = name == selected
		button.custom_minimum_size = Vector2(46, 46)
		# `Button.resizeImage(iconMed)`: 46px cell, 32px centred icon.
		button.expand_icon = true
		button.add_theme_constant_override("icon_max_width", 32)
		if button.icon == null:
			button.text = label
		button.pressed.connect(func() -> void:
			# `selectedBlocks.put(currentCategory, input.block)` (`:307`).
			_selected_blocks[str(category.get("name", ""))] = name
			_select_block(host, name))
		blocks.add_child(button)
		_block_buttons[name] = button
