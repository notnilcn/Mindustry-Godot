## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/Minimap.java (plan 14 §3.3/§3.5, M4).
##
## 140 px minimap widget: right-drag pans the camera, the wheel zooms, a left tap
## toggles fullscreen. Terrain pixels come from the native `MindMinimap`
## provider (`World/MinimapProvider`); the camera-region border is drawn from its
## `region()`. Pan/zoom/tap are emitted as intents so the renderer hook and
## plan-15 camera stay in their lanes.

## Named `MindMinimapWidget` (not `MindMinimap`) because plan 16 already
## registers the native `MindMinimap` provider node (render/minimap.rs); this is
## the plan-14 interactive control that consumes it.
class_name MindMinimapWidget
extends Control

signal pan_requested(world_delta: Vector2)
signal zoom_changed(factor: float)
signal tapped

## Upstream 140 px size.
const BASE_SIZE := 140.0
## `Minimap` table margin around the map element (`margin(5f)`).
const PANE_MARGIN := 5.0
## `Minimap` zoom clamp (plan §7c step 10).
const MIN_ZOOM := 0.25
const MAX_ZOOM := 10.0
## `mind_core::config::TILESIZE` (world pixels per minimap texture pixel).
const TILE_SIZE := 8.0
## Provider node path (`scenes/game.tscn`).
const PROVIDER_PATH := "/root/Spine/World/MinimapProvider"

## HUD minimap (`ui/Minimap.java`): draw only `renderer.minimap.getRegion()`
## (the camera-centred crop) inside a `Tex.pane` frame, and route the wheel to
## `MinimapRenderer.zoomBy`. The fullscreen fragment leaves this off and draws
## the whole map with the camera-region outline.
@export var crop_to_region := false

var zoom := 1.0
var _dragging := false
var _drag_last := Vector2.ZERO
## Full map extent in world pixels (updated from the provider texture size).
var map_extent := Vector2(200.0, 200.0)
var _provider: Node = null
var _pane: StyleBox = null


func _ready() -> void:
	var side := BASE_SIZE + (PANE_MARGIN * 2.0 if crop_to_region else 0.0)
	custom_minimum_size = Vector2(side, side)
	mouse_filter = Control.MOUSE_FILTER_STOP
	set_process(true)


func _process(_delta: float) -> void:
	# The provider rebuilds its texture/world revision and the camera region
	# moves every frame; redraw so both stay live.
	queue_redraw()


func _draw() -> void:
	var texture := _native_texture()
	if texture == null:
		_draw_placeholder()
		return
	# World pixels covered by the full minimap texture.
	map_extent = texture.get_size() * TILE_SIZE
	if crop_to_region:
		_draw_cropped(texture)
		return
	draw_rect(Rect2(Vector2.ZERO, size), Color(0, 0, 0, 0.6), true)
	draw_texture_rect(texture, Rect2(Vector2.ZERO, size), false)
	_draw_region(texture)
	draw_rect(Rect2(Vector2.ZERO, size), Color(0.8, 0.8, 0.8, 0.5), false, 1.0)


## `Minimap.draw`: `Draw.rect(renderer.minimap.getRegion(), ...)` over the
## `Tex.pane` background (5 px margin).
func _draw_cropped(texture: Texture2D) -> void:
	var pane := _pane_box()
	if pane != null:
		draw_style_box(pane, Rect2(Vector2.ZERO, size))
	else:
		draw_rect(Rect2(Vector2.ZERO, size), Color(0, 0, 0, 1), true)
		draw_rect(Rect2(Vector2.ZERO, size), Color(0.27, 0.27, 0.27, 1), false, 3.0)
	var inner := Rect2(Vector2(PANE_MARGIN, PANE_MARGIN), size - Vector2(PANE_MARGIN, PANE_MARGIN) * 2.0)
	draw_rect(inner, Color.BLACK, true)
	var node := _provider_node()
	var region := Rect2(Vector2.ZERO, texture.get_size())
	if node != null and node.has_method("region"):
		region = node.call("region")
	# The crop may extend past a map smaller than the view; clip to the texture
	# and keep the scale so the map stays square-pixelled.
	var clipped := region.intersection(Rect2(Vector2.ZERO, texture.get_size()))
	if clipped.size.x <= 0.0 or clipped.size.y <= 0.0 or region.size.x <= 0.0:
		return
	var scale := inner.size / region.size
	var dest := Rect2(inner.position + (clipped.position - region.position) * scale, clipped.size * scale)
	draw_texture_rect_region(texture, dest, clipped)


## `Tex.pane` ninepatch (36x27, splits 12) from the atlas, built once.
func _pane_box() -> StyleBox:
	if _pane != null:
		return _pane
	var assets := MindWidgets.assets()
	if assets == null or not assets.has_method("find_region"):
		return null
	var texture: Variant = assets.call("find_region", "pane")
	if not (texture is Texture2D):
		return null
	var box := StyleBoxTexture.new()
	box.texture = texture
	box.texture_margin_left = 12.0
	box.texture_margin_top = 12.0
	box.texture_margin_right = 12.0
	box.texture_margin_bottom = 12.0
	_pane = box
	return _pane


## Camera crop indicator (`MinimapRenderer.getRegion` scaled to the widget).
func _draw_region(texture: Texture2D) -> void:
	var node := _provider_node()
	if node == null or not node.has_method("region"):
		return
	var region: Rect2 = node.call("region")
	var texture_size := texture.get_size()
	if texture_size.x <= 0.0 or texture_size.y <= 0.0:
		return
	var scaled := Rect2(region.position / texture_size * size, region.size / texture_size * size)
	draw_rect(scaled, Color(1, 1, 1, 0.8), false, 1.0)


func _draw_placeholder() -> void:
	draw_rect(Rect2(Vector2.ZERO, size), Color(0, 0, 0, 0.6), true)
	var step := 20.0
	var x := step
	while x < size.x:
		draw_line(Vector2(x, 0), Vector2(x, size.y), Color(1, 1, 1, 0.08))
		x += step
	var y := step
	while y < size.y:
		draw_line(Vector2(0, y), Vector2(size.x, y), Color(1, 1, 1, 0.08))
		y += step
	draw_rect(Rect2(Vector2.ZERO, size), Color(0.8, 0.8, 0.8, 0.5), false, 1.0)


func _provider_node() -> Node:
	if _provider == null or not is_instance_valid(_provider):
		_provider = get_node_or_null(PROVIDER_PATH)
	return _provider


func _native_texture() -> Texture2D:
	var node := _provider_node()
	if node == null or not node.has_method("get_texture"):
		return null
	return node.call("get_texture") as Texture2D


## Sets the zoom (clamped, emits `zoom_changed`).
func set_zoom(value: float) -> void:
	var clamped := clampf(value, MIN_ZOOM, MAX_ZOOM)
	if is_equal_approx(clamped, zoom):
		return
	zoom = clamped
	zoom_changed.emit(zoom)


func _gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		var button := event as InputEventMouseButton
		var wheel := 0.0
		if button.button_index == MOUSE_BUTTON_WHEEL_UP and button.pressed:
			wheel = -1.0
		elif button.button_index == MOUSE_BUTTON_WHEEL_DOWN and button.pressed:
			wheel = 1.0
		if wheel != 0.0 and crop_to_region:
			# `renderer.minimap.zoomBy(amounty)`.
			var node := _provider_node()
			if node != null and node.has_method("zoom_by"):
				node.call("zoom_by", wheel)
			accept_event()
		elif wheel < 0.0:
			set_zoom(zoom * 1.1)
			accept_event()
		elif wheel > 0.0:
			set_zoom(zoom / 1.1)
			accept_event()
		elif button.button_index == MOUSE_BUTTON_RIGHT:
			_dragging = button.pressed
			_drag_last = button.position
			accept_event()
		elif button.button_index == MOUSE_BUTTON_LEFT and button.pressed:
			tapped.emit()
			accept_event()
	elif event is InputEventMouseMotion and _dragging:
		var motion := event as InputEventMouseMotion
		var delta := motion.position - _drag_last
		_drag_last = motion.position
		var scale := map_extent / size
		pan_requested.emit(-delta * scale)
		accept_event()
