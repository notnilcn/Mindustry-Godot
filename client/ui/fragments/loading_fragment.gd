## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/LoadingFragment.java (plan 14 §3.5/M2).
##
## Full-screen loading overlay: progress bar + text + cancel button. `loadAnd`
## (the async completion helper) is owned by the Rust/plan-04 flow; this node
## only renders progress.

extends Control

@onready var _bar: ProgressBar = get_node_or_null("Panel/Layout/Bar")
@onready var _text: Label = get_node_or_null("Panel/Layout/Text")
@onready var _button: Button = get_node_or_null("Panel/Layout/Button")


func _ready() -> void:
	visible = false


## Shows the overlay at `fraction` (0..1) with a `text` message.
func set_progress(fraction: float, text: String = "") -> void:
	visible = true
	if _bar != null:
		_bar.value = clampf(fraction, 0.0, 1.0) * 100.0
	if _text != null and not text.is_empty():
		_text.text = text


## Updates the label only.
func set_text(text: String) -> void:
	if _text != null:
		_text.text = text


## Hides the overlay (`hideLoading`).
func hide_loading() -> void:
	visible = false


## Whether the overlay is up.
func is_shown() -> bool:
	return visible
