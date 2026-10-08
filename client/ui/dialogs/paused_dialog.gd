## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/PausedDialog.java (14-217) and
##         core/src/mindustry/core/Control.java (ESC -> `ui.paused.show()`).
##
## In-game pause menu. `shouldPause = true` (manifest `pause`) so showing it
## through `MindUi` pauses the sim via the reference-counted governor. Every
## exit routes back through `MindUi` (settings/planet) or the `UiRoot` menu
## presentation; this dialog never mutates sim state. The `@hostserver` entry
## opens the host dialog (`PausedDialog.java:98-105`); save/load plumbing is
## owned elsewhere.

extends MindDialog

var _planet_button: Button = null
var _objective_button: Button = null
var _abandon_button: Button = null
var _host_button: Button = null
var _quit_button: Button = null
var _pending_quit := false
var _pending_abandon := false


func _ready() -> void:
	set_title_key("@menu")
	should_pause = true
	super._ready()
	_build()
	_connect_confirm()


## Upstream desktop layout: `@objective` (when the sector preset has a
## description), `@abandon`, `@back` (resume), `@settings`, `@hostserver`,
## `@quit`. `@planetmap`/`@research` are the mobile campaign branch.
func _build() -> void:
	_objective_button = add_button(_t("@objective"), _open_objective, "info", 240.0)
	_objective_button.name = "objective"
	_objective_button.visible = false
	_abandon_button = add_button(_t("@abandon"), _confirm_abandon, "cancel", 240.0)
	_abandon_button.name = "abandon"
	_abandon_button.visible = false
	var back := add_close_button(240.0)
	back.name = "back"
	var settings_button := add_button(_t("@settings"), _open_settings, "settings", 240.0)
	settings_button.name = "settings"
	_host_button = add_button(_t("@hostserver"), _open_host, "host", 240.0)
	_host_button.name = "hostserver"
	_host_button.disabled = _net_active()
	_planet_button = add_button(_t("@planetmap"), _open_planet, "map", 240.0)
	_planet_button.name = "planetmap"
	_planet_button.visible = false
	_quit_button = add_button(_t("@quit"), _confirm_quit, "exit", 240.0)
	_quit_button.name = "quit"


func shown() -> void:
	_refresh_campaign_buttons()
	_refresh_host_button()


## `PausedDialog.rebuild`: `@objective` needs a preset description; `@abandon`
## appears for a campaign sector (`state.rules.sector != null`); `@planetmap`
## only exists in the mobile campaign branch.
func _refresh_campaign_buttons() -> void:
	var state := _sector_state()
	var campaign := bool(state.get("campaign", false))
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


## `PausedDialog.showQuitConfirm` (`@quit.confirm`); quitting returns to the
## standalone menu. The exit save / `logic.reset` teardown is not ported
## (save/load plumbing is reserved); the sim is parked paused behind the menu.
func _confirm_quit() -> void:
	var ui := _ui()
	if ui != null:
		_pending_quit = true
		ui.call("show_confirm", _t("@quit.confirm"))
		return
	_quit_to_menu()


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
		_quit_to_menu()


## `PlanetDialog.abandonSectorConfirm`: clears the sector base/items and drops
## its save, ending the run back at the standalone menu.
func _abandon_sector() -> void:
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign == null or not campaign.has_method("abandon_sector"):
		return
	var cleared: Variant = campaign.call("abandon_sector")
	if bool(cleared):
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
