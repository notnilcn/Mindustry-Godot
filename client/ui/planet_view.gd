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
## `PlanetShader.lightDir` initial value (`Shaders.java:109`), used until
## `MindCampaign.planet_light` answers.
const DEFAULT_LIGHT := Vector3(0.57735, 0.57735, 0.57735)

@onready var space: TextureRect = $Space
@onready var sector_label: RichTextLabel = $SectorLabel
@onready var camera: Camera3D = $ViewportContainer/SubViewport/Camera
@onready var background: MeshInstance3D = $ViewportContainer/SubViewport/Background
@onready var body: MeshInstance3D = $ViewportContainer/SubViewport/Body
@onready var atmosphere: MeshInstance3D = $ViewportContainer/SubViewport/Atmosphere
## Whether the shown planet has an atmosphere billboard (`planet.hasAtmosphere`).
var _has_atmosphere := false
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
## `PlanetDialog.state.camPos`: the direction from the planet centre to the
## camera (planet space). Only direction matters — `_process` scales it by the
## camera distance (`PlanetRenderer.java:64-66`); drag rotates it.
var cam_pos := Vector3(0.0, CAM_LENGTH, 0.1)
## Lerped camera zoom (`PlanetDialog.state.zoom`); `zoom_target` is the value
## the wheel and the reveal set (`PlanetDialog.zoom`).
var zoom := 1.0
var zoom_target := 1.0
## `PlanetDialog.state.uiAlpha`: the sector overlays fade when the zoom passes
## 1.9 (`lerpDelta(uiAlpha, zoom < 1.9 ? 1 : 0, 0.1)`).
var ui_alpha := 1.0
## Values baked into the last marker rebuild (throttles the per-frame updates).
var _built_ui_alpha := -1.0
var _built_pulse := -1.0

var _tiles := PackedVector3Array()
var _corners := PackedVector3Array()
var _tile_corners: Array = []
var _grid_material: ShaderMaterial = null
var _body_material: ShaderMaterial = null
## `HexMesh.preRender` light in the planet mesh frame (`MindCampaign.planet_light`):
## `(solarSystem.position - position).rotate(Y, getRotation()).nor()` at the
## current `universe.secondsf()`, and `solarSystem.lightColor`.
var _light_dir := DEFAULT_LIGHT
var _ambient := Color(1.0, 1.0, 1.0)
var _sector_label_text := ""
## Preset whose icon is drawn inside the selected hex (`renderProjections`).
var _sector_icon_preset := ""
var _sector_icon: MeshInstance3D = null
## Per-sector status layer (`renderSectors` fills/rings + `renderProjections`
## icons), rebuilt from the `campaign_views` sector rows.
var _markers: Node3D = null
var _sectors: Array = []
## `PlanetRenderer.renderOrbit` loop meshes (one per system planet with a
## parent); the shared material's alpha tracks `params.uiAlpha`.
var _orbits: Node3D = null
var _orbit_material: StandardMaterial3D = null
## `Planet.cloudMesh` layers (`HexSkyMesh` shells) and their `speed`s.
var _clouds: Array = []
var _cloud_speeds := PackedFloat32Array()
## Arc `Time.globalTime` in ticks (advances while paused; `HexSkyMesh.relRot`).
var _global_time := 0.0

## Transparent draw order, mirroring `PlanetRenderer.render`/`renderTransparent`
## (`PlanetRenderer.java:105-127,147-168`): clouds (-7/-6) -> `renderSectors`
## (grid 4, rings/fills/hover 5, selection/outline 6) -> `drawAtmosphere` ->
## `renderOrbit` -> `renderProjections` (sector icons).
const ATMOSPHERE_PRIORITY := 7
const ORBIT_PRIORITY := 8
const ICON_PRIORITY := 9

## `Pal.gray` (`Pal.java:82`, `454545`) — the orbit line colour.
const ORBIT_GRAY := Color(0.271, 0.271, 0.271)

## `Team.sharded/crux/derelict.color`.
const SHARDED := Color(1.0, 0.827, 0.498)
const CRUX := Color(0.949, 0.333, 0.333)
const DERELICT := Color(0.302, 0.306, 0.345)
## `PlanetRenderer.shadowColor`.
const SHADOW := Color(0.0, 0.0, 0.0, 0.7)


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
	_sectors = []
	_tiles = data.get("tiles", PackedVector3Array())
	_corners = data.get("corners", PackedVector3Array())
	_tile_corners = data.get("tile_corners", [])
	_refresh_light()
	_build_body()
	_build_clouds()
	_build_grid()
	_build_atmosphere()
	_build_orbits()
	_clear_highlight()
	return true


## `PlanetDialog.renderSectors` + `renderProjections` for every sector but the
## selected one: selectable/unlocked cells get a status ring (based = sharded,
## based + enemy = sharded/crux, unlocked preset = derelict/white, locked preset
## = gray, enemy base = crux), the rest a 70% shadow fill; icons are the warning
## glyph for attacked sectors, the preset icon for unlocked base-less presets,
## a padlock for the next locked preset, else the sector's own icon.
func set_sectors(sectors: Array) -> void:
	_sectors = sectors
	_rebuild_markers()


func _rebuild_markers() -> void:
	if _markers == null:
		# code-instantiated: marker layer content is driven by the sector rows.
		_markers = Node3D.new()
		_markers.name = "SectorMarkers"
		$ViewportContainer/SubViewport.add_child(_markers)
	for child in _markers.get_children():
		child.queue_free()
	if _tile_corners.is_empty():
		return
	_built_ui_alpha = ui_alpha
	_built_pulse = _preset_pulse()
	var verts := PackedVector3Array()
	var colors := PackedColorArray()
	var arad := float(data.get("outline", 1.17)) + 0.001
	for sector_variant in _sectors:
		var sector: Dictionary = sector_variant
		var tile := view_tile(sector)
		if tile < 0 or tile >= _tile_corners.size() or tile == selected_tile:
			continue
		var color: Variant = _status_color(sector)
		if bool(sector.get("selectable", false)) or bool(sector.get("unlocked", false)):
			if color != null:
				var ring: Color = color
				ring = Color(ring.r * 0.8, ring.g * 0.8, ring.b * 0.8, ui_alpha)
				_append_ring(verts, colors, tile, arad, 0.026, ring)
		else:
			var shadow := Color(SHADOW.r, SHADOW.g, SHADOW.b, _gamma_shadow_alpha(SHADOW.a * ui_alpha))
			_append_fill(verts, colors, tile, arad, shadow)
		_add_icon(sector, tile)
	if verts.is_empty():
		return
	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = verts
	arrays[Mesh.ARRAY_COLOR] = colors
	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	var instance := MeshInstance3D.new()
	instance.mesh = mesh
	var material := _overlay_material()
	# Depth-tested so the globe disc hides the far hemisphere.
	material.no_depth_test = false
	material.render_priority = 5
	instance.material_override = material
	_markers.add_child(instance)


## Java alpha-blends the black `shadowColor` fill in its gamma-space
## framebuffer (`dst * (1 - a)`); Godot blends in linear, where the same alpha
## only darkens the encoded result by `(1 - a)^(1/2.2)` (a 70% shadow left the
## locked part of the globe at ~58% instead of 30%). Raise the alpha so the
## linear blend lands on Java's value: `1 - (1 - a)^2.2`.
func _gamma_shadow_alpha(alpha: float) -> float:
	return 1.0 - pow(1.0 - clampf(alpha, 0.0, 1.0), 2.2)


## `renderSectors` ring color (null = no ring).
func _status_color(sector: Dictionary) -> Variant:
	if bool(sector.get("has_base", false)):
		return SHARDED.lerp(CRUX, 0.5 if bool(sector.get("has_enemy_base", false)) else 0.0)
	if not str(sector.get("preset", "")).is_empty() and bool(sector.get("require_unlock", false)):
		# `derelict.lerp(white, absin(time, 10, 1))` (`PlanetDialog.java:463`).
		return DERELICT.lerp(Color.WHITE, _preset_pulse()) if bool(sector.get("preset_unlocked", false)) else MindStyles.ARC_GRAY
	if bool(sector.get("has_enemy_base", false)):
		return CRUX
	return null


func _tile_outer(tile: int, arad: float) -> PackedVector3Array:
	var out := PackedVector3Array()
	for id in _tile_corners[tile]:
		out.append(_corners[id] * arad)
	return out


func _append_fill(verts: PackedVector3Array, colors: PackedColorArray, tile: int, arad: float, color: Color) -> void:
	var outers := _tile_outer(tile, arad)
	var center := _tiles[tile] * arad
	for i in outers.size():
		verts.append(center)
		verts.append(outers[i])
		verts.append(outers[(i + 1) % outers.size()])
		for _k in 3:
			colors.append(color)


func _append_ring(verts: PackedVector3Array, colors: PackedColorArray, tile: int, arad: float, stroke: float, color: Color) -> void:
	var outers := _tile_outer(tile, arad)
	var center := _tiles[tile] * arad
	var inners := PackedVector3Array()
	for outer in outers:
		var offset: Vector3 = outer - center
		inners.append(center + offset.normalized() * (offset.length() - stroke))
	for i in outers.size():
		var j := (i + 1) % outers.size()
		verts.append(outers[i])
		verts.append(outers[j])
		verts.append(inners[j])
		verts.append(outers[i])
		verts.append(inners[j])
		verts.append(inners[i])
		for _k in 6:
			colors.append(color)


## `renderProjections` icon for one sector.
## Java has no facing cull here: each icon lies flat on the sector's plane
## (`planets.drawPlane` -> `Planet.setPlane`), so icons near the limb are seen
## edge-on and those behind the globe fail the depth test.
func _add_icon(sector: Dictionary, tile: int) -> void:
	var preset := str(sector.get("preset", ""))
	var has_base := bool(sector.get("has_base", false))
	var glyph := ""
	var region := ""
	var color := Color.WHITE
	if bool(sector.get("attacked", false)):
		glyph = "warn"
		color = SHARDED
	elif not has_base and not preset.is_empty() and bool(sector.get("require_unlock", false)) \
			and bool(sector.get("preset_unlocked", false)) and _opt(sector, "icon").is_empty() \
			and _opt(sector, "content_icon").is_empty():
		region = "sector-%s" % preset
	elif bool(sector.get("lock_icon", false)):
		glyph = "lock"
	else:
		# `Sector.icon()`: contentIcon, info.icon, else an unlocked preset's icon.
		var content_icon := _opt(sector, "content_icon")
		var icon := _opt(sector, "icon")
		if not content_icon.is_empty():
			region = content_icon
		elif not icon.is_empty():
			glyph = icon
		elif not preset.is_empty() and bool(sector.get("require_unlock", false)) and bool(sector.get("preset_unlocked", false)):
			region = "sector-%s" % preset
	var plane := _sector_plane(tile)
	if not region.is_empty():
		var assets := MindWidgets.assets()
		var texture: Texture2D = null
		if assets != null:
			texture = assets.call("find_region", region) as Texture2D
			if texture == null and not region.begins_with("sector-"):
				for prefix in ["item", "block", "unit", "liquid"]:
					texture = assets.call("find_region", "%s-%s-ui" % [prefix, region]) as Texture2D
					if texture != null:
						break
		if texture == null:
			return
		# code-instantiated: one sector-plane sprite per marked sector.
		var sprite := Sprite3D.new()
		sprite.texture = texture
		sprite.pixel_size = SECTOR_ICON_SIZE / maxf(float(texture.get_width()), 1.0)
		sprite.texture_filter = BaseMaterial3D.TEXTURE_FILTER_LINEAR
		sprite.render_priority = ICON_PRIORITY
		sprite.transform = plane
		sprite.modulate = Color(1.0, 1.0, 1.0, ui_alpha)
		_markers.add_child(sprite)
		return
	var text := MindIcons.glyph(glyph)
	if text.is_empty():
		return
	# code-instantiated: one icon-font glyph per marked sector.
	var label := Label3D.new()
	label.text = text
	var assets := MindWidgets.assets()
	if assets != null:
		var font: Variant = assets.call("icon_font")
		if font is Font:
			label.font = font
	label.font_size = 64
	label.outline_size = 0
	label.pixel_size = SECTOR_ICON_SIZE / 64.0
	label.modulate = Color(color.r, color.g, color.b, color.a * ui_alpha)
	label.render_priority = ICON_PRIORITY
	label.transform = plane
	_markers.add_child(label)


## `Planet.setPlane` (`Planet.java:700-712`) in the mesh frame the port renders
## in (Java's `rotate(Y, -getRotation())` and `+ position` cancel out): origin
## `tile.v * (outlineRad + 0.01) * radius`; up = `Vec3.Y` projected onto the
## sector plane (`Sector.plane` through corners 0/2/4, `Sector.java:271`) minus
## `tile.v * radius`; right = up turned by Arc's `rotate(tile.v, 90)` (a
## clockwise quarter turn seen from outside). The local +Z faces outward, so a
## Sprite3D/Label3D/QuadMesh drawn in its XY plane matches `PlaneBatch3D`.
func _sector_plane(tile: int) -> Transform3D:
	var radius := float(data.get("radius", 1.0))
	var v := _tiles[tile]
	var origin := v.normalized() * (float(data.get("outline", 1.17)) + 0.01 * radius)
	var up := Vector3.UP
	var ids: PackedInt32Array = _tile_corners[tile] if tile < _tile_corners.size() else PackedInt32Array()
	if ids.size() >= 5:
		var p1 := _corners[ids[0]]
		var p2 := _corners[ids[2]]
		var p3 := _corners[ids[4]]
		var normal := (p1 - p2).cross(p2 - p3).normalized()
		var top := v + Vector3.UP
		top -= normal * (normal.dot(top) - p1.dot(normal))
		up = top - v * radius
	if up.length_squared() < 1e-10:
		up = Vector3.UP
	up = up.normalized()
	# Arc `Vec3.rotate(axis, 90)` is Godot's `rotated(axis, -PI / 2)`.
	var right := up.rotated(v.normalized(), -PI * 0.5).normalized()
	return Transform3D(Basis(right, up, right.cross(up).normalized()), origin)


## Optional string field of a read-model row (`null` -> "").
func _opt(row: Dictionary, key: String) -> String:
	var value: Variant = row.get(key, null)
	return "" if value == null else str(value)


## Centers the camera on a tile and highlights its hex (`PlanetDialog.lookAt`).
## `u_mouse` stays at the origin (the planet is not hovered), so the sector grid
## fades out exactly like `Planet.renderSectors` with a ray that misses
## (`Shaders.planetGrid.mouse.lerp(Vec3.Zero)`). Selecting does NOT move the
## camera (`PlanetDialog.updateSelected`); the show/reveal paths call
## `look_at_tile`.
func focus_tile(tile: int) -> void:
	selected_tile = tile
	if not _sectors.is_empty():
		_rebuild_markers()
	if _grid_material != null:
		_grid_material.set_shader_parameter("u_mouse", Vector3.ZERO)
	set_hover(-1)
	_rebuild_highlight()


## `PlanetDialog.lookAt(sector)` (exact, alpha 1) / `lookAt(sector, alpha)`
## (slerp): turns `cam_pos` toward the sector's grid tile, keeping the length
## (`Planet.java:692-694`, `PlanetDialog.java:395-408`).
func look_at_tile(tile: int, alpha := 1.0) -> void:
	if tile < 0 or tile >= _tiles.size():
		return
	var target := _tiles[tile].normalized()
	cam_pos = target if alpha >= 1.0 else cam_pos.slerp(target, alpha)


## `PlanetDialog.java:114-138` (`dragged`): orbit the camera by a screen-space
## drag delta. `relative` is Godot's mouse-motion delta in the same y-down
## screen basis as Arc's `mx - lastX`/`my - lastY`; `cx / 9 * speed` around the
## camera up axis, `cy / 10` degrees of vertical travel clamped to 1..179.
func drag(relative: Vector2) -> void:
	var up_v := rad_to_deg(cam_pos.angle_to(Vector3.UP))
	var speed := 1.0 - absf(up_v - 90.0) / 90.0
	var cam_up := camera.basis.y.normalized()
	var cam_dir := -camera.basis.z
	cam_pos = cam_pos.rotated(cam_up, deg_to_rad(relative.x / 9.0 * speed))
	var amount := relative.y / 10.0
	amount = clampf(up_v + amount, 1.0, 179.0) - up_v
	var axis := cam_up.rotated(cam_dir, PI * 0.5)
	cam_pos = cam_pos.rotated(axis.normalized(), deg_to_rad(amount))


## `PlanetDialog.java:140-147` (`scrolled`): the wheel moves the target zoom
## within `planet.minZoom`/`planet.maxZoom`.
func scroll(amount_y: float) -> void:
	var min_zoom := float(data.get("min_zoom", 0.5))
	var max_zoom := float(data.get("max_zoom", 2.0))
	zoom_target = clampf(zoom_target + amount_y / 10.0, min_zoom, max_zoom)


## `PlanetRenderer.java:64-66`: the camera sits at
## `(radius + camRadius) * camLength + (zoom - 1) * (radius + camRadius) * 2`
## along `cam_pos`, looking at the planet centre; `state.zoom` lerps toward the
## target every frame (`PlanetDialog.java:1022`).
func _process(delta: float) -> void:
	if data.is_empty():
		return
	# Arc `Time.delta` is tick-scaled (1.0 at 60 fps), so Java's
	# `lerpDelta(state.zoom, zoom, 0.4f)` is a 0.4 per-frame factor; raise it to
	# `delta * 60` so the speed is framerate-independent.
	zoom = lerp(zoom, zoom_target, 1.0 - pow(1.0 - 0.4, delta * 60.0))
	var radius := float(data.get("radius", 1.0))
	var cam_radius := float(data.get("cam_radius", 0.0))
	var distance := (radius + cam_radius) * CAM_LENGTH + (zoom - 1.0) * (radius + cam_radius) * 2.0
	camera.position = cam_pos.normalized() * distance
	camera.look_at(Vector3.ZERO, Vector3.UP)
	# `HexMesh.preRender` runs every frame; the sun direction moves with
	# `universe.secondsf()` (orbit + spin), so re-read it.
	_refresh_light()
	_update_atmosphere_camera()
	# `PlanetDialog.draw`: `uiAlpha = lerpDelta(uiAlpha, zoom < 1.9 ? 1 : 0, 0.1)`
	# and `absin(Time.time, 10, 1)` are baked into the marker rebuilds (Java
	# redraws every frame; the port rebuilds when either moves).
	ui_alpha = lerp(ui_alpha, 1.0 if zoom < 1.9 else 0.0, 1.0 - pow(1.0 - 0.1, delta * 60.0))
	# `renderOrbit`'s `alpha = params.uiAlpha` (skipped at <= 0.02).
	if _orbit_material != null:
		_orbit_material.albedo_color = Color(ORBIT_GRAY.r, ORBIT_GRAY.g, ORBIT_GRAY.b, ui_alpha)
	if _orbits != null:
		_orbits.visible = ui_alpha > 0.02
	if absf(ui_alpha - _built_ui_alpha) > 0.02 or absf(_preset_pulse() - _built_pulse) > 0.04:
		_rebuild_markers()
	_global_time += delta * 60.0
	_update_clouds()


## `Mathf.absin(Time.time, 10f, 1f)` (`PlanetDialog.java:463`): a 10 s
## `|sin|` pulse for unlocked, still-locked presets.
func _preset_pulse() -> float:
	return absf(sin(Time.get_ticks_msec() / 1000.0 / 10.0 * TAU))


## Clears the selected sector highlight/icon without moving the camera
## (`PlanetDialog.viewPlanet` keeps the camera and unsets `selected`).
func clear_selection() -> void:
	selected_tile = -1
	_sector_icon_preset = ""
	_clear_highlight()
	_rebuild_sector_icon()
	if not _sectors.is_empty():
		_rebuild_markers()


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


## `HexMesh(planet, 6)`: the shaded hex mesh (`MeshBuilder.buildHex` with the
## generator colours) replaces the vendored-art billboard; the billboard stays
## as the fallback for planets without a hex generator (T13/PAR-0142).
func _build_body() -> void:
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	var mesh_data: Dictionary = {}
	if campaign != null and campaign.has_method("planet_mesh"):
		mesh_data = campaign.call("planet_mesh", planet)
	if mesh_data.is_empty():
		_build_body_billboard()
		return
	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = mesh_data.get("vertices", PackedVector3Array())
	arrays[Mesh.ARRAY_NORMAL] = mesh_data.get("normals", PackedVector3Array())
	arrays[Mesh.ARRAY_COLOR] = mesh_data.get("colors", PackedColorArray())
	arrays[Mesh.ARRAY_INDEX] = _godot_winding(mesh_data.get("indices", PackedInt32Array()))
	var flags := 0
	var emissive: PackedColorArray = mesh_data.get("emissive_colors", PackedColorArray())
	if not emissive.is_empty():
		# Godot wants `ARRAY_CUSTOM0` as a float/byte array, not a colour array.
		var floats := PackedFloat32Array()
		floats.resize(emissive.size() * 4)
		for index in emissive.size():
			var color := emissive[index]
			floats[index * 4] = color.r
			floats[index * 4 + 1] = color.g
			floats[index * 4 + 2] = color.b
			floats[index * 4 + 3] = color.a
		arrays[Mesh.ARRAY_CUSTOM0] = floats
		flags = (
			Mesh.ARRAY_FORMAT_CUSTOM0
			| (Mesh.ARRAY_CUSTOM_RGBA_FLOAT << Mesh.ARRAY_FORMAT_CUSTOM0_SHIFT)
		)
	var array_mesh := ArrayMesh.new()
	array_mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays, [], {}, flags)
	body.mesh = array_mesh
	var material := ShaderMaterial.new()
	material.shader = load("res://shaders/planet.gdshader")
	material.render_priority = -8
	material.set_shader_parameter("u_lightdir", _light_dir)
	material.set_shader_parameter("u_ambientColor", _vec3(_ambient))
	material.set_shader_parameter(
		"u_emissive", 1.0 if bool(mesh_data.get("emissive", false)) else 0.0
	)
	body.material_override = material
	_body_material = material
	_update_body_camera()
	body.visible = true


## `MeshBuilder.buildHex` emits OpenGL counter-clockwise front faces and
## `PlanetRenderer.render` culls `Gl.back` (`PlanetRenderer.java:53-54`). Godot's
## front faces are clockwise, so the same index order culled the near hemisphere
## and the globe showed the inside of its far side (wrong terrain, lit from
## behind). Swap the last two indices of every triangle.
func _godot_winding(indices: PackedInt32Array) -> PackedInt32Array:
	var out := indices.duplicate()
	for start in range(0, out.size() - 2, 3):
		out[start + 1] = indices[start + 2]
		out[start + 2] = indices[start + 1]
	return out


## `Planet.drawClouds` (`PlanetRenderer.renderTransparent`, `PlanetRenderer.java:149`):
## each `HexSkyMesh` layer of `cloudMesh` (`MindCampaign.planet_clouds`) after the
## body and before the atmosphere (`render_priority` -7, -6; see ATMOSPHERE_PRIORITY).
func _build_clouds() -> void:
	for node_variant in _clouds:
		(node_variant as Node).queue_free()
	_clouds = []
	_cloud_speeds = PackedFloat32Array()
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign == null or not campaign.has_method("planet_clouds"):
		return
	var layers: Array = campaign.call("planet_clouds", planet)
	var shader: Shader = load("res://shaders/clouds.gdshader")
	for layer_variant in layers:
		var layer: Dictionary = layer_variant
		var arrays := []
		arrays.resize(Mesh.ARRAY_MAX)
		arrays[Mesh.ARRAY_VERTEX] = layer.get("vertices", PackedVector3Array())
		arrays[Mesh.ARRAY_NORMAL] = layer.get("normals", PackedVector3Array())
		arrays[Mesh.ARRAY_COLOR] = layer.get("colors", PackedColorArray())
		arrays[Mesh.ARRAY_INDEX] = layer.get("indices", PackedInt32Array())
		var array_mesh := ArrayMesh.new()
		array_mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
		var material := ShaderMaterial.new()
		material.shader = shader
		material.render_priority = -7 + _clouds.size()
		# code-instantiated: one shell per data-driven `HexSkyMesh` layer
		# (`Planet.cloudMeshLoader`'s `MultiMesh`, Planets.java:49-52,127-130).
		var instance := MeshInstance3D.new()
		instance.name = "Clouds%d" % _clouds.size()
		instance.mesh = array_mesh
		instance.material_override = material
		body.get_parent().add_child(instance)
		_clouds.append(instance)
		_cloud_speeds.append(float(layer.get("speed", 0.0)))
	_update_clouds()


## `HexSkyMesh.render`/`preRender` (`HexSkyMesh.java:45-63`): hidden while
## `Mathf.zero(1 - uiAlpha, 0.01)`, `u_alpha = 1 - uiAlpha` on the viewed planet,
## spun by `relRot = Time.globalTime * speed / 40` degrees on top of the body
## frame, with the light direction rotated by the same angle.
func _update_clouds() -> void:
	var alpha := 1.0 - ui_alpha
	var hidden := absf(alpha) <= 0.01
	for index in _clouds.size():
		var node: MeshInstance3D = _clouds[index]
		node.visible = not hidden
		if hidden:
			continue
		var rel := deg_to_rad(_global_time * _cloud_speeds[index] / 40.0)
		node.rotation = Vector3(0.0, rel, 0.0)
		var material := node.material_override as ShaderMaterial
		material.set_shader_parameter("u_alpha", alpha)
		material.set_shader_parameter("u_lightdir", _light_dir.rotated(Vector3.UP, rel))
		material.set_shader_parameter("u_ambientColor", Vector3(_ambient.r, _ambient.g, _ambient.b))


## Vendored-art fallback (no hex generator data).
func _build_body_billboard() -> void:
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


## `PlanetRenderer.drawAtmosphere`: additive atmospheric scattering over
## `MeshBuilder.buildHex(Color.white, 2, 1.5f)` — a sphere of radius 1.5 that
## covers the `radius + atmosphereRad{In,Out}` shell (the shader reads the
## mesh's surface position as the ray target); light is `Planet.getLightNormal()`.
## Drawn in Java's `renderTransparent` order (`PlanetRenderer.java:147-168`):
## after the body, the clouds and `renderSectors` (grid, status rings, fills,
## hover, selection), so the additive veil tints the limb and the rings near it;
## before `renderOrbit` and `renderProjections` (icons), which stay untinted.
func _build_atmosphere() -> void:
	atmosphere.visible = false
	_has_atmosphere = false
	var atmos: Dictionary = data.get("atmosphere", {})
	if atmos.is_empty() or not bool(atmos.get("has", false)):
		return
	# code-instantiated: one atmosphere sphere per planet view. Deviation: a
	# smooth sphere instead of the 2-subdivision hex sphere (rotationally
	# symmetric, so the shader's per-fragment ray is the same surface ray).
	var sphere := SphereMesh.new()
	sphere.radius = 1.5
	sphere.height = 3.0
	sphere.radial_segments = 48
	sphere.rings = 24
	atmosphere.mesh = sphere
	var material := ShaderMaterial.new()
	material.shader = load("res://shaders/atmosphere.gdshader")
	material.render_priority = ATMOSPHERE_PRIORITY
	var radius := float(data.get("radius", 1.0))
	material.set_shader_parameter("u_innerRadius", radius + float(atmos.get("rad_in", 0.0)))
	material.set_shader_parameter("u_outerRadius", radius + float(atmos.get("rad_out", 0.3)))
	material.set_shader_parameter("u_color", _vec3(atmos.get("color", Color(0.3, 0.7, 1.0))))
	# `AtmosphereShader.apply` passes `planet.getLightNormal()` against a
	# world-space camera; the port's camera lives in the planet mesh frame, so
	# the same sun direction is used in that frame.
	material.set_shader_parameter("u_light", _light_dir)
	atmosphere.material_override = material
	_has_atmosphere = true
	atmosphere.visible = _atmosphere_setting()
	_update_atmosphere_camera()


## `PlanetRenderer.renderOrbit` (`PlanetRenderer.java:171-179`): a `lineLoop`
## of `(int)(radius * 10)` points around each parent position at `orbitRadius`,
## colour `Pal.gray` with `alpha = params.uiAlpha` (hidden below 0.02). The
## payload pre-computes each centre in the view frame (planet at the origin).
func _build_orbits() -> void:
	if _orbits == null:
		# code-instantiated: one loop mesh per solar-system planet with a
		# parent; the count is data-driven from the system layout.
		_orbits = Node3D.new()
		_orbits.name = "Orbits"
		$ViewportContainer/SubViewport.add_child(_orbits)
	for child in _orbits.get_children():
		_orbits.remove_child(child)
		child.queue_free()
	_orbit_material = StandardMaterial3D.new()
	_orbit_material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	_orbit_material.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	_orbit_material.albedo_color = Color(ORBIT_GRAY.r, ORBIT_GRAY.g, ORBIT_GRAY.b, ui_alpha)
	_orbit_material.cull_mode = BaseMaterial3D.CULL_DISABLED
	_orbit_material.render_priority = ORBIT_PRIORITY
	for row_variant in data.get("orbits", []):
		var row: Dictionary = row_variant
		var radius := float(row.get("radius", 0.0))
		var points := int(radius * 10.0)
		if points < 3:
			continue
		var center: Vector3 = row.get("center", Vector3.ZERO)
		# `Angles.circleVectors(points, radius, ...)` with a `Gl.lineLoop`
		# (the strip repeats the first point to close).
		var verts := PackedVector3Array()
		verts.resize(points + 1)
		for i in points + 1:
			var angle := TAU * float(i % points) / float(points)
			verts[i] = center + Vector3(cos(angle) * radius, 0.0, sin(angle) * radius)
		var arrays := []
		arrays.resize(Mesh.ARRAY_MAX)
		arrays[Mesh.ARRAY_VERTEX] = verts
		var mesh := ArrayMesh.new()
		mesh.add_surface_from_arrays(Mesh.PRIMITIVE_LINE_STRIP, arrays)
		var node := MeshInstance3D.new()
		node.mesh = mesh
		node.material_override = _orbit_material
		_orbits.add_child(node)
	_orbits.visible = ui_alpha > 0.02


## `PlanetRenderer.renderTransparent` (`PlanetRenderer.java:156`): the
## atmosphere draws only while `Core.settings.getBool("atmosphere")` (Graphics
## row, default true); read every frame like Java.
func _atmosphere_setting() -> bool:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("settings_get_bool"):
		return true
	return bool(ui.call("settings_get_bool", "atmosphere"))


## `AtmosphereShader.apply`: `u_campos` is the camera in planet space and
## `u_rcampos` the camera relative to the planet (the port keeps the planet at
## the viewport origin, so both are the camera position). The sphere is
## rotation-symmetric, so no billboard rotation is needed; the space backdrop
## still follows the camera.
func _update_atmosphere_camera() -> void:
	atmosphere.visible = _has_atmosphere and _atmosphere_setting()
	background.rotation = camera.rotation
	if atmosphere.material_override is ShaderMaterial:
		var material := atmosphere.material_override as ShaderMaterial
		material.set_shader_parameter("u_campos", camera.position)
		material.set_shader_parameter("u_rcampos", camera.position)
	_update_body_camera()


## `HexMesh.preRender` (`HexMesh.java:22-27`): the sun direction in the planet
## mesh frame and the sun's light colour, pushed to the body and atmosphere.
func _refresh_light() -> void:
	if planet.is_empty():
		return
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign == null or not campaign.has_method("planet_light"):
		return
	var light: Dictionary = campaign.call("planet_light", planet)
	if light.is_empty():
		return
	_light_dir = light.get("light_dir", DEFAULT_LIGHT)
	_ambient = light.get("ambient", Color(1.0, 1.0, 1.0))
	if _body_material != null:
		_body_material.set_shader_parameter("u_lightdir", _light_dir)
		_body_material.set_shader_parameter("u_ambientColor", _vec3(_ambient))
	if atmosphere.material_override is ShaderMaterial:
		(atmosphere.material_override as ShaderMaterial).set_shader_parameter("u_light", _light_dir)


## `Shaders.PlanetShader.apply`: `u_campos` is the camera and `u_camdir` its
## direction. The port keeps the camera in the planet mesh frame (Java's
## world camera rotated by `getRotation()`), matching `u_lightdir`.
func _update_body_camera() -> void:
	if _body_material == null:
		return
	_body_material.set_shader_parameter("u_campos", camera.global_position)
	_body_material.set_shader_parameter("u_camdir", -camera.global_basis.z)


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
		# code-instantiated: one data-driven sector-plane quad for the selected sector.
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
	# `renderProjections` `iw = 64f/4f` at the `PlaneBatch3D` scaling (1/150),
	# flat on the sector plane (`Planet.setPlane`) like every other icon.
	var quad := QuadMesh.new()
	quad.size = Vector2(SECTOR_ICON_SIZE, SECTOR_ICON_SIZE)
	_sector_icon.mesh = quad
	_sector_icon.transform = _sector_plane(selected_tile)
	var material := StandardMaterial3D.new()
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	material.albedo_texture = texture
	material.texture_filter = BaseMaterial3D.TEXTURE_FILTER_LINEAR
	material.cull_mode = BaseMaterial3D.CULL_DISABLED
	material.render_priority = ICON_PRIORITY
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


## A `Color` handed to a plain `vec3` uniform is converted sRGB -> linear by
## Godot, which darkened Java's raw `setUniformf(r, g, b)` values (the
## atmosphere lost most of its red/green and turned a dim blue). Pass the
## components verbatim.
func _vec3(color: Color) -> Vector3:
	return Vector3(color.r, color.g, color.b)


func _load_loose(key: String, cutout: bool) -> Texture2D:
	var assets := MindWidgets.assets()
	if assets == null:
		return null
	var method := "find_loose_cutout" if cutout else "find_loose"
	var texture: Variant = assets.call(method, key)
	return texture as Texture2D
