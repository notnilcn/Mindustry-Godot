## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/LaunchLoadoutDialog.java (plan 14 M5).
##
## Launch-loadout picker: lists the core's valid loadouts from the live
## `MindCampaign` read models (`Schematics.getLoadouts(core)`) and lets the
## player pick one for the next launch. The plan data (capacity, staged
## resources, max recomputation, validity) comes from
## `MindCampaign.launch_dialog_json`, a port of `LaunchLoadoutDialog.update`;
## the capacity grid/item stepping is `LoadoutDialog` (opened through the
## `@resources` button). The confirm button emits `loadout_chosen(index)` which
## the planet dialog forwards to the launcher branch of `_launch`.

extends MindDialog

## Emitted with the chosen schematic index (`Schematics.all` index).
signal loadout_chosen(index: int)

var _list: MindTable = null
var _selected := -1
var _plan: Dictionary = {}


func _ready() -> void:
	set_title_key("@configure")
	should_pause = false
	super._ready()
	_build()


func shown() -> void:
	refresh_campaign_views(str(_context.get("planet", "")))
	_rebuild()


func _build() -> void:
	_list = content_table()


## Loadout rows: the plan's launcher-core-compatible list (Java
## `schematics.getLoadouts()` filtered by `entry.key.size <= core.size`), with
## the planet-default rows as the pre-bootstrap fallback.
func _loadout_rows() -> Array:
	var rows: Array = _plan.get("loadouts", [])
	if rows.is_empty():
		return campaign_section("loadouts")
	return rows


## The default core for the selected planet (`PlanetDef.default_core`), read from
## the live loadout rows instead of the planet name (`_selected_core` bug).
func _selected_core() -> String:
	for loadout_variant in _loadout_rows():
		var loadout: Dictionary = loadout_variant
		var core := str(loadout.get("core", ""))
		if not core.is_empty():
			return core
	return ""


## `MindCampaign.launch_dialog_json` (`LaunchLoadoutDialog.update`); `{}` when
## the facade is absent.
func _launch_plan() -> Dictionary:
	var campaign := campaign_node()
	if campaign == null or not campaign.has_method("launch_dialog_json"):
		return {}
	var parsed: Variant = JSON.parse_string(str(campaign.call(
		"launch_dialog_json",
		str(_context.get("planet", "")),
		int(_context.get("launcher", -1)),
		int(_context.get("sector", -1)),
		_selected
	)))
	return parsed if parsed is Dictionary else {}


## The launcher's localized name (`Sector.name()` + the planet dialog's
## `sector.<preset>.name` rule).
func _localized_from() -> String:
	var name := str(_plan.get("from", ""))
	var preset := str(_plan.get("from_preset", ""))
	if not preset.is_empty():
		var key := "sector.%s.name" % preset
		var localized := _t("@%s" % key)
		if not localized.is_empty() and localized != key:
			return localized
	return name


func _rebuild() -> void:
	_list.clear_children()
	_plan = _launch_plan()
	# Select a row the launcher's plan offers (default row preferred); the plan
	# carries the requirements/capacity for the *selected* schematic, so a
	# selection change rebuilds it.
	var offered: Array = []
	for loadout_variant in _loadout_rows():
		offered.append(int((loadout_variant as Dictionary).get("index", -1)))
	if not offered.has(_selected):
		# The launcher-core default comes from the plan (`LauncherLoadout`'s
		# `universe.getLoadout(core)` fallback); several cores' rows carry
		# `is_default`, so the server's index is authoritative.
		_selected = int(_plan.get("selected", -1))
		if not offered.has(_selected) and not offered.is_empty():
			_selected = int(offered[0])
		_plan = _launch_plan()
	var loadouts := _loadout_rows()
	# `launch.from`: the launcher sector's name (`LaunchLoadoutDialog.java:165`).
	if not _plan.is_empty():
		_list.add(MindWidgets.label(MindWidgets.markup_format("@launch.from", [_localized_from()]))) \
			.grow_x_axis().pad(4)
		_list.row()
	if loadouts.is_empty():
		_list.add(MindWidgets.label(_t("@none"))).pad(8)
		_list.row()
	else:
		for index in loadouts.size():
			var loadout: Dictionary = loadouts[index]
			# code-instantiated: loadout rows come from plan-12 Schematics.getLoadouts.
			var row := HBoxContainer.new()
			row.add_theme_constant_override("separation", 6)
			var button := MindWidgets.button(str(loadout.get("name", "")))
			button.toggle_mode = true
			button.button_pressed = int(loadout.get("index", -1)) == _selected
			button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			button.pressed.connect(_select.bind(int(loadout.get("index", -1))))
			row.add_child(button)
			_list.add(row).grow_x_axis().pad(2)
			_list.row()
	_rebuild_items()
	clear_buttons()
	# `LaunchLoadoutDialog.java:41`: `buttons.defaults().size(160f, 64f)`.
	# Java order: `@back`, (`@resources.max` toggle), `@resources`, `@launch.text`.
	add_button(_t("@back"), hide_dialog, "left", 160.0)
	var allow := bool(_plan.get("allow_launch_loadout", true))
	if allow:
		# `@resources.max`: toggles `maxresources` (`getMax` default true).
		var max_button := add_button(_t("@resources.max"), _toggle_max, "add", 160.0)
		max_button.toggle_mode = true
		max_button.button_pressed = bool(_plan.get("max", true))
		var resources := add_button(_t("@resources"), _open_loadout, "edit", 160.0)
		resources.disabled = bool(_plan.get("max", true))
	var launch := add_button(_t("@launch.text"), _confirm, "ok", 160.0)
	launch.disabled = not _plan.is_empty() and not bool(_plan.get("valid", false))


## Capacity label + item rows (`LaunchLoadoutDialog.rebuild`,
## `LaunchLoadoutDialog.java:92-117,167`). Four items per row, Java text:
## `(al + as) + "[gray] (" + al + " + " + as + ")"`, scarlet `have/needed`
## when short.
func _rebuild_items() -> void:
	if _plan.is_empty():
		var resource_totals := _launch_resource_stacks()
		if not resource_totals.is_empty():
			var parts := PackedStringArray()
			for item in resource_totals:
				parts.append("%s x%d" % [str(item), int(resource_totals[item])])
			_list.add(MindWidgets.label("%s %s" % [_t("@resources"), ", ".join(parts)])).grow_x_axis().pad(2)
			_list.row()
		return
	var capacity := int(_plan.get("capacity", 0))
	if capacity > 0:
		_list.add(MindWidgets.label(MindWidgets.markup_format("@launch.capacity", [capacity]))) \
			.grow_x_axis().pad(4)
		_list.row()
	var allow := bool(_plan.get("allow_launch_loadout", false))
	var total: Dictionary = _plan.get("total", {})
	var requirements: Dictionary = _plan.get("requirements", {})
	var resources: Dictionary = _plan.get("resources", {})
	var stored: Dictionary = _plan.get("stored", {})
	var count := 0
	# code-instantiated: item rows are the item-id-ordered `total` map.
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 4)
	for item in total:
		var amount := int(total[item])
		if amount == 0:
			continue
		row.add_child(MindWidgets.image("item-%s-ui" % str(item)))
		var as_amount := int(requirements.get(item, 0))
		var al_amount := int(resources.get(item, 0))
		var amount_text := str(amount)
		if allow:
			amount_text += "[gray] (%d + %d)" % [al_amount, as_amount]
		if int(stored.get(item, 0)) < amount:
			amount_text = "[scarlet]%d[lightgray]/%s" % [mini(int(stored.get(item, 0)), amount), amount_text]
		row.add_child(MindWidgets.label(MindWidgets.markup(amount_text)))
		count += 1
		if count % 4 == 0:
			_list.add(row).grow_x_axis().pad(2)
			_list.row()
			# code-instantiated: grid row continuation for the item list.
			row = HBoxContainer.new()
			row.add_theme_constant_override("separation", 4)
	if count % 4 != 0:
		_list.add(row).grow_x_axis().pad(2)
		_list.row()
	if not bool(_plan.get("valid", false)):
		_list.add(MindWidgets.label(MindWidgets.markup("@sector.missingresources"))).grow_x_axis().pad(4)
		_list.row()


func _select(index: int) -> void:
	_selected = index
	_rebuild()


## `setMax`: writes the `maxresources` flag through the live settings store.
func _toggle_max() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("settings_put_flag"):
		ui.call("settings_put_flag", "maxresources", not bool(_plan.get("max", true)))
	_plan = {}
	_rebuild()


## Live `universe.getLaunchResources` as `{item: amount}` (`{}` without the
## facade or when nothing is staged).
func _launch_resource_stacks() -> Dictionary:
	var campaign := campaign_node()
	if campaign == null or not campaign.has_method("get_launch_resource_stacks"):
		return {}
	var parsed: Variant = JSON.parse_string(str(campaign.call("get_launch_resource_stacks")))
	return parsed if parsed is Dictionary else {}


## The selected loadout row, or `{}`.
func _find_selected_row() -> Dictionary:
	for loadout_variant in _loadout_rows():
		var loadout: Dictionary = loadout_variant
		if int(loadout.get("index", -1)) == _selected:
			return loadout
	return {}


## `LaunchLoadoutDialog.@launch.text`: `universe.updateLoadout(core, selected);
## confirm.run(); hide()` (`LaunchLoadoutDialog.java:150-152`).
func _confirm() -> void:
	if _selected < 0:
		return
	# `Universe.updateLoadout(core, selected)` (`LaunchLoadoutDialog.java:150-152`):
	# record the chosen schematic's file for the core before the planet dialog
	# reads it back for the launch (PAR-0085). A file-less schematic records the
	# empty string (Java `schem.file == null ? ""`).
	var row := _find_selected_row()
	var campaign := campaign_node()
	if campaign != null and campaign.has_method("update_loadout"):
		var file_value: Variant = row.get("file", "")
		var file_name := "" if file_value == null else str(file_value)
		campaign.call("update_loadout", str(row.get("core", "")), file_name)
	loadout_chosen.emit(_selected)


## `LaunchLoadoutDialog.@resources`: opens the capacity editor seeded with the
## live launch resources.
func _open_loadout() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	ui.call("open_dialog", "loadout", JSON.stringify({
		"planet": str(_context.get("planet", "")),
		"sector": int(_context.get("sector", -1)),
		"capacity": int(_plan.get("capacity", 0)),
		"index": _selected,
		"core": _selected_core(),
	}))
