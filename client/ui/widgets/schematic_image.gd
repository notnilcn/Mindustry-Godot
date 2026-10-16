## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/SchematicsDialog.java
##         (`SchematicImage`, end of file) + game/Schematics.java:200-290
##         `getBuffer` (background, 1-px silhouette, plan regions, border).
##
## The 200x200 schematic preview: tiled `schematic-background`, a soft black
## silhouette built from a 1-px-per-tile image stretched with linear filtering,
## each tile's `block-<name>-full` region (rotated when `rotate`), and the 4 px
## border (Pal.gray, accent while the parent button is hovered).

class_name MindSchematicImage
extends Control

## `SchematicImage.thickness` (border width).
const THICKNESS := 4.0
## `Pal.gray` / `Pal.accent` borders.
const BORDER_GRAY := Color(0.271, 0.271, 0.271)
const BORDER_ACCENT := Color(1.0, 0.827, 0.498)

var _width := 0
var _height := 0
var _tiles: Array = []
var _hovered := false
var _silhouette: ImageTexture = null
## `schematic-background` loaded from the assets dir (the sprite is not packed
## into the atlas; `MindAssets.assets_dir()` resolves the root).
var _background: Texture2D = null


func _init() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR


## Binds one `SchematicView` row (`width`/`height`/`tiles`).
func setup(schematic: Dictionary) -> void:
	_width = int(schematic.get("width", 0))
	_height = int(schematic.get("height", 0))
	_tiles = schematic.get("tiles", [])
	_silhouette = null
	queue_redraw()


func set_hovered(value: bool) -> void:
	if _hovered == value:
		return
	_hovered = value
	queue_redraw()


func _draw() -> void:
	if _width <= 0 or _height <= 0 or size.x <= 0.0 or size.y <= 0.0:
		return
	# Tiled `schematic-background` under everything (atlas first, then the raw
	# PNG through `MindAssets.assets_dir()`; a flat dark backing is the last
	# resort).
	var background := MindWidgets.icon_texture("schematic-background")
	if background == null:
		background = _background_texture()
	if background != null:
		draw_texture_rect(background, Rect2(Vector2.ZERO, size), true)
	else:
		draw_rect(Rect2(Vector2.ZERO, size), Color(0.05, 0.05, 0.05, 0.85))
	# Tile area with one padding cell on each side (`getBuffer` padding 2/2).
	var resolution := minf(size.x / float(_width + 2), size.y / float(_height + 2))
	var origin := Vector2(
		(size.x - _width * resolution) / 2.0, (size.y - _height * resolution) / 2.0
	)
	if _silhouette == null:
		_silhouette = _build_silhouette()
	if _silhouette != null:
		# 1-px-per-tile image stretched over the padded area: the linear filter
		# turns each occupied cell into a soft shadow (`getBuffer`).
		draw_texture_rect(
			_silhouette,
			Rect2(origin - Vector2(resolution, resolution), Vector2(_width + 2, _height + 2) * resolution),
			false
		)
	# Plan regions (`drawPlanRegion`: `fullIcon` at `plan.drawx()/drawy()`,
	# rotated by `rotation * 90` when the block rotates).
	for tile_variant in _tiles:
		var tile: Dictionary = tile_variant
		var name := str(tile.get("block", ""))
		var texture := MindWidgets.icon_texture("block-%s-full" % name)
		if texture == null:
			texture = MindWidgets.icon_texture("block-%s-ui" % name)
		if texture == null:
			continue
		var footprint := float(int(tile.get("size", 1)))
		var center := _tile_center(tile, origin, resolution)
		var half := Vector2(footprint, footprint) * resolution / 2.0
		var rect := Rect2(-half, half * 2.0)
		if bool(tile.get("rotate", false)) and int(tile.get("rotation", 0)) != 0:
			# Java angles are counter-clockwise in y-up space; Godot's canvas
			# is y-down, so the angle is negated.
			draw_set_transform(center, -deg_to_rad(float(int(tile.get("rotation", 0))) * 90.0), Vector2.ONE)
			draw_texture_rect(texture, rect, false)
			draw_set_transform(Vector2.ZERO, 0.0, Vector2.ONE)
		else:
			draw_texture_rect(texture, Rect2(center - half, half * 2.0), false)
	# `plans.each(req -> req.block.drawPlanConfigTop(req, eachPlans))`: power
	# nodes draw a laser to every linked plan (`PowerNode.java:335-351`).
	_draw_lasers(origin, resolution)
	# 4 px border: Pal.gray normally, accent while hovered.
	draw_rect(
		Rect2(Vector2.ZERO, size),
		BORDER_ACCENT if _hovered else BORDER_GRAY,
		false,
		THICKNESS
	)


## `BuildPlan.drawx()/drawy()` in widget pixels: the tile centre plus
## `Block.offset` (`(size + 1) % 2 * tilesize / 2`, half a tile for even
## sizes). Java's preview is y-up (`Schematics.getBuffer`), so rows count up
## from the bottom of the tile area.
func _tile_center(tile: Dictionary, origin: Vector2, resolution: float) -> Vector2:
	var footprint := int(tile.get("size", 1))
	var offset := 0.5 if footprint % 2 == 0 else 0.0
	var tx := float(int(tile.get("x", 0))) + 0.5 + offset
	var ty := float(int(tile.get("y", 0))) + 0.5 + offset
	return origin + Vector2(tx, float(_height) - ty) * resolution


## `PowerNode.drawPlanConfigTop` -> `drawLaser(.., false)` -> `Drawf.laser`
## (`PowerNode.java:183-197`, `Drawf.java:521-541`) in world units scaled by
## `resolution / tilesize`: endpoints pulled in by `size * 4 - 1.5`, the
## `laser` line (stroke `12 * laserScale`) between `laser-end` caps sized
## `region * laserScale * Draw.scl`; `setupColor(1)` is `laserColor1`
## (white) at `Renderer.laserOpacity`.
func _draw_lasers(origin: Vector2, resolution: float) -> void:
	var laser := MindWidgets.icon_texture("laser")
	var laser_end := MindWidgets.icon_texture("laser-end")
	if laser == null and laser_end == null:
		return
	var unit := resolution / 8.0
	var color := Color(1.0, 1.0, 1.0, _laser_opacity())
	if color.a <= 0.0:
		return
	for tile_variant in _tiles:
		var tile: Dictionary = tile_variant
		var targets: Array = tile.get("lasers", [])
		if targets.is_empty():
			continue
		var from := _tile_center(tile, origin, resolution)
		var size1 := float(int(tile.get("size", 1)))
		for target_variant in targets:
			var target_index := int(target_variant)
			if target_index < 0 or target_index >= _tiles.size():
				continue
			var other: Dictionary = _tiles[target_index]
			var to := _tile_center(other, origin, resolution)
			if from.is_equal_approx(to):
				continue
			var size2 := float(int(other.get("size", 1)))
			var dir := (to - from).normalized()
			var p1 := from + dir * (size1 * 4.0 - 1.5) * unit
			var p2 := to - dir * (size2 * 4.0 - 1.5) * unit
			var angle := dir.angle()
			if laser_end != null:
				var cap := laser_end.get_size() * 0.25 * 0.25 * unit
				# `Draw.rect(start, x, y, .., rot + 180)` / `Draw.rect(end, x2, y2, .., rot)`.
				draw_set_transform(p1, angle + PI, Vector2.ONE)
				draw_texture_rect(laser_end, Rect2(-cap / 2.0, cap), false, color)
				draw_set_transform(p2, angle, Vector2.ONE)
				draw_texture_rect(laser_end, Rect2(-cap / 2.0, cap), false, color)
			if laser != null:
				# `Lines.line(line, x + vx, y + vy, x2 - vx, y2 - vy, false)`,
				# `vx = cos(rot) * 8 * scale * Draw.scl`.
				var inset := dir * 0.5 * unit
				var a := p1 + inset
				var b := p2 - inset
				var length := a.distance_to(b)
				var stroke := 3.0 * unit
				draw_set_transform(a, angle, Vector2.ONE)
				draw_texture_rect(laser, Rect2(0.0, -stroke / 2.0, length, stroke), false, color)
			draw_set_transform(Vector2.ZERO, 0.0, Vector2.ONE)


## `Renderer.laserOpacity` (`lasersopacity` setting / 100, default 100).
func _laser_opacity() -> float:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("settings_get_int"):
		return 1.0
	return clampf(float(int(ui.call("settings_get_int", "lasersopacity", 100))) / 100.0, 0.0, 1.0)


## Loads `sprites/schematic-background.png` once through the asset root.
func _background_texture() -> Texture2D:
	if _background != null:
		return _background
	var assets := get_node_or_null("/root/MindAssets")
	if assets == null or not assets.has_method("assets_dir"):
		return null
	var path := "%s/sprites/schematic-background.png" % str(assets.call("assets_dir"))
	var image := Image.load_from_file(path)
	if image == null:
		return null
	_background = ImageTexture.create_from_image(image)
	return _background


## One black pixel per footprint cell (`getBuffer`: for every `dx, dy` in the
## block size, `Fill.square(padding/2 + wx + 0.5, padding/2 + wy + 0.5, 0.5)`
## with `wx = x + dx - (size - 1) / 2`). The buffer is y-up, so image rows
## count down from the top (`row = height - wy`).
func _build_silhouette() -> ImageTexture:
	var image := Image.create(_width + 2, _height + 2, false, Image.FORMAT_RGBA8)
	image.fill(Color(0.0, 0.0, 0.0, 0.0))
	for tile_variant in _tiles:
		var tile: Dictionary = tile_variant
		var footprint := int(tile.get("size", 1))
		var offset := -int((footprint - 1) / 2)
		for dx in footprint:
			for dy in footprint:
				var x := 1 + int(tile.get("x", 0)) + dx + offset
				var y := _height - (int(tile.get("y", 0)) + dy + offset)
				if x < 0 or y < 0 or x >= image.get_width() or y >= image.get_height():
					continue
				image.set_pixel(x, y, Color(0.0, 0.0, 0.0, 1.0))
	return ImageTexture.create_from_image(image)
