## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/CampaignRulesDialog.java
##         (plan 14 §3.4, M3).
##
## Difficulty + campaign rule toggles. Values come from the live
## `MindCampaign.get_campaign_rules(planet)` (WS2 contract) with the
## `campaign_views().rules` read model as fallback; every change is emitted as
## `rule_changed` and written through `set_campaign_rule(planet, key, value)`.

extends MindDialog

signal rule_changed(key: String, value: Variant)

## Vanilla toggle keys in `CampaignRulesDialog.check` order.
const RULE_KEYS := [
	"sector_invasion",
	"fog",
	"hide_spawns",
	"random_wave_ai",
	"pause_disabled",
	"rts_ai",
	"clear_sector_on_lose",
]
## Rule key -> bundle label; unknown keys fall back to the key itself.
const RULE_LABELS := {
	"fog": "@rules.fog",
	"hide_spawns": "@rules.hidespawns",
	"sector_invasion": "@rules.invasions",
	"random_wave_ai": "@rules.randomwaveai",
	"rts_ai": "@rules.rtsai.campaign",
	"pause_disabled": "@rules.pauseDisabled",
	"clear_sector_on_lose": "@rules.clearsectoronloss",
}

var _table: MindTable = null
var _planet := ""
var _difficulty := "normal"
var _toggles: Dictionary = {}


func _ready() -> void:
	set_title_key("@campaign.difficulty")
	should_pause = false
	super._ready()
	_table = content_table()
	add_close_button()


func shown() -> void:
	_planet = str(_context.get("planet", campaign_views().get("planet", "")))
	refresh_campaign_views(_planet)
	var rules: Dictionary = campaign_views().get("rules", {})
	# Live per-planet rules win over the read model's fixture values.
	var live := campaign_json("get_campaign_rules", [_planet])
	if not live.is_empty():
		rules = live
	_difficulty = str(rules.get("difficulty", "normal"))
	_toggles = {}
	for toggle_variant in rules.get("toggles", []):
		var toggle: Dictionary = toggle_variant
		_toggles[str(toggle.get("key", ""))] = bool(toggle.get("value", false))
	if _toggles.is_empty():
		for key in RULE_KEYS:
			_toggles[key] = false
	_rebuild()


func _rebuild() -> void:
	if _table == null:
		return
	_table.clear_children()
	# code-instantiated: difficulty buttons are the data-driven Difficulty list.
	var diff_row := HBoxContainer.new()
	for name in ["casual", "easy", "normal", "hard", "eradication"]:
		var button := Button.new()
		button.text = _t("@difficulty.%s" % name)
		button.theme_type_variation = "flatTogglet"
		button.toggle_mode = true
		button.button_pressed = name == _difficulty
		button.tooltip_text = _difficulty_tooltip(name)
		button.pressed.connect(_set_difficulty.bind(name))
		diff_row.add_child(button)
	_table.add(diff_row).grow_x_axis().pad(4)
	_table.row()
	for key in RULE_KEYS:
		# code-instantiated: rule toggles come from the plan-12 CampaignRules set.
		var label := str(RULE_LABELS.get(key, key))
		var check := MindWidgets.check(_t(label), bool(_toggles.get(key, false)), _toggle.bind(key))
		_table.add(check).grow_x_axis().pad(2).left()
		_table.row()


## Localized `Difficulty.info()` tooltip from the multiplier table.
func _difficulty_tooltip(name: String) -> String:
	var modifiers := {
		"casual": [0.5, 0.5, 2.0],
		"easy": [1.0, 0.75, 1.5],
		"normal": [1.0, 1.0, 1.0],
		"hard": [1.25, 1.5, 0.8],
		"eradication": [1.5, 2.0, 0.6],
	}
	var values: Array = modifiers.get(name, [1.0, 1.0, 1.0])
	var lines := PackedStringArray()
	var health := _percent_stat(float(values[0]), false)
	var spawn := _percent_stat(float(values[1]), false)
	var wave := _percent_stat(float(values[2]), true)
	if not health.is_empty():
		lines.append(MindWidgets.markup_format("@difficulty.enemyHealthMultiplier", [health]))
	if not spawn.is_empty():
		lines.append(MindWidgets.markup_format("@difficulty.enemySpawnMultiplier", [spawn]))
	if not wave.is_empty():
		lines.append(MindWidgets.markup_format("@difficulty.waveTimeMultiplier", [wave]))
	return "\n".join(lines) if not lines.is_empty() else _t("@difficulty.nomodifiers")


## `Difficulty.percentStat`/`percentStatNeg` colored delta ("" at 100%).
func _percent_stat(value: float, negated: bool) -> String:
	var delta := int(value * 100.0 - 100.0)
	if delta == 0:
		return ""
	var tag := "[negstat]" if delta > 0 else "[stat]"
	if negated:
		tag = "[stat]" if delta > 0 else "[negstat]"
	return "%s%+d%%[]" % [tag, delta]


func _set_difficulty(name: String) -> void:
	_difficulty = name
	_apply("difficulty", name)
	_rebuild()


func _toggle(value: bool, key: String) -> void:
	_toggles[key] = value
	_apply(key, value)


## Emits the change for listeners and writes it through the live facade.
func _apply(key: String, value: Variant) -> void:
	rule_changed.emit(key, value)
	var campaign := campaign_node()
	if campaign != null and campaign.has_method("set_campaign_rule"):
		campaign.callv("set_campaign_rule", [_planet, key, value])
