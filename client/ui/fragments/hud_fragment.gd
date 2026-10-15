## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/HudFragment.java (plan 14 §3.5, M4).
##
## In-game HUD. The named regions from §3.5 are declared statically in
## `hud_fragment.tscn` (tscn-first); this script binds them to the read-only
## `MindHud` surface (wave/enemy/status text, core items, toasts) and forwards
## interaction to the Rust facade. Per §3.10 rule 2 it never reads sim state
## directly.

extends Control

## HUD refresh cadence in seconds (change-cached labels; §3.10 rule 6).
const REFRESH_INTERVAL := 0.2
## `HudFragment.makeStatusTable` skip button glyph.
const SKIP_ICON := "play"

var _hud: Node = null
var _accum := 0.0
var _game_over_shown := false

@onready var status_row: PanelContainer = get_node_or_null("WavesStack/WavesMain/StatusRow")
@onready var status_label: RichTextLabel = get_node_or_null("WavesStack/WavesMain/StatusRow/Row/Status")
@onready var info_label: RichTextLabel = get_node_or_null("WavesStack/WavesMain/Info")
@onready var wave_buttons: HBoxContainer = get_node_or_null("WavesStack/WavesMain/WaveButtons")
@onready var fps_label: Label = get_node_or_null("FpsBox/Fps")
@onready var memory_label: Label = get_node_or_null("FpsBox/Memory")
@onready var ping_label: Label = get_node_or_null("FpsBox/Ping")
@onready var core_items: CoreItemsDisplay = get_node_or_null("CoreInfo/CoreItems")
@onready var paused_banner: RichTextLabel = get_node_or_null("PausedBanner")
@onready var waiting_banner: RichTextLabel = get_node_or_null("WaitingBanner")
@onready var hud_text: RichTextLabel = get_node_or_null("CoreInfo/HudText")
@onready var position_label: RichTextLabel = get_node_or_null("MinimapBox/Position")
@onready var minimap_widget: MindMinimapWidget = get_node_or_null("MinimapBox/Minimap")
@onready var core_attack: PanelContainer = get_node_or_null("CoreInfo/CoreAttack")
@onready var core_attack_label: Label = get_node_or_null("CoreInfo/CoreAttack/Label")
@onready var boss_bar: MarginContainer = get_node_or_null("CoreInfo/BossBar")
@onready var boss_mind_bar: MindBar = get_node_or_null("CoreInfo/BossBar/Bar")
@onready var status_icons: HBoxContainer = get_node_or_null("WavesStack/WavesMain/StatusIcons")
@onready var near_point: PanelContainer = get_node_or_null("NearPoint")
@onready var near_point_label: RichTextLabel = get_node_or_null("NearPoint/Label")
@onready var saving_label: RichTextLabel = get_node_or_null("Saving")

var _skip_button: Button = null
var _minimap_last_zoom := 1.0
## Per-core healths keyed `"x,y"` (HUD-side diff for the core-attack banner).
var _core_healths: Dictionary = {}
var _core_attack_time := 0.0
var _core_attack_total := 0.0
var _last_damaged_core := Vector2.ZERO
## Near-point banner alpha (`WaveSpawner.playerNear` lerp).
var _near_alpha := 0.0
## Applied status names signature (icon rebuild cache).
var _status_names := ""
## First-use UI-hidden announce latch.
var _ui_hidden_warned := false
## Whether the 3 s game-over timer is in flight (T16).
var _game_over_pending := false
## Logic `setHud` hide latch (`HudFragment.shown() = shown && !logicHideHud`,
## `HudFragment.java:1239`; H06 T3): 10-frame poll, previous visibility kept.
var _logic_hud_tick := 0
var _logic_hud_hidden := false
var _logic_hud_prev_visible := true


func _ready() -> void:
	_hud = get_node_or_null("/root/MindHud")
	_localize_banners()
	_build_wave_buttons()
	_setup_status_style()
	if _hud != null:
		if _hud.has_signal("hud_text"):
			_hud.connect("hud_text", _on_hud_text)
		if _hud.has_signal("toast"):
			_hud.connect("toast", _on_toast)
		if _hud.has_signal("announce"):
			_hud.connect("announce", _on_announce)
		if _hud.has_signal("unlock"):
			_hud.connect("unlock", _on_unlock)
		if _hud.has_signal("wave_event"):
			_hud.connect("wave_event", _on_wave_event)
		if _hud.has_signal("sector_event"):
			_hud.connect("sector_event", _on_sector_event)
	_connect_minimap()
	# `DesktopInput.java:455-457`: F6 toggles the `blockstatus` setting (the
	# input layer only emits the hotkey; the HUD owns the setting write).
	var input := get_node_or_null("/root/Spine/Input")
	if (
		input != null
		and input.has_signal("hotkey")
		and not input.is_connected("hotkey", Callable(self, "_on_hotkey"))
	):
		input.connect("hotkey", Callable(self, "_on_hotkey"))
	if core_attack != null:
		core_attack.visible = false
		if not core_attack.gui_input.is_connected(_on_core_attack_input):
			core_attack.gui_input.connect(_on_core_attack_input)
	refresh()


## View-toggle hotkeys owned by the HUD fragment (`Renderer.drawStatus`).
func _on_hotkey(action: String) -> void:
	if action == "toggle_block_status":
		var ui := get_node_or_null("/root/MindUi")
		if ui == null or not ui.has_method("settings_get_bool"):
			return
		var current := bool(ui.call("settings_get_bool", "blockstatus"))
		ui.call("settings_set", "blockstatus", not current)
	elif action == "toggle_menus":
		_toggle_menus()


## Local HUD visibility (`HudFragment.shown`, `HudFragment.java:45`); the
## `ui-hidden` setting is only the announce-once gate (`:455-461`), never the
## state.
var _menus_shown := true

## `HudFragment.toggleMenus` (`HudFragment.java:875-877` + `:455-461`): flip
## every HUD table; the first press announces the bind once
## (`Settings.getBoolOnce("ui-hidden")`).
func _toggle_menus() -> void:
	_menus_shown = not _menus_shown
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and not _ui_hidden_warned:
		_ui_hidden_warned = true
		if ui.has_method("announce"):
			var key_display := _bind_display("toggle_menus")
			var assets := get_node_or_null("/root/MindAssets")
			if assets != null and assets.has_method("bundle_get"):
				var fmt := str(assets.call("bundle_get", "showui"))
				ui.call("announce", fmt.replace("{0}", key_display), 5.0)
	# The tables are children of this fragment; `placement` is a sibling
	# fragment under the HUD group.
	for path in ["WavesStack", "CoreInfo", "MinimapBox", "FpsBox"]:
		var node := get_node_or_null(path)
		if node != null:
			node.visible = _menus_shown
	var hud := get_node_or_null("/root/Spine/Ui/UiRoot/HudGroup")
	if hud != null:
		var placement := hud.get_node_or_null("placement")
		if placement != null:
			placement.visible = _menus_shown


## Display name of one binding (`keybinds_json`), for the `showui` announce.
func _bind_display(action: String) -> String:
	var input := get_node_or_null("/root/Spine/Input")
	if input == null or not input.has_method("keybinds_json"):
		return "C"
	var parsed: Variant = JSON.parse_string(str(input.call("keybinds_json")))
	if parsed is Array:
		for entry_variant in parsed as Array:
			if (
				entry_variant is Dictionary
				and str((entry_variant as Dictionary).get("name", "")) == action
			):
				return str((entry_variant as Dictionary).get("display", "C"))
	return "C"


## `HudFragment.java:336-351`: the embedded minimap pans/zooms the camera and
## left-tap opens the fullscreen fragment.
func _connect_minimap() -> void:
	if minimap_widget == null:
		return
	if not minimap_widget.pan_requested.is_connected(_on_minimap_pan):
		minimap_widget.pan_requested.connect(_on_minimap_pan)
	if not minimap_widget.zoom_changed.is_connected(_on_minimap_zoom):
		minimap_widget.zoom_changed.connect(_on_minimap_zoom)
	if not minimap_widget.tapped.is_connected(_on_minimap_tapped):
		minimap_widget.tapped.connect(_on_minimap_tapped)


## `HudFragment.makeStatusTable`: the status table draws on `Tex.wavepane`
## (36x27 ninepatch, splits 12). Built from the atlas region so the objective
## panel matches the Java top-left readout instead of a bare label.
func _setup_status_style() -> void:
	if status_row == null:
		return
	var assets := MindWidgets.assets()
	if assets == null or not assets.has_method("find_region"):
		return
	var texture: Variant = assets.call("find_region", "wavepane")
	if not (texture is Texture2D):
		return
	var box := StyleBoxTexture.new()
	box.texture = texture
	box.texture_margin_left = 12.0
	box.texture_margin_top = 12.0
	box.texture_margin_right = 12.0
	box.texture_margin_bottom = 12.0
	# `table.marginTop(0).marginBottom(4).marginLeft(4)`; the 120x80 player
	# portrait cell sits inside, the status label to its right.
	box.content_margin_left = 4.0
	box.content_margin_top = 0.0
	box.content_margin_right = 8.0
	box.content_margin_bottom = 4.0
	status_row.add_theme_stylebox_override("panel", box)
	if status_label != null:
		status_label.add_theme_stylebox_override("normal", StyleBoxEmpty.new())


func _process(delta: float) -> void:
	_tick_banners(delta)
	_logic_hud_tick += 1
	if _logic_hud_tick >= 10:
		_logic_hud_tick = 0
		_update_logic_hud()
	_accum += delta
	if _accum < REFRESH_INTERVAL:
		return
	_accum = 0.0
	refresh()


## `HudFragment.shown() = shown && !logicHideHud` (`HudFragment.java:1239`):
## a world-processor `setHud false` hides the whole HUD until `setHud true`.
## Reads are 10-frame throttled; the pre-hide visibility is restored, and the
## property is only touched when the flag actually flips.
func _update_logic_hud() -> void:
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("logic_cutscene_json"):
		return
	var parsed: Variant = JSON.parse_string(str(host.call("logic_cutscene_json")))
	if typeof(parsed) != TYPE_DICTIONARY:
		return
	var hide := bool((parsed as Dictionary).get("hideHud", false))
	if hide == _logic_hud_hidden:
		return
	_logic_hud_hidden = hide
	if hide:
		_logic_hud_prev_visible = visible
		visible = false
	else:
		visible = _logic_hud_prev_visible


## Per-frame banner timers (core-attack countdown, near-point fade; Java runs
## both outside the 5 Hz table refresh).
func _tick_banners(delta: float) -> void:
	var paused := _hud != null and bool(_hud.get("paused"))
	if _core_attack_time > 0.0 and not paused:
		_core_attack_time = maxf(0.0, _core_attack_time - delta * 60.0)
	if core_attack != null:
		core_attack.visible = _core_attack_time > 0.0
		if core_attack.visible and core_attack_label != null:
			# `orange.lerp(scarlet, absin(Time.time, 2, 1))` (`Java absin`).
			var t := Time.get_ticks_msec() * 0.06
			var f := (sin(t / 4.0) + 1.0) * 0.5
			core_attack_label.add_theme_color_override(
				"font_color", Color("ffa500").lerp(Color("ff2400"), f)
			)
	# `table.alpha` lerp toward `playerNear() ? 1 : 0` by 0.1/tick.
	var near := false
	var host := get_node_or_null("/root/Spine/SimHost")
	if host != null and host.has_method("player_near_drop_zone"):
		near = bool(host.call("player_near_drop_zone"))
	var target := 1.0 if near else 0.0
	_near_alpha += (target - _near_alpha) * (1.0 - pow(0.9, delta * 60.0))
	if near_point != null:
		near_point.visible = _near_alpha > 0.005
		near_point.modulate.a = _near_alpha


## Banner texts from the bundle (`@paused`/`@waiting.players`), replacing the
## hardcoded English defaults in the scene.
func _localize_banners() -> void:
	if paused_banner != null:
		paused_banner.text = "[center]" + MindWidgets.markup("@paused")
	if waiting_banner != null:
		waiting_banner.text = "[center]" + MindWidgets.markup("@waiting.players")
	if core_attack_label != null:
		core_attack_label.text = MindWidgets.markup("@coreattack")
	if near_point_label != null:
		near_point_label.text = "[center]" + MindWidgets.markup("@nearpoint")
	if saving_label != null:
		saving_label.text = "[center]" + MindWidgets.markup("@saving")


## `HudFragment.java:491-510`: the icon-only skip-wave button lives inside the
## status table's row, right side, 40 px wide with `growY`
## (`s.button(Icon.play, rightStyle, 30f, ...).growY().fillX().right().width(40f)`).
## code-instantiated: the button's icon-font glyph is resolved through
## `MindIcons` and has no scene form.
func _build_wave_buttons() -> void:
	if _skip_button != null:
		return
	var row := get_node_or_null("WavesStack/WavesMain/StatusRow/Row")
	if row == null:
		return
	_skip_button = MindWidgets.icon_button(SKIP_ICON, "")
	_skip_button.name = "skip"
	_skip_button.custom_minimum_size = Vector2(40.0, 0.0)
	_skip_button.size_flags_vertical = Control.SIZE_FILL
	_skip_button.pressed.connect(_on_skip_wave)
	row.add_child(_skip_button)


## Pulls the typed HUD properties and drives the widgets that are not simple
## label binds (core items, skip button, game-over watcher).
func refresh() -> void:
	if _hud == null:
		_set_placeholder()
		return
	var status := str(_hud.get("status_text"))
	if status_label != null:
		# Java's `makeStatusTable` labelWrap cell is centred in the wavepane table
		# (HudFragment: `table.labelWrap(...)`), not left-aligned.
		status_label.text = "[center]" + status
		status_label.visible = not status.is_empty()
	if info_label != null:
		info_label.visible = false
	if fps_label != null:
		fps_label.text = "FPS: %d" % int(_hud.get("fps"))
	if memory_label != null:
		memory_label.text = "%d MB" % int(_hud.get("memory_mb"))
	if ping_label != null:
		ping_label.text = "%d ms" % int(_hud.get("ping"))
	if paused_banner != null:
		paused_banner.visible = bool(_hud.get("paused"))
		var paused_key := str(_hud.get("paused_text"))
		if paused_key.is_empty():
			paused_key = "@paused"
		paused_banner.text = "[center]" + MindWidgets.markup(paused_key)
	if waiting_banner != null:
		waiting_banner.visible = bool(_hud.get("waiting"))
	if position_label != null:
		var position := str(_hud.get("position_text"))
		position_label.text = "[center]" + position
		position_label.visible = not position.is_empty()
	_refresh_core_items()
	_refresh_skip_button()
	_refresh_boss()
	_refresh_core_attack()
	_refresh_status_icons()
	_refresh_saving()
	_refresh_loading()
	_check_game_over()


func _set_placeholder() -> void:
	if paused_banner != null:
		paused_banner.visible = false
	if waiting_banner != null:
		waiting_banner.visible = false
	if _skip_button != null:
		_skip_button.visible = false


## `HudFragment.canSkipWave` (client half): waves enabled, wave sending on,
## no enemies left, not mid-spawn (`state.rules.waveSending` is false in the
## tutorial map, so the skip stays hidden there - PAR-0030).
func _refresh_skip_button() -> void:
	if _skip_button == null:
		return
	var waves := bool(_hud.get("waves"))
	var sending := bool(_hud.get("wave_sending"))
	var spawning := bool(_hud.get("is_spawning"))
	var enemies := int(_hud.get("enemies"))
	_skip_button.visible = (
		waves and sending and not spawning and enemies <= 0 and bool(_hud.get("has_core"))
	)


## `Logic.skipWave` via the existing campaign facade (`run_wave`).
func _on_skip_wave() -> void:
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign != null and campaign.has_method("run_wave"):
		campaign.call("run_wave")
	refresh()


## `HudFragment` core-items table fed from the live sim core inventory.
func _refresh_core_items() -> void:
	if core_items == null or _hud == null:
		return
	var parsed: Variant = JSON.parse_string(str(_hud.call("core_items_json")))
	if parsed is Dictionary:
		core_items.update_items(parsed)


## `LoadingFragment` driver: shows the overlay while the Rust IO queue has
## pending save/load work (`MindSimHost.io_pending`) and hides it after.
func _refresh_loading() -> void:
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("io_pending"):
		return
	var pending := int(host.call("io_pending"))
	var saving := host.has_method("io_saving") and bool(host.call("io_saving"))
	var loading := get_node_or_null("/root/Spine/Ui/UiRoot/LoadingLayer/loading")
	if loading == null:
		return
	var shown := bool(loading.call("is_shown")) if loading.has_method("is_shown") else false
	# Saves show the small bottom `@saving` label, loads the full overlay
	# (`HudFragment.java:668-673`; previously any IO covered the screen).
	if pending > 0 and not saving and not shown:
		loading.call("set_progress", 0.0, "")
	elif (pending == 0 or saving) and shown:
		loading.call("hide_loading")


## Opens the game-over dialog on the campaign loss (`gameOver`) or sector
## capture edge; the dialog owns the continue/menu routing.
func _check_game_over() -> void:
	if _hud == null:
		return
	var over := bool(_hud.get("game_over"))
	# T16: `Logic.java:435-440` opens the dialog 3 s after `GameOverEvent`
	# (`Time.run(60 * 3, () -> ui.restart.show(winner))`); captures show the
	# `@sector.capture.current` announce instead (T9).
	if over:
		if not _game_over_shown:
			_game_over_shown = true
			_schedule_game_over()
	else:
		_game_over_shown = false
		_game_over_pending = false


## 3 s timer before the restart dialog (`Time.run(60 * 3, ...)`); cancelled
## when `gameOver` flips back.
func _schedule_game_over() -> void:
	_game_over_pending = true
	get_tree().create_timer(3.0).timeout.connect(_open_game_over_now)


func _open_game_over_now() -> void:
	if _hud == null or not bool(_hud.get("game_over")):
		_game_over_pending = false
		return
	_game_over_pending = false
	_open_game_over(false)


func _open_game_over(_captured: bool) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("open_dialog"):
		return
	var stats: Variant = {}
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign != null and campaign.has_method("game_stats_json"):
		stats = JSON.parse_string(str(campaign.call("game_stats_json")))
	var context := {
		"winner": str(_hud.get("sector_name")),
		"campaign": bool(_hud.get("campaign")),
		"sector": str(_hud.get("sector_name")),
		"stats": stats if stats is Dictionary else {},
	}
	ui.call("open_dialog", "restart", JSON.stringify(context))


func _on_hud_text(text: String) -> void:
	# code-instantiated: server-sent HUD text is data-driven (`setHudText`).
	if hud_text != null:
		hud_text.text = "[center]" + text
		hud_text.visible = not text.is_empty()


func _on_toast(text: String, icon: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("toast", text, icon)


func _on_announce(text: String, duration: float) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("announce", text, duration)


func _on_unlock(_name: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	# H03 T17: pass the raw bundle text; the toast label translates markup.
	var text := "@unlocked"
	var assets := get_node_or_null("/root/MindAssets")
	if assets != null and assets.has_method("bundle_get"):
		text = str(assets.call("bundle_get", "unlocked"))
	ui.call("toast", text, "lock-open")


func _on_wave_event() -> void:
	refresh()


func _on_sector_event(_kind: String, _name: String) -> void:
	refresh()


func _on_minimap_pan(world_delta: Vector2) -> void:
	var camera := get_node_or_null("/root/Spine/World/Camera2D")
	if camera == null or not camera.has_method("pan_to"):
		return
	var position: Vector2 = camera.get("position")
	camera.call("pan_to", position.x + world_delta.x, position.y + world_delta.y)


## `Renderer.scaleCamera` step from the widget's absolute zoom ratio.
func _on_minimap_zoom(factor: float) -> void:
	var camera := get_node_or_null("/root/Spine/World/Camera2D")
	if camera == null or not camera.has_method("zoom_by"):
		return
	var step := (factor / maxf(_minimap_last_zoom, 0.0001) - 1.0) * 4.0
	_minimap_last_zoom = factor
	if absf(step) > 0.0001:
		camera.call("zoom_by", step)


## `HudFragment` boss bar (`:633-656`): visible while `rules.waves` and the
## wave team has bosses; text = up to 6 boss emojis + `@guardian`, colour
## `Pal.health` (`ff341c`), value = sum(health)/sum(max).
func _refresh_boss() -> void:
	if boss_bar == null:
		return
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("boss_json"):
		boss_bar.visible = false
		return
	var parsed: Variant = JSON.parse_string(str(host.call("boss_json")))
	if not (parsed is Dictionary):
		boss_bar.visible = false
		return
	var state: Dictionary = parsed
	var count := int(state.get("count", 0))
	boss_bar.visible = bool(_hud.get("waves")) and count > 0
	if not boss_bar.visible:
		return
	if boss_mind_bar != null:
		var names: Array = state.get("names", [])
		var prefix := ""
		if count > 6:
			prefix = "[accent]+[] "
		var text := prefix + str(MindWidgets.markup("@guardian"))
		boss_mind_bar.setup(
			text, Color("ff341c"), clampf(float(state.get("value", 0.0)), 0.0, 1.0)
		)


## Core-attack banner (`HudFragment.java:596-631`): a health drop between
## refreshes shows the flashing `< Core is under attack! >` for 240 ticks; the
## tap pans to the damaged core.
func _refresh_core_attack() -> void:
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("player_core_healths_json"):
		return
	var parsed: Variant = JSON.parse_string(str(host.call("player_core_healths_json")))
	if not (parsed is Array):
		return
	var next: Dictionary = {}
	var total := 0.0
	for entry_variant in parsed as Array:
		if not (entry_variant is Dictionary):
			continue
		var entry: Dictionary = entry_variant
		var key := "%s,%s" % [str(entry.get("x", 0.0)), str(entry.get("y", 0.0))]
		var health := float(entry.get("health", 0.0))
		next[key] = health
		total += health
		if _core_healths.has(key) and health < float(_core_healths[key]) - 0.001:
			_last_damaged_core = Vector2(float(entry.get("x", 0.0)), float(entry.get("y", 0.0)))
			_core_attack_time = 240.0
	if total < _core_attack_total - 0.001 and not _core_healths.is_empty():
		_core_attack_time = 240.0
	_core_attack_total = total
	_core_healths = next


## Tapping the core-attack banner pans the camera to the damaged core
## (`HudFragment.java:596-631`).
func _on_core_attack_input(event: InputEvent) -> void:
	if not (event is InputEventMouseButton and event.pressed):
		return
	if event.button_index != MOUSE_BUTTON_LEFT:
		return
	var camera := get_node_or_null("/root/Spine/World/Camera2D")
	if camera != null and camera.has_method("pan_to"):
		camera.call("pan_to", _last_damaged_core.x, _last_damaged_core.y)


## `HudFragment.addInfoTable` (`:1205-1231`): one 32x32 icon per applied
## status effect with its localized name.
func _refresh_status_icons() -> void:
	if status_icons == null:
		return
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("player_unit_status"):
		return
	var status: Variant = host.call("player_unit_status")
	var list: Variant = null
	if status is Dictionary:
		list = JSON.parse_string(str((status as Dictionary).get("statuses", "[]")))
	if not (list is Array):
		list = []
	var names: Array[String] = []
	for entry in list as Array:
		if entry is Dictionary:
			names.append(str((entry as Dictionary).get("name", "")))
	var signature := ",".join(names)
	if signature == _status_names:
		return
	_status_names = signature
	for child in status_icons.get_children():
		child.queue_free()
	for name in names:
		if name.is_empty():
			continue
		# code-instantiated: one icon per applied status effect (T9).
		var icon := TextureRect.new()
		icon.custom_minimum_size = Vector2(32, 32)
		icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		icon.texture = MindWidgets.icon_texture("status-%s-ui" % name)
		if icon.texture == null:
			icon.texture = MindWidgets.icon_texture("status-%s" % name)
		icon.tooltip_text = MindWidgets.markup("@status.%s.name" % name)
		status_icons.add_child(icon)


## `@saving` label while a save is queued (`control.saves.isSaving()`).
func _refresh_saving() -> void:
	if saving_label == null:
		return
	var host := get_node_or_null("/root/Spine/SimHost")
	saving_label.visible = (
		host != null and host.has_method("io_saving") and bool(host.call("io_saving"))
	)


## `Minimap.clicked` -> `ui.minimapfrag.toggle()`.
func _on_minimap_tapped() -> void:
	var fragment := get_node_or_null("/root/Spine/Ui/UiRoot/HudGroup/minimap")
	if fragment != null and fragment.has_method("toggle"):
		fragment.call("toggle")
