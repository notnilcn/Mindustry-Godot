## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/SaveDialog.java.
##
## `SaveDialog extends LoadDialog` upstream: the same search row, gamemode
## filters and two-column save cards, but the bottom button is `@save.new` and a
## card click asks `@save.overwrite` before re-saving that slot.

extends "res://ui/dialogs/load_dialog.gd"

## Emitted with the slot name a save was requested for (MCP/test observation).
signal save_requested(slot: String)

## Slot awaiting the `@save.overwrite` answer (empty = none).
var _pending_slot := ""


func _ready() -> void:
	# `super("@savegame")` (SaveDialog.java:10).
	title_key = "@savegame"
	super._ready()
	save_requested.connect(_on_save_requested)
	_connect_save_confirm()


## `SaveDialog.addSetup`: `buttons.button("@save.new", Icon.add, ...)
## .fillX().margin(10f)` replaces Load's `@save.import` button.
func _add_setup_buttons() -> void:
	var new_button := add_button(_t("@save.new"), _request_new, "add", 210.0)
	new_button.name = "new_save"
	new_button.custom_minimum_size.y = 64.0


## `SaveDialog.modifyButton`: a card click asks before overwriting
## (`SaveDialog.java:31-39`); the per-card autosave/delete/rename/export
## buttons still come from `LoadDialog`.
func _card_pressed(slot: Dictionary) -> void:
	var stem := _slot_stem(slot)
	if stem.is_empty():
		return
	_pending_slot = stem
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_confirm"):
		ui.call("show_confirm", _t("@save.overwrite"))
	else:
		_emit_save(stem)


func _request_new() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	# `ui.showTextInput("@save", "@save.newslot", 30, "", ...)`
	# (`SaveDialog.java:22-25`).
	ui.call("show_text_input", _t("@save"), _t("@save.newslot"), 30, "", false, false)
	# One answer per prompt (a cancel resolves with ""): a lasting connection
	# would turn every later text prompt into a save.
	if not ui.is_connected("text_input_result", _on_name):
		ui.connect("text_input_result", _on_name, CONNECT_ONE_SHOT)


## `text -> ui.loadAnd("@saving", () -> control.saves.addSave(text))`.
func _on_name(text: String) -> void:
	if text.is_empty():
		return
	var ui_root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui_root != null and ui_root.has_method("load_and"):
		ui_root.call("load_and", _add_save.bind(text), "@saving")
	else:
		_add_save(text)


## `control.saves.addSave(text)` then `Core.app.post(this::setup)`: the dialog
## stays open and relists the slots.
func _add_save(text: String) -> void:
	save_requested.emit(text)
	_refresh_slots()
	_rebuild_list()


func _connect_save_confirm() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_signal("confirm_result") \
			and not ui.is_connected("confirm_result", Callable(self, "_on_save_confirm")):
		ui.connect("confirm_result", Callable(self, "_on_save_confirm"))


func _on_save_confirm(confirmed: bool) -> void:
	if _pending_slot.is_empty():
		return
	var slot := _pending_slot
	_pending_slot = ""
	if confirmed:
		_emit_save(slot)


## `SaveDialog.save`: hand the slot to the live facade, which queues the
## `.msav` write (`MindCampaign.save_slot`), then close.
func _emit_save(slot: String) -> void:
	save_requested.emit(slot)
	_close()


## Queues the write on the live campaign facade (applied at the next IO tick).
func _on_save_requested(slot: String) -> void:
	if slot.is_empty():
		return
	campaign_call("save_slot", [slot])


## `hide()` closes this dialog, not Load Game (`SaveDialog.save`).
func _close() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("close_dialog") and bool(ui.call("close_dialog", "save")):
		return
	hide_dialog()
