## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/WaveGraph.java (plan 19 M5 §3.9/§2.3.7).
##
## Presentation-only: the numeric series is computed in Rust
## (`MindEditor.wave_graph(from, to)`); this Control draws bars/legend and
## handles scroll-zoom/pan.

extends Control

enum Mode { COUNTS, TOTALS, HEALTH }

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"

var _editor: Node = null
var _from := 0
var _to := 20
var _mode := Mode.COUNTS


func _ready() -> void:
	_editor = get_node_or_null(editor_path)
	mouse_filter = Control.MOUSE_FILTER_STOP


func _draw() -> void:
	var data := _data()
	var units: Array = data.get("units", [])
	var series: Array = data.get("series", [])
	if series.is_empty():
		return
	var max_value := maxf(1.0, float(data.get("max", 1)))
	if _mode == Mode.TOTALS:
		max_value = maxf(1.0, float(data.get("max_total", 1)))
	elif _mode == Mode.HEALTH:
		max_value = maxf(1.0, float(data.get("max_health", 1)))
	var columns := series.size()
	var step := size.x / float(maxi(columns, 1))
	for column in columns:
		var row: PackedInt64Array = series[column]
		var height := 0.0
		if _mode == Mode.COUNTS:
			for unit_index in mini(row.size(), units.size()):
				var value := float(row[unit_index]) / max_value * size.y
				var rect := Rect2(column * step, size.y - height - value, step * 0.8, value)
				draw_rect(rect, Color.from_hsv(float(unit_index) / maxf(1.0, units.size()), 0.7, 1.0))
				height += value
		else:
			var totals: PackedInt64Array = data.get("totals", PackedInt64Array())
			var health: Array = data.get("health", [])
			var value := 0.0
			if _mode == Mode.TOTALS and column < totals.size():
				value = float(totals[column]) / max_value * size.y
			elif _mode == Mode.HEALTH and column < health.size():
				value = float(health[column]) / max_value * size.y
			draw_rect(Rect2(column * step, size.y - value, step * 0.8, value), Color(0.98, 0.78, 0.34))


func set_mode(mode: int) -> void:
	_mode = mode as Mode
	queue_redraw()


func _data() -> Dictionary:
	if _editor == null or not _editor.has_method("wave_graph"):
		return {}
	var value: Variant = _editor.call("wave_graph", _from, _to)
	return value if value is Dictionary else {}
