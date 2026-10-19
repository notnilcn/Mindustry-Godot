## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/ModsDialog.java (plan 20 §3.9, M6).
##
## Loaded-mod list. Layout follows `ModsDialog` 1:1: a fill-parent dialog whose
## centered `cont` holds the `@mod.reloadrequired` banner, the `@mod.import` /
## `@mods.browser` button pair, then either the search field + mod pane or the
## `@mods.none` box; the button bar is `@back` / `@mods.guide` (or
## `@mods.restore`) / `@mods.openfolder` (ctor, `ModsDialog.java:51-66`).
## `@mod.import` opens the small file-vs-GitHub chooser (`:145-198`).
## All mod state comes from the plan-20 `MindMods` autoload (JSON + signals); this
## script is UI-only and never mutates sim/content state. The GitHub HTTP path
## lives in `mod_browser_dialog.gd` (documented seam), mirroring upstream's
## `ui.mods.githubImportMod`. Version matching stays in `mind-core::mods::listing`.

extends MindDialog

## Ported from `Vars.modGuideURL` (`Vars.java:83`).
const MOD_GUIDE_URL := "https://mindustrygame.github.io/wiki/modding/1-modding/"
## `Iconc.file` (`core/assets-raw/fontgen/config.json`, css "file", code 61787);
## not in `MindIcons.CODES`, so the import chooser renders the code directly.
const ICON_FILE_CODE := 61787
## `ModsDialog.setup`: `float h = 110f` (mod row height).
const ROW_HEIGHT := 110.0
## `Styles.flatOver` (`Styles.java:116`, #454545).
const FLAT_OVER := Color(0x45 / 255.0, 0x45 / 255.0, 0x45 / 255.0, 1.0)
## `Styles.grayPanel` = `Pal.darkestGray` (`Pal.java:61`).
const DARKEST_GRAY := Color(0.1, 0.1, 0.1, 1.0)
## `Color.lightGray` (#bfbfbf).
const LIGHT_GRAY := Color(0xbf / 255.0, 0xbf / 255.0, 0xbf / 255.0, 1.0)

## `cont` root table, rebuilt by `_refresh` (`setup()` clears `cont`).
var _root: MindTable = null
## The mod pane's table (`pane[0]`), rebuilt per search (`rebuild.get(query)`).
var _list: MindTable = null
var _pane: MindScroll = null
var _search: LineEdit = null
var _banner: MindLabel = null
var _errors_button: Button = null
var _progress: ProgressBar = null
## `@mods.restore` / `@mods.guide` (Java `ModsDialog` ctor shows one or the other).
var _restore_button: Button = null
var _guide_button: Button = null
## internal name -> remote version from the browser's `ModListing`s (updates).
var _updates: Dictionary = {}
## internal name whose details block is expanded (`showMod`).
var _open_details := ""
## pending async-confirm target (`mods.removeMod`).
var _pending_delete := ""
## `boolean[] anyDisabled` — one per `setup()`, shared by every search rebuild.
var _any_disabled := false
## The `@mod.import` chooser overlay (`new BaseDialog("@mod.import")`).
var _import_overlay: Control = null
## A `@mod.import.github` text prompt of ours is open (the `MindUi`
## `text_input_result` signal is shared by every dialog).
var _pending_github := false


func _ready() -> void:
	set_title_key("@mods")
	should_pause = false
	# `BaseDialog` is `setFillParent(true)`: the content floats over the dimmed
	# stage (no boxed panel), like the other fill-parent dialogs.
	full_dialog = true
	super._ready()
	_build_shell()
	_build_buttons()
	_build_import_overlay()
	_connect_mods()


func shown() -> void:
	_refresh()


func hidden() -> void:
	if _import_overlay != null:
		_import_overlay.visible = false


## The `@mod.import` chooser is its own `BaseDialog` with `closeOnBack`
## (`addCloseButton`): Esc closes it, not the mods dialog underneath.
func blocks_escape() -> bool:
	if _import_overlay != null and _import_overlay.visible:
		_import_overlay.visible = false
		return true
	return false


## Called by `mod_browser_dialog.gd` after it parses the repository listings
## (upstream `withUpdates`); `updates` maps internal name -> latest version.
func set_updates(updates: Dictionary) -> void:
	_updates = updates
	_refresh()


## `cont` is centered between the title and the button bar (`Dialog` adds it
## `expand().fill()` and the table centers its rows). The shell's Body is a
## ScrollContainer: stop it scrolling so the column stretches and centers, and
## give the mod pane its own scroll (`cont.pane(...)`).
func _build_shell() -> void:
	if body != null:
		body.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
		if not body.resized.is_connected(_fit_pane):
			body.resized.connect(_fit_pane)
	if cont != null:
		cont.size_flags_vertical = Control.SIZE_EXPAND_FILL
		cont.alignment = BoxContainer.ALIGNMENT_CENTER
	# `onResize(this::setup)`.
	var viewport := get_viewport()
	if viewport != null and not viewport.size_changed.is_connected(_on_resize):
		viewport.size_changed.connect(_on_resize)


func _on_resize() -> void:
	if visible:
		_refresh()


## Ctor button bar (`ModsDialog.java:52-66`): `addCloseButton()` (210x64),
## then `@mods.restore` (Icon.power) while any mod is marked failed, else
## `@mods.guide` (Icon.link, 210x64), then `@mods.openfolder` (Icon.link).
func _build_buttons() -> void:
	var back := add_close_button(210.0)
	back.custom_minimum_size.y = 64.0
	_restore_button = _add_bar_button("@mods.restore", "power", _restore_failed)
	_guide_button = _add_bar_button("@mods.guide", "link", _open_guide)
	_add_bar_button("@mods.openfolder", "link", _open_folder)
	_restore_button.visible = false


## A `buttons.button(text, icon, listener)` cell over `buttons.defaults()
## .size(210, 64)` (set by `addCloseButton`), drawn like the close button.
func _add_bar_button(key: String, icon: String, callback: Callable) -> Button:
	# code-instantiated: the dialog's own button-bar entries (shared shell row).
	var button := MindWidgets.button("")
	button.custom_minimum_size = Vector2(210.0, 64.0)
	apply_pane_button(button, PANE_OUTLINE_GRAY)
	build_button_content(button, MindWidgets.glyph(icon, 20), _t(key))
	button.pressed.connect(callback)
	if buttons != null:
		buttons.add_child(button)
	return button


## `ModsDialog.setup()` (`:130-353`): clears and rebuilds `cont`.
func _refresh() -> void:
	if cont == null:
		return
	var mods := _mods()
	var entries: Array = mods.call("list") if mods != null else []

	# Ctor (`:55-61`): `mods.list().contains(LoadedMod::failed)` -> Restore,
	# else Guide (the port re-evaluates per setup; Java decides once at boot).
	var any_failed := false
	for entry in entries:
		if entry is Dictionary and bool(entry.get("failed", false)):
			any_failed = true
			break
	if _restore_button != null:
		_restore_button.visible = any_failed
	if _guide_button != null:
		_guide_button.visible = not any_failed

	if is_instance_valid(_root):
		cont.remove_child(_root)
		_root.queue_free()
	_root = content_table()
	# Java tables center their content; `MindTable` packs from the left, so the
	# column itself is centered in `cont`.
	_root.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	_pane = null
	_list = null
	_search = null
	_any_disabled = false

	var viewport_width := get_viewport_rect().size.x
	# `w = min(width / 1.05 - 28, 520)`; `cont.defaults().width(min(width / 1.05, 556)).pad(4)`.
	var w := minf(viewport_width / 1.05 - 28.0, 520.0)
	var cw := minf(viewport_width / 1.05, 556.0)

	# `cont.add("@mod.reloadrequired").visible(mods::requiresReload).center()`:
	# an invisible Arc cell keeps its space, so fade instead of hiding.
	_banner = MindWidgets.label("[center]%s[/center]" % MindWidgets.markup("@mod.reloadrequired"))
	_banner.modulate.a = 1.0 if mods != null and bool(mods.call("requires_reload")) else 0.0
	_root.add(_banner).width(cw).pad(4)
	_root.row()

	_root.add(_build_action_row()).width(w).height(60.0).pad(4)
	_root.row()

	# Port-only (OD1): content errors / unsupported scripts summary button.
	# code-instantiated: only present when `MindMods.errors()` is non-empty.
	var errors: Array = mods.call("errors") if mods != null else []
	_errors_button = MindWidgets.button(_t("@mod.errors"))
	_errors_button.visible = not errors.is_empty()
	_errors_button.pressed.connect(_show_errors)
	_root.add(_errors_button).width(cw).pad(4)
	_root.row()

	# Port-only: `MindMods.mod_import_progress` bar (upstream `modImportProgress`
	# drives the loading fragment).
	# code-instantiated: transient progress widget hidden unless an import runs.
	_progress = ProgressBar.new()
	_progress.min_value = 0.0
	_progress.max_value = 1.0
	_progress.visible = false
	_root.add(_progress).width(cw).pad(4)
	_root.row()

	if not entries.is_empty():
		# `search.image(Icon.zoom).padRight(8f); search.field("", rebuild).growX();`
		# code-instantiated: the search row is rebuilt with `cont` (`setup()`).
		var search_row := HBoxContainer.new()
		search_row.add_theme_constant_override("separation", 8)
		search_row.add_child(MindWidgets.glyph("zoom", 22))
		_search = MindWidgets.field()
		_search.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		_search.text_changed.connect(_rebuild_list)
		search_row.add_child(_search)
		_root.add(search_row).width(cw).pad(4)
		_root.row()

		# `cont.pane(table1 -> pane[0] = table1.margin(10f).top()).scrollX(false)`.
		# code-instantiated: the mod rows are data-driven from `MindMods.list()`.
		_list = MindTable.new()
		_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		_list.margin(10.0)
		# code-instantiated: the scroll pane wraps the runtime mod table.
		_pane = MindScroll.new()
		_pane.add_child(_list)
		_pane.set_scrolling_disabled(true, false)
		_root.add(_pane).width(cw).pad(4)
		_root.row()
		_rebuild_list("")

		# `withUpdates.size > 1`: the `@mods.update.all` row (w x 60, padTop 12).
		if _updates.size() > 1:
			var update_all := _gray_button()
			var row := _button_row(update_all, BoxContainer.ALIGNMENT_CENTER)
			row.add_child(MindWidgets.glyph("download", 24))
			row.add_child(_row_label(MindWidgets.markup("@mods.update.all")))
			row.add_child(MindWidgets.glyph("download", 24))
			update_all.pressed.connect(_update_all)
			_root.add(update_all).width(w).height(60.0).pad(4).set_pad_top(12.0)
			_root.row()
	else:
		# `cont.table(Styles.black6, t -> t.add("@mods.none")).height(80f)`.
		# code-instantiated: the empty-state box replaces the pane in `setup()`.
		var none_box := PanelContainer.new()
		none_box.theme_type_variation = "black6"
		var none_label := MindWidgets.label("[center]%s[/center]" % MindWidgets.markup("@mods.none"))
		none_label.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		none_box.add_child(none_label)
		_root.add(none_box).width(cw).height(80.0).pad(4)
		_root.row()

	_root.sort_now()
	call_deferred("_fit_pane")


## `cont.table(buttons -> ...)` (`:139-201`): `@mod.import` (Icon.add) and
## `@mods.browser` (Icon.menu), `Styles.flatBordert`, `growX().height(60f)
## .uniformX()`, `margin(12f)`.
func _build_action_row() -> HBoxContainer:
	# code-instantiated: the import/browser pair is rebuilt with `cont` (`setup()`).
	var actions := HBoxContainer.new()
	actions.add_theme_constant_override("separation", 0)
	var import_button := _flat_bordered_button("@mod.import", MindWidgets.glyph("add", 24))
	import_button.pressed.connect(_show_import_dialog)
	actions.add_child(import_button)
	var browser_button := _flat_bordered_button("@mods.browser", MindWidgets.glyph("menu", 24))
	browser_button.pressed.connect(_open_browser)
	actions.add_child(browser_button)
	return actions


## `Styles.flatBordert` (`Styles.java:174-181`): `up = pane`, `over =
## flatDownBase` (the white-framed 9-patch), `down = flatOver`.
func _flat_bordered_button(key: String, icon: Control) -> Button:
	# code-instantiated: action buttons are rebuilt with `cont`.
	var button := MindWidgets.button("")
	button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	button.focus_mode = Control.FOCUS_NONE
	apply_pane_button(button, PANE_OUTLINE_GRAY)
	button.add_theme_stylebox_override("hover", _pane_box(Color.BLACK, Color.WHITE))
	button.add_theme_stylebox_override("pressed", _pane_box(FLAT_OVER, FLAT_OVER))
	build_button_content(button, icon, _t(key))
	return button


## `rebuild.get(query)` (`:209-314`): one `Styles.grayt` card per matching mod,
## a gray rule before the first disabled one, and the update rows.
func _rebuild_list(query: String) -> void:
	if not is_instance_valid(_list):
		return
	_list.clear_children()
	var mods := _mods()
	var entries: Array = mods.call("list") if mods != null else []
	var needle := query.strip_edges().to_lower()
	var any := false
	for entry in entries:
		if not (entry is Dictionary):
			continue
		var row: Dictionary = entry
		var display := _strip_colors(_display_name(row))
		# `Strings.matches(query, displayName)`: case-insensitive contains.
		if not needle.is_empty() and not display.to_lower().contains(needle):
			continue
		any = true
		if not bool(row.get("enabled", false)) and not _any_disabled:
			_any_disabled = true
			# `cont.image().growX().height(4f).pad(6f).color(Pal.gray)`.
			# code-instantiated: the separator precedes the first disabled row.
			var rule := ColorRect.new()
			rule.color = FLAT_OVER
			rule.custom_minimum_size = Vector2(0.0, 4.0)
			_list.add(rule).grow_x_axis().height(4.0).pad(6)
			_list.row()
		_add_mod_row(row)
	if not any:
		_list.add(MindWidgets.label(MindWidgets.markup("@none.found"))).pad(4)
		_list.row()
	_list.sort_now()
	call_deferred("_fit_pane")


## The pane takes its content height, capped to the space left between the
## title and the button bar (an Arc `ScrollPane` cell shrinks to fit).
func _fit_pane() -> void:
	if not is_instance_valid(_pane) or not is_instance_valid(_list) or not is_instance_valid(_root) or body == null:
		return
	var content_height := _list.get_combined_minimum_size().y
	var others := _root.get_combined_minimum_size().y - _pane.custom_minimum_size.y
	var available := maxf(ROW_HEIGHT, body.size.y - others)
	var target := minf(content_height, available)
	if not is_equal_approx(_pane.custom_minimum_size.y, target):
		_pane.custom_minimum_size.y = target
		_root.queue_sort()


## `getStateText` (`:356-375`): the state line under a mod's name.
func _state_text(entry: Dictionary) -> String:
	match str(entry.get("state", "")):
		"incompatible":
			return _t("@mod.blacklisted")
		"unsupported":
			# Port-only state (no JS engine, OD1): the row shows the reason text
			# (`UnsupportedReason::message`, e.g. scripts) that `list()` carries.
			var reason := str(entry.get("reason", ""))
			return reason if not reason.is_empty() else _t("@mod.blacklisted")
		"circularDependencies":
			return _t("@mod.circulardependencies")
		"incompleteDependencies":
			return _t("@mod.incompletedependencies")
		"missingDependencies":
			return _t("@mod.unmetdependencies")
		"contentErrors":
			return _t("@mod.erroredcontent")
	return _t("@mod.multiplayer.compatible") if bool(entry.get("hidden", false)) else ""


## `getStateDetails`: the tooltip/details body for a state.
func _state_details(entry: Dictionary) -> String:
	var missing := ", ".join(_string_list(entry.get("missing_dependencies", [])))
	match str(entry.get("state", "")):
		"incompatible", "unsupported":
			var reason := str(entry.get("reason", ""))
			return reason if not reason.is_empty() else _t("@mod.blacklisted.details")
		"circularDependencies":
			return _t("@mod.circulardependencies.details")
		"incompleteDependencies":
			return _fmt("@mod.incompletedependencies.details", [missing])
		"missingDependencies":
			return _fmt("@mod.missingdependencies.details", [missing])
		"contentErrors":
			return _t("@mod.erroredcontent.details")
	return ""


## `hideDisabled = !isSupported() || hasUnmetDependencies() || hasContentErrors()`.
func _hide_disabled(entry: Dictionary) -> bool:
	return str(entry.get("state", "")) in [
		"unsupported", "incompatible", "missingDependencies",
		"incompleteDependencies", "circularDependencies", "contentErrors",
	]


## The card's two-line title (`:256-258`): `[accent]name` then the short
## description when enabled (or `hideDisabled`), else `@mod.failed` /
## `@mod.disabled`.
func _title_markup(entry: Dictionary, enabled: bool) -> String:
	var text := "[accent]%s\n" % _strip_colors(_display_name(entry))
	if enabled or _hide_disabled(entry):
		var description := str(entry.get("short_description", ""))
		if not description.is_empty():
			text += "[lightgray]" + description
	else:
		text += _t("@mod.failed") if bool(entry.get("failed", false)) else _t("@mod.disabled")
	return text


## One `cont.button(t -> ..., Styles.grayt, () -> showMod(mod)).size(w, h)
## .growX().pad(4f).padTop(8f)` card (`:222-296`).
func _add_mod_row(entry: Dictionary) -> void:
	var mod_name := str(entry.get("name", ""))
	var enabled := bool(entry.get("enabled", false))
	var state := str(entry.get("state", ""))
	# `.disabled(!mod.isSupported())` on the toggle.
	var supported := state != "unsupported" and state != "incompatible"

	# code-instantiated: mod rows are data-driven from the plan-20 `MindMods.list()`.
	var card := _gray_button()
	# `t.addListener(new Tooltip(... add(stateDetails) ...))` (plain-text tooltip).
	card.tooltip_text = _strip_colors(_state_details(entry))
	card.pressed.connect(_toggle_details.bind(mod_name))
	# code-instantiated: the card's content row (`t.margin(12f)`, top-left).
	var row := HBoxContainer.new()
	row.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	row.offset_left = 4.0
	row.offset_top = 4.0
	row.offset_right = -4.0
	row.offset_bottom = -4.0
	row.add_theme_constant_override("separation", 8)
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	card.add_child(row)

	# `title1.add(new BorderImage()).size(h - 8f)...b.setDrawable(Tex.nomap);
	# b.border(Pal.accent)`. The port has no per-mod icon texture yet
	# (`MindMods.list()` carries none), so every card shows `nomap`.
	# code-instantiated: per-row icon frame.
	var icon := MindBorderImage.new()
	icon.texture = MindWidgets.icon_texture("nomap")
	icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	icon.stretch_mode = TextureRect.STRETCH_SCALE
	icon.custom_minimum_size = Vector2(ROW_HEIGHT - 8.0, ROW_HEIGHT - 8.0)
	icon.thickness = 4.0
	icon.border(MindStyles.ACCENT)
	icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(icon)

	# code-instantiated: per-row title/state column (`title1.table(text -> ...)`).
	var text_column := VBoxContainer.new()
	text_column.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	text_column.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(text_column)
	# `.ellipsis(true).top().width(300f).growX()`: two clipped lines.
	var title := MindWidgets.label(MindWidgets.markup_text(_title_markup(entry, enabled)))
	title.fit_content = false
	title.clip_contents = true
	title.custom_minimum_size = Vector2(300.0, 52.0)
	title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	title.mouse_filter = Control.MOUSE_FILTER_IGNORE
	text_column.add_child(title)
	var state_text := _state_text(entry)
	if not state_text.is_empty():
		# `text.labelWrap(state).growX()`.
		var state_label := MindWidgets.label(MindWidgets.markup_text(state_text))
		state_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		state_label.custom_minimum_size.x = 300.0
		state_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		state_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
		text_column.add_child(state_label)

	# `t.table(right -> ...)`: enable toggle + delete, `Styles.clearNonei` 50x50.
	# code-instantiated: per-row action column.
	var right := HBoxContainer.new()
	right.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	right.add_theme_constant_override("separation", 0)
	row.add_child(right)
	var toggle := MindWidgets.glyph_button(
		"down-open" if enabled else "up-open",
		_t("@mod.enabled") if enabled else _t("@mod.disabled"),
		"clearNonei"
	)
	toggle.custom_minimum_size = Vector2(50.0, 50.0)
	toggle.focus_mode = Control.FOCUS_NONE
	toggle.disabled = not supported
	toggle.pressed.connect(_set_enabled.bind(mod_name, not enabled))
	right.add_child(toggle)
	var remove := MindWidgets.glyph_button("trash", _t("@delete"), "clearNonei")
	remove.custom_minimum_size = Vector2(50.0, 50.0)
	remove.focus_mode = Control.FOCUS_NONE
	remove.pressed.connect(_confirm_delete.bind(mod_name))
	right.add_child(remove)

	_list.add(card).grow_x_axis().height(ROW_HEIGHT).pad(4).set_pad_top(8.0)
	_list.row()

	if _updates.has(mod_name):
		# `hasUpdate(mod)`: `Styles.grayt` row, `Icon.download` + `@mods.update.available`
		# (`width(w).height(48f).padTop(-4f)`).
		# code-instantiated: update row from the `withUpdates` set (browser listing).
		var update := _gray_button()
		var update_row := _button_row(update, BoxContainer.ALIGNMENT_BEGIN)
		update_row.add_child(MindWidgets.glyph("download", 24, MindStyles.ARC_LIGHT_GRAY))
		update_row.add_child(_row_label(MindWidgets.markup_format("@mods.update.available", [_updates[mod_name]])))
		update.pressed.connect(_update_mod.bind(mod_name))
		_list.add(update).grow_x_axis().height(48.0).set_pad_left(4.0).set_pad_right(4.0)
		_list.row()

	if _open_details == mod_name:
		_add_details(mod_name)


## `Styles.grayt` (`Styles.java:157-165`): `up = grayPanel`, `over`/`down =
## flatOver`.
func _gray_button() -> Button:
	# code-instantiated: cards/rows are data-driven.
	var button := Button.new()
	button.focus_mode = Control.FOCUS_NONE
	var up := _flat(DARKEST_GRAY)
	var over := _flat(FLAT_OVER)
	button.add_theme_stylebox_override("normal", up)
	button.add_theme_stylebox_override("hover", over)
	button.add_theme_stylebox_override("pressed", over)
	button.add_theme_stylebox_override("hover_pressed", over)
	button.add_theme_stylebox_override("disabled", up)
	button.add_theme_stylebox_override("focus", StyleBoxEmpty.new())
	return button


## A mouse-transparent content row inside a button (`b.margin(6f)`).
func _button_row(button: Button, alignment: BoxContainer.AlignmentMode) -> HBoxContainer:
	# code-instantiated: button content rows are data-driven.
	var row := HBoxContainer.new()
	row.alignment = alignment
	row.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	row.offset_left = 6.0
	row.offset_right = -6.0
	row.add_theme_constant_override("separation", 8)
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	button.add_child(row)
	return row


func _row_label(markup: String) -> MindLabel:
	var label := MindWidgets.label(markup)
	label.autowrap_mode = TextServer.AUTOWRAP_OFF
	label.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return label


func _flat(color: Color) -> StyleBoxFlat:
	var box := StyleBoxFlat.new()
	box.bg_color = color
	return box


## `showMod`: the expanded details block (upstream opens a BaseDialog).
func _add_details(mod_name: String) -> void:
	var mods := _mods()
	var details: Dictionary = mods.call("details", mod_name) if mods != null else {}
	# code-instantiated: details are data-driven from `MindMods.details(name)`.
	var box := VBoxContainer.new()
	box.size_flags_horizontal = Control.SIZE_EXPAND_FILL

	var author := str(details.get("author", ""))
	if not author.is_empty():
		box.add_child(MindWidgets.label("%s %s" % [_t("@editor.author"), author]))
	var version := str(details.get("version", ""))
	if not version.is_empty():
		box.add_child(MindWidgets.label("%s %s" % [_t("@mod.version"), version]))
	var description := str(details.get("description", ""))
	if not description.is_empty():
		var body_label := MindWidgets.label(description)
		body_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		box.add_child(body_label)
	var state_details := _state_details(details)
	if not state_details.is_empty():
		box.add_child(MindWidgets.label(state_details))

	# Dependency dialog (`@mod.dependencies.downloadall`) delegates to the browser.
	var missing := _string_list(details.get("missing_dependencies", []))
	if not missing.is_empty():
		# code-instantiated: dependency import row only when deps are unmet.
		var dependency := MindWidgets.button(_t("@mod.dependencies.downloadall"))
		dependency.pressed.connect(_download_dependencies.bind(missing))
		box.add_child(dependency)

	# code-instantiated: per-mod action row inside the details block.
	var actions := HBoxContainer.new()
	var folder := MindWidgets.button(_t("@mods.openfolder"))
	folder.pressed.connect(_open_path.bind(str(details.get("config_folder", ""))))
	actions.add_child(folder)
	var repo := str(details.get("repo", ""))
	if not repo.is_empty():
		var open_repo := MindWidgets.button(_t("@mods.github.open"))
		open_repo.pressed.connect(_open_url.bind("https://github.com/%s" % repo))
		actions.add_child(open_repo)
	box.add_child(actions)

	_list.add(box).grow_x_axis().pad(6)
	_list.row()


func _toggle_details(mod_name: String) -> void:
	_open_details = "" if _open_details == mod_name else mod_name
	_rebuild_list(_search.text if is_instance_valid(_search) else "")


## `mods.setEnabled(mod, !mod.enabled()); setup();` (`MindMods` emits
## `mods_changed`, which re-runs `_refresh`).
func _set_enabled(mod_name: String, enabled: bool) -> void:
	var mods := _mods()
	if mods != null:
		mods.call("set_enabled", mod_name, enabled)
	_refresh()


## `@mods.restore` (`ModsDialog.java:56-59`): re-enable every failed mod and
## rebuild the list (a restart applies it; `setEnabled` sets requiresReload).
func _restore_failed() -> void:
	var mods := _mods()
	if mods == null:
		return
	for entry in mods.call("list"):
		if entry is Dictionary and bool(entry.get("failed", false)):
			mods.call("set_enabled", str(entry.get("name", "")), true)
	_refresh()


## `ui.showConfirm("@confirm", "@mod.remove.confirm", ...)`.
func _confirm_delete(mod_name: String) -> void:
	_pending_delete = mod_name
	var ui := _ui()
	if ui != null:
		ui.call("show_confirm", _t("@mod.remove.confirm"))
	else:
		_on_confirm(true)


func _on_confirm(confirmed: bool) -> void:
	if _pending_delete.is_empty():
		return
	var mod_name := _pending_delete
	_pending_delete = ""
	if not confirmed:
		return
	var mods := _mods()
	if mods == null:
		return
	var result: Dictionary = mods.call("remove_mod", mod_name)
	if not bool(result.get("ok", false)):
		_show_error(str(result.get("error", _t("@mod.delete.error"))))
	_updates.erase(mod_name)


## `@mod.import` (`:145-198`): `new BaseDialog("@mod.import")` with a
## `Tex.button` table of two 300x70 `Styles.flatt` buttons (`@mod.import.file`
## / Icon.file, `@mod.import.github` / Icon.github, margin 12) and
## `addCloseButton()`. Built once as an overlay above the mods dialog (the port
## has no runtime `BaseDialog`; a manifest dialog would be a new UI ABI entry).
func _build_import_overlay() -> void:
	# code-instantiated: the transient import chooser has no manifest entry.
	_import_overlay = Control.new()
	_import_overlay.name = "ImportDialog"
	_import_overlay.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_import_overlay.mouse_filter = Control.MOUSE_FILTER_STOP
	_import_overlay.visible = false
	add_child(_import_overlay)

	# Its own `Styles.defaultDialog` stage background (black9) over the mods dialog.
	var dim := ColorRect.new()
	dim.color = MindStyles.BLACK_9
	dim.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	dim.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_import_overlay.add_child(dim)

	# Same insets as the fill-parent dialog shell (`fullDialog`: 8 px sides and
	# bottom, title flush with the top).
	var frame := MarginContainer.new()
	frame.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	frame.add_theme_constant_override("margin_left", 8)
	frame.add_theme_constant_override("margin_right", 8)
	frame.add_theme_constant_override("margin_top", 0)
	frame.add_theme_constant_override("margin_bottom", 8)
	frame.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_import_overlay.add_child(frame)
	var layout := VBoxContainer.new()
	layout.add_theme_constant_override("separation", 8)
	layout.mouse_filter = Control.MOUSE_FILTER_IGNORE
	frame.add_child(layout)

	# Title row + 3 px accent rule (`BaseDialog.java:22-25`).
	var title := Label.new()
	title.text = _t("@mod.import")
	title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	title.add_theme_color_override("font_color", MindStyles.ACCENT)
	layout.add_child(title)
	var rule := ColorRect.new()
	rule.color = MindStyles.ACCENT
	rule.set_anchors_preset(Control.PRESET_BOTTOM_WIDE)
	rule.offset_top = -3.0
	rule.offset_bottom = 0.0
	rule.mouse_filter = Control.MOUSE_FILTER_IGNORE
	title.add_child(rule)

	var center := CenterContainer.new()
	center.size_flags_vertical = Control.SIZE_EXPAND_FILL
	center.mouse_filter = Control.MOUSE_FILTER_IGNORE
	layout.add_child(center)
	# `dialog.cont.table(Tex.button, t -> { t.margin(12f); ... })`.
	var panel := PanelContainer.new()
	var panel_box := _pane_box(Color.BLACK, PANE_OUTLINE_GRAY)
	panel_box.content_margin_left = 12.0
	panel_box.content_margin_right = 12.0
	panel_box.content_margin_top = 12.0
	panel_box.content_margin_bottom = 12.0
	panel.add_theme_stylebox_override("panel", panel_box)
	center.add_child(panel)
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 0)
	panel.add_child(column)
	var file_icon := Label.new()
	file_icon.text = char(ICON_FILE_CODE)
	MindIcons.apply_icon_font(file_icon)
	file_icon.add_theme_font_size_override("font_size", 24)
	file_icon.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	file_icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var file_button := _flat_button("@mod.import.file", file_icon)
	file_button.pressed.connect(_import_file)
	column.add_child(file_button)
	var github_button := _flat_button("@mod.import.github", MindWidgets.glyph("github", 24))
	github_button.pressed.connect(_import_github)
	column.add_child(github_button)

	# `dialog.addCloseButton()`: `@back` (Icon.left) 210x64.
	var bar := HBoxContainer.new()
	bar.alignment = BoxContainer.ALIGNMENT_CENTER
	bar.mouse_filter = Control.MOUSE_FILTER_IGNORE
	layout.add_child(bar)
	var back := MindWidgets.button("")
	back.custom_minimum_size = Vector2(210.0, 64.0)
	apply_pane_button(back, PANE_OUTLINE_GRAY)
	build_button_content(back, MindWidgets.glyph("left", 20), _t("@back"))
	back.pressed.connect(_hide_import_dialog)
	bar.add_child(back)


## `Styles.flatt` (`Styles.java:149-156`): `up = black`, `over`/`down = flatOver`;
## `t.defaults().size(300f, 70f)`, `.margin(12f)`.
func _flat_button(key: String, icon: Control) -> Button:
	# code-instantiated: chooser entries (see `_build_import_overlay`).
	var button := Button.new()
	button.custom_minimum_size = Vector2(300.0, 70.0)
	button.focus_mode = Control.FOCUS_NONE
	var up := _flat(Color.BLACK)
	var over := _flat(FLAT_OVER)
	button.add_theme_stylebox_override("normal", up)
	button.add_theme_stylebox_override("hover", over)
	button.add_theme_stylebox_override("pressed", over)
	button.add_theme_stylebox_override("focus", StyleBoxEmpty.new())
	build_button_content(button, icon, _t(key))
	return button


func _show_import_dialog() -> void:
	if _import_overlay == null:
		return
	_import_overlay.visible = true
	_import_overlay.move_to_front()


func _hide_import_dialog() -> void:
	if _import_overlay != null:
		_import_overlay.visible = false


## `@mod.import.file`: `dialog.hide()` then `FileChooser.open("zip", "jar")
## .submitMulti(...)` (native chooser; fallback is the plan-14 `file_chooser`
## dialog owned by plan 22's native-first path).
func _import_file() -> void:
	_hide_import_dialog()
	var mods := _mods()
	var start := str(mods.call("mods_dir")) if mods != null else "user://mods"
	DisplayServer.file_dialog_show(
		_t("@mod.import.file"), start, "",
		false, DisplayServer.FILE_DIALOG_MODE_OPEN_FILES,
		PackedStringArray(["*.zip", "*.jar"]), _on_files_chosen
	)


func _on_files_chosen(status: bool, paths: PackedStringArray, _filter_index: int) -> void:
	if not status:
		return
	for path in paths:
		_import_path(path)


func _import_path(path: String) -> void:
	var mods := _mods()
	if mods == null:
		return
	var result: Dictionary = mods.call("import_mod", path)
	if not bool(result.get("ok", false)):
		_show_error(str(result.get("error", _t("@mod.delete.error"))))
	_refresh()


## `@mod.import.github`: `dialog.hide()` then `ui.showTextInput(
## "@mod.import.github", "", 64, Core.settings.getString("lastmod", ""), ...)`.
func _import_github() -> void:
	_hide_import_dialog()
	var ui := _ui()
	if ui == null:
		return
	var last := str(ui.call("settings_get_text", "lastmod", ""))
	_pending_github = true
	ui.call("show_text_input", _t("@mod.import.github"), "", 64, last, false, false)


## `text_input_result` is emitted from inside `MindUi.resolve_text_input`
## (`&mut` bound), so the import (which writes `lastmod` through `MindUi`) runs
## deferred. Empty text = cancelled.
func _on_text_input(text: String) -> void:
	if not _pending_github:
		return
	_pending_github = false
	if text.strip_edges().is_empty():
		return
	call_deferred("_github_import", text)


## Text cleanup + `lastmod` + `githubImportMod` (`:176-191`). The HTTP path
## lives in the browser (`install_repos` resolves a cached listing's matching
## release, else the repository's latest), which runs without being shown,
## like Java's import that stays on the mods dialog.
func _github_import(raw: String) -> void:
	var text := raw.strip_edges().replace(" ", "")
	if text.begins_with("https://github.com/"):
		text = text.substr("https://github.com/".length())
	if text.is_empty():
		return
	var ui := _ui()
	if ui != null:
		ui.call("settings_put_text", "lastmod", text)
	var browser := get_node_or_null("/root/Spine/Ui/UiRoot/DialogLayer/mod_browser")
	if browser != null and browser.has_method("install_repos"):
		browser.call("install_repos", [text])
	elif ui != null:
		ui.call("open_dialog", "mod_browser", JSON.stringify({"repo": text}))


## `browser.show()`.
func _open_browser() -> void:
	var ui := _ui()
	if ui != null:
		ui.call("open_dialog", "mod_browser", "{}")


## `githubImportMod(mod.getRepo(), mod.isJava(), null, false)` via the browser.
func _update_mod(mod_name: String) -> void:
	var mods := _mods()
	var repo := ""
	if mods != null:
		var details: Dictionary = mods.call("details", mod_name)
		repo = str(details.get("repo", ""))
	if repo.is_empty():
		return
	var ui := _ui()
	if ui != null:
		ui.call("open_dialog", "mod_browser", JSON.stringify({"repo": repo}))


func _update_all() -> void:
	var repos: Array = []
	var mods := _mods()
	for mod_name in _updates.keys():
		if mods == null:
			break
		var details: Dictionary = mods.call("details", str(mod_name))
		var repo := str(details.get("repo", ""))
		if not repo.is_empty():
			repos.append(repo)
	var browser := get_node_or_null("/root/Spine/Ui/UiRoot/DialogLayer/mod_browser")
	if browser != null and browser.has_method("install_repos"):
		browser.call("install_repos", repos)
	else:
		_open_browser()


func _download_dependencies(dependencies: PackedStringArray) -> void:
	var names: Array = []
	for mod_name in dependencies:
		names.append(str(mod_name))
	var browser := get_node_or_null("/root/Spine/Ui/UiRoot/DialogLayer/mod_browser")
	_open_browser()
	if browser != null and browser.has_method("install_repos"):
		browser.call("install_repos", names)


## `Core.app.openFolder(modDirectory.absolutePath())`.
func _open_folder() -> void:
	var mods := _mods()
	if mods != null:
		_open_path(str(mods.call("mods_dir")))


## `Core.app.openURI(modGuideURL)`.
func _open_guide() -> void:
	_open_url(MOD_GUIDE_URL)


func _open_path(path: String) -> void:
	if not path.is_empty():
		OS.shell_open(path)


func _open_url(url: String) -> void:
	OS.shell_open(url)


func _show_errors() -> void:
	var mods := _mods()
	if mods == null:
		return
	var lines := PackedStringArray()
	for entry in mods.call("errors"):
		if entry is Dictionary:
			lines.append("%s: %s" % [str(entry.get("name", "")), str(entry.get("details", ""))])
	_show_error("\n".join(lines))


## Deferred: callers include `MindUi` signal handlers (`confirm_result`), where
## calling back into the `&mut`-bound `MindUi` would panic.
func _show_error(text: String) -> void:
	call_deferred("_show_error_now", text)


func _show_error_now(text: String) -> void:
	var ui := _ui()
	if ui == null:
		return
	if ui.has_method("show_text"):
		ui.call("show_text", _t("@mods"), text)
	else:
		ui.call("show_info", text)


func _on_mod_error(mod_name: String, details: String) -> void:
	_show_error("%s: %s" % [mod_name, details])


func _on_import_progress(_path: String, ratio: float) -> void:
	if not is_instance_valid(_progress):
		return
	_progress.visible = ratio < 1.0
	_progress.value = ratio


func _display_name(entry: Dictionary) -> String:
	return str(entry.get("display_name", entry.get("displayName", entry.get("name", ""))))


## `Strings.stripColors`: drops `[color]` / `[#rrggbb]` / `[]` markup tags.
func _strip_colors(text: String) -> String:
	var regex := RegEx.new()
	regex.compile("\\[(#?[0-9a-fA-F]{6,8}|[a-zA-Z_]*)\\]")
	return regex.sub(text, "", true)


func _mods() -> Node:
	return get_node_or_null("/root/MindMods")


func _ui() -> Node:
	return get_node_or_null("/root/MindUi")


## Bundle lookup with `{0}`-style substitution (`Bundle.format`).
func _fmt(key: String, args: Array) -> String:
	var result := _t(key)
	for index in args.size():
		result = result.replace("{%d}" % index, str(args[index]))
	return result


func _string_list(value: Variant) -> PackedStringArray:
	var result := PackedStringArray()
	if value is Array:
		for item in value:
			result.append(str(item))
	return result


func _connect_mods() -> void:
	var mods := _mods()
	if mods != null:
		if mods.has_signal("mods_changed") and not mods.is_connected("mods_changed", _refresh):
			mods.connect("mods_changed", _refresh)
		if mods.has_signal("mod_error") and not mods.is_connected("mod_error", Callable(self, "_on_mod_error")):
			mods.connect("mod_error", Callable(self, "_on_mod_error"))
		if mods.has_signal("mod_import_progress") and not mods.is_connected("mod_import_progress", Callable(self, "_on_import_progress")):
			mods.connect("mod_import_progress", Callable(self, "_on_import_progress"))
	var ui := _ui()
	if ui != null and ui.has_signal("confirm_result") and not ui.is_connected("confirm_result", Callable(self, "_on_confirm")):
		ui.connect("confirm_result", Callable(self, "_on_confirm"))
	if ui != null and ui.has_signal("text_input_result") and not ui.is_connected("text_input_result", Callable(self, "_on_text_input")):
		ui.connect("text_input_result", Callable(self, "_on_text_input"))
