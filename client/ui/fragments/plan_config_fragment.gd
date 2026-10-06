## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/PlanConfigFragment.java
##         (plan 14 §3.5, M4).
##
## Build-plan configuration popup (`BuildPlan.point_config`). The config dict is
## `{title, options: [{name, label}], value}`; choosing an option emits
## `plan_config_applied` with the selection. The plan-model caller (queued-plan
## point config) is the remaining plan 07/15 seam.

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
	# options (data-driven; count is runtime content).
	var title := Label.new()
	title.text = str(_config.get("title", ""))
	body.add_child(title)
	var options: Array = _config.get("options", [])
	if options.is_empty():
		var apply := Button.new()
		apply.text = "OK"
		apply.pressed.connect(func() -> void: _choose(str(_config.get("value", ""))))
		body.add_child(apply)
		return
	for option in options:
		if not (option is Dictionary):
			continue
		var name := str(option.get("name", ""))
		var label := str(option.get("label", name))
		var button := Button.new()
		button.text = label
		button.tooltip_text = label
		button.pressed.connect(func() -> void: _choose(name))
		body.add_child(button)


func _choose(value: String) -> void:
	_config["value"] = value
	plan_config_applied.emit(_config.duplicate())
	visible = false
