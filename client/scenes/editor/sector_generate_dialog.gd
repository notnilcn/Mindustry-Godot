## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/SectorGenerateDialog.java (plan 19 M4 §3.9).
##
## UI-only: planet/sector/seed fields; Apply calls `MindEditor.sector_generate`
## (the plan-06 `load_sector` host owns the generation).

extends MindDialog

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"

var _editor: Node = null
var _planet := "erekir"
var _sector := 0
var _seed := 0


func _ready() -> void:
	super._ready()
	_editor = get_node_or_null(editor_path)
	title_text = "Sector Generate"
	add_close_button()


func shown() -> void:
	_build()


func _build() -> void:
	if cont == null:
		return
	for child in cont.get_children():
		child.queue_free()
	_field("Planet", _planet, func(text: String) -> void: _planet = text)
	_field("Sector", str(_sector), func(text: String) -> void: _sector = int(text))
	_field("Seed", str(_seed), func(text: String) -> void: _seed = int(text))
	var apply := Button.new()
	apply.text = "Apply"
	apply.pressed.connect(_apply)
	if buttons != null:
		buttons.add_child(apply)


func _field(text: String, value: String, handler: Callable) -> void:
	var row := HBoxContainer.new()
	var label := Label.new()
	label.text = text
	row.add_child(label)
	var edit := LineEdit.new()
	edit.text = value
	edit.text_changed.connect(handler)
	row.add_child(edit)
	cont.add_child(row)


func _apply() -> void:
	if _editor != null and _editor.has_method("sector_generate"):
		_editor.call("sector_generate", _planet, _sector, _seed)
	hide_dialog()
