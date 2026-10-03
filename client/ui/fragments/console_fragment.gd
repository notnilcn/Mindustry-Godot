## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/ConsoleFragment.java (plan 14 M7).
##
## In-game console: line editor, history, scroll buttons and the mobile toggle.
## Commands execute through the Rust registry (`MindUi.console_execute`); the
## Rhino JS injection is dropped (deviation OD1). Line history/scroll mirror the
## upstream fragment.

extends Control

const MESSAGES_SHOWN := 30

var _open := false
var _shown := false
var _messages: PackedStringArray = []
var _history: PackedStringArray = [""]
var _history_pos := 0
var _scroll_pos := 0

var _panel: VBoxContainer = null
var _field: LineEdit = null
var _output: RichTextLabel = null


func _ready() -> void:
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
	_panel.offset_right = 900.0
	_panel.custom_minimum_size.y = 320.0
	add_child(_panel)
	_output = RichTextLabel.new()
	_output.bbcode_enabled = true
	_output.scroll_active = true
	_panel.add_child(_output)
	var row := HBoxContainer.new()
	if _mobile():
		# code-instantiated: mobile scroll/clear buttons (upstream mobile branch).
		var up := MindWidgets.image_button("upOpen", "cleari")
		up.pressed.connect(func() -> void: _scroll(1))
		row.add_child(up)
		var down := MindWidgets.image_button("downOpen", "cleari")
		down.pressed.connect(func() -> void: _scroll(-1))
		row.add_child(down)
		var cancel := MindWidgets.image_button("cancel", "cleari")
		cancel.pressed.connect(func() -> void: _shown = false)
		row.add_child(cancel)
	_field = MindWidgets.field(_t("@console"))
	_field.text_submitted.connect(_submit)
	row.add_child(_field)
	_panel.add_child(row)
	_output.text = "> "


func _mobile() -> bool:
	var ui := get_node_or_null("/root/MindUi")
	return ui != null and bool(ui.call("is_mobile"))


func _submit(line: String) -> void:
	if line.strip_edges().is_empty():
		return
	if _history.size() < 2 or _history[1] != line:
		_history.insert(1, line)
	_history_pos = 0
	if line.strip_edges() == "clear":
		_messages.clear()
		_refresh()
		_field.text = ""
		return
	var ui := get_node_or_null("/root/MindUi")
	var result := ""
	# code-instantiated: the result is produced by the Rust command registry.
	if ui != null and ui.has_method("console_execute"):
		result = str(ui.call("console_execute", line))
	_messages.append("[lightgray]> " + line.replace("[", "[lb]"))
	if not result.is_empty():
		_messages.append(result.replace("[", "[lb]"))
	if _messages.size() > MESSAGES_SHOWN:
		_messages = _messages.slice(_messages.size() - MESSAGES_SHOWN)
	_field.text = ""
	_refresh()


func _scroll(direction: int) -> void:
	_scroll_pos = clampi(_scroll_pos + direction, 0, maxi(0, _messages.size()))
	_refresh()


func _refresh() -> void:
	_output.text = "\n".join(_messages)


func toggle() -> void:
	_shown = not _shown
	_open = _shown and not _mobile()
	visible = _shown
	if _shown:
		_field.grab_focus()


func toggle_mobile() -> void:
	_shown = not _shown
	_open = false
	visible = _shown


func is_open() -> bool:
	return _open


func is_shown() -> bool:
	return _shown


func _t(key: String) -> String:
	var assets := MindWidgets.assets()
	if assets == null:
		return key.trim_prefix("@")
	return str(assets.call("bundle_get", key.trim_prefix("@")))
