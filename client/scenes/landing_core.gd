## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/world/blocks/storage/CoreBlock.java (drawLanding
## 480-538, drawLandingThrusters 540-563, drawLaunch 427-470, drawThrusters
## 566-573, touchdown 340-355/412/650).
##
## World-space landing core sprite at the `Layer::Space` band: while
## `MindCamera2D.landing_draw_json()` reports an active landing, draws the
## streak rays, the dropping core (scaled/rotated per the Java formulas) with
## its `circle-shadow` trail, the four team-coloured + white flame circles and
## the two-pass thruster plates. After touchdown the plates retract over 90
## ticks while the real core building shows through, and the landing fires
## `Fx.launch` plus `Effect.shake(5, 5)`. The white screen fade starts when the
## landing begins and the sector announce fires when it ends. View-only: reads
## Rust state, never mutates it.
##
## Note: Arc is y-up; the world camera flips the canvas y axis (zoom `(z, -z)`),
## so world coordinates and counter-clockwise angles match Java numerically and
## `_draw_region` mirrors each texture back upright (Arc `Draw.rect`).

extends Node2D

## Region pixels -> world pixels (block regions are 4x supersampled).
const CORE_REGION_SUPERSAMPLE := 0.25
## `Pal.lightTrail` (`drawf.rs::LIGHT_TRAIL`).
const LIGHT_TRAIL := Color("ffe2a9")
## `thrusterSizes` frame duration for the fade-out (`CoreBlock.java:44`).
const FADE_TICKS := 35.0 / 60.0
## `CoreBlock.thrusterSizes` (`CoreBlock.java:44`).
const THRUSTER_SIZES := [
	0.0, 0.0, 0.0, 0.0, 0.3, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0
]
## `CoreBlock.thrusterLength` default (`14f / 4f`, `CoreBlock.java:48`).
const THRUSTER_LENGTH := 14.0 / 4.0
## Per-core `thrusterLength` overrides (`Blocks.java:3166-3232`).
const THRUSTER_LENGTHS := {
	"core-foundation": 34.0 / 4.0,
	"core-nucleus": 40.0 / 4.0,
	"core-bastion": 34.0 / 4.0,
	"core-citadel": 40.0 / 4.0,
	"core-acropolis": 48.0 / 4.0,
}
## `Geometry.d4` (`Arc`): right, up, left, down in the y-up world (the camera
## flips the canvas y axis, so world coordinates match Java numerically).
const D4 := [Vector2(1, 0), Vector2(0, 1), Vector2(-1, 0), Vector2(0, -1)]

var _active := false
var _announced := false
var _block := ""
var _position := Vector2.ZERO
var _fin := 1.0
## `renderer.getDisplayScale()` during the landing (`landing_draw_json`).
var _scale := 1.0
## Player team name (`<block>-team-<team>` region).
var _team := ""
## `CoreBuild.thrusterTime`: 1 at touchdown, `-delta / 90` per tick.
var _thruster_time := 0.0
## Player team colour for the landing flames (`team.color`, refreshed when a
## landing starts; `Team.sharded.color` = `Pal.accent`).
var _team_color := Color("ffd37f")


func _ready() -> void:
	set_process(true)


func _process(delta: float) -> void:
	var camera := get_node_or_null("/root/Spine/World/Camera2D")
	var active := false
	if camera != null and camera.has_method("landing_draw_json"):
		var parsed: Variant = JSON.parse_string(str(camera.call("landing_draw_json")))
		if parsed is Dictionary:
			active = bool(parsed.get("active", false))
			_block = str(parsed.get("block", ""))
			_position = Vector2(float(parsed.get("x", 0.0)), float(parsed.get("y", 0.0)))
			_fin = float(parsed.get("fin", 1.0))
			_scale = float(parsed.get("scale", 1.0))
			_team = str(parsed.get("team", ""))
	if active and not _active:
		_start_landing()
		_refresh_team_color()
	elif not active and _active:
		_finish_landing()
	_active = active
	if _active:
		queue_redraw()
	elif _thruster_time > 0.0:
		# `CoreBuild.updateTile` (`CoreBlock.java:650`): `-= delta / 90`.
		_thruster_time = maxf(0.0, _thruster_time - delta * 60.0 / 90.0)
		queue_redraw()


## Player-team colour for the flame circles (`team.color`, Java).
func _refresh_team_color() -> void:
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("player_team_color"):
		return
	var packed := int(host.call("player_team_color"))
	_team_color = Color8(
		(packed >> 24) & 0xff, (packed >> 16) & 0xff, (packed >> 8) & 0xff, packed & 0xff
	)


## `CoreBlock.beginLaunch(false)`: a white full-screen cover fading out.
func _start_landing() -> void:
	_announced = false
	var fade := get_node_or_null("/root/Spine/Ui/UiRoot/OverlayLayer/fade_in")
	if fade != null and fade.has_method("start_white"):
		fade.call("start_white", FADE_TICKS)


## `Time.run(launchDuration())`: announce the sector (`[accent]<name>`) and its
## stored resources (`[lightgray]Resources:[white]` + item emoji).
func _finish_landing() -> void:
	if _announced:
		return
	_announced = true
	# Touchdown (`CoreBlock.java:412,650` + `Effect.shake(5, 5)`).
	_thruster_time = 1.0
	var fx := get_node_or_null("/root/Spine/MindFx")
	if fx != null and fx.has_method("spawn_effect"):
		fx.call("spawn_effect", "launch", _position.x, _position.y, 0)
	var shake_camera := get_node_or_null("/root/Spine/World/Camera2D")
	if shake_camera != null and shake_camera.has_method("apply_shake_event"):
		shake_camera.call("apply_shake_event", 5, 5)
	queue_redraw()
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	var ui := get_node_or_null("/root/MindUi")
	if campaign == null or ui == null or not campaign.has_method("get_sector_state"):
		return
	var state: Dictionary = {}
	var parsed: Variant = JSON.parse_string(str(campaign.call("get_sector_state")))
	if parsed is Dictionary:
		state = parsed
	# `state.isCampaign() && Vars.showSectorLandInfo && preset.showSectorLandInfo`
	# (`CoreBlock.java:414`), resolved in mind-core.
	if not bool(state.get("showLandInfo", false)):
		return
	var preset := str(state.get("preset", ""))
	var sector_name := str(state.get("sector", ""))
	if not preset.is_empty():
		var key := "sector.%s.name" % preset
		var localized := _bundle(key)
		sector_name = localized if not localized.is_empty() and localized != key else preset
	var text := "[accent]%s" % sector_name
	var resources := _sector_resources(int(state.get("sector", -1)))
	if not resources.is_empty():
		var parts := PackedStringArray()
		for item in resources:
			parts.append(":%s:" % str(item))
		text += "\n[lightgray]%s[white] %s" % [_bundle("sectors.resources"), " ".join(parts)]
	# The toast label translates raw markup (`MindRichLabel`; H03 T17).
	ui.call("announce", text, 5.0)


## `SectorInfo.resources` item names from the live campaign sector row.
func _sector_resources(id: int) -> Array:
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign == null or not campaign.has_method("campaign_views_json"):
		return []
	var parsed: Variant = JSON.parse_string(str(campaign.call("campaign_views_json", "")))
	if not (parsed is Dictionary):
		return []
	for sector_variant in (parsed as Dictionary).get("sectors", []):
		var sector: Dictionary = sector_variant
		if int(sector.get("id", -2)) == id:
			var stats: Dictionary = sector.get("stats", {})
			return stats.get("resources", [])
	return []


func _bundle(key: String) -> String:
	var assets := get_node_or_null("/root/MindAssets")
	if assets == null:
		return ""
	return str(assets.call("bundle_get", key))


## `Mathf.sample(values, t)`: linear interpolation over `t * (len - 1)`, `t`
## clamped to `0..1` (`Arc Mathf`; `CoreBlock.java:492`).
func _sample(values: Array, t: float) -> float:
	var index := clampf(t, 0.0, 1.0) * float(values.size() - 1)
	var low := int(floor(index))
	var high := mini(low + 1, values.size() - 1)
	return lerpf(float(values[low]), float(values[high]), index - float(low))


func _draw() -> void:
	var fin := clampf(_fin, 0.0, 1.0)
	var fout := 1.0 - fin
	if _active:
		# `CoreBlock.drawLaunch`/`drawLanding` (`CoreBlock.java:427-538`):
		# every landing size is multiplied by `scl = Scl.scl(4) /
		# renderer.getDisplayScale()`, so the dropping core keeps a constant
		# on-screen size while the camera zooms from `landZoomFrom` to
		# `landZoomTo`.
		var scl := 4.0 / maxf(_scale, 0.0001)
		# `CoreBlock.java:438-443`: seeded streak rays behind the core.
		_draw_streaks(fin, fout, scl)
		if _block.is_empty():
			return
		var base := MindWidgets.icon_texture(_block)
		var full := MindWidgets.icon_texture("block-%s-full" % _block)
		var region := base if base != null else full
		if region == null:
			return
		# `region.width * region.scl()`: world units of the unscaled sprite.
		var region_size := region.get_size() * CORE_REGION_SUPERSAMPLE
		var size := float(_core_size())
		var core_rotation := pow(fout, 2.0) * 135.0
		# `CoreBlock.java:484-492`.
		var s := region_size.x * scl * 3.6 * (1.0 - pow(1.0 - fout, 2.0))
		var frame := 1.0 if fin >= 0.25 else fin / 0.25
		var thruster_size := _sample(THRUSTER_SIZES, fin)
		# `Draw.color(Pal.lightTrail); Draw.rect("circle-shadow", x, y, s, s)`.
		var shadow := MindWidgets.icon_texture("circle-shadow")
		if shadow != null and s > 0.0:
			_draw_region(shadow, _position, 0.0, Vector2(s, s), LIGHT_TRAIL)
		# Thruster flames (`CoreBlock.java:503-513`): team colour, then white.
		var strength := (
			(1.0 + (size - 3.0) / 2.5) * scl * thruster_size
			* (0.95 + _absin(_time_ticks(), 2.0, 0.1))
		)
		var offset := (size - 3.0) * 3.0 * scl
		for i in 4:
			var v := Vector2.RIGHT.rotated(deg_to_rad(i * 90.0 + core_rotation))
			var base_len := (size * 8.0 / 2.0 + 1.0) * scl
			if strength > 0.0:
				draw_circle(
					_position + v * (base_len + strength * 2.0 + offset), 6.0 * strength, _team_color
				)
				draw_circle(
					_position + v * (base_len + strength * 0.5 + offset), 3.5 * strength, Color.WHITE
				)
		# `drawLandingThrusters`, `Drawf.spinSprite(region)`, the
		# `pow4In(thrusterFrame)` thruster pass, then the team region
		# (`CoreBlock.java:515-525`).
		_draw_landing_thrusters(frame, core_rotation, 1.0, scl)
		_spin_sprite(region, core_rotation, region_size * scl, Color.WHITE)
		_draw_landing_thrusters(frame, core_rotation, pow(frame, 4.0), scl)
		if base != null:
			var team := MindWidgets.icon_texture("%s-team-%s" % [_block, _team])
			var tint := Color.WHITE
			if team == null:
				team = MindWidgets.icon_texture("%s-team" % _block)
				tint = _team_color
			if team != null:
				_spin_sprite(team, core_rotation, team.get_size() * CORE_REGION_SUPERSAMPLE * scl, tint)
	elif _thruster_time > 0.0:
		# Post-touchdown retraction (`CoreBuild.draw`, `CoreBlock.java:340-355`).
		_draw_thrusters(1.0 - _thruster_time)


## Arc `Draw.rect(region, x, y, w, h, rotation)` in the y-up world: the
## camera flips the canvas y axis, so the region is mirrored back (its top
## edge faces world `+y`) and `rotation` turns counter-clockwise on screen,
## both as in Java.
func _draw_region(
	texture: Texture2D, center: Vector2, angle: float, world_size: Vector2, tint: Color
) -> void:
	var tex_size := texture.get_size()
	if tex_size.x <= 0.0 or tex_size.y <= 0.0:
		return
	draw_set_transform(
		center, deg_to_rad(angle), Vector2(world_size.x / tex_size.x, -world_size.y / tex_size.y)
	)
	draw_texture(texture, -tex_size * 0.5, tint)
	draw_set_transform(Vector2.ZERO, 0.0, Vector2.ONE)


## `Drawf.spinSprite` (`Drawf.java:606-613`): the region at `r mod 90`, then
## again at `r - 90` with alpha `r / 90`, so a 4-fold sprite turns smoothly.
func _spin_sprite(texture: Texture2D, angle: float, world_size: Vector2, tint: Color) -> void:
	var r := fposmod(angle, 90.0)
	_draw_region(texture, _position, r, world_size, tint)
	var faded := tint
	faded.a *= r / 90.0
	if faded.a > 0.0:
		_draw_region(texture, _position, r - 90.0, world_size, faded)


## `CoreBlock.drawLaunch` streaks (`CoreBlock.java:438-443`):
## `Angles.randLenVectors(1, pfin, 100, 800 * scl * pfin, ...)` from Rust
## (`landing_streaks_json`), stroke `scl * ffin * pf * 3`, length
## `(ffin * 20 + 1) * scl`.
func _draw_streaks(fin: float, fout: float, scl: float) -> void:
	var camera := get_node_or_null("/root/Spine/World/Camera2D")
	if camera == null or not camera.has_method("landing_streaks_json"):
		return
	# `pfin = Interp.pow3Out(fin)`, `pf = Interp.pow2In(fout)`.
	var pfin := 1.0 - pow(1.0 - fin, 3.0)
	var pf := fout * fout
	var parsed: Variant = JSON.parse_string(
		str(camera.call("landing_streaks_json", pfin, 800.0 * scl * pfin))
	)
	if not (parsed is Array):
		return
	for row_variant in parsed as Array:
		var row: Array = row_variant
		var ffin := float(row[3])
		var width := scl * ffin * pf * 3.0
		if width <= 0.0:
			continue
		var from := _position + Vector2(float(row[0]), float(row[1]))
		var direction := Vector2.RIGHT.rotated(deg_to_rad(float(row[2])))
		draw_line(from, from + direction * (ffin * 20.0 + 1.0) * scl, LIGHT_TRAIL, width)


## `CoreBlock.drawLandingThrusters` (`CoreBlock.java:540-563`): two shading
## passes of four plates around the core; the second pass rotates each offset
## by -90 and scales the alpha by `(rotation % 90) / 90`. `base_alpha` is the
## caller's alpha (`pow4In(thrusterFrame)` for the second call site); `scl`
## is `Draw.xscl` (offsets and plate sizes).
func _draw_landing_thrusters(frame: float, angle: float, base_alpha: float, scl: float) -> void:
	var length := float(THRUSTER_LENGTHS.get(_block, THRUSTER_LENGTH)) * (frame - 1.0) - 0.25
	for pass_index in 2:
		for i in 4:
			var texture := MindWidgets.icon_texture(
				"%s-thruster%d" % [_block, 2 if i >= 2 else 1]
			)
			if texture == null:
				continue
			var rot := i * 90.0 + fmod(angle, 90.0)
			var offset := Vector2.RIGHT.rotated(deg_to_rad(rot)) * length * scl
			var alpha := base_alpha
			if pass_index == 1:
				offset = offset.rotated(deg_to_rad(-90.0))
				alpha = fmod(angle, 90.0) / 90.0 * base_alpha
				rot -= 90.0
			if alpha <= 0.0:
				continue
			_draw_region(
				texture,
				_position + offset,
				rot,
				texture.get_size() * CORE_REGION_SUPERSAMPLE * scl,
				Color(1, 1, 1, alpha)
			)


## `CoreBlock.drawThrusters` (`CoreBlock.java:566-573`): four plates at
## `d4[i] * length`, rotation `i * 90`.
func _draw_thrusters(frame: float) -> void:
	var length := float(THRUSTER_LENGTHS.get(_block, THRUSTER_LENGTH)) * (frame - 1.0) - 0.25
	for i in 4:
		var texture := MindWidgets.icon_texture("%s-thruster%d" % [_block, 2 if i >= 2 else 1])
		if texture == null:
			continue
		var offset: Vector2 = D4[i] * length
		_draw_region(
			texture,
			_position + offset,
			i * 90.0,
			texture.get_size() * CORE_REGION_SUPERSAMPLE,
			Color.WHITE
		)


## Live core block size (`3` for every core; the flame lengths use it).
func _core_size() -> int:
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("block_size"):
		return 3
	var camera := get_node_or_null("/root/Spine/World/Camera2D")
	@warning_ignore("integer_division")
	var tile := Vector2i(int(_position.x) / 8, int(_position.y) / 8)
	if camera != null and camera.has_method("landing_draw_json"):
		var parsed: Variant = JSON.parse_string(str(camera.call("landing_draw_json")))
		if parsed is Dictionary:
			@warning_ignore("integer_division")
			tile = Vector2i(
				int(float(parsed.get("x", 0.0))) / 8, int(float(parsed.get("y", 0.0))) / 8
			)
	return maxi(1, int(host.call("block_size", tile.x, tile.y)))


## `Time.time` in ticks (Java draws `absin(2, 0.1)` on it).
func _time_ticks() -> float:
	return float(Time.get_ticks_msec()) * 0.06


## `Mathf.absin(time, scl, mag)`.
func _absin(time: float, scl: float, mag: float) -> float:
	return (sin(time / (scl * 2.0)) + 1.0) * 0.5 * mag
