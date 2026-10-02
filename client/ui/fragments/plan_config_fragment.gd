## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/PlanConfigFragment.java
##         (plan 14 §3.5, M4).
##
## Build-plan configuration popup (`BuildPlan.point_config`). The plan data model
## is plan 07/15; the M4 shell renders the frame and the apply signal.

extends Control

signal plan_config_applied(config: Dictionary)

var _config: Dictionary = {}

@onready var body: VBoxContainer = get_node_or_null("Panel/Body")


func _ready() -> void:
	visible = false


## Opens the popup for a plan point config (plan 07/15).
func configure(config: Dictionary) -> void:
	_config = config
	_rebuild()
	visible = true


func _rebuild() -> void:
	if body == null:
		return
	for child in body.get_children():
		child.queue_free()
	# code-instantiated: plan-config controls come from the plan-07 point config
	# (data-driven; placeholder until the plan model bridge lands).
