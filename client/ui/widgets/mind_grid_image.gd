## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/GridImage.java (plan 14 §3.3).
##
## Computed grid lines with a 10 px minimum spacing; `set_image_size` is used by
## Minimap/MapView-adjacent widgets.

class_name MindGridImage
extends Control

var image_width := 1
var image_height := 1


func _init(w: int = 1, h: int = 1) -> void:
	image_width = maxi(1, w)
	image_height = maxi(1, h)


## `GridImage.setImageSize`.
func set_image_size(w: int, h: int) -> void:
	image_width = maxi(1, w)
	image_height = maxi(1, h)
	queue_redraw()


func _draw() -> void:
	var xspace := size.x / float(image_width)
	var yspace := size.y / float(image_height)
	var jumpx := int(maxf(10.0, xspace) / maxf(0.001, xspace))
	var jumpy := int(maxf(10.0, yspace) / maxf(0.001, yspace))
	var color := modulate
	var x := 0
	while x <= image_width:
		draw_rect(Rect2(Vector2(xspace * float(x) - 1.0, -1.0), Vector2(2.0, size.y + 1.0)), color, true)
		x += maxi(1, jumpx)
	var y := 0
	while y <= image_height:
		draw_rect(Rect2(Vector2(-1.0, yspace * float(y) - 1.0), Vector2(size.x, 2.0)), color, true)
		y += maxi(1, jumpy)
