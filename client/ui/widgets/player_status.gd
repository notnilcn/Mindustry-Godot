## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/HudFragment.java (`makeStatusTable`:
##         the hexagon backdrop, the two `SideBar` health chevrons and the
##         `player.icon()` image, stacked in a 120x80 cell).
##
## Read-only HUD widget: polls `MindSimHost.player_unit_status()` and draws the
## player unit portrait inside the health hexagon.

class_name MindPlayerStatus
extends Control

## `Pal.darkerGray` hexagon fill.
const HEX_COLOR := Color(0.2, 0.2, 0.2, 1.0)
## `Pal.darkishGray` empty-bar backdrop.
const BACK_COLOR := Color(0.3, 0.3, 0.3, 1.0)
## `Pal.health`.
const HEALTH_COLOR := Color("ff341c")
## Table layout: `bw = 40`, `pad = -20`, image `maxWidth(54)`, centred in 120.
const BAR_WIDTH := 40.0
const BAR_PAD := -20.0
const IMAGE_MAX_WIDTH := 54.0
const HOST_PATH := "/root/Spine/SimHost"

var _healthf := 1.0
var _dead := true
var _type := ""
var _icon: Texture2D = null
var _icon_type := ""
var _shadow: Texture2D = null
## Per-bar `SideBar` animation state: [last, blink, value].
var _bars := [[1.0, 0.0, 1.0], [1.0, 0.0, 1.0]]


func _ready() -> void:
	custom_minimum_size = Vector2(120, 80)
	mouse_filter = Control.MOUSE_FILTER_IGNORE


func _process(delta: float) -> void:
	var host := get_node_or_null(HOST_PATH)
	if host != null and host.has_method("player_unit_status"):
		var status: Dictionary = host.call("player_unit_status")
		_dead = bool(status.get("dead", true))
		_healthf = float(status.get("healthf", 0.0))
		_type = str(status.get("type", ""))
	# `player.icon()`: dead players show the default (`alpha`) icon.
	var shown := "alpha" if _dead or _type.is_empty() else _type
	if shown != _icon_type:
		_icon_type = shown
		_icon = _region("unit-%s-ui" % shown)
	if _shadow == null:
		_shadow = _region("circle-shadow")
	var amount := 0.0 if _dead else _healthf
	for bar in _bars:
		_step_bar(bar, amount, delta)
	queue_redraw()


func _draw() -> void:
	var center := size * 0.5
	# `Fill.poly(cx, cy, 6, height / sqrt3)` (pointy left/right) + the shadow.
	var radius := size.y / sqrt(3.0)
	var hex := PackedVector2Array()
	for i in 6:
		var angle := TAU * float(i) / 6.0
		hex.push_back(center + Vector2(cos(angle), sin(angle)) * radius)
	draw_colored_polygon(hex, HEX_COLOR)
	if _shadow != null:
		var side := size.y * 1.13
		draw_texture_rect(_shadow, Rect2(center - Vector2(side, side) * 0.5, Vector2(side, side)), false, Color(0, 0, 0, 0.4))
	var total := BAR_WIDTH + BAR_PAD + IMAGE_MAX_WIDTH + BAR_PAD + BAR_WIDTH
	var left := (size.x - total) * 0.5
	var image_x := left + BAR_WIDTH + BAR_PAD
	var right_x := image_x + IMAGE_MAX_WIDTH + BAR_PAD
	# Left bar is `flip = true`; the image is brought to the front last.
	_draw_bar(Rect2(left, 0, BAR_WIDTH, size.y), _bars[0], true)
	_draw_bar(Rect2(right_x, 0, BAR_WIDTH, size.y), _bars[1], false)
	if _icon != null:
		# `Scaling.bounded`: shrink to fit, never upscale.
		var tex_size := _icon.get_size()
		var bound := Vector2(IMAGE_MAX_WIDTH, size.y)
		var zoom := minf(1.0, minf(bound.x / tex_size.x, bound.y / tex_size.y))
		var drawn := tex_size * zoom
		draw_texture_rect(_icon, Rect2(Vector2(image_x, 0) + (bound - drawn) * 0.5, drawn), false)


## `SideBar.draw` easing: blink on a drop, `lerpDelta` towards the new value.
func _step_bar(bar: Array, next: float, delta: float) -> void:
	if is_nan(next) or is_inf(next):
		next = 1.0
	if next < float(bar[0]):
		bar[1] = 1.0
	var frames := delta * 60.0
	bar[1] = lerpf(float(bar[1]), 0.0, 1.0 - pow(1.0 - 0.2, frames))
	bar[2] = lerpf(float(bar[2]), next, 1.0 - pow(1.0 - 0.15, frames))
	bar[0] = next


func _draw_bar(rect: Rect2, bar: Array, flip: bool) -> void:
	_draw_inner(rect, BACK_COLOR, 1.0, flip)
	_draw_inner(rect, HEALTH_COLOR.lerp(Color.WHITE, float(bar[1])), float(bar[2]), flip)


## `SideBar.drawInner` in Arc's y-up space, mapped into the rect (y-down).
func _draw_inner(rect: Rect2, color: Color, fract: float, flip: bool) -> void:
	if fract < 0.0:
		return
	fract = clampf(fract, 0.0, 1.0)
	var w := rect.size.x
	var h := rect.size.y
	var stroke := w * 0.35
	var bh := h / 2.0
	var f1 := minf(fract * 2.0, 1.0)
	var f2 := (fract - 0.5) * 2.0
	var bo := -(1.0 - f1) * (w - stroke)
	_quad(rect, flip, color, [
		Vector2(0, 0), Vector2(stroke, 0),
		Vector2(w + bo, bh * f1), Vector2(w - stroke + bo, bh * f1)])
	if f2 > 0.0:
		var bx := (w - stroke) * (1.0 - f2)
		_quad(rect, flip, color, [
			Vector2(w, bh), Vector2(w - stroke, bh),
			Vector2(bx, h * fract), Vector2(bx + stroke, h * fract)])


func _quad(rect: Rect2, flip: bool, color: Color, points: Array) -> void:
	var out := PackedVector2Array()
	for p in points:
		var local: Vector2 = p
		var x := rect.size.x - local.x if flip else local.x
		out.push_back(rect.position + Vector2(x, rect.size.y - local.y))
	# `Fill.quad` winds either way; Godot needs a non-degenerate polygon.
	if absf((out[1] - out[0]).cross(out[2] - out[0])) < 0.001 and absf((out[2] - out[0]).cross(out[3] - out[0])) < 0.001:
		return
	draw_colored_polygon(out, color)


func _region(region_name: String) -> Texture2D:
	var assets := MindWidgets.assets()
	if assets == null or not assets.has_method("find_region"):
		return null
	var texture: Variant = assets.call("find_region", region_name)
	return texture as Texture2D
