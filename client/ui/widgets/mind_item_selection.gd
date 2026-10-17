## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/world/blocks/ItemSelection.java
##         (`ItemSelection.buildTable`; J05 T1).
##
## The icon-grid content picker Java routes item/liquid/block/unit selection
## through (`Sorter`, `ItemSource`/`LiquidSource`, `Unloader`/`DuctRouter`,
## `LandingPad`, payload blocks, `Constructor`, `UnitCargoUnloadPoint`,
## `UnitFactory`): a `Styles.black6` table of 40 px `clearNoneTogglei` cells
## holding the content `uiIcon` at `clamp(selectionSize, 0, 40)`, a
## `ButtonGroup` with min-check 0 (re-tapping the checked icon sends `null`,
## clearing the config), an optional search field once the grid exceeds
## `rows * 1.5` rows, and the block's remembered `selectScroll` offset.
##
## The widget is built from the `config_spec` JSON (`MindInput.config_spec`'s
## `selection` route) and reports picks through `picked`; the fragment owns the
## submission (`configure_building`), so GDScript never mutates sim state.

class_name MindItemSelection
extends PanelContainer

## Emitted on a pick (`ItemSelection.java:73`: `consumer.get(button.isChecked()
## ? item : null)`): the content name, or "" when the tapped icon unchecked.
signal picked(value: String)

## One cell (`Table.defaults().size(40)`, `ItemSelection.java:54`).
const CELL := 40.0
## The tap closes the panel (`ItemSelection.java:70-71`, `closeSelect`).
const ICON_MARGIN := 0.0

var _block := ""
var _rows := 5
var _columns := 4
var _current := ""
var _close_select := true
var _options: Array = []
var _group: ButtonGroup = null
var _grid: GridContainer = null
var _scroll: ScrollContainer = null
var _holder: Control = null
var _search: LineEdit = null
var _search_row: HBoxContainer = null


## Builds the table for a `config_spec` selection route.
func build(spec: Dictionary) -> void:
	_block = str(spec.get("block", ""))
	_rows = maxi(1, int(spec.get("selection_rows", 5)))
	_columns = maxi(1, int(spec.get("selection_columns", 4)))
	_current = str(spec.get("current", ""))
	_options = spec.get("options", []) if spec.get("options", []) is Array else []
	# `Table main = new Table().background(Styles.black6)`
	# (`ItemSelection.java:86`): the translucent black panel, no padding.
	var background := StyleBoxFlat.new()
	background.bg_color = MindStyles.BLACK_6
	background.set_content_margin_all(0.0)
	add_theme_stylebox_override("panel", background)
	# code-instantiated: the option grid is runtime content from the
	# `config_spec` JSON; it has no scene-authorable per-building shape.
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 0)
	add_child(column)
	_search_row = _build_search_row()
	column.add_child(_search_row)
	_holder = Control.new()
	_holder.clip_contents = true
	column.add_child(_holder)
	_scroll = ScrollContainer.new()
	_scroll.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
	_scroll.get_v_scroll_bar().value_changed.connect(_on_scrolled)
	_apply_small_pane(_scroll.get_v_scroll_bar())
	_holder.add_child(_scroll)
	_grid = GridContainer.new()
	_grid.columns = _columns
	_grid.add_theme_constant_override("h_separation", 0)
	_grid.add_theme_constant_override("v_separation", 0)
	_grid.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_scroll.add_child(_grid)
	_rebuild_grid()
	# `Search` row visibility is decided from the unfiltered row count
	# (`ItemSelection.java:87`: `rowCount > rows * 1.5f`).
	_search_row.visible = _row_count(_visible_options()) > int(float(_rows) * 1.5)
	# `pane.setScrollYForce(block.selectScroll)` (`ItemSelection.java:104`).
	var offset := float(spec.get("select_scroll", 0.0))
	if offset > 0.0:
		_scroll.scroll_vertical = int(offset)
		_restore_scroll.call_deferred(offset)


## Deferred re-apply: the scroll range only exists after the first layout pass.
func _restore_scroll(offset: float) -> void:
	if is_instance_valid(_scroll):
		_scroll.scroll_vertical = int(offset)


## `Styles.smallPane` (`Styles.java:368-371`): the vertical track is `clear`
## and the knob the thin `scroll-knob-vertical-thin` region.
func _apply_small_pane(bar: VScrollBar) -> void:
	var track := StyleBoxEmpty.new()
	bar.add_theme_stylebox_override("scroll", track)
	bar.add_theme_stylebox_override("scroll_focus", track)
	var texture := MindWidgets.icon_texture("scroll-knob-vertical-thin")
	if texture != null:
		var knob := StyleBoxTexture.new()
		knob.texture = texture
		bar.add_theme_stylebox_override("grabber", knob)
		bar.add_theme_stylebox_override("grabber_highlight", knob)
		bar.add_theme_stylebox_override("grabber_pressed", knob)


## `ItemSelection.java:88-92`: `Icon.zoom` + the `@players.search` field.
func _build_search_row() -> HBoxContainer:
	# code-instantiated: the search row is conditional on the option count.
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 0)
	row.visible = false
	var zoom := MindWidgets.glyph("zoom", 22)
	zoom.custom_minimum_size = Vector2(CELL, CELL)
	zoom.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(zoom)
	_search = MindWidgets.field(_localized("@players.search", "search"))
	_search.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_search.text_changed.connect(_on_search_changed)
	row.add_child(_search)
	return row


## `rebuild.run()` (`ItemSelection.java:58-84`): rebuild the grid from the
## current search text, the unlock/planet filter and the button group.
func _rebuild_grid() -> void:
	for child in _grid.get_children():
		_grid.remove_child(child)
		child.queue_free()
	# `group.clear()` (`ItemSelection.java:59`) — a fresh group per rebuild.
	_group = ButtonGroup.new()
	_group.allow_unpress = true
	var text := _search.text.to_lower() if _search != null else ""
	for option in _visible_options():
		if not (option is Dictionary):
			continue
		if not text.is_empty() and not _label(option).to_lower().contains(text):
			continue
		var button := _make_button(option)
		_grid.add_child(button)
	# The pane is capped at `40 * rows` (`ItemSelection.java:111`), shrinking to
	# the grid when it is shorter.
	var content := _grid.get_combined_minimum_size()
	_holder.custom_minimum_size = Vector2(content.x, minf(content.y, CELL * float(_rows)))


## `if(!item.unlockedNow() || !item.isOnPlanet(state.getPlanet()) ||
## item.isHidden()) continue;` (`ItemSelection.java:68`) — the three filter
## results (`unlocked`, `on_planet`, `hidden`) are carried by the spec.
func _visible_options() -> Array:
	var out := []
	for option in _options:
		if not (option is Dictionary):
			continue
		if not bool(option.get("unlocked", true)) or not bool(option.get("on_planet", true)):
			continue
		if bool(option.get("hidden", false)):
			continue
		out.append(option)
	return out


## `ItemSelection.java:70-75`: a 40 px cell with the content icon at
## `clamp(selectionSize, 0, 40)`, grouped with min-check 0.
func _make_button(option: Dictionary) -> Button:
	var name := str(option.get("name", ""))
	# code-instantiated: one toggle per spec option (runtime content count).
	var button := Button.new()
	button.name = "option-%s" % name
	button.toggle_mode = true
	button.button_group = _group
	button.custom_minimum_size = Vector2(CELL, CELL)
	button.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	button.tooltip_text = _label(option)
	apply_clear_none_toggle(button)
	var texture := _icon_texture(option)
	if texture != null:
		# code-instantiated: one centered icon cell per option (the cell is the
		# `cont.defaults().size(40)` Table cell; the icon is drawn at
		# `clamp(selectionSize, 0, 40)` like `Scaling.fit` on the Java image).
		var center := CenterContainer.new()
		center.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		center.mouse_filter = Control.MOUSE_FILTER_IGNORE
		button.add_child(center)
		var icon := TextureRect.new()
		icon.texture = texture
		var size := clampf(float(option.get("size", 24.0)), 0.0, CELL)
		icon.custom_minimum_size = Vector2(size, size)
		icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
		center.add_child(icon)
	else:
		button.text = _label(option)
	if name == _current:
		# `button.update(() -> button.setChecked(holder.get() == item))`
		# (`ItemSelection.java:75`) without re-entering `picked`.
		button.set_pressed_no_signal(true)
	button.toggled.connect(func(on: bool) -> void: picked.emit(name if on else ""))
	return button


## `Styles.clearNoneTogglei` (`Styles.java:351-356`): `up = none`,
## `over = flatOver` (#454545), `down`/`checked = flatDown` (the
## `flat-down-base` 9-patch: black centre in a 4 px `Pal.accent` frame).
## Shared with `MindItemCommandRow` (J05 T2), the style Java uses for both
## panels' cells.
static func apply_clear_none_toggle(button: Button) -> void:
	var over := StyleBoxFlat.new()
	over.bg_color = Color8(0x45, 0x45, 0x45)
	over.set_content_margin_all(ICON_MARGIN)
	button.add_theme_stylebox_override("normal", StyleBoxEmpty.new())
	button.add_theme_stylebox_override("hover", over)
	button.add_theme_stylebox_override("pressed", flat_down_frame())
	button.add_theme_stylebox_override("hover_pressed", flat_down_frame())
	button.add_theme_stylebox_override("disabled", StyleBoxEmpty.new())
	button.add_theme_stylebox_override("focus", StyleBoxEmpty.new())


## `Styles.createFlatDown` (`Styles.java:479-495`).
static func flat_down_frame() -> StyleBox:
	var texture := MindWidgets.icon_texture("flat-down-base")
	if texture == null:
		var box := StyleBoxFlat.new()
		box.bg_color = Color.BLACK
		box.border_color = MindStyles.ACCENT
		box.set_border_width_all(4)
		box.set_content_margin_all(0.0)
		return box
	var frame := StyleBoxTexture.new()
	frame.texture = texture
	frame.texture_margin_left = 12.0
	frame.texture_margin_top = 12.0
	frame.texture_margin_right = 12.0
	frame.texture_margin_bottom = 12.0
	frame.set_content_margin_all(0.0)
	return frame


## `UnlockableContent.uiIcon` (`UnlockableContent.java:108-117`): the
## `{type}-{name}-ui` region, else the `fullOverride`/full-icon chain.
static func _icon_texture(option: Dictionary) -> Texture2D:
	var icons: Variant = option.get("icons", [])
	if not (icons is Array):
		return null
	for candidate in icons:
		var texture := MindWidgets.icon_texture(str(candidate))
		if texture != null:
			return texture
	return null


## `Int` division: Java's `rowCount` counts only broken rows
## (`ItemSelection.java:77-80`).
func _row_count(options: Array) -> int:
	return options.size() / _columns


func _label(option: Dictionary) -> String:
	return _localized(str(option.get("label", "")), str(option.get("name", "")))


## Resolves a bundle key through `MindAssets` with a raw fallback.
func _localized(key: String, fallback: String) -> String:
	var resolved := key.trim_prefix("@")
	if resolved.is_empty():
		return fallback
	var assets := get_node_or_null("/root/MindAssets")
	if assets == null:
		return fallback
	var value := str(assets.call("bundle_get", resolved))
	if value.is_empty() or value == resolved:
		return fallback
	return value


func _on_search_changed(_text: String) -> void:
	_rebuild_grid()


## `pane.update(() -> block.selectScroll = pane.getScrollY())`
## (`ItemSelection.java:105-107`).
func _on_scrolled(offset: float) -> void:
	if _block.is_empty():
		return
	var input := get_node_or_null("/root/Spine/Input")
	if input != null and input.has_method("set_config_scroll"):
		input.call("set_config_scroll", _block, offset)
