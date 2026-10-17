## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/MapEditorDialog.java (the `menu` dialog,
## `:65-270`).
##
## UI-only: the editor menu grid (save / map info / generate / resize / import /
## export / play in game / playtest / sector generate / quit). Every action
## forwards to `/root/Spine/MindEditor` or opens another editor dialog; the
## shell owns Escape and the unsaved-changes confirm.

extends MindDialog

## `MapEditorDialog`: `swidth = 180f`, buttons `(swidth, 60)`; wide rows
## `swidth * 2 + 10`.
const BUTTON_SIZE := Vector2(180.0, 60.0)
const WIDE_SIZE := Vector2(370.0, 60.0)

## `MapEditorDialog`: menu rows in order (Steam's workshop button is skipped);
## `wide` entries span both columns.
const ENTRIES := [
	{"key": "@editor.savemap", "icon": "save", "action": "save"},
	{"key": "@editor.mapinfo", "icon": "pencil", "action": "map_info"},
	{"key": "@editor.generate", "icon": "terrain", "action": "map_generate"},
	{"key": "@editor.resize", "icon": "grid", "action": "map_resize"},
	{"key": "@editor.import", "icon": "download", "action": "editor_import"},
	{"key": "@editor.export", "icon": "upload", "action": "editor_export"},
	{"key": "@editor.ingame", "icon": "right", "action": "ingame"},
	{"key": "@editor.playtest", "icon": "play", "action": "playtest"},
	# The wide rows are `menu.cont` cells (`MapEditorDialog.java:181-184`,
	# `padTop(-3)` under the cont `pad(3)`); measured in java/32-edmenu.png at
	# y=370 and y=434 (the 5 px padBottom of the last grid row, then 4 px).
	{"key": "@editor.sectorgenerate", "icon": "terrain", "action": "sector_generate", "wide": true, "pad_bottom": 4.0},
	{"key": "@quit", "icon": "exit", "action": "quit", "wide": true},
]

var _editor: Node = null


func _ready() -> void:
	set_title_key("@menu")
	should_pause = false
	super._ready()
	_editor = get_node_or_null("/root/Spine/MindEditor")
	add_close_button()


func shown() -> void:
	_build()


func _build() -> void:
	# A reused dialog node: drop the previous open's body before rebuilding
	# (`shown()` rebuilds on every open; L01 T1).
	clear_content()
	var root := content_table()
	# `BaseDialog` fills the parent; the fixed menu rows centre in the full
	# screen (Java's `MapEditorDialog` menu table).
	# code-instantiated: the menu grid is data-driven from `ENTRIES`.
	var grid := MindWidgets.table()
	var column := 0
	for entry_variant in ENTRIES:
		var entry: Dictionary = entry_variant
		var wide := bool(entry.get("wide", false))
		# code-instantiated: one button per `MapEditorDialog` menu entry.
		var button := MindWidgets.icon_button(str(entry.get("icon", "")), _t(str(entry.get("key", ""))))
		button.pressed.connect(_run.bind(str(entry.get("action", ""))))
		var size := WIDE_SIZE if wide else BUTTON_SIZE
		var cell := grid.add(button).width(size.x).height(size.y).set_colspan(2 if wide else 1)
		if wide:
			cell.set_pad_top(float(entry.get("pad_top", 0.0))).set_pad_bottom(float(entry.get("pad_bottom", 0.0)))
		else:
			# `t.defaults().size(swidth, 60f).padBottom(5).padRight(5).padLeft(5)`
			# (`MapEditorDialog.java:71`): a 65 px row pitch and 190 px columns.
			cell.set_pad_left(5.0).set_pad_right(5.0).set_pad_bottom(5.0)
		if wide:
			grid.row()
			column = 0
		else:
			column += 1
			if column % 2 == 0:
				grid.row()
	if column % 2 != 0:
		grid.row()
	# `menu.cont.table(t -> ...)` under `cont.defaults().pad(3)` (Dialog.java:213).
	root.add(grid).pad(3)


func _run(action: String) -> void:
	match action:
		"save":
			_save()
		"map_info", "map_generate", "map_resize", "editor_import", "editor_export", "sector_generate":
			_open_menu_dialog(action)
		"ingame":
			_edit_in_game()
		"playtest":
			_playtest()
		"quit":
			_quit()
		_:
			pass


## `MapEditorDialog.save` (`:410-450`): name -> info dialog + `@editor.save.noname`;
## builtin map -> `@editor.save.overwrite`; success -> `@editor.saved`. The menu
## hides on every path.
func _save() -> void:
	if _editor == null:
		return
	var result: Dictionary = _call("save")
	if bool(result.get("ok", false)):
		_notify_info(_t("@editor.saved"))
		_close_pressed()
		return
	match str(result.get("error", "")):
		"noname":
			_open_menu_dialog("map_info")
			_notify(_t("@editor.save.noname"))
		"overwrite":
			_notify(_t("@editor.save.overwrite"))
		_:
			_notify(_t("@editor.errorsave"))
	_close_pressed()


## Every non-action entry opens a registered editor dialog (`menu.hide()` +
## `dialog.show()` upstream).
func _open_menu_dialog(name: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("open_dialog"):
		return
	# From the menu the generate dialog applies to the editor (`applyToEditor`).
	if name == "map_generate":
		var dialog := _dialog("map_generate")
		if dialog != null:
			dialog.set("applied", true)
	ui.call("open_dialog", name, "")
	_close_pressed()


## `MapEditorDialog.editInGame` (`:326-372`): save first, then play the hidden
## `editor` gamemode. Java plays the live editor tiles; the port's editor grid is
## not the live world, so it goes through the T1 playtest path
## (`start_map_mode(..., playtest = true)`).
func _edit_in_game() -> void:
	if _editor == null or not bool(_call("save").get("ok", false)):
		_notify(_t("@editor.errorsave"))
		return
	var name := str(_tags().get("name", ""))
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if name.is_empty() or campaign == null \
			or not bool(campaign.call("start_map_mode", name, "editor", true, "")):
		_notify(_t("@editor.errorsave"))
		return
	_close_pressed()
	_hide_editor()
	_show_game()


## `MapEditorDialog.playtest` (`:374-393`): save, then play immediately (shift)
## or open the map-play dialog in playtesting mode.
func _playtest() -> void:
	if _editor == null:
		return
	var result: Dictionary = _call("playtest")
	if not bool(result.get("ok", false)):
		_notify(_save_error(str(result.get("error", ""))))
		return
	_close_pressed()
	if bool(result.get("dialog", false)):
		var ui := get_node_or_null("/root/MindUi")
		if ui != null and ui.has_method("open_dialog"):
			ui.call(
				"open_dialog",
				"map_play",
				JSON.stringify({"map": str(_tags().get("name", "")), "playtest": true})
			)
		return
	# Play started directly (the Java shift path): hide the editor shell.
	_hide_editor()
	_show_game()


## The menu's `@quit` -> `tryExit()` (`MapEditorDialog.java:262-265`): the shell
## owns the unsaved-changes confirm.
func _quit() -> void:
	_close_pressed()
	var shell := get_node_or_null("/root/Spine/Ui/EditorDialog")
	if shell != null and shell.has_method("_confirm_exit"):
		shell.call("_confirm_exit")


func _call(method: String) -> Dictionary:
	if _editor == null or not _editor.has_method(method):
		return {}
	var value: Variant = _editor.call(method)
	return value if value is Dictionary else {}


## `Save`/`playtest` error -> message (`MapEditorDialog.save`, `:410-450`).
func _save_error(error: String) -> String:
	match error:
		"noname":
			return _t("@editor.save.noname")
		"overwrite":
			return _t("@editor.save.overwrite")
		_:
			return _t("@editor.errorsave")


func _tags() -> Dictionary:
	if _editor == null or not _editor.has_method("tags"):
		return {}
	var value: Variant = _editor.call("tags")
	return value if value is Dictionary else {}


func _dialog(name: String) -> Node:
	var ui_root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui_root == null or not ui_root.has_method("dialog"):
		return null
	return ui_root.call("dialog", name)


func _hide_editor() -> void:
	var shell := get_node_or_null("/root/Spine/Ui/EditorDialog")
	if shell != null and shell.has_method("hide_dialog"):
		shell.call("hide_dialog")


func _show_game() -> void:
	var ui_root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui_root != null and ui_root.has_method("set_menu_visible"):
		ui_root.call("set_menu_visible", false)


## `ui.showInfoFade` has no publisher in the port; the info dialog is the
## nearest surface (`@editor.saved`).
func _notify_info(text: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_info"):
		ui.call("show_info", text)


## `ui.showErrorMessage` via the shared `show_text` prompt (`@error.title`).
func _notify(text: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_text"):
		ui.call("show_text", MindWidgets.markup_text(_t("@error.title")), MindWidgets.markup_text(text))
