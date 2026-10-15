## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/SchematicsDialog.java
##         (`SchematicInfoDialog`, :774-834).
##
## Schematic info: the `schematic.info` size line, the preview image, the
## summed requirements (four per row, scarlet when the player core cannot
## afford them) and the schematics' power production/consumption.

extends MindDialog

var _body: MindTable = null


func _ready() -> void:
	set_title_key("@schematic")
	should_pause = false
	full_dialog = true
	super._ready()
	_body = content_table()


func shown() -> void:
	refresh_campaign_views()
	_build()


## `SchematicInfoDialog.show`: the `[[Schematic] name]` title plus the body.
func _build() -> void:
	if _body == null:
		return
	_body.clear_children()
	var schematics := campaign_section("schematics")
	var index := int(_context.get("index", -1))
	var schematic := _find_schematic(schematics, index)
	if schematic.is_empty():
		return
	if title_label != null:
		title_label.text = "[[%s] %s" % [_t("@schematic"), str(schematic.get("name", ""))]
		title_label.visible = true
	var info := MindTable.new()
	info.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	# `Core.bundle.format("schematic.info", w, h, tiles.size)`.
	info.add(
		MindWidgets.label(
			MindWidgets.markup_format(
				"@schematic.info",
				[
					int(schematic.get("width", 0)),
					int(schematic.get("height", 0)),
					(schematic.get("tiles", []) as Array).size(),
				]
			)
		)
	).grow_x_axis()
	info.row()
	# code-instantiated: the preview is bound to the selected schematic row.
	var holder := Control.new()
	holder.custom_minimum_size = Vector2(0, 320)
	holder.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var image := MindSchematicImage.new()
	image.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	image.setup(schematic)
	holder.add_child(image)
	info.add(holder).grow_x_axis().pad(6)
	info.row()
	_build_requirements(info, schematic)
	var production := float(schematic.get("power_production", 0.0)) * 60.0
	var consumption := float(schematic.get("power_consumption", 0.0)) * 60.0
	if production != 0.0 or consumption != 0.0:
		var row := HBoxContainer.new()
		row.add_theme_constant_override("separation", 3)
		if production != 0.0:
			row.add_child(MindWidgets.glyph("power", 20, Color("ffd37f")))
			var plus := MindWidgets.label("+%s" % _auto_fixed(production, 2))
			plus.add_theme_color_override("default_color", Color("ffd37f"))
			row.add_child(plus)
			if consumption != 0.0:
				var gap := Control.new()
				gap.custom_minimum_size = Vector2(15, 0)
				row.add_child(gap)
		if consumption != 0.0:
			row.add_child(MindWidgets.glyph("power", 20, Color("e55454")))
			var minus := MindWidgets.label("-%s" % _auto_fixed(consumption, 2))
			minus.add_theme_color_override("default_color", Color("e55454"))
			row.add_child(minus)
		info.add(row).pad(4)
		info.row()
	var description := str(schematic.get("description", ""))
	if not description.is_empty():
		var wrapped := MindWidgets.label("[lightgray]%s" % description)
		wrapped.custom_minimum_size = Vector2(500, 0)
		wrapped.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		info.add(wrapped).pad(8)
		info.row()
	_body.add(info).grow_x_axis()
	_body.call_deferred("sort_now")


## Requirement rows: 32 px icon + amount, four per row; when a player core
## exists and cannot afford the stack the label is `have/amount` in scarlet
## (`SchematicInfoDialog.show`'s label provider).
func _build_requirements(info: MindTable, schematic: Dictionary) -> void:
	var requirements: Array = schematic.get("requirements", [])
	if requirements.is_empty():
		return
	var have := _core_items()
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 4)
	var count := 0
	for entry_variant in requirements:
		var entry: Array = entry_variant
		var item := str(entry[0])
		var amount := int(entry[1])
		# code-instantiated: one requirement cell per schematic stack.
		var icon := TextureRect.new()
		icon.texture = MindWidgets.icon_texture("item-%s-ui" % item)
		icon.custom_minimum_size = Vector2(32, 32)
		icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		row.add_child(icon)
		var label := MindWidgets.label(str(amount))
		var available := int(have.get(item, 0))
		if not have.is_empty() and available < amount:
			label.text = "[scarlet]%d[]/%d" % [available, amount]
		row.add_child(label)
		count += 1
		if count % 4 == 0:
			info.add(row).pad(2)
			info.row()
			row = HBoxContainer.new()
			row.add_theme_constant_override("separation", 4)
	if count % 4 != 0:
		info.add(row).pad(2)
		info.row()


## The player core's item counts (`player.core().items`); empty in the menu.
func _core_items() -> Dictionary:
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("core_items_json"):
		return {}
	var parsed: Variant = JSON.parse_string(str(host.call("core_items_json")))
	return parsed if parsed is Dictionary else {}


## Arc `Strings.autoFixed(value, max)`: truncate to `max` decimals, drop
## trailing zeros (`Strings.java:1230`).
func _auto_fixed(value: float, max_decimals: int) -> String:
	var text := "%.*f" % [max_decimals, value]
	if text.contains("."):
		text = text.rstrip("0").rstrip(".")
	return text if not text.is_empty() else "0"


## The view row for `index` (falls back to the first row).
func _find_schematic(schematics: Array, index: int) -> Dictionary:
	for row_variant in schematics:
		var row: Dictionary = row_variant
		if int(row.get("index", -1)) == index:
			return row
	return schematics[0] as Dictionary if not schematics.is_empty() else {}
