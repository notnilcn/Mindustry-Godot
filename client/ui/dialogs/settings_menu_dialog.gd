## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/SettingsMenuDialog.java (plan 14 §3.4).
##
## Fill-parent `BaseDialog` whose scroll pane (`prefs`) shows either the
## category menu (`menu`, a `Tex.button` table of 300x60 `Styles.flatt` rows)
## or one category's `SettingsTable`. `shown` and Back return to the menu
## (`back()`); Back/Escape on the menu hides the dialog (`addCloseButton`
## override, `SettingsMenuDialog.java:656-675`). Language/Controls open their
## own dialogs; Game Data shows the `dataDialog` (and its `planetDataDialog`)
## in place, titled `@settings.data`, with Back returning one level.
##
## The category order, row keys, kinds, ranges and value formats come from the
## Rust model `mind_core::ui::settings` through the `MindUi.settings_*`
## endpoints; this script only renders and forwards edits. Values persist
## through the shared settings store behind `MindUi`; sound rows additionally
## drive the live `MindAudio` mixer. `shouldPause = true` matches upstream.

extends MindDialog

## `dataDialog` actions (`SettingsMenuDialog.java:175-291`, desktop order).
const DATA_ACTIONS := [
	{"id": "clear-data", "text": "@settings.cleardata", "icon": "trash"},
	{"id": "clear-planet-data", "text": "@settings.clearplanetdata", "icon": "trash"},
	{"id": "clear-saves", "text": "@settings.clearsaves", "icon": "trash"},
	{"id": "clear-research", "text": "@settings.clearresearch", "icon": "trash"},
	{"id": "clear-campaign-saves", "text": "@settings.clearcampaignsaves", "icon": "trash"},
	{"id": "export-data", "text": "@data.export", "icon": "upload"},
	{"id": "import-data", "text": "@data.import", "icon": "download"},
	{"id": "open-folder", "text": "@data.openfolder", "icon": "folder"},
	{"id": "export-crash-logs", "text": "@crash.export", "icon": "upload"},
]

## Upstream confirm text for the destructive data actions.
const DATA_CONFIRMS := {
	"clear-data": "@settings.clearall.confirm",
	"clear-saves": "@settings.clearsaves.confirm",
	"clear-research": "@settings.clearresearch.confirm",
	"clear-campaign-saves": "@settings.clearcampaignsaves.confirm",
	"import-data": "@data.import.confirm",
}

## Page ids that are not settings tables.
const PAGE_MENU := ""
const PAGE_DATA := "data"
const PAGE_PLANET_DATA := "planet-data"

## `menu.defaults().size(300f, 60f)` (`rebuildMenu`).
const MENU_BUTTON_SIZE := Vector2(300.0, 60.0)
## `t.defaults().size(280f, 60f)` (`dataDialog`/`planetDataDialog`).
const DATA_BUTTON_SIZE := Vector2(280.0, 60.0)
## `float marg = 8f` (menu rows) / `.marginLeft(4)` (data rows).
const MENU_MARGIN_LEFT := 8.0
const DATA_MARGIN_LEFT := 4.0
## `isize = iconMed` (`Scl.scl(32f)`): the icon cell; the glyph font size
## that fills it like the reference frame (30 px gear).
const ICON_CELL := 32.0
const ICON_FONT_SIZE := 28
## `prefs.margin(14f)`.
const PREFS_MARGIN := 14
## `Math.min(Core.graphics.getWidth() / 1.2f / Scl.scl(1f), 500f)` row width.
const ROW_MAX_WIDTH := 500.0
## `Styles.flatOver` (`whiteui.tint(#454545)`); `flatt.up = black`.
const FLAT_OVER := Color(0.27, 0.27, 0.27, 1.0)
## `Tex.button` ninepatch: 12 px splits; the reference frame shows the 300 px
## rows inside a 322 px box (11 px padding per side).
const BUTTON_SPLIT := 12.0
const BUTTON_PAD := 11.0

var _categories: Array = []
## Current page: `PAGE_MENU`, a table category id, `PAGE_DATA` or
## `PAGE_PLANET_DATA` (`prefs.getChildren().first()`).
var _page := PAGE_MENU
var _pending_action := ""
## Selected planet on the planet-data page (`SettingsMenuDialog.planet`).
var _planet_name := ""
## Scale the Graphics table was built with (`SettingsMenuDialog.lastUiScale`).
var _uiscale_start := -1


func _ready() -> void:
	set_title_key("@settings")
	should_pause = true
	# `BaseDialog.setFillParent(true)` over `Styles.defaultDialog`
	# (`windowEmpty` background, `black9` stage = the shell's `Dim`).
	full_dialog = true
	super._ready()
	_connect_confirm()
	_categories = _load_categories()
	_setup_prefs()
	_add_back_button()
	_show_menu()


## `shown(() -> { back(); rebuildMenu(); })`. Never calls `MindUi` here:
## `open_dialog` holds its mutable borrow while `shown()` runs.
func shown() -> void:
	_show_menu()


## The pane (`pane(prefs).grow().top()`): the shell's scroll body. Upstream's
## removed `cont` cell keeps its `expand()`, so a page shorter than the pane
## sits centered between the title and the Back row (reference frame: the
## menu at y 87..529 of 648); a longer page scrolls from the top.
func _setup_prefs() -> void:
	if body != null:
		body.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	if cont == null:
		return
	cont.size_flags_vertical = Control.SIZE_EXPAND_FILL
	if cont is BoxContainer:
		(cont as BoxContainer).alignment = BoxContainer.ALIGNMENT_CENTER


## `addCloseButton` override (`SettingsMenuDialog.java:656-664`):
## `buttons.button("@back", Icon.left, ...).size(210f, 64f)`; Back returns to
## the menu from a category page and hides the dialog from the menu.
func _add_back_button() -> void:
	var button := add_close_button(210.0)
	button.custom_minimum_size = Vector2(210.0, 64.0)
	if button.pressed.is_connected(_close_pressed):
		button.pressed.disconnect(_close_pressed)
	button.pressed.connect(_on_back)


## `keyDown(escape|back)` (`SettingsMenuDialog.java:666-674`): a category page
## goes back to the menu; on the menu the shared ESC path hides the dialog.
func _unhandled_key_input(event: InputEvent) -> void:
	if not visible or _page == PAGE_MENU:
		return
	var key := event as InputEventKey
	if key == null or not key.pressed or key.echo:
		return
	if key.keycode != KEY_ESCAPE and key.keycode != KEY_BACK:
		return
	if not is_top():
		return
	_on_back()
	get_viewport().set_input_as_handled()


func _on_back() -> void:
	match _page:
		PAGE_MENU:
			_close_pressed()
		PAGE_PLANET_DATA:
			# `planetDataDialog` Back reveals the `dataDialog` under it.
			_show_data()
		_:
			_show_menu()


func _load_categories() -> Array:
	var ui := _ui()
	if ui != null and ui.has_method("settings_categories_json"):
		var parsed: Variant = JSON.parse_string(str(ui.call("settings_categories_json")))
		if parsed is Array:
			return parsed
	return []


func _load_rows(category_id: String) -> Array:
	var ui := _ui()
	if ui != null and ui.has_method("settings_rows_json"):
		var parsed: Variant = JSON.parse_string(str(ui.call("settings_rows_json", category_id)))
		if parsed is Array:
			return parsed
	return []


## Replaces the pane content (`prefs.clearChildren(); prefs.add(page)`),
## wrapped in the `prefs.margin(14f)` inset and centered horizontally.
func _set_page(page_id: String, title_key: String, page: Control) -> void:
	_page = page_id
	set_title_key(title_key)
	if cont == null:
		return
	for child in cont.get_children():
		cont.remove_child(child)
		child.queue_free()
	# code-instantiated: the pane holds one runtime-built page at a time
	# (menu / category table / data actions), swapped on every navigation.
	var margin := MarginContainer.new()
	margin.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	for side in ["left", "top", "right", "bottom"]:
		margin.add_theme_constant_override("margin_" + side, PREFS_MARGIN)
	margin.add_child(page)
	cont.add_child(margin)
	if body != null:
		body.scroll_vertical = 0


## `back()` + `rebuildMenu()`: the category menu (`SettingsMenuDialog.java:336-365`).
func _show_menu() -> void:
	var rows := VBoxContainer.new()  # code-instantiated: menu rows follow the model categories.
	rows.add_theme_constant_override("separation", 0)
	for category_variant in _categories:
		var category: Dictionary = category_variant
		var button := _flat_button(
			str(category.get("icon", "")),
			_t("@" + str(category.get("key", ""))),
			MENU_BUTTON_SIZE,
			MENU_MARGIN_LEFT
		)
		button.pressed.connect(_on_category_pressed.bind(category))
		rows.add_child(button)
	_set_page(PAGE_MENU, "@settings", _button_panel(rows))


func _on_category_pressed(category: Dictionary) -> void:
	match str(category.get("action", "table")):
		"table":
			_show_table(str(category.get("id", "")))
		"language", "controls":
			# `ui.language::show` / `ui.controls::show` over this dialog.
			var ui := _ui()
			if ui != null:
				ui.call("open_dialog", str(category.get("action", "")), "")
		"data":
			_show_data()


## `visible(index)`: one category's `SettingsTable` (`SettingsTable.rebuild`).
func _show_table(category_id: String) -> void:
	# code-instantiated: setting rows are data-driven from the model and the
	# shared store; the row count and keys are not known at scene build time.
	var table := VBoxContainer.new()
	table.add_theme_constant_override("separation", 0)
	table.custom_minimum_size.x = _row_width()
	for row in _load_rows(category_id):
		_add_row(table, row)
	_add_reset_button(table, category_id)
	_set_page(category_id, "@settings", table)


## `Math.min(Core.graphics.getWidth() / 1.2f / Scl.scl(1f), 500f)`.
func _row_width() -> float:
	return minf(ROW_MAX_WIDTH, get_viewport_rect().size.x / 1.2)


## `Setting(name)`: `setting.<name>.name` (raw key fallback) and the optional
## `setting.<name>.description` tooltip (`ui.addDescTooltip`).
func _setting_title(key: String) -> String:
	return _bundle_or(("setting.%s.name" % key), key)


func _setting_description(key: String) -> String:
	return _bundle_or(("setting.%s.description" % key), "")


func _bundle_or(key: String, fallback: String) -> String:
	var value := _t("@" + key)
	return fallback if value == key or value.is_empty() else value


func _add_row(table: VBoxContainer, row: Dictionary) -> void:
	var key := str(row.get("key", ""))
	if key.is_empty():
		return
	var title := _setting_title(key)
	match str(row.get("kind", "check")):
		"check":
			# `CheckSetting.add`: `minWidth(min(500, w/1.2)).fillX().height(45f)
			# .left().padTop(7f)`.
			table.add_child(MindWidgets.space(0.0, 7.0))
			var check := MindWidgets.check(
				title, bool(row.get("value", false)), func(value): _set_value(key, value)
			)
			check.custom_minimum_size = Vector2(_row_width(), 45.0)
			check.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			check.tooltip_text = _setting_description(key)
			table.add_child(check)
		"slider":
			_add_slider_row(table, row, key, title)


## `SliderSetting.add`: `table.stack(slider, content).width(...).left().padTop(4f)`;
## `content` (title left/growX/wrap, value right, `margin(3, 33, 3, 33)`,
## `touchable = disabled`) is drawn over the slider.
func _add_slider_row(table: VBoxContainer, row: Dictionary, key: String, title: String) -> void:
	var value := int(row.get("value", 0))
	# `int[] lastUiScale = {settings.getInt("uiscale", 100)}` is captured once.
	if key == "uiscale" and _uiscale_start < 0:
		_uiscale_start = value
	table.add_child(MindWidgets.space(0.0, 4.0))
	# code-instantiated: the stack/value header is data-driven per row.
	var stack := MarginContainer.new()
	stack.custom_minimum_size.x = _row_width()
	var slider := MindWidgets.slider(
		float(row.get("min", 0)), float(row.get("max", 100)), float(row.get("step", 1))
	)
	slider.value = value
	slider.tooltip_text = _setting_description(key)
	slider.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	slider.size_flags_vertical = Control.SIZE_EXPAND_FILL
	stack.add_child(slider)
	var content := MarginContainer.new()
	content.mouse_filter = Control.MOUSE_FILTER_IGNORE
	content.add_theme_constant_override("margin_left", 33)
	content.add_theme_constant_override("margin_right", 33)
	content.add_theme_constant_override("margin_top", 3)
	content.add_theme_constant_override("margin_bottom", 3)
	var header := HBoxContainer.new()
	header.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var title_label := MindWidgets.styled_label(title, "outlineLabel")
	title_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	title_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	title_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	title_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	header.add_child(title_label)
	# `Styles.outlineLabel` cells centre vertically in Arc; a Godot `Label`
	# defaults to top alignment, which would ride above the slider's track.
	var value_label := MindWidgets.styled_label(_format_value(row, value), "outlineLabel")
	value_label.autowrap_mode = TextServer.AUTOWRAP_OFF
	value_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	value_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	header.add_child(value_label)
	content.add_child(header)
	stack.add_child(content)

	slider.value_changed.connect(
		func(new_value: float):
			var number := int(new_value)
			value_label.text = _format_value(row, number)
			_set_value(key, number)
			# `SettingsMenuDialog.java:432-438`: flag the change so the next
			# boot confirms the scale (`Control.java` prompt).
			if key == "uiscale" and _uiscale_start >= 0:
				var ui := _ui()
				if ui != null:
					ui.call("set_uiscale_changed", number != _uiscale_start)
	)
	table.add_child(stack)


## `button(bundle.get("settings.reset", "Reset to Defaults"), ...)
## .margin(14).width(240f).pad(6)` closing every `SettingsTable`.
func _add_reset_button(table: VBoxContainer, category_id: String) -> void:
	table.add_child(MindWidgets.space(0.0, 6.0))
	var button := MindWidgets.button(_bundle_or("settings.reset", "Reset to Defaults"))
	button.custom_minimum_size = Vector2(240.0, 54.0)
	button.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	button.pressed.connect(
		func():
			var ui := _ui()
			if ui != null:
				ui.call("settings_reset", category_id)
			if category_id == "sound":
				_sync_audio_defaults()
			_show_table(category_id)
	)
	table.add_child(button)
	table.add_child(MindWidgets.space(0.0, 6.0))


## The audio mixer keeps its own live settings mirror; after a Sound reset
## push the restored values to it (upstream reads `Core.settings` directly).
func _sync_audio_defaults() -> void:
	var audio := get_node_or_null("/root/Spine/MindAudio")
	if audio == null:
		return
	for row in _load_rows("sound"):
		var value: Variant = row.get("value")
		if str(row.get("kind", "")) == "slider":
			value = int(value)
		audio.call("set_setting", str(row.get("key", "")), value)


## `dataDialog` (`SettingsMenuDialog.java:172-292`), shown in place.
func _show_data() -> void:
	var rows := VBoxContainer.new()  # code-instantiated: data actions are a data-driven list.
	rows.add_theme_constant_override("separation", 0)
	for action in DATA_ACTIONS:
		var button := _flat_button(
			str(action.icon), _t(str(action.text)), DATA_BUTTON_SIZE, DATA_MARGIN_LEFT
		)
		button.pressed.connect(_run_data_action.bind(str(action.id)))
		rows.add_child(button)
	_set_page(PAGE_DATA, "@settings.data", _button_panel(rows))


func _run_data_action(action_id: String) -> void:
	var ui := _ui()
	if ui == null:
		return
	match action_id:
		"clear-planet-data":
			_show_planet_data()
		"export-data":
			_pick_file("@data.export", "mindustry-data-export.zip",
					false, PackedStringArray(["*.zip"]), _on_export_data)
		"export-crash-logs":
			if bool(ui.call("crash_logs_available")):
				_pick_file("@crash.export", "logs.txt",
						false, PackedStringArray(["*.txt"]), _on_export_crash_logs)
			else:
				ui.call("show_info", _t("@crash.none"))
		"import-data", "clear-data", "clear-saves", "clear-research", "clear-campaign-saves":
			# Destructive: confirm first (upstream `ui.showConfirm`).
			_pending_action = action_id
			ui.call("show_confirm", _t(str(DATA_CONFIRMS[action_id])))
		_:
			ui.call("settings_action", action_id)


## `planetDataDialog` (`SettingsMenuDialog.java:93-170`), shown in place.
func _show_planet_data() -> void:
	var planets := _selectable_planets()
	if planets.is_empty():
		_show_data()
		return
	if not planets.any(func(row: Dictionary) -> bool: return str(row.get("name", "")) == _planet_name):
		_planet_name = str(planets[0].get("name", ""))
	# code-instantiated: planet entries are runtime content data, not scene nodes.
	var rows := VBoxContainer.new()
	rows.add_theme_constant_override("separation", 0)
	var select := _flat_button(
		"planet", _planet_select_text(), DATA_BUTTON_SIZE, DATA_MARGIN_LEFT, true
	)
	select.pressed.connect(_open_planet_menu)
	rows.add_child(select)
	var research := _flat_button(
		"trash", _t("@settings.clearplanetresearch"), DATA_BUTTON_SIZE, DATA_MARGIN_LEFT
	)
	research.pressed.connect(_confirm_planet_research)
	rows.add_child(research)
	var saves := _flat_button(
		"trash", _t("@settings.clearplanetcampaignsaves"), DATA_BUTTON_SIZE, DATA_MARGIN_LEFT
	)
	saves.pressed.connect(_confirm_planet_campaign_saves)
	rows.add_child(saves)
	_set_page(PAGE_PLANET_DATA, "@settings.data", _button_panel(rows))


func _open_planet_menu() -> void:
	var planets := _selectable_planets()
	var menu := PopupMenu.new()  # code-instantiated: planet entries are runtime data.
	for index in planets.size():
		menu.add_item(_t("@planet.%s.name" % str(planets[index].get("name", ""))), index)
	menu.id_pressed.connect(
		func(index: int) -> void:
			if index >= 0 and index < planets.size():
				_planet_name = str(planets[index].get("name", ""))
				_show_planet_data()
	)
	menu.close_requested.connect(menu.queue_free)
	add_child(menu)
	menu.popup_centered()


func _confirm_planet_research() -> void:
	_pending_action = "planet-research"
	var ui := _ui()
	if ui != null:
		ui.call(
			"show_confirm",
			MindWidgets.markup_format(
				"@settings.clearplanetresearch.confirm", [_planet_display()]
			)
		)


func _confirm_planet_campaign_saves() -> void:
	_pending_action = "planet-campaign-saves"
	var ui := _ui()
	if ui != null:
		ui.call(
			"show_confirm",
			MindWidgets.markup_format(
				"@settings.clearplanetcampaignsaves.confirm", [_planet_display()]
			)
		)


## `new Table(Tex.button)`: the ninepatch box around the menu/data rows.
func _button_panel(content: Control) -> PanelContainer:
	# code-instantiated: the panel wraps the runtime-built rows of each page.
	var panel := PanelContainer.new()
	panel.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	var texture := MindWidgets.icon_texture("button")
	if texture != null:
		var box := StyleBoxTexture.new()
		box.texture = texture
		box.texture_margin_left = BUTTON_SPLIT
		box.texture_margin_top = BUTTON_SPLIT
		box.texture_margin_right = BUTTON_SPLIT
		box.texture_margin_bottom = BUTTON_SPLIT
		box.content_margin_left = BUTTON_PAD
		box.content_margin_top = BUTTON_PAD
		box.content_margin_right = BUTTON_PAD
		box.content_margin_bottom = BUTTON_PAD
		panel.add_theme_stylebox_override("panel", box)
	else:
		# Atlas absent (test host): the flat `Tex.button` look (black fill,
		# 4 px #454545 outline).
		var flat := StyleBoxFlat.new()
		flat.bg_color = Color.BLACK
		flat.set_border_width_all(4)
		flat.border_color = FLAT_OVER
		flat.set_corner_radius_all(4)
		flat.set_content_margin_all(BUTTON_PAD)
		panel.add_theme_stylebox_override("panel", flat)
	panel.add_child(content)
	return panel


## `button(text, icon, Styles.flatt, isize, listener).marginLeft(marg)`: a
## black row (`flatOver` on hover/press), the `iconMed` icon cell at the left
## margin and the label centered in the remaining width.
## `rich` renders `text_value` as BBCode (the planet-select row's colored name).
func _flat_button(
	icon_name: String, text_value: String, button_size: Vector2, margin_left: float, rich: bool = false
) -> Button:
	# code-instantiated: menu/data rows are data-driven per page.
	var button := Button.new()
	button.custom_minimum_size = button_size
	button.focus_mode = Control.FOCUS_NONE
	var up := _flat_box(Color.BLACK)
	var over := _flat_box(FLAT_OVER)
	button.add_theme_stylebox_override("normal", up)
	button.add_theme_stylebox_override("hover", over)
	button.add_theme_stylebox_override("pressed", over)
	button.add_theme_stylebox_override("hover_pressed", over)
	button.add_theme_stylebox_override("disabled", up)
	button.add_theme_stylebox_override("focus", StyleBoxEmpty.new())
	var row := HBoxContainer.new()
	row.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	row.offset_left = margin_left
	row.add_theme_constant_override("separation", 0)
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	button.add_child(row)
	var icon := MindWidgets.glyph(icon_name, ICON_FONT_SIZE)
	icon.custom_minimum_size = Vector2(ICON_CELL, ICON_CELL)
	icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(icon)
	if rich:
		var rich_label := MindWidgets.label(text_value)
		rich_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		rich_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
		rich_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		rich_label.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		rich_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
		row.add_child(rich_label)
		return button
	var label := Label.new()
	label.text = text_value
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	label.clip_text = true
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(label)
	return button


func _flat_box(fill: Color) -> StyleBoxFlat:
	var box := StyleBoxFlat.new()
	box.bg_color = fill
	return box


func _connect_confirm() -> void:
	var ui := _ui()
	if ui != null and ui.has_signal("confirm_result") \
			and not ui.is_connected("confirm_result", Callable(self, "_on_confirm")):
		ui.connect("confirm_result", Callable(self, "_on_confirm"))


func _on_confirm(confirmed: bool) -> void:
	if _pending_action.is_empty():
		return
	var action_id := _pending_action
	_pending_action = ""
	if not confirmed:
		return
	var ui := _ui()
	if ui == null:
		return
	match action_id:
		"import-data":
			_pick_file("@data.import", "",
					true, PackedStringArray(["*.zip"]), _on_import_data)
		"planet-research":
			_campaign_action("clear_planet_research")
		"planet-campaign-saves":
			_campaign_action("clear_planet_campaign_saves")
		_:
			ui.call("settings_action", action_id)


## Native file chooser (`FileChooser.export`/`open`, native-first like the
## mods dialog; the plan-14 `file_chooser` dialog is the fallback path).
func _pick_file(
	title_key: String,
	default_name: String,
	open: bool,
	filters: PackedStringArray,
	callback: Callable
) -> void:
	DisplayServer.file_dialog_show(
		_t(title_key),
		"",
		default_name,
		false,
		DisplayServer.FILE_DIALOG_MODE_OPEN_FILES if open else DisplayServer.FILE_DIALOG_MODE_SAVE_FILE,
		filters,
		callback
	)


func _on_export_data(status: bool, paths: PackedStringArray, _filter: int) -> void:
	if not status or paths.is_empty():
		return
	var ui := _ui()
	if ui != null:
		ui.call("data_export", paths[0])


func _on_import_data(status: bool, paths: PackedStringArray, _filter: int) -> void:
	if not status or paths.is_empty():
		return
	var ui := _ui()
	if ui == null:
		return
	if not bool(ui.call("data_import", paths[0])):
		ui.call("show_info", _t("@data.invalid"))


func _on_export_crash_logs(status: bool, paths: PackedStringArray, _filter: int) -> void:
	if not status or paths.is_empty():
		return
	var ui := _ui()
	if ui != null:
		ui.call("export_crash_logs", paths[0])


func _campaign_action(method: String) -> void:
	var campaign := _campaign()
	if campaign != null:
		campaign.call(method, _planet_name)


func _selectable_planets() -> Array:
	var campaign := _campaign()
	if campaign == null or not campaign.has_method("selectable_planets"):
		return []
	var result: Variant = campaign.call("selectable_planets")
	return result if result is Array else []


func _planet_display() -> String:
	return _t("@planet.%s.name" % _planet_name)


## `settings.planetselect` with the upstream `[#iconColor]` name color.
func _planet_select_text() -> String:
	var label := _planet_display()
	for row in _selectable_planets():
		if str(row.get("name", "")) == _planet_name:
			var color: Variant = row.get("iconColor")
			if color is Color:
				label = "[#%s]%s[]" % [(color as Color).to_html(false), label]
			break
	return MindWidgets.markup_format("@settings.planetselect", [label])


func _campaign() -> Node:
	return get_node_or_null("/root/Spine/MindCampaign")


func _set_value(key: String, value: Variant) -> void:
	var ui := _ui()
	if ui == null:
		return
	ui.call("settings_set", key, value)
	if _page == "sound":
		# The audio mixer keeps its own live settings mirror; drive it directly.
		var audio := get_node_or_null("/root/Spine/MindAudio")
		if audio != null:
			audio.call("set_setting", key, value)


## Upstream `SliderSetting.sp` value processors, keyed by the model's format tag.
func _format_value(row: Dictionary, value: int) -> String:
	match str(row.get("format", "plain")):
		"percent":
			return "%d%%" % value
		"seconds":
			return MindWidgets.markup_format("@setting.seconds", [value])
		"multiplier":
			# `(i / 4f) + "x"`: Java float `toString` (`1.0x`, `0.25x`, `1.75x`).
			return _java_float(value / 4.0) + "x"
		"x":
			return "%dx" % value
		"bloomPercent":
			return "%d%%" % int(value / 4.0 * 100.0)
		"pixels":
			return "%dpx" % value
		"fpsCap":
			if value > 240:
				return _t("@setting.fpscap.none")
			return MindWidgets.markup_format("@setting.fpscap.text", [value])
		_:
			return str(value)


## Java `Float.toString` for the quarter steps the screenshake slider produces.
func _java_float(number: float) -> String:
	var text := str(snappedf(number, 0.01))
	if not text.contains("."):
		text += ".0"
	return text


func _ui() -> Node:
	return get_node_or_null("/root/MindUi")
