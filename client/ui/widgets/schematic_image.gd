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
	# Tiled `schematic-background` under everything (the PNG is not packed into
	# the atlas yet: a flat dark backing keeps the card readable; noted in the
	# commit).
	var background := MindWidgets.icon_texture("schematic-background")
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
	# Plan regions (`fullIcon`) at `(x + 1.5, y + 1.5)` tiles.
	for tile_variant in _tiles:
		var tile: Dictionary = tile_variant
		var name := str(tile.get("block", ""))
		var texture := MindWidgets.icon_texture("block-%s-full" % name)
		if texture == null:
			texture = MindWidgets.icon_texture("block-%s-ui" % name)
		if texture == null:
			continue
		var footprint := float(int(tile.get("size", 1)))
		var center := origin + Vector2(
			float(int(tile.get("x", 0))) + footprint / 2.0,
			float(int(tile.get("y", 0))) + footprint / 2.0
		) * resolution
		var half := Vector2(footprint, footprint) * resolution / 2.0
		var rect := Rect2(-half, half * 2.0)
		if bool(tile.get("rotate", false)) and int(tile.get("rotation", 0)) != 0:
			draw_set_transform(center, deg_to_rad(float(int(tile.get("rotation", 0))) * 90.0), Vector2.ONE)
			draw_texture_rect(texture, rect, false)
			draw_set_transform(Vector2.ZERO, 0.0, Vector2.ONE)
		else:
			draw_texture_rect(texture, Rect2(center - half, half * 2.0), false)
	# 4 px border: Pal.gray normally, accent while hovered.
	draw_rect(
		Rect2(Vector2.ZERO, size),
		BORDER_ACCENT if _hovered else BORDER_GRAY,
		false,
		THICKNESS
	)


## One black pixel per occupied cell at `(1 + x + (size-1)/2)`
## (`getBuffer`'s `padding/2 + wx + 0.5`).
func _build_silhouette() -> ImageTexture:
	var image := Image.create(_width + 2, _height + 2, false, Image.FORMAT_RGBA8)
	image.fill(Color(0.0, 0.0, 0.0, 0.0))
	for tile_variant in _tiles:
		var tile: Dictionary = tile_variant
		var offset := int((int(tile.get("size", 1)) - 1) / 2)
		var x := 1 + int(tile.get("x", 0)) + offset
		var y := 1 + int(tile.get("y", 0)) + offset
		if x < 0 or y < 0 or x >= image.get_width() or y >= image.get_height():
			continue
		image.set_pixel(x, y, Color(0.0, 0.0, 0.0, 1.0))
	return ImageTexture.create_from_image(image)
