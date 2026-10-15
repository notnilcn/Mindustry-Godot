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
## `Vars.finishLaunch` ran (the first `set_menu_visible(true)`).
var _launch_finished := false
## `@mods.initfailed` shown for this boot (`Control.java:75-80`, once).
var _mod_init_failure_shown := false
## The active transient prompt root (`showConfirm`/`showTextInput`/`showText`).
var _active_prompt: Control = null
## Current centred announce box (`UI.announce` replaces the last one).
var _announce_node: Control = null
## Its cancel path (Esc); invalid when no prompt is up.
var _prompt_cancel: Callable = Callable()


func _enter_tree() -> void:
	# Static children `_ready` before the parent, so the theme is applied here to
	# be in place before the instanced dialogs/fragments `_ready` runs.
	if not _theme_applied:
		_theme_applied = true
		_apply_theme()


func _ready() -> void:
	_bind_manifest()
	_connect_prompts()
	_connect_net()
	_apply_ui_scale()
	_boot()
	_check_uiscale_changed()
	_check_mod_init_failure()
	# G07 T13: `UI.java:146` hooks a global click sound on every clickable
	# element; BaseButton covers Button/CheckBox/TextureButton.
	get_tree().node_added.connect(_on_node_added)
	for node in get_tree().root.find_children("*", "BaseButton", true, false):
		_hook_button(node)


func _on_node_added(node: Node) -> void:
	if node is BaseButton:
		_hook_button(node)


func _hook_button(button: BaseButton) -> void:
	if not button.pressed.is_connected(_on_any_button_pressed):
		button.pressed.connect(_on_any_button_pressed)


## `UI.java:146` `ClickListener.clicked = () -> Sounds.uiButton.play()`.
func _on_any_button_pressed() -> void:
	var ui := _ui()
	if ui != null:
		ui.call("play_ui_sound", "uiButton")


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
		"", MindWidgets.markup_format("@uiscale.reset", [UISCALE_REVERT_SECONDS]), false, 400.0
	)
	_add_prompt_button(_uiscale_prompt.buttons, "@ok", "", Vector2(120.0, 54.0), func() -> void:
		_finish_uiscale(true))
	_add_prompt_button(_uiscale_prompt.buttons, "@uiscale.cancel", "", Vector2(120.0, 54.0), func() -> void:
		_finish_uiscale(false))
	set_process(true)


## `Control.java:746-750` (`Binding.menu`, ESC): close the top dialog, hide the
## fullscreen minimap, or open the in-game pause menu. Game-over and
## campaign-complete stay modal (no ESC close), matching upstream. M/F8 are
## owned by the `MindInput` binding path (rebindable), not this handler.
func _unhandled_key_input(event: InputEvent) -> void:
	var key := event as InputEventKey
	if key == null or not key.pressed or key.echo:
		return
	if key.keycode == KEY_ESCAPE and _on_escape():
		get_viewport().set_input_as_handled()


## Transient prompts take Esc before the GUI (a focused `LineEdit` would
## otherwise consume the first press); `closeOnBack` on the prompt dialogs.
func _input(event: InputEvent) -> void:
	if _active_prompt == null:
		return
	var key := event as InputEventKey
	if key == null or not key.pressed or key.echo or key.keycode != KEY_ESCAPE:
		return
	var cancel := _prompt_cancel
	_clear_prompt()
	if cancel.is_valid():
		cancel.call()
	get_viewport().set_input_as_handled()


func _on_escape() -> bool:
	var ui := _ui()
	if ui == null:
		return false
	var loading := get_node_or_null("LoadingLayer/loading") as Control
	if loading != null and loading.visible:
		if loading.has_method("hide_loading"):
			loading.call("hide_loading")
		return true
	var console := get_node_or_null("HudGroup/console")
	if console != null and console.visible and console.has_method("toggle"):
		console.call("toggle")
		return true
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
		var top_dialog := get_node_or_null("DialogLayer/%s" % top)
		if top_dialog != null and top_dialog.has_method("blocks_escape") and bool(top_dialog.call("blocks_escape")):
			return false
		ui.call("close_top_dialog")
		return true
	if menu_group.visible:
		return false
	# The editor shell owns keys while it is open (`MapEditorDialog.doInput`);
	# its Escape opens the editor menu, never the pause menu.
	var editor_dialog := get_node_or_null("/root/Spine/Ui/EditorDialog")
	if editor_dialog != null and (editor_dialog as Control).visible:
		return false
	# `Control.java:733-736`: Esc pauses the sim (`state.set(paused)`) when not
	# in a network game and `pauseDisabled` is off.
	var opened: bool = bool(ui.call("open_dialog", "paused", ""))
	if opened:
		var host := get_node_or_null("/root/Spine/SimHost")
		if host != null and host.has_method("set_paused"):
			host.call("set_paused", true)
	return opened


func _process(delta: float) -> void:
	_drain_logic_messages()
	if _uiscale_prompt.is_empty():
		return
	_uiscale_countdown -= delta
	var seconds := maxi(0, int(ceil(_uiscale_countdown)))
	if seconds != _uiscale_shown_seconds:
		_uiscale_shown_seconds = seconds
		_uiscale_prompt.body.text = MindWidgets.markup_format("@uiscale.reset", [seconds])
	if _uiscale_countdown <= 0.0:
		_finish_uiscale(false)


## World-processor messages (`message`/`localeprint`) drained from the sim host
## and routed through the same `MindUi` surfaces as Java's `FlushMessageI`
## (`announce`/`hudfrag.showToast`/`showInfoToast`); H06 T4. Bundle and
## map-locale resolution happen in core before the drain, so no text lookups
## here.
func _drain_logic_messages() -> void:
	if menu_group != null and menu_group.visible:
		return
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("drain_logic_messages"):
		return
	var ui := _ui()
	if ui == null:
		return
	var rows: Variant = host.call("drain_logic_messages")
	if not (rows is Array):
		return
	for row in rows:
		if not (row is Dictionary):
			continue
		var text := str(row.get("text", ""))
		var duration := float(row.get("duration", 0.0))
		match str(row.get("type", "")):
			"announce":
				ui.call("announce", text, duration)
			"notify":
				ui.call("toast", text, "info")
			"toast":
				# `UI.showInfoToast` visuals are H03's; fall back to the
				# announcement box until `MindUi.show_info_toast` lands.
				if ui.has_method("show_info_toast"):
					ui.call("show_info_toast", text, duration)
				else:
					ui.call("announce", text, duration)


func _finish_uiscale(keep: bool) -> void:
	if _uiscale_prompt.is_empty():
		return
	_uiscale_prompt.root.queue_free()
	_uiscale_prompt = {}
	# `_process` also drains world-processor messages (H06 T4), so it stays on.
	set_process(true)
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
	if menu_visible and not _launch_finished:
		_launch_finished = true
		# `Vars.finishLaunch` (ClientLauncher.java:253): the main menu is up, so
		# the launch succeeded — drop the crash marker and record lastBuild.
		var platform := get_node_or_null("/root/Spine/MindPlatform")
		if platform != null and platform.has_method("finish_launch"):
			platform.call("finish_launch")


## `Control.java:75-80`: after the client load, a boot that skipped mod loading
## shows `@mods.initfailed` (`ui.showInfo`, deferred four ticks upstream).
func _check_mod_init_failure() -> void:
	if _mod_init_failure_shown:
		return
	var mods := get_node_or_null("/root/MindMods")
	if mods == null:
		return
	if not bool(mods.call("skipped_mod_loading")):
		return
	if mods.call("list").is_empty():
		return
	_mod_init_failure_shown = true
	await get_tree().process_frame
	await get_tree().process_frame
	await get_tree().process_frame
	await get_tree().process_frame
	var ui := _ui()
	if ui != null:
		ui.call("show_info", "@mods.initfailed")


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


## Wires manifest dialog intents that cross into the Rust facades.
func _connect_net() -> void:
	var host_dialog := dialog("host")
	if host_dialog != null and host_dialog.has_signal("host_requested"):
		if not host_dialog.is_connected("host_requested", Callable(self, "_on_host_requested")):
			host_dialog.connect("host_requested", Callable(self, "_on_host_requested"))
	var join_dialog := dialog("join")
	if join_dialog == null:
		return
	if join_dialog.has_signal("connect_requested") \
			and not join_dialog.is_connected("connect_requested", Callable(self, "_on_connect_requested")):
		join_dialog.connect("connect_requested", Callable(self, "_on_connect_requested"))
	if join_dialog.has_signal("join_requested") \
			and not join_dialog.is_connected("join_requested", Callable(self, "_on_join_requested")):
		join_dialog.connect("join_requested", Callable(self, "_on_join_requested"))


## `JoinDialog.connect`: direct-connect to the addressed SpacetimeDB server and
## enter the match browser.
func _on_connect_requested(address: String) -> void:
	var net := get_node_or_null("/root/Spine/MindNet")
	if net != null and net.has_method("connect_to"):
		net.call("connect_to", address)


## `JoinDialog.safeConnect`: join the picked public match.
func _on_join_requested(match_id: int) -> void:
	var net := get_node_or_null("/root/Spine/MindNet")
	if net != null and net.has_method("join_match"):
		net.call("join_match", match_id, "")


## `HostDialog.runHost`: hosts the currently loaded state; the lobby metadata
## comes from the live campaign sector or the custom map's `Rules.mode_name`
## (seed/max-players match `HostParams::default()`).
func _on_host_requested(_host_name: String, mode: String) -> void:
	var net := get_node_or_null("/root/Spine/MindNet")
	if net == null or not net.has_method("create_match"):
		return
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	var rules := "{}"
	if campaign != null and campaign.has_method("get_rules_json"):
		rules = str(campaign.call("get_rules_json"))
	var gamemode := mode.trim_prefix("@mode.")
	if gamemode.is_empty():
		gamemode = "survival"
	net.call("create_match", _current_map_id(campaign), 1, gamemode, "public", 8, rules)


## Hosted-state map id: `planet/sector` for a live campaign sector, the custom
## map's `Rules.mode_name`, else the default spine world.
func _current_map_id(campaign: Node) -> String:
	if campaign != null and campaign.has_method("get_sector_state"):
		var state: Variant = campaign.call("get_sector_state")
		if state is Dictionary:
			var dict: Dictionary = state
			var planet := str(dict.get("planet", ""))
			if bool(dict.get("campaign", false)) and not planet.is_empty():
				return "%s-%d" % [planet, int(dict.get("sector", 0))]
	if campaign != null and campaign.has_method("get_rules_json"):
		var parsed: Variant = JSON.parse_string(str(campaign.call("get_rules_json")))
		if parsed is Dictionary:
			var mode_name: Variant = (parsed as Dictionary).get("modeName")
			if mode_name is String:
				var name_text: String = mode_name
				if not name_text.is_empty():
					return name_text
	return "spine"


func _on_show_info(text: String) -> void:
	_toast_label(text, 2.0)


func _on_toast(text: String, _icon: String) -> void:
	# `HudFragment.showToast`: `if(state.isMenu()) return;` then uiNotify.
	if menu_group.visible:
		return
	var ui := _ui()
	if ui != null:
		ui.call("play_ui_sound", "uiNotify")
	_toast_label(text, 3.5)


func _on_announce(text: String, duration: float) -> void:
	_announce_box(text, maxf(1.0, duration))


## `UI.announce(CharSequence, float)` (`UI.java:636-652`): a `Styles.black3`
## table, margin 8, centred `outlineLabel`, at the viewport centre, fading out
## over `duration` with `Interp.pow4In`; a new announce replaces the last.
func _announce_box(text: String, duration: float) -> void:
	# code-instantiated: announces are transient, high-churn centred boxes.
	if _announce_node != null and is_instance_valid(_announce_node):
		_announce_node.queue_free()
	var box := PanelContainer.new()
	var style := StyleBoxFlat.new()
	style.bg_color = Color(0, 0, 0, 0.3)
	style.content_margin_left = 8.0
	style.content_margin_top = 8.0
	style.content_margin_right = 8.0
	style.content_margin_bottom = 8.0
	box.add_theme_stylebox_override("panel", style)
	box.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var label := MindRichLabel.new()
	# H03 T17: producers pass Java markup; translate once here
	# (`UI.announce` label parses markup; `UI.java:636-652`).
	label.text = "[center]" + MindWidgets.markup_text(text)
	box.add_child(label)
	# Centred on the viewport (`setPosition(0, 0, Align.center)`).
	box.set_anchors_preset(Control.PRESET_CENTER)
	box.grow_horizontal = Control.GROW_DIRECTION_BOTH
	box.grow_vertical = Control.GROW_DIRECTION_BOTH
	overlay_layer.add_child(box)
	_announce_node = box
	var tween := create_tween()
	tween.tween_interval(duration)
	tween.set_trans(Tween.TRANS_QUART).set_ease(Tween.EASE_IN)
	tween.tween_property(box, "modulate:a", 0.0, 0.4)
	tween.tween_callback(box.queue_free)


func _toast_label(text: String, duration: float) -> void:
	# code-instantiated: toasts are transient, high-churn overlay labels.
	# H03 T17: `UI.showInfoToast`/`HudFragment.showToast` labels parse markup;
	# producers pass raw bundle/markup text, translated once here.
	var label := MindRichLabel.new()
	label.text = MindWidgets.markup_text(text)
	label.position = Vector2(24, 24)
	overlay_layer.add_child(label)
	var tween := create_tween()
	tween.tween_interval(duration)
	tween.tween_property(label, "modulate:a", 0.0, 0.4)
	tween.tween_callback(label.queue_free)


func _on_show_text(title: String, text: String) -> void:
	# `UI.showText`: Dialog(title) + text (400 wrap) + `@ok` 110x50
	# (UI.java:466-474 showInfo shape).
	var prompt := _build_prompt(title, text, false, 400.0)
	var close := func() -> void:
		prompt.root.queue_free()
		_clear_prompt()
	prompt.cancel = close
	_prompt_cancel = close
	_add_prompt_button(prompt.buttons, "@ok", "", Vector2(110.0, 50.0), close)


func _on_show_confirm(text: String) -> void:
	# `UI.java:582-606 showConfirm(text, cb)`: `@confirm` title, 500 px wrapped
	# body, `@cancel`/`@ok` 200x54 with icons, Enter = OK, Esc = cancel.
	var prompt := _build_prompt(_t("@confirm"), text, false, 500.0)
	var cancel := func() -> void:
		var ui := _ui()
		if ui != null:
			ui.call("resolve_confirm", false)
		prompt.root.queue_free()
		_clear_prompt()
	var confirm := func() -> void:
		var ui := _ui()
		if ui != null:
			ui.call("resolve_confirm", true)
		prompt.root.queue_free()
		_clear_prompt()
	prompt.cancel = cancel
	_prompt_cancel = cancel
	_add_prompt_button(prompt.buttons, "@cancel", "cancel", Vector2(200.0, 54.0), cancel)
	var ok := _add_prompt_button(prompt.buttons, "@ok", "ok", Vector2(200.0, 54.0), confirm)
	ok.grab_focus()


func _on_text_input_request(title: String, message: String, max_length: int, default_text: String, numeric: bool, allow_empty: bool) -> void:
	# `UI.java:309-355 showTextInput` (desktop): accent rule, message, 400x50
	# field, `@cancel`/`@ok` 120x54; Enter = OK, Esc = cancel.
	var prompt := _build_prompt(title, message, true, 400.0)
	var field: LineEdit = prompt.field
	field.text = default_text
	if max_length > 0:
		field.max_length = max_length
	var cancel := func() -> void:
		var ui := _ui()
		if ui != null:
			ui.call("resolve_text_input", "")
		prompt.root.queue_free()
		_clear_prompt()
	var approve := func() -> void:
		var ui := _ui()
		if ui != null:
			ui.call("resolve_text_input", field.text)
		prompt.root.queue_free()
		_clear_prompt()
	prompt.cancel = cancel
	_prompt_cancel = cancel
	_add_prompt_button(prompt.buttons, "@cancel", "", Vector2(120.0, 54.0), cancel)
	var ok := _add_prompt_button(prompt.buttons, "@ok", "", Vector2(120.0, 54.0), approve)
	ok.disabled = not allow_empty and field.text.is_empty()
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
		ok.disabled = not allow_empty and value.is_empty())
	# `UI.java:342-348`: Enter in the field confirms (when non-empty unless
	# `allowEmpty`).
	field.text_submitted.connect(func(_value: String) -> void:
		if allow_empty or not field.text.is_empty():
			approve.call())
	field.grab_focus()


## Clears the active-prompt bookkeeping (used by every prompt exit path).
func _clear_prompt() -> void:
	_active_prompt = null
	_prompt_cancel = Callable()


## Bundle lookup with the key echoed when assets are absent (prompts).
func _t(key: String) -> String:
	var assets := MindWidgets.assets()
	if assets == null:
		return key.trim_prefix("@")
	return str(assets.call("bundle_get", key.trim_prefix("@")))


## Prompt title row: accent label + 3 px rule (`BaseDialog.java:24-25`, the same
## shape as `mind_dialog.gd _ensure_title_underline`).
func _add_prompt_title(layout: VBoxContainer, title_text: String) -> void:
	if title_text.is_empty():
		return
	# code-instantiated: prompt titles are transient and parameterized.
	var title := MindWidgets.label("[center]%s[/center]" % title_text)
	title.add_theme_color_override("default_color", MindStyles.ACCENT)
	title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	layout.add_child(title)
	var rule := ColorRect.new()
	rule.color = MindStyles.ACCENT
	rule.custom_minimum_size = Vector2(0.0, 3.0)
	layout.add_child(rule)


## Builds a transient prompt panel (`showTextInput`/`showConfirm`/`showText`).
## code-instantiated: prompts are one-off transient overlays; their structure is
## parameterized by the request (field/no-field, body width) and has no static
## scene.
func _build_prompt(
	title_text: String, message: String, with_field: bool, body_width: float
) -> Dictionary:
	var root := ColorRect.new()
	root.color = MindStyles.BLACK_9
	root.set_anchors_preset(Control.PRESET_FULL_RECT)
	root.mouse_filter = Control.MOUSE_FILTER_STOP
	# Prompts are topmost (`UI` has one shared layer upstream; the port's
	# `Ui/EditorDialog` is a sibling of `UiRoot`, so raise above it).
	root.z_index = 1000
	overlay_layer.add_child(root)
	_active_prompt = root
	var center := CenterContainer.new()
	center.set_anchors_preset(Control.PRESET_FULL_RECT)
	center.mouse_filter = Control.MOUSE_FILTER_IGNORE
	root.add_child(center)
	var panel := PanelContainer.new()
	panel.theme_type_variation = "defaultDialog"
	center.add_child(panel)
	var layout := VBoxContainer.new()
	layout.add_theme_constant_override("separation", 6)
	panel.add_child(layout)
	_add_prompt_title(layout, title_text)
	if with_field:
		# `UI.java:325`: the 400x4 accent rule above the text-input message.
		var rule := ColorRect.new()
		rule.color = MindStyles.ACCENT
		rule.custom_minimum_size = Vector2(400.0, 4.0)
		layout.add_child(rule)
	var body := MindWidgets.label("[center]%s[/center]" % message)
	body.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	body.custom_minimum_size.x = body_width
	body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	layout.add_child(body)
	var field := LineEdit.new()
	field.custom_minimum_size = Vector2(400.0, 50.0)
	if with_field:
		layout.add_child(field)
	var buttons := HBoxContainer.new()
	buttons.alignment = BoxContainer.ALIGNMENT_CENTER
	buttons.add_theme_constant_override("separation", 4)
	layout.add_child(buttons)
	return {"root": root, "field": field, "buttons": buttons, "body": body}


## Prompt action button (`buttons.defaults().size(...)` + `button("@key", Icon)`).
func _add_prompt_button(
	container: Container, label_key: String, glyph: String, size: Vector2, callback: Callable
) -> Button:
	var button := MindWidgets.icon_button(glyph, _t(label_key))
	button.custom_minimum_size = size
	button.pressed.connect(callback)
	container.add_child(button)
	return button
