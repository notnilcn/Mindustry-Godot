## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/BlockConfigFragment.java
##         (plan 14 §3.5, M4).
##
## Scale-in configuration panel anchored to a building's tile. The widget set is
## `ConfigUiSpec` from plan 07 (`build_configuration`); this M4 shell renders the
## frame and the `configure(world_pos, spec_json)` entry point.

extends Control

var _config: Dictionary = {}

@onready var title: Label = get_node_or_null("Panel/Layout/Title")
@onready var body: VBoxContainer = get_node_or_null("Panel/Layout/Body")


func _ready() -> void:
	visible = false


## Opens the panel for a building (`ConfigUiSpec` JSON from plan 07).
func configure(world_pos: Vector2, spec_json: String) -> void:
	position = world_pos
	var parsed: Variant = JSON.parse_string(spec_json)
	_config = parsed if parsed is Dictionary else {}
	if title != null:
		title.text = str(_config.get("title", ""))
	_rebuild()
	visible = true


func _rebuild() -> void:
	if body == null:
		return
	for child in body.get_children():
		child.queue_free()
	# code-instantiated: config rows come from the plan-07 `ConfigUiSpec`
	# (item/liquid/block/unit selectors, sliders, clear) — data-driven.
