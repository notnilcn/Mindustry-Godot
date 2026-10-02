## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/WarningBar.java (plan 14 §3.3).
##
## Skewed stripe bar with top/bottom rules (debug/overflow usage).

class_name MindWarningBar
extends Control

@export var bar_width := 40.0
@export var spacing := 80.0
@export var skew := 40.0


func _ready() -> void:
	spacing = bar_width * 2.0
	skew = bar_width
	queue_redraw()


func _draw() -> void:
	var color := modulate
	var amount := int(size.x / maxf(1.0, spacing)) + 2
	for i in amount:
		var rx := float(i - 1) * spacing
		var points := PackedVector2Array([
			Vector2(rx, 0.0),
			Vector2(rx + skew, size.y),
			Vector2(rx + skew + bar_width, size.y),
			Vector2(rx + bar_width, 0.0),
		])
		draw_colored_polygon(points, color)
	draw_line(Vector2(0.0, 0.0), Vector2(size.x, 0.0), color, 3.0)
	draw_line(Vector2(0.0, size.y), Vector2(size.x, size.y), color, 3.0)
