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
## Note: Arc is y-up, Godot is y-down; like the existing core draw this file
## feeds the Java angles/offsets straight into Godot, which mirrors the
## rotational direction but keeps every magnitude and every 90-degree-divisible
## direction exact.

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
## `CoreBlock.thrusterLength` = `14f / 4f`.
const THRUSTER_LENGTH := 14.0 / 4.0
## `Geometry.d4` (`Arc`): right, down, left, up in Godot's y-down space.
const D4 := [Vector2(1, 0), Vector2(0, 1), Vector2(-1, 0), Vector2(0, -1)]

var _active := false
var _announced := false
var _block := ""
var _position := Vector2.ZERO
var _fin := 1.0
## `CoreBuild.thrusterTime`: 1 at touchdown, `-delta / 90` per tick.
var _thruster_time := 0.0
## Player team colour for the landing flames (cached on first use).
var _team_color := Color("d4816b")


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
	var preset := str(state.get("preset", ""))
	var name := str(state.get("sector", ""))
	if not preset.is_empty():
		var key := "sector.%s.name" % preset
		var localized := _bundle(key)
		name = localized if not localized.is_empty() and localized != key else preset
	var text := "[accent]%s" % name
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
		# `CoreBlock.java:443-448`: seeded streak rays behind the core.
		_draw_streaks(fin, fout)
		if _block.is_empty():
			return
		var texture := MindWidgets.icon_texture("block-%s-full" % _block)
		if texture == null:
			return
		# `CoreBlock.java:480-538`.
		var size := float(_core_size())
		var thruster_size := _sample(THRUSTER_SIZES, fin)
		var strength := (
			(1.0 + (size - 3.0) / 2.5) * thruster_size * (0.95 + _absin(_time_ticks(), 2.0, 0.1))
		)
		var offset := (size - 3.0) * 3.0
		var rotation := pow(fout, 2.0) * 135.0
		for i in 4:
			# `v = trns(i * 90 + rotation, 1)` with the Java lengths.
			var v := Vector2.RIGHT.rotated(deg_to_rad(i * 90.0 + rotation))
			draw_circle(
				_position + v * ((size * 4.0 + 1.0) + strength * 2.0 + offset),
				6.0 * strength,
				_team_color
			)
			draw_circle(
				_position + v * ((size * 4.0 + 1.0) + strength * 0.5 + offset),
				3.5 * strength,
				Color.WHITE
			)
		# `Interp.pow2Out`/`Interp.pow2In` (CoreBlock.java:485-486).
		var scale_factor := 3.6 * (1.0 - pow(1.0 - fout, 2.0))
		var draw_size := texture.get_size() * CORE_REGION_SUPERSAMPLE * scale_factor
		if draw_size.x <= 0.0 or draw_size.y <= 0.0:
			return
		var shadow := MindWidgets.icon_texture("circle-shadow")
		if shadow != null:
			draw_texture_rect(
				shadow, Rect2(_position - draw_size * 0.5, draw_size), false, LIGHT_TRAIL
			)
		# The white drop tint (`Draw.mixcol(white, pow5In(fout))`) keeps the
		# sprite white; the accent tint only applies while launching.
		var frame := 1.0 if fin >= 0.25 else fin / 0.25
		_draw_landing_thrusters(frame, rotation, 1.0)
		draw_set_transform(
			_position, deg_to_rad(rotation), draw_size / texture.get_size()
		)
		draw_texture(texture, -texture.get_size() * 0.5)
		draw_set_transform(Vector2.ZERO, 0.0, Vector2.ONE)
		# Second thruster pass at `alpha pow4In(thrusterFrame)`
		# (`CoreBlock.java:519-521`), then the team top region (`:523-525`)
		# whose overlay the composed `-full` icon already carries.
		_draw_landing_thrusters(frame, rotation, pow(frame, 4.0))
	elif _thruster_time > 0.0:
		# Post-touchdown retraction (`CoreBuild.draw`, `CoreBlock.java:340-355`).
		_draw_thrusters(1.0 - _thruster_time)


## `CoreBlock.drawLaunch` streaks: `Angles.randLenVectors(1, pfin, 100,
## 800 * pfin, ...)` from Rust (`landing_streaks_json`).
func _draw_streaks(fin: float, fout: float) -> void:
	var camera := get_node_or_null("/root/Spine/World/Camera2D")
	if camera == null or not camera.has_method("landing_streaks_json"):
		return
	# `pfin = Interp.pow3Out(fin)`, `pf = Interp.pow2In(fout)`.
	var pfin := 1.0 - pow(1.0 - fin, 3.0)
	var pf := fout * fout
	var parsed: Variant = JSON.parse_string(
		str(camera.call("landing_streaks_json", pfin, 800.0 * pfin))
	)
	if not (parsed is Array):
		return
	for row_variant in parsed as Array:
		var row: Array = row_variant
		var ffin := float(row[3])
		var width := ffin * pf * 3.0
		if width <= 0.0:
			continue
		var from := _position + Vector2(float(row[0]), float(row[1]))
		var direction := Vector2.RIGHT.rotated(deg_to_rad(float(row[2])))
		draw_line(from, from + direction * (ffin * 20.0 + 1.0), LIGHT_TRAIL, width)


## `CoreBlock.drawLandingThrusters` (`CoreBlock.java:540-563`): two shading
## passes of four plates around the core; the second pass rotates each offset
## by -90 and scales the alpha by `(rotation % 90) / 90`. `base_alpha` is the
## caller's alpha (`pow4In(thrusterFrame)` for the second call site).
func _draw_landing_thrusters(frame: float, rotation: float, base_alpha: float) -> void:
	var length := THRUSTER_LENGTH * (frame - 1.0) - 0.25
	for pass_index in 2:
		for i in 4:
			var texture := MindWidgets.icon_texture(
				"%s-thruster%d" % [_block, 2 if i >= 2 else 1]
			)
			if texture == null:
				continue
			var rot := i * 90.0 + fmod(rotation, 90.0)
			var offset := Vector2.RIGHT.rotated(deg_to_rad(rot)) * length
			var alpha := base_alpha
			if pass_index == 1:
				offset = offset.rotated(deg_to_rad(-90.0))
				alpha = fmod(rotation, 90.0) / 90.0 * base_alpha
				rot -= 90.0
			if alpha <= 0.0:
				continue
			draw_set_transform(
				_position + offset,
				deg_to_rad(rot),
				Vector2(CORE_REGION_SUPERSAMPLE, CORE_REGION_SUPERSAMPLE)
			)
			draw_texture(texture, -texture.get_size() * 0.5, Color(1, 1, 1, alpha))
	draw_set_transform(Vector2.ZERO, 0.0, Vector2.ONE)


## `CoreBlock.drawThrusters` (`CoreBlock.java:566-573`): four plates at
## `d4[i] * length`, rotation `i * 90`.
func _draw_thrusters(frame: float) -> void:
	var length := THRUSTER_LENGTH * (frame - 1.0) - 0.25
	for i in 4:
		var texture := MindWidgets.icon_texture("%s-thruster%d" % [_block, 2 if i >= 2 else 1])
		if texture == null:
			continue
		var offset: Vector2 = D4[i] * length
		draw_set_transform(
			_position + offset,
			deg_to_rad(i * 90.0),
			Vector2(CORE_REGION_SUPERSAMPLE, CORE_REGION_SUPERSAMPLE)
		)
		draw_texture(texture, -texture.get_size() * 0.5)
	draw_set_transform(Vector2.ZERO, 0.0, Vector2.ONE)


## Live core block size (`3` for every core; the flame lengths use it).
func _core_size() -> int:
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("block_size"):
		return 3
	var camera := get_node_or_null("/root/Spine/World/Camera2D")
	var tile := Vector2i(int(_position.x) / 8, int(_position.y) / 8)
	if camera != null and camera.has_method("landing_draw_json"):
		var parsed: Variant = JSON.parse_string(str(camera.call("landing_draw_json")))
		if parsed is Dictionary:
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
