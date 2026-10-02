## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/FadeInFragment.java (plan 14 §3.5/M2).

extends ColorRect


func _ready() -> void:
	color = Color(0.0, 0.0, 0.0, 1.0)


## Fades the black cover out (`FadeInFragment`).
func start(duration: float = 0.5) -> void:
	var tween := create_tween()
	tween.tween_property(self, "color:a", 0.0, duration)
	tween.tween_callback(func() -> void: visible = false)
