## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/MapLocalesDialog.java (plan 19 M6 §3.9).
##
## UI-only: locale list + property cards. `MapLocales` JSON and its validation
## are owned by `MindEditor.locales_json`/`set_locales_json` (`maps/locales.rs`).

extends MindDialog

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"

var _editor: Node = null
var _list: VBoxContainer = null


func _ready() -> void:
	super._ready()
	_editor = get_node_or_null(editor_path)
	title_text = "Locales"
	add_close_button()


func shown() -> void:
	_build()


func _build() -> void:
	if cont == null:
		return
	for child in cont.get_children():
		child.queue_free()
	_list = VBoxContainer.new()
	cont.add_child(_list)
	var actions := HBoxContainer.new()
	actions.add_child(_button("Add Locale", _add_locale))
	actions.add_child(_button("Apply To All", _apply_to_all))
	cont.add_child(actions)
	_rebuild()


func _rebuild() -> void:
	if _list == null:
		return
	for child in _list.get_children():
		child.queue_free()
	var locales := _locales()
	for locale: String in locales.keys():
		var card := VBoxContainer.new()
		card.add_child(_label(locale))
		var properties: Dictionary = locales[locale] if locales[locale] is Dictionary else {}
		for key: String in properties.keys():
			card.add_child(_label("  %s = %s" % [key, str(properties[key])]))
		_list.add_child(card)


func _locales() -> Dictionary:
	if _editor == null or not _editor.has_method("locales_json"):
		return {}
	var parsed: Variant = JSON.parse_string(str(_editor.call("locales_json")))
	return parsed if parsed is Dictionary else {}


func _write(locales: Dictionary) -> void:
	if _editor != null and _editor.has_method("set_locales_json"):
		_editor.call("set_locales_json", JSON.stringify(locales))


func _add_locale() -> void:
	var locales := _locales()
	locales["en"] = locales.get("en", {})
	_write(locales)
	_rebuild()


func _apply_to_all() -> void:
	var locales := _locales()
	var keys: Array = locales.keys()
	if keys.is_empty():
		return
	var source: Dictionary = locales[keys[0]]
	for locale: String in keys:
		var target: Dictionary = locales[locale]
		for key: String in source.keys():
			if not target.has(key):
				target[key] = source[key]
	_write(locales)
	_rebuild()


func _button(text: String, handler: Callable) -> Button:
	var button := Button.new()
	button.text = text
	button.pressed.connect(handler)
	return button


func _label(text: String) -> Label:
	var label := Label.new()
	label.text = text
	return label
