## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/MinimapFragment.java (plan 14 §3.5, M4).
##
## Fullscreen minimap: hosts the `MindMinimap` widget, captures scroll/keys and
## pans/zooms the camera (plan 15). The widget's pan/zoom signals are forwarded
## to `MindUi`/the camera host; draw content is plan 16's.

extends Control

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
	# Forwarded to the plan-15 camera host; the name is frozen by §3.11.
	var camera := get_node_or_null("/root/Spine/World/Camera2D")
	if camera != null and camera.has_method("pan_by"):
		camera.call("pan_by", world_delta)


func _on_zoom(factor: float) -> void:
	var camera := get_node_or_null("/root/Spine/World/Camera2D")
	if camera != null and camera.has_method("zoom_by"):
		camera.call("zoom_by", factor)


func _on_tapped() -> void:
	visible = not visible
