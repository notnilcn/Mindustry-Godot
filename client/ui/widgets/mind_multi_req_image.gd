## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/MultiReqImage.java (plan 14 §3.3).
##
## Cycles the visible requirement every second when none is valid
## (`time += Time.delta/60` in the source, i.e. one second per full cycle).

class_name MindMultiReqImage
extends MindStack

var _displays: Array = []
var _time := 0.0


## Adds a requirement display (`MultiReqImage.add`).
func add_req(display: MindReqImage) -> void:
	_displays.append(display)
	add_child(display)


func _process(delta: float) -> void:
	_time += delta
	for display in _displays:
		display.visible = false
	for display in _displays:
		if display.valid():
			display.visible = true
			return
	if not _displays.is_empty():
		var index := int(_time) % _displays.size()
		_displays[index].visible = true
