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
	var buttons_only := _buttons_only()
	if title != null:
		title.text = _t(str(_config.get("title", "")), str(_config.get("block", "")))
		title.visible = not buttons_only
	if panel != null:
		# `BlockConfigFragment.showConfig`: `table.background(null)` — blocks
		# whose `buildConfiguration` only adds buttons show just the buttons.
		if buttons_only:
			panel.add_theme_stylebox_override("panel", StyleBoxEmpty.new())
		else:
			panel.remove_theme_stylebox_override("panel")
	_rebuild()
	size = panel.get_combined_minimum_size() if buttons_only and panel != null else PANEL_SIZE
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


func _process(_delta: float) -> void:
	if not visible:
		return
	# `table.pack()`: the buttons-only table is exactly its buttons.
	if _buttons_only() and panel != null:
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
	# `MessageBlock.buildConfiguration` opens a text dialog; the illuminator-style
	# color spec never reaches this branch.
	var multiline := bool(_config.get("multiline", false))
	var field: Control
	if multiline:
		var edit := TextEdit.new()
		edit.custom_minimum_size = Vector2(240, 90)
		edit.wrap_mode = TextEdit.LINE_WRAPPING_BOUNDARY
		edit.text = str(_config.get("current", ""))
		field = edit
	else:
		var line := LineEdit.new()
		line.text = str(_config.get("current", ""))
		field = line
	field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	body.add_child(field)
	var row := HBoxContainer.new()
	body.add_child(row)
	var apply := Button.new()
	apply.text = _t("@ok", "OK")
	apply.pressed.connect(func() -> void: _submit("string", _text_of(field)))
	row.add_child(apply)
	_add_close_button(row)


## Reads the submitted text from a `LineEdit` or `TextEdit`.
func _text_of(field: Control) -> String:
	if field is TextEdit:
		return (field as TextEdit).text
	if field is LineEdit:
		return (field as LineEdit).text
	return ""


## `LightBlock.buildConfiguration`: a color picker; the change submits the packed
## `Color.rgba()` integer (`("int", str(color.to_rgba32()))`).
func _build_color_picker() -> void:
	# code-instantiated: the picker is one widget driven by the spec's
	# `current` color; it has no scene-authorable per-building shape.
	var picker := ColorPickerButton.new()
	picker.custom_minimum_size = Vector2(160, 32)
	picker.color = _rgba_from_int(int(_config.get("current", 0xffd37fff)))
	body.add_child(picker)
	var row := HBoxContainer.new()
	body.add_child(row)
	var apply := Button.new()
	apply.text = _t("@ok", "OK")
	apply.pressed.connect(func() -> void: _submit("int", str(picker.color.to_rgba32())))
	row.add_child(apply)
	_add_close_button(row)


## `Color.rgba8888` → Godot `Color` (the packed value is `0xRRGGBBAA`).
func _rgba_from_int(value: int) -> Color:
	return Color8((value >> 24) & 0xff, (value >> 16) & 0xff, (value >> 8) & 0xff, value & 0xff)


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
