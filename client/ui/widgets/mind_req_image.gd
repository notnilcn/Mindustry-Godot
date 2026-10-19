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
func setup(image: Control, is_valid: Callable) -> void:
	_valid = is_valid
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


## `ReqImage.java:22-29`: two parallel top-left -> bottom-right strokes,
## `Pal.removeBack` (`a73e3e`) shifted 2 px down under `Pal.remove` (`e55454`).
## Arc's y-up `(x, y - 2 + height) -> (x + width, y - 2)` is `(0, 2) -> (w, h + 2)`
## in Godot's y-down space. `Lines.line` caps by default: each end extends
## `stroke / 2` along the line (`Lines.java:58-60,94-112`).
class _RequirementX extends Control:
	func _draw() -> void:
		var remove := Color(0.898, 0.329, 0.329)
		var remove_back := Color(0.655, 0.243, 0.243)
		_capped(Vector2(0.0, 2.0), Vector2(size.x, size.y + 2.0), remove_back)
		_capped(Vector2.ZERO, size, remove)

	func _capped(from: Vector2, to: Vector2, color: Color) -> void:
		var stroke := 2.0
		var dir := (to - from).normalized() * (stroke / 2.0)
		draw_line(from - dir, to + dir, color, stroke)
