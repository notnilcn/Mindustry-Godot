## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/world/blocks/storage/CoreBlock.java (drawLanding,
## beginLaunch; 369-418, 480-538).
##
## World-space landing core sprite. While `MindCamera2D.landing_draw_json()`
## reports an active landing, draws the dropping core (scaled and rotated per
## the Java formulas, a `circle-shadow` tinted `Pal.lightTrail` behind it) over
## its tile. The white screen fade starts when the landing begins and the
## sector announce (`[accent]<sector>` + stored resources) fires when it ends.
## View-only: reads Rust state, never mutates it. Thrusters and hiding the real
## core building are not drawn in this slice (recorded as remaining).

extends Node2D

## Region pixels -> world pixels (block regions are 4x supersampled).
const CORE_REGION_SUPERSAMPLE := 0.25
## `Pal.lightTrail` (`drawf.rs::LIGHT_TRAIL`).
const LIGHT_TRAIL := Color("ffe2a9")
## `thrusterSizes` frame duration for the fade-out (`CoreBlock.java:44`).
const FADE_TICKS := 35.0 / 60.0

var _active := false
var _announced := false
var _block := ""
var _position := Vector2.ZERO
var _fin := 1.0


func _ready() -> void:
	set_process(true)


func _process(_delta: float) -> void:
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
	elif not active and _active:
		_finish_landing()
	_active = active
	if _active:
		queue_redraw()


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
	# The toast label expects translated BBCode (`MindRichLabel`).
	ui.call("announce", MindWidgets.markup(text), 5.0)


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


func _draw() -> void:
	if not _active or _block.is_empty():
		return
	var texture := MindWidgets.icon_texture("block-%s-full" % _block)
	if texture == null:
		return
	var fout := 1.0 - clampf(_fin, 0.0, 1.0)
	# `Interp.pow2Out`/`Interp.pow2In` (CoreBlock.java:485-486).
	var scale_factor := 3.6 * (1.0 - pow(1.0 - fout, 2.0))
	var rotation := pow(fout, 2.0) * 135.0
	# `region.width * region.scl() * 3.6 * pow2Out(fout)`, `region.scl()` = 1/4.
	var size := texture.get_size() * CORE_REGION_SUPERSAMPLE * scale_factor
	if size.x <= 0.0 or size.y <= 0.0:
		return
	var shadow := MindWidgets.icon_texture("circle-shadow")
	if shadow != null:
		draw_texture_rect(
			shadow,
			Rect2(_position - size * 0.5, size),
			false,
			LIGHT_TRAIL
		)
	draw_set_transform(_position, deg_to_rad(rotation), size / texture.get_size())
	draw_texture(texture, -texture.get_size() * 0.5)
	draw_set_transform(Vector2.ZERO, 0.0, Vector2.ONE)
