## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/HintsFragment.java (42-160, 118-137).
##
## Hint catalogue + display driver. The full upstream trigger model (placed
## blocks / input events / playtime save) is not ported; this driver shows the
## first unfinished hint whose `shown` predicate holds (only the predicates the
## port can evaluate; the rest stay hidden), with a timed auto-advance and the
## upstream `@hint.skip` button. Completion persists
## as `<name>-hint-done` (`DefaultHint.finished/finish`) through `MindUi`, so an
## imported Java profile's finished hints stay hidden.

extends Control

## Upstream `DefaultHint` declaration order (bundle keys `hint.<name>`).
const HINTS := [
	"desktopMove",
	"zoom",
	"desktopShoot",
	"breaking",
	"depositItems",
	"desktopPause",
	"unitControl",
	"unitSelectControl",
	"respawn",
	"launch",
	"schematicSelect",
	"conveyorPathfind",
	"boost",
	"blockInfo",
	"derelict",
	"payloadPickup",
	"payloadDrop",
	"waveFire",
	"rebuildSelect",
	"guardian",
	"cannotUpgrade",
	"factoryControl",
	"coreUpgrade",
	"serpuloCoreZone",
	"presetLaunch",
	"presetDifficulty",
	"coreIncinerate",
]

## `HintsFragment` gate: total playtime must exceed 8000 ms before a hint shows.
const PLAYTIME_GATE := 8.0
## Camera-movement threshold that counts as "the player moved" for the
## `desktopMove`/`zoom` trigger hints (the port has no player entity yet, so the
## shared WASD/scroll bindings drive the camera).
const MOVE_EPSILON := 0.5
## Timed auto-advance fallback for hints whose upstream trigger is not ported yet.
const HINT_DURATION := 15.0

var _current := ""
## Hints completed this session (`hints.remove(current)`).
var _done := {}
var _playtime := 0.0
var _elapsed := 0.0
var _ui: Node = null
var _last_camera := Vector2.ZERO
var _last_zoom := 0.0

@onready var _label: RichTextLabel = get_node_or_null("Panel/Layout/Box/Label")
@onready var _skip: Button = get_node_or_null("Panel/Layout/Skip")


func _ready() -> void:
	visible = false
	_ui = get_node_or_null("/root/MindUi")
	if _label != null:
		_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	if _skip != null:
		_skip.text = _t("@hint.skip")
		if not _skip.pressed.is_connected(complete_hint):
			_skip.pressed.connect(complete_hint)
	_snapshot_camera()
	set_process(true)


func _process(delta: float) -> void:
	var active := _in_game()
	visible = active and not _current.is_empty()
	if not active:
		return
	if _is_paused():
		# `desktopPause` completes on the pause key (`Binding.pause`).
		if _current == "desktopPause":
			complete_hint()
		return
	_check_trigger()
	if not _current.is_empty():
		if _current != "desktopMove" and _current != "zoom":
			_elapsed += delta
			if _elapsed >= HINT_DURATION:
				complete_hint()
		return
	_playtime += delta
	if _playtime >= PLAYTIME_GATE:
		_show_next()


## Upstream hints complete on their trigger, not a timer (`DesktopHint`
## predicate). `desktopMove` completes on the first non-zero move axis
## (HintsFragment.java:170) — the player unit via SimHost when present (the
## authoritative hook); the camera-delta fallback only applies to hosts without
## it (a launch camera pan must not complete the hint); `zoom` completes when
## the scroll changes the zoom.
func _check_trigger() -> void:
	if _current.is_empty():
		return
	if _current == "desktopMove":
		var host := get_node_or_null("/root/Spine/SimHost")
		if host != null and host.has_method("player_unit_moved"):
			if bool(host.call("player_unit_moved")):
				complete_hint()
			return
		if _camera_moved():
			complete_hint()
		return
	var camera := get_node_or_null("/root/Spine/World/Camera2D")
	if camera == null:
		return
	var position: Vector2 = camera.get("position")
	var zoom := float(camera.get("zoom").x)
	_last_camera = position
	var zoomed := absf(zoom - _last_zoom) > 0.01
	_last_zoom = zoom
	if _current == "zoom" and zoomed:
		complete_hint()


## Camera-delta fallback for hosts without the player-unit hook.
func _camera_moved() -> bool:
	var camera := get_node_or_null("/root/Spine/World/Camera2D")
	if camera == null:
		return false
	var position: Vector2 = camera.get("position")
	var moved := position.distance_to(_last_camera) > MOVE_EPSILON
	_last_camera = position
	return moved


func _snapshot_camera() -> void:
	var camera := get_node_or_null("/root/Spine/World/Camera2D")
	if camera != null:
		_last_camera = camera.get("position")
		_last_zoom = float(camera.get("zoom").x)


## Upstream `HintsFragment.desktopMove`: the hint completes on the first
## non-zero move axis (`Core.input.axis(Binding.moveX/moveY)`).
func _player_unit_moved() -> bool:
	var host := get_node_or_null("/root/Spine/SimHost")
	return host != null and host.has_method("player_unit_moved") and bool(host.call("player_unit_moved"))


## The hint overlay shows only during a live (non-menu) game and when the HUD
## is enabled (`hints` setting defaults to true; `MindUi.hud_visible`).
func _in_game() -> bool:
	if _ui != null and not bool(_ui.call("hud_visible")):
		return false
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("get_state"):
		return false
	return str(host.call("get_state")) == "playing"


func _is_paused() -> bool:
	var host := get_node_or_null("/root/Spine/SimHost")
	return host != null and host.has_method("is_paused") and bool(host.call("is_paused"))


func _show_next() -> void:
	# `checkNext`: finished hints are dropped, then the first hint (declaration
	# order) whose `shown` predicate holds is displayed.
	var state := _sector_state()
	for hint_name in HINTS:
		if _done.has(hint_name) or _hint_finished(hint_name):
			continue
		if _hint_shown(hint_name, state):
			show_hint(hint_name)
			return


## `DefaultHint.show()` predicates the port can evaluate. Hints whose upstream
## predicate reads state the port does not expose yet (enemy counts, unit
## payloads, placed-block events, ...) stay hidden rather than firing out of
## context.
func _hint_shown(hint_name: String, state: Dictionary) -> bool:
	var preset := str(state.get("preset", ""))
	var campaign := bool(state.get("campaign", false))
	match hint_name:
		"desktopMove", "zoom":
			return true
		"desktopPause":
			# `isTutorial && !net.active() && state.wave >= 2`
			return preset == "groundZero" and int(state.get("wave", 0)) >= 2
		"launch":
			return (preset == "groundZero" or preset == "onset") and bool(state.get("captured", false))
		"presetLaunch":
			return campaign and preset.is_empty() and state.has("sector")
		_:
			return false


func _sector_state() -> Dictionary:
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign == null or not campaign.has_method("get_sector_state"):
		return {}
	var state: Variant = campaign.call("get_sector_state")
	return state if state is Dictionary else {}


## `DefaultHint.finished`: `Core.settings.getBool(name + "-hint-done")`.
func _hint_finished(hint_name: String) -> bool:
	return _ui != null and _ui.has_method("hint_finished") and bool(_ui.call("hint_finished", hint_name))


## Shows the hint by catalogue name (bundle `hint.<name>`).
func show_hint(hint_name: String) -> void:
	if _label != null:
		_label.text = MindWidgets.markup("@hint.%s" % hint_name)
	_current = hint_name
	_elapsed = 0.0
	_snapshot_camera()
	visible = true


## Completes/hides the current hint and arms the next one.
func complete_hint() -> void:
	# `HintsFragment.complete` -> `current.finish()` persists the flag.
	if not _current.is_empty():
		_done[_current] = true
		if _ui != null and _ui.has_method("hint_finish"):
			_ui.call("hint_finish", _current)
	_current = ""
	_elapsed = 0.0
	_snapshot_camera()
	visible = false


func _t(key: String) -> String:
	var resolved := key.trim_prefix("@")
	var assets := MindWidgets.assets()
	if assets == null:
		return resolved
	return str(assets.call("bundle_get", resolved))
