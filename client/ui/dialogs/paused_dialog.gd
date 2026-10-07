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
var _host_button: Button = null
var _quit_button: Button = null
var _pending_quit := false


func _ready() -> void:
	set_title_key("@menu")
	should_pause = true
	super._ready()
	_build()
	_connect_confirm()


## Upstream desktop layout: `@back` (resume), `@settings`, `@planetmap`
## (campaign only), `@hostserver`, `@quit`. The optional objective full-text and
## abandon-sector entries are not ported yet.
func _build() -> void:
	add_close_button(240.0)
	var settings_button := add_button(_t("@settings"), _open_settings, "settings", 240.0)
	settings_button.name = "settings"
	_planet_button = add_button(_t("@planetmap"), _open_planet, "map", 240.0)
	_planet_button.name = "planetmap"
	_planet_button.visible = false
	_host_button = add_button(_t("@hostserver"), _open_host, "host", 240.0)
	_host_button.name = "hostserver"
	_host_button.disabled = _net_active()
	_quit_button = add_button(_t("@quit"), _confirm_quit, "exit", 240.0)
	_quit_button.name = "quit"


func shown() -> void:
	_refresh_campaign_buttons()
	_refresh_host_button()


## `PausedDialog.rebuild`: `@planetmap` appears only in campaign sectors.
func _refresh_campaign_buttons() -> void:
	if _planet_button == null:
		return
	_planet_button.visible = _is_campaign()


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


func _is_campaign() -> bool:
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign == null or not campaign.has_method("get_sector_state"):
		return false
	var state: Variant = campaign.call("get_sector_state")
	return state is Dictionary and bool(state.get("campaign", false))


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
	if not _pending_quit:
		return
	_pending_quit = false
	if confirmed:
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
