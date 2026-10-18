## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/SaveDialog.java (plan 14 §3.4, M3).
##
## Save (extends `LoadDialog` upstream): the live save-slot list plus a trailing
## "new save" row. The new-save row prompts for a name; an existing slot asks for
## the `@save.overwrite` confirmation. Both hand the slot to
## `MindCampaign.save_slot`, which queues the `.msav` write at the next IO tick.

extends MindDialog

## Emitted with the slot name a save was requested for (MCP/test observation).
signal save_requested(slot: String)

var _grid: MindTable = null
## Slot rows for the current open (`Saves.getSaveSlots` snapshot); context wins.
var _slots: Array = []
## Slot awaiting the `@save.overwrite` answer (empty = none).
var _pending_slot := ""


func _ready() -> void:
	set_title_key("@savegame")
	should_pause = false
	super._ready()
	_grid = content_table()
	add_close_button()
	save_requested.connect(_on_save_requested)
	_connect_confirm()


func shown() -> void:
	_refresh_slots()
	_rebuild()


## Context slots (MCP/test injection) win; otherwise the live `Saves` listing
## (`LoadDialog.java:92`), the same source the load dialog reads.
func _refresh_slots() -> void:
	var injected: Variant = context().get("slots", null)
	if injected is Array:
		_slots = injected
		return
	var campaign := campaign_node()
	if campaign == null or not campaign.has_method("list_save_slots"):
		_slots = []
		return
	var listed: Variant = campaign.call("list_save_slots")
	_slots = listed if listed is Array else []


func _rebuild() -> void:
	if _grid == null:
		return
	_grid.clear_children()
	# code-instantiated: the new-save row is a data-driven list entry.
	var new_button := MindWidgets.button(_t("@save.new"))
	new_button.pressed.connect(_request_new)
	_grid.add(new_button).grow_x_axis().pad(4)
	_grid.row()
	for entry in _slots:
		var name := ""
		var label := ""
		if entry is Dictionary:
			var slot: Dictionary = entry
			# `SaveSlot.isHidden`: sector saves are hidden from the list.
			if bool(slot.get("sector", false)):
				continue
			name = _slot_stem(slot)
			label = str(slot.get("name", name))
		else:
			name = str(entry)
			label = name
		if name.is_empty():
			continue
		# code-instantiated: the slot entries are a data-driven list.
		var button := MindWidgets.button(label)
		button.pressed.connect(_request.bind(name))
		_grid.add(button).grow_x_axis().pad(4)
		_grid.row()


## The on-disk stem of a slot row (the identity load/save use for the file).
func _slot_stem(slot: Dictionary) -> String:
	var file := str(slot.get("file", ""))
	var stem := file.get_file().get_basename()
	if stem.is_empty():
		stem = str(slot.get("name", ""))
	return stem


func _request_new() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	# code-instantiated: the save-name prompt is a one-off transient overlay.
	# `ui.showTextInput("@save", "@save.newslot", 30, "", ...)`
	# (`SaveDialog.java:26-27`); `@save.name` is not a bundle key (PAR-0438).
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
## stays open and relists the slots (the overwrite path hides instead).
func _add_save(text: String) -> void:
	save_requested.emit(text)
	_refresh_slots()
	_rebuild()


## `SaveDialog.modifyButton`: an existing slot asks before overwriting
## (`@save.overwrite`); a new name saves directly (`SaveDialog.java:35-41`).
func _request(slot: String) -> void:
	if slot.is_empty():
		return
	if not _has_slot(slot):
		_emit_save(slot)
		return
	_pending_slot = slot
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_confirm"):
		ui.call("show_confirm", _t("@save.overwrite"))
	else:
		_emit_save(slot)


func _has_slot(slot: String) -> bool:
	for entry in _slots:
		if entry is Dictionary and _slot_stem(entry) == slot:
			return true
	return false


func _connect_confirm() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_signal("confirm_result") \
			and not ui.is_connected("confirm_result", Callable(self, "_on_confirm")):
		ui.connect("confirm_result", Callable(self, "_on_confirm"))


func _on_confirm(confirmed: bool) -> void:
	if _pending_slot.is_empty():
		return
	var slot := _pending_slot
	_pending_slot = ""
	if confirmed:
		_emit_save(slot)


## `SaveDialog.save`: hand the slot to the live facade, which queues the `.msav`
## write (`MindCampaign.save_slot`), then close.
func _emit_save(slot: String) -> void:
	save_requested.emit(slot)
	_close()


## Queues the write on the live campaign facade (applied at the next IO tick).
func _on_save_requested(slot: String) -> void:
	if slot.is_empty():
		return
	campaign_call("save_slot", [slot])


func _close() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("close_dialog") and bool(ui.call("close_dialog", "save")):
		return
	hide_dialog()
