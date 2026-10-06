## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/LanguageDialog.java (plan 14 §3.4, M3).
##
## Locale list with native display names. The available locale set comes from
## plan 03's `MindAssets.locales()`; persistence of the choice is plan 04's
## `SettingsStore` (deferred), so this shell emits `locale_selected` and shows
## the restart notice.

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


func _ready() -> void:
	set_title_key("@settings.language")
	should_pause = false
	super._ready()
	_build()
	add_close_button()


func _build() -> void:
	var list := content_table()
	list.add_theme_constant_override("separation", 4)
	for locale in _available_locales():
		# code-instantiated: locale rows are data-driven from the plan-03 locale set.
		var button := Button.new()
		button.text = get_display_name(locale)
		button.theme_type_variation = "flatTogglet"
		button.custom_minimum_size = Vector2(400, 50)
		button.toggle_mode = true
		button.button_pressed = locale == _current_locale
		button.pressed.connect(_select.bind(locale))
		# Upstream `langs.add(button)...size(400f, 50f)`; `size(s)` would make
		# the cell square.
		list.add(button).width(400.0).height(50.0)
		list.row()


## `LanguageDialog.getDisplayName` — falls back to the raw code.
static func get_display_name(locale: String) -> String:
	var key := locale.replace("in_ID", "id_ID")
	return DISPLAY_NAMES.get(key, key)


func _available_locales() -> PackedStringArray:
	var assets := MindWidgets.assets()
	if assets != null:
		var result: Variant = assets.call("locales")
		if result is PackedStringArray and not (result as PackedStringArray).is_empty():
			return result as PackedStringArray
	return PackedStringArray(DISPLAY_NAMES.keys())


func _select(locale: String) -> void:
	_current_locale = locale
	locale_selected.emit(locale)
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("show_info", _t("@language.restart"))
