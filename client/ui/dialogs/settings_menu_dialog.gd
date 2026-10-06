## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/SettingsMenuDialog.java (plan 14 §3.4).
##
## Category rail + settings table. The category order, row keys, kinds, ranges
## and value formats come from the Rust model `mind_core::ui::settings` through
## the `MindUi.settings_*` endpoints; this script only renders and forwards
## edits. Values persist through the plan-04 `SettingsStore` behind `MindUi`;
## sound rows additionally drive the live `MindAudio` mixer.
## `shouldPause = true` matches upstream.

extends MindDialog

## Data-category actions rendered inline (upstream opens its `dataDialog`).
## Only actions with a ported backing subsystem are listed; the Rust endpoint
## rejects unknown ids instead of faking success.
const DATA_ACTIONS := [
	{"id": "clear-saves", "text": "@settings.clearsaves", "icon": "trash"},
	{"id": "open-folder", "text": "@data.openfolder", "icon": "folder"},
]

var _rail: VBoxContainer = null
var _table_host: MindTable = null
var _categories: Array = []
var _selected := ""
var _pending_action := ""


func _ready() -> void:
	set_title_key("@settings")
	should_pause = true
	super._ready()
	_connect_confirm()
	_categories = _load_categories()
	_build()
	add_close_button()


func _build() -> void:
	var root := content_table()
	root.add_theme_constant_override("separation", 8)
	var columns := HBoxContainer.new()
	columns.size_flags_vertical = Control.SIZE_EXPAND_FILL
	columns.add_theme_constant_override("separation", 14)
	root.add(columns).grow_x_axis().grow_y_axis()

	# code-instantiated: the category rail is the data-driven SettingsCategory
	# list from the model; no static scene exists per category.
	_rail = VBoxContainer.new()
	_rail.add_theme_constant_override("separation", 4)
	for category in _categories:
		var button := MindWidgets.icon_button(
			str(category.get("icon", "")), _t("@" + str(category.get("key", ""))), "flatt"
		)
		# Upstream `menu.defaults().size(300f, 60f)` at the reference's UI scale.
		button.custom_minimum_size = Vector2(300.0, 50.0)
		button.pressed.connect(_on_category_pressed.bind(category))
		_rail.add_child(button)
	columns.add_child(_rail)

	_table_host = MindTable.new()
	_table_host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_table_host.size_flags_vertical = Control.SIZE_EXPAND_FILL
	columns.add_child(_table_host)
	if not _categories.is_empty():
		_show_table(str(_categories[0].get("id", "")))


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


func _on_category_pressed(category: Dictionary) -> void:
	match str(category.get("action", "table")):
		"table":
			_show_table(str(category.get("id", "")))
		"language", "controls":
			var ui := _ui()
			if ui != null:
				ui.call("open_dialog", str(category.get("action", "")), "")
		"data":
			_show_data()


func _show_table(category_id: String) -> void:
	_selected = category_id
	if _table_host == null:
		return
	_table_host.clear_children()
	# code-instantiated: setting rows are data-driven from the model and the
	# plan-04 store; the row count and keys are not known at scene build time.
	for row in _load_rows(category_id):
		_add_row(row)
	_add_reset_button()
	_resort_table()


func _add_row(row: Dictionary) -> void:
	var key := str(row.get("key", ""))
	if key.is_empty():
		return
	var title := _t("@setting.%s.name" % key)
	match str(row.get("kind", "check")):
		"check":
			var check := MindWidgets.check(
				title, bool(row.get("value", false)), func(value): _set_value(key, value)
			)
			_table_host.add(check).grow_x_axis().fill_y_axis().set_min_height(45.0).set_pad_top(7.0)
			_table_host.row()
		"slider":
			_add_slider_row(row, key, title)


func _add_slider_row(row: Dictionary, key: String, title: String) -> void:
	var value := int(row.get("value", 0))
	var slider := MindWidgets.slider(
		float(row.get("min", 0)), float(row.get("max", 100)), float(row.get("step", 1))
	)
	slider.value = value
	slider.custom_minimum_size.x = 420.0
	slider.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var value_label := MindWidgets.styled_label(_format_value(row, value), "outlineLabel")

	# code-instantiated: the title/value header is data-driven per row.
	var header := HBoxContainer.new()
	header.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var title_label := MindWidgets.label(title)
	# Upstream `content.add(title, Styles.outlineLabel).left().growX().wrap()`.
	title_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	title_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	header.add_child(title_label)
	header.add_child(value_label)

	slider.value_changed.connect(
		func(new_value: float):
			var number := int(new_value)
			value_label.text = _format_value(row, number)
			_set_value(key, number)
	)
	_table_host.add(header).grow_x_axis().set_pad_top(4.0)
	_table_host.row()
	_table_host.add(slider).grow_x_axis().set_min_height(30.0)
	_table_host.row()


func _add_reset_button() -> void:
	var button := MindWidgets.button(_t("@settings.reset"))
	button.custom_minimum_size.x = 240.0
	button.pressed.connect(
		func():
			var ui := _ui()
			if ui != null:
				ui.call("settings_reset", _selected)
			_show_table(_selected)
	)
	_table_host.add(button).pad(14.0).set_min_width(240.0)
	_table_host.row()


func _show_data() -> void:
	_selected = "data"
	if _table_host == null:
		return
	_table_host.clear_children()
	# code-instantiated: data actions are a data-driven list with no static scene.
	for action in DATA_ACTIONS:
		var button := MindWidgets.icon_button(
			str(action.icon), _t(str(action.text)), "flatt"
		)
		button.custom_minimum_size.x = 280.0
		button.pressed.connect(_run_data_action.bind(str(action.id)))
		_table_host.add(button).set_min_height(60.0).pad(4.0)
		_table_host.row()
	_resort_table()


func _run_data_action(action_id: String) -> void:
	var ui := _ui()
	if ui == null:
		return
	if action_id == "clear-saves":
		# Destructive: confirm first, upstream `settings.clearsaves.confirm`.
		_pending_action = action_id
		ui.call("show_confirm", _t("@settings.clearsaves.confirm"))
		return
	ui.call("settings_action", action_id)


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
	if ui != null:
		ui.call("settings_action", action_id)


func _set_value(key: String, value: Variant) -> void:
	var ui := _ui()
	if ui == null:
		return
	ui.call("settings_set", key, value)
	if _selected == "sound":
		# The audio mixer keeps its own live settings mirror; drive it directly.
		var audio := get_node_or_null("/root/MindAudio")
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
			return "%.1fx" % (value / 4.0)
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


func _resort_table() -> void:
	_table_host.sort_now()
	call_deferred("_deferred_resort")


func _deferred_resort() -> void:
	if _table_host != null:
		_table_host.sort_now()


func _ui() -> Node:
	return get_node_or_null("/root/MindUi")
