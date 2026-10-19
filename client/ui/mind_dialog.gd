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
## Whether the dialog uses the full-screen dialog style (`fullDialog`).
@export var full_dialog := false
## Whether the dialog fills the parent (`BaseDialog.java:22`
## `setFillParent(true)`). Java's dialogs are always fill-parent; a floating
## dialog (`Dialog.setFillParent(false)`, `UI.java:586` `showConfirm`) sets this
## to false and sizes to its content.
@export var fill_parent := true
## Accent color of the title row.
@export var title_color := Color.WHITE
## Java `cont.top()`: the body content sticks to the top of the content area
## instead of centring (`SchematicsDialog.java:78`, `DatabaseDialog.java:57`,
## `SectorSelectDialog.java:27`, `CampaignRulesDialog.java:42`,
## `MapProcessorsDialog.java:58`). Default false = Arc `Table` centre alignment.
@export var cont_top := false
## Java `BaseDialog.makeButtonOverlay()` (`BaseDialog.java:51-56`) and
## `PausedDialog` (`PausedDialog.java:24-25`): `clearChildren(); add(titleTable)`
## re-adds the title row *after* the Dialog constructor set `defaults().pad(3)`
## (arc `Dialog.java:208`), so the title cell is padded 3 px on every side and
## `cont` sits in a padded `stack` cell. Every other dialog keeps the
## constructor's unpadded title cell (`add(titleTable)` before the defaults):
## its title text and rule sit 3 px higher (java/15-coredb.png and
## java/09-settings.png: rule y=27-29; java/02-custom.png and java/08-pause.png:
## y=30-32).
@export var button_overlay := false

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
## Shared boot snapshot so every dialog does not re-project the live campaign
## on its own `_ready`; per-open refreshes always re-fetch.
static var _shared_views: Dictionary = {}

## In-flight hide fade (`hide_dialog`) and its logical-hidden flag; a re-show
## cancels the tween and `is_shown` reports false while it runs.
var _hide_tween: Tween = null
var _hiding := false


func _ready() -> void:
	visible = false
	_apply_title()
	_watch_title_shift()
	_apply_full_dialog()
	_apply_cont_alignment()
	_watch_buttons_row()
	# `BaseDialog.java:22` fills the parent for every dialog; the floating mode
	# (`set_fill_parent(false)`) keeps the panel at its content size
	# (`Dialog.setFillParent(false)`, `UI.java:582-606`).
	if fill_parent and not full_dialog:
		fill_panel()
	# Preload the read-model cache while `MindUi` cannot be bound yet (dialogs are
	# constructed before any `open_dialog` call), so `shown()` never re-enters the
	# `&mut` Rust endpoint through the lazy fallback.
	_ensure_campaign_views_loaded()


## L01 T2 — Arc `Dialog` lays out `add(cont).expand().fill(); row();
## add(buttons).fillX()` (arc/scene/ui/Dialog.java:209-211): `cont` takes all the
## height between the title and the button row and, as a `Table` (default
## `Align.center`, Table.java), centres its content both ways unless the dialog
## calls `cont.top()`. The shell's `Content` VBox (alignment centre) sits in the
## `Body` scroll; without vertical EXPAND the ScrollContainer gave it only its
## minimum height, so every fill dialog's body stuck to the top (pause menu,
## editor menu, Map Info, Game Over in runs/20261017-L-twin). Expanding it lets
## the VBox centre (or top-align) a short body; a tall body still scrolls.
func _apply_cont_alignment() -> void:
	if cont == null:
		return
	cont.size_flags_vertical = Control.SIZE_EXPAND_FILL
	if cont is BoxContainer:
		(cont as BoxContainer).alignment = (
			BoxContainer.ALIGNMENT_BEGIN if cont_top else BoxContainer.ALIGNMENT_CENTER
		)


## `cont.top()` at runtime (see [member cont_top]).
func set_cont_top(value: bool) -> void:
	cont_top = value
	_apply_cont_alignment()
	if cont != null:
		for child in cont.get_children():
			if child is MindTable:
				(child as MindTable).set_table_top(value)


## An empty Arc `buttons` table has no height, so `cont` reaches the bottom of
## the dialog (the pause menu's rows centre on the full height under the title,
## `PausedDialog` adds nothing to `buttons`). Hide the shell's button row while
## it holds no visible button so it reserves neither height nor separation.
func _watch_buttons_row() -> void:
	if buttons == null:
		return
	buttons.child_entered_tree.connect(_on_buttons_child_entered)
	buttons.child_exiting_tree.connect(func(_node: Node) -> void: _sync_buttons_row.call_deferred())
	for child in buttons.get_children():
		_on_buttons_child_entered(child)
	_sync_buttons_row()


func _on_buttons_child_entered(node: Node) -> void:
	if node is CanvasItem and not node.visibility_changed.is_connected(_sync_buttons_row):
		node.visibility_changed.connect(_sync_buttons_row)
	_sync_buttons_row.call_deferred()


func _sync_buttons_row() -> void:
	if buttons == null or not is_instance_valid(buttons):
		return
	var any := false
	for child in buttons.get_children():
		if child is CanvasItem and child.visible and not child.is_queued_for_deletion():
			any = true
			break
	buttons.visible = any
	_apply_body_bottom(any)


## Arc's `buttons` table pads every button by its own `defaults().pad(3)`
## (`Dialog.java:214`), so a button sits 9 px under `cont` (cont cell 3 +
## buttons cell 3 + button 3) where the Layout leaves 6: java/17-mapinfo.png
## and java/19-edmenu.png centre `cont` on y=37..567 above Back at y=576. The
## shell's `Body` takes the extra 3 px as a bottom inset while a button row is
## shown. `makeButtonOverlay` dialogs stack the buttons over `cont` instead,
## and a dialog that restyles its body keeps its own insets.
func _apply_body_bottom(has_buttons: bool) -> void:
	if body == null:
		return
	if body.has_theme_stylebox_override("panel"):
		var current := body.get_theme_stylebox("panel")
		if not current.has_meta("mind_body_inset"):
			return
	var inset := StyleBoxEmpty.new()
	inset.set_meta("mind_body_inset", true)
	inset.content_margin_bottom = BUTTON_CELL_PAD if has_buttons and not button_overlay else 0.0
	body.add_theme_stylebox_override("panel", inset)


## `Dialog.setFillParent(false)`: sizes the panel to its content (floating
## confirm-style dialogs); the stage backdrop stays full-screen.
func set_fill_parent(value: bool) -> void:
	fill_parent = value
	if value:
		fill_panel()


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
	_fit_panel()


## Fills the centered panel to the whole viewport without switching its style
## (`BaseDialog.java:18` `setFillParent(true)`; `ResearchDialog` is a plain
## `BaseDialog` whose panel must stay `defaultDialog`, not `fullDialog`).
func fill_panel() -> void:
	var viewport := get_viewport()
	if viewport != null and not viewport.size_changed.is_connected(_resize_full_panel):
		viewport.size_changed.connect(_resize_full_panel)
	_fit_panel()


func _fit_panel() -> void:
	var panel := get_node_or_null("Center/Panel") as Control
	if panel != null:
		panel.custom_minimum_size = get_viewport_rect().size


## Receives the JSON context passed to `MindUi.open_dialog` (used by the MCP
## catalogue sweep and plan-12-dependent dialogs).
func set_context_json(json_text: String) -> void:
	var parsed: Variant = JSON.parse_string(json_text)
	_context = parsed if parsed is Dictionary else {}


## Current context dictionary.
func context() -> Dictionary:
	return _context


## Title row geometry (`BaseDialog.java:23-25`): the centred label, then
## `titleTable.image(whiteui, accent).growX().height(3f).pad(4f)`. Measured in
## runs/20261017-L-twin (java/26-pause.png, java/09-custom.png): text y=10-22,
## rule y=30-32, `cont` (pad 3, Dialog.java:213) from y~40. The port's label had
## no top inset and the rule overlapped its bottom edge (text y=3-15, rule
## y=16-18), lifting every dialog body by ~14 px. The 7 px above the text is the
## dialog background's inset (`defaultDialog`/`fullDialog` panel margin in
## `MindThemeBuilder`, shared with untitled dialogs) and includes the padded
## title cell of [member button_overlay] dialogs; an unpadded title row (the
## default) is lifted by [constant TITLE_CELL_PAD] (see `_shift_title_row`).
## Keep 8 px under the text (+ the Layout's 6 px separation = the rule's 4 px
## gap, 3 px rule, 4 px pad and the 3 px cont pad) and hang the rule 4 px under
## the text. The Layout's 6 px is the 3 + 3 px `pad(3)` between two
## `cont`/`buttons` cells (Dialog.java:213-214; L01 T3: the Custom Game search
## row and the pane, java/custom.png).
const TITLE_INSET_TOP := 0.0
const TITLE_INSET_BOTTOM := 8.0
## Arc `Dialog` `defaults().pad(3)` (`Dialog.java:208`) on a re-added title cell.
const TITLE_CELL_PAD := 3.0
## Arc `buttons.defaults().pad(3)` (`Dialog.java:214`) above each button.
const BUTTON_CELL_PAD := 3.0


## Bottom inset under the title text: the rule's 4 px gap, 3 px rule and 1 px,
## plus the padded title cell's bottom pad for [member button_overlay] dialogs
## (`BaseDialog.java:51-56`: `cont` then sits `pad(3)` + the stack's `pad(3)`
## under the title row instead of the plain `cont` cell's 3 px).
func _title_inset_bottom() -> float:
	return TITLE_INSET_BOTTOM + (TITLE_CELL_PAD if button_overlay else 0.0)


func _apply_title() -> void:
	if title_label == null:
		return
	# The shell's own inset box (a dialog-supplied override is left alone).
	if not title_label.has_theme_stylebox_override("normal"):
		var fresh := StyleBoxEmpty.new()
		fresh.set_meta("mind_title_inset", true)
		title_label.add_theme_stylebox_override("normal", fresh)
	var inset := title_label.get_theme_stylebox("normal")
	if inset.has_meta("mind_title_inset"):
		inset.content_margin_top = TITLE_INSET_TOP
		inset.content_margin_bottom = _title_inset_bottom()
	title_label.text = title_text
	title_label.visible = not title_text.is_empty()
	title_label.add_theme_color_override("font_color", title_color)
	_ensure_title_underline()
	var panel := get_node_or_null("Center/Panel") as Control
	if panel != null:
		panel.queue_sort()


## `makeButtonOverlay()` at runtime (see [member button_overlay]).
func set_button_overlay(value: bool) -> void:
	button_overlay = value
	_apply_title()
	_sync_buttons_row()


## `BaseDialog.java:24-25`: a 3 px accent rule (white texture tinted `Pal.accent`)
## under the title row spanning the dialog width (less the image's `pad(4f)`,
## which the panel's 8 px side margin carries), sitting flush with the title
## cell's bottom padding. A padded title cell (`button_overlay`) narrows it by
## 3 px a side (java/02-custom.png: 1130 px vs 1136 px wide).
func _ensure_title_underline() -> void:
	var line := title_label.get_node_or_null("Underline") as ColorRect
	if line == null:
		# code-instantiated: the rule is part of the shared dialog shell.
		line = ColorRect.new()
		line.name = "Underline"
		line.color = MindStyles.ACCENT
		line.set_anchors_preset(Control.PRESET_BOTTOM_WIDE)
		line.mouse_filter = Control.MOUSE_FILTER_IGNORE
		title_label.add_child(line)
	# Rule 4 px under the text, 3 px tall: [text_bottom + 4, text_bottom + 7).
	var bottom := _title_inset_bottom()
	line.offset_top = 4.0 - bottom
	line.offset_bottom = 7.0 - bottom
	var side := TITLE_CELL_PAD if button_overlay else 0.0
	line.offset_left = side
	line.offset_right = -side


## Lifts the shell's `Layout` by [constant TITLE_CELL_PAD] after the panel
## fits it, for a titled dialog whose title cell is unpadded (the default Arc
## `Dialog` layout: `add(titleTable)` precedes `defaults().pad(3)`,
## `Dialog.java:70-79,208`). The panel inset stays shared with the padded
## (`button_overlay`) and untitled dialogs; only the stock `defaultDialog`/
## `fullDialog` panels shift (dialogs that restyle their panel own its insets).
func _watch_title_shift() -> void:
	var panel := get_node_or_null("Center/Panel") as Container
	if panel != null and not panel.sort_children.is_connected(_shift_title_row):
		panel.sort_children.connect(_shift_title_row)


func _shift_title_row() -> void:
	if title_label == null or button_overlay or not title_label.visible:
		return
	var panel := get_node_or_null("Center/Panel") as PanelContainer
	var layout := get_node_or_null("Center/Panel/Layout") as Control
	if panel == null or layout == null or panel.has_theme_stylebox_override("panel"):
		return
	var variation := String(panel.theme_type_variation)
	if variation != "defaultDialog" and variation != "fullDialog":
		return
	if panel.get_theme_stylebox("panel").get_margin(SIDE_TOP) < TITLE_CELL_PAD:
		return
	layout.position.y -= TITLE_CELL_PAD
	layout.size.y += TITLE_CELL_PAD


## Shows the dialog (called by `MindUi`; also safe to call directly).
func show_dialog() -> void:
	# A re-show during the hide fade cancels it (`Dialog.setShowAction` wins
	# over an in-flight `setHideAction` action).
	if _hide_tween != null and _hide_tween.is_valid():
		_hide_tween.kill()
	_hide_tween = null
	_hiding = false
	visible = true
	move_to_front()
	shown()
	# `UI.java:132-133`: showing fades in over 0.1 s.
	MindWidgets.fade_in(self, 0.1)


## Immediate layout pass over every nested `MindTable`. Not needed for layout
## since L01 T1 (tables sort on `NOTIFICATION_SORT_CHILDREN` and propagate their
## minimum size); kept for dialogs that read child rects in the same frame.
func _resort_tables() -> void:
	for node in find_children("*", "", true, false):
		if node is MindTable:
			node.sort_now()


## Hides the dialog. `Dialog.setHideAction(() -> sequence(fadeOut(0.1f)))`
## (`UI.java:132-133`): the panel fades out for 0.1 s and only then hides; the
## `MindUi` stack bookkeeping stays synchronous through `is_shown`.
func hide_dialog() -> void:
	if not visible or _hiding:
		return
	_hiding = true
	hidden()
	# `BaseDialog.java:27-32`: every dialog hide plays uiBack (emit-only; G07
	# owns the playback wiring).
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("play_ui_sound"):
		ui.call("play_ui_sound", "uiBack")
	if _hide_tween != null and _hide_tween.is_valid():
		_hide_tween.kill()
	_hide_tween = create_tween()
	_hide_tween.tween_property(self, "modulate:a", 0.0, 0.1)
	_hide_tween.tween_callback(_finish_hide)


## End of the hide fade: hide and restore the alpha for the next show.
func _finish_hide() -> void:
	_hide_tween = null
	_hiding = false
	modulate.a = 1.0
	visible = false


## `is_shown` for `MindUi.reveal_top` / `is_dialog_shown` (false as soon as the
## hide starts, so a stacked parent is revealed without waiting for the fade).
func is_shown() -> bool:
	return visible and not _hiding


## True when this dialog is the top of the MindUi stack (Arc modal focus):
## `_input`/`_unhandled_input` handlers of covered dialogs must ignore events.
func is_top() -> bool:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return visible
	var stack: PackedStringArray = ui.call("dialog_stack")
	return not stack.is_empty() and stack[stack.size() - 1] == String(name)


## Adds an `@back` close button to the button row (`BaseDialog.addCloseButton`,
## `BaseDialog.java:73-83`: `buttons.button("@back", Icon.left).size(210f,
## 64f)`, i.e. the themed `defaultt` skin with T1's icon cell layout).
func add_close_button(width: float = 210.0) -> Button:
	# code-instantiated: the close button is added by the dialog's own script at
	# runtime (button row contents vary per dialog).
	var button := MindWidgets.icon_button("left", _t("@back"))
	button.custom_minimum_size = Vector2(width, 64.0)
	button.pressed.connect(_close_pressed)
	if buttons != null:
		buttons.add_child(button)
	return button


## `Tex.pane` border in the reference frames (#454545).
const PANE_OUTLINE_GRAY := Color(0.27, 0.27, 0.27)


## Pane style box (`Tex.pane`/flat button look): black fill with a flat border
## (Java rail rows and action-row buttons sampled #454545; the checked rail row
## takes `Tex.buttonDown`'s accent tint).
func _pane_box(fill: Color, border: Color, width: float = 4.0, radius: float = 2.0, bottom_width: float = -1.0) -> StyleBoxFlat:
	var box := StyleBoxFlat.new()
	box.bg_color = fill
	box.border_width_left = int(width)
	box.border_width_top = int(width)
	box.border_width_right = int(width)
	box.border_width_bottom = int(bottom_width if bottom_width >= 0.0 else width)
	box.border_color = border
	box.corner_radius_top_left = int(radius)
	box.corner_radius_top_right = int(radius)
	box.corner_radius_bottom_left = int(radius)
	box.corner_radius_bottom_right = int(radius)
	box.content_margin_left = 2.0
	box.content_margin_right = 2.0
	box.content_margin_top = 2.0
	box.content_margin_bottom = 2.0
	return box


## Applies the pane look to one button (`Styles.flatTogglet` over `Tex.pane`).
## Unchecked toggles can drop the bottom edge (`bottom_border = 0`) where the
## next pane's border closes the stack.
func apply_pane_button(button: Button, border: Color, bottom_border: float = 4.0) -> void:
	var fill := Color(0.0, 0.0, 0.0, 1.0)
	button.add_theme_stylebox_override("normal", _pane_box(fill, border, 4.0, 2.0, bottom_border))
	button.add_theme_stylebox_override("hover", _pane_box(fill, border, 4.0, 2.0, bottom_border))
	button.add_theme_stylebox_override("pressed", _pane_box(fill, border))
	button.add_theme_stylebox_override("disabled", _pane_box(fill, border, 4.0, 2.0, bottom_border))


## Back-compat alias for the pre-promotion name (planet dialog call sites).
func _apply_pane_button(button: Button, border: Color, bottom_border: float = 4.0) -> void:
	apply_pane_button(button, border, bottom_border)


## `planetTable.button(name, icon, Styles.flatTogglet)`: the icon sits at the
## left margin (10 px in Java, ~13 px to its pixel row) and the label centers in
## the space that remains to the right of it.
func build_button_content(button: Button, icon: Control, label_text: String) -> void:
	# code-instantiated: button content rows are data-driven per dialog.
	var row := HBoxContainer.new()
	row.alignment = BoxContainer.ALIGNMENT_BEGIN
	row.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	row.offset_left = 13.0
	row.offset_right = -8.0
	row.add_theme_constant_override("separation", 8)
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	button.add_child(row)
	if icon != null:
		row.add_child(icon)
	var label := MindWidgets.label(label_text)
	# `RichTextLabel` min width collapses under word wrap inside a bare HBox;
	# action-row labels are single-line, so disable wrapping.
	label.autowrap_mode = TextServer.AUTOWRAP_OFF
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(label)


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
## cached for the dialog's lifetime. The cache is preloaded by `_ready` (before
## `MindUi` binds itself) and `refresh_campaign_views()` rebuilds it on open so a
## running dialog reflects live campaign state.
func campaign_views() -> Dictionary:
	_ensure_campaign_views_loaded()
	return _campaign_views_cache


## First-load path: live `MindCampaign` when it exposes the read model, else the
## `MindUi` fixture. Only called from `_ready` (never from `shown()`, where
## `MindUi` is mutably bound by `open_dialog`).
func _ensure_campaign_views_loaded() -> void:
	if _campaign_views_loaded:
		return
	_campaign_views_loaded = true
	if not _shared_views.is_empty():
		_campaign_views_cache = _shared_views
		return
	var live := _live_campaign_views("")
	if not live.is_empty():
		_campaign_views_cache = live
		_shared_views = live
		return
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("campaign_views"):
		var parsed: Variant = JSON.parse_string(str(ui.call("campaign_views")))
		if parsed is Dictionary:
			_campaign_views_cache = parsed
			_shared_views = parsed


## Rebuilds the cached campaign read models from the live `MindCampaign` node
## for `planet` (`""` = active) and returns the cache. Safe from `shown()` while
## `MindUi.open_dialog` holds its mutable borrow because it never calls `MindUi`.
func refresh_campaign_views(planet: String = "") -> Dictionary:
	var live := _live_campaign_views(planet)
	if not live.is_empty():
		_campaign_views_cache = live
		_campaign_views_loaded = true
		_shared_views = live
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


## `cont.clearChildren()`: drops every body child. Dialogs whose Java original
## builds a new `BaseDialog` per open (e.g. `EditorMapsDialog.showMap`,
## `EditorMapsDialog.java:101`) reuse this node, so a rebuild must clear first or
## the previous body stays stacked above the new one.
func clear_content() -> void:
	if cont == null:
		return
	for child in cont.get_children():
		cont.remove_child(child)
		child.queue_free()


## The dialog's content column as a new `MindTable` appended to `cont` (call
## [method clear_content] first when rebuilding).
## code-instantiated: dialog bodies are data-driven and rebuilt on `shown()`.
func content_table() -> MindTable:
	var table := MindTable.new()
	table.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	# Arc `cont` is `add(cont).expand().fill()` (Dialog.java:209): the body
	# table takes the whole content height and aligns its grid itself (centre,
	# or top for `cont.top()`), so `grow_y` cells fill like Java's (L01 T3: the
	# Banned Blocks lists collapsed to two rows without this).
	table.size_flags_vertical = Control.SIZE_EXPAND_FILL
	table.set_table_top(cont_top)
	# `cont.defaults().pad(3)` (arc `Dialog.java:213`): every `cont` cell
	# without its own pad gets 3 px (L01 T3: Map Play's rows sat 3-6 px tighter
	# than java/mapplay-caldera.png).
	table.defaults().pad(3.0)
	if cont != null:
		cont.add_child(table)
	return table


## Hook for subclasses to rebuild dynamic content when shown (BaseDialog.shown).
func shown() -> void:
	pass


## Whether ESC must not close this dialog. Java dialogs consume the key when
## modal and only close when `closeOnBack`/`addCloseButton` registered a
## listener (`arc Dialog.java:455-470`); the default stays closable.
func blocks_escape() -> bool:
	return false


## Hook for subclasses to tear down on hide (BaseDialog.hidden).
func hidden() -> void:
	pass
