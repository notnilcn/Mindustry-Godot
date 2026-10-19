## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/BannedContentDialog.java
## (call sites `ui/dialogs/CustomRulesDialog.java:36-37,189,210`).
##
## The two-pane ban picker: search + clear, the block category filter row, the
## red banned / accent unbanned panes of 50px content cells, the per-pane
## `@addall` buttons and the `@empty` placeholders. Content rows and the
## `rules` tag round-trip come from `MindEditor` (`bannable_content_json` /
## `banned_names_json` / `set_banned_names_json`); the set model is
## `mind_core::editor::banned`.
##
## Context: `{"kind": "block"|"unit", "rules": {...}}`. With `rules` the dialog
## edits that caller-owned dictionary and emits `banned_edited(json)` per change
## (`CustomRulesDialog` keeps the working copy); without it the dialog edits the
## editor's map `rules` tag directly.

extends MindDialog

## Emitted with the updated rules JSON when the dialog edits caller-owned rules.
signal banned_edited(json: String)

## `BannedContentDialog.rebuildTable`: 50px cells, 32px icons (`resizeImage(8*4f)`).
const CELL := 50.0
const ICON := 32.0
## `TabHoverColor` / Java `Color.valueOf("f25555")` banned accent.
const COLOR_BANNED := Color("f25555")

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"
## Fallback kind when the context carries none (`ContentType.block`).
@export var content_type := "block"

var _editor: Node = null
var _kind := "block"
## Working rules dictionary: caller-supplied or the editor `rules` tag.
var _rules: Dictionary = {}
## True when `_rules` mirrors the editor `rules` tag (no caller-supplied rules).
var _tag_mode := false
## Normalized search (`contentSearch`): trimmed, collapsed, lowercased.
var _search := ""
## Selected category (`selectedCategory`), "" for none.
var _category := ""
## Bannable rows from `bannable_content_json` (`{name, id, category, ui_icon}`).
var _rows: Array = []
## Category names for the filter row (`Category.values()`).
var _categories: Array = []
## Current banned names (`contentSet` order).
var _banned: Array = []
var _search_field: LineEdit = null
var _selected_grid: GridContainer = null
var _deselected_grid: GridContainer = null
var _add_all_selected: Button = null
var _add_all_deselected: Button = null


func _ready() -> void:
	set_title_key("@bannedblocks" if content_type == "block" else "@bannedunits")
	should_pause = false
	super._ready()
	_editor = get_node_or_null(editor_path)
	_kind = content_type
	add_close_button()


func set_context_json(json_text: String) -> void:
	super.set_context_json(json_text)
	var ctx := context()
	_kind = str(ctx.get("kind", content_type))
	_search = ""
	_category = ""
	var rules: Variant = ctx.get("rules", null)
	if rules is Dictionary:
		_rules = (rules as Dictionary).duplicate(true)
		_tag_mode = false
	else:
		_rules = _tag_rules()
		_tag_mode = true
	set_title_key("@bannedblocks" if _kind == "block" else "@bannedunits")


## `shown(this::build)`: load the bannable content + the live set, then build.
func shown() -> void:
	_load_content()
	_banned = _read_banned()
	_build()


func _build() -> void:
	if cont == null:
		return
	for child in cont.get_children():
		cont.remove_child(child)
		child.queue_free()
	var root := content_table()

	# `t.table(s -> s.label("@search") + field + cancel button)`.
	# The search table is content-sized and centred (`cont.table(t -> ...)` with
	# no grow, `BannedContentDialog.java:55-86`; L01 T3 java/ed-banned.png).
	var top := VBoxContainer.new()
	root.add(top).pad(4)
	root.row()
	# code-instantiated: the search row mirrors Java's label + field + clear.
	var search_row := HBoxContainer.new()
	search_row.add_theme_constant_override("separation", 10)
	# `t.table(s -> ...)`: the search table is centred in `t` and its cells
	# centre vertically (Arc cell default `Align.center`).
	search_row.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	search_row.alignment = BoxContainer.ALIGNMENT_CENTER
	var search_label := MindWidgets.label(_t("@search"))
	search_label.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	search_row.add_child(search_label)
	_search_field = MindWidgets.field("")
	_search_field.text = _search
	# `s.field(...)` has no growX: Arc `TextField` prefWidth is 150.
	_search_field.custom_minimum_size.x = 150.0
	_search_field.text_changed.connect(_on_search)
	search_row.add_child(_search_field)
	var clear := MindWidgets.icon_button("cancel", "", "emptyi")
	clear.custom_minimum_size = Vector2(35.0, 35.0)
	clear.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	clear.pressed.connect(func() -> void:
		_search = ""
		if _search_field != null:
			_search_field.text = ""
		_rebuild())
	search_row.add_child(clear)
	top.add_child(search_row)

	# `if(type == ContentType.block)` category toggles (`Category.values()`).
	if _kind == "block" and not _categories.is_empty():
		# code-instantiated: one toggle per block category (data-driven row).
		var category_row := HBoxContainer.new()
		category_row.alignment = BoxContainer.ALIGNMENT_CENTER
		category_row.add_theme_constant_override("separation", 4)
		for category in _categories:
			var category_name := str(category)
			var button := MindWidgets.glyph_button(category_name, category_name, "squareTogglei")
			button.custom_minimum_size = Vector2(45.0, 45.0)
			button.toggle_mode = true
			button.button_pressed = _category == category_name
			button.pressed.connect(func() -> void:
				_category = "" if _category == category_name else category_name
				_rebuild())
			category_row.add_child(button)
		top.add_child(category_row)

	# `cont.table(...)` / `cont.table(...)`: the banned + unbanned panes.
	# `.fill().expandY().uniform()` (`BannedContentDialog.java:105,128`): the
	# panes take the height but only their content width (`50 * cols` grids).
	# Portrait stacks the panes (`if(Core.graphics.isPortrait()) cont.row()`,
	# `cell2.padTop(10f)`), landscape puts them side by side (`padLeft(10f)`).
	var panes: BoxContainer
	if _portrait():
		panes = VBoxContainer.new()
	else:
		panes = HBoxContainer.new()
	panes.size_flags_vertical = Control.SIZE_EXPAND_FILL
	panes.add_theme_constant_override("separation", 10)
	root.add(panes).grow_y_axis().pad(4)
	panes.add_child(_pane(true))
	panes.add_child(_pane(false))

	_rebuild()


## One pane (`table.image().color(...)` + `pane(...)` + `@addall`).
func _pane(selected: bool) -> Control:
	var box := VBoxContainer.new()
	# `.fill().expandY().uniform()` (`BannedContentDialog.java:105,128`): both
	# panes take the same width (the wider pref: `50 * cols` + the scroll bar).
	box.size_flags_horizontal = Control.SIZE_FILL
	box.size_flags_vertical = Control.SIZE_EXPAND_FILL
	var color := COLOR_BANNED if selected else MindStyles.ACCENT
	var prefix := "banned" if selected else "unbanned"
	var title := MindWidgets.label(_t("@%s%s" % [prefix, "blocks" if _kind == "block" else "units"]))
	title.add_theme_color_override("font_color", color)
	# `table.add(...).padBottom(-1).top()`: the title centres over the rule.
	title.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	box.add_child(title)
	# code-instantiated: the 3px rule under the pane header (Java `image()` row).
	var line := ColorRect.new()
	line.color = color
	line.custom_minimum_size = Vector2(0.0, 3.0)
	line.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	box.add_child(line)
	var scroll := ScrollContainer.new()
	scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	scroll.custom_minimum_size = Vector2(0.0, 110.0)
	box.add_child(scroll)
	# code-instantiated: the icon grid is data-driven (content rows).
	var grid := GridContainer.new()
	grid.columns = _columns()
	# `table.pane(table2 -> ...)`: Arc sizes the pane's table to the scroll area
	# when its content is smaller, and a table centres its cells, so `<empty>`
	# (or a short list) sits in the middle of the pane.
	# code-instantiated: the centring wrapper of the data-driven grid.
	var center := CenterContainer.new()
	center.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	center.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.add_child(center)
	center.add_child(grid)
	var add_all := MindWidgets.icon_button("add", _t("@addall"))
	add_all.custom_minimum_size.y = 48.0
	add_all.pressed.connect(func() -> void: _add_all(selected))
	box.add_child(add_all)
	if selected:
		_selected_grid = grid
		_add_all_selected = add_all
	else:
		_deselected_grid = grid
		_add_all_deselected = add_all
	return box


## `rebuildTable` column count (`Core.graphics.getWidth() / Scl.scl()`):
## `max(4, (screen - 300) / 50 / 2)` cells per pane.
func _columns() -> int:
	var width := get_viewport_rect().size.x
	if _portrait():
		# `max(4, (width - 100) / 50)` (`BannedContentDialog.java:155-156`).
		return maxi(4, int((width - 100.0) / CELL))
	return maxi(4, int((width - 300.0) / CELL / 2.0))


## Arc `Core.graphics.isPortrait()`: taller than wide.
func _portrait() -> bool:
	var view_size := get_viewport_rect().size
	return view_size.y > view_size.x


## `rebuildTables()`: refilter and repaint both panes.
func _rebuild() -> void:
	if _selected_grid == null:
		return
	for grid in [_selected_grid, _deselected_grid]:
		for child in grid.get_children():
			grid.remove_child(child)
			child.queue_free()
	var filtered := _filtered_rows()
	var banned_rows: Array = []
	var available_rows: Array = []
	for row in filtered:
		if _banned.has(str(row.get("name", ""))):
			banned_rows.append(row)
		else:
			available_rows.append(row)
	_fill_grid(_selected_grid, banned_rows)
	_fill_grid(_deselected_grid, available_rows)
	# `contentSet.toSeq().containsAll(filteredContent)` — enabled exactly when a
	# filtered row is still unbanned.
	_add_all_selected.disabled = available_rows.is_empty()
	# `available.containsAll(filteredContent)` — enabled exactly when a filtered
	# row is banned.
	_add_all_deselected.disabled = banned_rows.is_empty()
	_uniform_panes.call_deferred()


## Arc `uniform()` on both pane cells: each takes the larger pref width (the
## scrolling pane adds its bar), so the banned pane is never narrower.
func _uniform_panes() -> void:
	if _selected_grid == null or not is_instance_valid(_selected_grid):
		return
	var boxes: Array[Control] = []
	for grid in [_selected_grid, _deselected_grid]:
		var box := (grid as Control).get_parent().get_parent().get_parent() as Control
		if box == null:
			return
		box.custom_minimum_size.x = 0.0
		boxes.append(box)
	var width := 0.0
	for box in boxes:
		width = maxf(width, box.get_combined_minimum_size().x)
	for box in boxes:
		box.custom_minimum_size.x = width


## One pane's buttons (`rebuildTable(table, isSelected)`; a click always moves
## the cell to the other pane).
func _fill_grid(grid: GridContainer, rows: Array) -> void:
	if rows.is_empty():
		var empty := MindWidgets.label(_t("@empty"))
		empty.custom_minimum_size.x = CELL * grid.columns
		empty.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		grid.add_child(empty)
		return
	for row_value in rows:
		var row: Dictionary = row_value
		var content_name := str(row.get("name", ""))
		var icon := str(row.get("ui_icon", ""))
		# code-instantiated: one 50px content cell per bannable row.
		var button := MindWidgets.image_button_first(
			PackedStringArray([icon, content_name]), "clearNonei"
		)
		button.custom_minimum_size = Vector2(CELL, CELL)
		button.expand_icon = true
		button.add_theme_constant_override("icon_max_width", int(ICON))
		if button.icon == null:
			button.text = content_name
		button.tooltip_text = _localized(content_name)
		button.pressed.connect(_toggle.bind(content_name))
		grid.add_child(button)


## `content.getBy(type).select(pred)` + search/category (`contentSearch`,
## `selectedCategory`), sorted by content id (`Seq.sort()` -> `Content.compareTo`).
func _filtered_rows() -> Array:
	var out: Array = []
	for row_value in _rows:
		var row: Dictionary = row_value
		if not _category.is_empty() and str(row.get("category", "")) != _category:
			continue
		if not _search.is_empty():
			var content_name := str(row.get("name", ""))
			# `content.localizedName.toLowerCase().contains(contentSearch)` only
			# (`BannedContentDialog.java:89,141`), never the internal name.
			if not _localized(content_name).to_lower().contains(_search):
				continue
		out.append(row)
	out.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		return int(a.get("id", 0)) < int(b.get("id", 0)))
	return out


## Clicking a cell moves the content between panes (`contentSet.add/remove`).
func _toggle(content_name: String) -> void:
	if _banned.has(content_name):
		_banned.erase(content_name)
	else:
		_banned.append(content_name)
	_commit()
	_rebuild()


## `@addall` on the banned pane adds, on the unbanned pane removes
## (`contentSet.addAll(filteredContent)` / `removeAll`).
func _add_all(banned_pane: bool) -> void:
	var filtered := _filtered_rows()
	for row_value in filtered:
		var content_name := str((row_value as Dictionary).get("name", ""))
		if banned_pane:
			if not _banned.has(content_name):
				_banned.append(content_name)
		else:
			_banned.erase(content_name)
	_commit()
	_rebuild()


## Writes the set back: caller rules via `banned_edited`, else the editor tag.
func _commit() -> void:
	_rules[_banned_key()] = _banned.duplicate()
	if _tag_mode:
		if _editor != null and _editor.has_method("set_banned_names_json"):
			_editor.call("set_banned_names_json", _kind, JSON.stringify(_banned))
	else:
		banned_edited.emit(JSON.stringify(_rules))


func _banned_key() -> String:
	return "bannedBlocks" if _kind == "block" else "bannedUnits"


## `ObjectSet<T> contentSet` contents (`Rules.bannedBlocks`/`bannedUnits`).
func _read_banned() -> Array:
	var out: Array = []
	var raw: Variant = _rules.get(_banned_key(), [])
	if raw is Array:
		for value in raw:
			if value is String and not (value as String).is_empty() and not out.has(value):
				out.append(value)
	return out


## `editor.tags["rules"]` parsed (`MapEditorDialog.save` writes it back).
func _tag_rules() -> Dictionary:
	if _editor == null or not _editor.has_method("tags"):
		return {}
	var tags: Variant = _editor.call("tags")
	if tags is Dictionary:
		var parsed: Variant = JSON.parse_string(str((tags as Dictionary).get("rules", "{}")))
		if parsed is Dictionary:
			return parsed
	return {}


## `content.<T>getBy(type)` rows + the category names (`bannable_content_json`).
func _load_content() -> void:
	_rows = []
	_categories = []
	if _editor == null or not _editor.has_method("bannable_content_json"):
		return
	var parsed: Variant = JSON.parse_string(str(_editor.call("bannable_content_json", _kind)))
	if parsed is Dictionary:
		var rows: Variant = (parsed as Dictionary).get("rows", [])
		if rows is Array:
			_rows = rows
		var categories: Variant = (parsed as Dictionary).get("categories", [])
		if categories is Array:
			_categories = categories


## `value.trim().replaceAll(" +", " ").toLowerCase()` (`contentSearch`).
func _on_search(text: String) -> void:
	_search = " ".join(text.strip_edges().split(" ", false)).to_lower()
	_rebuild()


## `content.localizedName` (`Core.bundle`), falling back to the content name.
func _localized(content_name: String) -> String:
	var prefix := "block" if _kind == "block" else "unit"
	return _t("@%s.%s.name" % [prefix, content_name])
