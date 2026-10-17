## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: arc.scene.ui.layout.Cell (scene2d cell property surface).
##
## Fluent per-cell configuration for [MindTable]. Values are interpreted in
## Scl-scaled pixels by the table's layout pass (plan 14 §3.3, OD-UI1).

class_name MindCell
extends RefCounted

var control: Control = null
## The owning [MindTable] (set by `MindTable.add`); size-affecting setters
## invalidate it (L01 T1). Null for `MindTable.defaults()` and detached cells.
var table: MindTable = null

var grow_x := false
var grow_y := false
var fill_x := false
var fill_y := false
var expand_x := false
var expand_y := false
var uniform := false
var uniform_x := false
var uniform_y := false

var width_value := -1.0
var height_value := -1.0
var size_value := -1.0
var min_width := -1.0
var max_width := -1.0
var min_height := -1.0
var max_height := -1.0

var pad_left := 0.0
var pad_right := 0.0
var pad_top := 0.0
var pad_bottom := 0.0
var colspan := 1
var align := 0 ## 0=center, 1=top, 2=bottom, 3=left, 4=right, 5=topLeft, 6=topRight, 7=bottomLeft, 8=bottomRight
var label_align := 0
var color := Color.WHITE
var disabled := false


func _init(c: Control = null) -> void:
	control = c


## Size-affecting change: re-measure the owning table and its ancestors
## (`Table.invalidateHierarchy`, Arc `WidgetGroup.java:98-101`). Arc cells are
## read in the table's next `layout()`; Godot caches minimum sizes, so the
## chained `add(x).width(...)`/`.pad(...)` after `add()` must drop that cache.
func _changed() -> MindCell:
	if table != null and is_instance_valid(table):
		table.invalidate()
	return self


## Copies layout defaults into this cell (used by `MindTable.defaults()`).
func copy_from(other: MindCell) -> void:
	grow_x = other.grow_x
	grow_y = other.grow_y
	fill_x = other.fill_x
	fill_y = other.fill_y
	expand_x = other.expand_x
	expand_y = other.expand_y
	uniform = other.uniform
	uniform_x = other.uniform_x
	uniform_y = other.uniform_y
	width_value = other.width_value
	height_value = other.height_value
	size_value = other.size_value
	min_width = other.min_width
	max_width = other.max_width
	min_height = other.min_height
	max_height = other.max_height
	pad_left = other.pad_left
	pad_right = other.pad_right
	pad_top = other.pad_top
	pad_bottom = other.pad_bottom
	align = other.align
	label_align = other.label_align


func grow() -> MindCell:
	grow_x = true
	grow_y = true
	return _changed()


func grow_x_axis() -> MindCell:
	grow_x = true
	return _changed()


func grow_y_axis() -> MindCell:
	grow_y = true
	return _changed()


func fill() -> MindCell:
	fill_x = true
	fill_y = true
	return _changed()


func fill_x_axis() -> MindCell:
	fill_x = true
	return _changed()


func fill_y_axis() -> MindCell:
	fill_y = true
	return _changed()


func expand() -> MindCell:
	expand_x = true
	expand_y = true
	return _changed()


func expand_x_axis() -> MindCell:
	expand_x = true
	return _changed()


func expand_y_axis() -> MindCell:
	expand_y = true
	return _changed()


func set_uniform() -> MindCell:
	uniform = true
	return _changed()


func set_uniform_x() -> MindCell:
	uniform_x = true
	return _changed()


func set_uniform_y() -> MindCell:
	uniform_y = true
	return _changed()


func top() -> MindCell:
	align = 1
	return _changed()


func left() -> MindCell:
	align = 3
	return _changed()


func bottom() -> MindCell:
	align = 2
	return _changed()


func right() -> MindCell:
	align = 4
	return _changed()


func width(w: float) -> MindCell:
	width_value = w
	return _changed()


func height(h: float) -> MindCell:
	height_value = h
	return _changed()


func size(s: float) -> MindCell:
	size_value = s
	return _changed()


func set_min_width(w: float) -> MindCell:
	min_width = w
	return _changed()


func set_max_width(w: float) -> MindCell:
	max_width = w
	return _changed()


func set_min_height(h: float) -> MindCell:
	min_height = h
	return _changed()


func set_max_height(h: float) -> MindCell:
	max_height = h
	return _changed()


func pad(p: float) -> MindCell:
	pad_left = p
	pad_right = p
	pad_top = p
	pad_bottom = p
	return _changed()


func set_pad_top(p: float) -> MindCell:
	pad_top = p
	return _changed()


func set_pad_left(p: float) -> MindCell:
	pad_left = p
	return _changed()


func set_pad_bottom(p: float) -> MindCell:
	pad_bottom = p
	return _changed()


func set_pad_right(p: float) -> MindCell:
	pad_right = p
	return _changed()


func set_align(value: int) -> MindCell:
	align = value
	return _changed()


func set_label_align(value: int) -> MindCell:
	label_align = value
	return self


func set_colspan(value: int) -> MindCell:
	colspan = max(1, value)
	return _changed()


func set_color(value: Color) -> MindCell:
	color = value
	return self


func set_disabled(value: bool) -> MindCell:
	disabled = value
	return self
