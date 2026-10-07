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
	if _bar != null:
		_bar.value = clampf(fraction, 0.0, 1.0) * 100.0
	if _text != null and not text.is_empty():
		_text.text = text


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
