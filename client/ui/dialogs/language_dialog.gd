## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/LanguageDialog.java (plan 14 §3.4, M3).
##
## Locale list with native display names. The available locale set comes from
## `MindAssets.locales()`; the list order matches `Vars.java:315-327` (sorted by
## display name, `router` last). A click writes `locale` through
## `Core.settings` (`MindUi.settings_put_text`) and shows `@language.restart`,
## like `LanguageDialog.java:78-84`; the bundle is selected at boot
## (`Vars.java:536-564`).

extends MindDialog

signal locale_selected(locale: String)

## `LanguageDialog.displayNames` — locale code -> native name (parity ABI).
const DISPLAY_NAMES := {
	"ca": "Català", "id_ID": "Bahasa Indonesia", "da": "Dansk", "de": "Deutsch",
	"et": "Eesti", "en": "English", "es": "Español", "eu": "Euskara",
	"fil": "Filipino", "fr": "Français", "it": "Italiano", "lt": "Lietuvių",
	"hu": "Magyar", "nl": "Nederlands", "nl_BE": "Nederlands (België)",
	"pl": "Polski", "pt_BR": "Português (Brasil)", "pt_PT": "Português (Portugal)",
	"ro": "Română", "fi": "Suomi", "sv": "Svenska", "vi": "Tiếng Việt",
	"tk": "Türkmen dili", "tr": "Türkçe", "cs": "Čeština", "be": "Беларуская",
	"bg": "Български", "ru": "Русский", "sr": "Српски", "uk_UA": "Українська",
	"th": "ไทย", "zh_CN": "简体中文", "zh_TW": "正體中文", "ja": "日本語",
	"ko": "한국어", "router": "router",
}

static var _current_locale := "en"

## Locale rows by code (`LanguageDialog`'s `ButtonGroup<TextButton>`).
var _buttons: Dictionary = {}


func _ready() -> void:
	set_title_key("@settings.language")
	should_pause = false
	super._ready()
	_build()
	add_close_button()


## `LanguageDialog.getLocale` current choice: the stored `locale` setting (the
## boot-resolved value once `findClosestLocale` ran), else the live locale.
func current_locale() -> String:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		var stored: Variant = ui.call("settings_get_text", "locale", "default")
		if stored is String and not (stored as String).is_empty() and (stored as String) != "default":
			return stored as String
	var assets := MindWidgets.assets()
	if assets != null and assets.has_method("locale"):
		var live: Variant = assets.call("locale")
		if live is String and not (live as String).is_empty():
			return live as String
	return "en"


func _build() -> void:
	_current_locale = current_locale()
	_buttons.clear()
	var group := ButtonGroup.new()
	var list := content_table()
	list.add_theme_constant_override("separation", 4)
	for locale in _available_locales():
		# code-instantiated: locale rows are data-driven from the plan-03 locale set.
		var button := Button.new()
		button.text = get_display_name(locale)
		button.theme_type_variation = "flatTogglet"
		button.custom_minimum_size = Vector2(400, 50)
		button.toggle_mode = true
		button.button_group = group
		button.button_pressed = locale == _current_locale
		button.pressed.connect(_select.bind(locale))
		# Upstream `langs.add(button)...size(400f, 50f)`; `size(s)` would make
		# the cell square.
		list.add(button).width(400.0).height(50.0)
		list.row()
		_buttons[locale] = button


## `LanguageDialog`'s per-frame `update(t -> t.setChecked(loc.equals(getLocale())))`
## (one checked row; `ButtonGroup` keeps the exclusivity, like upstream).
func _refresh_checked() -> void:
	var current := current_locale()
	for code in _buttons:
		var button: Button = _buttons[code]
		if not is_instance_valid(button):
			continue
		button.set_pressed_no_signal(code == current)


## `LanguageDialog.getDisplayName` — falls back to the raw code.
static func get_display_name(locale: String) -> String:
	var key := locale.replace("in_ID", "id_ID")
	return DISPLAY_NAMES.get(key, key)


## `Vars.init`'s `Vars.locales` (`Vars.java:315-327`): the `assets/locales` list
## sorted by display name (case-insensitive) with the `router` pseudo-locale
## appended last. Unknown locales keep their raw code (Java `Locale.toString`).
func _available_locales() -> PackedStringArray:
	var codes: Array[String] = []
	var assets := MindWidgets.assets()
	if assets != null:
		var result: Variant = assets.call("locales")
		if result is PackedStringArray and not (result as PackedStringArray).is_empty():
			for code in result as PackedStringArray:
				codes.append(code)
	if codes.is_empty():
		for code in DISPLAY_NAMES.keys():
			codes.append(str(code))
	if not codes.has("router"):
		codes.append("router")
	codes.sort_custom(
		func(a: String, b: String) -> bool:
			return get_display_name(a).nocasecmp_to(get_display_name(b)) < 0
	)
	var out := PackedStringArray()
	for code in codes:
		out.append(code)
	return out


## `LanguageDialog.java:78-84`: a click on a *different* locale stores
## `Core.settings.put("locale", code)` and shows `@language.restart` (the
## running bundle is not swapped until the next boot).
func _select(locale: String) -> void:
	if locale == current_locale():
		_refresh_checked()
		return
	_current_locale = locale
	locale_selected.emit(locale)
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("settings_put_text", "locale", locale)
		ui.call("show_info", _t("@language.restart"))
	_refresh_checked()
