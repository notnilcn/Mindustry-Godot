## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: arc.scene.ui.layout.Collapser (plan 14 §3.3).
##
## Animated min-size collapse: `set_collapsed(true)` shrink-tweens the vertical
## minimum size, `false` restores it.

class_name MindCollapser
extends Control

@export var duration := 0.2

var collapsed := false
var _expanded_height := 0.0


func _ready() -> void:
	if _expanded_height <= 0.0:
		_expanded_height = custom_minimum_size.y


## `Collapser.setCollapsed`.
func set_collapsed(value: bool) -> void:
	if value == collapsed:
		return
	collapsed = value
	if not value:
		_expanded_height = maxf(_expanded_height, custom_minimum_size.y)
	var target := 0.0 if value else _expanded_height
	var tween := create_tween()
	tween.tween_property(self, "custom_minimum_size:y", target, duration)


## `Collapser.setDuration`.
func set_duration(value: float) -> void:
	duration = value
