## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/BlockConfigFragment.java
##         (plan 14 §3.5, M4).
##
## Scale-in configuration panel anchored to a building's tile. The spec JSON is
## produced by `MindInput` (`DesktopBridge::config_spec`): the block title, the
## tile, and the selectable options (item/liquid content names) plus a `text`
## flag for code-configurable blocks. Submitting routes through
## `MindInput.configure_building` (a sim command; GDScript never mutates sim
## state).

extends Control

var _config: Dictionary = {}

@onready var title: Label = get_node_or_null("Panel/Layout/Title")
@onready var body: VBoxContainer = get_node_or_null("Panel/Layout/Body")


func _ready() -> void:
	visible = false


## Opens the panel for a building (`ConfigUiSpec` JSON from `MindInput`).
func configure(world_pos: Vector2, spec_json: String) -> void:
	position = world_pos
	var parsed: Variant = JSON.parse_string(spec_json)
	_config = parsed if parsed is Dictionary else {}
	if title != null:
		title.text = _t(str(_config.get("title", "")), str(_config.get("block", "")))
	_rebuild()
	visible = true


## Resolves a bundle key through `MindAssets` with a raw fallback.
func _t(key: String, fallback: String) -> String:
	var resolved := key.trim_prefix("@")
	if resolved.is_empty():
		return fallback
	var assets := get_node_or_null("/root/MindAssets")
	if assets == null:
		return fallback
	var value := str(assets.call("bundle_get", resolved))
	if value.is_empty() or value == resolved:
		return fallback
	return value


func _rebuild() -> void:
	if body == null:
		return
	for child in body.get_children():
		child.queue_free()
	# code-instantiated: config rows come from the `config_spec` JSON (item or
	# liquid option list / logic code field) produced by `MindInput`; the count
	# is runtime content, not authorable in the scene.
	if bool(_config.get("text", false)):
		_build_text_editor()
	else:
		_build_options()


func _build_text_editor() -> void:
	var field := LineEdit.new()
	field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	body.add_child(field)
	var row := HBoxContainer.new()
	body.add_child(row)
	var apply := Button.new()
	apply.text = _t("@ok", "OK")
	apply.pressed.connect(func() -> void: _submit("string", field.text))
	row.add_child(apply)
	_add_close_button(row)


func _build_options() -> void:
	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.custom_minimum_size = Vector2(0, 150)
	body.add_child(scroll)
	var grid := GridContainer.new()
	grid.columns = 4
	scroll.add_child(grid)
	var clear := Button.new()
	clear.text = _t("@clear", "clear")
	clear.tooltip_text = _t("@clear", "clear")
	clear.pressed.connect(func() -> void: _submit("none", ""))
	grid.add_child(clear)
	_add_close_button(grid)
	for option in _config.get("options", []):
		if not (option is Dictionary):
			continue
		var name := str(option.get("name", ""))
		var label := _t(str(option.get("label", "")), name)
		var button := Button.new()
		button.text = label
		button.tooltip_text = label
		button.pressed.connect(func() -> void: _submit("content", name))
		grid.add_child(button)


## code-instantiated: transient close affordance (one per open).
func _add_close_button(row: Container) -> void:
	var close := Button.new()
	close.text = "X"
	close.tooltip_text = "close"
	close.pressed.connect(func() -> void: visible = false)
	row.add_child(close)


func _submit(kind: String, value: String) -> void:
	var input := get_node_or_null("/root/Spine/Input")
	if input == null or not input.has_method("configure_building"):
		return
	var x := int(_config.get("x", 0))
	var y := int(_config.get("y", 0))
	input.call("configure_building", x, y, kind, value)
