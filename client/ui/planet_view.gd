## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/PlanetDialog.java +
## core/src/mindustry/graphics/g3d/PlanetRenderer.java (plan 16 §3.10).
##
## The campaign sector-select globe: a `SubViewport` 3D scene with the vendored
## planet art as the surface, the `PlanetGrid` line mesh (`build_planet_grid`)
## drawn with the ported `planetgrid` shader, and the selected sector highlighted
## from the grid corner geometry. All data comes from `MindCampaign.planet_view`,
## which exposes the Godot-free `mind_core::render::g3d` topology.

class_name PlanetView
extends Control

## Emitted with the `PlanetGrid` tile index under a click (-1 = miss).
signal sector_picked(tile: int)

## `PlanetRenderer.camLength`; camera distance is `(radius + camRadius) * this`.
const CAM_LENGTH := 4.0
## Vendored planet-art quad size; the sprite's bright disc is ~0.37 of its width.
const BODY_SIZE := 2.7

@onready var space: TextureRect = $Space
@onready var sector_label: Label = $SectorLabel
@onready var camera: Camera3D = $ViewportContainer/SubViewport/Camera
@onready var body: MeshInstance3D = $ViewportContainer/SubViewport/Body
@onready var grid: MeshInstance3D = $ViewportContainer/SubViewport/Grid
@onready var selection: MeshInstance3D = $ViewportContainer/SubViewport/Selection
@onready var outline: MeshInstance3D = $ViewportContainer/SubViewport/Outline

## `planet_view` payload (grid topology, preset remap, render params).
var data: Dictionary = {}
## Active planet content name.
var planet := ""
## Selected grid tile index (-1 = none).
var selected_tile := -1

var _tiles := PackedVector3Array()
var _corners := PackedVector3Array()
var _tile_corners: Array = []
var _grid_material: ShaderMaterial = null
var _sector_label_text := ""


func _ready() -> void:
	visible = false
	space.texture = _load_loose("space", false)


## Shows/hides the whole globe layer (the dialog owns the mode).
func set_active(active: bool) -> void:
	visible = active


## Loads the planet's grid + params and builds the body/grid meshes.
func set_planet(name: String) -> bool:
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign == null or not campaign.has_method("planet_view"):
		return false
	data = campaign.call("planet_view", name)
	if data.is_empty():
		return false
	planet = name
	_tiles = data.get("tiles", PackedVector3Array())
	_corners = data.get("corners", PackedVector3Array())
	_tile_corners = data.get("tile_corners", [])
	_build_body()
	_build_grid()
	_clear_highlight()
	return true


## Centers the camera on a tile and highlights its hex (`PlanetDialog.lookAt`).
func focus_tile(tile: int) -> void:
	selected_tile = tile
	var direction := Vector3.FORWARD
	if tile >= 0 and tile < _tiles.size():
		direction = _tiles[tile].normalized()
	var radius := float(data.get("radius", 1.0))
	var cam_radius := float(data.get("cam_radius", 0.0))
	camera.position = direction * ((radius + cam_radius) * CAM_LENGTH)
	camera.look_at(Vector3.ZERO, Vector3.UP)
	if _grid_material != null:
		# `Planet.renderSectors` lerps `u_mouse` toward the hovered sector; the
		# selected sector stands in for the hover so the grid is visible.
		_grid_material.set_shader_parameter("u_mouse", direction * float(data.get("outline", 1.17)))
	_rebuild_highlight()
	_update_label()


## The in-world hover label (`PlanetDialog.draw` `[ Ground Zero ]`).
func set_sector_label(text_value: String) -> void:
	_sector_label_text = text_value
	_update_label()


## Nearest `PlanetGrid` tile under a viewport point, or -1 (`Planet.getSector`).
func pick(viewport_position: Vector2) -> int:
	if _tiles.is_empty():
		return -1
	var radius := float(data.get("outline", 1.17))
	var origin := camera.project_ray_origin(viewport_position)
	var direction := camera.project_ray_normal(viewport_position)
	var b := origin.dot(direction)
	var c := origin.length_squared() - radius * radius
	var discriminant := b * b - c
	if discriminant < 0.0:
		return -1
	var hit := (origin + direction * (-b - sqrt(discriminant))).normalized()
	var best := -1
	var best_distance := 2.0
	for index in _tiles.size():
		var distance := hit.distance_squared_to(_tiles[index])
		if distance < best_distance:
			best_distance = distance
			best = index
	return best


## Tile position on the render shell (world units).
func tile_position(tile: int) -> Vector3:
	if tile < 0 or tile >= _tiles.size():
		return Vector3.ZERO
	return _tiles[tile].normalized() * float(data.get("outline", 1.17))


## Projects a tile to viewport coordinates (`Planet.project`), or null offscreen.
func tile_to_screen(tile: int) -> Variant:
	if tile < 0 or tile >= _tiles.size():
		return null
	var point := camera.unproject_position(tile_position(tile))
	var size := camera.get_viewport().get_visible_rect().size
	if point.x < 0.0 or point.y < 0.0 or point.x > size.x or point.y > size.y:
		return null
	return point


func _update_label() -> void:
	if sector_label == null:
		return
	sector_label.text = "" if _sector_label_text.is_empty() else "[ %s ]" % _sector_label_text
	var point = tile_to_screen(selected_tile) if not _sector_label_text.is_empty() else null
	if point == null:
		sector_label.visible = false
		return
	sector_label.visible = true
	sector_label.reset_size()
	sector_label.position = (point as Vector2) - sector_label.size * 0.5


## Remapped view tile for a `campaign_views` sector row (`Planet.getData().presets`).
func view_tile(sector: Dictionary) -> int:
	var presets: Dictionary = data.get("presets", {})
	var preset := str(sector.get("preset", ""))
	if not preset.is_empty() and presets.has(preset):
		return int(presets[preset])
	return int(sector.get("id", -1))


func _build_body() -> void:
	var texture := _load_loose("planets/%s" % planet, true)
	# code-instantiated: the globe quad is a data-driven billboard whose scale
	# depends on the planet radius; no static scene owns per-planet meshes.
	var quad := QuadMesh.new()
	quad.size = Vector2(BODY_SIZE, BODY_SIZE)
	body.mesh = quad
	body.visible = texture != null
	if texture == null:
		return
	var material := StandardMaterial3D.new()
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	material.albedo_texture = texture
	material.texture_filter = BaseMaterial3D.TEXTURE_FILTER_LINEAR
	material.billboard_mode = BaseMaterial3D.BILLBOARD_ENABLED
	# The opaque background is cut out by luminance alpha, so only the disc
	# writes depth and occludes the far half of the grid.
	material.depth_draw_mode = BaseMaterial3D.DEPTH_DRAW_ALWAYS
	material.render_priority = -8
	body.material_override = material


func _build_grid() -> void:
	var lines: PackedVector3Array = data.get("lines", PackedVector3Array())
	if lines.is_empty():
		return
	var colors := PackedColorArray()
	colors.resize(lines.size())
	colors.fill(Color(1.0, 1.0, 1.0, 0.9))
	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = lines
	arrays[Mesh.ARRAY_COLOR] = colors
	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_LINES, arrays)
	_grid_material = ShaderMaterial.new()
	_grid_material.shader = load("res://shaders/planetgrid.gdshader")
	_grid_material.render_priority = 4
	grid.mesh = mesh
	grid.material_override = _grid_material


func _rebuild_highlight() -> void:
	_clear_highlight()
	if selected_tile < 0 or selected_tile >= _tile_corners.size():
		return
	var ids: PackedInt32Array = _tile_corners[selected_tile]
	var scale := float(data.get("outline", 1.17)) + 0.004
	var positions := PackedVector3Array()
	for id in ids:
		positions.append(_corners[id] * scale)

	# Filled hex (`drawSelection`/`fill`).
	var fill := PackedVector3Array()
	var fill_colors := PackedColorArray()
	var accent := Color(1.0, 0.72, 0.32, 0.8)
	for index in range(1, positions.size() - 1):
		fill.append(positions[0])
		fill.append(positions[index])
		fill.append(positions[index + 1])
		for _i in 3:
			fill_colors.append(accent)
	var fill_arrays := []
	fill_arrays.resize(Mesh.ARRAY_MAX)
	fill_arrays[Mesh.ARRAY_VERTEX] = fill
	fill_arrays[Mesh.ARRAY_COLOR] = fill_colors
	var fill_mesh := ArrayMesh.new()
	fill_mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, fill_arrays)
	selection.mesh = fill_mesh
	selection.material_override = _overlay_material()

	# Outline loop (`drawBorders`).
	var loop := PackedVector3Array()
	var loop_colors := PackedColorArray()
	var edge := Color(1.0, 0.92, 0.75, 1.0)
	for index in positions.size():
		loop.append(positions[index])
		loop.append(positions[(index + 1) % positions.size()])
		for _i in 2:
			loop_colors.append(edge)
	var loop_arrays := []
	loop_arrays.resize(Mesh.ARRAY_MAX)
	loop_arrays[Mesh.ARRAY_VERTEX] = loop
	loop_arrays[Mesh.ARRAY_COLOR] = loop_colors
	var loop_mesh := ArrayMesh.new()
	loop_mesh.add_surface_from_arrays(Mesh.PRIMITIVE_LINES, loop_arrays)
	outline.mesh = loop_mesh
	outline.material_override = _overlay_material()


func _clear_highlight() -> void:
	selection.mesh = null
	outline.mesh = null


func _overlay_material() -> StandardMaterial3D:
	var material := StandardMaterial3D.new()
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.vertex_color_use_as_albedo = true
	material.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	material.cull_mode = BaseMaterial3D.CULL_DISABLED
	material.no_depth_test = true
	material.render_priority = 6
	return material


func _load_loose(key: String, cutout: bool) -> Texture2D:
	var assets := MindWidgets.assets()
	if assets == null:
		return null
	var method := "find_loose_cutout" if cutout else "find_loose"
	var texture: Variant = assets.call(method, key)
	return texture as Texture2D
