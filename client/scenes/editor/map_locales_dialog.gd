## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/MapLocalesDialog.java
##
## The locale-bundle editor: the locale list (select/edit/delete/add), the
## property cards with correct/missing/same colouring, the search row with the
## property filter, `@locales.applytoall`, the property-add card and the
## `@back`/`@editor.apply`/`@edit` buttons. The `locales` tag round-trip is
## `MindEditor.locales_json`/`set_locales_json`; the clipboard text format is
## `writeLocale`/`readLocale`/`writeBundles`/`readBundles` below.

extends MindDialog

## `cardWidth`.
const CARD_WIDTH := 400.0
## `collapseButton` size (`size(35f)`).
const SMALL := 35.0
## Card status tints (`Pal.accent` / `Pal.techBlue` / `Pal.gray`).
const COLOR_MISSING := Color("ffd37f")
const COLOR_SAME := Color("8ca9e8")
const COLOR_CORRECT := Color("666666")
## Native locale display names (`LanguageDialog.getDisplayName`).
const LANGUAGE_SCRIPT := preload("res://ui/dialogs/language_dialog.gd")

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"

enum Status { CORRECT, MISSING, SAME }

var _main_scroll: ScrollContainer = null
var _empty_box: CenterContainer = null
var _editor: Node = null
var _locales: Dictionary = {}
var _last_saved: Dictionary = {}
var _saved := true
var _selected_locale := "en"
var _apply_to_all := true
var _collapsed := false
var _search := ""
var _search_by_value := false
var _show_correct := true
var _show_missing := true
var _show_same := true

var _langs: VBoxContainer = null
var _main: HFlowContainer = null
var _apply_button: Button = null
var _search_field: LineEdit = null
var _prop_name: LineEdit = null
var _prop_value: TextEdit = null
var _pending_locale_delete := ""
var _pending_icon_target := ""
## `viewPropertyDialog(key)` state (`@locales.viewproperty`).
var _view_key := ""
var _view_body: VBoxContainer = null
var _view_search: LineEdit = null
## `addIconDialog` gallery state (`@locales.addicon`).
var _icon_body: VBoxContainer = null
var _icon_search := ""
## Unsaved-changes guard, the deleted-locale confirm and the save-on-close
## prompt all answer through `confirm_result`; `_pending_*` selects the action.
var _apply_from_prompt := false


func _ready() -> void:
	set_title_key("@editor.locales")
	should_pause = false
	super._ready()
	_editor = get_node_or_null(editor_path)
	# `selectedLocale = MapLocales.currentLocale()` (constructor).
	_selected_locale = _default_locale()
	clear_buttons()
	# `buttons.table(t -> { back, apply, edit })` + `?`.
	# `buttons.add("").uniform()` + the centred Back/Apply/Edit table + `?`
	# (60x64, uniform): the three buttons stay centred and `?` sits at the
	# right end (java/ed-locales.png).
	# code-instantiated: the balancing cells of the Java button table.
	var lead := Control.new()
	lead.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	lead.mouse_filter = Control.MOUSE_FILTER_IGNORE
	buttons.add_child(lead)
	add_button(_t("@back"), _back, "left", 210.0)
	_apply_button = add_button(_t("@editor.apply"), _apply, "ok", 210.0)
	add_button(_t("@edit"), _edit_dialog, "edit", 210.0)
	var tail := Control.new()
	tail.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	tail.mouse_filter = Control.MOUSE_FILTER_IGNORE
	buttons.add_child(tail)
	add_button("?", _show_info, "", 60.0)
	# `.size(210f, 64f)` / `.size(60f, 64f)`: full-height buttons.
	for child in buttons.get_children():
		if child is Button:
			(child as Button).custom_minimum_size.y = 64.0
	_connect_prompts()
	# `resized(this::buildMain)` (`MapLocalesDialog.java:93`).
	resized.connect(_build_main)


func shown() -> void:
	_locales = _read_locales()
	_last_saved = _locales.duplicate(true)
	_saved = true
	_setup()


## `setup()`: the locale list + the search/apply column + the add-property card.
func _setup() -> void:
	if cont == null:
		return
	for child in cont.get_children():
		cont.remove_child(child)
		child.queue_free()
	var root := content_table()
	var row := HBoxContainer.new()
	row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.size_flags_vertical = Control.SIZE_EXPAND_FILL
	row.add_theme_constant_override("separation", 10)
	root.add(row).grow_x_axis().grow_y_axis().pad(6)

	# `cont.add(langs).left()`: `langs = new Table(Tex.button)`, content-sized
	# and vertically centred in its cell (`MapLocalesDialog.java:60,112`).
	# code-instantiated: the locale list is data-driven from the locales tag.
	var langs_frame := PanelContainer.new()
	langs_frame.add_theme_stylebox_override("panel", MindThemeBuilder._button_nine_patch("button"))
	langs_frame.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	row.add_child(langs_frame)
	_langs = VBoxContainer.new()
	langs_frame.add_child(_langs)

	var right := VBoxContainer.new()
	right.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	right.size_flags_vertical = Control.SIZE_EXPAND_FILL
	row.add_child(right)

	# Search / collapse-all / filter row (`:124-147`).
	# code-instantiated: the search row is a fixed four-control strip.
	var search_row := HBoxContainer.new()
	search_row.add_theme_constant_override("separation", 10)
	search_row.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	var collapse := MindWidgets.icon_button("down-open" if not _collapsed else "up-open", "", "emptyTogglei")
	collapse.custom_minimum_size = Vector2(SMALL, SMALL)
	collapse.toggle_mode = true
	collapse.button_pressed = _collapsed
	collapse.pressed.connect(func() -> void:
		_collapsed = not _collapsed
		_build_main())
	search_row.add_child(collapse)
	var filter := MindWidgets.icon_button("filter", "", "emptyi")
	filter.custom_minimum_size = Vector2(SMALL, SMALL)
	filter.tooltip_text = _t("@locales.filter")
	filter.pressed.connect(_filter_dialog)
	search_row.add_child(filter)
	_search_field = MindWidgets.field(_t("@locales.searchvalue" if _search_by_value else "@locales.searchname"))
	_search_field.text = _search
	_search_field.max_length = 64
	_search_field.custom_minimum_size.x = 250.0
	_search_field.text_changed.connect(func(text: String) -> void:
		_search = text
		_build_main())
	search_row.add_child(_search_field)
	var clear := MindWidgets.icon_button("cancel", "", "emptyi")
	clear.custom_minimum_size = Vector2(SMALL, SMALL)
	clear.pressed.connect(func() -> void:
		_search = ""
		if _search_field != null:
			_search_field.text = ""
		_build_main())
	search_row.add_child(clear)
	right.add_child(search_row)

	# `t.check("@locales.applytoall", applytoall, b -> applytoall = b)`.
	# `.pad(10f)`: centred under the search row.
	var apply_all := MindWidgets.check(_t("@locales.applytoall"), _apply_to_all,
		func(pressed: bool) -> void: _apply_to_all = pressed)
	apply_all.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	right.add_child(apply_all)
	# `main.image().color(Pal.gray).height(3f).growX().top()` (`buildMain`).
	var rule := ColorRect.new()
	rule.color = Color8(0x45, 0x45, 0x45)
	rule.custom_minimum_size.y = 3.0
	right.add_child(rule)

	# `t.add(main).center().grow()`.
	# code-instantiated: the property cards are data-driven (locale bundles).
	var scroll := ScrollContainer.new()
	scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.custom_minimum_size = Vector2(0.0, 300.0)
	right.add_child(scroll)
	_main = HFlowContainer.new()
	_main.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_main.add_theme_constant_override("h_separation", 8)
	_main.add_theme_constant_override("v_separation", 8)
	scroll.add_child(_main)
	_main_scroll = scroll
	# `main.add("@empty").center()`: mid-column when nothing is listed.
	_empty_box = CenterContainer.new()
	_empty_box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_empty_box.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_empty_box.add_child(MindWidgets.label(_t("@empty")))
	right.add_child(_empty_box)

	# `cont.table(Tex.button, t -> { name field; value area; @add })`.
	# code-instantiated: the property-add card is a fixed three-control block.
	var add_panel := PanelContainer.new()
	# `cont.table(Tex.button, ...).right()`: its own column, vertically centred.
	add_panel.add_theme_stylebox_override("panel", MindThemeBuilder._button_nine_patch("button"))
	add_panel.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	var add_box := VBoxContainer.new()
	add_panel.add_child(add_box)
	_prop_name = MindWidgets.field(_t("@locales.property.name"))
	_prop_name.max_length = 64
	add_box.add_child(_prop_name)
	_prop_value = TextEdit.new()
	_prop_value.placeholder_text = _t("@locales.property.text")
	_prop_value.custom_minimum_size = Vector2(CARD_WIDTH, 140.0)
	add_box.add_child(_prop_value)
	var add_prop := MindWidgets.icon_button("add", _t("@add"))
	add_prop.pressed.connect(_add_property)
	# `.padTop(10f).size(cardWidth, 50f)`.
	add_prop.custom_minimum_size = Vector2(CARD_WIDTH, 50.0)
	add_box.add_theme_constant_override("separation", 10)
	add_box.add_child(add_prop)
	row.add_child(add_panel)

	# `buildTables()`: the selected locale always exists before the tables build.
	if not _locales.has(_selected_locale):
		_locales[_selected_locale] = {}
	_build_langs()
	_build_main()
	_apply_button.disabled = _saved


## `buildLocalesTable()`: present locales get select/edit/trash, then `@add`.
func _build_langs() -> void:
	if _langs == null:
		return
	for child in _langs.get_children():
		_langs.remove_child(child)
		child.queue_free()
	# `langs.pane(p -> ...)`: the locale rows scroll, so the Tex.button frame
	# may be narrower than a row (Java clips the trash icon on 1152x648).
	var pane := ScrollContainer.new()
	pane.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_SHOW_NEVER
	pane.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_langs.add_child(pane)
	var rows := VBoxContainer.new()
	pane.add_child(rows)
	for locale_name in _available_locales():
		var locale := str(locale_name)
		if not _locales.has(locale):
			continue
		var line := HBoxContainer.new()
		var select := MindWidgets.button(LANGUAGE_SCRIPT.get_display_name(locale))
		select.theme_type_variation = "flatTogglet"
		select.custom_minimum_size = Vector2(200.0, 50.0)
		select.toggle_mode = true
		select.button_pressed = _selected_locale == locale
		select.pressed.connect(func() -> void:
			if _selected_locale == locale:
				return
			_selected_locale = locale
			_setup())
		line.add_child(select)
		var edit := MindWidgets.icon_button("edit", "", "flati")
		edit.custom_minimum_size = Vector2(50.0, 50.0)
		edit.pressed.connect(_locale_edit_dialog.bind(locale))
		line.add_child(edit)
		var trash := MindWidgets.icon_button("cancel", "", "flati")
		trash.custom_minimum_size = Vector2(50.0, 50.0)
		trash.pressed.connect(_confirm_delete_locale.bind(locale))
		line.add_child(trash)
		rows.add_child(line)
	pane.custom_minimum_size.y = rows.get_combined_minimum_size().y
	var add := MindWidgets.icon_button("add", _t("@add"))
	add.custom_minimum_size = Vector2(250.0, 50.0)
	add.pressed.connect(_add_locale_dialog)
	_langs.add_child(add)


func _show_empty(empty: bool) -> void:
	if _main_scroll != null:
		_main_scroll.visible = not empty
	if _empty_box != null:
		_empty_box.visible = empty


## `buildMain()`: the property cards of the selected locale.
func _build_main() -> void:
	if _main == null:
		return
	for child in _main.get_children():
		_main.remove_child(child)
		child.queue_free()
	if not _locales.has(_selected_locale):
		_locales[_selected_locale] = {}
	var props: Dictionary = _locales[_selected_locale] if _locales[_selected_locale] is Dictionary else {}
	_show_empty(props.is_empty())
	if props.is_empty():
		return
	var keys: Array = props.keys()
	keys.sort()
	var shown_count := 0
	for key_value in keys:
		var key := str(key_value)
		var value := str(props[key])
		var needle: String = value if _search_by_value else key
		if not _search.is_empty() and not needle.to_lower().contains(_search.to_lower()):
			continue
		var status := _property_status(key, value, _selected_locale, false)
		if status == Status.CORRECT and not _show_correct:
			continue
		if status == Status.MISSING and not _show_missing:
			continue
		if status == Status.SAME and not _show_same:
			continue
		_main.add_child(_property_card(key, value, status))
		shown_count += 1
	_show_empty(shown_count == 0)


## One `Tex.whitePane` property card (`:217-300`).
func _property_card(key: String, value: String, status: Status) -> Control:
	# code-instantiated: one card per locale property (data-driven).
	var card := PanelContainer.new()
	card.theme_type_variation = "whitePane"
	card.custom_minimum_size.x = CARD_WIDTH
	var box := VBoxContainer.new()
	card.add_child(box)
	var header := HBoxContainer.new()
	box.add_child(header)
	var expanded := not _collapsed
	var collapse := MindWidgets.icon_button("down-open" if expanded else "up-open", "", "emptyTogglei")
	collapse.custom_minimum_size = Vector2(SMALL, SMALL)
	header.add_child(collapse)
	var name_field := MindWidgets.field("")
	name_field.text = key
	name_field.max_length = 64
	name_field.custom_minimum_size.x = CARD_WIDTH - 125.0
	name_field.text_submitted.connect(func(text: String) -> void: _rename_property(card, key, text))
	name_field.focus_exited.connect(func() -> void: _rename_property(card, key, name_field.text))
	header.add_child(name_field)
	var remove := MindWidgets.icon_button("cancel", "", "emptyi")
	remove.custom_minimum_size = Vector2(SMALL, SMALL)
	remove.pressed.connect(_remove_property.bind(key))
	header.add_child(remove)
	var edit := MindWidgets.icon_button("edit", "", "emptyi")
	edit.custom_minimum_size = Vector2(SMALL, SMALL)
	edit.pressed.connect(_property_edit_dialog.bind(key, value))
	header.add_child(edit)
	var area := TextEdit.new()
	area.text = value
	area.custom_minimum_size.y = 140.0
	area.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	area.text_changed.connect(func() -> void:
		props_put(key, area.text)
		_saved = false
		_apply_button.disabled = false)
	area.visible = expanded
	box.add_child(area)
	collapse.pressed.connect(func() -> void:
		area.visible = not area.visible
		collapse.icon = MindWidgets.icon_texture("down-open" if area.visible else "up-open"))
	card.modulate = _status_color(status)
	return card


## `updateCard`: missing -> `Pal.accent`, same -> `Pal.techBlue`, correct -> gray.
func _status_color(status: Status) -> Color:
	match status:
		Status.MISSING:
			return COLOR_MISSING
		Status.SAME:
			return COLOR_SAME
		_:
			return COLOR_CORRECT


## `getPropertyStatus` (`:322-350`): any other locale lacking the key = missing,
## any other locale with the same value = same, else correct.
func _property_status(key: String, value: String, locale: String, for_view: bool) -> Status:
	for other in _locales.keys():
		var other_locale := str(other)
		if other_locale == locale:
			continue
		var props: Dictionary = _locales[other_locale] if _locales[other_locale] is Dictionary else {}
		if not props.has(key):
			if not for_view:
				return Status.MISSING
		elif str(props[key]) == value:
			return Status.SAME
	return Status.CORRECT


func _selected_props() -> Dictionary:
	if not _locales.has(_selected_locale) or not (_locales[_selected_locale] is Dictionary):
		_locales[_selected_locale] = {}
	return _locales[_selected_locale]


## `props.put(key, value)`: the tag is written on `@apply`/back-confirm only
## (`MapLocalesDialog` keeps the working copy in memory until then).
func props_put(key: String, value: String) -> void:
	_selected_props()[key] = value
	_mark_dirty()


func _mark_dirty() -> void:
	_saved = false
	if _apply_button != null:
		_apply_button.disabled = false


## `@add` on the property card: apply-to-all or selected locale only.
func _add_property() -> void:
	if _prop_name == null:
		return
	var key := _prop_name.text
	var value := _prop_value.text
	if _apply_to_all:
		for locale in _locales.keys():
			if _locales[locale] is Dictionary:
				_locales[locale][key] = value
	else:
		_selected_props()[key] = value
	_mark_dirty()
	_build_main()


## The name field: duplicate keys tint the card `f25555`; otherwise rename.
func _rename_property(card: Control, old_key: String, new_key: String) -> void:
	if new_key.is_empty() or new_key == old_key:
		return
	var props := _selected_props()
	if props.has(new_key):
		# `MapLocalesDialog.java:247-251`: refuse and tint the card red.
		card.modulate = Color("f25555")
		return
	if _apply_to_all:
		for locale in _locales.keys():
			var bundle: Dictionary = _locales[locale] if _locales[locale] is Dictionary else {}
			if bundle.has(old_key):
				var value: String = str(bundle[old_key])
				bundle.erase(old_key)
				bundle[new_key] = value
	else:
		var value: String = str(props[old_key])
		props.erase(old_key)
		props[new_key] = value
	_mark_dirty()
	_build_main()


func _remove_property(key: String) -> void:
	if _apply_to_all:
		for locale in _locales.keys():
			if _locales[locale] is Dictionary:
				_locales[locale].erase(key)
	else:
		_selected_props().erase(key)
	_mark_dirty()
	_build_main()


## `ui.showConfirm("@confirm", "@locales.deletelocale", ...)`.
func _confirm_delete_locale(locale: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("show_confirm"):
		return
	_pending_locale_delete = locale
	ui.call("show_confirm", "%s\n%s" % [_t("@confirm"), _t("@locales.deletelocale")])


## `addLocaleDialog`: pick any `Vars.locales` entry not present yet.
func _add_locale_dialog() -> void:
	var missing: Array = []
	for locale_name in _available_locales():
		var locale := str(locale_name)
		if not _locales.has(locale):
			missing.append(LANGUAGE_SCRIPT.get_display_name(locale))
	if missing.is_empty():
		return
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("open_dialog"):
		return
	_connect_locale_picker()
	ui.call("open_dialog", "list_select", JSON.stringify({
		"title": "@add",
		"items": missing,
		"reset": "",
	}))


## `localeEditDialog(name)`: copy/load the locale's `.properties` text.
func _locale_edit_dialog(locale: String) -> void:
	_overlay("@edit", [
		["@copy.clipboard", "copy", func() -> void:
			DisplayServer.clipboard_set(_write_locale(locale))
			_notify("@copied")
			_close_overlay()],
		["@load.clipboard", "download", func() -> void:
			_locales[locale] = _read_locale(DisplayServer.clipboard_get())
			_mark_dirty()
			_setup()
			_close_overlay()],
	])


## `editDialog()`: copy/load every bundle, or roll back to the last apply.
func _edit_dialog() -> void:
	_overlay("@edit", [
		["@copy.clipboard", "copy", func() -> void:
			DisplayServer.clipboard_set(_write_bundles())
			_notify("@copied")
			_close_overlay()],
		["@load.clipboard", "download", func() -> void:
			_locales = _read_bundles(DisplayServer.clipboard_get())
			_mark_dirty()
			_setup()
			_close_overlay()],
		["@locales.rollback", "undo", func() -> void:
			_locales = _last_saved.duplicate(true)
			_saved = true
			if _apply_button != null:
				_apply_button.disabled = true
			_setup()
			_close_overlay()],
	])


## `propEditDialog`: add-to-other / view-in-all-locales / `@locales.addicon`
## / rollback (`:376-423`).
func _property_edit_dialog(key: String, value: String) -> void:
	_overlay("@edit", [
		["@locales.addtoother", "add", func() -> void:
			for locale in _locales.keys():
				var bundle: Dictionary = _locales[locale] if _locales[locale] is Dictionary else {}
				if not bundle.has(key):
					bundle[key] = value
			_mark_dirty()
			_build_main()
			_close_overlay()],
		["@locales.viewproperty", "zoom", func() -> void:
			_close_overlay()
			_view_property_dialog(key)],
		["@locales.addicon", "image", func() -> void:
			_pending_icon_target = key
			_close_overlay()
			_add_icon_dialog()],
		["@locales.rollback", "undo", func() -> void:
			var saved: Dictionary = _last_saved.get(_selected_locale, {})
			if saved.has(key):
				_selected_props()[key] = saved[key]
				_mark_dirty()
				_build_main()
			_close_overlay()],
	])


## `viewPropertyDialog(key)` (`:483-578`): the property across every locale,
## with its own search row, status filter and editable cards.
func _view_property_dialog(key: String) -> void:
	_view_key = key
	_close_overlay()
	# code-instantiated: the cross-locale view has no static scene.
	var overlay := ColorRect.new()
	overlay.name = "LocalesView"
	overlay.color = Color(0, 0, 0, 0.8)
	overlay.set_anchors_preset(Control.PRESET_FULL_RECT)
	overlay.mouse_filter = Control.MOUSE_FILTER_STOP
	add_child(overlay)
	var center := CenterContainer.new()
	center.set_anchors_preset(Control.PRESET_FULL_RECT)
	overlay.add_child(center)
	var panel := PanelContainer.new()
	panel.theme_type_variation = "defaultDialog"
	center.add_child(panel)
	var box := VBoxContainer.new()
	box.add_theme_constant_override("separation", 6)
	panel.add_child(box)
	box.add_child(MindWidgets.label(_format_bundle("@locales.viewing", key)))
	var search_row := HBoxContainer.new()
	search_row.add_theme_constant_override("separation", 8)
	var filter := MindWidgets.icon_button("filter", "", "emptyi")
	filter.custom_minimum_size = Vector2(SMALL, SMALL)
	filter.pressed.connect(func() -> void: _filter_dialog(_build_prop_view))
	search_row.add_child(filter)
	_view_search = MindWidgets.field(_t("@locales.searchvalue" if _search_by_value else "@locales.searchlocale"))
	_view_search.text = _search
	_view_search.max_length = 64
	_view_search.custom_minimum_size.x = 250.0
	_view_search.text_changed.connect(func(text: String) -> void:
		_search = text
		_build_prop_view())
	search_row.add_child(_view_search)
	var clear := MindWidgets.icon_button("cancel", "", "emptyi")
	clear.custom_minimum_size = Vector2(SMALL, SMALL)
	clear.pressed.connect(func() -> void:
		_search = ""
		if _view_search != null:
			_view_search.text = ""
		_build_prop_view())
	search_row.add_child(clear)
	box.add_child(search_row)
	# code-instantiated: one card per locale (data-driven).
	var scroll := ScrollContainer.new()
	scroll.custom_minimum_size = Vector2(CARD_WIDTH + 20.0, 320.0)
	box.add_child(scroll)
	_view_body = VBoxContainer.new()
	_view_body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_view_body.add_theme_constant_override("separation", 6)
	scroll.add_child(_view_body)
	var back := MindWidgets.button(_t("@back"))
	back.pressed.connect(func() -> void:
		# `dialog.hidden(this::buildMain)` (`:506`).
		var view := get_node_or_null("LocalesView")
		if view != null:
			view.queue_free()
		_build_main())
	box.add_child(back)
	_build_prop_view()


## `buildPropView(key)` (`:511-578`): the per-locale cards of `_view_key`.
func _build_prop_view() -> void:
	if _view_body == null:
		return
	for child in _view_body.get_children():
		_view_body.remove_child(child)
		child.queue_free()
	var shown_count := 0
	for locale_name in _available_locales():
		var locale := str(locale_name)
		if not _locales.has(locale):
			continue
		var props: Dictionary = _locales[locale] if _locales[locale] is Dictionary else {}
		var value: Variant = props.get(_view_key)
		# `getPropertyStatus`: a null value is `missing` in the view (`:330`).
		var status := Status.MISSING if value == null \
			else _property_status(_view_key, str(value), locale, true)
		if status == Status.CORRECT and not _show_correct:
			continue
		if status == Status.MISSING and not _show_missing:
			continue
		if status == Status.SAME and not _show_same:
			continue
		if status != Status.MISSING and not _search.is_empty():
			var needle := str(value).to_lower() if _search_by_value \
				else LANGUAGE_SCRIPT.get_display_name(locale).to_lower()
			if not needle.contains(_search.to_lower()):
				continue
		_view_body.add_child(_view_card(locale, value, status))
		shown_count += 1
	if shown_count == 0:
		_view_body.add_child(MindWidgets.label(_t("@empty")))


func _view_card(locale: String, value: Variant, status: Status) -> Control:
	var card := PanelContainer.new()
	card.theme_type_variation = "whitePane"
	card.custom_minimum_size.x = CARD_WIDTH
	var box := VBoxContainer.new()
	card.add_child(box)
	var head := MindWidgets.label(LANGUAGE_SCRIPT.get_display_name(locale))
	head.add_theme_color_override("font_color", Color("ffd37f"))
	box.add_child(head)
	if status == Status.MISSING:
		# `@add` seeds `moai` (Java quirk) and swaps in the area.
		var add := MindWidgets.icon_button("add", _t("@add"))
		add.pressed.connect(func() -> void:
			props_put_view(locale, "moai")
			_build_prop_view())
		box.add_child(add)
	else:
		var area := TextEdit.new()
		area.text = str(value)
		area.custom_minimum_size = Vector2(CARD_WIDTH, 140.0)
		area.text_changed.connect(func() -> void:
			props_put_view(locale, area.text))
		box.add_child(area)
	card.modulate = _status_color(status)
	return card


## `locales.get(name).put(key, v); saved = false`.
func props_put_view(locale: String, value: String) -> void:
	if not _locales.has(locale) or not (_locales[locale] is Dictionary):
		_locales[locale] = {}
	_locales[locale][_view_key] = value
	_mark_dirty()


## `filterDialog(hidden)` (`:580-617`): show-correct/missing/same +
## search-by-name/value; `after` rebuilds the caller's view on close.
func _filter_dialog(after: Callable = Callable()) -> void:
	var rebuild := func() -> void:
		if after.is_valid():
			after.call()
		else:
			_build_main()
	# Java stacks the filter above its caller; the named overlay does not
	# close the view dialog underneath (`_overlay` closes the default slot).
	var panel := _overlay_named("@locales.filter", "LocalesFilter")
	panel.add_child(MindWidgets.check(_t("@locales.showcorrect"), _show_correct,
		func(pressed: bool) -> void:
			_show_correct = pressed
			rebuild.call()))
	panel.add_child(MindWidgets.check(_t("@locales.showmissing"), _show_missing,
		func(pressed: bool) -> void:
			_show_missing = pressed
			rebuild.call()))
	panel.add_child(MindWidgets.check(_t("@locales.showsame"), _show_same,
		func(pressed: bool) -> void:
			_show_same = pressed
			rebuild.call()))
	panel.add_child(MindWidgets.check(_t("@locales.byvalue"), _search_by_value,
		func(pressed: bool) -> void:
			_search_by_value = pressed
			if _search_field != null:
				_search_field.placeholder_text = _t("@locales.searchvalue" if pressed else "@locales.searchname")
			if _view_search != null:
				_view_search.placeholder_text = _t("@locales.searchvalue" if pressed else "@locales.searchlocale")
			rebuild.call()))
	var back := MindWidgets.button(_t("@back"))
	back.pressed.connect(func() -> void:
		var node := get_node_or_null("LocalesFilter")
		if node != null:
			node.queue_free()
		rebuild.call())
	panel.add_child(back)


## A dialog-local overlay panel (`BaseDialog` sub-dialogs upstream; the
## `custom_rules_dialog._overlay_open` shape).
func _overlay(title_key: String, rows: Array) -> VBoxContainer:
	_close_overlay()
	return _overlay_named(title_key, "LocalesOverlay", rows)


## A named overlay that leaves other overlays in place (stacked sub-dialogs).
func _overlay_named(title_key: String, node_name: String, rows: Array = []) -> VBoxContainer:
	# code-instantiated: per-action popup panels have no static scene.
	var overlay := ColorRect.new()
	overlay.name = node_name
	overlay.color = Color(0, 0, 0, 0.8)
	overlay.set_anchors_preset(Control.PRESET_FULL_RECT)
	overlay.mouse_filter = Control.MOUSE_FILTER_STOP
	add_child(overlay)
	var center := CenterContainer.new()
	center.set_anchors_preset(Control.PRESET_FULL_RECT)
	center.mouse_filter = Control.MOUSE_FILTER_IGNORE
	overlay.add_child(center)
	var panel := PanelContainer.new()
	panel.theme_type_variation = "defaultDialog"
	center.add_child(panel)
	var box := VBoxContainer.new()
	box.add_theme_constant_override("separation", 6)
	panel.add_child(box)
	box.add_child(MindWidgets.label(_t(title_key)))
	for row_data in rows:
		var row: Array = row_data
		var button := MindWidgets.icon_button(str(row[1] if row.size() > 1 else ""), _t(str(row[0])))
		if row.size() > 2:
			button.pressed.connect(row[2])
		box.add_child(button)
	return box


func _close_overlay() -> void:
	var overlay := get_node_or_null("LocalesOverlay")
	if overlay != null:
		overlay.queue_free()


## The `@back` flow (`:64-77`): unsaved edits prompt `@editor.savechanges`.
func _back() -> void:
	if not _saved:
		var ui := get_node_or_null("/root/MindUi")
		if ui != null and ui.has_method("show_confirm"):
			_apply_from_prompt = true
			ui.call("show_confirm", "%s\n\n%s" % [
				_t("@editor.locales"), MindWidgets.markup("@editor.savechanges"),
			])
	_close_pressed()


## `@editor.apply` (`:78-84`): write the tag, remember the applied copy.
func _apply() -> void:
	_apply_now()


func _apply_now() -> void:
	_write()
	_last_saved = _locales.duplicate(true)
	_saved = true
	if _apply_button != null:
		_apply_button.disabled = true


## `?` button: `ui.showInfo("@locales.info")`.
func _show_info() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_info"):
		ui.call("show_info", MindWidgets.markup("@locales.info"))


func _read_locales() -> Dictionary:
	if _editor == null or not _editor.has_method("locales_json"):
		return {}
	var parsed: Variant = JSON.parse_string(str(_editor.call("locales_json")))
	return parsed if parsed is Dictionary else {}


func _write() -> void:
	if _editor != null and _editor.has_method("set_locales_json"):
		_editor.call("set_locales_json", JSON.stringify(_locales))


## `MapLocales.currentLocale()` (`:97-104`): the stored `locale` setting, else
## the live resolved bundle locale (`Locale.getDefault`).
func _default_locale() -> String:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("settings_get_text"):
		var stored := str(ui.call("settings_get_text", "locale", "default"))
		if not stored.is_empty() and stored != "default":
			return stored
	var assets := get_node_or_null("/root/MindAssets")
	if assets != null and assets.has_method("locale"):
		var live := str(assets.call("locale"))
		if not live.is_empty():
			return live
	return "en"


## `Vars.locales` via `MindAssets.locales()`.
func _available_locales() -> PackedStringArray:
	var assets := get_node_or_null("/root/MindAssets")
	if assets != null and assets.has_method("locales"):
		return assets.call("locales")
	return PackedStringArray(["en"])


## `writeLocale(key)` (`:686-694`): `key = value` lines with `\n` escapes.
func _write_locale(locale: String) -> String:
	var props: Dictionary = _locales.get(locale, {}) if _locales.get(locale, {}) is Dictionary else {}
	var data := ""
	for key in props.keys():
		var value: String = str(props[key]).replace("\\n", "\\\\n").replace("\n", "\\n")
		data += "%s = %s\n" % [key, value]
	return data


## `writeBundles()` (`:679-687`): `locale:` headers over `writeLocale`.
func _write_bundles() -> String:
	var data := ""
	for locale in _locales.keys():
		data += "%s:\n%s" % [locale, _write_locale(str(locale))]
	return data


## `readLocale(data)` (`:748-762`).
func _read_locale(data: String) -> Dictionary:
	var out := {}
	for line in data.split("\n", true):
		var index := line.find(" = ")
		if index != -1:
			var value: String = line.substr(index + 3).replace("\\n", "\n").replace("\\\n", "\\n")
			out[line.substr(0, index)] = value
	return out


## `readBundles(data)` (`:734-748`).
func _read_bundles(data: String) -> Dictionary:
	var bundles := {}
	var current := ""
	for line in data.split("\n", true):
		if line.ends_with(":") and not line.contains("="):
			current = line.substr(0, line.length() - 1)
			bundles[current] = {}
			continue
		var index := line.find(" = ")
		if index != -1 and not current.is_empty():
			var value: String = line.substr(index + 3).replace("\\n", "\n").replace("\\\n", "\\n")
			bundles[current][line.substr(0, index)] = value
	return bundles


func _connect_prompts() -> void:
	_connect_locale_picker()
	_connect_icon_select()
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	if ui.has_signal("confirm_result") and not ui.is_connected("confirm_result", _on_confirm):
		ui.connect("confirm_result", _on_confirm)


func _connect_locale_picker() -> void:
	var ui_root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui_root == null or not ui_root.has_method("dialog"):
		return
	var picker: Node = ui_root.call("dialog", "list_select")
	if picker != null and picker.has_signal("selected") \
			and not picker.is_connected("selected", _on_locale_picked):
		picker.selected.connect(_on_locale_picked)


func _connect_icon_select() -> void:
	var ui_root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui_root == null or not ui_root.has_method("dialog"):
		return
	var dialog: Node = ui_root.call("dialog", "icon_select")
	if dialog != null and dialog.has_signal("icon_selected") \
			and not dialog.is_connected("icon_selected", _on_icon_selected):
		dialog.icon_selected.connect(_on_icon_selected)


func _on_locale_picked(display_name: String) -> void:
	if display_name.is_empty():
		return
	for locale_name in _available_locales():
		var locale := str(locale_name)
		if LANGUAGE_SCRIPT.get_display_name(locale) == display_name:
			_locales[locale] = {}
			_selected_locale = locale
			_mark_dirty()
			_setup()
			break


func _on_icon_selected(region_name: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("close_dialog", "icon_select")
	if _pending_icon_target.is_empty():
		return
	var glyph := _icon_char(region_name)
	if glyph.is_empty():
		return
	var props := _selected_props()
	if props.has(_pending_icon_target):
		props[_pending_icon_target] = str(props[_pending_icon_target]) + glyph
		_mark_dirty()
		_build_main()
	_pending_icon_target = ""


func _icon_char(region_name: String) -> String:
	var assets := get_node_or_null("/root/MindAssets")
	if assets != null and assets.has_method("unicode_str"):
		var text := str(assets.call("unicode_str", region_name))
		if not text.is_empty():
			return text
	return MindIcons.glyph(region_name)


func _on_confirm(confirmed: bool) -> void:
	if not _pending_locale_delete.is_empty():
		var locale := _pending_locale_delete
		_pending_locale_delete = ""
		if confirmed:
			_locales.erase(locale)
			if not _locales.has(_selected_locale):
				var keys: Array = _locales.keys()
				_selected_locale = str(keys[0]) if not keys.is_empty() else _default_locale()
			_mark_dirty()
			_setup()
		return
	if _apply_from_prompt:
		_apply_from_prompt = false
		if confirmed:
			_apply_now()


func _notify(key: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_info"):
		ui.call("show_info", _t(key))


## `Core.bundle.format(key, args)` for the `@locales.viewing` title.
func _format_bundle(key: String, arg: String) -> String:
	var assets := get_node_or_null("/root/MindAssets")
	if assets != null and assets.has_method("bundle_format"):
		return str(assets.call("bundle_format", key.trim_prefix("@"), PackedStringArray([arg])))
	return "%s %s" % [_t(key), arg]


## `addIconDialog(cons)` (`:619-706`): the full `Iconc` gallery, then every
## content icon (`item/block/liquid/status/unit`) and the base teams.
func _add_icon_dialog() -> void:
	var box := _overlay_named("@locales.addicon", "LocalesIcons")
	var search_row := HBoxContainer.new()
	search_row.add_theme_constant_override("separation", 8)
	var field := MindWidgets.field(_t("@search"))
	field.custom_minimum_size.x = 250.0
	field.max_length = 64
	field.text_changed.connect(func(text: String) -> void:
		# `v.replace(" ", "").toLowerCase()` (`:623`).
		_icon_search = text.replace(" ", "").to_lower()
		_build_icon_table())
	search_row.add_child(field)
	var clear := MindWidgets.icon_button("cancel", "", "emptyi")
	clear.custom_minimum_size = Vector2(SMALL, SMALL)
	clear.pressed.connect(func() -> void:
		field.text = ""
		_icon_search = ""
		_build_icon_table())
	search_row.add_child(clear)
	box.add_child(search_row)
	# code-instantiated: one button per icon/content entry (data-driven).
	var scroll := ScrollContainer.new()
	scroll.custom_minimum_size = Vector2(720.0, 380.0)
	box.add_child(scroll)
	_icon_body = VBoxContainer.new()
	_icon_body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(_icon_body)
	var back := MindWidgets.button(_t("@back"))
	back.pressed.connect(func() -> void:
		var node := get_node_or_null("LocalesIcons")
		if node != null:
			node.queue_free())
	box.add_child(back)
	_build_icon_table()


## `iconsTable(table, search, dialog, cons)` (`:644-706`).
func _build_icon_table() -> void:
	if _icon_body == null:
		return
	for child in _icon_body.get_children():
		_icon_body.remove_child(child)
		child.queue_free()
	var search := _icon_search
	var added := 0
	# `Iconc.codes` in file order (names whose char resolves).
	var assets := get_node_or_null("/root/MindAssets")
	var icon_names: PackedStringArray = PackedStringArray()
	if assets != null and assets.has_method("icon_names"):
		icon_names = assets.call("icon_names")
	var entries: Array = []
	for icon_name in icon_names:
		var entry_name := str(icon_name)
		if search.is_empty() or entry_name.to_lower().contains(search):
			entries.append(entry_name)
	added += _icon_section(entries, "icon")
	# `contentIcons = {item, block, liquid, status, unit}` (`:37`).
	for type_name in ["item", "block", "liquid", "status", "unit"]:
		var host := get_node_or_null("/root/Spine/SimHost")
		if host == null or not host.has_method("content_list"):
			break
		# `content_list` is a `PackedStringArray`, not a GDScript `Array`.
		var content: Variant = host.call("content_list", type_name)
		var content_entries: Array = []
		if content is PackedStringArray:
			for content_name in content:
				var entry_name := str(content_name)
				if _content_search_ok(entry_name, search):
					content_entries.append(entry_name)
		added += _icon_section(content_entries, str(type_name))
	# `Team.baseTeams` with `team-<name>` regions.
	var teams: Array = []
	for row_value in get_node_or_null("/root/Spine/MindEditor").call("palette_teams"):
		var team_row: Dictionary = row_value
		var team_name := str(team_row.get("name", ""))
		if _content_search_ok(team_name, search):
			teams.append(team_name)
	added += _icon_section(teams, "team")
	if added == 0:
		_icon_body.add_child(MindWidgets.label(_t("@empty")))


func _content_search_ok(entry_name: String, search: String) -> bool:
	return search.is_empty() or entry_name.to_lower().contains(search)


## One grid section; returns the number of rendered tiles.
func _icon_section(names: Array, kind: String) -> int:
	if names.is_empty():
		return 0
	var grid := GridContainer.new()
	grid.columns = 12
	grid.add_theme_constant_override("h_separation", 2)
	grid.add_theme_constant_override("v_separation", 2)
	for icon_name in names:
		var entry_name := str(icon_name)
		var button := _gallery_button(entry_name, kind)
		button.pressed.connect(func() -> void: _insert_icon_char(_icon_char(entry_name)))
		grid.add_child(button)
	_icon_body.add_child(grid)
	return names.size()


## One gallery tile: the Iconc/`uiIcon` texture when resolvable, else the glyph.
func _gallery_button(entry_name: String, kind: String) -> Button:
	var candidates := _region_candidates(kind, entry_name)
	var button: Button = null
	if not candidates.is_empty():
		button = MindWidgets.image_button_first(candidates, "flati")
		button.expand_icon = true
		button.add_theme_constant_override("icon_max_width", 32)
	if button == null or button.icon == null:
		# code-instantiated: one icon-font glyph tile per gallery entry.
		button = Button.new()
		button.theme_type_variation = "flati"
		var glyph := _icon_char(entry_name)
		if glyph.is_empty():
			button.text = entry_name.substr(0, 2)
		else:
			var label := Label.new()
			label.text = glyph
			MindIcons.apply_icon_font(label)
			label.add_theme_font_size_override("font_size", 24)
			label.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
			label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
			label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
			label.mouse_filter = Control.MOUSE_FILTER_IGNORE
			button.add_child(label)
	button.custom_minimum_size = Vector2(44.0, 44.0)
	button.tooltip_text = entry_name
	return button


## The `Icon.icons`/`uiIcon` region names per gallery kind.
func _region_candidates(kind: String, entry_name: String) -> PackedStringArray:
	match kind:
		"icon":
			# `Icon.icons.get(key)` texture (`Iconc` `<name>|<texture>` pair).
			var assets := get_node_or_null("/root/MindAssets")
			if assets != null and assets.has_method("icon_texture"):
				var texture := str(assets.call("icon_texture", entry_name))
				if not texture.is_empty():
					return PackedStringArray([texture])
		"item":
			return PackedStringArray(["item-%s" % entry_name, entry_name])
		"block":
			return PackedStringArray(["block-%s-ui" % entry_name, "block-%s-full" % entry_name, entry_name])
		"liquid":
			return PackedStringArray(["liquid-%s-ui" % entry_name, "liquid-%s" % entry_name])
		"unit":
			return PackedStringArray(["unit-%s-ui" % entry_name, entry_name])
		"status":
			return PackedStringArray(["status-%s" % entry_name, entry_name])
		"team":
			return PackedStringArray(["team-%s" % entry_name])
	return PackedStringArray()


## `cons.get(res)`: append the chosen glyph to the pending property.
func _insert_icon_char(glyph: String) -> void:
	if glyph.is_empty() or _pending_icon_target.is_empty():
		return
	var props := _selected_props()
	if props.has(_pending_icon_target):
		props[_pending_icon_target] = str(props[_pending_icon_target]) + glyph
		_mark_dirty()
		_build_main()
	var node := get_node_or_null("LocalesIcons")
	if node != null:
		node.queue_free()
	_pending_icon_target = ""
