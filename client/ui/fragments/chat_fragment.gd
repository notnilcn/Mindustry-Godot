## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/ChatFragment.java (plan 14 M7).
##
## In-game chat: modes (normal/team/admin), history and message rendering. The
## message text is validated/forwarded through `MindUi.chat_send` (transport is
## plan 21's relay; the shell never fakes a server round trip). The rolling
## buffer + fade timing mirror the Rust `ui::chat` model, which is unit-tested.

extends Control

const MESSAGES_SHOWN := 10
const MODES := ["normal", "team", "admin"]
const PREFIXES := ["", "/t", "/a"]

var _messages: PackedStringArray = []
var _history: PackedStringArray = [""]
var _history_pos := 0
var _mode := 0
var _fade := 0.0
var _shown := false

var _panel: VBoxContainer = null
var _field: LineEdit = null
var _log: RichTextLabel = null
var _mode_button: Button = null


func _ready() -> void:
	# code-instantiated: chat is a persistent HUD fragment whose rows/messages
	# are runtime data, so the layout is built once here (plan 14 §3.10).
	set_anchors_preset(Control.PRESET_BOTTOM_LEFT)
	_build()
	visible = false
	set_process(true)


func _build() -> void:
	_panel = VBoxContainer.new()
	_panel.anchor_left = 0.0
	_panel.anchor_top = 1.0
	_panel.anchor_bottom = 1.0
	_panel.offset_bottom = -8.0
	_panel.offset_left = MindUiMargins.left()
	_panel.offset_right = 720.0
	add_child(_panel)
	_log = RichTextLabel.new()
	_log.bbcode_enabled = true
	_log.fit_content = true
	_log.scroll_active = false
	_panel.add_child(_log)
	var row := HBoxContainer.new()
	_mode_button = MindWidgets.button("")
	_mode_button.pressed.connect(_next_mode)
	row.add_child(_mode_button)
	_field = MindWidgets.field(_t("@chat.message"))
	_field.text_submitted.connect(_submit)
	row.add_child(_field)
	_panel.add_child(row)
	if _mobile():
		_panel.offset_bottom = -105.0
		_panel.offset_right = 480.0
	_refresh_mode()
	_refresh_log()


func _mobile() -> bool:
	var ui := get_node_or_null("/root/MindUi")
	return ui != null and bool(ui.call("is_mobile"))


func _request_send() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("chat_send"):
		return
	var text := "%s%s" % [PREFIXES[_mode], _field.text]
	ui.call("chat_send", text, MODES[_mode])
	_field.text = ""


func _next_mode() -> void:
	var previous := PREFIXES[_mode]
	var admin := false
	_mode = (_mode + 1) % MODES.size()
	while MODES[_mode] == "admin" and not admin:
		_mode = (_mode + 1) % MODES.size()
	if _field.text.begins_with(previous):
		_field.text = PREFIXES[_mode] + _field.text.substr(previous.length())
	_refresh_mode()


func _refresh_mode() -> void:
	_mode_button.text = MODES[_mode] if not PREFIXES[_mode].is_empty() else "@"


func _submit(_text: String) -> void:
	_request_send()


## Adds a message to the rolling buffer (called by the HUD/relay).
func add_message(message: String) -> void:
	if message.is_empty():
		return
	_messages.insert(0, message)
	if _messages.size() > MESSAGES_SHOWN:
		_messages.remove_at(_messages.size() - 1)
	_fade = minf(_fade + 1.0, float(MESSAGES_SHOWN)) + 1.0
	_refresh_log()
	if not _shown:
		_fade = maxf(_fade, 1.0)


func _refresh_log() -> void:
	_log.text = "\n".join(_messages)


func _process(delta: float) -> void:
	if _fade > 0.0 and not _shown:
		_fade = maxf(0.0, _fade - delta / 3.0)
		_log.modulate.a = clampf(_fade, 0.0, 1.0)


func toggle() -> void:
	_shown = not _shown
	visible = _shown
	if _shown:
		_field.grab_focus()
	else:
		_field.release_focus()


func is_open() -> bool:
	return _shown


func _t(key: String) -> String:
	var assets := MindWidgets.assets()
	if assets == null:
		return key.trim_prefix("@")
	return str(assets.call("bundle_get", key.trim_prefix("@")))
