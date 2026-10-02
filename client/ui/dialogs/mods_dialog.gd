## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/ModsDialog.java (plan 14 §3.4, M3).
##
## Loaded-mod list with enable/disable toggles, error rows and an import button.
## The mod data comes from the plan-20 `MindMods` autoload; zip/HTTP/import
## progress is plan 20's service surface.

extends MindDialog

var _table: MindTable = null


func _ready() -> void:
	set_title_key("@mods")
	should_pause = false
	super._ready()
	_table = content_table()
	add_close_button()
	buttons.add_child(_make_button("@mods.import", _import))
	var mods := get_node_or_null("/root/MindMods")
	if mods != null and mods.has_signal("mods_changed") and not mods.is_connected("mods_changed", _rebuild):
		mods.connect("mods_changed", _rebuild)


func shown() -> void:
	_rebuild()


func _rebuild() -> void:
	if _table == null:
		return
	_table.clear_children()
	var mods := get_node_or_null("/root/MindMods")
	var entries: Array = []
	if mods != null:
		entries = mods.call("list")
	if entries.is_empty():
		_table.add(MindWidgets.label(_t("@mods.none"))).pad(8)
		return
	for entry in entries:
		# code-instantiated: mod rows are data-driven from the plan-20 mod list.
		var row := HBoxContainer.new()
		var info := ModEntry.from(entry)
		row.add_child(MindWidgets.label("%s [gray]%s" % [info.display_name, info.version]))
		var toggle := Button.new()
		toggle.text = _t("@enabled") if info.enabled else _t("@disabled")
		toggle.theme_type_variation = "flatTogglet"
		toggle.toggle_mode = true
		toggle.button_pressed = info.enabled
		toggle.pressed.connect(_set_enabled.bind(info.internal_name, not info.enabled))
		row.add_child(toggle)
		_table.add(row).grow_x_axis().pad(4)
		_table.row()


func _set_enabled(mod_name: String, enabled: bool) -> void:
	var mods := get_node_or_null("/root/MindMods")
	if mods != null:
		mods.call("set_enabled", mod_name, enabled)
	_rebuild()


func _import() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("show_info", _t("@mods.importfail"))


func _make_button(text: String, callback: Callable) -> Button:
	var button := MindWidgets.button(_t(text))
	button.pressed.connect(callback)
	return button


## Typed view over the plan-20 mod list dictionary (`ModListEntry` serialization).
class ModEntry extends RefCounted:
	var internal_name := ""
	var display_name := ""
	var version := ""
	var enabled := false

	static func from(entry: Variant) -> ModEntry:
		var result := ModEntry.new()
		if entry is Dictionary:
			result.internal_name = str(entry.get("name", ""))
			result.display_name = str(entry.get("display_name", entry.get("displayName", result.internal_name)))
			result.version = str(entry.get("version", ""))
			result.enabled = bool(entry.get("enabled", false))
		return result
