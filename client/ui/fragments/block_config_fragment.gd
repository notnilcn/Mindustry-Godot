## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/BlockConfigFragment.java
##         (plan 14 §3.5, M4).
##
## Scale-in configuration panel anchored to a building's tile. The spec JSON is
## produced by `MindInput` (`DesktopBridge::config_spec`): the block title, the
## tile, and the selectable options (item/liquid content names) plus a `text`
## flag for code-configurable blocks. Submitting routes through
## `MindInput.configure_building` (a sim command; GDScript never mutates sim
## state).

extends Control

## Scene size of the titled panel (`block_config_fragment.tscn`) used by the
## option/text/link/colour specs.
const PANEL_SIZE := Vector2(300, 220)
## `BlockConfigFragment.showConfig` scale-in (`Actions.scaleTo(1f, 1f, 0.07f,
## Interp.pow3Out)`).
const SCALE_IN_SECONDS := 0.07

var _config: Dictionary = {}
## Tapped tile of the configured building (`config.getSelected()`).
var _tile := Vector2i(-1, -1)
var _scale_tween: Tween = null
## Launch-pad destination planet (the spec's `planet`, `LaunchPad.java:192`).
var _launch_planet := ""

@onready var panel: PanelContainer = get_node_or_null("Panel")
@onready var title: Label = get_node_or_null("Panel/Layout/Title")
@onready var body: VBoxContainer = get_node_or_null("Panel/Layout/Body")


func _ready() -> void:
	visible = false


## Opens the panel for a building (`ConfigUiSpec` JSON from `MindInput`).
## `world_pos` (the tap point) is only the fallback anchor: the table follows
## the building every frame like `Building.updateTableAlign`.
func configure(world_pos: Vector2, spec_json: String) -> void:
	position = world_pos
	var parsed: Variant = JSON.parse_string(spec_json)
	_config = parsed if parsed is Dictionary else {}
	_tile = Vector2i(int(_config.get("x", -1)), int(_config.get("y", -1)))
	var frameless := _frameless()
	if title != null:
		title.text = _t(str(_config.get("title", "")), str(_config.get("block", "")))
		title.visible = not frameless
	if panel != null:
		# `BlockConfigFragment.showConfig`: `table.background(null)` — blocks
		# whose `buildConfiguration` only adds buttons show just the buttons,
		# and the `ItemSelection` grid carries its own `Styles.black6` table.
		if frameless:
			panel.add_theme_stylebox_override("panel", StyleBoxEmpty.new())
		else:
			panel.remove_theme_stylebox_override("panel")
	_rebuild()
	size = panel.get_combined_minimum_size() if frameless and panel != null else PANEL_SIZE
	_align_to_building()
	visible = true
	# `table.setOrigin(Align.center)` + `scaleTo(0f, 1f)` -> `scaleTo(1f, 1f,
	# 0.07f, Interp.pow3Out)`.
	pivot_offset = size / 2.0
	scale = Vector2(0.0, 1.0)
	if _scale_tween != null and _scale_tween.is_valid():
		_scale_tween.kill()
	_scale_tween = create_tween()
	_scale_tween.tween_property(self, "scale", Vector2.ONE, SCALE_IN_SECONDS).set_trans(
		Tween.TRANS_CUBIC
	).set_ease(Tween.EASE_OUT)


## `LogicBlock`/`CanvasBlock.buildConfiguration` add a single pencil button
## (`LogicBlock.java:737-739`, `CanvasBlock.java:314-317`): no title, no
## background pane, no close button.
func _buttons_only() -> bool:
	return bool(_config.get("logic", false)) or bool(_config.get("canvas", false))


## Panels that draw no title and no own pane background: the buttons-only
## configurations above and the `ItemSelection` grid / unit-command row (J05
## T1/T2, whose `Styles.black6` backgrounds sit inside the widgets).
func _frameless() -> bool:
	return (
		_buttons_only()
		or bool(_config.get("selection", false))
		or _config.get("commands", null) is Dictionary
		or bool(_config.get("launch", false))
	)


func _process(_delta: float) -> void:
	if not visible:
		return
	# `table.pack()`: the frameless table is exactly its contents.
	if _frameless() and panel != null:
		size = panel.get_combined_minimum_size()
		pivot_offset = size / 2.0
	_align_to_building()


## `Building.updateTableAlign` (`BuildingComp.java:1682-1685`): the table's
## top-centre sits at `mouseScreen(x, y - block.size * tilesize / 2f - 1)`, i.e.
## just under the building, every frame. A building that is gone hides the
## table (`BlockConfigFragment.showConfig` update: `!selected.isValid()`).
func _align_to_building() -> void:
	if _tile.x < 0 or _tile.y < 0:
		return
	var host := get_node_or_null("/root/Spine/SimHost")
	var camera := get_node_or_null("/root/Spine/World/Camera2D")
	if host == null or camera == null or not host.has_method("building_center_world"):
		return
	if not camera.has_method("world_to_screen"):
		return
	var center: Variant = host.call("building_center_world", _tile.x, _tile.y)
	if not (center is Vector2):
		if visible:
			_close()
		return
	var block_size := 1
	if host.has_method("block_size"):
		block_size = maxi(1, int(host.call("block_size", _tile.x, _tile.y)))
	var anchor := center as Vector2
	var screen: Variant = camera.call(
		"world_to_screen", anchor.x, anchor.y - float(block_size) * 4.0 - 1.0
	)
	if screen is Vector2:
		var point := screen as Vector2
		position = Vector2(point.x - size.x / 2.0, point.y)


## Resolves a bundle key through `MindAssets` with a raw fallback.
func _t(key: String, fallback: String) -> String:
	var resolved := key.trim_prefix("@")
	if resolved.is_empty():
		return fallback
	var assets := get_node_or_null("/root/MindAssets")
	if assets == null:
		return fallback
	var value := str(assets.call("bundle_get", resolved))
	if value.is_empty() or value == resolved:
		return fallback
	return value


func _rebuild() -> void:
	if body == null:
		return
	# Detach before freeing so the panel's minimum size (the buttons-only table
	# size) no longer counts the previous rows this frame.
	for child in body.get_children():
		body.remove_child(child)
		child.queue_free()
	# Separation resets per route: Java stacks the grid and the command row with
	# no table spacing (`table.row()`, `UnitFactory.java:291`) while the other
	# panels keep the themed spacing.
	body.remove_theme_constant_override("separation")
	# code-instantiated: config rows come from the `config_spec` JSON (item or
	# liquid option list / logic code field / color picker) produced by
	# `MindInput`; the count is runtime content, not authorable in the scene.
	if bool(_config.get("link", false)):
		_build_link_panel()
	elif bool(_config.get("color", false)):
		_build_color_picker()
	elif bool(_config.get("logic", false)):
		_build_logic_panel()
	elif bool(_config.get("canvas", false)):
		_build_canvas_panel()
	elif bool(_config.get("text", false)):
		_build_text_editor()
	elif bool(_config.get("launch", false)):
		_build_launch_panel()
	elif bool(_config.get("selection", false)) or _config.get("commands", null) is Dictionary:
		body.add_theme_constant_override("separation", 0)
		_build_selection()
	else:
		_build_options()


## `LogicBlock.buildConfiguration` (`LogicBlock.java:736-739`): one 40x40
## `Icon.pencil` button with `Styles.cleari` opening the logic editor. The
## pencil is an icon-font glyph (`MindIcons`), like every upstream `Icon.*`.
func _build_logic_panel() -> void:
	# code-instantiated: Java builds this single pencil button per processor at
	# tap time; it has no scene-authorable per-building shape.
	var pencil := MindWidgets.glyph_button("pencil", "edit", "cleari")
	pencil.custom_minimum_size = Vector2(40, 40)
	pencil.tooltip_text = _t("@edit", "edit")
	pencil.pressed.connect(_open_logic_dialog)
	pencil.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	body.add_child(pencil)


## `CanvasBlock.buildConfiguration` (`CanvasBlock.java:314-317`): one 40x40
## `Icon.pencil` button opening `CanvasEditDialog`.
func _build_canvas_panel() -> void:
	# code-instantiated: Java builds this single pencil button per canvas at tap
	# time; it has no scene-authorable per-building shape.
	var pencil := MindWidgets.glyph_button("pencil", "edit", "cleari")
	pencil.custom_minimum_size = Vector2(40, 40)
	pencil.tooltip_text = _t("@edit", "edit")
	pencil.pressed.connect(_open_canvas_dialog)
	pencil.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	body.add_child(pencil)


## Opens the canvas editor (`CanvasEditDialog`; T14).
func _open_canvas_dialog() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("open_dialog"):
		return
	var x := int(_config.get("x", 0))
	var y := int(_config.get("y", 0))
	ui.call("open_dialog", "canvas_edit", JSON.stringify({"x": x, "y": y}))


## Opens the logic editor dialog for this processor (T7). Until the dialog is
## registered `open_dialog` returns false and nothing happens.
func _open_logic_dialog() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("open_dialog"):
		return
	var x := int(_config.get("x", 0))
	var y := int(_config.get("y", 0))
	ui.call("open_dialog", "logic", JSON.stringify({"x": x, "y": y}))


## H02 T6: link blocks (bridges, conduits, drivers, power nodes) show no
## option list; tapping another block of the kind while this panel is open
## makes the link. The current link offset is rendered read-only.
func _build_link_panel() -> void:
	# code-instantiated: one current-link row driven by the spec's `current`
	# value; it has no scene-authorable per-building shape.
	var current: Variant = _config.get("current", null)
	var label := Label.new()
	if current is Dictionary and str(current.get("type", "")) == "point":
		label.text = "(%s, %s)" % [str(current.get("x", 0)), str(current.get("y", 0))]
	else:
		label.text = _t("@none", "none")
	body.add_child(label)
	var row := HBoxContainer.new()
	body.add_child(row)
	_add_close_button(row)


func _build_text_editor() -> void:
	# `MessageBlock.buildConfiguration` (`MessageBlock.java:109-154`, desktop):
	# one 40 px `Icon.pencil` `Styles.cleari` button; it opens the
	# `@editmessage` floating dialog (380x160 TextArea, `maxTextLength`,
	# `maxNewlines`, `n / max` counter, `@ok` 130x60) and `deselect()`s.
	# L05 T2 (PAR-0380): the inline field had no counter or newline cap.
	if not bool(_config.get("multiline", false)):
		_build_inline_text()
		return
	# code-instantiated: one pencil button per message block at tap time.
	var pencil := MindWidgets.glyph_button("pencil", "edit", "cleari")
	pencil.custom_minimum_size = Vector2(40, 40)
	pencil.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	pencil.pressed.connect(_open_message_edit)
	body.add_child(pencil)


func _open_message_edit() -> void:
	var current := str(_config.get("current", ""))
	var max_text := maxi(1, int(_config.get("max_text", 400)))
	var max_newlines := maxi(0, int(_config.get("max_newlines", 24)))
	var root := get_node_or_null("/root/Spine/Ui/UiRoot")
	_close()
	if root == null or not root.has_method("show_message_edit"):
		return
	root.call("show_message_edit", current, max_text, max_newlines, func(text: String) -> void:
		# `if(!a.getText().contentEquals(message)) configure(a.getText())`.
		if text != current:
			_submit("string", text))


## Single-line text config (no Java block uses it today; kept for the spec).
func _build_inline_text() -> void:
	var line := LineEdit.new()
	line.text = str(_config.get("current", ""))
	line.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	body.add_child(line)
	var row := HBoxContainer.new()
	body.add_child(row)
	var apply := Button.new()
	apply.text = _t("@ok", "OK")
	apply.pressed.connect(func() -> void: _submit("string", line.text))
	row.add_child(apply)
	_add_close_button(row)


## `LaunchPad.buildConfiguration` (`LaunchPad.java:185-203`): one 40 px
## `Icon.upOpen` button opening the sector-destination picker; the choice writes
## `rules.sector.info.destination` (`MindCampaign.set_launch_destination`) and
## closes the panel (`deselect()`).
func _build_launch_panel() -> void:
	# code-instantiated: one launch button per launch pad.
	var button := MindWidgets.glyph_button("up-open", "▲", "cleari")
	button.custom_minimum_size = Vector2(40, 40)
	button.tooltip_text = _t("@launch.select", "select destination")
	button.pressed.connect(_open_launch_select)
	body.add_child(button)


func _open_launch_select() -> void:
	_launch_planet = str(_config.get("planet", ""))
	_close()
	var ui := get_node_or_null("/root/MindUi")
	var root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui == null:
		return
	if root != null and root.has_method("dialog"):
		var dialog: Node = root.call("dialog", "sector_select")
		if dialog != null and dialog.has_signal("sector_chosen"):
			# The launch-loadout flow keeps its own handler on this signal; the
			# launch pad owns the choice while its dialog is up.
			for connection in dialog.sector_chosen.get_connections():
				dialog.sector_chosen.disconnect(connection["callable"])
			if not dialog.sector_chosen.is_connected(_on_launch_destination_chosen):
				dialog.sector_chosen.connect(_on_launch_destination_chosen)
	ui.call("open_dialog", "sector_select", JSON.stringify({"planet": _launch_planet}))


func _on_launch_destination_chosen(sector: int) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("close_dialog", "sector_select")
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign != null and campaign.has_method("set_launch_destination"):
		campaign.call("set_launch_destination", _launch_planet, sector)


## Reads the submitted text from a `LineEdit` or `TextEdit`.
func _text_of(field: Control) -> String:
	if field is TextEdit:
		return (field as TextEdit).text
	if field is LineEdit:
		return (field as LineEdit).text
	return ""


## `LightBlock.buildConfiguration` (`LightBlock.java:109-114`): one 40 px
## `Icon.pencil` `Styles.cleari` button; it opens `ui.picker.show(color.a(0.5),
## false, res -> configure(res.rgba()))` and `deselect()`s (L05 T2, PAR-0380).
func _build_color_picker() -> void:
	# code-instantiated: one pencil button per illuminator at tap time.
	var pencil := MindWidgets.glyph_button("pencil", "edit", "cleari")
	pencil.custom_minimum_size = Vector2(40, 40)
	pencil.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	pencil.pressed.connect(_open_color_picker)
	body.add_child(pencil)


func _open_color_picker() -> void:
	var color := _rgba_from_int(int(_config.get("current", 0xffd37fff)))
	color.a = 0.5
	var x := int(_config.get("x", 0))
	var y := int(_config.get("y", 0))
	var root := get_node_or_null("/root/Spine/Ui/UiRoot")
	_close()
	var picker: Node = root.call("dialog", "picker") if root != null and root.has_method("dialog") else null
	if picker == null or not picker.has_method("show_picker"):
		return
	picker.call("show_picker", color, false, func(result: Color) -> void:
		var input := get_node_or_null("/root/Spine/Input")
		if input != null and input.has_method("configure_building"):
			# `res.rgba()` truncates like Arc (`(int)(c * 255)`).
			var packed: int = load("res://ui/dialogs/color_picker.gd").arc_rgba(result)
			input.call("configure_building", x, y, "int", str(packed)))


## `Color.rgba8888` → Godot `Color` (the packed value is `0xRRGGBBAA`).
func _rgba_from_int(value: int) -> Color:
	return Color8((value >> 24) & 0xff, (value >> 16) & 0xff, (value >> 8) & 0xff, value & 0xff)


## J05 T1: `ItemSelection.buildTable` blocks (item/liquid/block/unit pickers).
## The grid widget owns the layout; picking closes the panel
## (`ItemSelection.java:70-71`, `closeSelect`) and re-tapping the checked icon
## submits an empty value (`:73`, Java's `null` → `configure(null)`).
##
## J05 T2: unit factories additionally stack the produced unit's command row
## (`UnitFactory.java:292-340`); a reconstructor's whole panel is that row
## (`Reconstructor.java:215-240`, 5 columns of 44 px cells).
func _build_selection() -> void:
	if bool(_config.get("selection", false)):
		# code-instantiated: the icon grid is built from the `config_spec` JSON's
		# option list (runtime content); it has no scene-authorable shape.
		var selection := MindItemSelection.new()
		body.add_child(selection)
		selection.build(_config)
		selection.picked.connect(_on_selection_pick)
	var commands: Variant = _config.get("commands", null)
	if commands is Dictionary:
		# code-instantiated: the command row is built from the produced unit's
		# command list in the spec.
		var row := MindItemCommandRow.new()
		body.add_child(row)
		row.build(commands)
		row.picked.connect(_on_command_pick.bind(row))


func _on_selection_pick(value: String) -> void:
	var kind := str(_config.get("pick_kind", "content"))
	if value.is_empty():
		# `configure(null)`/`configClear`: the re-tap cleared the selection.
		_submit("none", "")
	else:
		_submit(kind, value)
	_close()


## A command-row pick configures the command (`configure(item)`); the
## reconstructor also closes the panel (`deselect()`, `Reconstructor.java:230`)
## while the factory's row leaves it open (`UnitFactory.java:311-315`).
func _on_command_pick(name: String, row: Node) -> void:
	_submit("content", name)
	if row != null and row.has_method("close_select") and bool(row.call("close_select")):
		_close()


func _build_options() -> void:
	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.custom_minimum_size = Vector2(0, 150)
	body.add_child(scroll)
	var grid := GridContainer.new()
	grid.columns = 4
	scroll.add_child(grid)
	var clear := Button.new()
	clear.text = _t("@clear", "clear")
	clear.tooltip_text = _t("@clear", "clear")
	clear.pressed.connect(func() -> void: _submit("none", ""))
	grid.add_child(clear)
	_add_close_button(grid)
	for option in _config.get("options", []):
		if not (option is Dictionary):
			continue
		var name := str(option.get("name", ""))
		var label := _t(str(option.get("label", "")), name)
		var button := Button.new()
		button.text = label
		button.tooltip_text = label
		button.pressed.connect(func() -> void: _submit("content", name))
		grid.add_child(button)


## code-instantiated: transient close affordance (one per open).
func _add_close_button(row: Container) -> void:
	var close := Button.new()
	close.text = "X"
	close.tooltip_text = "close"
	close.pressed.connect(func() -> void: _close())
	row.add_child(close)


## Hides the panel and clears the input's `config.getSelected()` mirror
## (`MindInput.close_block_config`; H02 T6).
func _close() -> void:
	visible = false
	var input := get_node_or_null("/root/Spine/Input")
	if input != null and input.has_method("close_block_config"):
		input.call("close_block_config")


func _submit(kind: String, value: String) -> void:
	var input := get_node_or_null("/root/Spine/Input")
	if input == null or not input.has_method("configure_building"):
		return
	var x := int(_config.get("x", 0))
	var y := int(_config.get("y", 0))
	input.call("configure_building", x, y, kind, value)
