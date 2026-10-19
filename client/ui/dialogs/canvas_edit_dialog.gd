## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/CanvasEditDialog.java (plan 13 M4, T14).
##
## Canvas editor: a `canvasSize` x `canvasSize` paint surface over the block's
## palette. Left click paints, left drag paints a Bresenham line, middle click
## picks the colour; `fill` / `grid` toggles mirror the Java buttons
## (`CanvasEditDialog.java:190-212`). Applying packs the pixels LSB-first
## (`CanvasBlock.setByte`, bpp = log2(nextPowerOfTwo(palette.length))) and sends
## them through `MindInput.configure_building_bytes` (`CanvasEditDialog.save`
## -> `canvas.configure(packPixmap(pix))`).
##
## `@import`/`@export` PNG and the 2-second auto-save mirror
## `CanvasEditDialog.java:46-71,216-254`; the hover-cell outline stays deferred.

extends MindDialog

## Display-side state, seeded from `SimHost.logic_canvas_sync`.
var _x := 0
var _y := 0
var _size := 12
var _bpp := 3
var _palette: PackedInt64Array = PackedInt64Array()
## One palette index per pixel (`pos = y * size + x`, row 0 = top).
var _pixels: PackedByteArray = PackedByteArray()
var _cur_color := 0
var _fill := false
var _grid := true
var _modified := false
var _canvas: Control = null
var _palette_row: HBoxContainer = null
var _swatches: Array[Button] = []
var _last_cell := Vector2i(-1, -1)
var _texture: ImageTexture = null
## `refreshTime = 60 * 2` (`CanvasEditDialog.java:24`): auto-save accumulator.
var _save_timer := 0.0


func _ready() -> void:
	# `CanvasEditDialog` removes the title table (`:36`).
	should_pause = false
	full_dialog = false
	super._ready()
	set_title_text("")
	_build()


func _build() -> void:
	var table := content_table()
	# `cont.table(Tex.pane, body -> ...)` (`:73`): the paint element.
	_canvas = Control.new()
	# code-instantiated: the paint surface size is the block's canvas size; it
	# has no scene-authorable shape.
	# `CanvasEditDialog.java:185`: 480 px on desktop.
	_canvas.custom_minimum_size = Vector2(480, 480)
	_canvas.gui_input.connect(_on_canvas_input)
	_canvas.draw.connect(_draw_canvas)
	table.add(_canvas)
	table.row()

	var controls := HBoxContainer.new()
	table.add(controls)
	var grid_button := MindWidgets.glyph_button("grid", "grid", "clearNoneTogglei")
	grid_button.toggle_mode = true
	grid_button.button_pressed = true
	grid_button.custom_minimum_size = Vector2(44, 44)
	grid_button.tooltip_text = _t("@grid")
	grid_button.toggled.connect(
		func(pressed: bool) -> void:
			_grid = pressed
			_canvas.queue_redraw()
	)
	controls.add_child(grid_button)

	_palette_row = HBoxContainer.new()
	controls.add_child(_palette_row)

	# `Icon.fill` has no icon-font alias yet; `glyph_button` falls back to text.
	var fill_button := MindWidgets.glyph_button("fill", "fill", "clearNoneTogglei")
	fill_button.toggle_mode = true
	fill_button.custom_minimum_size = Vector2(44, 44)
	fill_button.tooltip_text = _t("@fill")
	fill_button.toggled.connect(func(pressed: bool) -> void: _fill = pressed)
	controls.add_child(fill_button)

	add_close_button(160.0)
	var import_button := add_button(_t("@import"), _import_png, "download", 160.0)
	import_button.custom_minimum_size.y = 64.0
	var export_button := add_button(_t("@export"), _export_png, "upload", 160.0)
	export_button.custom_minimum_size.y = 64.0


## `CanvasEditDialog.update`: `if(refreshTime >= 60*2) save()`.
func _process(delta: float) -> void:
	if not visible or not _modified:
		return
	_save_timer += delta * 60.0
	if _save_timer >= 120.0:
		_save_timer = 0.0
		_save()


func shown() -> void:
	_x = int(_context.get("x", 0))
	_y = int(_context.get("y", 0))
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null:
		return
	var rows: Array = host.call("logic_canvas_sync", -1.0e9, -1.0e9, 1.0e9, 1.0e9)
	for row in rows:
		if not (row is Dictionary):
			continue
		if int(row.get("x", -1)) == _x and int(row.get("y", -1)) == _y:
			_size = maxi(1, int(row.get("pixels", _size)))
			_bpp = maxi(1, int(row.get("bpp", _bpp)))
			var palette: Variant = row.get("palette", PackedInt64Array())
			if palette is PackedInt64Array:
				_palette = palette
			break
	var data: Variant = host.call("logic_canvas_pixels", _x, _y)
	if data is PackedByteArray and data.size() == _size * _size:
		_pixels = data
	else:
		_pixels = PackedByteArray()
		_pixels.resize(_size * _size)
	_cur_color = clampi(_cur_color, 0, maxi(0, _palette.size() - 1))
	_modified = false
	_refresh_texture()
	_rebuild_swatches()
	if _canvas != null:
		_canvas.queue_redraw()


## Applies the packed pixmap on hide (`CanvasEditDialog.hidden` -> `save`).
func hidden() -> void:
	_save()


## Rebuilds the palette swatches for the block's palette
## (`CanvasEditDialog.java:194-207`).
func _rebuild_swatches() -> void:
	if _palette_row == null:
		return
	for child in _palette_row.get_children():
		child.queue_free()
	_swatches.clear()
	for index in _palette.size():
		var button := Button.new()
		# code-instantiated: one palette swatch per canvas palette colour.
		button.custom_minimum_size = Vector2(40, 40)
		button.toggle_mode = true
		button.theme_type_variation = "squareTogglei"
		button.button_pressed = index == _cur_color
		button.modulate = _rgba8(int(_palette[index]))
		button.pressed.connect(_select_color.bind(index))
		_palette_row.add_child(button)
		_swatches.append(button)


func _select_color(index: int) -> void:
	_cur_color = index
	_refresh_swatches()


func _refresh_swatches() -> void:
	for index in _swatches.size():
		_swatches[index].button_pressed = index == _cur_color


## `packPixmap`: LSB-first `bpp`-bit packing (`CanvasBlock.java:216-227`).
func _pack() -> PackedByteArray:
	var packed := PackedByteArray()
	@warning_ignore("integer_division")
	var bytes := (_size * _size * _bpp + 7) / 8
	packed.resize(bytes)
	for pos in _size * _size:
		var value := int(_pixels[pos]) if pos < _pixels.size() else 0
		var bit_offset := pos * _bpp
		for i in _bpp:
			var word := (bit_offset + i) >> 3
			var mask := 1 << ((bit_offset + i) & 7)
			if (value >> i) & 1 != 0:
				packed[word] = int(packed[word]) | mask
			else:
				packed[word] = int(packed[word]) & ~mask
	return packed


## `CanvasEditDialog.save`: `canvas.configure(canvas.packPixmap(pix))`.
func _save() -> void:
	if not _modified:
		return
	var input := get_node_or_null("/root/Spine/Input")
	if input == null or not input.has_method("configure_building_bytes"):
		return
	input.call("configure_building_bytes", _x, _y, _pack())
	_modified = false


## `@export`: `file.writePng(pix)` (`CanvasEditDialog.java:216-222`).
func _export_png() -> void:
	var dialog := FileDialog.new()
	dialog.file_mode = FileDialog.FILE_MODE_SAVE_FILE
	dialog.access = FileDialog.ACCESS_FILESYSTEM
	dialog.add_filter("*.png", "PNG")
	dialog.file_selected.connect(func(path: String) -> void:
		var image := _pixels_image()
		if image != null:
			image.save_png(path)
		dialog.queue_free())
	dialog.canceled.connect(dialog.queue_free)
	add_child(dialog)
	dialog.popup_centered()


## `@import`: decode, scale/centre and map every pixel to the nearest palette
## entry (`CanvasEditDialog.java:224-274`).
func _import_png() -> void:
	var dialog := FileDialog.new()
	dialog.file_mode = FileDialog.FILE_MODE_OPEN_FILE
	dialog.access = FileDialog.ACCESS_FILESYSTEM
	dialog.add_filter("*.png,*.jpg,*.jpeg", "Images")
	dialog.file_selected.connect(func(path: String) -> void:
		_import_image(path)
		dialog.queue_free())
	dialog.canceled.connect(dialog.queue_free)
	add_child(dialog)
	dialog.popup_centered()


func _import_image(path: String) -> void:
	var image := Image.load_from_file(path)
	if image == null:
		return
	image.convert(Image.FORMAT_RGBA8)
	# Scale preserving aspect when larger, else fill with palette[0] and centre.
	var base := _rgba8(int(_palette[0])) if not _palette.is_empty() else Color.BLACK
	var target := _size
	var scaled := image
	if image.get_width() > target or image.get_height() > target:
		var ratio := minf(float(target) / float(image.get_width()), float(target) / float(image.get_height()))
		var w := maxi(1, roundi(image.get_width() * ratio))
		var h := maxi(1, roundi(image.get_height() * ratio))
		image.resize(w, h, Image.INTERPOLATE_BILINEAR)
		scaled = image
	@warning_ignore("integer_division")
	var ox := (target - scaled.get_width()) / 2
	@warning_ignore("integer_division")
	var oy := (target - scaled.get_height()) / 2
	for y in target:
		for x in target:
			var color := base
			var sx := x - ox
			var sy := y - oy
			if sx >= 0 and sy >= 0 and sx < scaled.get_width() and sy < scaled.get_height():
				var pixel := scaled.get_pixel(sx, sy)
				# `findClosest`: blend the source alpha over palette[0].
				color = base.blend(pixel) if pixel.a < 1.0 else pixel
			_pixels[y * target + x] = _nearest_palette(color)
	_modified = true
	_refresh_texture()
	_canvas.queue_redraw()


func _pixels_image() -> Image:
	var image := Image.create(_size, _size, false, Image.FORMAT_RGBA8)
	for y in _size:
		for x in _size:
			image.set_pixel(x, y, _rgba8(int(_palette[_pixels[y * _size + x]])) if _palette.size() > 0 else Color.BLACK)
	return image


func _nearest_palette(color: Color) -> int:
	var best := 0
	var best_distance := INF
	for index in _palette.size():
		var candidate := _rgba8(int(_palette[index]))
		var distance := pow(color.r - candidate.r, 2.0) + pow(color.g - candidate.g, 2.0) + pow(color.b - candidate.b, 2.0)
		if distance < best_distance:
			best_distance = distance
			best = index
	return best


## Paint surface input (`CanvasEditDialog`'s `InputListener`; `:87-135`).
func _on_canvas_input(event: InputEvent) -> void:
	if _canvas == null:
		return
	if event is InputEventMouseButton:
		var button := event as InputEventMouseButton
		var cell := _cell_at(button.position)
		if button.button_index == MOUSE_BUTTON_LEFT and button.pressed:
			if _fill:
				_flood_fill(cell)
			else:
				_paint(cell)
				_last_cell = cell
			_canvas.queue_redraw()
		elif button.button_index == MOUSE_BUTTON_MIDDLE and button.pressed:
			var pos := _index_at(cell)
			if pos >= 0 and pos < _pixels.size():
				_cur_color = int(_pixels[pos])
				_refresh_swatches()
	elif event is InputEventMouseMotion and _last_cell.x >= 0:
		var motion := event as InputEventMouseMotion
		if motion.button_mask & MOUSE_BUTTON_MASK_LEFT == 0:
			return
		if _fill:
			return
		# `Bresenham2.line(lastX, lastY, cx, cy, this::draw)`.
		for cell in _line_cells(_last_cell, _cell_at(motion.position)):
			_paint(cell)
		_last_cell = _cell_at(motion.position)
		_canvas.queue_redraw()


func _cell_size() -> Vector2:
	var rect := _canvas.get_rect().size
	if _size <= 0:
		return Vector2.ONE
	return Vector2(maxf(1.0, rect.x / _size), maxf(1.0, rect.y / _size))


## `convertX`/`convertY` (`:78-84`): screen cell -> canvas cell (row 0 = top).
func _cell_at(screen_pos: Vector2) -> Vector2i:
	var cell := _cell_size()
	return Vector2i(
		int(screen_pos.x / cell.x), int(screen_pos.y / cell.y)
	)


func _index_at(cell: Vector2i) -> int:
	if cell.x < 0 or cell.y < 0 or cell.x >= _size or cell.y >= _size:
		return -1
	return cell.y * _size + cell.x


## `draw(x, y)`: set when in bounds and different (`:138-144`).
func _paint(cell: Vector2i) -> void:
	var pos := _index_at(cell)
	if pos < 0 or pos >= _pixels.size():
		return
	if int(_pixels[pos]) == _cur_color:
		return
	_pixels[pos] = _cur_color
	_modified = true
	_set_pixel_texture(cell.x, cell.y)


## `fill` mode flood fill (`:94-114`).
func _flood_fill(cell: Vector2i) -> void:
	var start := _index_at(cell)
	if start < 0:
		return
	var dst := int(_pixels[start])
	if dst == _cur_color:
		return
	var stack: Array[int] = [start]
	while not stack.is_empty():
		var pos: int = stack.pop_back()
		var cx := pos % _size
		@warning_ignore("integer_division")
		var cy := pos / _size
		_pixels[pos] = _cur_color
		_modified = true
		_set_pixel_texture(cx, cy)
		var offsets: Array[Vector2i] = [
			Vector2i(1, 0), Vector2i(-1, 0), Vector2i(0, 1), Vector2i(0, -1)
		]
		for offset in offsets:
			var nx := cx + offset.x
			var ny := cy + offset.y
			if nx < 0 or ny < 0 or nx >= _size or ny >= _size:
				continue
			var next := ny * _size + nx
			if int(_pixels[next]) == dst:
				stack.append(next)


## Bresenham cells between two canvas cells.
func _line_cells(from: Vector2i, to: Vector2i) -> Array[Vector2i]:
	var cells: Array[Vector2i] = []
	var x := from.x
	var y := from.y
	var dx := absi(to.x - x)
	var dy := absi(to.y - y)
	var sx := 1 if x < to.x else -1
	var sy := 1 if y < to.y else -1
	var err := dx - dy
	while true:
		cells.append(Vector2i(x, y))
		if x == to.x and y == to.y:
			break
		var e2 := 2 * err
		if e2 > -dy:
			err -= dy
			x += sx
		if e2 < dx:
			err += dx
			y += sy
	return cells


## Rebuilds the pixel texture from `_pixels` (top row first).
func _refresh_texture() -> void:
	var image := Image.create(_size, _size, false, Image.FORMAT_RGBA8)
	for pos in _size * _size:
		var index := int(_pixels[pos]) if pos < _pixels.size() else 0
		var color := _rgba8(int(_palette[index]) if index < _palette.size() else 0)
		@warning_ignore("integer_division")
		image.set_pixel(pos % _size, pos / _size, color)
	_texture = ImageTexture.create_from_image(image)
	if _canvas != null:
		_canvas.queue_redraw()


func _set_pixel_texture(x: int, y: int) -> void:
	if _texture == null:
		return
	var image := _texture.get_image()
	var index := int(_pixels[y * _size + x])
	var color := _rgba8(int(_palette[index]) if index < _palette.size() else 0)
	image.set_pixel(x, y, color)
	_texture.update(image)
	if _canvas != null:
		_canvas.queue_redraw()


## `Pixmap.get`-style RGBA8888 -> `Color`.
func _rgba8(value: int) -> Color:
	return Color8(
		(value >> 24) & 0xff, (value >> 16) & 0xff, (value >> 8) & 0xff, value & 0xff
	)


func _draw_canvas() -> void:
	if _canvas == null or _texture == null:
		return
	var rect := _canvas.get_rect().size
	_canvas.draw_texture_rect(_texture, Rect2(Vector2.ZERO, rect), false)
	if not _grid:
		return
	# `CanvasEditDialog`'s grid (`:152-170`): a line every `jump` cells.
	var cell := _cell_size()
	var min_space := 10.0
	var jump_x := maxi(1, int(maxf(min_space, cell.x) / cell.x))
	var jump_y := maxi(1, int(maxf(min_space, cell.y) / cell.y))
	for col in range(0, _size + 1, jump_x):
		_canvas.draw_rect(
			Rect2(Vector2(col * cell.x - 1.0, 0.0), Vector2(2.0, rect.y)), Color(0, 0, 0, 0.35)
		)
	for row in range(0, _size + 1, jump_y):
		_canvas.draw_rect(
			Rect2(Vector2(0.0, row * cell.y - 1.0), Vector2(rect.x, 2.0)), Color(0, 0, 0, 0.35)
		)
