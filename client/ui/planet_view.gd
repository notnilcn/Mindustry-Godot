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
## Sector icon size inside the selected hex (`renderProjections` `iw = 64f/4f`,
## ~21 px on screen at the near-surface scale of ~198 px per world unit).
const SECTOR_ICON_SIZE := 0.106

## `PlanetRenderer.outlineColor`/`hoverColor` are Pal.accent (#ffd37f).
const ACCENT := Color(1.0, 0.827, 0.498)
## `Planet.getLightNormal()` = normalize(sun.position - planet.position). Orbit
## positions are not simulated in the port, so the direction is fitted to the
## reference frame's halo (bright on the right/upper-right limb).
const LIGHT_NORMAL := Vector3(0.9036, 0.1988, 0.3795)

@onready var space: TextureRect = $Space
@onready var sector_label: RichTextLabel = $SectorLabel
@onready var camera: Camera3D = $ViewportContainer/SubViewport/Camera
@onready var background: MeshInstance3D = $ViewportContainer/SubViewport/Background
@onready var body: MeshInstance3D = $ViewportContainer/SubViewport/Body
@onready var atmosphere: MeshInstance3D = $ViewportContainer/SubViewport/Atmosphere
@onready var grid: MeshInstance3D = $ViewportContainer/SubViewport/Grid
@onready var selection: MeshInstance3D = $ViewportContainer/SubViewport/Selection
@onready var hover_fill: MeshInstance3D = $ViewportContainer/SubViewport/Hover
@onready var outline: MeshInstance3D = $ViewportContainer/SubViewport/Outline

## `planet_view` payload (grid topology, preset remap, render params).
var data: Dictionary = {}
## Active planet content name.
var planet := ""
## Selected grid tile index (-1 = none).
var selected_tile := -1
## Hovered grid tile index (-1 = none); drives the grid `u_mouse` and the label
## (`PlanetDialog.hovered`).
var hover_tile := -1

var _tiles := PackedVector3Array()
var _corners := PackedVector3Array()
var _tile_corners: Array = []
var _grid_material: ShaderMaterial = null
var _sector_label_text := ""
## Preset whose icon is drawn inside the selected hex (`renderProjections`).
var _sector_icon_preset := ""
var _sector_icon: MeshInstance3D = null


func _ready() -> void:
	visible = false
	var space_texture := _load_loose("space", false)
	space.texture = space_texture
	_build_background(space_texture)


## The starfield as a camera-facing 3D quad inside the viewport so the additive
## atmosphere glow composites over it (`PlanetRenderer`'s skybox slot); the UV
## crop mirrors the 2D `$Space` `STRETCH_KEEP_ASPECT_COVERED` framing.
func _build_background(texture: Texture2D) -> void:
	background.visible = false
	if texture == null:
		return
	# code-instantiated: one backdrop billboard for the whole planet layer.
	var quad := QuadMesh.new()
	var half_fov := deg_to_rad(camera.fov * 0.5)
	var height := 2.0 * CAM_LENGTH * tan(half_fov)
	var width := height * 1152.0 / 648.0
	quad.size = Vector2(width, height)
	background.mesh = quad
	var size := texture.get_size()
	var cover := maxf(width / size.x, height / size.y)
	var material := StandardMaterial3D.new()
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.albedo_texture = texture
	material.texture_filter = BaseMaterial3D.TEXTURE_FILTER_LINEAR
	material.cull_mode = BaseMaterial3D.CULL_DISABLED
	material.depth_draw_mode = BaseMaterial3D.DEPTH_DRAW_DISABLED
	material.render_priority = -10
	var uv_scale := Vector2(width / cover / size.x, height / cover / size.y)
	material.uv1_scale = Vector3(uv_scale.x, uv_scale.y, 1.0)
	material.uv1_offset = Vector3((1.0 - uv_scale.x) * 0.5, (1.0 - uv_scale.y) * 0.5, 0.0)
	background.material_override = material
	background.visible = true


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
	_build_atmosphere()
	_clear_highlight()
	return true


## Centers the camera on a tile and highlights its hex (`PlanetDialog.lookAt`).
## `u_mouse` stays at the origin (the planet is not hovered), so the sector grid
## fades out exactly like `Planet.renderSectors` with a ray that misses
## (`Shaders.planetGrid.mouse.lerp(Vec3.Zero)`).
func focus_tile(tile: int) -> void:
	selected_tile = tile
	var direction := Vector3.FORWARD
	if tile >= 0 and tile < _tiles.size():
		direction = _tiles[tile].normalized()
	var radius := float(data.get("radius", 1.0))
	var cam_radius := float(data.get("cam_radius", 0.0))
	camera.position = direction * ((radius + cam_radius) * CAM_LENGTH)
	camera.look_at(Vector3.ZERO, Vector3.UP)
	_update_atmosphere_camera()
	if _grid_material != null:
		_grid_material.set_shader_parameter("u_mouse", Vector3.ZERO)
	set_hover(-1)
	_rebuild_highlight()


## Sets the hovered tile (`PlanetDialog.hovered`): the grid fades in around it,
## the hex takes the accent hover fill and the dialog draws the name label.
func set_hover(tile: int) -> void:
	hover_tile = tile
	if _grid_material != null:
		var mouse := Vector3.ZERO
		if tile >= 0 and tile < _tiles.size():
			mouse = _tiles[tile].normalized() * float(data.get("outline", 1.17))
		_grid_material.set_shader_parameter("u_mouse", mouse)
	_rebuild_hover_fill()
	_update_label()


## `PlanetDialog.draw`: `planets.fill(hovered, hoverColor.write().a(0.5f), -0.003f)`
## paints the hovered hex with the accent at 50% just under the shell.
func _rebuild_hover_fill() -> void:
	hover_fill.mesh = null
	if hover_tile < 0 or hover_tile >= _tile_corners.size():
		return
	var ids: PackedInt32Array = _tile_corners[hover_tile]
	var arad := float(data.get("outline", 1.17)) - 0.003
	var center := _tiles[hover_tile] * arad if hover_tile < _tiles.size() else Vector3.ZERO
	var corners: Array = []
	for id in ids:
		corners.append(_corners[id] * arad)
	var fan := PackedVector3Array()
	var colors := PackedColorArray()
	var accent_fill := Color(ACCENT.r, ACCENT.g, ACCENT.b, 0.5)
	for i in corners.size():
		fan.append(center)
		fan.append(corners[i])
		fan.append(corners[(i + 1) % corners.size()])
		for _j in 3:
			colors.append(accent_fill)
	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = fan
	arrays[Mesh.ARRAY_COLOR] = colors
	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	hover_fill.mesh = mesh
	var material := _overlay_material()
	material.render_priority = 5
	hover_fill.material_override = material


## Sets the preset whose icon is drawn at the selected sector
## (`PlanetDialog.renderProjections`: `sec.preset.uiIcon`); empty clears it.
func set_sector_icon(preset: String) -> void:
	_sector_icon_preset = preset
	_rebuild_sector_icon()


## The in-world hover label (`PlanetDialog.hoverLabel`): `[ name ]` under the
## hovered sector only.
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
	# `PlanetDialog.draw`: the label exists only for `hovered != null`; the
	# selected sector alone draws no name.
	var point: Variant = null
	if hover_tile >= 0 and not _sector_label_text.is_empty():
		point = tile_to_screen(hover_tile)
	if point == null:
		sector_label.visible = false
		return
	# `PlanetDialog.draw`: `[accent][lb] [white]<name>[accent] ]` — gold
	# brackets with the name in white (`Styles.outlineLabel` carries the black
	# outline via the scene's theme overrides).
	sector_label.text = "[color=#ffd37f][lb] [color=white]%s[color=#ffd37f] ]" % _sector_label_text
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
	# `Planet.gridMeshLoader` builds the grid with `outlineColor` (Pal.accent).
	colors.fill(ACCENT)
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


## `PlanetRenderer.drawAtmosphere`: additive atmospheric scattering over a
## billboard (`MeshBuilder.buildHex(Color.white, 2, 1.5f)`); radii are
## `radius + atmosphereRad{In,Out}`, light is `Planet.getLightNormal()`.
func _build_atmosphere() -> void:
	atmosphere.visible = false
	var atmos: Dictionary = data.get("atmosphere", {})
	if atmos.is_empty() or not bool(atmos.get("has", false)):
		return
	# code-instantiated: one atmosphere billboard per planet view.
	var quad := QuadMesh.new()
	quad.size = Vector2(3.0, 3.0)
	atmosphere.mesh = quad
	var material := ShaderMaterial.new()
	material.shader = load("res://shaders/atmosphere.gdshader")
	material.render_priority = -9
	var radius := float(data.get("radius", 1.0))
	material.set_shader_parameter("u_innerRadius", radius + float(atmos.get("rad_in", 0.0)))
	material.set_shader_parameter("u_outerRadius", radius + float(atmos.get("rad_out", 0.3)))
	material.set_shader_parameter("u_color", atmos.get("color", Color(0.3, 0.7, 1.0)))
	material.set_shader_parameter("u_light", LIGHT_NORMAL)
	atmosphere.material_override = material
	atmosphere.visible = true
	_update_atmosphere_camera()


## `AtmosphereShader.apply`: `u_campos` is the camera in planet space and
## `u_rcampos` the camera relative to the planet (the port keeps the planet at
## the viewport origin, so both are the camera position). The mesh faces the
## camera like `Planet.getTransform` does in Java; the space backdrop follows.
func _update_atmosphere_camera() -> void:
	atmosphere.rotation = camera.rotation
	background.rotation = camera.rotation
	if atmosphere.material_override is ShaderMaterial:
		var material := atmosphere.material_override as ShaderMaterial
		material.set_shader_parameter("u_campos", camera.position)
		material.set_shader_parameter("u_rcampos", camera.position)


func _rebuild_highlight() -> void:
	_clear_highlight()
	if selected_tile < 0 or selected_tile >= _tile_corners.size():
		return
	var ids: PackedInt32Array = _tile_corners[selected_tile]
	# `PlanetRenderer.drawSelection(selected, uiAlpha)` draws a stroke band
	# `(outlineRad + 0.001) * radius` wide `0.04` toward the tile center, in
	# Pal.accent; `drawBorders` re-draws the hex outline in accent at 30%.
	var arad := float(data.get("outline", 1.17)) + 0.001
	var center := _tiles[selected_tile] * arad if selected_tile < _tiles.size() else Vector3.ZERO
	var stroke := 0.04
	var outers := PackedVector3Array()
	var inners := PackedVector3Array()
	for id in ids:
		var outer: Vector3 = _corners[id] * arad
		outers.append(outer)
		var offset := outer - center
		inners.append(center + offset.normalized() * (offset.length() - stroke))

	# Band ring (`Planet.drawSelection`: quad(outer_i, outer_i+1, inner_i+1, inner_i)).
	var band := PackedVector3Array()
	var band_colors := PackedColorArray()
	for i in outers.size():
		var j := (i + 1) % outers.size()
		band.append(outers[i])
		band.append(outers[j])
		band.append(inners[j])
		band.append(outers[i])
		band.append(inners[j])
		band.append(inners[i])
		for _i in 6:
			band_colors.append(ACCENT)
	var band_arrays := []
	band_arrays.resize(Mesh.ARRAY_MAX)
	band_arrays[Mesh.ARRAY_VERTEX] = band
	band_arrays[Mesh.ARRAY_COLOR] = band_colors
	var band_mesh := ArrayMesh.new()
	band_mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, band_arrays)
	selection.mesh = band_mesh
	selection.material_override = _overlay_material()
	_rebuild_sector_icon()

	# Outline loop (`drawBorders`, `borderColor` = accent at 30%).
	var loop := PackedVector3Array()
	var loop_colors := PackedColorArray()
	var edge := Color(ACCENT.r, ACCENT.g, ACCENT.b, 0.3)
	for index in outers.size():
		loop.append(outers[index])
		loop.append(outers[(index + 1) % outers.size()])
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
	if _sector_icon != null:
		_sector_icon.mesh = null


## The selected sector's icon (`PlanetDialog.renderProjections`: every visible
## sector draws its preset icon; the port draws the selected one).
func _rebuild_sector_icon() -> void:
	if _sector_icon == null:
		# code-instantiated: one data-driven billboard for the selected sector.
		_sector_icon = MeshInstance3D.new()
		_sector_icon.name = "SectorIcon"
		_sector_icon.mesh = null
		$ViewportContainer/SubViewport.add_child(_sector_icon)
	var texture: Texture2D = null
	if not _sector_icon_preset.is_empty():
		var assets := MindWidgets.assets()
		if assets != null:
			texture = assets.call("find_region", "sector-%s" % _sector_icon_preset) as Texture2D
	if texture == null or selected_tile < 0 or selected_tile >= _tiles.size():
		_sector_icon.mesh = null
		return
	# `renderProjections` `iw = 64f/4f` at the sector plane scale (~0.026 world
	# units on screen); billboard quad slightly outside the outline shell.
	var quad := QuadMesh.new()
	quad.size = Vector2(SECTOR_ICON_SIZE, SECTOR_ICON_SIZE)
	_sector_icon.mesh = quad
	var arad := float(data.get("outline", 1.17)) + 0.004
	_sector_icon.position = _tiles[selected_tile].normalized() * arad
	var material := StandardMaterial3D.new()
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	material.albedo_texture = texture
	material.texture_filter = BaseMaterial3D.TEXTURE_FILTER_LINEAR
	material.billboard_mode = BaseMaterial3D.BILLBOARD_ENABLED
	material.no_depth_test = true
	material.render_priority = 7
	_sector_icon.material_override = material


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
