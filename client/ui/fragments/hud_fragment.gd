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

@onready var status_label: RichTextLabel = get_node_or_null("WavesStack/WavesMain/Status")
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

var _skip_button: Button = null
var _minimap_last_zoom := 1.0


func _ready() -> void:
	_hud = get_node_or_null("/root/MindHud")
	_localize_banners()
	_build_wave_buttons()
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
	refresh()


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


func _process(delta: float) -> void:
	_accum += delta
	if _accum < REFRESH_INTERVAL:
		return
	_accum = 0.0
	refresh()


## Banner texts from the bundle (`@paused`/`@waiting.players`), replacing the
## hardcoded English defaults in the scene.
func _localize_banners() -> void:
	if paused_banner != null:
		paused_banner.text = "[center]" + MindWidgets.markup("@paused")
	if waiting_banner != null:
		waiting_banner.text = "[center]" + MindWidgets.markup("@waiting.players")


## `HudFragment.java:502-510`: icon-only skip-wave button next to the status
## table. code-instantiated: the button's icon-font glyph is resolved through
## `MindIcons` and has no scene form.
func _build_wave_buttons() -> void:
	if wave_buttons == null or _skip_button != null:
		return
	_skip_button = MindWidgets.icon_button(SKIP_ICON, "")
	_skip_button.name = "skip"
	_skip_button.pressed.connect(_on_skip_wave)
	wave_buttons.add_child(_skip_button)


## Pulls the typed HUD properties and drives the widgets that are not simple
## label binds (core items, skip button, game-over watcher).
func refresh() -> void:
	if _hud == null:
		_set_placeholder()
		return
	var status := str(_hud.get("status_text"))
	if status_label != null:
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
	if waiting_banner != null:
		waiting_banner.visible = bool(_hud.get("waiting"))
	if position_label != null:
		var position := str(_hud.get("position_text"))
		position_label.text = "[right]" + position
		position_label.visible = not position.is_empty()
	_refresh_core_items()
	_refresh_skip_button()
	_refresh_loading()
	_check_game_over()


func _set_placeholder() -> void:
	if paused_banner != null:
		paused_banner.visible = false
	if waiting_banner != null:
		waiting_banner.visible = false
	if _skip_button != null:
		_skip_button.visible = false


## `HudFragment.canSkipWave` (client half): waves enabled and no enemies left.
func _refresh_skip_button() -> void:
	if _skip_button == null:
		return
	var waves := bool(_hud.get("waves"))
	var enemies := int(_hud.get("enemies"))
	_skip_button.visible = waves and enemies <= 0 and bool(_hud.get("has_core"))


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
	var loading := get_node_or_null("/root/Spine/Ui/UiRoot/LoadingLayer/loading")
	if loading == null:
		return
	var shown := bool(loading.call("is_shown")) if loading.has_method("is_shown") else false
	if pending > 0 and not shown:
		loading.call("set_progress", 0.0, "")
	elif pending == 0 and shown:
		loading.call("hide_loading")


## Opens the game-over dialog on the campaign loss (`gameOver`) or sector
## capture edge; the dialog owns the continue/menu routing.
func _check_game_over() -> void:
	if _hud == null:
		return
	var campaign := bool(_hud.get("campaign"))
	var captured := campaign and bool(_hud.get("has_core")) and bool(_hud.get("was_captured"))
	var over := bool(_hud.get("game_over"))
	if over or captured:
		if not _game_over_shown:
			_game_over_shown = true
			_open_game_over(captured)
	elif not captured:
		_game_over_shown = false


func _open_game_over(captured: bool) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("open_dialog"):
		return
	var context := {
		"winner": str(_hud.get("sector_name")),
		"campaign": bool(_hud.get("campaign")),
		"captured": captured,
		"sector": str(_hud.get("sector_name")),
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
	if ui != null:
		ui.call("toast", MindWidgets.markup("@unlocked"), "lock-open")


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


## `Minimap.clicked` -> `ui.minimapfrag.toggle()`.
func _on_minimap_tapped() -> void:
	var fragment := get_node_or_null("/root/Spine/Ui/UiRoot/HudGroup/minimap")
	if fragment != null and fragment.has_method("toggle"):
		fragment.call("toggle")
