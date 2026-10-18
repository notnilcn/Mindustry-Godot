## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/logic/LogicDialog.java, LCanvas.java,
##         LStatement.java (plan 13 M4, I03 T7-T9).
##
## The mlog editor shell: a scrolling statement column (`LCanvas`) plus the
## `@back`/`@edit`/`@variables`/`@add` button row (160x64, `LogicDialog.setup`).
## Each statement is an `LCanvas.StatementElem`: a category-tinted `whitePane`
## frame with a category-coloured header (`localizedName()`, address, `+`/copy/
## delete) over the statement's field row; the column is `getTargetWidth()`
## wide and centred. Jump curves are `JumpCurve`'s trapezoids with
## `DragLayout.setJumpHeights` lane prediction. The Add (`showAddDialog`) and
## Edit (`@editor.export`) menus are nested full-screen `BaseDialog`s.
##
## Load/save goes through `MindLogic.parse_statements` / `write_statements` and
## `SimHost.logic_configure_code` (`LogicBlock.showEditDialog`).

extends MindDialog

## `LogicDialog.show(code, null, true, consumer)`: `MapGenerateDialog`'s
## `LogicFilter` code option has no world tile; the edited code is echoed back
## through this signal instead of a processor write.
signal code_edited(code: String)

## `LCategory` colours (`Pal`; `LCategory.java:16-22`).
const CATEGORY_COLORS := {
	"io": Color8(0xa0, 0x8a, 0x8a),
	"block": Color8(0xd4, 0x81, 0x6b),
	"operation": Color8(0x87, 0x7b, 0xad),
	"control": Color8(0x6b, 0xb2, 0xb2),
	"unit": Color8(0xc7, 0xb5, 0x9d),
	"world": Color8(0x6b, 0x84, 0xd4),
	"unknown": Color8(0x4d, 0x4d, 0x4d),
}
## `LCategory` icons (`Icon.logicSmall`, `effectSmall`, `settingsSmall`,
## `rotateSmall`, `unitsSmall`, `terrainSmall`; `unknown` has none).
const CATEGORY_ICONS := {
	"io": "logic",
	"block": "effect",
	"operation": "settings",
	"control": "rotate",
	"unit": "units",
	"world": "terrain",
}
## `LExecutor.maxInstructions`.
const MAX_INSTRUCTIONS := 1000
## `Pal.place` (jump curves/buttons while hovered).
const PLACE := Color8(0x63, 0x35, 0xf8)
## `Pal.darkishGray` (Add dialog category headers).
const DARKISH_GRAY := Color(0.3, 0.3, 0.3, 1.0)
## `Styles.flatOver` (`#454545`).
const FLAT_OVER := Color8(0x45, 0x45, 0x45)
## `DragLayout.space`.
const ELEMENT_SPACE := 10
## `JumpButton` cell size (`JumpStatement.build`: `.size(30)`).
const JUMP_BUTTON_SIZE := 30.0

var _x := 0
var _y := 0
var _privileged := false
var _code := ""
## Editor target (`MapProcessorsDialog`'s name button /
## `LogicBuild.showEditDialog(true)`): read/write through `MindEditor`.
var _editor_mode := false
## Standalone `show(code, null, true, consumer)` mode (`MapGenerateDialog`'s
## `LogicFilter` code option): no tile; the code is echoed via `code_edited`.
var _filter_mode := false
## `LogicDialog.forceRestart`: `@logic.restart` saves even when unchanged.
var _force_restart := false
## `[{name, category, fields: [String]}]`.
var _statements: Array = []
## The statement column (`LCanvas.DragLayout`), centred in `_scroll`.
var _list: VBoxContainer = null
var _scroll: ScrollContainer = null
var _modified := false
## Bottom button row (`LogicDialog.setup`).
var _variables_button: Button = null
var _add_button: Button = null
## Variables panel state (`LogicDialog`'s 15-frame refresh).
var _vars_popup: PopupPanel = null
var _vars_rows: Array = []
var _vars_timer := 0
## Statement element nodes (parallel to `_statements`; `LCanvas.StatementElem`).
var _row_nodes: Array = []
## `index -> JumpButton` for the jump elements (`LCanvas.JumpButton`).
var _jump_buttons: Dictionary = {}
## Jump index whose button is under the pointer (`listener.isOver()`).
var _jump_over := -1
## Jump-curve + insertion overlay (`LCanvas.DragLayout.draw`, `JumpCurve`).
var _overlay: Control = null
## `JumpCurve.predHeight` per jump index (`DragLayout.setJumpHeights`).
var _pred_heights: Dictionary = {}
## `JumpCurve.uiHeight` (exponentially smoothed) per jump index.
var _curve_heights: Dictionary = {}
## `DragLayout.tmpOccupiers2` / `tmpBits2`: shared scratch buffers of the
## recursive height prediction (Java aliases them across recursion levels).
var _tmp_occupiers: Array = []
var _tmp_bits: Dictionary = {}
## Drag-reorder state (`LCanvas.DragLayout`): dragged element, pointer origin
## and the current insertion index.
var _drag_index := -1
var _drag_active := false
var _drag_start := Vector2.ZERO
var _drop_index := -1
## Jump-target selection state (`LCanvas.JumpButton` touch drag).
var _jump_selecting := -1
var _jump_hover := -1
var _jump_pointer := Vector2.ZERO
## The nested `BaseDialog` (Add / Edit menu) currently stacked on top.
var _sub_dialog: Control = null
## `@clear` waits for `ui.showConfirm("@logic.clear.confirm", ...)`.
var _pending_clear := false
## `registered name -> MindLogic.statement_meta` cache.
var _meta_cache: Dictionary = {}
## `region name -> Texture2D` (`_region`).
var _region_cache: Dictionary = {}
## `Tex.logicNode` (jump buttons and curve heads).
var _node_texture: Texture2D = null
## Arc colour-markup matcher for plain-text tooltips.
static var _markup_regex: RegEx = null


func _ready() -> void:
	should_pause = true
	full_dialog = true
	super._ready()
	# `LogicDialog` calls `clearChildren()`: the "logic" title row never shows.
	set_title_text("")
	_build()
	# `LCanvas.DragLayout`/`JumpCurve` redraw every frame while visible.
	set_process(true)
	var viewport := get_viewport()
	if viewport != null and not viewport.size_changed.is_connected(_on_resize):
		viewport.size_changed.connect(_on_resize)


func _build() -> void:
	if cont != null:
		cont.size_flags_vertical = Control.SIZE_EXPAND_FILL
	if body != null:
		body.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	# code-instantiated: the canvas holder clips the jump overlay to the pane
	# (`JumpCurve` draws inside `canvas.pane`).
	var holder := Control.new()
	holder.clip_contents = true
	holder.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	holder.size_flags_vertical = Control.SIZE_EXPAND_FILL
	holder.custom_minimum_size = Vector2(0, 320)
	if cont != null:
		cont.add_child(holder)
	else:
		add_child(holder)
	# code-instantiated: the canvas pane owns the runtime-sized statement list.
	_scroll = ScrollContainer.new()
	_scroll.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	holder.add_child(_scroll)
	# code-instantiated: `t.center()` inside the pane centres the column.
	var center := CenterContainer.new()
	center.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	center.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_scroll.add_child(center)
	# code-instantiated: one element per statement; the count is the program
	# length (`DragLayout`, `space = 10`).
	_list = VBoxContainer.new()
	_list.add_theme_constant_override("separation", ELEMENT_SPACE)
	_list.custom_minimum_size = Vector2(_target_width(), 0)
	center.add_child(_list)

	# code-instantiated: the jump-arrow and drop-line overlay sits above the
	# statement list (`LCanvas.DragLayout.draw` + `JumpCurve`); it never takes
	# mouse events.
	_overlay = Control.new()
	_overlay.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_overlay.mouse_filter = Control.MOUSE_FILTER_IGNORE
	holder.add_child(_overlay)
	_overlay.draw.connect(_draw_overlay)

	# `LogicDialog.setup`: `buttons.defaults().size(160f, 64f)`.
	_bottom_button("left", "@back", _close)
	_bottom_button("edit", "@edit", _open_edit_menu)
	_variables_button = _bottom_button("menu", "@variables", _open_variables_panel)
	_add_button = _bottom_button("add", "@add", _open_add_dialog.bind(-1))


## One `buttons.button(text, icon, listener)` cell at 160x64.
func _bottom_button(icon: String, key: String, listener: Callable) -> Button:
	# code-instantiated: the editor's button row is built by `setup()`.
	var button := MindWidgets.icon_button(icon, _t(key))
	button.custom_minimum_size = Vector2(160, 64)
	button.pressed.connect(listener)
	if buttons != null:
		buttons.add_child(button)
	return button


## Sets an `icon_button`'s label (the glyph row's label child, else `text`).
func _set_button_text(button: Button, text: String) -> void:
	if button == null:
		return
	if button.get_child_count() > 0:
		var row := button.get_child(0)
		for child in row.get_children():
			if child is Label and not (child as Label).text.is_empty() \
					and child.get_index() > 0:
				(child as Label).text = text
				return
	button.text = text


## `LCanvas.getTargetWidth`: 410 when compact, else 95% of the screen minus 80
## clamped to 400..1200 (`isCompact`: width < 900 * 1.2).
func _target_width() -> float:
	var width := get_viewport_rect().size.x
	if width < 900.0 * 1.2:
		return 410.0
	return clampf(width * 0.95 - 80.0, 400.0, 1200.0)


## `LogicDialog.onResize`: the column follows `getTargetWidth`.
func _on_resize() -> void:
	if _list == null:
		return
	var width := _target_width()
	_list.custom_minimum_size.x = width
	for node in _row_nodes:
		(node as Control).custom_minimum_size.x = width


## `LogicDialog.show`: loads the processor's program into the statement list.
## `context.editor` targets an editor world tile (`MindEditor`,
## `MapProcessorsDialog`'s name button -> `showEditDialog(true)`);
## `context.filter` is the standalone `show(code, null, true, consumer)` mode
## (the map-generate `LogicFilter` code option); otherwise the live sim
## (`LogicBlock` config).
func shown() -> void:
	_x = int(_context.get("x", 0))
	_y = int(_context.get("y", 0))
	_editor_mode = bool(_context.get("editor", false))
	_filter_mode = bool(_context.get("filter", false))
	_force_restart = false
	_close_sub_dialog()
	_on_resize()
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null:
		return
	if _filter_mode:
		_privileged = bool(_context.get("privileged", true))
		_code = str(_context.get("code", ""))
	elif _editor_mode:
		var editor := get_node_or_null("/root/Spine/MindEditor")
		if editor == null:
			return
		var state: Variant = editor.call("logic_get_state", _x, _y)
		if state is Dictionary:
			_privileged = bool(state.get("privileged", false))
			_code = str(state.get("code", ""))
		else:
			_code = ""
	else:
		var host := get_node_or_null("/root/Spine/SimHost")
		if host == null:
			return
		var state: Variant = host.call("logic_get_state", _x, _y)
		if state is Dictionary:
			_privileged = bool(state.get("privileged", false))
		_code = str(host.call("logic_get_code", _x, _y))
	var parsed: Variant = logic.call("parse_statements", _code, _privileged)
	_statements = parsed if parsed is Array else []
	# `LCanvas.load`: `statements.truncate(LExecutor.maxInstructions)`.
	if _statements.size() > MAX_INSTRUCTIONS:
		_statements.resize(MAX_INSTRUCTIONS)
	_modified = false
	_curve_heights.clear()
	if _scroll != null:
		_scroll.scroll_vertical = 0
	_rebuild_list()
	_update_buttons()


## `LogicDialog.hidden`: `consumer.get(canvas.save())`; the consumer only
## writes back when forced (`@logic.restart`) or the code changed.
func hidden() -> void:
	_close_sub_dialog()
	_pending_clear = false
	if not _modified and not _force_restart:
		return
	_save()


## `shouldShowVariables`: an executor with variables outside the menu state.
## The map editor and the filter/objective code editors run in `State.menu`
## (`MapEditorDialog.java:317`), so they show `@logic.globals`; a live
## processor always has `@counter`.
func _should_show_variables() -> bool:
	if _editor_mode or _filter_mode:
		return false
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null:
		return false
	# `executor.vars.length > 0` (`LogicDialog.java:253`): an empty program has
	# no executor vars, so it shows `@logic.globals`.
	var state: Variant = host.call("logic_get_state", _x, _y)
	if not (state is Dictionary):
		return false
	var vars: Variant = (state as Dictionary).get("vars", [])
	return vars is Array and not (vars as Array).is_empty()


## The `variables` button's `update` (`@variables` vs `@logic.globals`) and
## the `@add` button's `disabled` (`>= maxInstructions`).
func _update_buttons() -> void:
	_set_button_text(
		_variables_button,
		_t("@variables") if _should_show_variables() else _t("@logic.globals")
	)
	if _add_button != null:
		_add_button.disabled = _statements.size() >= MAX_INSTRUCTIONS


func _rebuild_list() -> void:
	if _list == null:
		return
	var keep_scroll := _scroll.scroll_vertical if _scroll != null else 0
	for child in _list.get_children():
		_list.remove_child(child)
		child.queue_free()
	_row_nodes.clear()
	_jump_buttons.clear()
	_jump_over = -1
	for index in _statements.size():
		var row: Variant = _statements[index]
		if not (row is Dictionary):
			continue
		var panel := _statement_row(index, row)
		_list.add_child(panel)
		_row_nodes.append(panel)
	_compute_jump_heights()
	if _scroll != null:
		_scroll.set_deferred("scroll_vertical", keep_scroll)
	_update_buttons()
	if _overlay != null:
		_overlay.queue_redraw()


## Registry metadata for a statement (`statement_meta`, cached).
func _meta(name: String) -> Dictionary:
	if _meta_cache.has(name):
		return _meta_cache[name]
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null:
		return {}
	var info: Dictionary = {}
	var meta: Variant = logic.call("statement_meta", name)
	if meta is Dictionary:
		info = meta as Dictionary
	_meta_cache[name] = info
	return info


## `LStatement.typeName()`: the Java class name without `Statement`
## (`InvalidStatement` -> `Invalid`, `OperationStatement` -> `Operation`).
func _type_name(name: String) -> String:
	var variant := str(_meta(name).get("rust_variant", ""))
	return name if variant.is_empty() else variant


## `LStatement.statementKey()`: `typeName().toLowerCase()`.
func _statement_key(name: String) -> String:
	return _type_name(name).to_lower()


## `LStatement.localizedName()` (`LStatement.java:371-374`):
## `instruction.<statementKey>` with `name()` (`insertSpaces(typeName)`) as the
## fallback; the raw name when the `logiclocalization` setting is off.
func _localized_name(name: String) -> String:
	var plain := _insert_spaces(_type_name(name))
	if not _logic_localization():
		return plain
	var key := "instruction." + _statement_key(name)
	var resolved := _t(key)
	return plain if resolved == key else resolved


## `LStatement.logicLocalization()`: `Core.settings.getBool("logiclocalization", true)`.
func _logic_localization() -> bool:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("settings_get_bool"):
		return true
	return bool(ui.call("settings_get_bool", "logiclocalization"))


## `Strings.insertSpaces`: a space before every non-leading upper-case letter.
static func _insert_spaces(text: String) -> String:
	var out := ""
	for i in text.length():
		var ch := text[i]
		if i > 0 and ch != ch.to_lower() and ch == ch.to_upper():
			out += " "
		out += ch
	return out


## Arc colour markup (`[accent]`, `[#rrggbb]`, `[]`) removed for plain-text
## tooltips; `[[` stays a literal bracket.
static func _plain_markup(text: String) -> String:
	if _markup_regex == null:
		_markup_regex = RegEx.new()
		_markup_regex.compile("\\[(#[0-9a-fA-F]{3,8}|[a-zA-Z]*)\\]")
	var escaped := text.replace("[[", "\u0001")
	return _markup_regex.sub(escaped, "", true).replace("\u0001", "[")


## `LCanvas.tooltip(cell, key)`: the bundle text when the key exists.
func _bundle_tooltip(key: String) -> String:
	var lkey := key.to_lower().replace(" ", "")
	var resolved := _t(lkey)
	return "" if resolved == lkey else _plain_markup(resolved)


## The header title: `localizedName()`, plus `-> <dest>` for a set jump
## (`JumpStatement.build`'s title label update).
func _header_text(row: Dictionary) -> String:
	var name := str(row.get("name", "noop"))
	var text := _localized_name(name)
	var dest := _jump_target(row)
	if name == "jump" and dest >= 0:
		text += " -> %d" % dest
	return text


## One `LCanvas.StatementElem` (`LCanvas.java:417-510`): a `Tex.whitePane`
## frame tinted with the category colour over a drop shadow and a 30% black
## fill (`draw`), the 38 px `Tex.whiteui` header (name, growX spacer, address,
## `+`/copy/delete `logici` buttons) and the statement's field row.
func _statement_row(index: int, row: Dictionary) -> Control:
	var name := str(row.get("name", "noop"))
	var category := str(row.get("category", "unknown"))
	var color: Color = CATEGORY_COLORS.get(category, CATEGORY_COLORS["unknown"])
	var full := _statements.size() >= MAX_INSTRUCTIONS
	# code-instantiated: one element per statement; the count is the program
	# length, not scene-authorable.
	var elem := PanelContainer.new()
	elem.custom_minimum_size = Vector2(_target_width(), 0)
	elem.mouse_filter = Control.MOUSE_FILTER_PASS
	elem.add_theme_stylebox_override("panel", _shadow_box())
	var frame := PanelContainer.new()
	frame.mouse_filter = Control.MOUSE_FILTER_PASS
	frame.add_theme_stylebox_override("panel", _white_pane_box(color))
	elem.add_child(frame)
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 0)
	column.mouse_filter = Control.MOUSE_FILTER_PASS
	frame.add_child(column)

	var header := PanelContainer.new()
	header.custom_minimum_size = Vector2(0, 38)
	header.mouse_filter = Control.MOUSE_FILTER_STOP
	header.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	var header_box := StyleBoxFlat.new()
	header_box.bg_color = color
	header_box.content_margin_left = 6
	header_box.content_margin_right = 6
	header_box.content_margin_top = 6
	header_box.content_margin_bottom = 6
	header.add_theme_stylebox_override("panel", header_box)
	column.add_child(header)
	var box := HBoxContainer.new()
	box.add_theme_constant_override("separation", 0)
	box.mouse_filter = Control.MOUSE_FILTER_IGNORE
	header.add_child(box)
	box.add_child(_outline_label(_header_text(row), color))
	box.add_child(_gap(8))
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
	box.add_child(spacer)
	box.add_child(_outline_label(str(index), color))
	box.add_child(_gap(8))
	# "taken from foo's client": `showAddDialog(index + 1)`.
	var add := _logici_button("add", _open_add_dialog.bind(index + 1))
	add.disabled = full
	add.tooltip_text = _t("@add")
	box.add_child(add)
	box.add_child(_gap(6))
	var copy := _logici_button("copy", _copy_statement.bind(index))
	copy.disabled = full
	copy.tooltip_text = _t("@copy")
	box.add_child(copy)
	box.add_child(_gap(6))
	var delete := _logici_button("cancel", _delete_statement.bind(index))
	delete.tooltip_text = _t("@delete")
	box.add_child(delete)
	# `StatementElem`'s header drag: left-drag reorders, middle-click copies.
	header.gui_input.connect(_header_input.bind(index, header))

	# `add(t).pad(4).padTop(2).left().grow()` with `marginLeft(4)` (+
	# `marginRight(4)` when wrapping); `marginBottom(7)` sits on the frame.
	var body_margin := MarginContainer.new()
	body_margin.mouse_filter = Control.MOUSE_FILTER_PASS
	var wraps := bool(_meta(name).get("use_wrapping", true))
	body_margin.add_theme_constant_override("margin_left", 8)
	body_margin.add_theme_constant_override("margin_right", 8 if wraps else 4)
	body_margin.add_theme_constant_override("margin_top", 2)
	body_margin.add_theme_constant_override("margin_bottom", 4)
	column.add_child(body_margin)
	body_margin.add_child(_field_row(index, row, color, wraps))
	return elem


## Atlas region lookup cached per dialog (`MindAssets.find_region`).
func _region(region_name: String) -> Texture2D:
	if _region_cache.has(region_name):
		return _region_cache[region_name]
	var texture := MindWidgets.icon_texture(region_name)
	if texture != null:
		_region_cache[region_name] = texture
	return texture


## A sizing-only header cell (`padRight`) that never takes pointer input.
func _gap(width: float) -> Control:
	var gap := MindWidgets.space(width, 0)
	gap.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return gap


## `StatementElem.draw`: `Fill.dropShadow` (10 px, 0.9 alpha) under a 30%
## black fill.
func _shadow_box() -> StyleBoxFlat:
	var box := StyleBoxFlat.new()
	box.bg_color = Color(0, 0, 0, 0.3)
	box.shadow_color = Color(0, 0, 0, 0.45)
	box.shadow_size = 10
	return box


## `Tex.whitePane` (5 px white 9-patch border, 12/11 splits) tinted with the
## element colour; `margin(0f)` drops the drawable padding except the
## `marginBottom(7)`.
func _white_pane_box(color: Color) -> StyleBox:
	var texture := _region("white-pane")
	if texture != null:
		var box := StyleBoxTexture.new()
		box.texture = texture
		box.texture_margin_left = 12
		box.texture_margin_top = 12
		box.texture_margin_right = 11
		box.texture_margin_bottom = 11
		box.modulate_color = color
		box.content_margin_left = 0
		box.content_margin_top = 0
		box.content_margin_right = 0
		box.content_margin_bottom = 7
		return box
	var flat := StyleBoxFlat.new()
	flat.draw_center = false
	flat.border_color = color
	flat.border_width_left = 5
	flat.border_width_top = 5
	flat.border_width_right = 5
	flat.border_width_bottom = 5
	flat.content_margin_bottom = 7
	return flat


## `Styles.outlineLabel` tinted `color` (header name/address).
func _outline_label(text: String, color: Color) -> Label:
	# code-instantiated: header labels are rebuilt per statement element.
	var label := Label.new()
	label.text = text
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	label.add_theme_color_override("font_color", color)
	label.add_theme_color_override("font_outline_color", Color.BLACK)
	label.add_theme_constant_override("outline_size", 4)
	return label


## `t.button(Icon.x, Styles.logici, listener).size(24f)`: a black icon, no
## background.
func _logici_button(icon: String, listener: Callable) -> Button:
	# code-instantiated: header buttons are rebuilt per statement element.
	var button := Button.new()
	button.custom_minimum_size = Vector2(24, 24)
	button.focus_mode = Control.FOCUS_NONE
	for state in ["normal", "hover", "pressed", "disabled", "focus", "hover_pressed"]:
		button.add_theme_stylebox_override(state, StyleBoxEmpty.new())
	var glyph := MindWidgets.glyph(icon, 20, Color.BLACK)
	glyph.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	glyph.mouse_filter = Control.MOUSE_FILTER_IGNORE
	button.add_child(glyph)
	button.pressed.connect(listener)
	return button


## `StatementElem`'s header `touchDown`/`touchDragged`/`touchUp`
## (`LCanvas.java:456-494`): left-drag reorders (the drop lands at
## `insertPosition`), middle-click copies.
func _header_input(event: InputEvent, index: int, box: Control) -> void:
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_MIDDLE and event.pressed:
		box.accept_event()
		if _statements.size() < MAX_INSTRUCTIONS:
			_copy_statement(index)
		return
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT:
		if event.pressed:
			_drag_index = index
			_drag_active = false
			_drag_start = event.position
			box.accept_event()
		else:
			box.accept_event()
			var finish := _drag_active
			_drag_index = -1
			_drag_active = false
			if finish:
				_drag_index = index
				_finish_drag()
				_drag_index = -1
			if _overlay != null:
				_overlay.queue_redraw()
		return
	if event is InputEventMouseMotion and _drag_index == index:
		if not _drag_active and event.position.distance_to(_drag_start) > 8.0:
			_drag_active = true
		if _drag_active:
			_drop_index = _drop_index_at(box.get_global_transform() * event.position)
			if _overlay != null:
				_overlay.queue_redraw()
		box.accept_event()


## The per-statement field row (`LStatement.build(Table)`): `MindLogic.editor_layout`
## returns the exact Java item order (labels, "="/"linknum" separators, enum
## selects, and the colour/align/char pencils). Wrapping statements flow like
## `WrapTable`; the rest (`jump`) stay on one row.
func _field_row(index: int, row: Dictionary, color: Color, wraps: bool) -> Control:
	var fields_box: Container = null
	# code-instantiated: the field row is rebuilt per statement on every list
	# rebuild (`LCanvas.StatementElem` calls `st.build(t)`).
	if wraps:
		var flow := HFlowContainer.new()
		flow.add_theme_constant_override("h_separation", 4)
		flow.add_theme_constant_override("v_separation", 2)
		fields_box = flow
	else:
		var hbox := HBoxContainer.new()
		hbox.add_theme_constant_override("separation", 4)
		fields_box = hbox
	fields_box.mouse_filter = Control.MOUSE_FILTER_PASS
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null:
		return fields_box
	var name := str(row.get("name", "noop"))
	var values: Array = row.get("fields", [])
	# `SetRuleStatement.rebuild` normalizes `p1` to `@sharded` for team rules;
	# Java mutates the statement at build time, so the dialog mirrors it.
	var normalized: Variant = logic.call("editor_normalize", name, PackedStringArray(values))
	if normalized is PackedStringArray and normalized != PackedStringArray(values):
		values = Array(normalized)
		row["fields"] = values
		_statements[index] = row
		_modified = true
	var items: Variant = logic.call("editor_layout", name, PackedStringArray(values))
	if not (items is Array):
		return fields_box
	# `LStatements.java:1465-1474`: a permanent status has no `seconds` field.
	if name == "status" and not values.is_empty() and str(values[0]) != "true":
		var host := get_node_or_null("/root/Spine/SimHost")
		if host != null and host.has_method("status_permanent") \
				and bool(host.call("status_permanent", str(values[0]))):
			items = (items as Array).filter(func(entry: Variant) -> bool:
				return not (entry is Dictionary) or str((entry as Dictionary).get("label", "")) != "seconds")
	for entry in items:
		if not (entry is Dictionary):
			continue
		var kind := str((entry as Dictionary).get("kind", "field"))
		var field_index := int((entry as Dictionary).get("index", -1))
		var label := _field_label_text(str((entry as Dictionary).get("label", "")))
		var label_before := bool((entry as Dictionary).get("label_before", true))
		var value := ""
		if field_index >= 0 and field_index < values.size():
			value = str(values[field_index])
		match kind:
			"text":
				if not label.is_empty():
					fields_box.add_child(_field_label(label))
			"field":
				if label_before and not label.is_empty():
					fields_box.add_child(_field_label(label))
				# `LStatement.field`: `Styles.nodeField`, `size(180f, 40f)`,
				# tinted with the element colour; `print`/`format` grow.
				var edit := LineEdit.new()
				edit.text = value
				edit.custom_minimum_size = Vector2(180, 40)
				if bool((entry as Dictionary).get("wide", false)):
					edit.size_flags_horizontal = Control.SIZE_EXPAND_FILL
				_apply_node_field(edit, color)
				edit.tooltip_text = _bundle_tooltip("%s.%s" % [_insert_spaces(_type_name(name)), label])
				edit.text_submitted.connect(_set_field.bind(index, field_index))
				edit.focus_exited.connect(_commit_field.bind(index, field_index, edit))
				fields_box.add_child(edit)
				if not label_before and not label.is_empty():
					fields_box.add_child(_field_label(label))
			"select":
				if label_before and not label.is_empty():
					fields_box.add_child(_field_label(label))
				var button := MindWidgets.button(_select_text(name, field_index, value))
				# `Styles.logict` tinted with the element colour.
				button.theme_type_variation = "logict"
				_apply_logict(button, color)
				button.custom_minimum_size = Vector2(90, 40)
				button.pressed.connect(_open_select.bind(index, field_index, name, button))
				fields_box.add_child(button)
			"color":
				var picker := ColorPickerButton.new()
				picker.custom_minimum_size = Vector2(40, 40)
				picker.color = _parse_color(value)
				picker.color_changed.connect(_set_color.bind(index, field_index))
				fields_box.add_child(picker)
			"align":
				var align_button := MindWidgets.image_button("pencil", "logict")
				_apply_logict(align_button, color)
				align_button.custom_minimum_size = Vector2(40, 40)
				align_button.tooltip_text = _field_label_text("align")
				align_button.pressed.connect(_open_align_select.bind(index, field_index))
				fields_box.add_child(align_button)
			"char":
				var char_button := MindWidgets.image_button("pencil", "logict")
				_apply_logict(char_button, color)
				char_button.custom_minimum_size = Vector2(40, 40)
				char_button.pressed.connect(_open_char_select.bind(index, field_index))
				fields_box.add_child(char_button)
			"jump":
				# `table.add().growX(); table.add(new JumpButton(...)).size(30).right()`.
				var push := Control.new()
				push.size_flags_horizontal = Control.SIZE_EXPAND_FILL
				push.mouse_filter = Control.MOUSE_FILTER_IGNORE
				fields_box.add_child(push)
				fields_box.add_child(_jump_button(index))
	return fields_box


## `LCanvas.JumpButton` (`LCanvas.java:551-624`): a `Tex.logicNode` image,
## white at rest and `Pal.place` while hovered.
func _jump_button(index: int) -> Button:
	# code-instantiated: one jump node per jump statement element.
	var button := Button.new()
	button.custom_minimum_size = Vector2(JUMP_BUTTON_SIZE, JUMP_BUTTON_SIZE)
	button.focus_mode = Control.FOCUS_NONE
	button.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	for state in ["normal", "hover", "pressed", "disabled", "focus", "hover_pressed"]:
		button.add_theme_stylebox_override(state, StyleBoxEmpty.new())
	if _node_texture == null:
		_node_texture = MindWidgets.icon_texture("logic-node")
	var texture := _node_texture
	if texture != null:
		button.icon = texture
		button.expand_icon = true
		button.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER
	else:
		button.text = "o"
	button.add_theme_color_override("icon_normal_color", Color.WHITE)
	button.add_theme_color_override("icon_hover_color", PLACE)
	button.add_theme_color_override("icon_pressed_color", PLACE)
	button.add_theme_color_override("icon_hover_pressed_color", PLACE)
	button.mouse_entered.connect(func() -> void: _jump_over = index)
	button.mouse_exited.connect(
		func() -> void:
			if _jump_over == index:
				_jump_over = -1
	)
	# `JumpButton`: press-drag-release onto a target element.
	button.gui_input.connect(_jump_button_input.bind(index, button))
	_jump_buttons[index] = button
	return button


## `Styles.nodeField` (`underlineWhite` background) tinted with the element
## colour; the text stays white.
func _apply_node_field(edit: LineEdit, color: Color) -> void:
	var normal := _underline_box(color)
	edit.add_theme_stylebox_override("normal", normal)
	edit.add_theme_stylebox_override("focus", _underline_box(color))
	edit.add_theme_stylebox_override("read_only", _underline_box(MindStyles.ARC_GRAY))
	edit.add_theme_color_override("font_color", Color.WHITE)


## `Styles.logict` (`up = underlineWhite`, `over/down = flatOver`) with the
## cell's `.color(table.color)` tint on the background only.
func _apply_logict(button: Button, color: Color) -> void:
	button.add_theme_stylebox_override("normal", _underline_box(color))
	button.add_theme_stylebox_override("hover", _flat_box(FLAT_OVER))
	button.add_theme_stylebox_override("pressed", _flat_box(FLAT_OVER))
	button.add_theme_stylebox_override("hover_pressed", _flat_box(FLAT_OVER))
	button.add_theme_color_override("font_color", Color.WHITE)


## `Tex.underlineWhite`: a 4 px bottom rule.
func _underline_box(color: Color) -> StyleBoxFlat:
	var box := StyleBoxFlat.new()
	box.bg_color = Color(0, 0, 0, 0)
	box.border_color = color
	box.border_width_bottom = 4
	box.content_margin_left = 6
	box.content_margin_right = 6
	box.content_margin_top = 4
	box.content_margin_bottom = 6
	return box


func _flat_box(color: Color) -> StyleBoxFlat:
	var box := StyleBoxFlat.new()
	box.bg_color = color
	box.content_margin_left = 6
	box.content_margin_right = 6
	box.content_margin_top = 4
	box.content_margin_bottom = 4
	return box


## `bundle(desc)`: `name.token.<token>` with the raw token as the fallback.
func _field_label_text(token: String) -> String:
	if token.is_empty():
		return ""
	# The raw token when `logiclocalization` is off (`LStatement.java:158-161`).
	if not _logic_localization():
		return token
	var key := "name.token." + token
	var resolved := _t(key)
	return token if resolved == key else resolved


func _field_label(text: String) -> Label:
	# code-instantiated: one label per field-row item (count is layout-driven).
	var label := Label.new()
	label.text = text
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	label.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	label.custom_minimum_size = Vector2(0, 40)
	return label


## The current option's display text for a select (`LStatement.bundle(Enum)`
## then the symbol/name fallback).
func _select_text(name: String, field_index: int, value: String) -> String:
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null:
		return value
	var options: Variant = logic.call("editor_options", name, field_index)
	if options is Array:
		for option in options:
			if option is Dictionary and str((option as Dictionary).get("name", "")) == value:
				return _option_text(option)
	return value


func _option_text(option: Variant) -> String:
	var text := str((option as Dictionary).get("label", (option as Dictionary).get("name", "")))
	var key := str((option as Dictionary).get("key", ""))
	# `LStatement.bundle(Enum)`/`selectTranslate` (`LStatement.java:163-184`):
	# no translation when `logiclocalization` is off.
	if not key.is_empty() and _logic_localization():
		var resolved := _t(key)
		if resolved != key:
			text = resolved
	return text


## `showSelect`: a popup of `logicTogglet` buttons (`LStatement.java:233-253`).
func _open_select(index: int, field_index: int, name: String, button: Button) -> void:
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null:
		return
	var options: Variant = logic.call("editor_options", name, field_index)
	if not (options is Array):
		return
	var popup := PopupPanel.new()
	# code-instantiated: one transient enum-select popup per open.
	add_child(popup)
	var grid := GridContainer.new()
	grid.columns = 4
	popup.add_child(grid)
	var unit_actions := _unit_actions_enabled()
	for option in options:
		if not (option is Dictionary):
			continue
		var option_name := str((option as Dictionary).get("name", ""))
		# `UnitControlStatement` hides build/deconstruct when the rules disable them.
		if name == "ucontrol" and not unit_actions and option_name in ["build", "deconstruct"]:
			continue
		var toggle := Button.new()
		toggle.text = _option_text(option)
		toggle.toggle_mode = true
		toggle.theme_type_variation = "logicTogglet"
		toggle.button_pressed = option_name == _current_field_value(index, field_index)
		toggle.custom_minimum_size = Vector2(60, 38)
		toggle.tooltip_text = _bundle_tooltip("lenum." + option_name)
		toggle.pressed.connect(_select_enum.bind(index, field_index, option_name, popup))
		grid.add_child(toggle)
	popup.popup_centered()


## `fieldAlignSelect` (`LStatement.java:259-268`): the nine align presets,
## set as `@<name>` (`LStatement.afterRead` normalizes them).
func _open_align_select(index: int, field_index: int) -> void:
	var popup := PopupPanel.new()
	# code-instantiated: one transient align-select popup per open.
	add_child(popup)
	var grid := GridContainer.new()
	grid.columns = 3
	popup.add_child(grid)
	var current := _current_field_value(index, field_index)
	for align in [
		"topLeft", "top", "topRight", "left", "center", "right", "bottomLeft", "bottom", "bottomRight"
	]:
		var toggle := Button.new()
		toggle.text = _field_label_text(align)
		toggle.toggle_mode = true
		toggle.theme_type_variation = "logicTogglet"
		toggle.button_pressed = current == "@" + align
		toggle.custom_minimum_size = Vector2(150, 40)
		toggle.pressed.connect(_select_enum.bind(index, field_index, "@" + align, popup))
		grid.add_child(toggle)
	popup.popup_centered()


## `PrintCharStatement`'s pencil grid (`LStatements.java:290-311`).
func _open_char_select(index: int, field_index: int) -> void:
	var popup := PopupPanel.new()
	# code-instantiated: one transient character-picker popup per open.
	add_child(popup)
	var grid := GridContainer.new()
	grid.columns = 8
	popup.add_child(grid)
	for code in range(32, 127):
		var toggle := Button.new()
		toggle.text = String.chr(code)
		toggle.theme_type_variation = "flatt"
		toggle.custom_minimum_size = Vector2(32, 32)
		toggle.pressed.connect(_select_enum.bind(index, field_index, str(code), popup))
		grid.add_child(toggle)
	popup.popup_centered()


func _current_field_value(index: int, field_index: int) -> String:
	if index < 0 or index >= _statements.size():
		return ""
	var values: Array = (_statements[index] as Dictionary).get("fields", [])
	return str(values[field_index]) if field_index < values.size() else ""


## The `logicUnitBuild`/`logicUnitDeconstruct` rules probe (T1's add-dialog
## filter; `UnitControlStatement.build`).
func _unit_actions_enabled() -> bool:
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null:
		return true
	var parsed: Variant = JSON.parse_string(str(host.call("logic_rules_probe")))
	if not (parsed is Dictionary):
		return true
	var info := parsed as Dictionary
	return bool(info.get("logicUnitBuild", true)) or bool(info.get("logicUnitDeconstruct", true))


## A select changed: commit, apply the Java change fixups and rebuild the
## element (`DrawStatement.rebuild`, `rebuild(table)` in each `build`).
func _select_enum(index: int, field_index: int, option: String, popup: PopupPanel) -> void:
	popup.hide()
	popup.queue_free()
	if not _commit_field_value(option, index, field_index):
		return
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null or index < 0 or index >= _statements.size():
		return
	var row: Dictionary = _statements[index]
	var name := str(row.get("name", "noop"))
	var values: Array = row.get("fields", [])
	var changed: Variant = logic.call("apply_type_change", name, PackedStringArray(values))
	if changed is PackedStringArray:
		var list: Array = []
		for value in (changed as PackedStringArray):
			list.append(str(value))
		row["fields"] = list
		_statements[index] = row
	_modified = true
	_rebuild_list()


## `col(table, x, res -> ...)`: the colour picker commits `%rrggbb[aa]`.
func _set_color(color: Color, index: int, field_index: int) -> void:
	var alpha := color.a >= 1.0
	var packed := "%02x%02x%02x" % [
		int(color.r * 255.0), int(color.g * 255.0), int(color.b * 255.0)
	]
	if not alpha:
		packed += "%02x" % int(color.a * 255.0)
	_commit_field_value("%" + packed, index, field_index)


func _parse_color(value: String) -> Color:
	var text := value.trim_prefix("%")
	if text.length() < 6:
		return Color.WHITE
	var r := ("0x" + text.substr(0, 2)).hex_to_int()
	var g := ("0x" + text.substr(2, 2)).hex_to_int()
	var b := ("0x" + text.substr(4, 2)).hex_to_int()
	var a := 255
	if text.length() >= 8:
		a = ("0x" + text.substr(6, 2)).hex_to_int()
	return Color8(r, g, b, a)


## `field(table, value, setter)`: `setter.get(sanitize(text))`.
func _set_field(text: String, index: int, field_index: int) -> void:
	_commit_field_value(text, index, field_index)


## Commit on focus loss (`TextField.changed` in Java runs per keystroke; the
## port commits on submit/blur so typing stays cheap).
func _commit_field(index: int, field_index: int, edit: LineEdit) -> void:
	_commit_field_value(edit.text, index, field_index)


func _commit_field_value(text: String, index: int, field_index: int) -> bool:
	if index < 0 or index >= _statements.size():
		return false
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null:
		return false
	var sanitized := str(logic.call("sanitize", text))
	var row: Dictionary = _statements[index]
	var values: Array = row.get("fields", [])
	if field_index >= values.size():
		return false
	if str(values[field_index]) == sanitized:
		return false
	values[field_index] = sanitized
	row["fields"] = values
	_statements[index] = row
	_modified = true
	return true


## `StatementElem.copy` (`LCanvas.java:517-536`): the duplicate lands after the
## element; a copied `jump`'s target bumps when it pointed below (`:521-526`).
func _copy_statement(index: int) -> void:
	if index < 0 or index >= _statements.size() or _statements.size() >= MAX_INSTRUCTIONS:
		return
	var original: Dictionary = _statements[index]
	var copied: Dictionary = original.duplicate(true)
	# `addChildAfter`: element references below the insertion shift down.
	_shift_jumps_for_insert(index + 1)
	_statements.insert(index + 1, copied)
	if str(original.get("name", "")) == "jump":
		var fields := jump_fields(copied)
		if not fields.is_empty():
			var dest := int(fields[0])
			if dest > index:
				fields[0] = str(dest + 1)
				copied["fields"] = fields
	_modified = true
	_rebuild_list()


## Element references (`JumpStatement.dest`) survive an insertion at `at`:
## every target at or below it moves down one (`saveUI` recomputes the index).
func _shift_jumps_for_insert(at: int) -> void:
	for row in _statements:
		if not (row is Dictionary) or str((row as Dictionary).get("name", "")) != "jump":
			continue
		var fields := jump_fields(row)
		if fields.is_empty():
			continue
		var dest := int(fields[0])
		if dest >= at:
			fields[0] = str(dest + 1)
			(row as Dictionary)["fields"] = fields


## `StatementElem`'s delete button: dropped targets become `-1` and later
## indices shift down (Java's `saveUI` recomputes `indexOf(dest)`).
func _delete_statement(index: int) -> void:
	if index < 0 or index >= _statements.size():
		return
	_statements.remove_at(index)
	for row in _statements:
		if not (row is Dictionary) or str((row as Dictionary).get("name", "")) != "jump":
			continue
		var fields := jump_fields(row)
		if fields.is_empty():
			continue
		var dest := int(fields[0])
		if dest == index:
			fields[0] = "-1"
		elif dest > index:
			fields[0] = str(dest - 1)
		(row as Dictionary)["fields"] = fields
	_modified = true
	_rebuild_list()


## The `jump` row's `dest_index` (`-1` when unset/out of range).
func _jump_target(row: Dictionary) -> int:
	if str(row.get("name", "")) != "jump":
		return -1
	var fields: Array = row.get("fields", [])
	if fields.is_empty():
		return -1
	var dest := int(fields[0])
	return dest if dest >= 0 and dest < _statements.size() else -1


func jump_fields(row: Variant) -> Array:
	if row is Dictionary:
		var fields: Variant = (row as Dictionary).get("fields", [])
		if fields is Array and not (fields as Array).is_empty():
			return fields
	return [] as Array


## `LCanvas.DragLayout.finishLayout`: drop the dragged element at
## `insertPosition` and remap every `jump` target like Java's element refs do
## (`JumpStatement.saveUI` recomputes `indexOf(dest)`).
func _finish_drag() -> void:
	var from := _drag_index
	if from < 0 or from >= _statements.size():
		return
	var drop := clampi(_drop_index, 0, _statements.size())
	var to := drop if drop <= from else drop - 1
	if to != from:
		for row in _statements:
			if not (row is Dictionary) or str((row as Dictionary).get("name", "")) != "jump":
				continue
			var fields := jump_fields(row)
			if fields.is_empty():
				continue
			var dest := int(fields[0])
			if dest == from:
				dest = to
			elif to >= from and from < dest and dest <= to:
				dest -= 1
			elif to < from and to <= dest and dest < from:
				dest += 1
			fields[0] = str(dest)
			(row as Dictionary)["fields"] = fields
		var moved: Variant = _statements.pop_at(from)
		_statements.insert(to, moved)
		_modified = true
	_drop_index = -1
	_rebuild_list()


## The insertion index for a global pointer position (`DragLayout.layout`'s
## `insertPosition`).
func _drop_index_at(global_pos: Vector2) -> int:
	for i in _row_nodes.size():
		var rect := (_row_nodes[i] as Control).get_global_rect()
		if global_pos.y < rect.position.y + rect.size.y * 0.5:
			return i
	return _row_nodes.size()


## The statement element under a global pointer position (`canvas.hovered`).
func _row_at(global_pos: Vector2) -> int:
	for i in _row_nodes.size():
		if (_row_nodes[i] as Control).get_global_rect().has_point(global_pos):
			return i
	return -1


## `JumpButton`'s touch handlers (`LCanvas.java:570-600`): press clears the
## target and starts selecting (the curve follows the pointer / hovered
## element), release sets the hovered element (not the jump's own) or leaves
## the target cleared.
func _jump_button_input(event: InputEvent, index: int, button: Control) -> void:
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT:
		button.accept_event()
		if event.pressed:
			_jump_selecting = index
			_set_jump_target(index, -1)
			_jump_hover = _row_at(button.get_global_transform() * event.position)
			_jump_pointer = button.get_global_transform() * event.position
			_compute_jump_heights()
		else:
			var hovered := _row_at(button.get_global_transform() * event.position)
			if hovered >= 0 and hovered != index:
				_set_jump_target(index, hovered)
			_jump_selecting = -1
			_jump_hover = -1
			_rebuild_list()
		if _overlay != null:
			_overlay.queue_redraw()
		return
	if event is InputEventMouseMotion and _jump_selecting == index:
		_jump_pointer = button.get_global_transform() * event.position
		_jump_hover = _row_at(_jump_pointer)
		if _overlay != null:
			_overlay.queue_redraw()
		button.accept_event()


## `JumpButton`'s setter: writes the `dest_index` field (`-1` clears).
func _set_jump_target(index: int, target: int) -> void:
	if index < 0 or index >= _statements.size():
		return
	var row: Dictionary = _statements[index]
	var fields: Array = row.get("fields", [])
	if fields.is_empty():
		return
	if str(fields[0]) == str(target):
		return
	fields[0] = str(target)
	row["fields"] = fields
	_statements[index] = row
	_modified = true


## `DragLayout.setJumpHeights` (`LCanvas.java:274-321`): representative
## curves (widest forward jump per end, widest backward jump per begin) get
## nested lane heights; every curve then takes its representative's height.
func _compute_jump_heights() -> void:
	_pred_heights.clear()
	var curves: Array = []
	for i in _statements.size():
		if not (_statements[i] is Dictionary) or i == _jump_selecting:
			continue
		var dest := _jump_target(_statements[i])
		if dest < 0:
			continue
		curves.append({
			"jump": i,
			"begin": mini(i, dest),
			"end": maxi(i, dest),
			"flipped": i >= dest,
			"pred": 0,
			"done": false,
		})
	var repr_before: Dictionary = {}
	var repr_after: Dictionary = {}
	for curve_value in curves:
		var curve: Dictionary = curve_value
		var begin: int = curve["begin"]
		var end: int = curve["end"]
		if bool(curve["flipped"]):
			if repr_after.has(begin) and int((repr_after[begin] as Dictionary)["end"]) >= end:
				continue
			repr_after[begin] = curve
		else:
			if repr_before.has(end) and int((repr_before[end] as Dictionary)["begin"]) <= begin:
				continue
			repr_before[end] = curve
	var processed: Array = []
	for value in repr_before.values():
		processed.append(value)
	for value in repr_after.values():
		processed.append(value)
	for order in processed.size():
		(processed[order] as Dictionary)["order"] = order
	processed.sort_custom(
		func(a: Dictionary, b: Dictionary) -> bool:
			if int(a["begin"]) != int(b["begin"]):
				return int(a["begin"]) < int(b["begin"])
			return int(a["order"]) < int(b["order"])
	)
	var occupiers: Array = []
	var occupied: Dictionary = {}
	for i in processed.size():
		var cur: Dictionary = processed[i]
		var kept: Array = []
		for occupier_value in occupiers:
			var occupier: Dictionary = occupier_value
			if int(occupier["end"]) > int(cur["begin"]):
				kept.append(occupier)
			else:
				occupied.erase(int(occupier["pred"]))
		occupiers = kept
		var height := _jump_height(processed, i, occupiers, occupied)
		occupiers.append(cur)
		occupied[height] = true
	_tmp_occupiers.clear()
	for curve_value in curves:
		var curve: Dictionary = curve_value
		var rep: Dictionary = (
			repr_after[int(curve["begin"])] if bool(curve["flipped"])
			else repr_before[int(curve["end"])]
		)
		_pred_heights[int(curve["jump"])] = int(rep["pred"])


## `DragLayout.getJumpHeight` (`LCanvas.java:323-353`): the lowest free lane
## above every jump nested inside this one. The scratch buffers are shared
## across recursion levels exactly like Java's static `tmpOccupiers2`/`tmpBits2`.
func _jump_height(processed: Array, index: int, occupiers: Array, occupied: Dictionary) -> int:
	var jmp: Dictionary = processed[index]
	if bool(jmp["done"]):
		return int(jmp["pred"])
	if not is_same(occupiers, _tmp_occupiers):
		_tmp_occupiers.assign(occupiers)
	if not is_same(occupied, _tmp_bits):
		_tmp_bits.clear()
		_tmp_bits.merge(occupied)
	var max_height := -1
	for i in range(index + 1, processed.size()):
		var cur: Dictionary = processed[i]
		if int(cur["end"]) > int(jmp["end"]):
			continue
		var kept: Array = []
		for occupier_value in _tmp_occupiers:
			var occupier: Dictionary = occupier_value
			if int(occupier["end"]) > int(cur["begin"]):
				kept.append(occupier)
			else:
				_tmp_bits.erase(int(occupier["pred"]))
		_tmp_occupiers.assign(kept)
		var height := _jump_height(processed, i, _tmp_occupiers, _tmp_bits)
		_tmp_occupiers.append(cur)
		_tmp_bits[height] = true
		max_height = maxi(max_height, height)
	var lane := max_height + 1
	while occupied.has(lane):
		lane += 1
	jmp["pred"] = lane
	jmp["done"] = true
	_tmp_occupiers.clear()
	return lane


## `LCanvas.DragLayout.draw` + `JumpCurve.draw`: the drop line while dragging
## and one trapezoid per `jump` from its node to the target element's right
## edge (`JumpCurve.act`: `t.set(hover.getWidth(), hover.getHeight() / 2f)`).
func _draw_overlay() -> void:
	if _overlay == null:
		return
	if _drag_active and _drop_index >= 0 and not _row_nodes.is_empty():
		var first := (_row_nodes[0] as Control).get_global_rect()
		var y := 0.0
		if _drop_index <= 0:
			y = first.position.y - 3.0
		elif _drop_index >= _row_nodes.size():
			y = (_row_nodes[_row_nodes.size() - 1] as Control).get_global_rect().end.y + 3.0
		else:
			var above := (_row_nodes[_drop_index - 1] as Control).get_global_rect()
			var below := (_row_nodes[_drop_index] as Control).get_global_rect()
			y = (above.end.y + below.position.y) * 0.5
		var left := first.position.x
		var right := left + first.size.x
		_overlay.draw_line(
			_to_overlay(Vector2(left, y)),
			_to_overlay(Vector2(right, y)),
			Color(1, 1, 1, 0.7),
			3.0
		)
	var hovered_last := -1
	for jump_value in _jump_buttons.keys():
		var jump: int = jump_value
		# `if(button.listener.isOver()) toFront()`: the hovered curve draws last.
		if jump == _jump_over:
			hovered_last = jump
			continue
		_draw_jump(jump)
	if hovered_last >= 0:
		_draw_jump(hovered_last)


## One `JumpCurve.draw` (`LCanvas.java:678-723`).
func _draw_jump(jump: int) -> void:
	var button: Control = _jump_buttons.get(jump)
	if button == null or not is_instance_valid(button):
		return
	var src := _to_overlay(button.get_global_rect().get_center())
	var dst := Vector2.ZERO
	var target := -1
	if jump == _jump_selecting:
		target = _jump_hover
		if target < 0:
			dst = _to_overlay(_jump_pointer)
	elif jump < _statements.size():
		target = _jump_target(_statements[jump])
		if target < 0:
			return
	else:
		return
	if target >= 0:
		if target >= _row_nodes.size():
			return
		var rect := (_row_nodes[target] as Control).get_global_rect()
		dst = _to_overlay(Vector2(rect.end.x, rect.position.y + rect.size.y * 0.5))
	if src.is_equal_approx(dst):
		return
	var color := PLACE if jump == _jump_over else Color.WHITE
	var ui_height: float = _curve_heights.get(jump, 60.0)
	# Trapezoidal jumps (`drawCurve`): out by `uiHeight`, across, back in.
	var dy := 0.0
	if dst.y != src.y:
		dy = (1.0 if dst.y > src.y else -1.0) * ui_height * 0.5
	var corner_a := Vector2(src.x + ui_height, src.y + dy)
	var corner_b := Vector2(src.x + ui_height, dst.y - dy)
	var points := PackedVector2Array()
	var crossing: Variant = Geometry2D.segment_intersects_segment(src, corner_a, dst, corner_b)
	if crossing is Vector2:
		points = PackedVector2Array([src, crossing as Vector2, dst])
	else:
		points = PackedVector2Array([src, corner_a, corner_b, dst])
	_overlay.draw_polyline(points, color, 4.0)
	# `Tex.logicNode.draw(t.x + s * 0.75f, t.y - s / 2f, -s, s)`: the mirrored
	# node at the target.
	if _node_texture == null:
		_node_texture = MindWidgets.icon_texture("logic-node")
	var texture := _node_texture
	var s := button.size.x if button.size.x > 0.0 else JUMP_BUTTON_SIZE
	if texture != null:
		_overlay.draw_set_transform(Vector2(dst.x + s * 0.75, dst.y - s * 0.5), 0.0, Vector2(-1, 1))
		_overlay.draw_texture_rect(texture, Rect2(Vector2.ZERO, Vector2(s, s)), false, color)
		_overlay.draw_set_transform(Vector2.ZERO, 0.0, Vector2.ONE)


func _to_overlay(global_pos: Vector2) -> Vector2:
	return _overlay.get_global_transform().affine_inverse() * global_pos


## `JumpCurve.drawCurve`'s height smoothing:
## `uiHeight = lerp(40 + 10 * predHeight, uiHeight, 0.9^delta)` (portrait:
## `20 + 8 * predHeight`).
func _update_curve_heights(delta: float) -> void:
	var portrait := get_viewport_rect().size.y > get_viewport_rect().size.x
	var base := 20.0 if portrait else 40.0
	var step := 8.0 if portrait else 10.0
	var keep := pow(0.9, delta * 60.0)
	for jump_value in _jump_buttons.keys():
		var jump: int = jump_value
		var target := base + step * float(_pred_heights.get(jump, 0))
		var current: float = _curve_heights.get(jump, 60.0)
		_curve_heights[jump] = target + (current - target) * keep


## A nested full-screen `BaseDialog` stacked over the editor: `black9` stage,
## the accent title + 3 px rule, a centred content cell and the button row
## with `addCloseButton()` (`@back`, 210x64).
func _open_sub_dialog(title_key: String) -> Dictionary:
	_close_sub_dialog()
	# code-instantiated: a transient nested dialog (one per open), stacked
	# over the editor like Java's `new BaseDialog(...)`.
	var root := Control.new()
	root.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	root.mouse_filter = Control.MOUSE_FILTER_STOP
	add_child(root)
	var dim := ColorRect.new()
	dim.color = MindStyles.BLACK_9
	dim.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	dim.mouse_filter = Control.MOUSE_FILTER_IGNORE
	root.add_child(dim)
	var layout := VBoxContainer.new()
	layout.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	layout.add_theme_constant_override("separation", 0)
	root.add_child(layout)
	var title := Label.new()
	title.text = _t(title_key)
	title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	title.add_theme_color_override("font_color", MindStyles.ACCENT)
	layout.add_child(title)
	var rule_margin := MarginContainer.new()
	for side in ["margin_left", "margin_right", "margin_top", "margin_bottom"]:
		rule_margin.add_theme_constant_override(side, 4)
	layout.add_child(rule_margin)
	var rule := ColorRect.new()
	rule.color = MindStyles.ACCENT
	rule.custom_minimum_size = Vector2(0, 3)
	rule_margin.add_child(rule)
	var center := CenterContainer.new()
	center.size_flags_vertical = Control.SIZE_EXPAND_FILL
	layout.add_child(center)
	var row := HBoxContainer.new()
	row.alignment = BoxContainer.ALIGNMENT_CENTER
	layout.add_child(row)
	var back := MindWidgets.icon_button("left", _t("@back"))
	back.custom_minimum_size = Vector2(210, 64)
	back.pressed.connect(_close_sub_dialog.bind(true))
	row.add_child(back)
	var bottom := Control.new()
	bottom.custom_minimum_size = Vector2(0, 8)
	layout.add_child(bottom)
	_sub_dialog = root
	return {"root": root, "cont": center, "buttons": row}


## Hides the nested dialog (`BaseDialog.hidden` plays `uiBack`).
func _close_sub_dialog(play_sound: bool = false) -> void:
	if _sub_dialog == null:
		return
	if is_instance_valid(_sub_dialog):
		_sub_dialog.queue_free()
	_sub_dialog = null
	if play_sound:
		var ui := get_node_or_null("/root/MindUi")
		if ui != null and ui.has_method("play_ui_sound"):
			ui.call("play_ui_sound", "uiBack")


## A nested dialog owns Esc (`closeOnBack`); the editor's own Esc closes it.
func blocks_escape() -> bool:
	return _sub_dialog != null


## Esc closes the nested dialog; Shift+Enter opens the Add dialog
## (`LogicDialog`'s `keyDown(KeyCode.enter, ...)`).
func _input(event: InputEvent) -> void:
	if not visible:
		return
	var key := event as InputEventKey
	if key == null or not key.pressed or key.echo:
		return
	if key.keycode == KEY_ESCAPE and _sub_dialog != null:
		_close_sub_dialog(true)
		get_viewport().set_input_as_handled()
		return
	if (key.keycode == KEY_ENTER or key.keycode == KEY_KP_ENTER) and key.shift_pressed \
			and _sub_dialog == null and is_top():
		_open_add_dialog(-1)
		get_viewport().set_input_as_handled()


## `Tex.button` 9-patch (12/11 splits) for the Add/Edit content tables.
func _button_box() -> StyleBox:
	var texture := _region("button")
	if texture != null:
		var box := StyleBoxTexture.new()
		box.texture = texture
		box.texture_margin_left = 12
		box.texture_margin_top = 12
		box.texture_margin_right = 11
		box.texture_margin_bottom = 11
		return box
	return _pane_box(Color.BLACK, PANE_OUTLINE_GRAY)


## `LogicDialog`'s `@edit` dialog (`LogicDialog.java:107-158`):
## `BaseDialog("@editor.export")` with a `Tex.button` table of `flatt`
## 280x60 rows (`@clear`, `@copy.clipboard`, `@load.clipboard`,
## `@logic.restart`) and `addCloseButton`. `@editor.worldprocessors.editname`
## needs a world-processor tag config the sim does not expose yet.
func _open_edit_menu() -> void:
	var parts := _open_sub_dialog("@editor.export")
	var margin := MarginContainer.new()
	for side in ["margin_left", "margin_right", "margin_top", "margin_bottom"]:
		margin.add_theme_constant_override(side, 10)
	(parts["cont"] as Control).add_child(margin)
	var panel := PanelContainer.new()
	panel.add_theme_stylebox_override("panel", _button_box())
	margin.add_child(panel)
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 0)
	panel.add_child(column)
	column.add_child(_edit_row("cancel", "@clear", _edit_clear))
	column.add_child(_edit_row("copy", "@copy.clipboard", _edit_copy))
	var load_row := _edit_row("download", "@load.clipboard", _edit_load)
	load_row.disabled = DisplayServer.clipboard_get().is_empty()
	column.add_child(load_row)
	column.add_child(_edit_row("refresh", "@logic.restart", _edit_restart))


## One `t.button(text, icon, Styles.flatt, ...).size(280f, 60f).left().marginLeft(12f)`.
func _edit_row(icon: String, key: String, listener: Callable) -> Button:
	# code-instantiated: the edit menu rows are built per open.
	var button := MindWidgets.icon_button(icon, _t(key), "flatt", BoxContainer.ALIGNMENT_BEGIN)
	button.custom_minimum_size = Vector2(280, 60)
	button.add_theme_stylebox_override("normal", _flat_box(Color.BLACK))
	button.add_theme_stylebox_override("hover", _flat_box(FLAT_OVER))
	button.add_theme_stylebox_override("pressed", _flat_box(FLAT_OVER))
	if button.get_child_count() > 0:
		(button.get_child(0) as Control).offset_left = 12.0
	button.pressed.connect(listener)
	return button


## `@clear`: `ui.showConfirm("@logic.clear.confirm", canvas::clearStatements)`
## then `dialog.hide()`.
func _edit_clear() -> void:
	var ui := get_node_or_null("/root/MindUi")
	_close_sub_dialog(true)
	if ui == null or not ui.has_method("show_confirm"):
		return
	if ui.has_signal("confirm_result") and not ui.is_connected("confirm_result", _on_confirm):
		ui.connect("confirm_result", _on_confirm)
	_pending_clear = true
	ui.call("show_confirm", _t("@logic.clear.confirm"))


func _on_confirm(confirmed: bool) -> void:
	if not _pending_clear:
		return
	_pending_clear = false
	if not confirmed:
		return
	# `LCanvas.clearStatements`.
	_statements.clear()
	_modified = true
	_rebuild_list()


## `@copy.clipboard`: `Core.app.setClipboardText(canvas.save())`.
func _edit_copy() -> void:
	_close_sub_dialog(true)
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic != null:
		DisplayServer.clipboard_set(str(logic.call("write_statements", _statements)))


## `@load.clipboard`: `canvas.load(Core.app.getClipboardText())`.
func _edit_load() -> void:
	_close_sub_dialog(true)
	_load_from_clipboard()


## `@logic.restart`: `forceRestart = true; dialog.hide(); hide();` — the
## editor closes and the consumer writes the program back even if unchanged.
func _edit_restart() -> void:
	_force_restart = true
	_close_sub_dialog(true)
	_close()


## `@load.clipboard`: parse the clipboard text into the statement list.
func _load_from_clipboard() -> void:
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null:
		return
	var text := DisplayServer.clipboard_get()
	if text.strip_edges().is_empty():
		return
	var parsed: Variant = logic.call("parse_statements", text, _privileged)
	if parsed is Array:
		_statements = parsed
		if _statements.size() > MAX_INSTRUCTIONS:
			_statements.resize(MAX_INSTRUCTIONS)
		_modified = true
		_rebuild_list()


## `LogicDialog`'s `@variables` button (`:162-243`): the variables panel when
## `shouldShowVariables()`, else the globals panel; the button label follows.
func _open_variables_panel() -> void:
	if not _should_show_variables():
		_open_globals_panel()
		return
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null:
		return
	var state: Variant = host.call("logic_get_state", _x, _y)
	var vars: Array = []
	if state is Dictionary:
		var list: Variant = (state as Dictionary).get("vars", [])
		if list is Array:
			vars = list
	_open_vars_panel(vars)


## The per-variable rows (`LogicDialog.java:183-247`): name (accent), the value
## (140 wide, refreshed every 15 frames) and the type.
func _open_vars_panel(vars: Array) -> void:
	var popup := PopupPanel.new()
	# code-instantiated: the variables panel is transient and data-driven.
	add_child(popup)
	var column := VBoxContainer.new()
	column.custom_minimum_size = Vector2(420, 320)
	popup.add_child(column)
	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	column.add_child(scroll)
	var list := VBoxContainer.new()
	list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(list)
	_vars_rows.clear()
	for entry in vars:
		if not (entry is Dictionary):
			continue
		var row := HBoxContainer.new()
		var name_label := Label.new()
		name_label.text = " " + str((entry as Dictionary).get("name", "")) + " "
		name_label.add_theme_color_override("font_color", MindStyles.ACCENT)
		row.add_child(name_label)
		var value_label := Label.new()
		value_label.text = _format_var_value(entry)
		value_label.custom_minimum_size = Vector2(140, 0)
		value_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		row.add_child(value_label)
		var type_label := Label.new()
		type_label.text = " " + str((entry as Dictionary).get("type", "")) + " "
		type_label.custom_minimum_size = Vector2(120, 0)
		row.add_child(type_label)
		list.add_child(row)
		_vars_rows.append([str((entry as Dictionary).get("name", "")), value_label])
	var buttons_row := HBoxContainer.new()
	column.add_child(buttons_row)
	var globals := MindWidgets.icon_button("list", _t("@logic.globals"), "cleart")
	globals.pressed.connect(
		func() -> void:
			popup.hide()
			_open_globals_panel()
	)
	buttons_row.add_child(globals)
	_vars_popup = popup
	_vars_timer = 0
	popup.popup_centered()
	set_process(true)


## `LogicDialog`'s value text: objects print through the sim, numbers as
## integers within 0.00001 (`LogicDialog.java:206-215`).
func _format_var_value(entry: Variant) -> String:
	return str((entry as Dictionary).get("value", ""))


## The globals panel (`GlobalVarsDialog`): `@name` + its `lglobal.<name>`
## description.
func _open_globals_panel() -> void:
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null:
		return
	var popup := PopupPanel.new()
	# code-instantiated: the globals panel is transient and data-driven.
	add_child(popup)
	var column := VBoxContainer.new()
	column.custom_minimum_size = Vector2(520, 380)
	popup.add_child(column)
	var scroll := ScrollContainer.new()
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	column.add_child(scroll)
	var list := VBoxContainer.new()
	list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(list)
	var rows: Variant = logic.call("globals")
	if rows is Array:
		for entry in rows:
			if not (entry is Dictionary):
				continue
			var row := HBoxContainer.new()
			var name_label := Label.new()
			name_label.text = " " + str((entry as Dictionary).get("name", "")) + " "
			name_label.custom_minimum_size = Vector2(150, 0)
			row.add_child(name_label)
			var desc_label := Label.new()
			desc_label.text = _t(str((entry as Dictionary).get("description", "")))
			desc_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
			desc_label.custom_minimum_size = Vector2(320, 0)
			row.add_child(desc_label)
			list.add_child(row)
	popup.popup_centered()


## Per-frame upkeep: jump-curve smoothing/redraw (`JumpCurve.act`) and the
## 15-frame variables refresh while the panel is shown
## (`LogicDialog.java:204-222`).
func _process(delta: float) -> void:
	if not is_visible_in_tree():
		return
	_update_curve_heights(delta)
	# `DragLayout`/`JumpCurve` repaint with the scroll position every frame.
	if _overlay != null:
		_overlay.queue_redraw()
	if _vars_popup == null or not _vars_popup.visible:
		return
	_vars_timer += 1
	if _vars_timer < 15:
		return
	_vars_timer = 0
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null:
		return
	var state: Variant = host.call("logic_get_state", _x, _y)
	if not (state is Dictionary):
		return
	var vars: Variant = (state as Dictionary).get("vars", [])
	if not (vars is Array):
		return
	for entry in vars:
		if not (entry is Dictionary):
			continue
		var name := str((entry as Dictionary).get("name", ""))
		for row in _vars_rows:
			if row[0] == name:
				(row[1] as Label).text = _format_var_value(entry)


## `LogicDialog.showAddDialog(position)` (`LogicDialog.java:260-347`): a
## nested `BaseDialog("@add")` whose `Tex.button` table holds the search row
## (`Icon.zoom` + field, `@players.search`) over a pane of category sections:
## a header (`Pal.darkishGray` icon 15 px, name with the description tooltip,
## 5 px rule) then 130x50 `flatt` buttons in the category colour, three per
## row, each with its `lst.<key>` tooltip. Enter adds the first match.
func _open_add_dialog(at_index: int = -1) -> void:
	if _statements.size() >= MAX_INSTRUCTIONS:
		return
	var entries := _add_entries()
	var parts := _open_sub_dialog("@add")
	var panel := PanelContainer.new()
	panel.add_theme_stylebox_override("panel", _button_box())
	(parts["cont"] as Control).add_child(panel)
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 4)
	panel.add_child(column)
	var search_row := HBoxContainer.new()
	search_row.add_theme_constant_override("separation", 8)
	column.add_child(search_row)
	search_row.add_child(MindWidgets.glyph("zoom", 22))
	var search := MindWidgets.field(_t("@players.search"))
	search.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	search_row.add_child(search)
	var scroll := ScrollContainer.new()
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	column.add_child(scroll)
	var list := VBoxContainer.new()
	list.add_theme_constant_override("separation", 0)
	scroll.add_child(list)
	var matched: Array = [""]
	var rebuild := func() -> void:
		for child in list.get_children():
			list.remove_child(child)
			child.queue_free()
		var text := search.text.to_lower()
		matched[0] = ""
		var sections: Dictionary = {}
		for entry_value in entries:
			var entry: Dictionary = entry_value
			var label: String = entry["label"]
			var type_name: String = entry["type_name"]
			if not text.is_empty() and not label.to_lower().contains(text) \
					and not type_name.to_lower().contains(text):
				continue
			var name: String = entry["name"]
			if str(matched[0]).is_empty():
				matched[0] = name
			var category: String = entry["category"]
			# `t.find(category.name)`: one section per category, in first-seen order.
			if not sections.has(category):
				list.add_child(_add_category_header(category))
				# code-instantiated: one button grid per listed category.
				var grid := GridContainer.new()
				grid.columns = 3
				grid.add_theme_constant_override("h_separation", 0)
				grid.add_theme_constant_override("v_separation", 0)
				list.add_child(grid)
				sections[category] = grid
			(sections[category] as GridContainer).add_child(
				_add_statement_button(entry, at_index)
			)
		call_deferred("_fit_add_scroll", scroll, list, search_row)
	search.text_changed.connect(func(_text: String) -> void: rebuild.call())
	# `search.keyDown(KeyCode.enter, ...)`: add the first match.
	search.text_submitted.connect(
		func(text: String) -> void:
			if not text.is_empty() and not str(matched[0]).is_empty():
				_add_statement(str(matched[0]), at_index)
	)
	rebuild.call()
	search.call_deferred("grab_focus")


## `.maxHeight(Core.graphics.getHeight() * 0.8f)` on the content table: the
## pane takes the list's size up to 80% of the screen.
func _fit_add_scroll(scroll: ScrollContainer, list: Control, search_row: Control) -> void:
	if not is_instance_valid(scroll) or not is_instance_valid(list):
		return
	var wanted := list.get_combined_minimum_size()
	var cap := get_viewport_rect().size.y * 0.8 - 30.0
	if is_instance_valid(search_row):
		cap -= search_row.size.y + 4.0
	var height := minf(wanted.y, maxf(cap, 100.0))
	var width := wanted.x
	if wanted.y > height:
		width += scroll.get_v_scroll_bar().get_combined_minimum_size().x
	scroll.custom_minimum_size = Vector2(width, height)


## The category header row (`LogicDialog.java:313-319`).
func _add_category_header(category: String) -> Control:
	# code-instantiated: one header per listed category.
	var margin := MarginContainer.new()
	margin.add_theme_constant_override("margin_left", 5)
	margin.add_theme_constant_override("margin_right", 5)
	margin.add_theme_constant_override("margin_top", 10)
	margin.add_theme_constant_override("margin_bottom", 5)
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 0)
	margin.add_child(row)
	var icon := str(CATEGORY_ICONS.get(category, ""))
	if not icon.is_empty():
		var glyph := MindWidgets.glyph(icon, 15, DARKISH_GRAY)
		glyph.custom_minimum_size = Vector2(15, 15)
		row.add_child(glyph)
		row.add_child(MindWidgets.space(10, 0))
	var title := Label.new()
	title.text = _t("lcategory." + category)
	title.add_theme_color_override("font_color", DARKISH_GRAY)
	title.mouse_filter = Control.MOUSE_FILTER_STOP
	title.tooltip_text = _plain_markup(_t("lcategory." + category + ".description"))
	row.add_child(title)
	row.add_child(MindWidgets.space(10, 0))
	var line := ColorRect.new()
	line.color = DARKISH_GRAY
	line.custom_minimum_size = Vector2(0, 5)
	line.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	line.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	line.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(line)
	return margin


## One `cat.button(example.localizedName(), style, ...).size(130f, 50f)` with
## `style = flatt` + `fontColor = category.color`, `font = Fonts.outline`.
func _add_statement_button(entry: Dictionary, at_index: int) -> Button:
	# code-instantiated: one button per listed statement.
	var button := Button.new()
	button.text = str(entry["label"])
	button.custom_minimum_size = Vector2(130, 50)
	button.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	button.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	button.theme_type_variation = "flatt"
	button.add_theme_stylebox_override("normal", _flat_box(Color.BLACK))
	button.add_theme_stylebox_override("hover", _flat_box(FLAT_OVER))
	button.add_theme_stylebox_override("pressed", _flat_box(FLAT_OVER))
	button.add_theme_stylebox_override("hover_pressed", _flat_box(FLAT_OVER))
	var color: Color = entry["color"]
	for state in ["font_color", "font_hover_color", "font_pressed_color", "font_focus_color", "font_hover_pressed_color"]:
		button.add_theme_color_override(state, color)
	button.add_theme_color_override("font_outline_color", Color.BLACK)
	button.add_theme_constant_override("outline_size", 4)
	button.tooltip_text = str(entry["tooltip"])
	button.pressed.connect(_add_statement.bind(str(entry["name"]), at_index))
	return button


## Candidate statements for the add dialog in `LogicIO.allStatements` order,
## after the Java filters (`LogicDialog.java:300-304`): `InvalidStatement`,
## `hidden()`, privileged-only outside world processors, `nonPrivileged()`
## inside them (none in vanilla), and unit statements without
## `logicUnitControl` on a non-privileged processor.
func _add_entries() -> Array:
	var out: Array = []
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null:
		return out
	var unit_allowed := true
	var host := get_node_or_null("/root/Spine/SimHost")
	if host != null and not _editor_mode and not _filter_mode:
		var parsed: Variant = JSON.parse_string(str(host.call("logic_rules_probe")))
		if parsed is Dictionary:
			unit_allowed = bool((parsed as Dictionary).get("logicUnitControl", true))
	for name_value in logic.call("statement_names"):
		var name := str(name_value)
		var info := _meta(name)
		if info.is_empty():
			continue
		if _type_name(name) == "Invalid":
			continue
		if bool(info.get("hidden", false)):
			continue
		var privileged_only := bool(info.get("privileged", false))
		if privileged_only and not _privileged:
			continue
		if bool(info.get("non_privileged", false)) and _privileged:
			continue
		var category := str(info.get("category", "unknown"))
		if category == "unit" and not _privileged and not unit_allowed:
			continue
		out.append({
			"name": name,
			"category": category,
			"color": CATEGORY_COLORS.get(category, CATEGORY_COLORS["unknown"]),
			"label": _localized_name(name),
			"type_name": _type_name(name),
			"tooltip": _bundle_tooltip("lst." + _statement_key(name)),
		})
	return out


## Adds a default instance of `name` at `at_index` (`-1` appends):
## `canvas.addAt(position == -1 ? size : position, prov.get())`, then
## `dialog.hide()`.
func _add_statement(name: String, at_index: int) -> void:
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null or _statements.size() >= MAX_INSTRUCTIONS:
		return
	var row: Variant = logic.call("default_statement_row", name)
	if not (row is Dictionary) or (row as Dictionary).is_empty():
		return
	var at := _statements.size() if at_index < 0 else clampi(at_index, 0, _statements.size())
	_shift_jumps_for_insert(at)
	_statements.insert(at, row)
	_modified = true
	_close_sub_dialog(true)
	_rebuild_list()


## `LCanvas.save` -> `LogicBuild.showEditDialog`'s consumer.
func _save() -> void:
	var logic := get_node_or_null("/root/Spine/MindLogic")
	if logic == null:
		return
	var code := str(logic.call("write_statements", _statements))
	_force_restart = false
	if _filter_mode:
		# `modified.get(result)`: the generate dialog owns the filter JSON.
		_code = code
		_modified = false
		code_edited.emit(code)
		return
	var error := ""
	if _editor_mode:
		var editor := get_node_or_null("/root/Spine/MindEditor")
		if editor == null:
			return
		error = str(editor.call("logic_configure_code", _x, _y, code))
	else:
		var host := get_node_or_null("/root/Spine/SimHost")
		if host == null:
			return
		error = str(host.call("logic_configure_code", _x, _y, code))
	if error.is_empty():
		_code = code
		_modified = false
	elif error.begins_with("toolong:"):
		_show_toolong(error.trim_prefix("toolong:").to_int())


func _show_toolong(length: int) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("toast"):
		return
	var assets := get_node_or_null("/root/MindAssets")
	var text := "too long"
	if assets != null and assets.has_method("bundle_format"):
		text = str(assets.call("bundle_format", "logic.error.toolong", ["16000", str(length)]))
	ui.call("toast", text, "error")


## Closes through `MindUi` so the stack and the pause governor stay in sync
## (`BaseDialog.hide`); the manifest name is `logic`.
func _close() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("close_dialog"):
		if bool(ui.call("close_dialog", "logic")):
			return
	hide_dialog()
