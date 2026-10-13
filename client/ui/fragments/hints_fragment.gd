## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/HintsFragment.java (45-160, 169-304).
##
## Hint catalogue + display driver. Hints never time out: they complete on their
## upstream predicate or the Skip button (which persists `<name>-hint-done`), a
## hint whose `shown` predicate turns false is hidden without persisting, and
## the display gate is `saves.getTotalPlaytime() > 8000` (total, not session,
## and only when not in a cutscene). The T4a slice ports the `desktopMove`/`zoom`/
## `desktopPause`/`launch`/`presetLaunch` predicates; T4b adds the rest.

extends Control

## Upstream `DefaultHint` declaration order (`HintsFragment.java:169-304`;
## `respawn` is mobile-only and dropped on desktop).
const HINTS := [
	"desktopMove",
	"zoom",
	"breaking",
	"desktopShoot",
	"depositItems",
	"desktopPause",
	"unitControl",
	"unitSelectControl",
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

## `HintsFragment` display gate: `saves.getTotalPlaytime() > 8000` ms.
const PLAYTIME_GATE_MS := 8000

var _current := ""
## Hints persisted as finished (`DefaultHint.finish`; Skip/complete paths).
var _done := {}
## Hints removed for this session (`hints.remove(hint)`; never persisted).
var _finished := {}
var _ui: Node = null
var _last_zoom := 0.0

@onready var _panel: Control = get_node_or_null("Panel")
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
	_snapshot_zoom()
	set_process(true)


func _process(_delta: float) -> void:
	# `HintsFragment.java:45`: the fragment shows only with the `hints` setting
	# on and the HUD up.
	if not _hints_enabled() or not _in_game():
		visible = false
		return
	# `HintsFragment.java:46-68` driver (the current hint first).
	if not _current.is_empty():
		visible = not _is_cutscene()
		if _hint_complete(_current):
			complete_hint()
		elif not _hint_shown(_current, _sector_state()):
			# `hide()`: not persisted; the hint stays in the list.
			_hide_current()
		return
	visible = false
	# `else if(!hints.isEmpty())`: first hint in ordinal order whose `show()`
	# holds. A hint already complete is removed silently (never persisted).
	var state := _sector_state()
	for hint_name in HINTS:
		if _done.has(hint_name) or _finished.has(hint_name) or _hint_finished(hint_name):
			continue
		if not _hint_shown(hint_name, state):
			continue
		if _hint_complete(hint_name):
			_finished[hint_name] = true
			continue
		# `renderer.isCutscene() || !state.isGame()`: wait (no display, no
		# derelict-mouse fallback in this slice).
		if _is_cutscene():
			return
		if _total_playtime_ms() <= PLAYTIME_GATE_MS:
			return
		show_hint(hint_name)
		return


## `HintsFragment.java:169-304` per-hint `complete()` predicates ported in T4a
## (the remaining 22 land with T4b; their input-key completions need the G08
## `MindInput.take_hint_keys()` seam).
func _hint_complete(hint_name: String) -> bool:
	match hint_name:
		"desktopMove":
			return _player_unit_moved()
		"zoom":
			# `Binding.zoom`: the camera zoom changed since the last poll.
			var camera := _camera()
			if camera == null:
				return false
			var zoom := float(camera.get("zoom").x)
			var zoomed := absf(zoom - _last_zoom) > 0.01
			_last_zoom = zoom
			return zoomed
		"desktopPause":
			return _is_paused()
		"launch":
			# `ui.planet.isShown()`.
			return _ui != null and _ui.has_method("is_dialog_shown") \
				and bool(_ui.call("is_dialog_shown", "planet"))
		"presetLaunch":
			var state := _sector_state()
			return bool(state.get("campaign", false)) \
				and str(state.get("preset", "")) == "frozenForest"
		_:
			return false


## `DefaultHint.show()` predicates the port can evaluate (T4a set; T4b adds the
## rest). Hints whose predicate needs state the port does not expose yet stay
## hidden rather than firing out of context.
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


## `HintsFragment.complete()`: persists the hint and removes it.
func complete_hint() -> void:
	if not _current.is_empty():
		_done[_current] = true
		if _ui != null and _ui.has_method("hint_finish"):
			_ui.call("hint_finish", _current)
	_current = ""
	visible = false
	if _panel != null:
		_panel.modulate.a = 1.0


## `HintsFragment.hide()` (fade 0.6 s + translate y -200; not persisted).
func _hide_current() -> void:
	var name_done := _current
	_current = ""
	visible = false
	if _panel == null:
		return
	var tween := _panel.create_tween()
	tween.set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)
	tween.tween_property(_panel, "modulate:a", 0.0, 0.6)
	tween.parallel().tween_property(_panel, "position:y", _panel.position.y + 200.0, 0.6)
	tween.tween_callback(func() -> void:
		if _current != name_done:
			_panel.position.y -= 200.0
			_panel.modulate.a = 1.0)


## `HintsFragment.display()`: fade the panel in over 1 s.
func show_hint(hint_name: String) -> void:
	_current = hint_name
	_finished.erase(hint_name)
	if _panel != null:
		var tween := _panel.create_tween()
		tween.tween_property(_panel, "modulate:a", 1.0, 1.0).from(0.0)
	if _label != null:
		# `HintsFragment.java:349-355`: desktop replaces tap -> click.
		var text := MindWidgets.markup("@hint.%s" % hint_name)
		_label.text = text.replace("tap", "click").replace("Tap", "Click")
	visible = true
	_snapshot_zoom()


func _sector_state() -> Dictionary:
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign == null or not campaign.has_method("get_sector_state"):
		return {}
	var state: Variant = campaign.call("get_sector_state")
	return state if state is Dictionary else {}


## `DefaultHint.finished`: `Core.settings.getBool(name + "-hint-done")`.
func _hint_finished(hint_name: String) -> bool:
	return _ui != null and _ui.has_method("hint_finished") and bool(_ui.call("hint_finished", hint_name))


## `saves.getTotalPlaytime() > 8000` (total, not session time).
func _total_playtime_ms() -> int:
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign == null or not campaign.has_method("get_total_playtime"):
		return 0
	return int(campaign.call("get_total_playtime"))


## `settings.getBool("hints", true)`.
func _hints_enabled() -> bool:
	if _ui == null:
		return true
	if not _ui.has_method("settings_get_flag_default"):
		return true
	return bool(_ui.call("settings_get_flag_default", "hints", true))


## The hint overlay shows only during a live (non-menu) game and when the HUD
## is up (`ui.hudfrag.shown()`).
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


## `renderer.isCutscene()`: no hint triggers or advances during a cutscene.
func _is_cutscene() -> bool:
	var camera := _camera()
	return camera != null and camera.has_method("is_cutscene") and bool(camera.call("is_cutscene"))


func _camera() -> Node:
	return get_node_or_null("/root/Spine/World/Camera2D")


## `DesktopHint.complete`: the player unit's move axis is non-zero.
func _player_unit_moved() -> bool:
	var host := get_node_or_null("/root/Spine/SimHost")
	return host != null and host.has_method("player_unit_moved") and bool(host.call("player_unit_moved"))


func _snapshot_zoom() -> void:
	var camera := _camera()
	if camera != null:
		_last_zoom = float(camera.get("zoom").x)


func _t(key: String) -> String:
	var resolved := key.trim_prefix("@")
	var assets := MindWidgets.assets()
	if assets == null:
		return resolved
	return str(assets.call("bundle_get", resolved))
