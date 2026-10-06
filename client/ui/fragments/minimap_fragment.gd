## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/MinimapFragment.java (plan 14 §3.5, M4).
##
## Fullscreen minimap: hosts the `MindMinimapWidget`, captures scroll/keys and
## pans/zooms the camera (plan 15). The widget's pan/zoom signals are forwarded
## to the `MindCamera2D` host; terrain/entity pixels come from the native
## `MindMinimap` provider the widget consumes. Left-tap or ESC (via `UiRoot`)
## hides the overlay.

extends Control

## Last absolute widget zoom, converted into a `Renderer.scaleCamera` step.
var _last_zoom := 1.0

@onready var minimap: MindMinimapWidget = get_node_or_null("Center/Minimap")


func _ready() -> void:
	if minimap != null:
		if not minimap.pan_requested.is_connected(_on_pan):
			minimap.pan_requested.connect(_on_pan)
		if not minimap.zoom_changed.is_connected(_on_zoom):
			minimap.zoom_changed.connect(_on_zoom)
		if not minimap.tapped.is_connected(_on_tapped):
			minimap.tapped.connect(_on_tapped)


func _on_pan(world_delta: Vector2) -> void:
	var camera := get_node_or_null("/root/Spine/World/Camera2D")
	if camera == null or not camera.has_method("pan_to"):
		return
	var position: Vector2 = camera.get("position")
	camera.call("pan_to", position.x + world_delta.x, position.y + world_delta.y)


## `Renderer.scaleCamera`: 4.0 is `ZOOM_STEP_DIVISOR`, so a factor ratio `r`
## becomes the step `(r - 1) * 4`.
func _on_zoom(factor: float) -> void:
	var camera := get_node_or_null("/root/Spine/World/Camera2D")
	if camera == null or not camera.has_method("zoom_by"):
		return
	var step := (factor / maxf(_last_zoom, 0.0001) - 1.0) * 4.0
	_last_zoom = factor
	if absf(step) > 0.0001:
		camera.call("zoom_by", step)


## `MinimapFragment.toggle`: left-tap on the map (or a second key press) closes.
func _on_tapped() -> void:
	toggle()


func toggle() -> void:
	visible = not visible
	if visible:
		move_to_front()
