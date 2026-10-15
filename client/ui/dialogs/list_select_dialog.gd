## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/BaseDialog.java picker pattern (the
## anonymous `showUnits`/`showEffects`/spawn-select dialogs in
## `editor/WaveInfoDialog.java:366-430` and the editor menu's map pickers).
##
## Generic one-column picker: a scrollable list of titled rows, an optional
## reset row, and a `selected(name)` signal (empty name = reset/closed).
## Context: `{"title": "@key", "items": ["a", ...], "reset": "@reset.key"}`.

extends MindDialog

## Emitted with the picked entry ("" when the caller resets/closes).
signal selected(name: String)

var _items: Array = []
var _reset_key := ""
## Set while the dialog is hiding after a pick, so `hidden` does not emit again.
var _chosen := false


func _ready() -> void:
	should_pause = false
	super._ready()
	add_close_button()


func set_context_json(json_text: String) -> void:
	super.set_context_json(json_text)
	var ctx := context()
	var title := str(ctx.get("title", ""))
	title_text = _t(title) if not title.is_empty() else ""
	_reset_key = str(ctx.get("reset", ""))
	_items = []
	var items: Variant = ctx.get("items", [])
	if items is Array:
		for item in items:
			_items.append(str(item))


func shown() -> void:
	_chosen = false
	_build()


func hidden() -> void:
	if not _chosen:
		selected.emit("")


func _build() -> void:
	var root := content_table()
	var scroll := ScrollContainer.new()
	scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	root.add(scroll).grow_x_axis().grow_y_axis()
	# code-instantiated: the row list is caller-supplied picker data.
	var list := VBoxContainer.new()
	list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(list)
	if not _reset_key.is_empty():
		# `showUnits(reset = true)`: the reset row.
		var reset := MindWidgets.icon_button("cancel", _t(_reset_key))
		reset.pressed.connect(func() -> void: _pick(""))
		list.add_child(reset)
	var columns := 3
	var index := 0
	var row: HBoxContainer = null
	for item in _items:
		# code-instantiated: picker rows are data-driven (units/effects/spawns).
		var button := MindWidgets.button(str(item))
		button.pressed.connect(_pick.bind(str(item)))
		if index % columns == 0:
			row = HBoxContainer.new()
			row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			list.add_child(row)
		row.add_child(button)
		button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		index += 1
	if _items.is_empty():
		list.add_child(MindWidgets.label("@none.found"))


func _pick(name: String) -> void:
	_chosen = true
	selected.emit(name)
	_close_pressed()
