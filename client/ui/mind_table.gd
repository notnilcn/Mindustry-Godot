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
## Arc `Table.top()`: the grid hugs the top edge instead of the default
## `Align.center` (`Table.java:39`). Dialogs map `cont.top()` onto this.
var table_top := false
## Arc `Table.left()`: the grid hugs the left edge.
var table_left := false


## Layout invalidation (L01 T1), the port of Arc's `invalidateHierarchy`
## (`arc/scene/ui/layout/WidgetGroup.java:98-106`: every add/remove and
## `Table.row()`/`Table.invalidate()` (`Table.java:174-177`, `:647-650`) marks the
## table and each ancestor for layout on the next `validate`).
##
## Godot's analogue is the minimum-size chain: `update_minimum_size()` drops the
## cached minimum of this node and every ancestor and emits
## `minimum_size_changed` (deferred), which each parent `Container` turns into its
## own `update_minimum_size()` + `queue_sort()`. Children are wired by
## `Container.add_child_notify` (their `minimum_size_changed`,
## `visibility_changed` and `size_flags_changed` re-sort this table), so the cases
## the engine cannot see are the ones that live in [MindCell]/table fields:
## cell setters after `add()`, `row()` and `margin()`. Those call [method invalidate].
##
## `queue_sort()` defers to the native `Container::_sort_children`, which only
## sends `NOTIFICATION_SORT_CHILDREN`; a script `_sort_children()` is not a
## virtual and is never called by it. Before L01 T1 this table did not handle the
## notification, so it laid out only when a dialog called [method sort_now] —
## the root of the stale-rect bugs (PAR-0410: the filtered Custom Game grid kept
## the unfiltered grid's 1608 px height and centred its single tile off-screen).
func _notification(what: int) -> void:
	if what == NOTIFICATION_SORT_CHILDREN:
		_sort_children()


## `Table.left()` / back to centre alignment.
func set_table_left(value: bool) -> void:
	table_left = value
	queue_sort()


## `Table.top()` / back to centre alignment.
func set_table_top(value: bool) -> void:
	table_top = value
	queue_sort()


## Marks this table's minimum size and layout dirty and propagates up the parent
## chain (`WidgetGroup.invalidateHierarchy`). Cheap: both calls coalesce into one
## deferred update per frame.
func invalidate() -> void:
	update_minimum_size()
	queue_sort()


## Appends `child` to the current row and returns its [MindCell].
func add(child: Control) -> MindCell:
	var cell := MindCell.new(child)
	if _defaults != null:
		cell.copy_from(_defaults)
	cell.table = self
	_cells.append({"control": child, "cell": cell, "row": _current_row})
	add_child(child)
	invalidate()
	return cell


## Starts a new row (subsequent `add` calls land in it; `Table.row()`
## invalidates, `Table.java:647-650`).
func row() -> void:
	_current_row += 1
	invalidate()


## Removes every cell and child, resetting the row cursor (dialogs that
## rebuild their rows; freed controls otherwise stay in `_cells` and break
## `_grid`).
func clear() -> void:
	for entry in _cells:
		var control: Control = entry["control"]
		if is_instance_valid(control):
			remove_child(control)
			control.queue_free()
	_cells.clear()
	_current_row = 0
	invalidate()


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
	invalidate()
	queue_redraw()


## Forces an immediate layout pass at the current size. Layout normally runs from
## `NOTIFICATION_SORT_CHILDREN` (see [method _notification]); this stays for
## headless checks and callers that must read child rects in the same frame.
func sort_now() -> void:
	_sort_children()


## Removes every child and cell.
func clear_children() -> void:
	for entry in _cells:
		var control: Control = entry.control
		if is_instance_valid(control):
			remove_child(control)
			control.queue_free()
	_cells.clear()
	_current_row = 0
	invalidate()


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

	# Arc `Table.align = Align.center` by default (`Table.java:39`): the whole
	# grid is centred inside the table's bounds when it is smaller. Dialogs whose
	# panel fills the screen (every `BaseDialog`) rely on this to keep their
	# narrow button columns centred like Java.
	var grid_width := _margin_value * 2.0
	for value in col_widths:
		grid_width += value
	var grid_height := _margin_value * 2.0
	for value in row_heights:
		grid_height += value
	var origin := Vector2(
		0.0 if table_left else maxf(0.0, (size.x - grid_width) * 0.5),
		0.0 if table_top else maxf(0.0, (size.y - grid_height) * 0.5)
	)

	for placement in grid.placements:
		var cell: MindCell = placement.cell
		var control: Control = placement.control
		if not is_instance_valid(control):
			continue
		var cell_width := 0.0
		for s in placement.span:
			cell_width += col_widths[placement.col + s]
		var cell_height: float = row_heights[placement.row_index]
		var pos := origin + Vector2(col_x[placement.col], row_y[placement.row_index])
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
		# Visible-only layout, like every Container and Arc's `Table` (a hidden
		# cell must not reserve space; the collapsers depend on this).
		var control: Control = entry.control
		if is_instance_valid(control) and not control.visible:
			continue
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

	var spanning: Array = []
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
		if placement.span == 1:
			col_min[placement.col] = maxf(col_min[placement.col], w)
		else:
			spanning.append([placement, w])
		row_min[placement.row_index] = maxf(row_min[placement.row_index], h)

	# Arc `Table.computeSize`: "Distribute any additional min and pref width
	# added by colspanned cells to the columns spanned" — only the deficit a
	# spanning cell adds over its columns' own widths, spread evenly. Splitting
	# the whole width (the old behaviour) widened narrow columns, e.g. the
	# editor Map Info label column next to the colspan-2 route frame (L03 T3).
	for entry in spanning:
		var placement: Dictionary = entry[0]
		var need: float = entry[1]
		var have := 0.0
		for s in placement.span:
			have += col_min[placement.col + s]
		if need > have:
			var extra := (need - have) / float(placement.span)
			for s in placement.span:
				col_min[placement.col + s] += extra

	return {
		"placements": placements,
		"col_min": col_min,
		"row_min": row_min,
	}
