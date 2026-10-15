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
	add_button(_t("@back"), _back, "left", 210.0)
	_apply_button = add_button(_t("@editor.apply"), _apply, "ok", 210.0)
	add_button(_t("@edit"), _edit_dialog, "edit", 210.0)
	add_button("?", _show_info, "", 60.0)
	_connect_prompts()


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

	# code-instantiated: the locale list is data-driven from the locales tag.
	_langs = VBoxContainer.new()
	_langs.custom_minimum_size.x = 260.0
	row.add_child(_langs)

	var right := VBoxContainer.new()
	right.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	right.size_flags_vertical = Control.SIZE_EXPAND_FILL
	row.add_child(right)

	# Search / collapse-all / filter row (`:124-147`).
	# code-instantiated: the search row is a fixed four-control strip.
	var search_row := HBoxContainer.new()
	search_row.add_theme_constant_override("separation", 8)
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
	right.add_child(MindWidgets.check(_t("@locales.applytoall"), _apply_to_all,
		func(pressed: bool) -> void: _apply_to_all = pressed))

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

	# `cont.table(Tex.button, t -> { name field; value area; @add })`.
	# code-instantiated: the property-add card is a fixed three-control block.
	var add_panel := PanelContainer.new()
	add_panel.theme_type_variation = "smallPane"
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
	add_box.add_child(add_prop)
	right.add_child(add_panel)

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
	for name in _available_locales():
		var locale := str(name)
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
		_langs.add_child(line)
	var add := MindWidgets.icon_button("add", _t("@add"))
	add.custom_minimum_size = Vector2(250.0, 50.0)
	add.pressed.connect(_add_locale_dialog)
	_langs.add_child(add)


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
	if props.is_empty():
		_main.add_child(MindWidgets.label(_t("@empty")))
		return
	var keys: Array = props.keys()
	keys.sort()
	var shown := 0
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
		shown += 1
	if shown == 0:
		_main.add_child(MindWidgets.label(_t("@empty")))


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
	name_field.text_submitted.connect(func(text: String) -> void: _rename_property(key, text))
	name_field.focus_exited.connect(func() -> void: _rename_property(key, name_field.text))
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
func _rename_property(old_key: String, new_key: String) -> void:
	if new_key.is_empty() or new_key == old_key:
		return
	var props := _selected_props()
	if props.has(new_key):
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
	for name in _available_locales():
		var locale := str(name)
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


## `propEditDialog`: add-to-other / rollback (view-in-all-locales and
## `@locales.addicon` are still missing).
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
		["@locales.rollback", "undo", func() -> void:
			var saved: Dictionary = _last_saved.get(_selected_locale, {})
			if saved.has(key):
				_selected_props()[key] = saved[key]
				_mark_dirty()
				_build_main()
			_close_overlay()],
		["@locales.addicon", "image", func() -> void:
			_pending_icon_target = key
			_connect_icon_select()
			_close_overlay()
			var ui := get_node_or_null("/root/MindUi")
			if ui != null:
				ui.call("open_dialog", "icon_select", "{}")],
	])


## `filterDialog`: show-correct/missing/same + search-by-name/value.
func _filter_dialog() -> void:
	var panel := _overlay("@locales.filter", [])
	panel.add_child(MindWidgets.check(_t("@locales.showcorrect"), _show_correct,
		func(pressed: bool) -> void:
			_show_correct = pressed
			_build_main()))
	panel.add_child(MindWidgets.check(_t("@locales.showmissing"), _show_missing,
		func(pressed: bool) -> void:
			_show_missing = pressed
			_build_main()))
	panel.add_child(MindWidgets.check(_t("@locales.showsame"), _show_same,
		func(pressed: bool) -> void:
			_show_same = pressed
			_build_main()))
	panel.add_child(MindWidgets.check(_t("@locales.byvalue"), _search_by_value,
		func(pressed: bool) -> void:
			_search_by_value = pressed
			if _search_field != null:
				_search_field.placeholder_text = _t("@locales.searchvalue" if pressed else "@locales.searchname")
			_build_main()))


## A dialog-local overlay panel (`BaseDialog` sub-dialogs upstream; the
## `custom_rules_dialog._overlay_open` shape).
func _overlay(title_key: String, rows: Array) -> VBoxContainer:
	_close_overlay()
	# code-instantiated: per-action popup panels have no static scene.
	var overlay := ColorRect.new()
	overlay.name = "LocalesOverlay"
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
	for name in _available_locales():
		var locale := str(name)
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
