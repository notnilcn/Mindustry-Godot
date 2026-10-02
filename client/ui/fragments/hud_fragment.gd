## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/fragments/HudFragment.java (plan 14 §3.5, M4).
##
## In-game HUD. The named regions from §3.5 are declared statically in
## `hud_fragment.tscn` (tscn-first); this script binds their labels to the
## read-only `MindHud` surface once it exists and otherwise leaves placeholder
## text. Per §3.10 rule 2 it never reads sim state directly.

extends Control

## HUD refresh cadence in seconds (change-cached labels; §3.10 rule 6).
const REFRESH_INTERVAL := 0.2

var _hud: Node = null
var _accum := 0.0

@onready var status_label: Label = get_node_or_null("WavesStack/WavesMain/Status")
@onready var fps_label: Label = get_node_or_null("FpsBox/Fps")
@onready var memory_label: Label = get_node_or_null("FpsBox/Memory")
@onready var ping_label: Label = get_node_or_null("FpsBox/Ping")
@onready var paused_banner: Label = get_node_or_null("PausedBanner")
@onready var waiting_banner: Label = get_node_or_null("WaitingBanner")
@onready var hud_text: Label = get_node_or_null("CoreInfo/HudText")
@onready var position_label: Label = get_node_or_null("MinimapBox/Position")


func _ready() -> void:
	_hud = get_node_or_null("/root/MindHud")
	if _hud != null and _hud.has_signal("hud_text"):
		_hud.connect("hud_text", _on_hud_text)
	if _hud != null and _hud.has_signal("toast"):
		_hud.connect("toast", _on_toast)
	refresh()


func _process(delta: float) -> void:
	_accum += delta
	if _accum < REFRESH_INTERVAL:
		return
	_accum = 0.0
	refresh()


## Pulls the typed HUD properties (no-op placeholders until `MindHud` lands).
func refresh() -> void:
	if _hud == null:
		_set_placeholder()
		return
	if status_label != null:
		status_label.text = str(_hud.get("status_text"))
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
		position_label.text = str(_hud.get("position_text"))


func _set_placeholder() -> void:
	if paused_banner != null:
		paused_banner.visible = false
	if waiting_banner != null:
		waiting_banner.visible = false


func _on_hud_text(text: String) -> void:
	# code-instantiated: server-sent HUD text is data-driven (`setHudText`).
	if hud_text != null:
		hud_text.text = text
		hud_text.visible = not text.is_empty()


func _on_toast(text: String, icon: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("toast", text, icon)
