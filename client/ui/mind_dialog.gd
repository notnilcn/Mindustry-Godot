## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/BaseDialog.java (plan 14 §3.4).
##
## All dialog scripts extend this. Visibility and the active-dialog stack are
## owned by `MindUi` (Rust); this node only renders and calls `show_dialog`/
## `hide_dialog`. Pause transitions are the governor's job (`should_pause` is
## declared by the manifest), never the widget's.

class_name MindDialog
extends Control

## Title text; empty hides the title row entirely.
@export var title_text := ""
## Whether showing this dialog pauses the game (manifest `pause`).
@export var should_pause := false
## Whether the dialog uses the full-screen dialog style.
@export var full_dialog := false
## Accent color of the title row.
@export var title_color := Color.WHITE

@onready var title_label: Label = get_node_or_null("Center/Panel/Layout/Title")
@onready var body: ScrollContainer = get_node_or_null("Center/Panel/Layout/Body")
@onready var cont: Container = get_node_or_null("Center/Panel/Layout/Body/Content")
@onready var buttons: Container = get_node_or_null("Center/Panel/Layout/Buttons")

var _context: Dictionary = {}


func _ready() -> void:
	visible = false
	_apply_title()


## Receives the JSON context passed to `MindUi.open_dialog` (used by the MCP
## catalogue sweep and plan-12-dependent dialogs).
func set_context_json(json_text: String) -> void:
	var parsed: Variant = JSON.parse_string(json_text)
	_context = parsed if parsed is Dictionary else {}


## Current context dictionary.
func context() -> Dictionary:
	return _context


func _apply_title() -> void:
	if title_label == null:
		return
	title_label.text = title_text
	title_label.visible = not title_text.is_empty()
	title_label.add_theme_color_override("font_color", title_color)


## Shows the dialog (called by `MindUi`; also safe to call directly).
func show_dialog() -> void:
	visible = true
	move_to_front()
	shown()


## Hides the dialog.
func hide_dialog() -> void:
	if not visible:
		return
	visible = false
	hidden()


## Whether the dialog is currently visible.
func is_shown() -> bool:
	return visible


## Adds an `@back` close button to the button row (`addCloseButton`).
func add_close_button(width: float = 210.0) -> Button:
	# code-instantiated: the close button is added by the dialog's own script at
	# runtime (button row contents vary per dialog).
	var button := Button.new()
	button.text = "Back"
	button.theme_type_variation = "defaultt"
	button.pressed.connect(hide_dialog)
	button.custom_minimum_size.x = width
	if buttons != null:
		buttons.add_child(button)
	return button


## Hook for subclasses to rebuild dynamic content when shown (BaseDialog.shown).
func shown() -> void:
	pass


## Hook for subclasses to tear down on hide (BaseDialog.hidden).
func hidden() -> void:
	pass
