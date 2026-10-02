## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/DatabaseDialog.java (plan 14 §3.4, M3).
##
## Content database: search field + content-type tabs. The content catalogue and
## unlock filters come from plan 02; the M3 shell provides the search/tab frame
## and opens `ContentInfoDialog` for a selected entry.

extends MindDialog

## `ContentType` tab order used by `DatabaseDialog` (`items`, `blocks`, `units`).
const TABS := ["@items", "@blocks", "@units", "@liquids"]

var _tab_list: MindTable = null
var _tab_index := 0
var _search: LineEdit = null


func _ready() -> void:
	set_title_key("@database")
	should_pause = true
	super._ready()
	_build()
	add_close_button()


func _build() -> void:
	var table := content_table()
	table.add_theme_constant_override("separation", 6)

	_search = MindWidgets.field(_t("@search"))
	_search.text_changed.connect(func(_value: String) -> void: _rebuild())
	table.add(_search).grow_x_axis().pad(4)
	table.row()

	# code-instantiated: content tabs are the data-driven ContentType list.
	var tabs := HBoxContainer.new()
	tabs.alignment = BoxContainer.ALIGNMENT_CENTER
	for index in TABS.size():
		var button := Button.new()
		button.text = _t(TABS[index])
		button.theme_type_variation = "flatTogglet"
		button.toggle_mode = true
		button.button_pressed = index == 0
		button.pressed.connect(_select_tab.bind(index))
		tabs.add_child(button)
	table.add(tabs).grow_x_axis().pad(4)
	table.row()

	_tab_list = MindTable.new()
	_tab_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	table.add(_tab_list).grow_x_axis().grow_y_axis().pad(4)
	_rebuild()


func _select_tab(index: int) -> void:
	_tab_index = index
	_rebuild()


func _rebuild() -> void:
	if _tab_list == null:
		return
	_tab_list.clear_children()
	var query := ""
	if _search != null:
		query = _search.text.to_lower()
	# code-instantiated: rows come from the plan-02 content registry once exposed;
	# M3 renders the filtered empty-state.
	var placeholder := MindWidgets.label(_t("@none"))
	placeholder.modulate = MindStyles.UNLAUNCHED
	_tab_list.add(placeholder).pad(8)
	if not query.is_empty():
		pass
