## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: arc.scene.ui.layout.Stack (plan 14 §3.3).
##
## Children share the full cell rectangle; used by ReqImage/MultiReqImage and
## HUD overlays.

class_name MindStack
extends Container


func _get_minimum_size() -> Vector2:
	var result := Vector2.ZERO
	for child in get_children():
		if child is Control:
			var min_size: Vector2 = child.get_combined_minimum_size()
			result = Vector2(maxf(result.x, min_size.x), maxf(result.y, min_size.y))
	return result


## `Container` sends `NOTIFICATION_SORT_CHILDREN`; a script `_sort_children()`
## is not a virtual (see `MindTable._notification`), so route it here.
func _notification(what: int) -> void:
	if what == NOTIFICATION_SORT_CHILDREN:
		_sort_children()


func _sort_children() -> void:
	var rect := Rect2(Vector2.ZERO, size)
	for child in get_children():
		if child is Control:
			child.position = rect.position
			child.size = rect.size
