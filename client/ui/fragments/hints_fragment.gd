## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/HintsFragment.java (45-160, 169-304).
##
## Hint catalogue + display driver. Hints never time out: they complete on their
## upstream predicate or the Skip button (which persists `<name>-hint-done`), a
## hint whose `shown` predicate turns false is hidden without persisting, and
## the display gate is `saves.getTotalPlaytime() > 8000` (total, not session,
## and only when not in a cutscene). Input completions consume
## `MindInput.take_hint_keys()` (G08 T15); `derelictmouse` is host-detected from
## the hovered tile's team.

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
## Pending input triggers from `MindInput.take_hint_keys()` (G08 T15).
var _hint_keys: PackedStringArray = []
## Last probed hover tile (`derelictmouse`).
var _mouse_tile := Vector2i(-1, -1)

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
	_poll_hint_keys()
	# One poll per frame (`hint_state_json` sweeps the buildings and grid).
	var hints := _hint_state()
	var state := _sector_state()
	# `HintsFragment.java:46-68` driver (the current hint first).
	if not _current.is_empty():
		visible = not _is_cutscene()
		if _hint_complete(_current, state, hints):
			complete_hint()
		elif not _hint_shown(_current, state, hints):
			# `hide()`: not persisted; the hint stays in the list.
			_hide_current()
		return
	visible = false
	_poll_derelict_mouse()
	# `else if(!hints.isEmpty())`: first hint in ordinal order whose `show()`
	# holds. A hint already complete is removed silently (never persisted).
	for hint_name in HINTS:
		if _done.has(hint_name) or _finished.has(hint_name) or _hint_finished(hint_name):
			continue
		if not _hint_shown(hint_name, state, hints):
			continue
		if _hint_complete(hint_name, state, hints):
			_finished[hint_name] = true
			continue
		# `renderer.isCutscene() || !state.isGame()`: wait (no display).
		if _is_cutscene():
			return
		if _total_playtime_ms() <= PLAYTIME_GATE_MS:
			return
		show_hint(hint_name)
		return


## `HintsFragment.java:169-304` per-hint `shown()` predicates. `state` is the
## `MindCampaign.get_sector_state()` projection, `hints` the `hint_state_json`
## poll (both fetched once per frame).
func _hint_shown(hint_name: String, state: Dictionary, hints: Dictionary) -> bool:
	var preset := str(state.get("preset", ""))
	var campaign := bool(state.get("campaign", false))
	var planet := str(state.get("planet", ""))
	var tutorial := preset == "groundZero"
	var serpulo := bool(hints.get("serpulo", true))
	var dead := bool(hints.get("player_dead", true))
	match hint_name:
		"desktopMove", "zoom":
			return true
		"breaking":
			# `isTutorial && team conveyor count > 5`
			return tutorial and int(hints.get("conveyors", 0)) > 5
		"desktopShoot":
			# `isSerpulo && enemies > 0`
			return serpulo and int(hints.get("enemies", 0)) > 0
		"depositItems":
			return not dead and bool(hints.get("player_has_item", false))
		"desktopPause":
			# `isTutorial && !net.active() && state.wave >= 2`
			return tutorial and int(state.get("wave", 0)) >= 2
		"unitControl":
			return serpulo and int(hints.get("team_units", 0)) > 2 and not dead
		"unitSelectControl":
			return serpulo and int(hints.get("team_units", 0)) > 3 and not dead \
				and preset != "fungalPass" and not bool(hints.get("fungal_pass_saved", false))
		"launch":
			return (preset == "groundZero" or preset == "onset") and bool(state.get("captured", false))
		"schematicSelect":
			return _placed_has(hints, ["router", "duct-router"])
		"conveyorPathfind":
			return _selected_block() == "titanium-conveyor"
		"boost":
			return not dead and bool(hints.get("player_can_boost", false))
		"blockInfo":
			return _selected_block() == "graphite-press"
		"derelict":
			return _has_event(hints, "derelictmouse") and not tutorial
		"payloadPickup":
			return serpulo and not dead and int(hints.get("player_payload", -1)) == 0
		"payloadDrop":
			return not dead and int(hints.get("player_payload", -1)) > 0
		"waveFire":
			return _has_event(hints, "fire") and _content_unlocked("wave")
		"rebuildSelect":
			return int(hints.get("team_rebuild_plans", 0)) >= 10
		"guardian":
			var boss := _boss(hints)
			return not boss.is_empty() and serpulo \
				and float(boss.get("armor", 0.0)) >= 4.0 \
				and _content_unlocked("salvo") \
				and not bool(boss.get("flying", false))
		"cannotUpgrade":
			return _has_event(hints, "cannotupgrade")
		"factoryControl":
			return not (campaign and preset == "onset") and int(hints.get("fabricators", 0)) > 0
		"coreUpgrade":
			return campaign and planet == "serpulo" \
				and _content_unlocked("core-foundation") \
				and str(hints.get("core_block", "")) == "core-shard" \
				and bool(hints.get("core_has_foundation_cost", false))
		"serpuloCoreZone":
			return campaign and planet == "serpulo" \
				and bool(hints.get("core_zone_present", false)) \
				and (not bool(hints.get("attack_mode", false)) or int(hints.get("cores_destroyed", 0)) > 0)
		"presetLaunch":
			return campaign and preset.is_empty() and state.has("sector")
		"presetDifficulty":
			return campaign and preset.is_empty() \
				and float(hints.get("sector_threat", 0.0)) >= 0.5 \
				and not bool(hints.get("tar_fields_captured", true))
		"coreIncinerate":
			return campaign and not str(hints.get("core_block", "")).is_empty() \
				and int(hints.get("core_copper", 0)) >= int(hints.get("core_capacity", 0)) - 10
		_:
			return false


## The matching `complete()` predicates (`HintsFragment.java:169-304`).
func _hint_complete(hint_name: String, state: Dictionary, hints: Dictionary) -> bool:
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
		"breaking":
			return _has_event(hints, "break")
		"desktopShoot":
			return _consume_hint_key("shoot")
		"depositItems":
			return not bool(hints.get("player_dead", true)) and not bool(hints.get("player_has_item", false))
		"desktopPause":
			return _is_paused()
		"unitControl":
			return not bool(hints.get("player_dead", true)) and not bool(hints.get("player_spawned_by_core", true))
		"unitSelectControl":
			# `commandMode && selectedUnits > 0 && first CommandAI.targetPos
			# != null` (`HintsFragment.java:199-203`).
			return _input_command_mode() and _selection_size() > 0 \
				and _first_selection_has_target()
		"launch":
			return _dialog_shown("planet")
		"schematicSelect":
			return _consume_hint_key("schematic_select_release") or _consume_hint_key("pick_tap")
		"conveyorPathfind":
			return _consume_hint_key("diagonal_release")
		"boost":
			return _consume_hint_key("boost_down")
		"blockInfo":
			return _dialog_shown("content")
		"derelict":
			return _has_event(hints, "derelictbreak")
		"payloadPickup":
			return int(hints.get("player_payload", -1)) > 0
		"payloadDrop":
			return int(hints.get("player_payload", -1)) == 0
		"waveFire":
			return int(hints.get("extinguishers", 0)) > 0
		"rebuildSelect":
			return _consume_hint_key("rebuild_selecting")
		"guardian":
			return int(_boss_root(hints).get("count", 0)) == 0
		"cannotUpgrade":
			# `complete: () -> false` (`HintsFragment.java:266-269`): the hint is
			# only ever dismissed manually.
			return false
		"factoryControl":
			return _has_event(hints, "factorycontrol")
		"coreUpgrade":
			return _placed_has(hints, ["core-foundation"])
		"serpuloCoreZone":
			return int(hints.get("cores", 0)) > 1
		"presetLaunch":
			return bool(state.get("campaign", false)) and not str(state.get("preset", "")).is_empty()
		"presetDifficulty":
			return bool(state.get("campaign", false)) and not str(state.get("preset", "")).is_empty()
		"coreIncinerate":
			return false
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


## `MindInput.take_hint_keys()` (G08 T15) buffered into `_hint_keys`.
func _poll_hint_keys() -> void:
	var input := get_node_or_null("/root/Spine/Input")
	if input == null or not input.has_method("take_hint_keys"):
		return
	var keys: PackedStringArray = input.call("take_hint_keys")
	for key in keys:
		if not _hint_keys.has(key):
			_hint_keys.append(key)


## Consumes a pending input trigger (one-shot; `keyTap`/`keyRelease` parity).
func _consume_hint_key(key: String) -> bool:
	var index := _hint_keys.find(key)
	if index < 0:
		return false
	_hint_keys.remove_at(index)
	return true


## `HintsFragment.java:100-102`: while no hint is up, hovering a team-0
## (derelict) building adds `derelictmouse`.
func _poll_derelict_mouse() -> void:
	var camera := _camera()
	var host := get_node_or_null("/root/Spine/SimHost")
	if camera == null or host == null or not host.has_method("building_team_at"):
		return
	if not camera.has_method("screen_to_tile"):
		return
	var mouse := get_viewport().get_mouse_position()
	var tile: Vector2i = camera.call("screen_to_tile", mouse.x, mouse.y)
	if tile.x < 0 or tile.y < 0 or tile == _mouse_tile:
		return
	_mouse_tile = tile
	if int(host.call("building_team_at", tile.x, tile.y)) == 0:
		host.call("hint_event", "derelictmouse")


func _sector_state() -> Dictionary:
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign == null or not campaign.has_method("get_sector_state"):
		return {}
	var state: Variant = campaign.call("get_sector_state")
	return state if state is Dictionary else {}


## `MindSimHost.hint_state_json()` (one sweep per frame).
func _hint_state() -> Dictionary:
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("hint_state_json"):
		return {}
	var parsed: Variant = JSON.parse_string(str(host.call("hint_state_json")))
	return parsed if parsed is Dictionary else {}


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


## Whether `placedBlocks` contains any of `names` (`HintsFragment.schematicSelect`
## and `coreUpgrade` predicates).
func _placed_has(hints: Dictionary, names: Array) -> bool:
	var placed: Array = hints.get("placed", [])
	for name in names:
		if placed.has(name):
			return true
	return false


## Whether the event set contains `name`.
func _has_event(hints: Dictionary, name: String) -> bool:
	return (hints.get("events", []) as Array).has(name)


## The first boss entry (`boss_json.first`), or `{}`.
func _boss(hints: Dictionary) -> Dictionary:
	var boss: Variant = _boss_root(hints).get("first")
	return boss if boss is Dictionary else {}


## The whole `boss_json` object (count/names/value/first).
func _boss_root(hints: Dictionary) -> Dictionary:
	var boss: Variant = hints.get("boss")
	return boss if boss is Dictionary else {}


## `UnlockableContent.unlockedHost()` by name (G05 T4b gates).
func _content_unlocked(name: String) -> bool:
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign == null or not campaign.has_method("content_unlocked"):
		return true
	return bool(campaign.call("content_unlocked", name))


## `state.block.name` (`MindSimHost.selected_block`).
func _selected_block() -> String:
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("selected_block"):
		return ""
	return str(host.call("selected_block"))


## `input.commandMode` (G08).
func _input_command_mode() -> bool:
	var input := get_node_or_null("/root/Spine/Input")
	return input != null and input.has_method("command_mode") and bool(input.call("command_mode"))


## Selected unit count (`control.input.selectedUnits.size`).
func _selection_size() -> int:
	var input := get_node_or_null("/root/Spine/Input")
	if input == null or not input.has_method("selection_json"):
		return 0
	var parsed: Variant = JSON.parse_string(str(input.call("selection_json")))
	return (parsed as Array).size() if parsed is Array else 0


## `selectedUnits.first().controller() instanceof CommandAI ai && ai.targetPos
## != null` (`HintsFragment.java:199-203`): the first selected unit id
## (`selection_json` order) must have a live command target row
## (`SimHost.command_targets_json` merges the move target and attack target).
func _first_selection_has_target() -> bool:
	var input := get_node_or_null("/root/Spine/Input")
	if input == null or not input.has_method("selection_json"):
		return false
	var selection: Variant = JSON.parse_string(str(input.call("selection_json")))
	if not (selection is Array) or (selection as Array).is_empty():
		return false
	var first_id := int((selection as Array)[0])
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("command_targets_json"):
		return false
	var targets: Variant = JSON.parse_string(str(host.call("command_targets_json")))
	if not (targets is Array):
		return false
	for row_variant in targets as Array:
		if row_variant is Dictionary and int((row_variant as Dictionary).get("id", -1)) == first_id:
			return true
	return false


func _dialog_shown(name: String) -> bool:
	return _ui != null and _ui.has_method("is_dialog_shown") and bool(_ui.call("is_dialog_shown", name))


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
