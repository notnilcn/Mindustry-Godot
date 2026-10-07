## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/ContentInfoDialog.java
##         (plan 14 §3.4/§3.7, M3).
##
## Content stats/description panel. Renders the live
## `MindCampaign.content_info_json(content)` payload (WS2 contract) with its
## `stats` rows, purpose/category sections, details and the data-patch
## indicator; falls back to the `MindAssets` bundle text when the facade is
## absent. Opened from the research rows and the database (`content` context).

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

	var header := HBoxContainer.new()
	header.add_theme_constant_override("separation", 8)
	# code-instantiated: the icon/name header is parameterized by the content.
	var icon := MindWidgets.image(str(info.get("icon", content_name)))
	icon.custom_minimum_size = Vector2(48.0, 48.0)
	icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	header.add_child(icon)
	header.add_child(
		MindWidgets.styled_label("[accent]%s" % str(info.get("localized", content_name)), "techLabel")
	)
	_table.add(header).grow_x_axis().pad(4)
	_table.row()

	var kind := str(info.get("type", ""))
	if not kind.is_empty():
		_table.add(MindWidgets.label(kind)).pad(2)
		_table.row()

	if bool(info.get("patched", false)):
		_table.add(MindWidgets.label(_t("@database.patched"))).pad(4).left()
		_table.row()

	var description := str(info.get("description", ""))
	var stats: Array = info.get("stats", [])
	if not description.is_empty():
		if not stats.is_empty():
			_table.add(MindWidgets.styled_label(_t("@category.purpose"), "techLabel")).grow_x_axis().pad(4)
			_table.row()
		var body := MindWidgets.label(description)
		body.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		_table.add(body).grow_x_axis().pad(6)
		_table.row()
	if not stats.is_empty() and not description.is_empty():
		_table.add(MindWidgets.styled_label(_t("@category.general"), "techLabel")).grow_x_axis().pad(4)
		_table.row()
	_render_stats(stats)

	var details := str(info.get("details", ""))
	if not details.is_empty():
		var detail_label := MindWidgets.label(details)
		detail_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		_table.add(detail_label).grow_x_axis().pad(8)
		_table.row()
	var credit := str(info.get("credit", ""))
	if not credit.is_empty():
		_table.add(MindWidgets.label(MindWidgets.markup_format("@content.createdby", [credit]))).pad(4)
		_table.row()


## `StatCat`-grouped stat rows: `[{"category", "label", "value"}]`.
func _render_stats(stats: Array) -> void:
	var current_category := ""
	for stat_variant in stats:
		if not (stat_variant is Dictionary):
			continue
		var stat: Dictionary = stat_variant
		var category := str(stat.get("category", ""))
		if not category.is_empty() and category != current_category:
			current_category = category
			_table.add(MindWidgets.styled_label(_stat_text(category), "techLabel")).grow_x_axis().pad(4)
			_table.row()
		# code-instantiated: one label/value row per computed stat.
		var row := HBoxContainer.new()
		row.add_theme_constant_override("separation", 8)
		row.add_child(MindWidgets.label("[lightgray]%s:" % _stat_text(str(stat.get("label", "")))))
		row.add_child(MindWidgets.label(str(stat.get("value", ""))))
		_table.add(row).grow_x_axis().pad(1).left()
		_table.row()


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
