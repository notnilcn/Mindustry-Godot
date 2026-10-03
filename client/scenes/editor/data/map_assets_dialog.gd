## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/data/MapAssetsDialog.java (plan 19 M6 §3.12).
##
## UI-only: type tab strip + import/export zip + clear-all + per-type view body.
## The asset records and zip I/O are owned by plan 20's `DataManagerApi`
## (`editor/assets.rs`); `MindEditor.assets()` returns the current type's rows.

extends MindDialog

## `DataAssetType.all` folders (`patch/content/bundle/image/sound/music`).
const TYPES := ["patches", "content", "bundles", "sprites", "sounds", "music"]

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"

var _editor: Node = null
var _type := "patches"


func _ready() -> void:
	super._ready()
	_editor = get_node_or_null(editor_path)
	title_text = "Map Assets"
	add_close_button()


func shown() -> void:
	_build()


func _build() -> void:
	if cont == null:
		return
	for child in cont.get_children():
		child.queue_free()
	var tabs := HBoxContainer.new()
	for type in TYPES:
		var button := Button.new()
		button.text = type
		button.toggle_mode = true
		button.button_pressed = type == _type
		button.pressed.connect(_select_type.bind(type))
		tabs.add_child(button)
	cont.add_child(tabs)
	for row: Variant in _assets():
		var value: Dictionary = row if row is Dictionary else {}
		cont.add_child(_label("%s  %s" % [str(value.get("name", "")), str(value.get("path", ""))]))


func _select_type(type: String) -> void:
	_type = type
	shown()


func _assets() -> Array:
	if _editor != null and _editor.has_method("assets"):
		var value: Variant = _editor.call("assets")
		if value is Array:
			return value
	return []


func _label(text: String) -> Label:
	var label := Label.new()
	label.text = text
	return label
