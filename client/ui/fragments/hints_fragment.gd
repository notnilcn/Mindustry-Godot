## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/HintsFragment.java (42-160, 118-137).
##
## Hint catalogue + display driver. The full upstream trigger model (placed
## blocks / input events / playtime save) is not ported; this driver shows the
## ordered catalogue one hint at a time once the playtime gate is met, with a
## timed auto-advance and the upstream `@hint.skip` button. Completion is
## in-memory for the session (no settings persistence yet).

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
var _next := 0
var _playtime := 0.0
var _elapsed := 0.0
var _ui: Node = null
var _last_camera := Vector2.ZERO
var _last_zoom := 0.0

@onready var _label: RichTextLabel = get_node_or_null("Panel/Layout/Label")
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
## (HintsFragment.java:170) — the player unit via SimHost when present,
## else the movement bindings moving the camera; `zoom` when the scroll
## changes the zoom.
func _check_trigger() -> void:
	if _current.is_empty():
		return
	if _current == "desktopMove" and _player_unit_moved():
		complete_hint()
		return
	var camera := get_node_or_null("/root/Spine/World/Camera2D")
	if camera == null:
		return
	var position: Vector2 = camera.get("position")
	var zoom := float(camera.get("zoom").x)
	var moved := position.distance_to(_last_camera) > MOVE_EPSILON
	var zoomed := absf(zoom - _last_zoom) > 0.01
	_last_camera = position
	_last_zoom = zoom
	if (_current == "desktopMove" and moved) or (_current == "zoom" and zoomed):
		complete_hint()


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
	if _next >= HINTS.size():
		return
	show_hint(HINTS[_next])
	_next += 1


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
