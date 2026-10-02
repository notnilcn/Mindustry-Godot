## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/BorderImage.java (plan 14 §3.3).
##
## Texture with a colored stroke border; `forceNearest` sets the texture filter
## (Godot edition: `texture_filter` on the canvas item).

class_name MindBorderImage
extends TextureRect

@export var thickness := 4.0
@export var pad := 0.0
@export var border_color := Color(0.5, 0.5, 0.5)
@export var force_nearest := false
@export var draw_alpha := false
@export var alpha_color := Color.GRAY


func border(color: Color) -> MindBorderImage:
	border_color = color
	queue_redraw()
	return self


func _ready() -> void:
	texture_filter = CanvasItem.TEXTURE_FILTER_NEAREST if force_nearest else CanvasItem.TEXTURE_FILTER_LINEAR


func _draw() -> void:
	if draw_alpha:
		draw_rect(Rect2(Vector2.ZERO, size), alpha_color, true)
	if texture != null:
		draw_texture_rect(texture, Rect2(Vector2.ZERO, size), false, modulate)
	draw_rect(Rect2(Vector2(-pad, -pad), size + Vector2(pad, pad) * 2.0), border_color, false, thickness)
