## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/MapGenerateDialog.java (plan 19 M4 §3.9).
##
## UI-only: the filter-card list with per-filter `FilterOption` editors
## (sliders/block pickers/toggles/logic code), the live preview with the
## draggable mirror axis, the `@edit` clipboard menu and the `@add` picker.
## The filter JSON, the option descriptors, the block predicates and the
## preview pixels are owned by Rust (`MindEditor.filters_json`,
## `filter_options_json`, `filter_types_json`, `default_ores_json`,
## `filter_blocks_json`, `generate_preview_texture`, `apply_filters`).

extends MindDialog

## `MapGenerateDialog.rebuildFilters`: `table(...).width(280f)`.
const CARD_WIDTH := 280.0
## Header icon buttons (`t.defaults().size(42f)`).
const HEADER_ICON := 42.0
## `MapGenerateDialog.hit`: `Scl.scl(16f)` preview-space pick radius.
const HIT_PAD := 16.0
## `FilterOption.BlockOption` picker: 10 cells per row (`i % 10 == 0`).
const PICKER_COLUMNS := 10
const PICKER_CELL := 44.0
## `Pal.gray` (`Pal.java:82`) header tint.
const HEADER_GRAY := Color("454545")
## `Pal.darkerGray` (`Pal.java:60`) mirror line.
const DARKER_GRAY := Color(0.2, 0.2, 0.2)

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"
## `applied == true` (editor menu) shows Apply; false (map info) writes back.
@export var applied := true

var _editor: Node = null
## Working filters (`MapGenerateDialog.filters`), the raw `genfilters` objects.
var _filters: Array = []
## `filter_options_json` rows (option descriptors + resolved values), tag order.
var _rows: Array = []
var _preview: TextureRect = null
var _preview_box: Control = null
var _mirror_overlay: MirrorOverlay = null
var _loading: Control = null
var _filter_table: VBoxContainer = null
## Mirror filter being dragged (`MapGenerateDialog.dragging`), -1 for none.
var _dragging := -1
var _last_drag := 0
## Code-built modal overlay (edit menu / add picker / block picker).
var _overlay: Control = null
## Guards stale async preview refreshes (`updateQueued`).
var _preview_token := 0
## Target of a pending standalone logic-code edit (`pendingCode`).
var _pending_code_index := -1
var _pending_code_field := ""


## Preview overlay that draws the mirror axes and forwards drag events
## (`MapGenerateDialog`'s anonymous `InputListener` on the `BorderImage`).
class MirrorOverlay extends Control:
	var owner_dialog: Node = null

	func _draw() -> void:
		if owner_dialog != null:
			owner_dialog.call("_draw_mirrors", self)

	func _gui_input(event: InputEvent) -> void:
		if owner_dialog != null:
			owner_dialog.call("_mirror_input", event)


func _ready() -> void:
	set_title_key("@editor.generate")
	should_pause = false
	super._ready()
	_editor = get_node_or_null(editor_path)
	clear_buttons()
	# `buttons.defaults().size(180f, 64f).pad(2f)`; button order matches Java:
	# back, apply (applied mode), randomize, edit, add.
	add_button(_t("@back"), _close_pressed, "left", 180.0)
	if applied:
		add_button(_t("@editor.apply"), _apply, "ok", 180.0)
	add_button(_t("@editor.randomize"), _randomize_all, "refresh", 180.0)
	add_button(_t("@edit"), _show_edit_menu, "edit", 180.0)
	add_button(_t("@add"), _show_add_menu, "add", 180.0)


## `shown(this::setup)`: read the tag filters, then build preview + cards.
func shown() -> void:
	if _editor == null:
		return
	_rows = _read_rows()
	_filters = _read_filters()
	if not applied:
		# `MapInfoDialog.java:88-90`: randomize on open so the filters are not
		# all the same seed.
		for filter in _filters:
			if filter is Dictionary:
				(filter as Dictionary)["seed"] = _random_seed()
		_write(_filters)
	_build()
	_refresh_preview()


## `MapInfoDialog.java:93-96`: reset the seed to 0 so it is not written.
func hidden() -> void:
	_close_overlay()
	if applied or _editor == null:
		return
	for filter in _filters:
		if filter is Dictionary:
			(filter as Dictionary)["seed"] = 0
	_write(_filters)


## `setup()`: `cont.table(t -> { preview | filter pane }).grow()`.
func _build() -> void:
	if cont == null:
		return
	for child in cont.get_children():
		cont.remove_child(child)
		child.queue_free()
	_preview = null
	_preview_box = null
	_mirror_overlay = null
	_loading = null
	_filter_table = null
	var root := content_table()
	var columns := HBoxContainer.new()
	columns.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	columns.size_flags_vertical = Control.SIZE_EXPAND_FILL
	columns.add_theme_constant_override("separation", 10)
	root.add(columns).grow_x_axis().grow_y_axis()
	columns.add_child(_build_preview())
	columns.add_child(_build_filter_list())
	_rebuild_filters()


## The left preview column (`t.stack(preview, ...).uniformX().grow().padRight(10)`).
func _build_preview() -> Control:
	# code-instantiated: preview box + mirror overlay + loading stack.
	var box := Control.new()
	box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	box.size_flags_vertical = Control.SIZE_EXPAND_FILL
	box.custom_minimum_size = Vector2(320.0, 320.0)
	box.clip_contents = true
	box.resized.connect(_sync_mirror_overlay)
	_preview_box = box
	_preview = TextureRect.new()
	_preview.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	_preview.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	_preview.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_preview.mouse_filter = Control.MOUSE_FILTER_IGNORE
	box.add_child(_preview)
	_mirror_overlay = MirrorOverlay.new()
	_mirror_overlay.owner_dialog = self
	_mirror_overlay.mouse_filter = Control.MOUSE_FILTER_IGNORE
	box.add_child(_mirror_overlay)
	# `t.stack(preview, new Stack(){ black8 + Icon.refresh })`: the loading
	# overlay (`generating && !updateEditorOnChange && dragging == null`).
	_loading = Control.new()
	_loading.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_loading.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var dim := ColorRect.new()
	dim.color = Color(0.0, 0.0, 0.0, 0.8)
	dim.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	dim.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_loading.add_child(dim)
	var spinner := MindWidgets.glyph("refresh", 48)
	spinner.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	spinner.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	spinner.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	spinner.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_loading.add_child(spinner)
	_loading.visible = false
	box.add_child(_loading)
	return box


## The right filter pane (`t.pane(p -> filterTable = ...).scrollX(false)`).
func _build_filter_list() -> Control:
	var scroll := ScrollContainer.new()
	scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	scroll.custom_minimum_size = Vector2(CARD_WIDTH + 40.0, 320.0)
	# code-instantiated: the card list count is the filter stack size.
	var list := VBoxContainer.new()
	list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	list.add_theme_constant_override("separation", 6)
	scroll.add_child(list)
	_filter_table = list
	return scroll


## `rebuildFilters()`: one `Tex.pane` card per filter (or `@filters.empty`).
func _rebuild_filters() -> void:
	if _filter_table == null:
		return
	for child in _filter_table.get_children():
		_filter_table.remove_child(child)
		child.queue_free()
	if _filters.is_empty():
		# The bundle text carries `[lightgray]` Arc markup: translate it (the
		# plain lookup printed the tag literally, L01 T3 godot/ed-generate.png).
		var empty := MindWidgets.label(_tm("@filters.empty"))
		empty.custom_minimum_size.x = 200.0
		_filter_table.add_child(empty)
		return
	for index in _filters.size():
		_filter_table.add_child(_filter_card(index))
	_sync_mirror_overlay()
	if _mirror_overlay != null:
		_mirror_overlay.queue_redraw()


## One filter card: the gray header row (`filter.name()` + action icons) and
## the option rows (`for(FilterOption option : filter.options())`).
func _filter_card(index: int) -> Control:
	var row: Dictionary = _rows[index] if index < _rows.size() and _rows[index] is Dictionary else {}
	var card := PanelContainer.new()
	card.add_theme_stylebox_override("panel", _pane_box(Color.BLACK, PANE_OUTLINE_GRAY, 3.0))
	card.custom_minimum_size.x = CARD_WIDTH
	card.size_flags_horizontal = Control.SIZE_FILL
	# code-instantiated: one card per filter; the stack is runtime-sized.
	var body := VBoxContainer.new()
	body.add_theme_constant_override("separation", 0)
	card.add_child(body)
	body.add_child(_card_header(index, row))
	var options := VBoxContainer.new()
	options.add_theme_constant_override("separation", 2)
	for option_value in _options_of(row):
		if option_value is Dictionary:
			options.add_child(_option_row(option_value, index))
	body.add_child(options)
	return card


## `c.table(Tex.whiteui, t -> { name + refresh/up/down/copy/remove })`.
func _card_header(index: int, row: Dictionary) -> Control:
	var header := PanelContainer.new()
	var style := StyleBoxFlat.new()
	style.bg_color = HEADER_GRAY
	style.content_margin_left = 4.0
	style.content_margin_right = 4.0
	style.content_margin_top = 2.0
	style.content_margin_bottom = 2.0
	header.add_theme_stylebox_override("panel", style)
	# code-instantiated: header buttons are per-filter action rows.
	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", 0)
	header.add_child(line)
	var name := str(row.get("name", ""))
	var label := MindWidgets.label(_t("@filter." + name))
	label.custom_minimum_size.x = 100.0
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	line.add_child(label)
	var spacer := MindWidgets.space(0.0, 0.0)
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	line.add_child(spacer)
	line.add_child(
		_card_icon("refresh", "@editor.randomize", _randomize_index.bind(index))
	)
	if index > 0:
		line.add_child(_card_icon("up-open", "@editor.moveup", _move.bind(index, -1)))
	if index < _filters.size() - 1:
		line.add_child(_card_icon("down-open", "@editor.movedown", _move.bind(index, 1)))
	line.add_child(_card_icon("copy", "@editor.copy", _copy_filter.bind(index)))
	line.add_child(_card_icon("cancel", "@waves.remove", _remove_filter.bind(index)))
	return header


## One 42px header icon button (`t.defaults().size(42f).padLeft(-5f)`).
func _card_icon(icon_name: String, tooltip_key: String, handler: Callable) -> Button:
	var button := MindWidgets.glyph_button(icon_name, icon_name, "clearNonei")
	button.custom_minimum_size = Vector2(HEADER_ICON, HEADER_ICON)
	button.size_flags_horizontal = Control.SIZE_SHRINK_END
	button.tooltip_text = _t(tooltip_key)
	button.pressed.connect(handler)
	return button


## `FilterOption.build` dispatch: slider / block / toggle / custom.
func _option_row(option: Dictionary, index: int) -> Control:
	match str(option.get("kind", "")):
		"slider":
			return _slider_row(option, index)
		"block":
			return _block_row(option, index)
		"toggle":
			return _toggle_row(option, index)
		"custom":
			return _custom_row(option, index)
		_:
			return MindWidgets.label("?")


## `SliderOption.build`: the label/value row stacked on the slider; the change
## fires on release (`updateEditorOnChange` is false, `Vars.java:143`).
func _slider_row(option: Dictionary, index: int) -> Control:
	var field := str(option.get("field", ""))
	var value := _value_float(index, field)
	# code-instantiated: one slider cell per descriptor (runtime row).
	var holder := Control.new()
	holder.custom_minimum_size = Vector2(0.0, 46.0)
	holder.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var slider := MindWidgets.slider(
		float(option.get("min", 0.0)), float(option.get("max", 1.0)), float(option.get("step", 0.00001))
	)
	slider.name = "FilterSlider_%s" % field
	slider.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	slider.offset_top = 10.0
	slider.offset_bottom = -10.0
	slider.value = value
	holder.add_child(slider)
	var overlay := HBoxContainer.new()
	overlay.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	overlay.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var label := MindWidgets.label(_t("@filter.option." + str(option.get("name", ""))))
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	overlay.add_child(label)
	var readout := MindWidgets.label(_auto_fixed(value))
	readout.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	readout.mouse_filter = Control.MOUSE_FILTER_IGNORE
	overlay.add_child(readout)
	holder.add_child(overlay)
	slider.value_changed.connect(func(changed: float) -> void:
		readout.text = _auto_fixed(changed))
	slider.drag_ended.connect(func(changed: bool) -> void:
		if changed:
			_commit_field(index, field, slider.value, bool(option.get("integer", false))))
	return holder


## `BlockOption.build`: the icon button (click -> picker) + the option label.
func _block_row(option: Dictionary, index: int) -> Control:
	var field := str(option.get("field", ""))
	# code-instantiated: the block cell is data-driven (filter option value).
	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", 6)
	var button := _block_button(_value_string(index, field))
	button.name = "BlockOption_%s" % field
	button.custom_minimum_size = Vector2(48.0, 48.0)
	button.pressed.connect(_open_block_picker.bind(index, option))
	button.gui_input.connect(func(event: InputEvent) -> void:
		if not (event is InputEventMouseButton) or not (event as InputEventMouseButton).pressed:
			return
		var mouse := event as InputEventMouseButton
		if mouse.button_index == MOUSE_BUTTON_MIDDLE:
			# `button.clicked(KeyCode.mouseMiddle, ...)`: copy the block name.
			DisplayServer.clipboard_set(_value_string(index, field))
			_toast("@copied")
		elif mouse.button_index == MOUSE_BUTTON_RIGHT:
			# `button.clicked(KeyCode.mouseRight, ...)`: paste a valid block.
			var pasted := DisplayServer.clipboard_get().strip_edges()
			if _valid_block(str(option.get("predicate", "anyOptional")), pasted):
				_pick_block(index, field, pasted))
	line.add_child(button)
	line.add_child(MindWidgets.label(_t("@filter.option." + str(option.get("name", "")))))
	return line


## The current value's `uiIcon` button (`b.image(supplier.get().uiIcon)`).
func _block_button(name: String) -> Button:
	if name == "air":
		var air := MindWidgets.glyph_button("cancel", _t("@none"), "clearNonei")
		air.expand_icon = true
		return air
	var button := MindWidgets.image_button_first(
		PackedStringArray(["block-%s-ui" % name, "block-%s-full" % name]), "clearNonei"
	)
	if button.icon == null:
		button.text = name
	button.expand_icon = true
	button.add_theme_constant_override("icon_max_width", 36)
	button.tooltip_text = _localized_block(name)
	return button


## `ToggleOption.build`: the centered check row.
func _toggle_row(option: Dictionary, index: int) -> Control:
	var field := str(option.get("field", ""))
	var check := MindWidgets.check(_t("@filter.option." + str(option.get("name", ""))), _value_bool(index, field))
	check.name = "FilterToggle_%s" % field
	check.custom_minimum_size.y = 40.0
	check.toggled.connect(func(pressed_state: bool) -> void:
		_commit_field(index, field, pressed_state, false))
	return check


## The `LogicFilter` code option: `Icon.pencil` button + the option label.
func _custom_row(option: Dictionary, index: int) -> Control:
	# code-instantiated: custom actions are descriptor-driven (logic code).
	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", 6)
	var action := str(option.get("action", ""))
	var button := MindWidgets.glyph_button("pencil", _t("@edit"), "clearNonei")
	button.name = "CustomOption_%s" % action
	button.custom_minimum_size = Vector2(48.0, 48.0)
	button.pressed.connect(_open_custom_action.bind(action, index, str(option.get("field", ""))))
	line.add_child(button)
	line.add_child(MindWidgets.label(_t("@filter.option." + str(option.get("name", "")))))
	return line


## `ui.logic.show(code, null, true, code -> ...)`: the shared logic editor in
## its standalone mode (context `filter` + `code`); it echoes the edited code
## back through `code_edited`.
func _open_custom_action(action: String, index: int, field: String) -> void:
	if action != "logiccode":
		return
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("open_dialog"):
		return
	var logic := _dialog("logic")
	if logic != null and logic.has_signal("code_edited") \
			and not logic.is_connected("code_edited", _on_filter_code_edited):
		logic.connect("code_edited", _on_filter_code_edited)
	_pending_code_index = index
	_pending_code_field = field
	ui.call(
		"open_dialog",
		"logic",
		JSON.stringify({"filter": true, "privileged": true, "code": _value_string(index, field)})
	)


func _on_filter_code_edited(code: String) -> void:
	if _pending_code_index < 0:
		return
	_commit_field(_pending_code_index, _pending_code_field, code, false)
	_pending_code_index = -1


## `@editor.randomize` toolbar button: `for(filter : filters) filter.randomize()`.
func _randomize_all() -> void:
	for filter in _filters:
		if filter is Dictionary:
			(filter as Dictionary)["seed"] = _random_seed()
	_write(_filters)
	_refresh_preview()


## The per-card refresh (`filter.randomize(); update()`).
func _randomize_index(index: int) -> void:
	if index < 0 or index >= _filters.size() or not (_filters[index] is Dictionary):
		return
	(_filters[index] as Dictionary)["seed"] = _random_seed()
	_write(_filters)
	_refresh_preview()


## The per-card up/down swap (`filters.swap(idx, idx +/- 1)`).
func _move(index: int, direction: int) -> void:
	var other := index + direction
	if index < 0 or other < 0 or index >= _filters.size() or other >= _filters.size():
		return
	var tmp: Variant = _filters[index]
	_filters[index] = _filters[other]
	_filters[other] = tmp
	_write(_filters)
	_rows = _read_rows()
	_rebuild_filters()
	_refresh_preview()


## The per-card copy (`copy.randomize(); filters.insert(idx + 1, copy)`).
func _copy_filter(index: int) -> void:
	if index < 0 or index >= _filters.size() or not (_filters[index] is Dictionary):
		return
	var duplicate: Dictionary = (_filters[index] as Dictionary).duplicate(true)
	duplicate["seed"] = _random_seed()
	_filters.insert(index + 1, duplicate)
	_write(_filters)
	_rows = _read_rows()
	_rebuild_filters()
	_refresh_preview()


## The per-card remove (`filters.remove(filter)`).
func _remove_filter(index: int) -> void:
	if index < 0 or index >= _filters.size():
		return
	_filters.remove_at(index)
	_write(_filters)
	_rows = _read_rows()
	_rebuild_filters()
	_refresh_preview()


## `buttons.button("@editor.apply", Icon.ok, () -> ui.loadAnd(() -> { apply();
## hide(); }))` (`MapGenerateDialog.java:79-85`).
func _apply() -> void:
	var ui := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui != null and ui.has_method("load_and"):
		ui.call("load_and", _apply_now)
	else:
		_apply_now()


func _apply_now() -> void:
	if _editor != null:
		_write(_filters)
		if _editor.has_method("apply_filters"):
			_editor.call("apply_filters")
	_close_pressed()


## `showAdd()`: every `Maps.allFilterTypes` entry (post filters hidden in
## applied mode) plus the `@filter.defaultores` row.
func _show_add_menu() -> void:
	var types: Array = _read_types()
	# code-instantiated: picker rows are data-driven (filter type registry).
	var grid := GridContainer.new()
	grid.columns = 3
	grid.add_theme_constant_override("h_separation", 4)
	grid.add_theme_constant_override("v_separation", 4)
	for row_value in types:
		var row: Dictionary = row_value
		if bool(row.get("post", false)) and applied:
			continue
		var button := _icon_text_button(str(row.get("icon", "")), _t("@filter." + str(row.get("name", ""))))
		button.custom_minimum_size = Vector2(195.0, 56.0)
		button.pressed.connect(_add_filter.bind(str(row.get("class", ""))))
		grid.add_child(button)
	var default_ores := _icon_text_button("", _t("@filter.defaultores"), "refresh")
	default_ores.custom_minimum_size = Vector2(195.0, 56.0)
	default_ores.pressed.connect(_add_default_ores)
	grid.add_child(default_ores)
	_show_overlay(_t("@add"), grid, 640.0, 380.0)


## `BaseDialog` overlay Escape handling: closes the code-built sub-dialog
## first (`MindUi` closes the whole dialog otherwise).
func _unhandled_input(event: InputEvent) -> void:
	if _overlay == null:
		return
	if event is InputEventKey and (event as InputEventKey).pressed \
			and (event as InputEventKey).keycode == KEY_ESCAPE:
		_close_overlay()
		get_viewport().set_input_as_handled()


## `showAdd()`'s `filter.randomize(); filters.add(filter);`.
func _add_filter(class_tag: String) -> void:
	if class_tag.is_empty():
		return
	_filters.append({"class": class_tag, "seed": _random_seed()})
	_write(_filters)
	_rows = _read_rows()
	_rebuild_filters()
	_close_overlay()
	_refresh_preview()


## `@filter.defaultores`: `maps.addDefaultOres(filters)`.
func _add_default_ores() -> void:
	if _editor == null or not _editor.has_method("default_ores_json"):
		return
	var parsed: Variant = JSON.parse_string(str(_editor.call("default_ores_json")))
	if parsed is Array:
		for filter in parsed:
			_filters.append(filter)
		_write(_filters)
		_rows = _read_rows()
		_rebuild_filters()
	_close_overlay()
	_refresh_preview()


## `buttons.button("@edit", ...)`: `BaseDialog("@editor.export")` with the
## clipboard/clear/reset rows.
func _show_edit_menu() -> void:
	# code-instantiated: menu rows depend on the clipboard/applied flags.
	var rows := VBoxContainer.new()
	rows.custom_minimum_size.x = 320.0
	_rows_menu_button(rows, "copy.clipboard", "copy", _copy_clipboard, false)
	_rows_menu_button(
		rows, "load.clipboard", "download", _load_clipboard,
		DisplayServer.clipboard_get().strip_edges().is_empty()
	)
	_rows_menu_button(rows, "clear", "cancel", _clear_filters, false)
	var row_count := 3
	if not applied:
		_rows_menu_button(rows, "settings.reset", "refresh", _reset_filters, false)
		row_count += 1
	# 60px per row (`@settings.reset` only in map-info mode).
	_show_overlay(_t("@editor.export"), rows, 320.0, float(row_count) * 60.0 + 4.0)


func _rows_menu_button(
	rows: VBoxContainer, key: String, icon: String, handler: Callable, disabled: bool
) -> void:
	var button := MindWidgets.icon_button(icon, _t("@" + key), "flatt", BoxContainer.ALIGNMENT_BEGIN)
	button.custom_minimum_size = Vector2(280.0, 60.0)
	button.disabled = disabled
	button.pressed.connect(handler)
	rows.add_child(button)


## `Core.app.setClipboardText(JsonIO.write(filters))`.
func _copy_clipboard() -> void:
	DisplayServer.clipboard_set(JSON.stringify(_filters))
	_close_overlay()
	_toast("@copied")


## `filters.set(JsonIO.read(Seq.class, Core.app.getClipboardText()))`.
func _load_clipboard() -> void:
	var text := DisplayServer.clipboard_get().strip_edges()
	var parsed: Variant = JSON.parse_string(text)
	if not (parsed is Array):
		_close_overlay()
		_toast("@save.import.invalid", "error")
		return
	_filters = parsed
	_write(_filters)
	_rows = _read_rows()
	_close_overlay()
	_rebuild_filters()
	_refresh_preview()


## `filters.clear(); rebuildFilters(); update()`.
func _clear_filters() -> void:
	_filters = []
	_write(_filters)
	_rows = []
	_close_overlay()
	_rebuild_filters()
	_refresh_preview()


## `filters.set(maps.readFilters(""))` (the `@settings.reset` row, map-info
## mode only).
func _reset_filters() -> void:
	if _editor != null and _editor.has_method("set_filters_json"):
		_editor.call("set_filters_json", "")
	_close_overlay()
	_rows = _read_rows()
	_filters = _read_filters()
	_rebuild_filters()
	_refresh_preview()


## `FilterOption.BlockOption` picker: the block grid filtered by the predicate
## (`content.blocks()` + `filter.get(block)`; 10 icons per row).
func _open_block_picker(index: int, option: Dictionary) -> void:
	var predicate := str(option.get("predicate", "anyOptional"))
	var rows: Array = _block_rows(predicate)
	# code-instantiated: picker cells are content rows.
	var grid := GridContainer.new()
	grid.columns = PICKER_COLUMNS
	grid.add_theme_constant_override("h_separation", 4)
	grid.add_theme_constant_override("v_separation", 4)
	for row_value in rows:
		var row: Dictionary = row_value
		var name := str(row.get("name", ""))
		var cell := _block_button(name)
		cell.custom_minimum_size = Vector2(PICKER_CELL, PICKER_CELL)
		if name == "air":
			cell.tooltip_text = _t("@none")
		cell.pressed.connect(_pick_block.bind(index, str(option.get("field", "")), name))
		grid.add_child(cell)
	_show_overlay(_t("@filter.option." + str(option.get("name", ""))), grid, 900.0, 420.0)


## Sets one option value and regenerates (`BlockOption` consumer + `changed`).
func _pick_block(index: int, field: String, name: String) -> void:
	_commit_field(index, field, name, false)
	_close_overlay()
	# The button icon reflects the new value (`update(i -> ...setRegion)`).
	_rebuild_filters()
	_refresh_preview()


## Writes one field into the working filters + the tag, then `update()`s.
func _commit_field(index: int, field: String, value: Variant, integer: bool) -> void:
	if index < 0 or index >= _filters.size() or not (_filters[index] is Dictionary):
		return
	var stored: Variant = value
	if integer and value is float:
		stored = roundi(value)
	(_filters[index] as Dictionary)[field] = stored
	# Keep the resolved-value mirror in sync (`option.changed = this::update`).
	if index < _rows.size() and _rows[index] is Dictionary:
		var values: Variant = (_rows[index] as Dictionary).get("values")
		if values is Dictionary:
			(values as Dictionary)[field] = stored
	_write(_filters)
	_refresh_preview()


## `update()`: regenerate the preview from the editor grid + tag filters.
func _refresh_preview() -> void:
	if _preview == null or _mirror_overlay == null or _editor == null:
		return
	if not _editor.has_method("generate_preview_texture"):
		return
	_preview_token += 1
	var token := _preview_token
	_set_loading(true)
	# Yield one frame so the `generating` overlay is visible (Java posts the
	# pixmap apply; the port runs the generator synchronously).
	await get_tree().process_frame
	if token != _preview_token or not is_instance_valid(_preview) or not is_inside_tree():
		return
	var texture: Variant = _editor.call("generate_preview_texture")
	if texture is Texture2D:
		_preview.texture = texture
	_set_loading(false)
	_sync_mirror_overlay()
	if _mirror_overlay != null:
		_mirror_overlay.queue_redraw()


func _set_loading(visible_now: bool) -> void:
	if _loading != null:
		_loading.visible = visible_now


## `BorderImage` fit: the drawn image rectangle inside the preview box.
func _image_rect() -> Rect2:
	if _preview == null or _preview.texture == null or _preview_box == null:
		return Rect2()
	var box := _preview_box.size
	var texture_size := _preview.texture.get_size()
	if texture_size.x <= 0.0 or texture_size.y <= 0.0:
		return Rect2()
	var scale := minf(box.x / texture_size.x, box.y / texture_size.y)
	var drawn := texture_size * scale
	return Rect2((box - drawn) * 0.5, drawn)


## The mirror overlay covers exactly the drawn image (`Filter.draw(image)`).
func _sync_mirror_overlay() -> void:
	if _mirror_overlay == null:
		return
	var rect := _image_rect()
	_mirror_overlay.position = rect.position
	_mirror_overlay.size = rect.size
	_mirror_overlay.mouse_filter = (
		Control.MOUSE_FILTER_STOP if _has_mirror() else Control.MOUSE_FILTER_IGNORE
	)


func _has_mirror() -> bool:
	for filter in _filters:
		if filter is Dictionary and str((filter as Dictionary).get("class", "")) == "mirror":
			return true
	return false


## `MirrorFilter.draw(image)`: the axis line (8px darkerGray + 3px accent) and
## the pivot (accent 6 + darkerGray 4), clipped to the image (`clipHalfLine`).
func _draw_mirrors(overlay: Control) -> void:
	var rect := Rect2(Vector2.ZERO, overlay.size)
	if rect.size.x <= 0.0 or rect.size.y <= 0.0:
		return
	for filter in _filters:
		if not (filter is Dictionary) or str((filter as Dictionary).get("class", "")) != "mirror":
			continue
		var axis := filter as Dictionary
		var px := rect.position.x + clampf(float(axis.get("axisX", 0.5)), 0.0, 1.0) * rect.size.x
		var py := rect.position.y + clampf(float(axis.get("axisY", 0.5)), 0.0, 1.0) * rect.size.y
		var dir := _axis_dir(int(axis.get("angle", 45)))
		var reach := maxf(rect.size.x, rect.size.y) * 3.0
		var segment := _clip_segment(
			rect, Vector2(px, py) - dir * reach, Vector2(px, py) + dir * reach
		)
		if segment.size() == 2:
			overlay.draw_line(segment[0], segment[1], DARKER_GRAY, 8.0)
			overlay.draw_line(segment[0], segment[1], MindStyles.ACCENT, 3.0)
		overlay.draw_circle(Vector2(px, py), 6.0, MindStyles.ACCENT)
		overlay.draw_circle(Vector2(px, py), 4.0, DARKER_GRAY)


## Liang-Barsky segment clip (`clipHalfLine` keeps the line inside the image).
func _clip_segment(rect: Rect2, p0: Vector2, p1: Vector2) -> Array:
	var d := p1 - p0
	var t0 := 0.0
	var t1 := 1.0
	var p := [-d.x, d.x, -d.y, d.y]
	var q := [
		p0.x - rect.position.x,
		rect.end.x - p0.x,
		p0.y - rect.position.y,
		rect.end.y - p0.y,
	]
	for i in 4:
		if absf(p[i]) < 0.000001:
			if q[i] < 0.0:
				return []
			continue
		var r: float = q[i] / p[i]
		if p[i] < 0.0:
			t0 = maxf(t0, r)
		else:
			t1 = minf(t1, r)
		if t0 > t1:
			return []
	return [p0 + d * t0, p0 + d * t1]


## `MapView`-style axis direction: `dragAxis.trnsExact(angle - 90f, 1f)`.
func _axis_dir(angle: int) -> Vector2:
	var radians := deg_to_rad(float(angle) - 90.0)
	return Vector2(cos(radians), sin(radians))


## `hit(x, y)`: the closest mirror axis within the 16px pad (or the single
## mirror when the pointer is inside the padded image).
func _hit_mirror(position: Vector2) -> int:
	if _mirror_overlay == null:
		return -1
	var rect := Rect2(Vector2.ZERO, _mirror_overlay.size)
	if rect.size.x <= 0.0 or rect.size.y <= 0.0:
		return -1
	var best := -1
	var dst := INF
	var amount := 0
	for index in _filters.size():
		var filter: Variant = _filters[index]
		if not (filter is Dictionary) or str((filter as Dictionary).get("class", "")) != "mirror":
			continue
		var axis := filter as Dictionary
		var px := rect.position.x + clampf(float(axis.get("axisX", 0.5)), 0.0, 1.0) * rect.size.x
		var py := rect.position.y + clampf(float(axis.get("axisY", 0.5)), 0.0, 1.0) * rect.size.y
		var dir := _axis_dir(int(axis.get("angle", 45)))
		var ndst: float = absf((position.x - px) * dir.y - (position.y - py) * dir.x)
		if ndst < dst:
			dst = ndst
			best = index
		amount += 1
	if best >= 0 and dst <= HIT_PAD:
		return best
	if amount == 1 and best >= 0 and rect.grow(HIT_PAD).has_point(position):
		return best
	return -1


## `drag(x, y)`: the normalized axis position from the pointer.
func _drag_mirror(position: Vector2) -> void:
	if _dragging < 0 or _dragging >= _filters.size():
		return
	if _mirror_overlay == null:
		return
	var rect := Rect2(Vector2.ZERO, _mirror_overlay.size)
	if rect.size.x <= 0.0 or rect.size.y <= 0.0:
		return
	var axis: Dictionary = _filters[_dragging]
	axis["axisX"] = clampf((position.x - rect.position.x) / rect.size.x, 0.0, 1.0)
	axis["axisY"] = clampf((position.y - rect.position.y) / rect.size.y, 0.0, 1.0)
	_write(_filters)
	if _mirror_overlay != null:
		_mirror_overlay.queue_redraw()


## The preview listener: press selects, motion drags (throttled `update()` at
## 33 ms like `dragUpdate`), release commits.
func _mirror_input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		var mouse := event as InputEventMouseButton
		if mouse.button_index != MOUSE_BUTTON_LEFT:
			return
		if mouse.pressed:
			_dragging = _hit_mirror(mouse.position)
			if _dragging >= 0:
				_drag_mirror(mouse.position)
				_last_drag = Time.get_ticks_msec()
				_refresh_preview()
		elif _dragging >= 0:
			_drag_mirror(mouse.position)
			_dragging = -1
			_refresh_preview()
	elif event is InputEventMouseMotion and _dragging >= 0:
		_drag_mirror(event.position)
		var now := Time.get_ticks_msec()
		if now - _last_drag > 33:
			_last_drag = now
			_refresh_preview()


## Code-built modal overlay for the edit menu / add picker / block picker
## (`BaseDialog` children in Java).
func _show_overlay(title: String, content: Control, min_w: float, min_h: float) -> void:
	_close_overlay()
	content.name = "OverlayContent"
	# code-instantiated: sub-dialogs are code-built overlays over the dialog.
	var layer := Control.new()
	layer.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	layer.mouse_filter = Control.MOUSE_FILTER_STOP
	var scrim := ColorRect.new()
	scrim.color = Color(0.0, 0.0, 0.0, 0.7)
	scrim.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	scrim.mouse_filter = Control.MOUSE_FILTER_STOP
	scrim.gui_input.connect(func(event: InputEvent) -> void:
		if event is InputEventMouseButton and (event as InputEventMouseButton).pressed:
			_close_overlay())
	layer.add_child(scrim)
	var center := CenterContainer.new()
	center.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	center.mouse_filter = Control.MOUSE_FILTER_IGNORE
	layer.add_child(center)
	var panel := PanelContainer.new()
	panel.add_theme_stylebox_override("panel", _pane_box(Color(0, 0, 0, 0.95), PANE_OUTLINE_GRAY, 4.0))
	center.add_child(panel)
	var margin := MarginContainer.new()
	margin.add_theme_constant_override("margin_left", 12)
	margin.add_theme_constant_override("margin_right", 12)
	margin.add_theme_constant_override("margin_top", 8)
	margin.add_theme_constant_override("margin_bottom", 8)
	panel.add_child(margin)
	var box := VBoxContainer.new()
	box.add_theme_constant_override("separation", 6)
	margin.add_child(box)
	var title_row := HBoxContainer.new()
	var title_label := MindWidgets.label(title)
	title_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	title_row.add_child(title_label)
	var close := MindWidgets.glyph_button("cancel", "X", "clearNonei")
	close.custom_minimum_size = Vector2(36.0, 36.0)
	close.pressed.connect(_close_overlay)
	title_row.add_child(close)
	box.add_child(title_row)
	var scroll := ScrollContainer.new()
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	scroll.custom_minimum_size = Vector2(min_w, min_h)
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.add_child(content)
	box.add_child(scroll)
	add_child(layer)
	_overlay = layer


func _close_overlay() -> void:
	if _overlay == null:
		return
	var layer := _overlay
	_overlay = null
	if is_instance_valid(layer):
		remove_child(layer)
		layer.queue_free()


## The `@edit` sub-dialog used `dialog.hide()`; mirrors `MindUi` lookups.
func _dialog(name: String) -> Node:
	var ui_root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui_root == null or not ui_root.has_method("dialog"):
		return null
	return ui_root.call("dialog", name)


func _toast(key: String, kind: String = "info") -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("toast"):
		ui.call("toast", _t(key), kind)


## `Strings.autoFixed(value, 2)`.
func _auto_fixed(value: float) -> String:
	return "%.2f" % value


func _random_seed() -> int:
	return randi() % 999999999


## `filter_options_json` rows (`filter.options()` + getters).
func _read_rows() -> Array:
	if _editor == null or not _editor.has_method("filter_options_json"):
		return []
	var parsed: Variant = JSON.parse_string(str(_editor.call("filter_options_json")))
	return parsed if parsed is Array else []


## The working filters: the `genfilters` tag (`filters_json`), falling back to
## the class defaults when the tag is `{}`/unset (`Maps.readFilters("")`).
func _read_filters() -> Array:
	if _editor == null or not _editor.has_method("filters_json"):
		return []
	var parsed: Variant = JSON.parse_string(str(_editor.call("filters_json")))
	if parsed is Array:
		return parsed
	var out: Array = []
	for row_value in _rows:
		if not (row_value is Dictionary):
			continue
		var values: Variant = (row_value as Dictionary).get("values", {})
		if values is Dictionary:
			out.append((values as Dictionary).duplicate(true))
	return out


func _write(filters: Array) -> void:
	if _editor != null and _editor.has_method("set_filters_json"):
		_editor.call("set_filters_json", JSON.stringify(filters))


## `Maps.allFilterTypes` rows for the add picker (`filter_types_json`).
func _read_types() -> Array:
	if _editor == null or not _editor.has_method("filter_types_json"):
		return []
	var parsed: Variant = JSON.parse_string(str(_editor.call("filter_types_json")))
	return parsed if parsed is Array else []


## The predicate's selectable blocks (`filter_blocks_json`).
func _block_rows(predicate: String) -> Array:
	if _editor == null or not _editor.has_method("filter_blocks_json"):
		return []
	var parsed: Variant = JSON.parse_string(str(_editor.call("filter_blocks_json", predicate)))
	return parsed if parsed is Array else []


func _valid_block(predicate: String, name: String) -> bool:
	if name.is_empty():
		return false
	for row_value in _block_rows(predicate):
		if row_value is Dictionary and str((row_value as Dictionary).get("name", "")) == name:
			return true
	return false


func _options_of(row: Dictionary) -> Array:
	var options: Variant = row.get("options", [])
	return options if options is Array else []


func _value_of(index: int, field: String) -> Variant:
	if index >= 0 and index < _rows.size() and _rows[index] is Dictionary:
		var values: Variant = (_rows[index] as Dictionary).get("values", {})
		if values is Dictionary and (values as Dictionary).has(field):
			return (values as Dictionary)[field]
	if index >= 0 and index < _filters.size() and _filters[index] is Dictionary:
		return (_filters[index] as Dictionary).get(field)
	return null


func _value_float(index: int, field: String) -> float:
	var value: Variant = _value_of(index, field)
	if value is float or value is int:
		return float(value)
	return 0.0


func _value_bool(index: int, field: String) -> bool:
	return bool(_value_of(index, field))


func _value_string(index: int, field: String) -> String:
	var value: Variant = _value_of(index, field)
	return str(value) if value != null else ""


## `content.localizedName` (`Core.bundle`) for a block.
func _localized_block(name: String) -> String:
	return _t("@block.%s.name" % name)


## An icon + left-aligned label button (the `@add` picker rows).
func _icon_text_button(icon_region: String, text_value: String, glyph: String = "") -> Button:
	var button := Button.new()
	button.theme_type_variation = "defaultt"
	# code-instantiated: icon+label picker rows are data-driven (filter types).
	var row := HBoxContainer.new()
	row.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	row.offset_left = 8.0
	row.offset_right = -4.0
	row.add_theme_constant_override("separation", 6)
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	button.add_child(row)
	if not glyph.is_empty():
		row.add_child(MindWidgets.glyph(glyph, 24))
	elif not icon_region.is_empty():
		var icon := MindWidgets.content_icon(icon_region, "block", 28.0)
		if icon != null:
			row.add_child(icon)
	var label := MindWidgets.label(text_value)
	label.autowrap_mode = TextServer.AUTOWRAP_OFF
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_LEFT
	label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(label)
	return button
