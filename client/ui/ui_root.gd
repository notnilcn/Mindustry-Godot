## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/core/UI.java (`init`, `updateMargins`, groups).
##
## The Godot-side UI root under `Spine/Ui`. It owns the layer groups and binds
## every manifest dialog/fragment to the node statically declared by name in
## `scenes/ui/ui_root.tscn`, builds the theme, and registers dialogs with the
## Rust `MindUi` singleton. Layout and lifecycle only — no game rules or sim
## reads (plan 14 §3.2/§3.10).

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

## `Control.java:640` UI-scale confirmation countdown (`60 * 11` frames).
const UISCALE_REVERT_SECONDS := 11

var _dialogs: Dictionary = {}
var _theme_applied := false
var _uiscale_prompt: Dictionary = {}
var _uiscale_countdown := 0.0
var _uiscale_shown_seconds := -1


func _enter_tree() -> void:
	# Static children `_ready` before the parent, so the theme is applied here to
	# be in place before the instanced dialogs/fragments `_ready` runs.
	if not _theme_applied:
		_theme_applied = true
		_apply_theme()


func _ready() -> void:
	_bind_manifest()
	_connect_prompts()
	_apply_ui_scale()
	_boot()
	_check_uiscale_changed()


## `Control.java:640` boot confirmation for a changed UI scale: the prompt
## counts down, `OK` keeps the new scale and `@uiscale.cancel` reverts to 100%
## and exits.
func _check_uiscale_changed() -> void:
	var ui := _ui()
	if ui == null or not ui.has_method("uiscale_changed"):
		return
	if not bool(ui.call("uiscale_changed")):
		return
	_uiscale_countdown = UISCALE_REVERT_SECONDS
	_uiscale_prompt = _build_prompt(
		"", MindWidgets.markup_format("@uiscale.reset", [UISCALE_REVERT_SECONDS]), false
	)
	_add_prompt_button(_uiscale_prompt.buttons, MindWidgets.markup("@ok"), func() -> void:
		_finish_uiscale(true))
	_add_prompt_button(_uiscale_prompt.buttons, MindWidgets.markup("@uiscale.cancel"), func() -> void:
		_finish_uiscale(false))
	set_process(true)


## `Control.java:746-750` (`Binding.menu`, ESC): close the top dialog, hide the
## fullscreen minimap, or open the in-game pause menu. Game-over and
## campaign-complete stay modal (no ESC close), matching upstream.
func _unhandled_key_input(event: InputEvent) -> void:
	var key := event as InputEventKey
	if key == null or not key.pressed or key.echo:
		return
	if key.keycode == KEY_ESCAPE and _on_escape():
		get_viewport().set_input_as_handled()


func _on_escape() -> bool:
	var ui := _ui()
	if ui == null:
		return false
	var minimap := get_node_or_null("HudGroup/minimap") as Control
	if minimap != null and minimap.visible:
		minimap.visible = false
		return true
	if bool(ui.call("has_dialog")):
		var stack: PackedStringArray = ui.call("dialog_stack")
		var top := ""
		if not stack.is_empty():
			top = str(stack[stack.size() - 1])
		if top == "restart" or top == "campaign_complete":
			return false
		ui.call("close_top_dialog")
		return true
	if menu_group.visible:
		return false
	return bool(ui.call("open_dialog", "paused", ""))


func _process(delta: float) -> void:
	if _uiscale_prompt.is_empty():
		return
	_uiscale_countdown -= delta
	var seconds := maxi(0, int(ceil(_uiscale_countdown)))
	if seconds != _uiscale_shown_seconds:
		_uiscale_shown_seconds = seconds
		_uiscale_prompt.body.text = MindWidgets.markup_format("@uiscale.reset", [seconds])
	if _uiscale_countdown <= 0.0:
		_finish_uiscale(false)


func _finish_uiscale(keep: bool) -> void:
	if _uiscale_prompt.is_empty():
		return
	_uiscale_prompt.root.queue_free()
	_uiscale_prompt = {}
	set_process(false)
	var ui := _ui()
	if ui == null:
		return
	ui.call("set_uiscale_changed", false)
	if not keep:
		ui.call("settings_set", "uiscale", 100)
		get_tree().quit()


## `Vars.java:517 Scl.setProduct`: the persisted `uiscale` percent scales the
## whole UI at boot (minimum 25%, matching upstream). The window content scale
## factor is Godot's equivalent of the Scl product.
func _apply_ui_scale() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("settings_rows_json"):
		return
	var parsed: Variant = JSON.parse_string(str(ui.call("settings_rows_json", "graphics")))
	if not (parsed is Array):
		return
	for row in parsed:
		if row is Dictionary and str(row.get("key", "")) == "uiscale":
			var scale := maxf(float(row.get("value", 100)), 25.0) / 100.0
			get_window().content_scale_factor = scale
			return


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


func _bind_manifest() -> void:
	if not FileAccess.file_exists(DIALOGS_MANIFEST):
		push_warning("[ui] missing %s" % DIALOGS_MANIFEST)
		return
	var parsed: Variant = JSON.parse_string(FileAccess.get_file_as_string(DIALOGS_MANIFEST))
	if not (parsed is Dictionary):
		push_warning("[ui] dialogs_manifest is not an object")
		return
	var manifest: Dictionary = parsed
	var expected := {}
	for entry in manifest.get("dialogs", []):
		if _bind(entry, dialog_layer, false):
			expected[str(entry.get("name", ""))] = true
	for entry in manifest.get("fragments", []):
		if _bind(entry, _group_for(str(entry.get("group", ""))), true):
			expected[str(entry.get("name", ""))] = true
	_verify_manifest(expected)


func _bind(entry: Dictionary, parent: Control, is_fragment: bool) -> bool:
	var node_name := str(entry.get("name", ""))
	if node_name.is_empty():
		return false
	var instance := parent.get_node_or_null(NodePath(node_name))
	if instance == null:
		push_error("[ui] manifest entry `%s` has no scene node under %s" % [node_name, parent.name])
		return false
	if is_fragment:
		instance.visible = not bool(entry.get("hidden", false))
		return true
	_dialogs[node_name] = instance
	var ui := _ui()
	if ui != null:
		var should_pause := bool(entry.get("pause", false))
		ui.call("register_dialog", node_name, instance, should_pause)
	return true


## Cross-check the manifest against the statically-instanced scene children
## (mirrors `MindThemeBuilder.verify`): every manifest name resolves, and every
## static child under a layer group is a manifest entry.
func _verify_manifest(expected: Dictionary) -> void:
	for parent in [dialog_layer, menu_group, hud_group, overlay_layer, loading_layer]:
		for child in parent.get_children():
			if not expected.has(str(child.name)):
				push_error("[ui] scene node `%s` under %s is missing from dialogs_manifest" % [child.name, parent.name])


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
	var body := MindWidgets.label(message)
	body.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	layout.add_child(body)
	var field := LineEdit.new()
	if with_field:
		layout.add_child(field)
	var buttons := HBoxContainer.new()
	buttons.alignment = BoxContainer.ALIGNMENT_CENTER
	layout.add_child(buttons)
	return {"root": root, "field": field, "buttons": buttons, "body": body}


func _add_prompt_button(container: Container, text: String, callback: Callable) -> Button:
	var button := Button.new()
	button.text = text
	button.theme_type_variation = "defaultt"
	button.pressed.connect(callback)
	container.add_child(button)
	return button
