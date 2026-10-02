## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/Minimap.java (plan 14 §3.3/§3.5, M4).
##
## 140 px minimap widget: right-drag pans the camera, the wheel zooms (0.25–10),
## a left tap toggles fullscreen. The world texture/entities are drawn by plan
## 16's renderer; this widget paints a placeholder grid and emits the pan/zoom
## intents so the renderer hook and plan-15 camera stay in their lanes.

class_name MindMinimap
extends Control

signal pan_requested(world_delta: Vector2)
signal zoom_changed(factor: float)
signal tapped

## Upstream 140 px size.
const BASE_SIZE := 140.0
## `Minimap` zoom clamp (plan §7c step 10).
const MIN_ZOOM := 0.25
const MAX_ZOOM := 10.0

var zoom := 1.0
var _dragging := false
var _drag_last := Vector2.ZERO
## Optional half-extent of the map in world tiles (renderer-provided).
var map_extent := Vector2(200.0, 200.0)


func _ready() -> void:
	custom_minimum_size = Vector2(BASE_SIZE, BASE_SIZE)
	mouse_filter = Control.MOUSE_FILTER_STOP


func _draw() -> void:
	# Placeholder until plan 16 binds the minimap texture (renderer hook).
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
		if button.button_index == MOUSE_BUTTON_WHEEL_UP and button.pressed:
			set_zoom(zoom * 1.1)
			accept_event()
		elif button.button_index == MOUSE_BUTTON_WHEEL_DOWN and button.pressed:
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
