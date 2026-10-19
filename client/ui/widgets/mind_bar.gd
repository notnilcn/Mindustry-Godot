## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/Bar.java (plan 14 §3.3).
##
## HUD/stat bar: the `Tex.bar` 9-patch backing, the `Tex.barTop` 9-patch fill
## tinted by the value colour (lerped, with a blink flash when the value drops),
## optional outline, and a centered `Fonts.outline` label. Bound to a
## `Callable` returning the fraction (GDScript) or a `BarModel` id (`MindHud`).

class_name MindBar
extends Control

## `Fonts.outline` border (`ui/Fonts.java:247-249, 264-271`): 2 px of
## `Color.darkGray`. Godot's glyph stroke radius is `outline_size / 4` px (see
## `MindThemeBuilder` `outlineLabel`), so 8 draws Java's 2 px border.
const LABEL_OUTLINE_SIZE := 8
const LABEL_OUTLINE_COLOR := Color(0x3f / 255.0, 0x3f / 255.0, 0x3f / 255.0, 1.0)
## Pack splits `[left, right, top, bottom]` of `bar.9.png` / `bar-top.9.png`,
## used when the atlas carries none.
const BAR_SPLITS := [9, 9, 9, 9]
const BAR_TOP_SPLITS := [9, 10, 9, 10]

var bar_name := ""
var fraction := 0.0
var bar_color := Color.WHITE
var blink_color := Color.WHITE
var outline_color := Color(0.0, 0.0, 0.0, 0.0)
var outline_radius := 0.0

var _value := 0.0
var _last_value := 0.0
var _blink := 0.0


## Initializes the bar (`Bar(name, color, fraction)` / `Bar.set`): the blink
## colour starts as the bar colour (`blinkColor.set(color)`), so only bars that
## call `blink(...)` flash a different colour.
func setup(new_name: String, color: Color, frac: float) -> void:
	bar_name = new_name
	bar_color = color
	blink_color = color
	fraction = clampf(frac, 0.0, 1.0)
	_value = fraction
	_last_value = fraction
	queue_redraw()


## Resets the animated value (`Bar.reset`: `value = lastValue = blink = value`).
func reset(frac: float) -> void:
	_value = frac
	_last_value = frac
	_blink = frac
	queue_redraw()


## Snaps to the current fraction (`Bar.snap`).
func snap() -> void:
	_value = fraction
	_last_value = fraction
	queue_redraw()


## Triggers a blink flash (`Bar.flash`).
func flash() -> void:
	_blink = 1.0
	queue_redraw()


## Sets the flash colour (`Bar.blink`).
func blink(color: Color) -> MindBar:
	blink_color = color
	return self


## Sets the outline (`Bar.outline`).
func outline(color: Color, stroke: float) -> void:
	outline_color = color
	outline_radius = stroke
	queue_redraw()


## The animation half of `Bar.draw()` (`ui/Bar.java:77-98`): a drop below the
## last value starts a blink; blink and value ease with `Mathf.lerpDelta`.
func _process(delta: float) -> void:
	var computed := clampf(fraction, 0.0, 1.0)
	if is_nan(computed):
		computed = 0.0
	if _last_value > computed:
		_blink = 1.0
		_last_value = computed
	if is_nan(_last_value):
		_last_value = 0.0
	if is_inf(_last_value):
		_last_value = 1.0
	if is_nan(_value):
		_value = 0.0
	if is_inf(_value):
		_value = 1.0
	# `Mathf.lerpDelta(a, b, p)` = `lerp(a, b, clamp(p * Time.delta))`, with
	# `Time.delta` in 60 Hz ticks.
	var ticks := delta * 60.0
	_blink = lerpf(_blink, 0.0, clampf(0.2 * ticks, 0.0, 1.0))
	_value = lerpf(_value, computed, clampf(0.15 * ticks, 0.0, 1.0))
	queue_redraw()


## `Bar.draw()` (`ui/Bar.java:100-136`).
func _draw() -> void:
	var bar := MindWidgets.icon_texture("bar")
	var bar_splits := _splits("bar", BAR_SPLITS)
	if outline_radius > 0.0:
		var outer := Rect2(
			Vector2(-outline_radius, -outline_radius),
			size + Vector2(outline_radius, outline_radius) * 2.0
		)
		_draw_nine_patch(bar, bar_splits, outer, outline_color, outer.size.x)
	# `Draw.colorl(0.1f)`: the backing is the `bar` patch at 10% grey. Parent
	# alpha is applied by Godot's modulate chain (`Draw.alpha(parentAlpha)`).
	var full := Rect2(Vector2.ZERO, size)
	_draw_nine_patch(bar, bar_splits, full, Color(0.1, 0.1, 0.1, 1.0), size.x)
	var top_color := bar_color.lerp(blink_color, _blink)
	top_color.a = 1.0
	var top := MindWidgets.icon_texture("bar-top")
	var top_splits := _splits("bar-top", BAR_TOP_SPLITS)
	var top_width := size.x * _value
	var region_width := float(top.get_width()) if top != null else 0.0
	if top_width > region_width:
		_draw_nine_patch(top, top_splits, Rect2(Vector2.ZERO, Vector2(top_width, size.y)), top_color, top_width)
	else:
		# Below the region width the patch keeps its packed width and is
		# scissored to `topWidth` (`ScissorStack.push`, which rounds the rect
		# and skips a clip narrower than 1 px).
		var clip := roundf(top_width)
		if clip >= 1.0:
			_draw_nine_patch(top, top_splits, Rect2(Vector2.ZERO, Vector2(region_width, size.y)), top_color, clip)
	if not bar_name.is_empty():
		_draw_name()


## Atlas splits `[left, right, top, bottom]` for a 9-patch region.
func _splits(region: String, fallback: Array) -> Array:
	var assets := MindWidgets.assets()
	if assets != null and assets.has_method("tex_splits"):
		var splits: PackedInt32Array = assets.call("tex_splits", region)
		if splits.size() == 4:
			return Array(splits)
	return fallback


## Arc `NinePatch.draw` (`NinePatch.java:353-374`): the corners keep their
## packed size, the edges/centre stretch, and a patch whose rect is too small
## for its splits collapses its middle row/column. Pieces are clipped on the
## right at `clip_x` (local x) for the scissored `barTop` path. Without the
## atlas region the rect is drawn flat.
func _draw_nine_patch(texture: Texture2D, splits: Array, rect: Rect2, color: Color, clip_x: float) -> void:
	if texture == null:
		var flat_rect := rect
		flat_rect.size.x = minf(rect.size.x, clip_x - rect.position.x)
		if flat_rect.size.x > 0.0:
			draw_rect(flat_rect, color, true)
		return
	var tw := float(texture.get_width())
	var th := float(texture.get_height())
	var left := float(splits[0])
	var right := float(splits[1])
	var top := float(splits[2])
	var bottom := float(splits[3])
	# Arc lays the patch out y-up; in y-down terms the top row starts at the
	# rect top, the bottom row ends at the rect bottom and the middle row spans
	# what is left. Arc draws the bottom row first, so when the rect is shorter
	# than `top + bottom` the top row paints over the overlap.
	var xs := [
		[rect.position.x, left, 0.0, left],
		[rect.position.x + left, rect.size.x - left - right, left, tw - left - right],
		[rect.end.x - right, right, tw - right, right],
	]
	var ys := [
		[rect.end.y - bottom, bottom, th - bottom, bottom],
		[rect.position.y + top, rect.size.y - top - bottom, top, th - top - bottom],
		[rect.position.y, top, 0.0, top],
	]
	for row in ys:
		if row[1] <= 0.0 or row[3] <= 0.0:
			continue
		for col in xs:
			if col[1] <= 0.0 or col[3] <= 0.0:
				continue
			var dest_x: float = col[0]
			var dest_w: float = col[1]
			var src_x: float = col[2]
			var src_w: float = col[3]
			var visible_w := minf(dest_w, clip_x - dest_x)
			if visible_w <= 0.0:
				continue
			var src_visible := src_w * visible_w / dest_w
			draw_texture_rect_region(
				texture,
				Rect2(dest_x, row[0], visible_w, row[1]),
				Rect2(src_x, row[2], src_visible, row[3]),
				color
			)


## The centered `Fonts.outline` label: white with the 2 px dark-grey border,
## at `x + width/2 - lay.width/2`, `y + height/2 + lay.height/2 + 1`
## (`ui/Bar.java:126-133`, y-up; `lay.height` is the cap height).
func _draw_name() -> void:
	var font := get_theme_default_font()
	if font == null:
		return
	var font_size := get_theme_default_font_size()
	var text_size := font.get_string_size(bar_name, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size)
	# Java puts the cap top `lay.height/2 + 1` above the centre (y-up); with
	# this font's line height that lands the baseline 5 px above
	# `(height + lineHeight) / 2`. `lay.width` of `Fonts.outline` includes the
	# last glyph's 2 px border, so the run sits 1 px left of the plain centre.
	# Arc draws the cached glyphs on whole pixels, so the origin is rounded
	# (twin-measured against the 18 px HUD bars).
	var pos := Vector2(
		roundf((size.x - text_size.x) * 0.5 - 1.0), roundf((size.y + text_size.y) * 0.5 - 5.0)
	)
	draw_string_outline(
		font, pos, bar_name, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size,
		LABEL_OUTLINE_SIZE, LABEL_OUTLINE_COLOR
	)
	draw_string(font, pos, bar_name, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, Color.WHITE)
