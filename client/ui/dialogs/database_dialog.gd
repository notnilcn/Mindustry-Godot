## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/DatabaseDialog.java (plan 14 §3.4, M3).
##
## Content database: search field + content-type tabs. Rows come from the live
## content snapshot (`MindSimHost.content_list`); selecting one opens
## `ContentInfoDialog` with the content key. Unlock filtering is plan 02's.

extends MindDialog

## `ContentType` tab order used by `DatabaseDialog` (`items`, `blocks`, `units`).
const TABS := ["@items", "@blocks", "@units", "@liquids"]
## `ContentType.name()` per tab (`ContentRegistry.entries`).
const CONTENT_TYPES := ["item", "block", "unit", "liquid"]

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
	var names := _filtered_names(query)
	if names.is_empty():
		var placeholder := MindWidgets.label(_t("@none"))
		placeholder.modulate = MindStyles.UNLAUNCHED
		_tab_list.add(placeholder).pad(8)
		return
	for content_name in names:
		# code-instantiated: content rows come from the live content snapshot.
		var button := MindWidgets.button(content_name)
		button.pressed.connect(_open_content.bind(content_name))
		_tab_list.add(button).grow_x_axis().pad(2)
		_tab_list.row()


## Content names of the active tab, filtered by the search query.
func _filtered_names(query: String) -> Array:
	var names: Array = []
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("content_list"):
		return names
	var listed: Variant = host.call("content_list", CONTENT_TYPES[_tab_index])
	if not (listed is PackedStringArray):
		return names
	for value in listed:
		var name := str(value)
		if query.is_empty() or name.to_lower().contains(query):
			names.append(name)
	return names


## `ContentInfoDialog`: `ui.content.show(content)`.
func _open_content(content_name: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("open_dialog"):
		ui.call("open_dialog", "content", JSON.stringify({"content": content_name}))
