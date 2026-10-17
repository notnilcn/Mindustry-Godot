## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/ResearchDialog.java (`View` class,
##         lines 441-814; `Styles.nodei` up-drawable rule, connector drawing).
##
## The pannable/zoomable tech-tree canvas: one 60 px node button per TechNode at
## the Java `treeLayout` position (Java y-up, flipped here), a two-pass connector
## layer (gray locks under accent lines) and drag/wheel camera control. Node rows
## come from `MindCampaign.research_tree_json` (T1a).

class_name MindResearchView
extends Control

## Emitted on a click that is not a drag (`View` button click).
signal node_pressed(index: int)
## Emitted when the mouse enters a node button.
signal node_hovered(index: int)
## Emitted when the mouse exits a node button.
signal node_exited(index: int)

## `ResearchDialog.nodeSize`.
const NODE_SIZE := 60.0
## `button.resizeImage(32f)`.
const IMAGE_SIZE := 32.0
## Java wheel clamp (`View` wheel handler).
const MIN_SCALE := 0.25
const MAX_SCALE := 1.0
## `Pal.gray` (#454545) and `Color.gray` (#7f7f7f).
const PAL_GRAY := Color(0.271, 0.271, 0.271)
const IMAGE_GRAY := Color(0.498, 0.498, 0.498)

## `View.panX`/`panY` in Java (y-up) space.
var pan := Vector2(0.0, -200.0)
## `View.scale`.
var scale_factor := 1.0
## `View.moved`: true between drag-start and the deferred release reset.
var moved := false

var _nodes: Array = []
var _buttons: Array[Button] = []
var _bounds: Array = []
var _dirty := true
var _last_size := Vector2.ZERO
## Region name -> cached ninepatch style (shared across views).
static var _styles: Dictionary = {}


func _init() -> void:
	# Java's tree `View` is an unclipped `Group` stacked in `cont`
	# (`ResearchDialog.java:110-114`): nodes and lines run on under the bottom
	# button row, which draws over them (L03 T5, diff/40-tech-side.png).
	clip_contents = false
	mouse_filter = Control.MOUSE_FILTER_STOP


## Replaces the canvas contents with a `research_tree_json` view.
func set_tree(tree: Dictionary) -> void:
	for button in _buttons:
		button.queue_free()
	_buttons.clear()
	_nodes = tree.get("nodes", []) as Array
	_bounds = tree.get("bounds", []) as Array
	for index in _nodes.size():
		var node: Dictionary = _nodes[index]
		# code-instantiated: one button per tech node (`View.rebuildAll`).
		var button := Button.new()
		button.set_meta("node_index", index)
		button.custom_minimum_size = Vector2(NODE_SIZE, NODE_SIZE)
		button.size = Vector2(NODE_SIZE, NODE_SIZE)
		var node_style := _style_for(node)
		for style_name in ["normal", "hover", "pressed", "disabled"]:
			button.add_theme_stylebox_override(style_name, node_style)
		add_child(button)
		_build_button_content(button, node)
		button.pressed.connect(_on_button_pressed.bind(index))
		button.mouse_entered.connect(func() -> void: node_hovered.emit(index))
		button.mouse_exited.connect(func() -> void: node_exited.emit(index))
		# Buttons swallow motion events; forward them to the canvas so a drag
		# started on a node still pans.
		button.gui_input.connect(_forward_input)
		_buttons.append(button)
	_dirty = true


## `rebuildTree`: pan -200, scale 1.
func reset_view() -> void:
	pan = Vector2(0.0, -200.0)
	scale_factor = 1.0
	_dirty = true


## Button rect in viewport coordinates (hover popup anchor).
func button_rect(index: int) -> Rect2:
	if index < 0 or index >= _buttons.size():
		return Rect2()
	return _buttons[index].get_global_rect()


## The parsed node row at `index`.
func node_at(index: int) -> Dictionary:
	if index < 0 or index >= _nodes.size():
		return {}
	return _nodes[index]


func _build_button_content(button: Button, node: Dictionary) -> void:
	if bool(node.get("selectable", false)):
		var icon := MindWidgets.content_icon(
			str(node.get("icon", "")), str(node.get("type", "")), IMAGE_SIZE
		)
		if icon != null:
			icon.modulate = _image_color(node)
			icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
			icon.set_anchors_preset(Control.PRESET_CENTER)
			icon.offset_left = -IMAGE_SIZE / 2.0
			icon.offset_top = -IMAGE_SIZE / 2.0
			icon.offset_right = IMAGE_SIZE / 2.0
			icon.offset_bottom = IMAGE_SIZE / 2.0
			button.add_child(icon)
		else:
			# Missing non-sector region: Java draws `Icon.lock` only when the
			# node is not selectable (handled above); keep the button empty.
			push_warning("[ui] missing icon region `%s`" % str(node.get("icon", "")))
	else:
		var glyph := MindWidgets.glyph("lock", 32, _image_color(node))
		glyph.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		glyph.mouse_filter = Control.MOUSE_FILTER_IGNORE
		button.add_child(glyph)


## `button.getImage().setColor(...)` (ResearchDialog.java:512).
func _image_color(node: Dictionary) -> Color:
	if not bool(node.get("locked", true)):
		return Color.WHITE
	if bool(node.get("selectable", false)):
		return IMAGE_GRAY
	return PAL_GRAY


## `button.getStyle().up` rule (ResearchDialog.java:509).
func _style_for(node: Dictionary) -> StyleBoxTexture:
	var key := "button"
	if not bool(node.get("locked", true)):
		key = "button-over"
	elif not bool(node.get("selectable", false)) or not bool(node.get("can_spend", false)):
		key = "button-red"
	if _styles.has(key):
		return _styles[key]
	var box := StyleBoxTexture.new()
	box.texture = MindWidgets.icon_texture(key)
	box.texture_margin_left = 12.0
	box.texture_margin_top = 12.0
	box.texture_margin_right = 12.0
	box.texture_margin_bottom = 12.0
	_styles[key] = box
	return box


func _on_button_pressed(index: int) -> void:
	if moved:
		return
	node_pressed.emit(index)


func _forward_input(event: InputEvent) -> void:
	_gui_input(event)


func _gui_input(event: InputEvent) -> void:
	if event is InputEventMouseMotion and (event.button_mask & MOUSE_BUTTON_MASK_LEFT) != 0:
		pan.x += (event as InputEventMouseMotion).relative.x / scale_factor
		# Arc dy is y-up; Godot relative.y grows downward.
		pan.y -= (event as InputEventMouseMotion).relative.y / scale_factor
		moved = true
		_clamp()
		_dirty = true
		accept_event()
	elif event is InputEventMouseButton:
		var mouse := event as InputEventMouseButton
		if mouse.pressed and mouse.button_index == MOUSE_BUTTON_WHEEL_DOWN:
			_zoom(1.0)
			accept_event()
		elif mouse.pressed and mouse.button_index == MOUSE_BUTTON_WHEEL_UP:
			_zoom(-1.0)
			accept_event()
		elif not mouse.pressed and mouse.button_index == MOUSE_BUTTON_LEFT:
			# Deferred: a button's `pressed` fires on this release and must
			# still observe `moved` (Arc `released` runs after the click).
			call_deferred("_clear_moved")


## Java wheel: `scale = clamp(scale - amountY/10*scale, 0.25, 1)` with
## `amountY = +1` on wheel-down.
func _zoom(amount_y: float) -> void:
	scale_factor = clampf(
		scale_factor - amount_y / 10.0 * scale_factor, MIN_SCALE, MAX_SCALE
	)
	_dirty = true


func _clear_moved() -> void:
	moved = false


## `View.clamp` (ResearchDialog.java:539-549), in Java (y-up) space.
func _clamp() -> void:
	if _bounds.size() < 4:
		return
	var pad := NODE_SIZE
	var ox := size.x / 2.0
	var oy := size.y / 2.0
	var screen := get_viewport_rect().size
	var rx: float = float(_bounds[0]) + pan.x + ox
	var ry: float = pan.y + oy + float(_bounds[1])
	var rw: float = float(_bounds[2])
	var rh: float = float(_bounds[3])
	rx = clampf(rx, -rw + pad, screen.x - pad)
	ry = clampf(ry, -rh + pad, screen.y - pad)
	pan.x = rx - float(_bounds[0]) - ox
	pan.y = ry - float(_bounds[1]) - oy


func _process(_delta: float) -> void:
	if not _dirty and _last_size == size:
		return
	_last_size = size
	_dirty = false
	_place_buttons()
	queue_redraw()


func _place_buttons() -> void:
	var odd_offset := floori(get_viewport_rect().size.y) % 2 / 2.0
	for index in _buttons.size():
		var node: Dictionary = _nodes[index]
		var button := _buttons[index]
		button.visible = bool(node.get("visible", true))
		if not button.visible:
			continue
		var center := _java_point(
			float(node.get("x", 0.0)), float(node.get("y", 0.0)) + odd_offset
		)
		button.scale = Vector2.ONE * scale_factor
		button.position = center - Vector2(NODE_SIZE, NODE_SIZE) * 0.5 * scale_factor


## Java (y-up) point through pan/zoom into Godot view coordinates.
func _java_point(java_x: float, java_y: float) -> Vector2:
	var point := Vector2(
		java_x + pan.x + size.x / 2.0, size.y - (java_y + pan.y + size.y / 2.0)
	)
	var view_center := size / 2.0
	return view_center + (point - view_center) * scale_factor


func _draw() -> void:
	if _nodes.is_empty():
		return
	# `Draw.sort`: gray (locked) connectors under the accent ones.
	for pass_index in 2:
		var want_lock := pass_index == 0
		for index in _nodes.size():
			var node: Dictionary = _nodes[index]
			if not bool(node.get("visible", true)):
				continue
			for child_variant in (node.get("children", []) as Array):
				var child_index := int(child_variant)
				if child_index < 0 or child_index >= _nodes.size():
					continue
				var child: Dictionary = _nodes[child_index]
				if not bool(child.get("visible", true)):
					continue
				var lock: bool = (
					bool(node.get("locked", true)) or bool(child.get("locked", true))
				)
				if lock != want_lock:
					continue
				var color := PAL_GRAY if lock else MindStyles.ACCENT
				var node_x := float(node.get("x", 0.0))
				var node_y := float(node.get("y", 0.0))
				var child_x := float(child.get("x", 0.0))
				var child_y := float(child.get("y", 0.0))
				var node_point := _java_point(node_x, node_y)
				var child_point := _java_point(child_x, child_y)
				var manhattan := absf(node_x - child_x) + absf(node_y - child_y)
				var straight := (
					absf(absf(node_y - child_y) - absf(node_x - child_x)) <= 1.0
					and manhattan <= NODE_SIZE * 3.0
				)
				if straight:
					draw_line(node_point, child_point, color, 4.0 * scale_factor)
				else:
					var corner := _java_point(child_x, node_y)
					draw_line(node_point, corner, color, 4.0 * scale_factor)
					draw_line(corner, child_point, color, 4.0 * scale_factor)
