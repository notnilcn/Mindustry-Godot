## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/AboutDialog.java (`showCredits`).
##
## Credits body: the `credits.text` blurb, an accent rule, the `contributors`
## heading and the 3-column contributor grid (read from the packed
## `contributors` file through `MindAssets`).

extends MindDialog

const COLUMNS := 3


func _ready() -> void:
	set_title_key("@credits")
	should_pause = false
	super._ready()
	_build()
	add_close_button()


func _build() -> void:
	var table := content_table()
	var blurb := MindWidgets.label(MindWidgets.markup("@credits.text"))
	blurb.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	blurb.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	blurb.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	table.add(blurb).grow_x_axis().pad(12).set_colspan(COLUMNS)
	table.row()
	# code-instantiated: the accent rule between the blurb and the contributors.
	var rule := ColorRect.new()
	rule.color = MindStyles.ACCENT
	rule.custom_minimum_size = Vector2(0.0, 3.0)
	rule.mouse_filter = Control.MOUSE_FILTER_IGNORE
	table.add(rule).grow_x_axis().pad(3).set_colspan(COLUMNS)
	table.row()
	var header := MindWidgets.label(_t("@contributors"))
	header.add_theme_color_override("default_color", MindStyles.ACCENT)
	table.add(header).grow_x_axis().pad(4).set_colspan(COLUMNS)
	table.row()
	var column := 0
	for contributor in _contributors():
		# code-instantiated: contributor rows are data-driven from the assets file.
		var label := MindWidgets.label(str(contributor))
		label.add_theme_color_override("default_color", Color(0.8, 0.8, 0.8))
		table.add(label).set_align(3).pad(3).set_pad_left(6).set_pad_right(6)
		column += 1
		if column % COLUMNS == 0:
			table.row()
	if column % COLUMNS != 0:
		table.row()


## Contributor names from the packed `contributors` file.
func _contributors() -> PackedStringArray:
	var assets := MindWidgets.assets()
	if assets != null and assets.has_method("contributors"):
		var result: Variant = assets.call("contributors")
		if result is PackedStringArray:
			return result as PackedStringArray
	return PackedStringArray()
