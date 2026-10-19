## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/BorderImage.java (plan 14 §3.3).
##          arc/scene/ui/Image.java (`layout`: `Scaling` + `Align.center`)
##
## Texture with a colored stroke border; `forceNearest` sets the texture filter
## (Godot edition: `texture_filter` on the canvas item).
##
## Like an Arc `Image`, the widget sizes to its cell (not the texture): the
## texture is laid out inside it by `stretch_mode` (`STRETCH_KEEP_ASPECT_*` =
## `Scaling.fit`, `STRETCH_SCALE` = `Scaling.stretch`), and the border is drawn
## around the laid-out image rect (`imageX/imageY/imageWidth/imageHeight`), so
## a fitted preview is never cropped and its stroke hugs the picture.

class_name MindBorderImage
extends TextureRect

@export var thickness := 4.0
@export var pad := 0.0
## `Pal.gray` (`BorderImage.borderColor` default).
@export var border_color := Color(0.271, 0.271, 0.271)
@export var force_nearest := false
@export var draw_alpha := false
@export var alpha_color := Color.GRAY


func _init() -> void:
	# An Arc `Image` takes its cell's size; the texture size must not become
	# the minimum size (that pushed large map previews past their cards).
	expand_mode = TextureRect.EXPAND_IGNORE_SIZE


func border(color: Color) -> MindBorderImage:
	border_color = color
	queue_redraw()
	return self


func _ready() -> void:
	texture_filter = CanvasItem.TEXTURE_FILTER_NEAREST if force_nearest else CanvasItem.TEXTURE_FILTER_LINEAR


## The `TextureRect` base already draws the texture laid out by
## `stretch_mode`; this pass adds the alpha backdrop and the border stroke.
func _draw() -> void:
	var rect := image_rect()
	if draw_alpha:
		draw_rect(rect, alpha_color, true)
		if texture != null:
			# The backdrop covers the base draw; repaint the image over it.
			draw_texture_rect(texture, rect, false)
	# `Lines.rect(x + imageX - pad, ...)` (`BorderImage.java:69-70`) fills the
	# four `stroke`-wide edges *inside* the rect (arc `Lines.java:511-522`);
	# Godot's unfilled `draw_rect` centres the stroke on the edge, so inset by
	# half the stroke (a 300 px preview stays 300 px, java/17-mapinfo.png).
	var outer := Rect2(rect.position - Vector2(pad, pad), rect.size + Vector2(pad, pad) * 2.0)
	var half := Vector2(thickness, thickness) * 0.5
	draw_rect(Rect2(outer.position + half, outer.size - half * 2.0), border_color, false, thickness)


## `Image.layout`: the image rect inside the widget for the current
## `stretch_mode` (`Scaling.apply` + centered alignment).
func image_rect() -> Rect2:
	if texture == null:
		return Rect2(Vector2.ZERO, size)
	var texture_size := texture.get_size()
	if texture_size.x <= 0.0 or texture_size.y <= 0.0:
		return Rect2(Vector2.ZERO, size)
	match stretch_mode:
		TextureRect.STRETCH_KEEP_ASPECT, TextureRect.STRETCH_KEEP_ASPECT_CENTERED:
			var factor := minf(size.x / texture_size.x, size.y / texture_size.y)
			var fitted := texture_size * factor
			var origin := Vector2.ZERO
			if stretch_mode == TextureRect.STRETCH_KEEP_ASPECT_CENTERED:
				origin = (size - fitted) * 0.5
			return Rect2(origin, fitted)
		TextureRect.STRETCH_KEEP:
			return Rect2(Vector2.ZERO, texture_size)
		TextureRect.STRETCH_KEEP_CENTERED:
			return Rect2((size - texture_size) * 0.5, texture_size)
		_:
			return Rect2(Vector2.ZERO, size)
