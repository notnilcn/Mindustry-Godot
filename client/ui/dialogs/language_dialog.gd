## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/LanguageDialog.java (plan 14 §3.4, M3).
##
## Locale list with native display names. The available locale set comes from
## `MindAssets.locales()`; the list order matches `Vars.java:315-327` (sorted by
## display name, `router` last). A click writes `locale` through
## `Core.settings` (`MindUi.settings_put_text`) and shows the modal
## `@language.restart` info dialog, like `LanguageDialog.java:78-84`; the bundle
## is selected at boot (`Vars.java:536-564`).
##
## Layout (`LanguageDialog.setup`, :69-88): a fill-parent `BaseDialog` (no boxed
## panel) whose `cont` holds one `ScrollPane` of 400x50 `Styles.flatTogglet`
## rows with 24 px left/right margins, centered between the title and Back.

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
	# `BaseDialog` ctor `setFillParent(true)` over `Styles.defaultDialog`: the
	# rows float over the dimmed stage (no boxed panel), like the other
	# fill-parent dialogs (`full_text_dialog.gd`, `keybind_dialog.gd`).
	full_dialog = true
	super._ready()
	_build()
	# `addCloseButton()` = `addCloseButton(210f)`: `@back` at 210x64
	# (`BaseDialog.java:73-83`).
	var back := add_close_button(210.0)
	back.custom_minimum_size.y = 64.0


func shown() -> void:
	# The per-frame `update(t -> t.setChecked(...))` in Java; the port re-syncs
	# the checked row whenever the dialog opens (and after each click).
	# Deferred: `MindUi.open_dialog` holds its `&mut` binding while `shown()`
	# runs, so reading `settings_get_text` here would re-enter it.
	call_deferred("_refresh_checked")


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
	# `cont.add(pane)`: the pane takes the whole height between the title and
	# the button row and is centered horizontally. The shell's Body is itself a
	# ScrollContainer, so stop it scrolling and let the content column stretch
	# (a ScrollContainer only stretches an expanding child).
	if body != null:
		body.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	if cont != null:
		cont.size_flags_vertical = Control.SIZE_EXPAND_FILL
	# `Table langs = new Table(); langs.marginRight(24f).marginLeft(24f);`
	# code-instantiated: the locale table is filled from the runtime locale set.
	var list := MindTable.new()
	list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	# `new ScrollPane(langs)` + `pane.setScrollingDisabled(true, false)`.
	# code-instantiated: the scroll pane wraps the runtime locale table.
	var pane := MindScroll.new()
	pane.add_child(list)
	pane.set_scrolling_disabled(true, false)
	pane.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	pane.size_flags_vertical = Control.SIZE_EXPAND_FILL
	if cont != null:
		cont.add_child(pane)
	var group := ButtonGroup.new()
	for locale in _available_locales():
		# code-instantiated: locale rows are data-driven from the plan-03 locale set.
		var button := Button.new()
		button.text = get_display_name(locale)
		button.theme_type_variation = "flatTogglet"
		_apply_flat_toggle_style(button)
		button.custom_minimum_size = Vector2(400, 50)
		button.toggle_mode = true
		button.button_group = group
		button.button_pressed = locale == _current_locale
		button.focus_mode = Control.FOCUS_NONE
		button.pressed.connect(_select.bind(locale))
		# Upstream `langs.add(button).group(group)...size(400f, 50f).row()`;
		# the table's 24 px side margins are the cell pads.
		list.add(button).width(400.0).height(50.0).set_pad_left(24.0).set_pad_right(24.0)
		list.row()
		_buttons[locale] = button


## `Styles.flatTogglet` (`Styles.java:190-199`): `up = black`, `over = flatOver`
## (#454545), `down`/`checked = flatDown` (the `flat-down-base` 9-patch: black
## centre, 4 px `Pal.accent` frame, no padding). The current locale is the one
## row drawn with the accent frame.
func _apply_flat_toggle_style(button: Button) -> void:
	var up := _flat_box(Color.BLACK, 0)
	var over := _flat_box(Color(0x45 / 255.0, 0x45 / 255.0, 0x45 / 255.0, 1.0), 0)
	var down := _flat_box(Color.BLACK, 4)
	button.add_theme_stylebox_override("normal", up)
	button.add_theme_stylebox_override("hover", over)
	button.add_theme_stylebox_override("pressed", down)
	button.add_theme_stylebox_override("hover_pressed", down)
	button.add_theme_stylebox_override("disabled", up)
	button.add_theme_stylebox_override("focus", StyleBoxEmpty.new())
	button.add_theme_color_override("font_color", Color.WHITE)
	button.add_theme_color_override("font_hover_color", Color.WHITE)
	button.add_theme_color_override("font_pressed_color", Color.WHITE)
	button.add_theme_color_override("font_hover_pressed_color", Color.WHITE)
	button.add_theme_color_override("font_disabled_color", Color.GRAY)


func _flat_box(fill: Color, border: int) -> StyleBoxFlat:
	var box := StyleBoxFlat.new()
	box.bg_color = fill
	box.border_color = MindStyles.ACCENT
	box.border_width_left = border
	box.border_width_top = border
	box.border_width_right = border
	box.border_width_bottom = border
	return box


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
		# `ui.showInfo("@language.restart")` (`UI.java:466-474`): a modal
		# untitled dialog with the 400 px wrapped text and a 110x50 `@ok`.
		# `MindUi.show_info` is the port's top-centre toast, so route through
		# `show_text` with an empty title (the same prompt shape, no title row).
		ui.call("show_text", "", _t("@language.restart"))
	_refresh_checked()
