## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/ContentInfoDialog.java
##         (plan 14 §3.4/§3.7, M3; H05 T10a stats).
##
## Content stats/description panel. Renders the live
## `MindCampaign.content_info_json(content)` payload: the 48 px icon + accent
## name header, `@category.purpose` + description, one accent label per
## `StatCat` and one inset row per stat (`[lightgray]label:[] ` + the value
## cells), then details and the credit line. Falls back to the `MindAssets`
## bundle text when the facade returns `{}`.

extends MindDialog

var _table: MindTable = null


func _ready() -> void:
	set_title_key("@info.title")
	should_pause = false
	super._ready()
	_table = content_table()
	add_close_button()


func shown() -> void:
	if _table == null:
		return
	_table.clear_children()
	var content_name := str(_context.get("content", _context.get("name", "")))
	if content_name.is_empty():
		_table.add(MindWidgets.label(_t("@none"))).pad(8)
		return
	var info := campaign_json("content_info_json", [content_name])
	if info.is_empty():
		info = _bundle_info(content_name)

	_show_header(info, content_name)

	if bool(info.get("patched", false)):
		_table.add(MindWidgets.label(MindWidgets.markup("@database.patched"))).pad(4).left()
		_table.row()

	var categories: Array = info.get("categories", [])
	var stats: Array = info.get("stats", [])
	var has_stats := not categories.is_empty() or not stats.is_empty()
	var description := str(info.get("description", ""))
	if not description.is_empty():
		if has_stats:
			# `table.add("@category.purpose").color(Pal.accent).fillX().padTop(10)`
			_table.add(
				MindWidgets.styled_label(_tm("@category.purpose"), "techLabel")
			).grow_x_axis().set_pad_top(10).left()
			_table.row()
		# code-instantiated: description text comes from the live content.
		var body := MindWidgets.label(MindWidgets.markup_text("[lightgray]" + description))
		body.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		body.custom_minimum_size.x = 500.0
		_table.add(body).grow_x_axis().set_pad_left(10 if has_stats else 0).left()
		_table.row()
		if not bool(info.get("use_categories", false)) and has_stats:
			_table.add(
				MindWidgets.styled_label(_tm("@category.general"), "techLabel")
			).grow_x_axis().left()
			_table.row()

	if not categories.is_empty():
		_render_categories(categories, bool(info.get("use_categories", false)))
	else:
		_render_stats(stats)

	var details := str(info.get("details", ""))
	var hide_details := bool(info.get("hide_details", false))
	var locked := bool(info.get("locked", false))
	if not details.is_empty():
		var text := MindWidgets.markup_text("[gray]" + details)
		if locked and hide_details:
			# Java `Iconc.lock + " " + Core.bundle.get("unlock.incampaign")`.
			text = "[icon name=\"lock\"] %s" % _tm("@unlock.incampaign")
		# code-instantiated: details text comes from the live content.
		var detail_label := MindWidgets.label(text)
		detail_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		detail_label.custom_minimum_size.x = 400.0
		_table.add(detail_label).grow_x_axis().pad(6).set_pad_top(20).left()
		_table.row()
	var credit := str(info.get("credit", ""))
	if not credit.is_empty():
		_table.add(
			MindWidgets.label(MindWidgets.markup_format("@content.createdby", [credit]))
		).set_pad_top(40).left()
		_table.row()


## Header row: `title1.image(content.uiIcon).size(iconXLarge)` + `[accent]` name.
func _show_header(info: Dictionary, content_name: String) -> void:
	var header := HBoxContainer.new()
	header.add_theme_constant_override("separation", 5)
	# code-instantiated: the icon/name header is parameterized by the content.
	var icon := MindWidgets.image(str(info.get("icon", content_name)))
	icon.custom_minimum_size = Vector2(48.0, 48.0)
	icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	header.add_child(icon)
	var localized := str(info.get("localized", content_name))
	header.add_child(
		MindWidgets.styled_label(MindWidgets.markup_text("[accent]" + localized), "techLabel")
	)
	_table.add(header).grow_x_axis().pad(4).left()
	_table.row()


## One accent category label, then the inset stat rows (`useCategories`).
func _render_categories(categories: Array, use_categories: bool) -> void:
	for cat_variant in categories:
		if not (cat_variant is Dictionary):
			continue
		var cat: Dictionary = cat_variant
		var stats: Array = cat.get("stats", [])
		if stats.is_empty():
			continue
		if use_categories:
			_table.add(
				MindWidgets.styled_label(
					MindWidgets.markup_text("[accent]" + str(cat.get("label", ""))), "techLabel"
				)
			).grow_x_axis().left()
			_table.row()
		for stat_variant in stats:
			_render_stat(stat_variant)


## Legacy flat `[{category,label,value}]` rows (bundle fallback paths).
func _render_stats(stats: Array) -> void:
	var current_category := ""
	for stat_variant in stats:
		if not (stat_variant is Dictionary):
			continue
		var stat: Dictionary = stat_variant
		var category := str(stat.get("category", ""))
		if not category.is_empty() and category != current_category:
			current_category = category
			_table.add(
				MindWidgets.styled_label(
					MindWidgets.markup_text("[accent]" + _stat_text(category)), "techLabel"
				)
			).grow_x_axis().left()
			_table.row()
		# code-instantiated: one label/value row per computed stat.
		var row := HBoxContainer.new()
		row.add_theme_constant_override("separation", 8)
		row.add_child(
			MindWidgets.label(
				MindWidgets.markup_text(
					"[lightgray]%s:[] " % _stat_text(str(stat.get("label", "")))
				)
			)
		)
		row.add_child(MindWidgets.label(str(stat.get("value", ""))))
		_table.add(row).grow_x_axis().pad(1).left()
		_table.row()


## `table.table(inset -> { inset.left(); stats.statInfo(inset.add(label), stat); for value: value.display(inset); inset.add().size(10f); })`
func _render_stat(stat_variant: Variant) -> void:
	if not (stat_variant is Dictionary):
		return
	var stat: Dictionary = stat_variant
	# code-instantiated: one stat row per computed stat (data-driven).
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 2)
	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", 6)
	column.add_child(line)
	var label := MindWidgets.label(
		MindWidgets.markup_text("[lightgray]%s:[] " % str(stat.get("label", "")))
	)
	label.vertical_alignment = VERTICAL_ALIGNMENT_TOP
	if stat.get("tooltip") != null:
		MindTooltip.attach(label, _t("@%s" % str(stat.get("tooltip", ""))))
	line.add_child(label)
	var values: Array = stat.get("values", [])
	for value_index in values.size():
		if value_index > 0:
			line.add_child(_spacer(10.0))
		for cell_variant in (values[value_index] as Array):
			line = _add_cell(column, line, cell_variant)
	_table.add(column).grow_x_axis().pad(1).set_pad_left(10).left()
	_table.row()


## Adds one `Cell` (see `mind_core::ui::content_stats::Cell`) and returns the
## line container the next cell belongs to (`row` cells start a new line).
func _add_cell(column: Container, line: HBoxContainer, cell_variant: Variant) -> HBoxContainer:
	if not (cell_variant is Dictionary):
		return line
	var cell: Dictionary = cell_variant
	match str(cell.get("k", "")):
		"text":
			line.add_child(
				MindWidgets.label(MindWidgets.markup_text(str(cell.get("t", ""))))
			)
		"space":
			line.add_child(_spacer(float(cell.get("w", 0.0))))
		"row":
			# code-instantiated: multi-line stat values split into extra lines.
			var next := HBoxContainer.new()
			next.add_theme_constant_override("separation", 6)
			column.add_child(next)
			return next
		"icon":
			line.add_child(_icon_cell(cell))
		"panel":
			line.add_child(_panel_cell(cell))
		_:
			pass
	return line


## `StatValues.stack`: 32 px content icon with an outline amount and a click
## through to the item/liquid info dialog. Unit icons (`power`, `liquid`,
## `waves`) have no atlas region and draw the icon-font glyph instead.
func _icon_cell(cell: Dictionary) -> Control:
	var size := float(cell.get("size", 32))
	var region := str(cell.get("region", ""))
	if MindWidgets.icon_texture(region) == null:
		# code-instantiated: Iconc unit glyphs (`StatUnit.icon`) are font glyphs,
		# not atlas regions.
		var holder := Control.new()
		holder.custom_minimum_size = Vector2(size, size)
		holder.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		var glyph := MindWidgets.glyph(region, int(size), Color.WHITE)
		glyph.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		glyph.mouse_filter = Control.MOUSE_FILTER_IGNORE
		holder.add_child(glyph)
		return holder
	# code-instantiated: content icons are data-driven stat cells.
	var button := MindWidgets.image_button(region, "cleari")
	button.custom_minimum_size = Vector2(size, size)
	button.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	var content := str(cell.get("content", ""))
	if not content.is_empty():
		button.pressed.connect(open_content_info.bind(content))
	# `Cell::Icon.amount` serializes `None` as JSON null; `str(null)` is the
	# literal "<null>", so read the value and skip null explicitly.
	var amount_value: Variant = cell.get("amount")
	var amount := "" if amount_value == null else str(amount_value)
	if not amount.is_empty():
		var label := MindWidgets.styled_label(amount, "outlineLabel")
		label.set_anchors_preset(Control.PRESET_BOTTOM_LEFT)
		label.offset_top = -20.0
		label.mouse_filter = Control.MOUSE_FILTER_IGNORE
		button.add_child(label)
	return button


## `Cell::Panel`: nested cells inside a flat gray box (reconstructor upgrades).
func _panel_cell(cell: Dictionary) -> Control:
	# code-instantiated: panel cells are data-driven (upgrade pairs).
	var panel := PanelContainer.new()
	panel.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	var box := StyleBoxFlat.new()
	box.bg_color = Color(0.1, 0.1, 0.1, 0.9)
	box.content_margin_left = 4.0
	box.content_margin_right = 4.0
	box.content_margin_top = 2.0
	box.content_margin_bottom = 2.0
	panel.add_theme_stylebox_override("panel", box)
	# Nested cells may start their own rows (`Cell::Row`): the panel is a
	# VBox of HBox lines so `_add_cell`'s row containers land below, not beside
	# (ammo/upgrade panels broke when `inner` was a single HBox).
	var inner := VBoxContainer.new()
	inner.add_theme_constant_override("separation", 2)
	panel.add_child(inner)
	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", 4)
	inner.add_child(line)
	for nested_variant in (cell.get("cells", []) as Array):
		line = _add_cell(inner, line, nested_variant)
	return panel


func _spacer(width: float) -> Control:
	# code-instantiated: cell gaps are data-driven (Java `inset.add().size(10f)`).
	var spacer := Control.new()
	spacer.custom_minimum_size = Vector2(width, 0.0)
	spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return spacer


## Resolves a stat/category label: bundle keys (`category.purpose`) resolve
## through `MindAssets`, plain strings pass through.
func _stat_text(value: String) -> String:
	if value.is_empty():
		return value
	return _t("@%s" % value)


## Bundle-only fallback: `<type>.<name>.name|description|details` for the first
## unlockable type that has the name.
func _bundle_info(content_name: String) -> Dictionary:
	var assets := MindWidgets.assets()
	if assets == null:
		return {"content": content_name, "localized": content_name}
	for type_name in ["block", "item", "unit", "liquid", "status", "weather", "sector", "planet"]:
		var name_key := "%s.%s.name" % [type_name, content_name]
		var localized := str(assets.call("bundle_get", name_key))
		if localized.is_empty() or localized == name_key:
			continue
		return {
			"content": content_name,
			"localized": localized,
			"type": type_name,
			"description": _bundle_value(assets, "%s.%s.description" % [type_name, content_name]),
			"details": _bundle_value(assets, "%s.%s.details" % [type_name, content_name]),
			"credit": _bundle_value(assets, "%s.%s.credit" % [type_name, content_name]),
		}
	return {"content": content_name, "localized": content_name}


func _bundle_value(assets: Node, key: String) -> String:
	var value := str(assets.call("bundle_get", key))
	return "" if value == key else value
