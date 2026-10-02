## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: arc.scene.ui.Label + arc.util.IntFormat usage (change-cached binds).
##
## A change-cached rich label: `set_source(callable)` re-reads the callable each
## frame and only assigns `text` when the formatted value changes (IntFormat
## parity, plan 14 §3.5/§3.10). This keeps HUD text churn allocation-free.

class_name MindLabel
extends MindRichLabel

var _source: Callable = Callable()
var _last_value := ""


## Binds a value producer; the label refreshes at most once per frame.
func set_source(source: Callable) -> void:
	_source = source
	refresh()


## Forces one refresh (also called from `_process` while bound).
func refresh() -> void:
	if not _source.is_valid():
		return
	var value := str(_source.call())
	if value == _last_value:
		return
	_last_value = value
	text = value


func _process(_delta: float) -> void:
	refresh()


## Unbinds the source (used when a HUD element is hidden).
func clear_source() -> void:
	_source = Callable()
