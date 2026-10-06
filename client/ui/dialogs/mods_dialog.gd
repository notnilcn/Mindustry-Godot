## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/ModsDialog.java (plan 20 §3.9, M6).
##
## Loaded-mod list: reload banner, import (file / GitHub), browser, search, rows
## with enable/disable/delete and an expandable details block, plus update rows.
## All mod state comes from the plan-20 `MindMods` autoload (JSON + signals); this
## script is UI-only and never mutates sim/content state. The GitHub HTTP path
## lives in `mod_browser_dialog.gd` (documented seam), mirroring upstream's
## `ui.mods.githubImportMod`. Version matching stays in `mind-core::mods::listing`.

extends MindDialog

## Ported from `Vars.modGuideURL` (the guide/open-folder buttons).
const MOD_GUIDE_URL := "https://mindustrygame.github.io/wiki/modding/1-modding/"

var _list: MindTable = null
var _search: LineEdit = null
var _banner: MindLabel = null
var _errors_button: Button = null
var _progress: ProgressBar = null
## internal name -> remote version from the browser's `ModListing`s (updates).
var _updates: Dictionary = {}
## internal name whose details block is expanded (`showMod`).
var _open_details := ""
## pending async-confirm target (`mods.removeMod`).
var _pending_delete := ""


func _ready() -> void:
	set_title_key("@mods")
	should_pause = false
	super._ready()
	_build()
	add_close_button()
	_connect_mods()


func shown() -> void:
	_refresh()


## Called by `mod_browser_dialog.gd` after it parses the repository listings
## (upstream `withUpdates`); `updates` maps internal name -> latest version.
func set_updates(updates: Dictionary) -> void:
	_updates = updates
	_refresh()


func _build() -> void:
	var root := content_table()
	root.add_theme_constant_override("separation", 6)

	# `@mod.reloadrequired` banner (upstream `cont.add(...).visible(mods::requiresReload)`).
	_banner = MindWidgets.styled_label(_t("@mod.reloadrequired"), "techLabel")
	_banner.modulate = MindStyles.NEGSTAT
	_banner.visible = false
	root.add(_banner).grow_x_axis().pad(4)
	root.row()

	# Import/browser actions (upstream `@mod.import` sub-dialog flattened).
	# code-instantiated: the action row is the fixed import/open action set.
	var actions := HBoxContainer.new()
	actions.alignment = BoxContainer.ALIGNMENT_CENTER
	_add_action_button(actions, "@mod.import.file", _import_file)
	_add_action_button(actions, "@mod.import.github", _open_browser)
	_add_action_button(actions, "@mods.browser", _open_browser)
	_add_action_button(actions, "@mods.openfolder", _open_folder)
	_add_action_button(actions, "@mods.guide", _open_guide)
	root.add(actions).grow_x_axis().pad(4)
	root.row()

	# `@mod.errors`: content errors / unsupported scripts (OD1) summary button.
	# code-instantiated: only present when `MindMods.errors()` is non-empty.
	_errors_button = MindWidgets.button(_t("@mod.errors"))
	_errors_button.visible = false
	_errors_button.pressed.connect(_show_errors)
	root.add(_errors_button).grow_x_axis().pad(2)
	root.row()

	# `mod_import_progress` bar (browser downloads report through `MindMods`).
	# code-instantiated: transient progress widget hidden unless an import runs.
	_progress = ProgressBar.new()
	_progress.min_value = 0.0
	_progress.max_value = 1.0
	_progress.visible = false
	root.add(_progress).grow_x_axis().pad(2)
	root.row()

	_search = MindWidgets.field(_t("@search"))
	_search.text_changed.connect(func(_value: String) -> void: _refresh())
	root.add(_search).grow_x_axis().pad(4)
	root.row()

	_list = MindTable.new()
	_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_list.size_flags_vertical = Control.SIZE_EXPAND_FILL
	root.add(_list).grow().pad(4)


func _refresh() -> void:
	if _list == null:
		return
	var mods := _mods()
	_banner.visible = mods != null and bool(mods.call("requires_reload"))
	var errors: Array = mods.call("errors") if mods != null else []
	if _errors_button != null:
		_errors_button.visible = not errors.is_empty()
	_list.clear_children()

	if mods == null:
		_list.add(MindWidgets.label(MindWidgets.markup("@mods.none"))).pad(8)
		return
	var entries: Array = mods.call("list")
	if entries.is_empty():
		_list.add(MindWidgets.label(MindWidgets.markup("@mods.none"))).pad(8)
		return

	var query := _search.text.to_lower() if _search != null else ""
	var shown := 0
	for entry in entries:
		if not (entry is Dictionary):
			continue
		var row: Dictionary = entry
		var display := str(row.get("display_name", row.get("displayName", row.get("name", ""))))
		if not query.is_empty() and not display.to_lower().contains(query):
			continue
		_add_mod_row(row)
		shown += 1

	if shown == 0:
		_list.add(MindWidgets.label(MindWidgets.markup("@none.found"))).pad(8)
	if not _updates.is_empty():
		_list.row()
		# code-instantiated: batch action only when the browser reported updates.
		var update_all := MindWidgets.button(_t("@mods.update.all"))
		update_all.pressed.connect(_update_all)
		_list.add(update_all).grow_x_axis().pad(6)


## `getStateText`: the colored state string shown under a mod's name.
func _state_text(entry: Dictionary) -> String:
	match str(entry.get("state", "")):
		"incompatible", "unsupported":
			return _t("@mod.blacklisted")
		"circularDependencies":
			return _t("@mod.circulardependencies")
		"incompleteDependencies":
			return _t("@mod.incompletedependencies")
		"missingDependencies":
			return _t("@mod.unmetdependencies")
		"contentErrors":
			return _t("@mod.erroredcontent")
	return _t("@mod.multiplayer.compatible") if bool(entry.get("hidden", false)) else ""


## `getStateDetails`: the tooltip/details body for a state.
func _state_details(entry: Dictionary) -> String:
	var missing := ", ".join(_string_list(entry.get("missing_dependencies", [])))
	match str(entry.get("state", "")):
		"incompatible", "unsupported":
			var reason := str(entry.get("reason", ""))
			return reason if not reason.is_empty() else _t("@mod.blacklisted.details")
		"circularDependencies":
			return _t("@mod.circulardependencies.details")
		"incompleteDependencies":
			return _fmt("@mod.incompletedependencies.details", [missing])
		"missingDependencies":
			return _fmt("@mod.missingdependencies.details", [missing])
		"contentErrors":
			return _t("@mod.erroredcontent.details")
	return ""


## Short description or the disabled/failed fallback (upstream `title1.table`).
func _state_summary(entry: Dictionary, enabled: bool) -> String:
	if not enabled:
		return _t("@mod.disabled")
	var description := str(entry.get("short_description", ""))
	if not description.is_empty():
		return description
	return _state_text(entry)


func _add_mod_row(entry: Dictionary) -> void:
	var name := str(entry.get("name", ""))
	var enabled := bool(entry.get("enabled", false))
	var supported := str(entry.get("state", "")) != "unsupported"

	# code-instantiated: mod rows are data-driven from the plan-20 `MindMods.list()`.
	var box := HBoxContainer.new()
	box.size_flags_horizontal = Control.SIZE_EXPAND_FILL

	# Mod icon is loaded by `mind-gdext` (plan 20 M5 `Mods::load_icon`); the
	# region is provided once the icon applier lands, so the slot is empty-safe.
	var icon := MindWidgets.image("mod-icon")
	icon.custom_minimum_size = Vector2(48, 48)
	box.add_child(icon)

	var title := Button.new()
	title.theme_type_variation = "flatBordert"
	title.alignment = HORIZONTAL_ALIGNMENT_LEFT
	title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	title.text = str(entry.get("display_name", entry.get("displayName", name)))
	title.tooltip_text = _state_details(entry)
	title.pressed.connect(_toggle_details.bind(name))
	box.add_child(title)

	var summary := MindWidgets.label(_state_summary(entry, enabled))
	summary.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	box.add_child(summary)

	if _updates.has(name):
		# code-instantiated: update row from the `withUpdates` set (browser listing).
		var update := MindWidgets.button(_fmt("@mods.update.available", [_updates[name]]))
		update.pressed.connect(_update_mod.bind(name))
		box.add_child(update)

	var toggle := Button.new()
	toggle.theme_type_variation = "flatTogglet"
	toggle.toggle_mode = true
	toggle.button_pressed = enabled
	toggle.text = _t("@mod.enabled") if enabled else _t("@mod.disabled")
	toggle.disabled = not supported
	toggle.pressed.connect(_set_enabled.bind(name, not enabled))
	box.add_child(toggle)

	var remove := MindWidgets.button(_t("@delete"))
	remove.pressed.connect(_confirm_delete.bind(name))
	box.add_child(remove)

	_list.add(box).grow_x_axis().pad(3)
	_list.row()

	if _open_details == name:
		_add_details(name)


## `showMod`: the expanded details block (upstream opens a BaseDialog).
func _add_details(name: String) -> void:
	var mods := _mods()
	var details: Dictionary = mods.call("details", name) if mods != null else {}
	# code-instantiated: details are data-driven from `MindMods.details(name)`.
	var box := VBoxContainer.new()
	box.size_flags_horizontal = Control.SIZE_EXPAND_FILL

	var author := str(details.get("author", ""))
	if not author.is_empty():
		box.add_child(MindWidgets.label("%s %s" % [_t("@editor.author"), author]))
	var version := str(details.get("version", ""))
	if not version.is_empty():
		box.add_child(MindWidgets.label("%s %s" % [_t("@mod.version"), version]))
	var description := str(details.get("description", ""))
	if not description.is_empty():
		var body := MindWidgets.label(description)
		body.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		box.add_child(body)
	var state_details := _state_details(details)
	if not state_details.is_empty():
		box.add_child(MindWidgets.label(state_details))

	# Dependency dialog (`@mod.dependencies.downloadall`) delegates to the browser.
	var missing := _string_list(details.get("missing_dependencies", []))
	if not missing.is_empty():
		# code-instantiated: dependency import row only when deps are unmet.
		var dependency := MindWidgets.button(_t("@mod.dependencies.downloadall"))
		dependency.pressed.connect(_download_dependencies.bind(missing))
		box.add_child(dependency)

	var actions := HBoxContainer.new()
	var folder := MindWidgets.button(_t("@mods.openfolder"))
	folder.pressed.connect(_open_path.bind(str(details.get("config_folder", ""))))
	actions.add_child(folder)
	var repo := str(details.get("repo", ""))
	if not repo.is_empty():
		var open_repo := MindWidgets.button(_t("@mods.github.open"))
		open_repo.pressed.connect(_open_url.bind("https://github.com/%s" % repo))
		actions.add_child(open_repo)
	box.add_child(actions)

	_list.add(box).grow_x_axis().pad(6)
	_list.row()


func _toggle_details(name: String) -> void:
	_open_details = "" if _open_details == name else name
	_refresh()


func _set_enabled(name: String, enabled: bool) -> void:
	var mods := _mods()
	if mods != null:
		mods.call("set_enabled", name, enabled)
	_refresh()


func _confirm_delete(name: String) -> void:
	_pending_delete = name
	var ui := _ui()
	if ui != null:
		ui.call("show_confirm", _t("@mod.remove.confirm"))
	else:
		_on_confirm(true)


func _on_confirm(confirmed: bool) -> void:
	if _pending_delete.is_empty():
		return
	var name := _pending_delete
	_pending_delete = ""
	if not confirmed:
		return
	var mods := _mods()
	if mods == null:
		return
	var result: Dictionary = mods.call("remove_mod", name)
	if not bool(result.get("ok", false)):
		_show_error(str(result.get("error", _t("@mod.delete.error"))))


## Native file chooser (`FileChooser.open("zip", "jar")`; fallback is the plan-14
## `file_chooser` dialog owned by plan 22's native-first path).
func _import_file() -> void:
	var mods := _mods()
	var start := str(mods.call("mods_dir")) if mods != null else "user://mods"
	DisplayServer.file_dialog_show(
		_t("@mod.import.file"), start, "",
		false, DisplayServer.FILE_DIALOG_MODE_OPEN_FILES,
		PackedStringArray(["*.zip", "*.jar"]), _on_files_chosen
	)


func _on_files_chosen(status: bool, paths: PackedStringArray, _filter_index: int) -> void:
	if not status:
		return
	for path in paths:
		_import_path(path)


func _import_path(path: String) -> void:
	var mods := _mods()
	if mods == null:
		return
	var result: Dictionary = mods.call("import_mod", path)
	if not bool(result.get("ok", false)):
		_show_error(str(result.get("error", _t("@mod.delete.error"))))
	_refresh()


## GitHub import is delegated to the browser (all HTTP lives there).
func _open_browser() -> void:
	var ui := _ui()
	if ui != null:
		ui.call("open_dialog", "mod_browser", "{}")


func _update_mod(name: String) -> void:
	var mods := _mods()
	var repo := ""
	if mods != null:
		var details: Dictionary = mods.call("details", name)
		repo = str(details.get("repo", ""))
	if repo.is_empty():
		return
	var ui := _ui()
	if ui != null:
		ui.call("open_dialog", "mod_browser", JSON.stringify({"repo": repo}))


func _update_all() -> void:
	var repos: Array = []
	var mods := _mods()
	for name in _updates.keys():
		if mods == null:
			break
		var details: Dictionary = mods.call("details", str(name))
		var repo := str(details.get("repo", ""))
		if not repo.is_empty():
			repos.append(repo)
	var browser := get_node_or_null("/root/Spine/Ui/UiRoot/DialogLayer/mod_browser")
	if browser != null and browser.has_method("install_repos"):
		browser.call("install_repos", repos)
	else:
		_open_browser()


func _download_dependencies(dependencies: PackedStringArray) -> void:
	var names: Array = []
	for name in dependencies:
		names.append(str(name))
	var browser := get_node_or_null("/root/Spine/Ui/UiRoot/DialogLayer/mod_browser")
	_open_browser()
	if browser != null and browser.has_method("install_repos"):
		browser.call("install_repos", names)


func _open_folder() -> void:
	var mods := _mods()
	if mods != null:
		_open_path(str(mods.call("mods_dir")))


func _open_guide() -> void:
	_open_url(MOD_GUIDE_URL)


func _open_path(path: String) -> void:
	if not path.is_empty():
		OS.shell_open(path)


func _open_url(url: String) -> void:
	OS.shell_open(url)


func _show_errors() -> void:
	var mods := _mods()
	if mods == null:
		return
	var lines := PackedStringArray()
	for entry in mods.call("errors"):
		if entry is Dictionary:
			lines.append("%s: %s" % [str(entry.get("name", "")), str(entry.get("details", ""))])
	_show_error("\n".join(lines))


func _show_error(text: String) -> void:
	var ui := _ui()
	if ui == null:
		return
	if ui.has_method("show_text"):
		ui.call("show_text", _t("@mods"), text)
	else:
		ui.call("show_info", text)


func _on_mod_error(name: String, details: String) -> void:
	_show_error("%s: %s" % [name, details])


func _on_import_progress(_path: String, ratio: float) -> void:
	if _progress == null:
		return
	_progress.visible = ratio < 1.0
	_progress.value = ratio


func _add_action_button(container: Container, text: String, callback: Callable) -> void:
	var button := MindWidgets.button(_t(text))
	button.pressed.connect(callback)
	container.add_child(button)


func _mods() -> Node:
	return get_node_or_null("/root/MindMods")


func _ui() -> Node:
	return get_node_or_null("/root/MindUi")


## Bundle lookup with `{0}`-style substitution (`Bundle.format`).
func _fmt(key: String, args: Array) -> String:
	var result := _t(key)
	for index in args.size():
		result = result.replace("{%d}" % index, str(args[index]))
	return result


func _string_list(value: Variant) -> PackedStringArray:
	var result := PackedStringArray()
	if value is Array:
		for item in value:
			result.append(str(item))
	return result


func _connect_mods() -> void:
	var mods := _mods()
	if mods != null:
		if mods.has_signal("mods_changed") and not mods.is_connected("mods_changed", _refresh):
			mods.connect("mods_changed", _refresh)
		if mods.has_signal("mod_error") and not mods.is_connected("mod_error", Callable(self, "_on_mod_error")):
			mods.connect("mod_error", Callable(self, "_on_mod_error"))
		if mods.has_signal("mod_import_progress") and not mods.is_connected("mod_import_progress", Callable(self, "_on_import_progress")):
			mods.connect("mod_import_progress", Callable(self, "_on_import_progress"))
	var ui := _ui()
	if ui != null and ui.has_signal("confirm_result") and not ui.is_connected("confirm_result", Callable(self, "_on_confirm")):
		ui.connect("confirm_result", Callable(self, "_on_confirm"))
