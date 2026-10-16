## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/graphics/MenuRenderer.java (`drawFlyers`, `flyers`).
##
## Flyer squadron over the baked menu background. `MindRender.menu_flyers_json`
## returns one flyer type's sprites in the bake's texture-pixel space (32 px per
## tile, `MenuRenderer.flyerRot = 45`); this node maps them through the same
## stretched/centred transform the `MenuBackground` `TextureRect` uses and draws
## each sprite rotated like Java's flyer.
##
## Deviation (J02 T4): Java composes the live `UnitType.draw` (region + weapons
## + engines); until J03 lands the unit body draw, the packed
## `unit-<type>-full` icon is the interim flyer sprite.

extends Control

## `1 - MenuRenderer.darkness`: Java darkens the whole frame (background *and*
## flyers) after `drawFlyers`, so the squadron carries the same modulate.
const DARKNESS := 0.7


func _ready() -> void:
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	modulate = Color(DARKNESS, DARKNESS, DARKNESS, 1.0)
	set_process(true)


func _process(_delta: float) -> void:
	if is_visible_in_tree():
		queue_redraw()


func _draw() -> void:
	var render := get_node_or_null("/root/Spine/MindRender")
	var assets := get_node_or_null("/root/MindAssets")
	if render == null or assets == null:
		return
	var payload: Variant = render.call("menu_flyers_json")
	if typeof(payload) != TYPE_STRING:
		return
	var parsed: Variant = JSON.parse_string(payload)
	if typeof(parsed) != TYPE_DICTIONARY:
		return
	var rows: Array = parsed.get("rows", [])
	if rows.is_empty():
		return
	# The bake is displayed by the parent `TextureRect` with
	# `STRETCH_KEEP_ASPECT_COVERED`: scale to cover this rect, centred.
	var texture_size := Vector2(
		float(parsed.get("width", 1)), float(parsed.get("height", 1))
	)
	var rect := size
	var draw_scale := maxf(rect.x / texture_size.x, rect.y / texture_size.y)
	var offset := (rect - texture_size * draw_scale) * 0.5
	# A unit sprite is authored facing up and Java draws it at `rotation - 90`
	# for an up-right `flyerRot`; in Godot's y-down space the clockwise-positive
	# rotation that faces the sprite up-right is `+flyerRot`.
	var rotation := deg_to_rad(float(parsed.get("rot", 45.0)))
	var scale_vector := Vector2(draw_scale, draw_scale)
	for row in rows:
		if typeof(row) != TYPE_ARRAY or row.size() < 3:
			continue
		var geometry: PackedInt32Array = assets.call("region_geometry", str(row[2]))
		if geometry.size() < 5:
			continue
		var page: Texture2D = assets.call("page_texture", geometry[4])
		if page == null:
			continue
		var center := offset + Vector2(float(row[0]), float(row[1])) * draw_scale
		draw_set_transform(center, rotation, scale_vector)
		draw_texture_rect_region(
			page,
			Rect2(
				-geometry[2] * 0.5, -geometry[3] * 0.5, geometry[2], geometry[3]
			),
			Rect2(geometry[0], geometry[1], geometry[2], geometry[3])
		)
	draw_set_transform(Vector2.ZERO, 0.0, Vector2.ONE)
