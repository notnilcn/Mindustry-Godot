## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/LoadoutDialog.java (plan 14 M5).
##
## Core loadout capacity editor: per-item `-`/`+`/pencil stepping bounded by the
## launch capacity (`LoadoutDialog.setup`/`maxItems`), with `@max` and
## `@settings.reset`. The item amounts are seeded from the live
## `MindCampaign.get_launch_resources()` and written back through
## `set_launch_resources()` (WS2 contract); without the facade the editor still
## renders the loadout requirements read-only-editable.

extends MindDialog

## Emitted when the active loadout changes.
signal loadout_changed(index: int)

var _summary: MindTable = null
var _capacity := 0
var _index := -1
var _amounts: Dictionary = {}
var _seed_amounts: Dictionary = {}
var _pending_edit := ""


func _ready() -> void:
	set_title_key("@configure")
	should_pause = false
	full_dialog = true
	super._ready()
	_build()
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_signal("text_input_result") and not ui.is_connected("text_input_result", _on_text_input):
		ui.connect("text_input_result", _on_text_input)


func _build() -> void:
	_summary = content_table()


func shown() -> void:
	refresh_campaign_views(str(_context.get("planet", "")))
	_capacity = int(_context.get("capacity", campaign_views().get("launch_capacity", 0)))
	_index = int(_context.get("index", -1))
	_seed()
	_rebuild()


## Seeds the editor from the live launch resources, unioned with every loadout
## requirement item so all items can be stepped at least once.
func _seed() -> void:
	_seed_amounts = {}
	var live := campaign_json("get_launch_resources")
	if not live.is_empty():
		for item in live:
			_seed_amounts[str(item)] = int(live[item])
	else:
		for loadout_variant in campaign_section("loadouts"):
			var loadout: Dictionary = loadout_variant
			if _index >= 0 and int(loadout.get("index", -1)) != _index:
				continue
			for requirement in loadout.get("requirements", []):
				_seed_amounts[str(requirement[0])] = int(requirement[1])
	for loadout_variant in campaign_section("loadouts"):
		var seed_row: Dictionary = loadout_variant
		for requirement in seed_row.get("requirements", []):
			var item := str(requirement[0])
			if not _seed_amounts.has(item):
				_seed_amounts[item] = 0
	_amounts = _seed_amounts.duplicate()


func _rebuild() -> void:
	if _summary == null:
		return
	_summary.clear_children()
	if _capacity > 0:
		_summary.add(MindWidgets.label(MindWidgets.markup_format("@launch.capacity", [_capacity]))).grow_x_axis().pad(4)
		_summary.row()
	var keys := _amounts.keys()
	keys.sort()
	var pending: HBoxContainer = null
	for index in keys.size():
		if index % 2 == 0:
			pending = HBoxContainer.new()
			pending.add_theme_constant_override("separation", 6)
			_summary.add(pending).grow_x_axis().pad(2)
			_summary.row()
		pending.add_child(_item_row(str(keys[index])))
	clear_buttons()
	add_button(_t("@max"), _max_items, "add", 200.0)
	add_button(_t("@settings.reset"), _reset, "refresh", 200.0)
	add_close_button()


func _item_row(item: String) -> Control:
	# code-instantiated: item rows are the live launch-resource ItemSeq.
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 2)
	var minus := MindWidgets.button("-")
	minus.custom_minimum_size = Vector2(36.0, 36.0)
	minus.pressed.connect(_step.bind(item, -1))
	row.add_child(minus)
	var plus := MindWidgets.button("+")
	plus.custom_minimum_size = Vector2(36.0, 36.0)
	plus.pressed.connect(_step.bind(item, 1))
	row.add_child(plus)
	var pencil := MindWidgets.image_button("pencil", "flati")
	pencil.custom_minimum_size = Vector2(36.0, 36.0)
	pencil.pressed.connect(_edit.bind(item))
	row.add_child(pencil)
	var icon := MindWidgets.image(item)
	icon.custom_minimum_size = Vector2(28.0, 28.0)
	icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(icon)
	row.add_child(MindWidgets.label("%s  %d" % [_localized_item(item), int(_amounts.get(item, 0))]))
	return row


## Bundle label for an item (`item.<name>.name`), falling back to the content id.
func _localized_item(item: String) -> String:
	var assets := MindWidgets.assets()
	if assets == null:
		return item
	var key := "item.%s.name" % item
	var value := str(assets.call("bundle_get", key))
	return value if not value.is_empty() and value != key else item


## `LoadoutDialog.step`: 100/200/500/1000 depending on the current amount.
func _step_size(amount: int) -> int:
	if amount < 1000:
		return 100
	if amount < 2000:
		return 200
	if amount < 5000:
		return 500
	return 1000


func _step(item: String, direction: int) -> void:
	var amount := int(_amounts.get(item, 0))
	amount = maxi(amount + direction * _step_size(amount), 0)
	if _capacity > 0:
		amount = mini(amount, _capacity)
	_amounts[item] = amount
	_apply()
	_rebuild()


## `LoadoutDialog.maxItems`: every stack to the launch capacity.
func _max_items() -> void:
	if _capacity <= 0:
		return
	for item in _amounts:
		_amounts[item] = _capacity
	_apply()
	_rebuild()


## `LoadoutDialog.@settings.reset`: re-seed from the live state.
func _reset() -> void:
	_seed()
	_apply()
	_rebuild()


## Inline amount entry through the shared `showTextInput` prompt pipeline.
func _edit(item: String) -> void:
	_pending_edit = item
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_text_input"):
		ui.call(
			"show_text_input",
			_t("@configure"),
			MindWidgets.markup_format("@configure.invalid", [_capacity if _capacity > 0 else 9999]),
			10,
			str(int(_amounts.get(item, 0))),
			true,
			true
		)


func _on_text_input(text: String) -> void:
	if _pending_edit.is_empty() or not text.is_valid_int():
		_pending_edit = ""
		return
	var amount := maxi(int(text), 0)
	if _capacity > 0:
		amount = mini(amount, _capacity)
	_amounts[_pending_edit] = amount
	_pending_edit = ""
	_apply()
	_rebuild()


## Persists the edited amounts through the campaign facade and emits the change.
func _apply() -> void:
	var campaign := campaign_node()
	if campaign != null and campaign.has_method("set_launch_resources"):
		campaign.call("set_launch_resources", _amounts)
	if _index >= 0:
		loadout_changed.emit(_index)
