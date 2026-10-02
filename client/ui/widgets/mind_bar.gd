## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/Bar.java (plan 14 §3.3).
##
## HUD/stat bar: back fill, lerped top fill with a blink flash when the value
## drops, optional outline, and a centered outline-font label. Bound to a
## `Callable` returning the fraction (GDScript) or a `BarModel` id (`MindHud`).

class_name MindBar
extends Control

var bar_name := ""
var fraction := 0.0
var bar_color := Color.WHITE
var blink_color := Color.WHITE
var outline_color := Color(0.0, 0.0, 0.0, 0.0)
var outline_radius := 0.0

var _value := 0.0
var _last_value := 0.0
var _blink := 0.0


## Initializes the bar (`Bar(name, color, fraction)`).
func setup(new_name: String, color: Color, frac: float) -> void:
	bar_name = new_name
	bar_color = color
	fraction = clampf(frac, 0.0, 1.0)
	_value = fraction
	_last_value = fraction
	queue_redraw()


## Resets the animated value (`Bar.reset`).
func reset(frac: float) -> void:
	_value = frac
	_last_value = frac
	_blink = 0.0
	queue_redraw()


## Snaps to the current fraction (`Bar.snap`).
func snap() -> void:
	_value = fraction
	_last_value = fraction
	queue_redraw()


## Triggers a blink flash (`Bar.flash`).
func flash() -> void:
	_blink = 1.0
	queue_redraw()


## Sets the outline (`Bar.outline`).
func outline(color: Color, stroke: float) -> void:
	outline_color = color
	outline_radius = stroke
	queue_redraw()


func _process(_delta: float) -> void:
	var computed := clampf(fraction, 0.0, 1.0)
	if _last_value > computed:
		_blink = 1.0
		_last_value = computed
	_blink = lerpf(_blink, 0.0, 0.2)
	_value = lerpf(_value, computed, 0.15)
	queue_redraw()


func _draw() -> void:
	if outline_radius > 0.0:
		var outer := Rect2(Vector2(-outline_radius, -outline_radius), size + Vector2(outline_radius, outline_radius) * 2.0)
		draw_rect(outer, outline_color, false, outline_radius)
	draw_rect(Rect2(Vector2.ZERO, size), Color(0.1, 0.1, 0.1, 1.0), true)
	var top_width := size.x * clampf(_value, 0.0, 1.0)
	var top_color := bar_color.lerp(blink_color, _blink)
	draw_rect(Rect2(Vector2.ZERO, Vector2(maxf(0.0, top_width), size.y)), top_color, true)
	if not bar_name.is_empty():
		_draw_name()


func _draw_name() -> void:
	var font := get_theme_default_font()
	if font == null:
		return
	var font_size := get_theme_default_font_size()
	var text_size := font.get_string_size(bar_name, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size)
	var pos := Vector2((size.x - text_size.x) * 0.5, (size.y + text_size.y) * 0.5 - 2.0)
	# Dark outline then white fill (Fonts.outline parity).
	draw_string(font, pos + Vector2(1, 1), bar_name, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, Color(0, 0, 0, 0.8))
	draw_string(font, pos, bar_name, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, Color.WHITE)
