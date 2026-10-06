## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/BaseDialog.java (plan 14 §3.4).
##
## All dialog scripts extend this. Visibility and the active-dialog stack are
## owned by `MindUi` (Rust); this node only renders and calls `show_dialog`/
## `hide_dialog`. Pause transitions are the governor's job (`should_pause` is
## declared by the manifest), never the widget's.

class_name MindDialog
extends Control

## Title text; empty hides the title row entirely.
@export var title_text := ""
## Whether showing this dialog pauses the game (manifest `pause`).
@export var should_pause := false
## Whether the dialog uses the full-screen dialog style.
@export var full_dialog := false
## Accent color of the title row.
@export var title_color := Color.WHITE

@onready var title_label: Label = get_node_or_null("Center/Panel/Layout/Title")
@onready var body: ScrollContainer = get_node_or_null("Center/Panel/Layout/Body")
@onready var cont: Container = get_node_or_null("Center/Panel/Layout/Body/Content")
@onready var buttons: Container = get_node_or_null("Center/Panel/Layout/Buttons")

var _context: Dictionary = {}

## Path to the live campaign facade (owns sector/tech/rules/schematic state).
const CAMPAIGN_PATH := "/root/Spine/MindCampaign"

## Cached campaign read-model snapshot. `MindUi` binds `&mut self` for the
## duration of `open_dialog`, so re-entering `MindUi.campaign_views()` from a
## dialog's `shown()` panics ("already bound"); dialogs refresh through
## `refresh_campaign_views()` (which calls the separate `MindCampaign` node)
## and reuse this cache elsewhere.
var _campaign_views_cache: Dictionary = {}
var _campaign_views_loaded := false


func _ready() -> void:
	visible = false
	_apply_title()
	_apply_full_dialog()


## Stretches the centered panel to the whole viewport for `full_dialog` dialogs
## (`Styles.fullDialog`, upstream `PlanetDialog`/`CustomGameDialog`/`LoadDialog`).
func _apply_full_dialog() -> void:
	if not full_dialog:
		return
	var panel := get_node_or_null("Center/Panel") as Control
	if panel != null:
		panel.theme_type_variation = "fullDialog"
		panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		panel.size_flags_vertical = Control.SIZE_EXPAND_FILL
		_resize_full_panel()
	var viewport := get_viewport()
	if viewport != null and not viewport.size_changed.is_connected(_resize_full_panel):
		viewport.size_changed.connect(_resize_full_panel)


func _resize_full_panel() -> void:
	var panel := get_node_or_null("Center/Panel") as Control
	if panel != null and full_dialog:
		panel.custom_minimum_size = get_viewport_rect().size


## Receives the JSON context passed to `MindUi.open_dialog` (used by the MCP
## catalogue sweep and plan-12-dependent dialogs).
func set_context_json(json_text: String) -> void:
	var parsed: Variant = JSON.parse_string(json_text)
	_context = parsed if parsed is Dictionary else {}


## Current context dictionary.
func context() -> Dictionary:
	return _context


func _apply_title() -> void:
	if title_label == null:
		return
	title_label.text = title_text
	title_label.visible = not title_text.is_empty()
	title_label.add_theme_color_override("font_color", title_color)


## Shows the dialog (called by `MindUi`; also safe to call directly).
func show_dialog() -> void:
	visible = true
	move_to_front()
	shown()
	# `MindTable` layout is manual: place now (min sizes) and again next frame
	# once the containers have assigned their final size, so grow cells fill.
	_resort_tables()
	call_deferred("_resort_tables")


## Re-runs layout on every nested `MindTable` (grow cells otherwise stay at their
## minimum size when a grid is built while the dialog is hidden).
func _resort_tables() -> void:
	for node in find_children("*", "", true, false):
		if node is MindTable:
			node.sort_now()


## Hides the dialog.
func hide_dialog() -> void:
	if not visible:
		return
	visible = false
	hidden()


## Whether the dialog is currently visible.
func is_shown() -> bool:
	return visible


## Adds an `@back` close button to the button row (`addCloseButton`).
func add_close_button(width: float = 210.0) -> Button:
	# code-instantiated: the close button is added by the dialog's own script at
	# runtime (button row contents vary per dialog).
	var button := Button.new()
	button.text = _t("@back")
	button.theme_type_variation = "defaultt"
	button.pressed.connect(_close_pressed)
	button.custom_minimum_size.x = width
	if buttons != null:
		buttons.add_child(button)
	return button


## Closes through `MindUi` so the dialog stack and the pause governor stay in
## sync (upstream `BaseDialog.hide` pops the dialog); a dialog that was never
## registered still hides locally.
func _close_pressed() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("close_dialog"):
		if bool(ui.call("close_dialog", String(name))):
			return
	hide_dialog()


## Appends a themed action button to the button row (`buttons.button(...)`).
## `icon_name` adds an icon-font glyph; `width > 0` sets a minimum width.
func add_button(text_value: String, listener: Callable, icon_name: String = "", width: float = 0.0) -> Button:
	# code-instantiated: action buttons are data-driven per dialog and have no
	# static scene (the button row is shared across every dialog).
	var button: Button
	if icon_name.is_empty():
		button = MindWidgets.button(text_value)
	else:
		button = MindWidgets.icon_button(icon_name, text_value)
	if width <= 0.0 and not icon_name.is_empty():
		# A `Button` does not size to its child row; estimate a width from the
		# label so icon+text action buttons do not clip (e.g. "Add Server").
		width = maxf(150.0, float(text_value.length()) * 12.0 + 60.0)
	if width > 0.0:
		button.custom_minimum_size.x = width
	if listener.is_valid():
		button.pressed.connect(listener)
	if buttons != null:
		buttons.add_child(button)
	return button


## Bundle string translated to BBCode (`MindAssets.bundle_markup`): `[accent]`
## colors, `:icon:` tokens and `\n` escapes. Mirrors upstream `Core.bundle.get`.
func _tm(key: String) -> String:
	return MindWidgets.markup(key)


## Bundle lookup with the key echoed back when assets are absent (plan 03
## `Bundle.get` semantics; all user-visible strings must go through a key).
func _t(key: String) -> String:
	var resolved := key.trim_prefix("@")
	var assets := MindWidgets.assets()
	if assets == null:
		return resolved
	return str(assets.call("bundle_get", resolved))


## Sets the title from a bundle key and applies the accent title color.
func set_title_key(key: String) -> void:
	title_text = _t(key)
	title_color = MindStyles.ACCENT
	_apply_title()


## Sets literal title text (empty hides the row) and re-applies.
func set_title_text(value: String) -> void:
	title_text = value
	_apply_title()


## Removes every button from the shared button row (used when a dialog swaps
## between modes, e.g. campaign select vs. the planet view).
func clear_buttons() -> void:
	if buttons == null:
		return
	for child in buttons.get_children():
		child.queue_free()


## M5 campaign read models (plan 14 §3.11 12 seam). Reads the live
## `MindCampaign` projection when available and otherwise the `MindUi` fixture;
## cached for the dialog's lifetime. The cache is first loaded during `_ready`
## (before `MindUi` binds itself), and `refresh_campaign_views()` rebuilds it on
## open so a running dialog reflects live campaign state.
func campaign_views() -> Dictionary:
	if _campaign_views_loaded:
		return _campaign_views_cache
	_campaign_views_loaded = true
	var live := _live_campaign_views("")
	if not live.is_empty():
		_campaign_views_cache = live
		return _campaign_views_cache
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("campaign_views"):
		var parsed: Variant = JSON.parse_string(str(ui.call("campaign_views")))
		if parsed is Dictionary:
			_campaign_views_cache = parsed
	return _campaign_views_cache


## Rebuilds the cached campaign read models from the live `MindCampaign` node
## for `planet` (`""` = active) and returns the cache. Safe from `shown()` while
## `MindUi.open_dialog` holds its mutable borrow because it never calls `MindUi`.
func refresh_campaign_views(planet: String = "") -> Dictionary:
	var live := _live_campaign_views(planet)
	if not live.is_empty():
		_campaign_views_cache = live
		_campaign_views_loaded = true
	return _campaign_views_cache


## The live `MindCampaign` facade node, or null when absent (menu boot).
func campaign_node() -> Node:
	return get_node_or_null(CAMPAIGN_PATH)


## Fetches a campaign read-model method returning JSON; `{}` when unavailable.
func campaign_json(method: String, args: Array = []) -> Dictionary:
	var campaign := campaign_node()
	if campaign == null or not campaign.has_method(method):
		return {}
	var parsed: Variant = JSON.parse_string(str(campaign.callv(method, args)))
	return parsed if parsed is Dictionary else {}


## Calls a campaign facade method, returning `null` when it is absent.
func campaign_call(method: String, args: Array = []) -> Variant:
	var campaign := campaign_node()
	if campaign == null or not campaign.has_method(method):
		return null
	return campaign.callv(method, args)


## Opens the content-info dialog for a content name (`ResearchDialog` info
## button; the database passes the same `content` context key).
func open_content_info(content: String) -> void:
	if content.is_empty():
		return
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("open_dialog", "content", JSON.stringify({"content": content}))


## Shows a transient info prompt through `MindUi` (only valid outside an
## `open_dialog` call, which mutably binds `MindUi`).
func show_toast(text: String) -> void:
	if text.is_empty():
		return
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_info"):
		ui.call("show_info", text)


## `UI.formatTime`-style playtime (h/m/s) for millisecond values.
func format_time_ms(milliseconds: int) -> String:
	var seconds := int(milliseconds / 1000.0)
	var hours := seconds / 3600
	var minutes := (seconds % 3600) / 60
	if hours > 0:
		return "%dh %dm" % [hours, minutes]
	if minutes > 0:
		return "%dm %ds" % [minutes, seconds % 60]
	return "%ds" % seconds


## Live campaign read models from `MindCampaign.campaign_views_json` (WS2
## contract); `{}` when the facade or endpoint is absent.
func _live_campaign_views(planet: String) -> Dictionary:
	var campaign := campaign_node()
	if campaign == null or not campaign.has_method("campaign_views_json"):
		return {}
	var parsed: Variant = JSON.parse_string(str(campaign.call("campaign_views_json", planet)))
	return parsed if parsed is Dictionary else {}


## A named section of the campaign read models (`planets`/`sectors`/…).
func campaign_section(name: String) -> Array:
	return campaign_views().get(name, []) as Array


## The dialog's content column as a fresh `MindTable` (clears prior content).
## code-instantiated: dialog bodies are data-driven and rebuilt on `shown()`.
func content_table() -> MindTable:
	var table := MindTable.new()
	table.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	if cont != null:
		cont.add_child(table)
	return table


## Hook for subclasses to rebuild dynamic content when shown (BaseDialog.shown).
func shown() -> void:
	pass


## Hook for subclasses to tear down on hide (BaseDialog.hidden).
func hidden() -> void:
	pass
