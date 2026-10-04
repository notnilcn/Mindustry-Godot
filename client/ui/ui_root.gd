## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/core/UI.java (`init`, `updateMargins`, groups).
##
## The Godot-side UI root under `Spine/Ui`. It owns the layer groups, eagerly
## instantiates every manifest dialog/fragment (parity with `UI.init()`), builds
## the theme, and registers dialogs with the Rust `MindUi` singleton. Layout and
## lifecycle only — no game rules or sim reads (plan 14 §3.2/§3.10).

class_name MindUiRoot
extends Control

const DIALOGS_MANIFEST := "res://ui/dialogs_manifest.json"

@onready var menu_group: Control = $MenuGroup
@onready var hud_group: Control = $HudGroup
@onready var dialog_layer: Control = $DialogLayer
@onready var overlay_layer: Control = $OverlayLayer
@onready var loading_layer: Control = $LoadingLayer

## Theme build time in milliseconds (plan 14 §7d budget probe).
var theme_build_ms := 0

var _dialogs: Dictionary = {}


func _ready() -> void:
	_apply_theme()
	_load_manifest()
	_connect_prompts()
	_boot()


## Boot presentation (`MenuFragment`/`LoadingFragment`/`FadeInFragment` order):
## the standalone menu is shown, the in-game HUD group is hidden, the loading
## overlay is cleared and the black boot cover fades out.
func _boot() -> void:
	set_menu_visible(true)
	var fade := overlay_layer.get_node_or_null("fade_in")
	if fade != null and fade.has_method("start"):
		fade.call("start")


## Shows the standalone menu (`MenuGroup`/`MenuBackground`) and hides the
## in-game HUD group (parity: `state.isMenu()` gates both), clearing the loading
## overlay too.
func set_menu_visible(menu_visible: bool) -> void:
	menu_group.visible = menu_visible
	hud_group.visible = not menu_visible
	var background := get_node_or_null("../MenuBackground")
	if background != null:
		background.visible = menu_visible
	var loading := loading_layer.get_node_or_null("loading")
	if loading != null and loading.has_method("hide_loading"):
		loading.call("hide_loading")


func _ui() -> Node:
	return get_node_or_null("/root/MindUi")


func _apply_theme() -> void:
	var assets := MindWidgets.assets()
	var started := Time.get_ticks_msec()
	var theme := MindThemeBuilder.build(assets)
	theme_build_ms = int(Time.get_ticks_msec() - started)
	var manifest := MindThemeBuilder.load_manifest()
	if not MindThemeBuilder.verify(theme, manifest):
		push_warning("[ui] theme verification failed (styles_manifest)")
	self.theme = theme


func _load_manifest() -> void:
	if not FileAccess.file_exists(DIALOGS_MANIFEST):
		push_warning("[ui] missing %s" % DIALOGS_MANIFEST)
		return
	var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(DIALOGS_MANIFEST))
	if not (parsed is Dictionary):
		push_warning("[ui] dialogs_manifest is not an object")
		return
	var manifest: Dictionary = parsed
	for entry in manifest.get("dialogs", []):
		_instantiate(entry, dialog_layer, false)
	for entry in manifest.get("fragments", []):
		_instantiate(entry, _group_for(str(entry.get("group", ""))), true)


func _instantiate(entry: Dictionary, parent: Control, is_fragment: bool) -> void:
	var dialog_name := str(entry.get("name", ""))
	var scene_path := str(entry.get("scene", ""))
	if dialog_name.is_empty() or scene_path.is_empty():
		return
	# code-instantiated: dialogs/fragments are manifest-driven (name -> scene
	# path is data), so the scene tree cannot declare them statically.
	var packed: PackedScene = load(scene_path)
	if packed == null:
		push_warning("[ui] could not load %s" % scene_path)
		return
	var instance: Node = packed.instantiate()
	instance.name = dialog_name
	parent.add_child(instance)
	if is_fragment:
		# Manifest `hidden: true` marks fragments that are shown on demand
		# (fullscreen minimap, config/inventory popups); everything else is a
		# persistent HUD/menu element.
		instance.visible = not bool(entry.get("hidden", false))
		return
	_dialogs[dialog_name] = instance
	var ui := _ui()
	if ui != null:
		var should_pause := bool(entry.get("pause", false))
		ui.call("register_dialog", dialog_name, instance, should_pause)


func _group_for(group_name: String) -> Control:
	match group_name:
		"menu":
			return menu_group
		"hud":
			return hud_group
		"loading":
			return loading_layer
		_:
			return overlay_layer


## The instantiated dialog node by manifest name (null when absent).
func dialog(name: String) -> Node:
	return _dialogs.get(name)


func _connect_prompts() -> void:
	var ui := _ui()
	if ui == null:
		return
	if ui.has_signal("show_info") and not ui.is_connected("show_info", Callable(self, "_on_show_info")):
		ui.connect("show_info", Callable(self, "_on_show_info"))
	if ui.has_signal("toast") and not ui.is_connected("toast", Callable(self, "_on_toast")):
		ui.connect("toast", Callable(self, "_on_toast"))
	if ui.has_signal("announce") and not ui.is_connected("announce", Callable(self, "_on_announce")):
		ui.connect("announce", Callable(self, "_on_announce"))
	if ui.has_signal("show_text") and not ui.is_connected("show_text", Callable(self, "_on_show_text")):
		ui.connect("show_text", Callable(self, "_on_show_text"))
	if ui.has_signal("show_confirm") and not ui.is_connected("show_confirm", Callable(self, "_on_show_confirm")):
		ui.connect("show_confirm", Callable(self, "_on_show_confirm"))
	if ui.has_signal("text_input_request") and not ui.is_connected("text_input_request", Callable(self, "_on_text_input_request")):
		ui.connect("text_input_request", Callable(self, "_on_text_input_request"))


func _on_show_info(text: String) -> void:
	_toast_label(text, 2.0)


func _on_toast(text: String, _icon: String) -> void:
	_toast_label(text, 3.5)


func _on_announce(text: String, duration: float) -> void:
	_toast_label(text, maxf(1.0, duration))


func _toast_label(text: String, duration: float) -> void:
	# code-instantiated: toasts are transient, high-churn overlay labels.
	var label := MindWidgets.label(text)
	label.position = Vector2(24, 24)
	overlay_layer.add_child(label)
	var tween := create_tween()
	tween.tween_interval(duration)
	tween.tween_property(label, "modulate:a", 0.0, 0.4)
	tween.tween_callback(label.queue_free)


func _on_show_text(title: String, text: String) -> void:
	var prompt := _build_prompt(title, text, false)
	_add_prompt_button(prompt.buttons, "OK", func() -> void: prompt.root.queue_free())


func _on_show_confirm(text: String) -> void:
	var prompt := _build_prompt("", text, false)
	_add_prompt_button(prompt.buttons, "Yes", func() -> void:
		var ui := _ui()
		if ui != null:
			ui.call("resolve_confirm", true)
		prompt.root.queue_free())
	_add_prompt_button(prompt.buttons, "No", func() -> void:
		var ui := _ui()
		if ui != null:
			ui.call("resolve_confirm", false)
		prompt.root.queue_free())


func _on_text_input_request(title: String, message: String, max_length: int, default_text: String, numeric: bool, allow_empty: bool) -> void:
	var prompt := _build_prompt(title, message, true)
	var field: LineEdit = prompt.field
	field.text = default_text
	if max_length > 0:
		field.max_length = max_length
	var ok_button := _add_prompt_button(prompt.buttons, "OK", func() -> void:
		var ui := _ui()
		if ui != null:
			ui.call("resolve_text_input", field.text)
		prompt.root.queue_free())
	_add_prompt_button(prompt.buttons, "Cancel", func() -> void:
		var ui := _ui()
		if ui != null:
			ui.call("resolve_text_input", "")
		prompt.root.queue_free())
	ok_button.disabled = not allow_empty and default_text.is_empty()
	if numeric:
		field.text_changed.connect(func(value: String) -> void:
			var filtered := ""
			for ch in value:
				if ch >= "0" and ch <= "9":
					filtered += ch
			if filtered != value:
				field.text = filtered
				field.caret_column = filtered.length())
	field.text_changed.connect(func(value: String) -> void:
		ok_button.disabled = not allow_empty and value.is_empty())
	field.grab_focus()


## Builds a transient prompt panel (`showTextInput`/`showConfirm`/`showText`).
## code-instantiated: prompts are one-off transient overlays; their structure is
## parameterized by the request (field/no-field) and has no static scene.
func _build_prompt(title_text: String, message: String, with_field: bool) -> Dictionary:
	var root := ColorRect.new()
	root.color = Color(0.0, 0.0, 0.0, 0.55)
	root.set_anchors_preset(Control.PRESET_FULL_RECT)
	root.mouse_filter = Control.MOUSE_FILTER_STOP
	overlay_layer.add_child(root)
	var center := CenterContainer.new()
	center.set_anchors_preset(Control.PRESET_FULL_RECT)
	center.mouse_filter = Control.MOUSE_FILTER_IGNORE
	root.add_child(center)
	var panel := PanelContainer.new()
	panel.theme_type_variation = "defaultDialog"
	panel.custom_minimum_size = Vector2(420, 0)
	center.add_child(panel)
	var layout := VBoxContainer.new()
	panel.add_child(layout)
	if not title_text.is_empty():
		var title := Label.new()
		title.text = title_text
		title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		layout.add_child(title)
	var body := Label.new()
	body.text = message
	body.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	layout.add_child(body)
	var field := LineEdit.new()
	if with_field:
		layout.add_child(field)
	var buttons := HBoxContainer.new()
	buttons.alignment = BoxContainer.ALIGNMENT_CENTER
	layout.add_child(buttons)
	return {"root": root, "field": field, "buttons": buttons}


func _add_prompt_button(container: Container, text: String, callback: Callable) -> Button:
	var button := Button.new()
	button.text = text
	button.theme_type_variation = "defaultt"
	button.pressed.connect(callback)
	container.add_child(button)
	return button
