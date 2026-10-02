## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/MenuFragment.java (plan 14 §3.5, M3).
##
## Standalone menu. The button set is declared in `menu_fragment.tscn`
## (tscn-first); this script only routes each button to `MindUi.open_dialog` and
## builds the mobile variant (`buildMobile`) when `MindUi.is_mobile()`. Alpha/
## mod/editor gated buttons are hidden when their owner is unavailable.

extends Control

## Button node name -> dialog manifest key.
const BUTTON_DIALOGS := {
	"PlayButton": "join",
	"DatabaseButton": "database",
	"ModsButton": "mods",
	"SettingsButton": "settings",
	"LanguageButton": "language",
	"DiscordButton": "discord",
	"AboutButton": "about",
}

## Buttons owned by plans not yet wired in this client (rendered as info stubs).
const BUTTON_INFO := {
	"CampaignButton": "@campaign",
	"EditorButton": "@editor",
}

## Button node name -> bundle text key.
const BUTTON_TEXT := {
	"PlayButton": "@play",
	"CampaignButton": "@campaign",
	"DatabaseButton": "@database",
	"EditorButton": "@editor",
	"ModsButton": "@mods",
	"SettingsButton": "@settings",
	"LanguageButton": "@settings.language",
	"DiscordButton": "@discord",
	"AboutButton": "@about.button",
	"QuitButton": "@quit",
}

var _buttons: VBoxContainer = null
var _mobile_grid: GridContainer = null


func _ready() -> void:
	_buttons = get_node_or_null("Center/Panel/Layout/Buttons")
	_version_label()
	_localize_buttons()
	if _is_mobile():
		_build_mobile()
	_connect_buttons()


func _is_mobile() -> bool:
	var ui := get_node_or_null("/root/MindUi")
	return ui != null and bool(ui.call("is_mobile"))


func _version_label() -> void:
	var label := get_node_or_null("Center/Panel/Layout/Version")
	if label == null:
		return
	label.text = "v%s" % str(ProjectSettings.get_setting("application/config/version", "dev"))


## Applies bundle text to every declared menu button (parity: no hardcoded UI
## strings; the keys are `@play`/`@database`/…).
func _localize_buttons() -> void:
	if _buttons == null:
		return
	for button_name in BUTTON_TEXT:
		var button := _buttons.get_node_or_null(NodePath(button_name))
		if button != null:
			button.text = _t(str(BUTTON_TEXT[button_name]))


func _t(key: String) -> String:
	var resolved := key.trim_prefix("@")
	var assets := MindWidgets.assets()
	if assets == null:
		return resolved
	return str(assets.call("bundle_get", resolved))


func _connect_buttons() -> void:
	if _buttons == null:
		return
	for button_name in BUTTON_DIALOGS:
		var button := _buttons.get_node_or_null(NodePath(button_name))
		if button != null:
			button.pressed.connect(_open.bind(str(BUTTON_DIALOGS[button_name])))
	for button_name in BUTTON_INFO:
		var button := _buttons.get_node_or_null(NodePath(button_name))
		if button != null:
			button.pressed.connect(_info.bind(str(BUTTON_INFO[button_name])))
	var quit := _buttons.get_node_or_null("QuitButton")
	if quit != null:
		quit.pressed.connect(_quit)


func _open(dialog_name: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("open_dialog", dialog_name, "{}")


func _info(key: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("show_info", _t(key))


func _quit() -> void:
	get_tree().quit()


## Mobile layout (`MenuFragment.buildMobile`): a 2-column grid of
## `MindMobileButton`s. code-instantiated: the mobile grid is a layout variant
## chosen at runtime from `Vars.mobile`, and each entry reuses the desktop set.
func _build_mobile() -> void:
	if _buttons == null:
		return
	for child in _buttons.get_children():
		child.queue_free()
	_mobile_grid = GridContainer.new()
	_mobile_grid.columns = 2
	_buttons.add_child(_mobile_grid)
	var entries := [
		{"icon": "units", "key": "@play", "dialog": "join"},
		{"icon": "map", "key": "@database", "dialog": "database"},
		{"icon": "settings", "key": "@settings", "dialog": "settings"},
		{"icon": "effect", "key": "@mods", "dialog": "mods"},
		{"icon": "home", "key": "@about.button", "dialog": "about"},
		{"icon": "cancel", "key": "@quit", "dialog": ""},
	]
	for entry in entries:
		var dialog_name := str(entry.dialog)
		var callback := Callable(self, "_quit") if dialog_name.is_empty() else _open.bind(dialog_name)
		var button := MindWidgets.mobile_button(str(entry.icon), str(entry.key), callback)
		_mobile_grid.add_child(button)
