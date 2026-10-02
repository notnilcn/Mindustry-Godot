## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/ReqImage.java (plan 14 §3.3).
##
## Stacks a requirement image with a red X overlay shown while `valid` is false.

class_name MindReqImage
extends MindStack

var _valid: Callable = Callable()
var _overlay: Control = null


## Builds the requirement image with its validity predicate.
func setup(image: Control, valid: Callable) -> void:
	_valid = valid
	add_child(image)
	# code-instantiated: the cross overlay is a private draw-only element with no
	# static scene (plan 14 §3.3 ReqImage).
	_overlay = _RequirementX.new()
	add_child(_overlay)


## `ReqImage.valid()`.
func valid() -> bool:
	return _valid.is_valid() and bool(_valid.call())


func _process(_delta: float) -> void:
	if _overlay != null:
		_overlay.visible = not valid()


class _RequirementX extends Control:
	func _draw() -> void:
		var remove := Color(0.898, 0.329, 0.329)
		var remove_back := Color(0.65, 0.24, 0.24)
		draw_line(Vector2(0.0, size.y - 2.0), Vector2(size.x, 0.0), remove_back, 2.0)
		draw_line(Vector2(0.0, 0.0), Vector2(size.x, size.y), remove, 2.0)
