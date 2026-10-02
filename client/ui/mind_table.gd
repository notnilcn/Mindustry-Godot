## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: arc.scene.ui.layout.Table / Cell (plan 14 §3.3, OD-UI1).
##
## A scene2d-style table: children are placed into rows of cells with
## grow/fill/expand, per-cell padding, colspan and min/max sizing. The fluent
## `add(child).grow().pad(...)` API mirrors `Table.add(...)`. This M0 port covers
## the subset the shipped dialogs use; the full 20-case golden matrix lands with
## plan 14 M1/M8.

class_name MindTable
extends Container

var _cells: Array = []
var _current_row := 0
var _defaults: MindCell = null
var _background_name := ""
var _margin_value := 0.0


## Appends `child` to the current row and returns its [MindCell].
func add(child: Control) -> MindCell:
	var cell := MindCell.new(child)
	if _defaults != null:
		cell.copy_from(_defaults)
	_cells.append({"control": child, "cell": cell, "row": _current_row})
	add_child(child)
	queue_sort()
	return cell


## Starts a new row (subsequent `add` calls land in it).
func row() -> void:
	_current_row += 1


## Returns the cell whose flags apply to subsequent `add` calls (`defaults()`).
func defaults() -> MindCell:
	if _defaults == null:
		_defaults = MindCell.new()
	return _defaults


## Sets the background style name resolved from the theme (`_draw` pass).
func background(style_name: String) -> void:
	_background_name = style_name
	queue_redraw()


## Sets the table's outer margin.
func margin(value: float) -> void:
	_margin_value = value
	queue_sort()
	queue_redraw()


## Removes every child and cell.
func clear_children() -> void:
	for entry in _cells:
		var control: Control = entry.control
		if is_instance_valid(control):
			remove_child(control)
			control.queue_free()
	_cells.clear()
	_current_row = 0
	queue_sort()


func _draw() -> void:
	if _background_name.is_empty():
		return
	var box := get_theme_stylebox(_background_name, "MindTable")
	if box != null:
		draw_style_box(box, Rect2(Vector2.ZERO, size))


func _get_minimum_size() -> Vector2:
	var grid := _grid()
	var min_width := _margin_value * 2.0
	var min_height := _margin_value * 2.0
	for value in grid.col_min:
		min_width += value
	for value in grid.row_min:
		min_height += value
	return Vector2(min_width, min_height)


func _sort_children() -> void:
	var grid := _grid()
	var min_size := _get_minimum_size()
	var extra := size - min_size

	# Grow columns/rows: honor grow_x/grow_y/expand_x/expand_y.
	var col_extra := 0.0
	var row_extra := 0.0
	var grow_col := 0
	var grow_row := 0
	for placement in grid.placements:
		var cell: MindCell = placement.cell
		if cell.grow_x or cell.expand_x:
			grow_col += placement.span
		if cell.grow_y or cell.expand_y:
			grow_row += 1
	col_extra = maxf(0.0, extra.x) / maxf(1.0, float(grow_col))
	row_extra = maxf(0.0, extra.y) / maxf(1.0, float(grow_row))

	var col_widths: Array = []
	col_widths.resize(grid.col_min.size())
	for i in grid.col_min.size():
		col_widths[i] = grid.col_min[i]
	for placement in grid.placements:
		var cell: MindCell = placement.cell
		if cell.grow_x or cell.expand_x:
			for s in placement.span:
				col_widths[placement.col + s] += col_extra

	var row_heights: Array = []
	row_heights.resize(grid.row_min.size())
	for i in grid.row_min.size():
		row_heights[i] = grid.row_min[i]
	for placement in grid.placements:
		var cell: MindCell = placement.cell
		if cell.grow_y or cell.expand_y:
			row_heights[placement.row_index] += row_extra

	var col_x: Array = []
	var running := _margin_value
	for value in col_widths:
		col_x.append(running)
		running += value
	var row_y: Array = []
	running = _margin_value
	for value in row_heights:
		row_y.append(running)
		running += value

	for placement in grid.placements:
		var cell: MindCell = placement.cell
		var control: Control = placement.control
		if not is_instance_valid(control):
			continue
		var cell_width := 0.0
		for s in placement.span:
			cell_width += col_widths[placement.col + s]
		var cell_height: float = row_heights[placement.row_index]
		var pos := Vector2(col_x[placement.col], row_y[placement.row_index])
		var inner := Rect2(
			pos + Vector2(cell.pad_left, cell.pad_top),
			Vector2(
				maxf(0.0, cell_width - cell.pad_left - cell.pad_right),
				maxf(0.0, cell_height - cell.pad_top - cell.pad_bottom)
			)
		)
		_place(control, inner, cell)


func _place(control: Control, rect: Rect2, cell: MindCell) -> void:
	var cell_size := rect.size
	var min_dim := control.get_combined_minimum_size()
	var final_size := min_dim
	if cell.fill_x or cell.grow_x:
		final_size.x = cell_size.x
	if cell.fill_y or cell.grow_y:
		final_size.y = cell_size.y
	if cell.width_value >= 0.0:
		final_size.x = cell.width_value
	if cell.height_value >= 0.0:
		final_size.y = cell.height_value
	if cell.size_value >= 0.0:
		final_size = Vector2(cell.size_value, cell.size_value)
	if cell.max_width >= 0.0:
		final_size.x = minf(final_size.x, cell.max_width)
	if cell.max_height >= 0.0:
		final_size.y = minf(final_size.y, cell.max_height)

	var offset := Vector2.ZERO
	match cell.align:
		1:
			offset.x = (cell_size.x - final_size.x) * 0.5
		3:
			offset.y = (cell_size.y - final_size.y) * 0.5
		4:
			offset.x = cell_size.x - final_size.x
			offset.y = (cell_size.y - final_size.y) * 0.5
		2:
			offset.y = cell_size.y - final_size.y
			offset.x = (cell_size.x - final_size.x) * 0.5
		5:
			pass
		6:
			offset.x = cell_size.x - final_size.x
		7:
			offset.y = cell_size.y - final_size.y
		8:
			offset.x = cell_size.x - final_size.x
			offset.y = cell_size.y - final_size.y
		_:
			offset = (cell_size - final_size) * 0.5
	control.position = rect.position + offset
	control.size = final_size


func _grid() -> Dictionary:
	var rows := {}
	var order: Array = []
	for entry in _cells:
		var r: int = entry.row
		if not rows.has(r):
			rows[r] = []
			order.append(r)
		rows[r].append(entry)

	var placements: Array = []
	var col_count := 0
	for idx in order.size():
		var r: int = order[idx]
		var col := 0
		for entry in rows[r]:
			var cell: MindCell = entry.cell
			var span: int = max(1, cell.colspan)
			placements.append({
				"control": entry.control,
				"cell": cell,
				"col": col,
				"span": span,
				"row_index": idx,
			})
			col += span
		col_count = max(col_count, col)

	var col_min: Array = []
	col_min.resize(col_count)
	for i in col_count:
		col_min[i] = 0.0
	var row_min: Array = []
	row_min.resize(order.size())
	for i in order.size():
		row_min[i] = 0.0

	for placement in placements:
		var cell: MindCell = placement.cell
		var control: Control = placement.control
		if not is_instance_valid(control):
			continue
		var min_size := control.get_combined_minimum_size()
		var w: float = min_size.x
		var h: float = min_size.y
		if cell.width_value >= 0.0:
			w = maxf(w, cell.width_value)
		if cell.size_value >= 0.0:
			w = maxf(w, cell.size_value)
		if cell.min_width >= 0.0:
			w = maxf(w, cell.min_width)
		if cell.max_width >= 0.0:
			w = minf(w, cell.max_width)
		if cell.height_value >= 0.0:
			h = maxf(h, cell.height_value)
		if cell.size_value >= 0.0:
			h = maxf(h, cell.size_value)
		if cell.min_height >= 0.0:
			h = maxf(h, cell.min_height)
		if cell.max_height >= 0.0:
			h = minf(h, cell.max_height)
		w += cell.pad_left + cell.pad_right
		h += cell.pad_top + cell.pad_bottom
		var per := w / float(placement.span)
		for s in placement.span:
			col_min[placement.col + s] = maxf(col_min[placement.col + s], per)
		row_min[placement.row_index] = maxf(row_min[placement.row_index], h)

	return {
		"placements": placements,
		"col_min": col_min,
		"row_min": row_min,
	}
