## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/HintsFragment.java (plan 14 §3.5/M2).
##
## Hint catalogue + display. The full data-driven catalogue (`Hint` records with
## `complete`/`show`/`order`/`valid` and the Serpulo/Erekir branch) lands with the
## plan-12 sector runtime; M2 ships the shell and the ordered name list.

extends Control

## Ordered hint keys (`Hint` names; bundle keys `hint.<name>`).
const HINTS := [
	"introduction",
	"placement",
	"drills",
	"conveyors",
	"turrets",
	"waves",
	"coreItems",
	"research",
]

@onready var _label: Label = get_node_or_null("Panel/Label")


func _ready() -> void:
	visible = false


## Shows the hint by catalogue name (bundle `hint.<name>`).
func show_hint(hint_name: String) -> void:
	if _label != null:
		_label.text = _t("@hint.%s" % hint_name)
	visible = true


## Completes/hides the current hint.
func complete_hint() -> void:
	visible = false


func _t(key: String) -> String:
	var resolved := key.trim_prefix("@")
	var assets := MindWidgets.assets()
	if assets == null:
		return resolved
	return str(assets.call("bundle_get", resolved))
