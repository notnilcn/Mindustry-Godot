## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/PerformanceFragment.java (plan 14 §3.5/M2).
##
## Uses Godot `Performance` monitors in place of Arc `PerfCounter` (plan 14 §2.4).

extends Control

@onready var _label: Label = get_node_or_null("Panel/Label")


func _process(_delta: float) -> void:
	if _label == null:
		return
	var fps := Performance.get_monitor(Performance.TIME_FPS)
	var process_ms := Performance.get_monitor(Performance.TIME_PROCESS) * 1000.0
	var memory_mb := Performance.get_monitor(Performance.MEMORY_STATIC) / 1048576.0
	_label.text = "fps %d  cpu %.2f ms  mem %.1f MB" % [int(fps), process_ms, memory_mb]
