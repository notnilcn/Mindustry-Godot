## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/LoadingFragment.java (44-138).
##
## Full-screen loading overlay: progress bar + text + cancel button. The HUD
## refresh polls `MindSimHost.io_pending` and drives `set_progress`/`hide_loading`
## while an async save/load is in flight; `loadAnd` (the async completion helper)
## is owned by the Rust IO flow. Cancel hides the overlay and emits
## `cancel_requested` for a future cancellable IO path.

extends Control

## Emitted when the cancel button is pressed (`hint` parity: `cancelListener`).
signal cancel_requested

@onready var _bar: ProgressBar = get_node_or_null("Panel/Layout/Bar")
@onready var _text: Label = get_node_or_null("Panel/Layout/Text")
@onready var _button: Button = get_node_or_null("Panel/Layout/Button")


func _ready() -> void:
	visible = false
	if _button != null:
		_button.text = _t("@cancel")
		if not _button.pressed.is_connected(_cancel_pressed):
			_button.pressed.connect(_cancel_pressed)


## Shows the overlay at `fraction` (0..1) with an optional `text` message.
func set_progress(fraction: float, text: String = "") -> void:
	visible = true
	# Undo `show_text`'s text-only layout and colour.
	if _button != null:
		_button.visible = true
	if _text != null:
		_text.remove_theme_color_override("font_color")
	if _bar != null:
		_bar.visible = true
		_bar.value = clampf(fraction, 0.0, 1.0) * 100.0
	if _text != null and not text.is_empty():
		_text.text = text


## `show(String text)` (`LoadingFragment.java:111-122`): text only, no bar and
## no cancel button.
func show_text(text: String) -> void:
	visible = true
	if _bar != null:
		_bar.visible = false
	if _button != null:
		_button.visible = false
	if _text != null:
		_set_markup(text)


## `nameLabel` is an Arc markup label: `@loading` is `[accent]Loading...`. The
## scene's plain `Label` takes the leading colour tag as its font colour and
## drops the tags.
func _set_markup(text: String) -> void:
	var color := Color.WHITE
	var tag := RegEx.create_from_string("^\\[([#a-zA-Z0-9]+)\\]")
	var found := tag.search(text)
	if found != null:
		var color_name := found.get_string(1)
		if color_name == "accent":
			color = MindStyles.ACCENT
		elif Color.html_is_valid(color_name):
			color = Color.html(color_name)
		elif Color.from_string(color_name, Color(0, 0, 0, 0)).a > 0.0:
			color = Color.from_string(color_name, Color.WHITE)
	_text.add_theme_color_override("font_color", color)
	_text.text = RegEx.create_from_string("\\[[#a-zA-Z0-9]*\\]").sub(text, "", true)


## Updates the label only.
func set_text(text: String) -> void:
	if _text != null:
		_text.text = text


## Hides the overlay (`hideLoading`).
func hide_loading() -> void:
	visible = false


## Whether the overlay is up.
func is_shown() -> bool:
	return visible


## `LoadingFragment` cancel button: close now and forward the intent.
func _cancel_pressed() -> void:
	hide_loading()
	cancel_requested.emit()


func _t(key: String) -> String:
	var assets := MindWidgets.assets()
	if assets == null:
		return key.trim_prefix("@")
	return str(assets.call("bundle_get", key.trim_prefix("@")))
