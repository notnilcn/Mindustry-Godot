## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/MenuFragment.java.
##
## Standalone menu. `Logo`/`Version`/`Sidebar`/`Submenu`/`Discord` are declared in
## `menu_fragment.tscn`; this script lays out the logo, builds the desktop button
## tree (upstream `buildDesktop`), fades submenus in/out and routes each button to
## `MindUi.open_dialog`, an info prompt, or quit. Icons are the Mindustry icon-font
## glyphs (`icon_codes.json`) rendered through the `MindAssets` icon font.

extends Control

## UI icon name -> `icon.ttf` code point (`assets/icons/icon_codes.json`).
const ICON_CODES := {
	"play": 59433,
	"menu": 59532,
	"terrain": 59492,
	"steam": 59426,
	"book": 59483,
	"settings": 59516,
	"exit": 59487,
	"add": 59411,
	"download": 59513,
	"info": 61737,
	"paste": 59474,
}

const BUTTON_W := 230.0
const BUTTON_H := 70.0

## Upstream `MenuFragment.desktopButtons` (top-level). `submenu` opens the second
## column, `dialog` opens a manifest dialog, `info` shows a bundle prompt.
const DESKTOP_BUTTONS := [
	{"text": "@play", "icon": "play", "submenu": [
		{"text": "@campaign", "icon": "play", "dialog": "planet"},
		{"text": "@joingame", "icon": "add", "dialog": "join"},
		{"text": "@customgame", "icon": "terrain", "dialog": "custom"},
		{"text": "@loadgame", "icon": "download", "dialog": "load"},
	]},
	{"text": "@database.button", "icon": "menu", "submenu": [
		{"text": "@schematics", "icon": "paste", "dialog": "schematics"},
		{"text": "@database", "icon": "book", "dialog": "database"},
		{"text": "@about.button", "icon": "info", "dialog": "about"},
	]},
	{"text": "@editor", "icon": "terrain", "dialog": "editor_maps"},
	{"text": "@mods", "icon": "book", "dialog": "mods"},
	{"text": "@settings", "icon": "settings", "dialog": "settings"},
	{"text": "@quit", "icon": "exit", "action": "quit"},
]

## Mobile grid (`MenuFragment.buildMobile`): a flat 3-column set.
const MOBILE_BUTTONS := [
	{"text": "@campaign", "icon": "play", "dialog": "planet"},
	{"text": "@joingame", "icon": "add", "dialog": "join"},
	{"text": "@customgame", "icon": "terrain", "dialog": "custom"},
	{"text": "@loadgame", "icon": "download", "dialog": "load"},
	{"text": "@editor", "icon": "terrain", "dialog": "editor_maps"},
	{"text": "@settings", "icon": "settings", "dialog": "settings"},
	{"text": "@mods", "icon": "book", "dialog": "mods"},
	{"text": "@quit", "icon": "exit", "action": "quit"},
]

var _buttons: VBoxContainer = null
var _submenu: PanelContainer = null
var _submenu_buttons: VBoxContainer = null
var _icon_font: FontFile = null
var _active_button: Button = null


func _ready() -> void:
	_buttons = get_node_or_null("Sidebar/Buttons") as VBoxContainer
	_submenu = get_node_or_null("Submenu") as PanelContainer
	_submenu_buttons = get_node_or_null("Submenu/Buttons") as VBoxContainer
	_icon_font = _load_icon_font()
	_apply_version()
	_setup_discord()
	if _is_mobile():
		_build_mobile()
	else:
		_build_desktop()
	_apply_layout()
	get_viewport().size_changed.connect(_apply_layout)
	if _submenu != null:
		_submenu.visible = false


func _is_mobile() -> bool:
	var ui := get_node_or_null("/root/MindUi")
	return ui != null and bool(ui.call("is_mobile"))


func _load_icon_font() -> FontFile:
	var assets := MindWidgets.assets()
	if assets == null:
		return null
	var font: Variant = assets.call("icon_font")
	return font if font is FontFile else null


func _t(key: String) -> String:
	var resolved := key.trim_prefix("@")
	var assets := MindWidgets.assets()
	if assets == null:
		return resolved
	return str(assets.call("bundle_get", resolved))


## Positions the logo (top-center), version label, sidebar/submenu columns and
## the button-size defaults, re-run on viewport resize (`ResizeEvent`).
func _apply_layout() -> void:
	var viewport_w := get_viewport_rect().size.x
	var logo := get_node_or_null("Logo") as TextureRect
	if logo != null:
		# Upstream draws the logo at `min(logo.width * Scl.scl(1), width - Scl.scl(20))`,
		# 6px below the top edge (MenuFragment.java:114-121). The port has no separate
		# UI scale; the native atlas region is the 768x107 `logo` sprite.
		var texture := MindWidgets.icon_texture("logo")
		logo.texture = texture
		var native_w := 768.0
		var native_h := 107.0
		if texture != null and texture.get_width() > 0:
			native_w = float(texture.get_width())
			native_h = float(texture.get_height())
		var w := minf(native_w, viewport_w - 20.0)
		var h := w * native_h / native_w
		logo.offset_left = -w * 0.5
		logo.offset_right = w * 0.5
		logo.offset_top = 6.0
		logo.offset_bottom = 6.0 + h
	var version := get_node_or_null("Version") as Label
	if version != null:
		var top := (logo.offset_bottom if logo != null else 0.0) + 2.0
		version.offset_left = -160.0
		version.offset_right = 160.0
		version.offset_top = top
		version.offset_bottom = top + 24.0
	var x := viewport_w / 10.0
	var sidebar := get_node_or_null("Sidebar") as PanelContainer
	if sidebar != null:
		sidebar.offset_left = x
		sidebar.offset_right = x + BUTTON_W
	if _submenu != null:
		_submenu.offset_left = x + BUTTON_W
		_submenu.offset_right = x + BUTTON_W * 2.0


func _apply_version() -> void:
	var label := get_node_or_null("Version") as Label
	if label == null:
		return
	var info := _build_info()
	var build := int(info.get("build", -1))
	# `MenuFragment.java:112`: custom builds (build == -1) draw the combined
	# version string in `#fc8140aa`, stamped builds in `#ffffffba`.
	label.add_theme_color_override(
		"font_color", Color("fc8140aa") if build == -1 else Color("ffffffba")
	)
	label.text = str(info.get("combined", "custom build"))


## The embedded `Version.java` build report (`MindPlatform.get_build_info`).
func _build_info() -> Dictionary:
	var platform := get_node_or_null("/root/Spine/MindPlatform")
	if platform != null and platform.has_method("get_build_info"):
		var info: Variant = platform.call("get_build_info")
		if info is Dictionary:
			return info
	return {"build": -1, "combined": "custom build"}


func _setup_discord() -> void:
	var discord := get_node_or_null("Discord") as Button
	if discord == null:
		return
	var texture := MindWidgets.icon_texture("discord-banner")
	if texture == null:
		discord.visible = false
		return
	var box := StyleBoxTexture.new()
	box.texture = texture
	discord.add_theme_stylebox_override("normal", box)
	discord.add_theme_stylebox_override("hover", box)
	discord.add_theme_stylebox_override("pressed", box)
	discord.tooltip_text = _t("@discord")
	discord.pressed.connect(_open.bind("discord"))


func _build_desktop() -> void:
	if _buttons == null:
		return
	for entry in DESKTOP_BUTTONS:
		var button := _make_button(entry, Vector2(BUTTON_W, BUTTON_H))
		button.pressed.connect(_on_menu_button.bind(entry, button))
		_buttons.add_child(button)


## Mobile variant (`MenuFragment.buildMobile`): the top-level set in a centered
## 3-column grid instead of the left sidebar.
func _build_mobile() -> void:
	var sidebar := get_node_or_null("Sidebar") as PanelContainer
	if sidebar != null:
		sidebar.visible = false
	# code-instantiated: mobile grid is a runtime layout variant chosen from
	# `Vars.mobile`; the entries reuse the shared desktop set.
	var grid := GridContainer.new()
	grid.columns = 3
	grid.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	add_child(grid)
	for entry in MOBILE_BUTTONS:
		var button := _make_button(entry, Vector2(150.0, 150.0))
		button.pressed.connect(_on_menu_button.bind(entry, button))
		grid.add_child(button)


## Builds one icon+label menu button. code-instantiated: the icon glyph + label
## row is data-driven from the button table and has no static scene (the desktop
## set is `width`/`height` uniform, upstream `buttons(Table, MenuButton...)`).
func _make_button(entry: Dictionary, size: Vector2) -> Button:
	var button := Button.new()
	button.custom_minimum_size = size
	button.focus_mode = Control.FOCUS_NONE
	button.toggle_mode = entry.has("submenu")
	button.add_theme_stylebox_override("normal", _menu_style(Color(0.0, 0.0, 0.0, 0.0)))
	button.add_theme_stylebox_override("hover", _menu_style(Color(1.0, 1.0, 1.0, 0.08)))
	button.add_theme_stylebox_override("pressed", _menu_style(Color(1.0, 1.0, 1.0, 0.14)))
	var row := HBoxContainer.new()
	row.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	row.offset_left = 11.0
	row.add_theme_constant_override("separation", 8)
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	button.add_child(row)
	var icon := Label.new()
	icon.text = _glyph(str(entry.get("icon", "")))
	if _icon_font != null:
		icon.add_theme_font_override("font", _icon_font)
	icon.add_theme_font_size_override("font_size", 26)
	icon.custom_minimum_size = Vector2(30.0, 0.0)
	icon.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	icon.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(icon)
	var label := Label.new()
	label.text = _t(str(entry.get("text", "")))
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(label)
	return button


func _menu_style(color: Color) -> StyleBoxFlat:
	var box := StyleBoxFlat.new()
	box.bg_color = color
	box.content_margin_left = 11.0
	return box


func _glyph(icon_name: String) -> String:
	if not ICON_CODES.has(icon_name):
		return ""
	return char(int(ICON_CODES[icon_name]))


func _on_menu_button(entry: Dictionary, button: Button) -> void:
	if entry.has("submenu"):
		if _active_button == button:
			_hide_submenu()
		else:
			_show_submenu(entry["submenu"], button)
		return
	# Upstream `MenuFragment.buttons` fades the open submenu out before running a
	# leaf button's runnable, so the menu is back to its base state behind the
	# dialog.
	_hide_submenu()
	if entry.has("dialog"):
		_open(str(entry["dialog"]))
	elif entry.has("info"):
		_info(str(entry["info"]))
	elif entry.has("action") and str(entry["action"]) == "quit":
		_quit()


func _show_submenu(entries: Array, source: Button) -> void:
	if _submenu == null or _submenu_buttons == null:
		return
	for child in _submenu_buttons.get_children():
		child.queue_free()
	# code-instantiated: the submenu top spacer aligns its first row with the
	# clicked top-level button (upstream `submenu.add().height(...)`).
	var spacer := Control.new()
	spacer.custom_minimum_size = Vector2(0.0, source.position.y)
	_submenu_buttons.add_child(spacer)
	for entry in entries:
		var button := _make_button(entry, Vector2(BUTTON_W, BUTTON_H))
		button.pressed.connect(_on_menu_button.bind(entry, button))
		_submenu_buttons.add_child(button)
	_submenu.visible = true
	if _active_button != null and _active_button != source:
		_active_button.button_pressed = false
	_active_button = source
	source.button_pressed = true


func _hide_submenu() -> void:
	if _submenu != null:
		_submenu.visible = false
	if _submenu_buttons != null:
		for child in _submenu_buttons.get_children():
			child.queue_free()
	if _active_button != null:
		_active_button.button_pressed = false
		_active_button = null


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
