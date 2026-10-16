## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/PausedDialog.java (14-217) and
##         core/src/mindustry/core/Control.java (ESC -> `ui.paused.show()`).
##
## In-game pause menu. `shouldPause = true` (manifest `pause`) so showing it
## through `MindUi` pauses the sim via the reference-counted governor. Every
## exit routes back through `MindUi` (settings/planet) or the `UiRoot` menu
## presentation; this dialog never mutates sim state. The `@hostserver` entry
## opens the host dialog (`PausedDialog.java:98-105`); outside a campaign and
## outside the editor the desktop set also exposes `@savegame`/`@loadgame`,
## which open the save/load dialogs (`PausedDialog.java:89-93`).

extends MindDialog

var _save_row: Container = null
var _planet_button: Button = null
var _objective_button: Button = null
var _abandon_button: Button = null
var _save_button: Button = null
var _load_button: Button = null
var _host_button: Button = null
var _quit_button: Button = null
## `PausedDialog`'s bottom-left `Icon.book` rules button (`PausedDialog.java:20-37`).
var _rules_button: Button = null
var _pending_quit := false
var _pending_abandon := false


func _ready() -> void:
	set_title_key("@menu")
	should_pause = true
	full_dialog = true
	super._ready()
	# `Styles.defaultDialog` background (`Tex.windowEmpty`): the shared
	# `MindDialog` dim is already black 0.9 (`Styles.java:458-469`,
	# `defaultDialog.stageBackground = black9`), so no local override.
	_build()
	_connect_confirm()


## Upstream desktop layout: `@objective` (when the sector preset has a
## description), `@abandon`, `@back` (resume), `@settings`, `@savegame`/
## `@loadgame` (non-campaign, non-editor), `@hostserver`, `@quit`.
## `@planetmap`/`@research` are the mobile campaign branch.
##
## `PausedDialog.rebuild` stacks the desktop campaign entries in rows
## (`PausedDialog.java:69-109`); the Java rows live in the dialog's centered
## content (`cont.defaults().width(220f).height(55).pad(5f)`), not the bottom
## button row, so the port builds its own centered row column inside `cont`.
func _build() -> void:
	# code-instantiated: the pause dialog's fixed upstream row layout; the
	# shared `buttons` HBox is a single row for every dialog.
	var rows := VBoxContainer.new()
	rows.name = "Rows"
	rows.add_theme_constant_override("separation", 10)
	# `cont.grow()` with the row table centred: the Java rows sit around
	# mid-screen (r01-00), so host them in a centred filler between the title
	# and the (empty) bottom button row instead of the scrollable body.
	var layout := get_node_or_null("Center/Panel/Layout") as Container
	if layout != null:
		if body != null:
			body.visible = false
		var host := CenterContainer.new()
		host.name = "RowsCenter"
		host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		host.size_flags_vertical = Control.SIZE_EXPAND_FILL
		layout.add_child(host)
		if buttons != null:
			layout.move_child(host, buttons.get_index())
		host.add_child(rows)

	var campaign_row := _row(rows)
	_objective_button = _add_to(campaign_row, _t("@objective"), _open_objective, "info", 220.0)
	_objective_button.name = "objective"
	_objective_button.visible = false
	_abandon_button = _add_to(campaign_row, _t("@abandon"), _confirm_abandon, "cancel", 220.0)
	_abandon_button.name = "abandon"
	_abandon_button.visible = false

	var navigation_row := _row(rows)
	var back := _add_to(navigation_row, _t("@back"), _close_pressed, "left", 220.0)
	back.name = "back"
	var settings_button := _add_to(navigation_row, _t("@settings"), _open_settings, "settings", 220.0)
	settings_button.name = "settings"

	# Desktop non-campaign, non-editor entry set (`PausedDialog.java:89-93`);
	# the campaign/mobile branch replaces it with `@research`/`@planetmap`.
	_save_row = _row(rows)
	_save_button = _add_to(_save_row, _t("@savegame"), _open_save, "save", 220.0)
	_save_button.name = "savegame"
	_load_button = _add_to(_save_row, _t("@loadgame"), _open_load, "upload", 220.0)
	_load_button.name = "loadgame"

	# Mobile campaign branch entry (`PausedDialog.java:140-149`); desktop keeps
	# it hidden and takes no row space.
	_planet_button = _add_to(rows, _t("@planetmap"), _open_planet, "map", 220.0)
	_planet_button.name = "planetmap"
	_planet_button.visible = false

	_host_button = _add_to(rows, _t("@hostserver"), _open_host, "host", 450.0)
	_host_button.name = "hostserver"
	_host_button.disabled = _net_active()
	# `PausedDialog.java:116`: the quit entry spans both columns but keeps the
	# `dw + 10` (230) width, so it centres inside its cell instead of stretching.
	_quit_button = _add_to(rows, _t("@quit"), _confirm_quit, "exit", 230.0)
	_quit_button.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	_quit_button.name = "quit"
	# `PausedDialog.java:61,79,82` (`tpad = -60f`): the first row is pulled up in
	# Java, which reads as the column sitting slightly above centre. Reproduce
	# that bias with a small bottom spacer inside the centred row column (a
	# negative VBox margin has no container equivalent).
	# code-instantiated: layout bias for the fixed upstream row column.
	var bias := Control.new()
	bias.custom_minimum_size = Vector2(0.0, 14.0)
	bias.mouse_filter = Control.MOUSE_FILTER_IGNORE
	rows.add_child(bias)
	_build_rules_button()


## `stack(cont, new Table(t -> t.bottom().left().button(Icon.book, ...)
## .size(70f).tooltip("@customize")))` (`PausedDialog.java:20-37`): the rules
## entry sits in the bottom-left corner of the (fill-parent) dialog.
func _build_rules_button() -> void:
	# code-instantiated: the corner entry has no static scene (visibility is
	# rule- and net-dependent).
	_rules_button = MindWidgets.icon_button("book", "")
	_rules_button.name = "customize"
	_rules_button.custom_minimum_size = Vector2(70.0, 70.0)
	_rules_button.tooltip_text = _t("@customize")
	_rules_button.anchor_left = 0.0
	_rules_button.anchor_top = 1.0
	_rules_button.anchor_right = 0.0
	_rules_button.anchor_bottom = 1.0
	_rules_button.offset_left = 12.0
	_rules_button.offset_top = -82.0
	_rules_button.offset_right = 82.0
	_rules_button.offset_bottom = -12.0
	_rules_button.pressed.connect(_open_rules)
	add_child(_rules_button)
	_rules_button.visible = false


## `rulesDialog.hidden(() -> { Vars.state.rules = toEdit; Call.setRules(toEdit); })`:
## the dialog edits a copy; the live rules are written on close.
func _open_rules() -> void:
	var ui := get_node_or_null("/root/MindUi")
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if ui == null or campaign == null:
		return
	var rules: Variant = JSON.parse_string(str(campaign.call("get_rules_json")))
	if not (rules is Dictionary):
		return
	var current: Dictionary = rules
	ui.call(
		"open_dialog",
		"custom_rules",
		JSON.stringify({"rules": current, "reset": current, "show_rule_edit": false})
	)
	var ui_root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui_root == null or not ui_root.has_method("dialog"):
		return
	var rules_dialog: Node = ui_root.call("dialog", "custom_rules")
	if rules_dialog == null or not rules_dialog.has_signal("rules_edited"):
		return
	var callback := Callable(self, "_on_rules_edited")
	if rules_dialog.is_connected("rules_edited", callback):
		rules_dialog.disconnect("rules_edited", callback)
	rules_dialog.connect("rules_edited", callback)


func _on_rules_edited(json: String) -> void:
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign != null and campaign.has_method("set_rules_json"):
		campaign.call("set_rules_json", json)


## `visible(() -> state.rules.allowEditRules && (net.server() || !net.active()))`.
func _refresh_rules_button() -> void:
	if _rules_button == null:
		return
	var enabled := false
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign != null and campaign.has_method("get_rules_json"):
		var rules: Variant = JSON.parse_string(str(campaign.call("get_rules_json")))
		if rules is Dictionary:
			enabled = bool((rules as Dictionary).get("allowEditRules", false))
	if enabled:
		var net := get_node_or_null("/root/Spine/MindNet")
		if net != null and net.has_method("session_state"):
			var state := str(net.call("session_state"))
			# `net.server()` hosts; an inactive client is offline play.
			enabled = state == "offline" or state == "hosting" or state == "host"
	_rules_button.visible = enabled


## A centered two-entry row of the dialog's own button column.
func _row(parent: Container) -> HBoxContainer:
	# code-instantiated: fixed upstream row grouping (campaign entries pair
	# up); the shared button row is a single HBox.
	var row := HBoxContainer.new()
	row.alignment = BoxContainer.ALIGNMENT_CENTER
	row.add_theme_constant_override("separation", 10)
	parent.add_child(row)
	return row


## `MindDialog.add_button` variant that targets one row of this dialog.
func _add_to(
		parent: Container, text_value: String, listener: Callable, icon_name: String, width: float
) -> Button:
	# code-instantiated: the entry set is fixed per the upstream rebuild branch.
	var button := MindWidgets.icon_button(icon_name, text_value)
	# `cont.defaults().width(dw).height(55)` (`PausedDialog.java:75`).
	button.custom_minimum_size = Vector2(width, 55.0)
	if listener.is_valid():
		button.pressed.connect(listener)
	parent.add_child(button)
	return button


## Updates the visible label of an `MindWidgets.icon_button` (the text lives in
## an icon row child next to the glyph label, so `Button.text` alone is not
## enough; the glyph label is first).
func _set_button_text(button: Button, value: String) -> void:
	if button == null:
		return
	var labels: Array[Label] = []
	for child in button.get_children():
		if child is HBoxContainer:
			for inner in child.get_children():
				if inner is Label:
					labels.append(inner)
	if labels.is_empty():
		button.text = value
	else:
		labels[labels.size() - 1].text = value


func shown() -> void:
	_refresh_campaign_buttons()
	_refresh_host_button()
	_refresh_rules_button()


## `PausedDialog.rebuild`: `@objective` needs a preset description; `@abandon`
## appears for a campaign sector (`state.rules.sector != null`); `@planetmap`
## only exists in the mobile campaign branch.
func _refresh_campaign_buttons() -> void:
	var state := _sector_state()
	var campaign := bool(state.get("campaign", false))
	var editor := bool(state.get("editor", false))
	var can_save := not campaign and not editor
	# `PausedDialog.java:89-93`: the campaign branch builds no `@savegame`/
	# `@loadgame` row at all, so hide the whole row container (a hidden row keeps
	# its VBox separation and shifts the rows below it).
	if _save_row != null:
		_save_row.visible = can_save
	if _save_button != null:
		_save_button.visible = can_save
	if _load_button != null:
		_load_button.visible = can_save
		# `PausedDialog.java:92`: load is disabled while a match is active.
		_load_button.disabled = _net_active()
	if _objective_button != null:
		_objective_button.visible = not str(state.get("presetDescription", "")).is_empty()
	if _abandon_button != null:
		_abandon_button.visible = campaign
		_abandon_button.disabled = _net_active() or _game_over()
	if _planet_button != null:
		# `_is_mobile()` re-enters `MindUi`, which `MindUi.open_dialog` mutably
		# binds while calling `shown()`; resolve the mobile branch after that
		# call returns (same deferral as the game-over HUD toggle, EV-0061).
		_apply_planet_visibility.call_deferred(campaign)
	if _quit_button != null:
		# `PausedDialog.java:116,154-155`: the entry reads `@save.quit` while
		# the current save is an autosave (`SaveSlot.isAutosave`, default true)
		# regardless of campaign/custom (`control.saves.getCurrent()`).
		_set_button_text(_quit_button, _t("@save.quit" if _current_slot_autosave() else "@quit"))


## `control.saves.getCurrent().isAutosave()` (`PausedDialog.java:116`): the live
## current save slot's autosave flag from the saves facade (`false` when no
## slot is current).
func _current_slot_autosave() -> bool:
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign == null or not campaign.has_method("current_save_autosave"):
		return false
	return bool(campaign.call("current_save_autosave"))


## Mobile-branch gate for `@planetmap`, deferred out of the `MindUi` bind.
func _apply_planet_visibility(campaign: bool) -> void:
	if _planet_button != null:
		_planet_button.visible = campaign and _is_mobile()


## `PausedDialog.java:98-105`: the host entry is disabled while a match is
## active (`net.active()`); hosting itself runs in the host dialog.
func _refresh_host_button() -> void:
	if _host_button != null:
		_host_button.disabled = _net_active()


func _net_active() -> bool:
	var net := get_node_or_null("/root/Spine/MindNet")
	if net == null or not net.has_method("session_state"):
		return false
	return str(net.call("session_state")) != "offline"


## The live campaign sector summary (`MindCampaign.get_sector_state`).
func _sector_state() -> Dictionary:
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign == null or not campaign.has_method("get_sector_state"):
		return {}
	var state: Variant = campaign.call("get_sector_state")
	return state if state is Dictionary else {}


## `state.gameOver` from the live HUD state.
func _game_over() -> bool:
	var hud := get_node_or_null("/root/MindHud")
	return hud != null and bool(hud.get("game_over"))


## `Vars.mobile` (`MindUi.is_mobile`).
func _is_mobile() -> bool:
	var ui := _ui()
	return ui != null and ui.has_method("is_mobile") and bool(ui.call("is_mobile"))


## `ui.settings::show` while the pause dialog stays on the stack; closing
## settings restores this dialog with the governor still holding the pause.
func _open_settings() -> void:
	var ui := _ui()
	if ui != null:
		ui.call("open_dialog", "settings", "")


## `PausedDialog.java:91`: `@savegame` opens the save dialog (desktop,
## non-campaign, non-editor).
func _open_save() -> void:
	var ui := _ui()
	if ui != null:
		ui.call("open_dialog", "save", "")


## `PausedDialog.java:92`: `@loadgame` opens the load dialog.
func _open_load() -> void:
	var ui := _ui()
	if ui != null:
		ui.call("open_dialog", "load", "")


## `PausedDialog.java:146-149`: hide and open the planet map (campaign only).
func _open_planet() -> void:
	var ui := _ui()
	if ui != null:
		ui.call("open_dialog", "planet", "")


## `ui.fullText.show("@objective", preset.description)`.
func _open_objective() -> void:
	var description := str(_sector_state().get("presetDescription", ""))
	if description.is_empty():
		return
	var ui := _ui()
	if ui != null:
		ui.call("open_dialog", "full_text", JSON.stringify({
			"title": _t("@objective"),
			"text": description,
		}))


## `PausedDialog.java:82`: `ui.planet.abandonSectorConfirm(sector, hide)`.
func _confirm_abandon() -> void:
	var ui := _ui()
	if ui != null:
		_pending_abandon = true
		ui.call("show_confirm", _t("@sector.abandon.confirm"))
		return
	_abandon_sector()


## `PausedDialog.java:98-105`: `ui.host.show()` over the pause menu.
func _open_host() -> void:
	var ui := _ui()
	if ui != null:
		ui.call("open_dialog", "host", "")


## `PausedDialog.showQuitConfirm` (`@quit.confirm`); quitting runs the exit save
## (`runExitSave` writes the current autosave slot through the IO seam) before
## returning to the standalone menu.
func _confirm_quit() -> void:
	var ui := _ui()
	if ui != null:
		_pending_quit = true
		ui.call("show_confirm", _t("@quit.confirm"))
		return
	_quit_with_exit_save()


func _connect_confirm() -> void:
	var ui := _ui()
	if ui != null and ui.has_signal("confirm_result") \
			and not ui.is_connected("confirm_result", Callable(self, "_on_confirm")):
		ui.connect("confirm_result", Callable(self, "_on_confirm"))


func _on_confirm(confirmed: bool) -> void:
	if _pending_abandon:
		_pending_abandon = false
		if confirmed:
			_abandon_sector()
		return
	if not _pending_quit:
		return
	_pending_quit = false
	if confirmed:
		_quit_with_exit_save()


## `PlanetDialog.abandonSectorConfirm`: clears the sector base/items and drops
## its save, ending the run back at the standalone menu.
func _abandon_sector() -> void:
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign == null or not campaign.has_method("abandon_sector"):
		return
	var cleared: Variant = campaign.call("abandon_sector")
	if bool(cleared):
		_quit_to_menu()


## `PausedDialog.showQuitConfirm`'s quit runnable: `runExitSave()` writes the
## current autosave slot (and `logic.reset`s the match), then the menu shows.
func _quit_with_exit_save() -> void:
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	# `runExitSave` -> `checkPlaytest()` (`PausedDialog.java:189-197`): a
	# playtest (`state.playtestingMap`) skips the exit save and returns to the
	# editor. Close first: closing releases the pause governor, then
	# `check_playtest` re-parks the sim behind the editor.
	if campaign != null and campaign.has_method("playtesting_map") \
			and not str(campaign.call("playtesting_map")).is_empty():
		_close_pressed()
		var root := get_node_or_null("/root/Spine/Ui/UiRoot")
		if root != null and root.has_method("check_playtest") and bool(root.call("check_playtest")):
			return
	if campaign != null and campaign.has_method("run_exit_save"):
		campaign.call("run_exit_save")
	_quit_to_menu()


func _quit_to_menu() -> void:
	_close_pressed()
	var host := get_node_or_null("/root/Spine/SimHost")
	if host != null and host.has_method("set_paused"):
		host.call("set_paused", true)
	var root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if root != null and root.has_method("set_menu_visible"):
		root.call("set_menu_visible", true)


func _ui() -> Node:
	return get_node_or_null("/root/MindUi")
