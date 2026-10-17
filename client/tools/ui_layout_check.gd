# SPDX-License-Identifier: GPL-3.0-only
#
# Headless layout-engine check (L01 T1): `MindTable` minimum-size propagation.
#
# Arc re-lays a `Table` and every ancestor after each add/row/clear/cell change
# (`WidgetGroup.invalidateHierarchy`, arc/scene/ui/layout/WidgetGroup.java:98-106;
# `Table.row`/`Table.invalidate`, Table.java:174-177, :647-650). The port must do
# the same through Godot's minimum-size chain: after a rebuild, one frame later,
# every parent rect matches the new content without any `sort_now()` call.
#
# Cases:
#   1. a `MindTable` in a `CenterContainer`: 6 cells, measure, clear, refill
#      with 1 wide cell whose width is set AFTER `add()`, wait one frame, assert
#      the parent placed the table at its new minimum and the cell is laid out;
#   2. the same inside a `MindScroll` (content table fills the scroll area and
#      the scroll range follows the content);
#   3. the PAR-0410 shape (ScrollContainer > VBox > MindTable > nested
#      MindTable grid, as `MapListDialog` builds it): after a filter rebuild the
#      nested grid shrinks to its new minimum and its tile is visible;
#   4. hiding a cell's control and `margin()` re-measure the table.
#
# Usage:
#   godot4 --headless --path client --script res://tools/ui_layout_check.gd
#
# Exit code 0 when every assertion holds, 1 otherwise.

extends SceneTree

var _failures := 0


func _check(condition: bool, message: String) -> void:
	if condition:
		print("  ok   ", message)
	else:
		_failures += 1
		printerr("  FAIL ", message)


func _near(a: Vector2, b: Vector2) -> bool:
	return absf(a.x - b.x) <= 0.5 and absf(a.y - b.y) <= 0.5


## A fixed-minimum leaf cell.
func _leaf(w: float, h: float) -> Control:
	var c := Control.new()
	c.custom_minimum_size = Vector2(w, h)
	return c


func _frames(n: int = 1) -> void:
	for i in n:
		await process_frame


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	await _case_center()
	await _case_scroll()
	await _case_nested_filter()
	await _case_hidden_and_margin()
	print("LAYOUTCHECK: failed=", _failures)
	quit(1 if _failures > 0 else 0)


func _case_center() -> void:
	print("case 1: MindTable in a CenterContainer")
	var center := CenterContainer.new()
	center.size = Vector2(800, 600)
	root.add_child(center)
	var table := MindTable.new()
	center.add_child(table)
	for i in 6:
		if i > 0 and i % 3 == 0:
			table.row()
		table.add(_leaf(50, 40)).pad(4)
	await _frames()
	var first_min := table.get_combined_minimum_size()
	_check(_near(first_min, Vector2(174, 96)), "6 cells (3x2, pad 4) measure 174x96 (got %s)" % first_min)
	_check(_near(table.size, first_min), "parent sized the table to its minimum (got %s)" % table.size)
	_check(
		_near(table.position, (center.size - first_min) * 0.5),
		"CenterContainer centred the 6-cell table (got %s)" % table.position
	)

	table.clear()
	var wide := _leaf(10, 30)
	# Width set AFTER add(): the chained cell setter must invalidate the table.
	table.add(wide).width(300).pad(2)
	await _frames()
	var new_min := table.get_combined_minimum_size()
	_check(_near(new_min, Vector2(304, 34)), "refilled table measures 304x34 (got %s)" % new_min)
	_check(_near(table.size, new_min), "parent rect equals the new minimum after one frame (got %s)" % table.size)
	_check(
		_near(table.position, (center.size - new_min) * 0.5),
		"CenterContainer re-centred the refilled table (got %s)" % table.position
	)
	_check(
		_near(wide.position, Vector2(2, 2)) and _near(wide.size, Vector2(300, 30)),
		"the wide cell is laid out without sort_now() (got %s %s)" % [wide.position, wide.size]
	)
	center.free()


func _case_scroll() -> void:
	print("case 2: MindTable inside MindScroll")
	var scroll := MindScroll.new()
	scroll.size = Vector2(400, 300)
	root.add_child(scroll)
	var table := scroll.content()
	_check(table != null, "MindScroll created its content table")
	for i in 6:
		table.add(_leaf(100, 80))
		table.row()
	await _frames()
	_check(
		_near(table.get_combined_minimum_size(), Vector2(100, 480)),
		"6 stacked cells measure 100x480 (got %s)" % table.get_combined_minimum_size()
	)
	_check(is_equal_approx(table.size.y, 480.0), "tall content table is 480 high (got %s)" % table.size.y)
	_check(scroll.has_scroll(), "the scroll pane scrolls the tall content")

	table.clear()
	var wide := _leaf(10, 40)
	table.add(wide).width(360)
	await _frames()
	var area := scroll.size
	_check(
		_near(table.get_combined_minimum_size(), Vector2(360, 40)),
		"refilled content measures 360x40 (got %s)" % table.get_combined_minimum_size()
	)
	_check(
		_near(table.size, area),
		"content table fills the scroll area again after one frame (got %s, area %s)" % [table.size, area]
	)
	_check(not scroll.has_scroll(), "no stale scroll range after the refill")
	_check(
		_near(wide.position, Vector2((area.x - 360) * 0.5, (area.y - 40) * 0.5)),
		"the wide cell is centred in the content table (Table align center; got %s)" % wide.position
	)
	scroll.free()


func _case_nested_filter() -> void:
	print("case 3: PAR-0410 nesting (scroll > vbox > table > grid), filter rebuild")
	var body := ScrollContainer.new()
	body.size = Vector2(1136, 493)
	body.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	root.add_child(body)
	var content := VBoxContainer.new()
	content.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	body.add_child(content)
	var outer := MindTable.new()
	outer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	content.add_child(outer)
	var grid := MindTable.new()
	outer.add(grid).expand_x_axis().set_pad_left(28.0).set_pad_right(12.0)
	var last: Control = null
	for i in 30:
		if i > 0 and i % 5 == 0:
			grid.row()
		last = _leaf(200, 252)
		grid.add(last).width(200).bottom().pad(8)
	await _frames()
	var full_h := grid.size.y
	_check(is_equal_approx(full_h, 6 * 268.0), "unfiltered grid is 6 rows high (got %s)" % full_h)
	_check(
		_near(last.position, Vector2(4 * 216 + 8, 5 * 268 + 8)),
		"the 30th tile sits in row 6, column 5 (got %s)" % last.position
	)
	# The dialog used to force the stale rect itself (sort_now at the old size);
	# reproduce that so the refill must shrink an explicitly sized grid.
	grid.size = Vector2(1080, full_h)

	grid.clear_children()
	var tile := _leaf(200, 252)
	grid.add(tile).width(200).bottom().pad(8)
	await _frames()
	_check(
		_near(grid.size, grid.get_combined_minimum_size()),
		"filtered grid shrinks to its new minimum (got %s, min %s)" % [grid.size, grid.get_combined_minimum_size()]
	)
	_check(is_equal_approx(outer.size.y, 268.0), "outer table follows (268 high, got %s)" % outer.size.y)
	# Pane 1136 wide; pads 28/12 leave 1096; the 216 px grid centres in it.
	_check(
		_near(grid.position, Vector2(28 + (1096 - 216) * 0.5, 0)),
		"outer table centres the filtered grid in its expanded cell (got %s)" % grid.position
	)
	_check(_near(tile.position, Vector2(8, 8)), "the filtered tile is placed at its pad (got %s)" % tile.position)
	var tile_y := tile.get_global_rect().position.y - body.get_global_rect().position.y
	_check(
		tile_y >= 0.0 and tile_y + 252.0 <= body.size.y,
		"the filtered tile is inside the visible scroll area (y %s)" % tile_y
	)
	body.free()


func _case_hidden_and_margin() -> void:
	print("case 4: visibility and margin re-measure")
	var center := CenterContainer.new()
	center.size = Vector2(600, 400)
	root.add_child(center)
	var table := MindTable.new()
	center.add_child(table)
	var a := _leaf(100, 50)
	var b := _leaf(100, 50)
	table.add(a)
	table.row()
	table.add(b)
	await _frames()
	_check(_near(table.size, Vector2(100, 100)), "two rows measure 100x100 (got %s)" % table.size)
	b.visible = false
	await _frames()
	_check(_near(table.size, Vector2(100, 50)), "hiding a cell shrinks the table (got %s)" % table.size)
	table.margin(10)
	await _frames()
	_check(_near(table.size, Vector2(120, 70)), "margin(10) grows the table by 20x20 (got %s)" % table.size)
	_check(_near(a.position, Vector2(10, 10)), "the cell sits inside the margin (got %s)" % a.position)
	center.free()
