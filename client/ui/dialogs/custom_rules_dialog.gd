## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/CustomRulesDialog.java
##
## The full custom-rules editor: the category list (waves, resourcesbuilding,
## unit, enemy, environment, light, music, planet, teams), the `check`/`number`/
## `numberi`/`text`/`team` row builders, the `@edit` clipboard menu, the search
## filter and the hide-time `rules_edited` hand-off to `control.playMap`.
## Categories are transcribed in `setupMain()` order; each row keeps a live
## condition (`update(a -> a.setDisabled(!condition))` upstream).

extends MindDialog

## Emitted when the dialog hides: the edited rules JSON (`Rules.set(rules)`).
signal rules_edited(json: String)

## `Vars.tilesize` (world pixels per tile).
const TILESIZE := 8.0
## `CustomRulesDialog.setupMain` category order.
const CATEGORIES := [
	"waves",
	"resourcesbuilding",
	"unit",
	"enemy",
	"environment",
	"light",
	"music",
	"planet",
	"teams",
]
## Category accent rule height (`addToMain`, 3 px).
const RULE_HEIGHT := 3.0

var _rules: Dictionary = {}
var _reset: Dictionary = {}
var _show_rule_edit := false
var _search := ""
var _main: MindTable = null
var _current: MindTable = null
var _categories: Array[Dictionary] = []
var _rows: Array[Dictionary] = []
var _team_rows: Array[Dictionary] = []
var _erekir_env := 17
var _overlay: Control = null


func _ready() -> void:
	set_title_key("@mode.custom")
	should_pause = false
	full_dialog = true
	super._ready()
	add_close_button()
	add_button(_t("@edit"), _open_edit_menu, "pencil", 210.0)


func set_context_json(json_text: String) -> void:
	super.set_context_json(json_text)
	var ctx := context()
	var rules: Variant = ctx.get("rules", {})
	_rules = rules if rules is Dictionary else {}
	var reset: Variant = ctx.get("reset", {})
	_reset = reset if reset is Dictionary else {}
	_show_rule_edit = bool(ctx.get("show_rule_edit", false))


## `shown(this::setup)`: rebuild on every open (the rules may be a new object).
func shown() -> void:
	_setup()


## `rules.hidden(...)` (`PausedDialog`/`MapPlayDialog` read the edited rules when
## the dialog closes).
func hidden() -> void:
	if _rules.is_empty():
		return
	rules_edited.emit(JSON.stringify(_rules))


## `setup()`: the `players.search` filter row plus the category body.
func _setup() -> void:
	if cont != null:
		for child in cont.get_children():
			cont.remove_child(child)
			child.queue_free()
	var root := content_table()
	# code-instantiated: the search row mirrors `Icon.zoom` + field.
	var search_row := HBoxContainer.new()
	search_row.add_theme_constant_override("separation", 8)
	search_row.add_child(MindWidgets.glyph("zoom", 20))
	var field := MindWidgets.field(_t("@players.search"))
	field.text = _search
	field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	field.text_changed.connect(_on_search)
	search_row.add_child(field)
	# `field.setCursorPosition(ruleSearch.length()); Core.scene
	# .setKeyboardFocus(field)` (`CustomRulesDialog.java:130-132`).
	field.caret_column = _search.length()
	field.grab_focus.call_deferred()
	# `cont.table(...).fillX()` above `cont.pane(main)` (`CustomRulesDialog.java:
	# 126-134`): the column is as wide as the rule table's natural width, centred.
	root.add(search_row).fill_x_axis().pad(8)
	root.row()
	# code-instantiated: the rule table is data-driven from the Rules model.
	_main = MindTable.new()
	_main.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	root.add(_main).fill_x_axis()
	_setup_main()


## `setupMain`: every category and row, in Java order.
func _setup_main() -> void:
	_categories.clear()
	_rows.clear()
	_team_rows.clear()
	if _main == null:
		return
	_main.clear_children()
	_main.row()
	_erekir_env = _planet_env("erekir")

	_category("waves")
	_check("@rules.waves", func(v: bool) -> void: _rules["waves"] = v, func() -> bool: return bool(_rules.get("waves", false)))
	_check(
		"@rules.wavesending",
		func(v: bool) -> void: _rules["waveSending"] = v,
		func() -> bool: return bool(_rules.get("waveSending", true)),
		func() -> bool: return bool(_rules.get("waves", false))
	)
	_check(
		"@rules.wavetimer",
		func(v: bool) -> void: _rules["waveTimer"] = v,
		func() -> bool: return bool(_rules.get("waveTimer", true)),
		func() -> bool: return bool(_rules.get("waves", false))
	)
	_check(
		"@rules.waitForWaveToEnd",
		func(v: bool) -> void: _rules["waitEnemies"] = v,
		func() -> bool: return bool(_rules.get("waitEnemies", false)),
		func() -> bool:
			return bool(_rules.get("waves", false)) and bool(_rules.get("waveTimer", true))
	)
	_check(
		"@rules.randomwaveai",
		func(v: bool) -> void: _rules["randomWaveAi"] = v,
		func() -> bool: return bool(_rules.get("randomWaveAi", false)),
		func() -> bool: return bool(_rules.get("waves", false))
	)
	_check(
		"@rules.wavespawnatcores",
		func(v: bool) -> void: _rules["wavesSpawnAtCores"] = v,
		func() -> bool: return bool(_rules.get("wavesSpawnAtCores", true)),
		func() -> bool: return bool(_rules.get("waves", false))
	)
	_check(
		"@rules.airUseSpawns",
		func(v: bool) -> void: _rules["airUseSpawns"] = v,
		func() -> bool: return bool(_rules.get("airUseSpawns", false)),
		func() -> bool: return bool(_rules.get("waves", false))
	)
	_numberi(
		"@rules.wavelimit",
		func(v: int) -> void: _rules["winWave"] = float(v),
		func() -> int: return int(_rules.get("winWave", 0)),
		func() -> bool: return bool(_rules.get("waves", false)),
		0,
		INF
	)
	_number(
		"@rules.wavespacing",
		false,
		func(v: float) -> void: _rules["waveSpacing"] = v * 60.0,
		func() -> float: return float(_rules.get("waveSpacing", 7200.0)) / 60.0,
		func() -> bool:
			return bool(_rules.get("waves", false)) and bool(_rules.get("waveTimer", true)),
		1.0,
		INF
	)
	_number(
		"@rules.initialwavespacing",
		false,
		func(v: float) -> void: _rules["initialWaveSpacing"] = v * 60.0,
		func() -> float: return float(_rules.get("initialWaveSpacing", 0.0)) / 60.0,
		func() -> bool:
			return bool(_rules.get("waves", false)) and bool(_rules.get("waveTimer", true)),
		0.0,
		INF
	)
	_number(
		"@rules.dropzoneradius",
		false,
		func(v: float) -> void: _rules["dropZoneRadius"] = v * TILESIZE,
		func() -> float: return float(_rules.get("dropZoneRadius", 300.0)) / TILESIZE,
		func() -> bool: return bool(_rules.get("waves", false))
	)

	_category("resourcesbuilding")
	_check(
		"@rules.alloweditworldprocessors",
		func(v: bool) -> void: _rules["allowEditWorldProcessors"] = v,
		func() -> bool: return bool(_rules.get("allowEditWorldProcessors", false))
	)
	_check(
		"@rules.infiniteresources",
		func(v: bool) -> void: _rules["infiniteResources"] = v,
		func() -> bool: return bool(_rules.get("infiniteResources", false))
	)
	_check(
		"@rules.corebuildandconfig",
		func(v: bool) -> void: _rules["coreBuildAndConfig"] = v,
		func() -> bool: return bool(_rules.get("coreBuildAndConfig", false))
	)
	_check(
		"@rules.onlydepositcore",
		func(v: bool) -> void: _rules["onlyDepositCore"] = v,
		func() -> bool: return bool(_rules.get("onlyDepositCore", false))
	)
	_check(
		"@rules.coreunloaders",
		func(v: bool) -> void: _rules["allowCoreUnloaders"] = v,
		func() -> bool: return bool(_rules.get("allowCoreUnloaders", true))
	)
	_check(
		"@rules.derelictrepair",
		func(v: bool) -> void: _rules["derelictRepair"] = v,
		func() -> bool: return bool(_rules.get("derelictRepair", true))
	)
	_check(
		"@rules.reactorexplosions",
		func(v: bool) -> void: _rules["reactorExplosions"] = v,
		func() -> bool: return bool(_rules.get("reactorExplosions", true))
	)
	_check(
		"@rules.schematic",
		func(v: bool) -> void: _rules["schematicsAllowed"] = v,
		func() -> bool: return bool(_rules.get("schematicsAllowed", true))
	)
	_check(
		"@rules.coreincinerates",
		func(v: bool) -> void: _rules["coreIncinerates"] = v,
		func() -> bool: return bool(_rules.get("coreIncinerates", true))
	)
	_check(
		"@rules.cleanupdeadteams",
		func(v: bool) -> void: _rules["cleanupDeadTeams"] = v,
		func() -> bool: return bool(_rules.get("cleanupDeadTeams", true)),
		func() -> bool: return bool(_rules.get("pvp", false))
	)
	_check(
		"@rules.disableworldprocessors",
		func(v: bool) -> void: _rules["disableWorldProcessors"] = v,
		func() -> bool: return bool(_rules.get("disableWorldProcessors", false))
	)
	_number(
		"@rules.buildcostmultiplier",
		false,
		func(v: float) -> void: _rules["buildCostMultiplier"] = v,
		func() -> float: return float(_rules.get("buildCostMultiplier", 1.0)),
		func() -> bool: return not bool(_rules.get("infiniteResources", false))
	)
	_number(
		"@rules.buildspeedmultiplier",
		false,
		func(v: float) -> void: _rules["buildSpeedMultiplier"] = v,
		func() -> float: return float(_rules.get("buildSpeedMultiplier", 1.0)),
		Callable(),
		0.001,
		50.0
	)
	_number(
		"@rules.deconstructrefundmultiplier",
		false,
		func(v: float) -> void: _rules["deconstructRefundMultiplier"] = v,
		func() -> float: return float(_rules.get("deconstructRefundMultiplier", 0.5)),
		func() -> bool: return not bool(_rules.get("infiniteResources", false)),
		0.0,
		1.0
	)
	_number(
		"@rules.blockhealthmultiplier",
		false,
		func(v: float) -> void: _rules["blockHealthMultiplier"] = v,
		func() -> float: return float(_rules.get("blockHealthMultiplier", 1.0))
	)
	_number(
		"@rules.blockdamagemultiplier",
		false,
		func(v: float) -> void: _rules["blockDamageMultiplier"] = v,
		func() -> float: return float(_rules.get("blockDamageMultiplier", 1.0))
	)
	if _matches("@configure"):
		_current.add(_wide_button("@configure", "boxSmall", _open_loadout)).left()
		_current.row()
	if _matches("@bannedblocks"):
		_current.add(_wide_button("@bannedblocks", "cancelSmall", _open_banned_blocks)).left()
		_current.row()
	_check(
		"@rules.hidebannedblocks",
		func(v: bool) -> void: _rules["hideBannedBlocks"] = v,
		func() -> bool: return bool(_rules.get("hideBannedBlocks", false))
	)
	_check(
		"@bannedblocks.whitelist",
		func(v: bool) -> void: _rules["blockWhitelist"] = v,
		func() -> bool: return bool(_rules.get("blockWhitelist", false))
	)

	_category("unit")
	_check(
		"@rules.unitcapvariable",
		func(v: bool) -> void: _rules["unitCapVariable"] = v,
		func() -> bool: return bool(_rules.get("unitCapVariable", true))
	)
	_check(
		"@rules.unitpayloadsexplode",
		func(v: bool) -> void: _rules["unitPayloadsExplode"] = v,
		func() -> bool: return bool(_rules.get("unitPayloadsExplode", false))
	)
	_numberi(
		"@rules.unitcap",
		func(v: int) -> void: _rules["unitCap"] = float(v),
		func() -> int: return int(_rules.get("unitCap", 0)),
		Callable(),
		-999,
		999
	)
	_number(
		"@rules.unitfactoryactivation",
		false,
		func(v: float) -> void: _rules["unitFactoryActivationDelay"] = v * 60.0,
		func() -> float: return float(_rules.get("unitFactoryActivationDelay", 0.0)) / 60.0
	)
	_number(
		"@rules.unitdamagemultiplier",
		false,
		func(v: float) -> void: _rules["unitDamageMultiplier"] = v,
		func() -> float: return float(_rules.get("unitDamageMultiplier", 1.0))
	)
	_number(
		"@rules.unitcrashdamagemultiplier",
		false,
		func(v: float) -> void: _rules["unitCrashDamageMultiplier"] = v,
		func() -> float: return float(_rules.get("unitCrashDamageMultiplier", 1.0))
	)
	_number(
		"@rules.unitminespeedmultiplier",
		false,
		func(v: float) -> void: _rules["unitMineSpeedMultiplier"] = v,
		func() -> float: return float(_rules.get("unitMineSpeedMultiplier", 1.0))
	)
	_number(
		"@rules.unitbuildspeedmultiplier",
		false,
		func(v: float) -> void: _rules["unitBuildSpeedMultiplier"] = v,
		func() -> float: return float(_rules.get("unitBuildSpeedMultiplier", 1.0)),
		Callable(),
		0.0,
		50.0
	)
	_number(
		"@rules.unitcostmultiplier",
		false,
		func(v: float) -> void: _rules["unitCostMultiplier"] = v,
		func() -> float: return float(_rules.get("unitCostMultiplier", 1.0))
	)
	_check(
		"@rules.logicunitcontrol",
		func(v: bool) -> void: _rules["logicUnitControl"] = v,
		func() -> bool: return bool(_rules.get("logicUnitControl", true))
	)
	_check(
		"@rules.logicunitbuild",
		func(v: bool) -> void: _rules["logicUnitBuild"] = v,
		func() -> bool: return bool(_rules.get("logicUnitBuild", true)),
		func() -> bool: return bool(_rules.get("logicUnitControl", true))
	)
	_check(
		"@rules.logicunitdeconstruct",
		func(v: bool) -> void: _rules["logicUnitDeconstruct"] = v,
		func() -> bool: return bool(_rules.get("logicUnitDeconstruct", false)),
		func() -> bool: return bool(_rules.get("logicUnitControl", true))
	)
	if _matches("@bannedunits"):
		_current.add(_wide_button("@bannedunits", "unitsSmall", _open_banned_units)).left()
		_current.row()
	_check(
		"@bannedunits.whitelist",
		func(v: bool) -> void: _rules["unitWhitelist"] = v,
		func() -> bool: return bool(_rules.get("unitWhitelist", false))
	)

	_category("enemy")
	_check(
		"@rules.attack",
		func(v: bool) -> void: _rules["attackMode"] = v,
		func() -> bool: return bool(_rules.get("attackMode", false))
	)
	_check(
		"@rules.corecapture",
		func(v: bool) -> void: _rules["coreCapture"] = v,
		func() -> bool: return bool(_rules.get("coreCapture", false))
	)
	_check(
		"@rules.placerangecheck",
		func(v: bool) -> void: _rules["placeRangeCheck"] = v,
		func() -> bool: return bool(_rules.get("placeRangeCheck", false))
	)
	_check(
		"@rules.polygoncoreprotection",
		func(v: bool) -> void: _rules["polygonCoreProtection"] = v,
		func() -> bool: return bool(_rules.get("polygonCoreProtection", false))
	)
	_number(
		"@rules.enemycorebuildradius",
		false,
		func(v: float) -> void: _rules["enemyCoreBuildRadius"] = v * TILESIZE,
		func() -> float:
			return minf(float(_rules.get("enemyCoreBuildRadius", 400.0)) / TILESIZE, 200.0),
		func() -> bool: return not bool(_rules.get("polygonCoreProtection", false))
	)

	_category("environment")
	_check(
		"@rules.pauseDisabled",
		func(v: bool) -> void: _rules["pauseDisabled"] = v,
		func() -> bool: return bool(_rules.get("pauseDisabled", false))
	)
	_check(
		"@rules.explosions",
		func(v: bool) -> void: _rules["damageExplosions"] = v,
		func() -> bool: return bool(_rules.get("damageExplosions", true))
	)
	_check(
		"@rules.fire",
		func(v: bool) -> void: _rules["fire"] = v,
		func() -> bool: return bool(_rules.get("fire", true))
	)
	_check(
		"@rules.fog",
		func(v: bool) -> void: _rules["fog"] = v,
		func() -> bool: return bool(_rules.get("fog", false))
	)
	_check(
		"@rules.lighting",
		func(v: bool) -> void: _rules["lighting"] = v,
		func() -> bool: return bool(_rules.get("lighting", true))
	)
	_check(
		"@rules.limitarea",
		func(v: bool) -> void: _rules["limitMapArea"] = v,
		func() -> bool: return bool(_rules.get("limitMapArea", false)),
		func() -> bool: return not _in_game()
	)
	_numberi(
		"x",
		func(v: int) -> void: _rules["limitX"] = float(v),
		func() -> int: return int(_rules.get("limitX", 0)),
		func() -> bool: return bool(_rules.get("limitMapArea", false)) and not _in_game(),
		0,
		10000
	)
	_numberi(
		"y",
		func(v: int) -> void: _rules["limitY"] = float(v),
		func() -> int: return int(_rules.get("limitY", 0)),
		func() -> bool: return bool(_rules.get("limitMapArea", false)) and not _in_game(),
		0,
		10000
	)
	_numberi(
		"w",
		func(v: int) -> void: _rules["limitWidth"] = float(v),
		func() -> int: return int(_rules.get("limitWidth", 1)),
		func() -> bool: return bool(_rules.get("limitMapArea", false)) and not _in_game(),
		0,
		10000
	)
	_numberi(
		"h",
		func(v: int) -> void: _rules["limitHeight"] = float(v),
		func() -> int: return int(_rules.get("limitHeight", 1)),
		func() -> bool: return bool(_rules.get("limitMapArea", false)) and not _in_game(),
		0,
		10000
	)
	_number(
		"@rules.solarmultiplier",
		false,
		func(v: float) -> void: _rules["solarMultiplier"] = v,
		func() -> float: return float(_rules.get("solarMultiplier", 1.0))
	)
	if _matches("@rules.weather"):
		_current.add(_wide_button("@rules.weather", "rainSmall", _open_weather, 250.0)).left()
		_current.row()

	_category("light")
	if _matches("@rules.ambientlight"):
		_current.add(_ambient_light_button()).left()
		_current.row()
	_check(
		"@rules.lighting.unitlight",
		func(v: bool) -> void: _rules["unitLight"] = v,
		func() -> bool: return bool(_rules.get("unitLight", true))
	)

	_category("music")
	_check(
		"@rules.alwaysplaymusic",
		func(v: bool) -> void: _rules["alwaysPlayMusic"] = v,
		func() -> bool: return bool(_rules.get("alwaysPlayMusic", false)),
		func() -> bool: return not bool(_rules.get("disableMusic", false))
	)
	_check(
		"@rules.nomusic",
		func(v: bool) -> void: _rules["disableMusic"] = v,
		func() -> bool: return bool(_rules.get("disableMusic", false))
	)
	_text(
		"@rules.ambientmusic",
		func(s: String) -> void: _rules["ambientMusic"] = _music_value(s),
		func() -> String: return _music_text(_rules.get("ambientMusic")),
		func(text: String) -> bool: return _music_valid(text),
		func() -> bool: return not bool(_rules.get("disableMusic", false))
	)
	_text(
		"@rules.darkmusic",
		func(s: String) -> void: _rules["darkMusic"] = _music_value(s),
		func() -> String: return _music_text(_rules.get("darkMusic")),
		func(text: String) -> bool: return _music_valid(text),
		func() -> bool: return not bool(_rules.get("disableMusic", false))
	)

	_category("planet")
	if _matches("@rules.title.planet"):
		_current.add(_planet_table()).left()
		_current.row()

	_category("teams")
	if _show_rule_edit:
		_check(
			"@rules.allowedit",
			func(v: bool) -> void: _rules["allowEditRules"] = v,
			func() -> bool: return bool(_rules.get("allowEditRules", false))
		)
	_team(
		"@rules.playerteam",
		func(t: int) -> void: _rules["defaultTeam"] = float(t),
		func() -> int: return int(_rules.get("defaultTeam", 1))
	)
	_team(
		"@rules.enemyteam",
		func(t: int) -> void: _rules["waveTeam"] = float(t),
		func() -> int: return int(_rules.get("waveTeam", 2))
	)
	for team in _base_teams():
		_build_team_collapser(int(team.get("id", 0)))

	# `addToMain`: only categories with rows get their title + accent rule.
	for entry in _categories:
		var category: MindTable = entry["table"]
		if category.get_child_count() == 0:
			continue
		var title := _t("@rules.title.%s" % str(entry["name"]))
		_main.add(MindWidgets.styled_label(MindWidgets.markup_text(title), "techLabel")).left().grow_x_axis().pad(5)
		_main.row()
		# code-instantiated: the accent rule under each category title.
		var rule := ColorRect.new()
		rule.color = MindStyles.ACCENT
		rule.custom_minimum_size = Vector2(0.0, RULE_HEIGHT)
		_main.add(rule).left().grow_x_axis().set_pad_right(100.0).pad(5)
		_main.row()
		_main.add(category).left().grow_x_axis()
		_main.row()
	# A rebuilt table needs an immediate layout pass (a search keystroke rebuilt
	# the cells; the pending sort notification places the new ones).
	_sort_all(_main)
	_refresh_conditions()


## Bottom-up `sort_now` over every nested table (a rebuild must place the new
## cells in the same frame).
func _sort_all(node: Node) -> void:
	if node is MindTable:
		for child in node.get_children():
			_sort_all(child)
		(node as MindTable).sort_now()


## `category(name)`: a fresh left-aligned rule table.
func _category(name: String) -> void:
	var table := MindTable.new()
	table.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	# `current.left().defaults().fillX().expandX().left().pad(5)`
	# (`CustomRulesDialog.java:377-383`): every rule cell has a 5 px pad, so a
	# 32 px check row sits on a 42 px pitch (PAR-0430).
	table.defaults().pad(5.0)
	_current = table
	_categories.append({"name": name, "table": table})


## Bundle text with markup tags stripped for plain `Label`/`Button` widgets.
func _plain(key: String) -> String:
	var text := _t(key)
	var out := ""
	var depth := 0
	for ch in text:
		if ch == "[":
			depth += 1
		elif ch == "]":
			depth = maxi(0, depth - 1)
		elif depth == 0:
			out += ch
	return out


## Whether the bundle carries a key (`Core.bundle.has`).
func _has_key(key: String) -> bool:
	var assets := get_node_or_null("/root/MindAssets")
	if assets == null or not assets.has_method("bundle_get"):
		return false
	return str(assets.call("bundle_get", key)) != key


## `ruleInfo`: the `<key>.info` tooltip when the bundle has that key.
func _rule_info(control: Control, key: String) -> void:
	var info := "%s.info" % key.trim_prefix("@")
	if _has_key(info):
		control.tooltip_text = _plain("@%s" % info)


## `Core.bundle.get(key).toLowerCase().contains(ruleSearch)`.
func _matches(key: String) -> bool:
	if _search.is_empty():
		return true
	return _t(key).to_lower().contains(_search)


func _on_search(text: String) -> void:
	_search = text.strip_edges()
	while _search.contains("  "):
		_search = _search.replace("  ", " ")
	_search = _search.to_lower()
	_setup_main()


## `check(text, cons, prov, condition)` (Java's `check` + `ruleInfo`).
func _check(key: String, cons: Callable, prov: Callable, cond: Callable = Callable()) -> void:
	var row := _make_check_row(key, cons, prov, cond)
	if row != null:
		_row_into(_current, row, false)


func _numberi(
	key: String,
	cons: Callable,
	prov: Callable,
	cond: Callable,
	minimum: float,
	maximum: float
) -> void:
	var row := _make_number_row(key, true, cons, prov, cond, minimum, maximum)
	if row != null:
		_row_into(_current, row)


func _number(
	key: String,
	integer: bool,
	cons: Callable,
	prov: Callable,
	cond: Callable = Callable(),
	minimum: float = 0.0,
	maximum: float = INF
) -> void:
	var row := _make_number_row(key, integer, cons, prov, cond, minimum, maximum)
	if row != null:
		_row_into(_current, row)


## Adds a built row to a `MindTable` (`add` + `row`) or a plain `VBoxContainer`.
## `pad_top_zero`: the table-built rows (`number`, `team`) end in
## `.padTop(0)` (`CustomRulesDialog.java:404, 437, 452, 467`); checks keep the
## category's `pad(5)` (`:478`).
func _row_into(target: Node, row: Control, pad_top_zero: bool = true) -> void:
	# `main.left().defaults().fillX().left()` (`CustomRulesDialog.java:144`):
	# rule rows hug the left edge of the column.
	if row is MindTable:
		(row as MindTable).set_table_left(true)
	if target is MindTable:
		var cell := (target as MindTable).add(row).left().grow_x_axis()
		if pad_top_zero:
			cell.set_pad_top(0)
		(target as MindTable).row()
	else:
		row.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		target.add_child(row)


## One `check` row (`check(text, cons, prov, condition)` + `ruleInfo`).
func _make_check_row(
	key: String, cons: Callable, prov: Callable, cond: Callable = Callable()
) -> Control:
	if not _matches(key):
		return null
	var row := MindWidgets.table()
	var check := MindWidgets.check(_plain(key), bool(prov.call()), func(v: bool) -> void:
		cons.call(v)
		_refresh_conditions()
	)
	# `current.check(text, cons)` is a plain `CheckBox` (`Styles.defaultCheck`,
	# no background) left-aligned at its own width (`CustomRulesDialog.java:
	# 478-479`), not `Elems.check`'s grayPanel strip (L01 T3, java/rules-banned.png).
	for state in ["normal", "hover", "pressed", "hover_pressed", "disabled", "focus"]:
		check.add_theme_stylebox_override(state, StyleBoxEmpty.new())
	check.set_content_inset(0.0, 0.0)
	_rule_info(check, key)
	row.add(check).left()
	_register_row(row, cond, func(enabled: bool) -> void:
		check.disabled = not enabled
		check.modulate = Color(1, 1, 1, 1) if enabled else Color(1, 1, 1, 0.6)
	)
	return row


## One `number`/`numberi` row (width-120 field, min/max validated).
func _make_number_row(
	key: String,
	integer: bool,
	cons: Callable,
	prov: Callable,
	cond: Callable,
	minimum: float,
	maximum: float
) -> Control:
	if not _matches(key):
		return null
	var row := MindWidgets.table()
	var label := MindWidgets.label(MindWidgets.markup(key))
	_rule_info(label, key)
	row.add(label).left().set_pad_right(5)
	var field := MindWidgets.field(_number_text(prov.call(), integer))
	field.custom_minimum_size.x = 120.0
	field.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	field.text_changed.connect(func(text: String) -> void:
		var trimmed := text.strip_edges()
		if not _parse_number(trimmed, integer):
			_invalid_field(field, true)
			return
		var value := trimmed.to_float()
		if value < minimum or value > maximum:
			_invalid_field(field, true)
			return
		_invalid_field(field, false)
		cons.call(int(value) if integer else value)
		_refresh_conditions()
	)
	row.add(field).left().set_pad_right(100.0 if integer else 50.0)
	_register_row(row, cond, func(enabled: bool) -> void:
		field.editable = enabled
		label.add_theme_color_override(
			"default_color", Color(0.6, 0.6, 0.6, 1.0) if not enabled else Color(1, 1, 1, 1)
		)
	)
	return row


## A `check` row inside a plain container (the team collapsers).
func _check_in(
	table: Node, key: String, cons: Callable, prov: Callable, cond: Callable = Callable()
) -> void:
	var row := _make_check_row(key, cons, prov, cond)
	if row != null:
		_row_into(table, row, false)


## A `number` row inside a plain container (the team collapsers).
func _number_in(
	table: Node,
	key: String,
	integer: bool,
	cons: Callable,
	prov: Callable,
	cond: Callable = Callable(),
	minimum: float = 0.0,
	maximum: float = INF
) -> void:
	var row := _make_number_row(key, integer, cons, prov, cond, minimum, maximum)
	if row != null:
		_row_into(table, row)


## A `numberi` row inside a plain container (the team collapsers).
func _numberi_in(
	table: Node,
	key: String,
	cons: Callable,
	prov: Callable,
	cond: Callable = Callable(),
	minimum: float = 0.0,
	maximum: float = INF
) -> void:
	var row := _make_number_row(
		key, true, func(v: float) -> void: cons.call(int(v)), prov, cond, minimum, maximum
	)
	if row != null:
		_row_into(table, row)


## One `text` row (width-300 field, `valid` predicate).
func _text(
	key: String,
	cons: Callable,
	prov: Callable,
	valid: Callable,
	cond: Callable = Callable()
) -> void:
	if not _matches(key):
		return
	var row := MindWidgets.table()
	var label := MindWidgets.label(MindWidgets.markup(key))
	_rule_info(label, key)
	row.add(label).left().set_pad_right(5)
	var field := MindWidgets.field(str(prov.call()))
	field.custom_minimum_size.x = 300.0
	field.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	field.text_changed.connect(func(text: String) -> void:
		var ok := bool(valid.call(text))
		_invalid_field(field, not ok)
		if not ok:
			return
		cons.call(text)
		_refresh_conditions()
	)
	row.add(field).left().set_pad_right(50.0)
	_register_row(row, cond, func(enabled: bool) -> void:
		field.editable = enabled
		label.add_theme_color_override(
			"default_color", Color(0.6, 0.6, 0.6, 1.0) if not enabled else Color(1, 1, 1, 1)
		)
	)
	_row_into(_current, row)


## `team(text, cons, prov)`: a label plus one 60x60 `squareTogglei` per base team.
func _team(key: String, cons: Callable, prov: Callable) -> void:
	if not _matches(key):
		return
	var row := MindWidgets.table()
	var team_label := MindWidgets.label(MindWidgets.markup(key))
	_rule_info(team_label, key)
	row.add(team_label).left().set_pad_right(5)
	var buttons: Array[Button] = []
	var team_ids: Array[int] = []
	for team in _base_teams():
		var id := int(team.get("id", 0))
		var packed: int = int(team.get("color", 0xFFFFFFFF))
		var color := Color8((packed >> 24) & 0xFF, (packed >> 16) & 0xFF, (packed >> 8) & 0xFF)
		# code-instantiated: one tinted toggle per base team (`Team.baseTeams`).
		var button := Button.new()
		button.toggle_mode = true
		button.button_pressed = int(prov.call()) == id
		button.theme_type_variation = "squareTogglei"
		button.custom_minimum_size = Vector2(60.0, 60.0)
		button.icon = MindWidgets.icon_texture("whiteui")
		button.add_theme_color_override("icon_normal_color", color)
		button.add_theme_color_override("icon_pressed_color", color)
		button.add_theme_color_override("icon_hover_color", color)
		button.tooltip_text = _t("@team.%s.name" % str(team.get("name", "")))
		button.pressed.connect(func() -> void:
			cons.call(id)
			for index in buttons.size():
				buttons[index].button_pressed = team_ids[index] == id
			_refresh_conditions()
		)
		row.add(button).pad(1)
		buttons.append(button)
		team_ids.append(id)
	_row_into(_current, row)


## `Team.baseTeams` rows (`MindEditor.palette_teams`; `[{id, name, color}]`).
func _base_teams() -> Array:
	var editor := get_node_or_null("/root/Spine/MindEditor")
	if editor != null and editor.has_method("palette_teams"):
		var rows: Variant = editor.call("palette_teams")
		if rows is Array and not (rows as Array).is_empty():
			return rows
	return []


## `Team.<id>.localizedName()` (`team.<name>.name`); the colour rides the font
## override (`Team.coloredName()` tint, rendered by `_team_font_color`).
func _team_name(id: int) -> String:
	for team in _base_teams():
		if int(team.get("id", 0)) == id:
			return _t("@team.%s.name" % str(team.get("name", "")))
	return str(id)


func _team_font_color(id: int) -> Color:
	for team in _base_teams():
		if int(team.get("id", 0)) == id:
			var packed: int = int(team.get("color", 0xFFFFFFFF))
			return Color8((packed >> 24) & 0xFF, (packed >> 16) & 0xFF, (packed >> 8) & 0xFF)
	return Color.WHITE


## One base team's `TeamRule` (`rules.teams.get(team)`, created on demand).
func _team_rules(team: int) -> Dictionary:
	var teams: Variant = _rules.get("teams", {})
	if not (teams is Dictionary):
		teams = {}
	var key := str(team)
	if not (teams as Dictionary).has(key):
		(teams as Dictionary)[key] = {}
	_rules["teams"] = teams
	return (teams as Dictionary)[key]


## One base team's rule collapser (`teamRules` + `collapser`, Java's `for(Team
## team : Team.baseTeams)` block). The header toggles a `VBoxContainer` so the
## collapsed rows leave the layout (`Collapser` shrinks the min height upstream).
func _build_team_collapser(team: int) -> void:
	var row := MindWidgets.table()
	var header := Button.new()
	header.text = _team_name(team)
	header.add_theme_color_override("font_color", _team_font_color(team))
	header.toggle_mode = true
	header.theme_type_variation = "togglet"
	header.custom_minimum_size = Vector2(260.0, 55.0)
	header.alignment = HORIZONTAL_ALIGNMENT_LEFT
	# code-instantiated: one collapser body per base team.
	var content := VBoxContainer.new()
	content.visible = false
	header.pressed.connect(func() -> void:
		content.visible = header.button_pressed
		row.queue_sort()
		row.sort_now()
	)
	row.add(header).left()
	row.add(content).left().grow_x_axis()
	_team_rows.append({"team": team, "row": content, "header": header})
	var rules := _team_rules(team)
	_check_in(
		content,
		"@rules.blockhealthmultiplier",
		func(v: float) -> void: rules["blockHealthMultiplier"] = v,
		func() -> float: return float(rules.get("blockHealthMultiplier", 1.0))
	)
	_check_in(
		content,
		"@rules.blockdamagemultiplier",
		func(v: float) -> void: rules["blockDamageMultiplier"] = v,
		func() -> float: return float(rules.get("blockDamageMultiplier", 1.0))
	)
	_check_in(
		content,
		"@rules.rtsai",
		func(v: bool) -> void: rules["rtsAi"] = v,
		func() -> bool: return bool(rules.get("rtsAi", false)),
		func() -> bool: return team != int(_rules.get("defaultTeam", 1))
	)
	_numberi_in(
		content,
		"@rules.rtsminsquadsize",
		func(v: int) -> void: rules["rtsMinSquad"] = v,
		func() -> int: return int(rules.get("rtsMinSquad", 0)),
		func() -> bool: return bool(rules.get("rtsAi", false)),
		0,
		100
	)
	_numberi_in(
		content,
		"@rules.rtsmaxsquadsize",
		func(v: int) -> void: rules["rtsMaxSquad"] = v,
		func() -> int: return int(rules.get("rtsMaxSquad", 1)),
		func() -> bool: return bool(rules.get("rtsAi", false)),
		1,
		1000
	)
	_number_in(
		content,
		"@rules.rtsminattackweight",
		false,
		func(v: float) -> void: rules["rtsMinWeight"] = v,
		func() -> float: return float(rules.get("rtsMinWeight", 0.0)),
		func() -> bool: return bool(rules.get("rtsAi", false))
	)
	_check_in(
		content,
		"@rules.buildai",
		func(v: bool) -> void: rules["buildAi"] = v,
		func() -> bool: return bool(rules.get("buildAi", false)),
		func() -> bool:
			return (
				team != int(_rules.get("defaultTeam", 1))
				and int(_rules.get("env", 233)) != _erekir_env
				and not bool(_rules.get("pvp", false))
			)
	)
	_number_in(
		content,
		"@rules.buildaitier",
		true,
		func(v: float) -> void: rules["buildAiTier"] = v,
		func() -> float: return float(rules.get("buildAiTier", 0.0)),
		func() -> bool:
			return (
				bool(rules.get("buildAi", false))
				and int(_rules.get("env", 233)) != _erekir_env
				and not bool(_rules.get("pvp", false))
			),
		0.0,
		1.0
	)
	_check_in(
		content,
		"@rules.protectcores",
		func(v: bool) -> void: rules["protectCores"] = v,
		func() -> bool: return bool(rules.get("protectCores", false))
	)
	_number_in(
		content,
		"@rules.extracorebuildradius",
		false,
		func(v: float) -> void: rules["extraCoreBuildRadius"] = v * TILESIZE,
		func() -> float: return minf(float(rules.get("extraCoreBuildRadius", 0.0)) / TILESIZE, 200.0),
		func() -> bool:
			return (
				not bool(_rules.get("polygonCoreProtection", false))
				and bool(rules.get("protectCores", false))
			)
	)
	_check_in(
		content,
		"@rules.checkplacement",
		func(v: bool) -> void: rules["checkPlacement"] = v,
		func() -> bool: return bool(rules.get("checkPlacement", false))
	)
	_check_in(
		content,
		"@rules.infiniteresources",
		func(v: bool) -> void: rules["infiniteResources"] = v,
		func() -> bool: return bool(rules.get("infiniteResources", false))
	)
	_check_in(
		content,
		"@rules.fillitems",
		func(v: bool) -> void: rules["fillItems"] = v,
		func() -> bool: return bool(rules.get("fillItems", false))
	)
	_number_in(
		content,
		"@rules.buildspeedmultiplier",
		false,
		func(v: float) -> void: rules["buildSpeedMultiplier"] = v,
		func() -> float: return float(rules.get("buildSpeedMultiplier", 1.0)),
		Callable(),
		0.001,
		50.0
	)
	_number_in(
		content,
		"@rules.unitfactoryactivation",
		false,
		func(v: float) -> void: rules["unitFactoryActivationDelay"] = v * 60.0,
		func() -> float: return float(rules.get("unitFactoryActivationDelay", 0.0)) / 60.0
	)
	_number_in(
		content,
		"@rules.unitdamagemultiplier",
		false,
		func(v: float) -> void: rules["unitDamageMultiplier"] = v,
		func() -> float: return float(rules.get("unitDamageMultiplier", 1.0))
	)
	_number_in(
		content,
		"@rules.unitcrashdamagemultiplier",
		false,
		func(v: float) -> void: rules["unitCrashDamageMultiplier"] = v,
		func() -> float: return float(rules.get("unitCrashDamageMultiplier", 1.0))
	)
	_number_in(
		content,
		"@rules.unitminespeedmultiplier",
		false,
		func(v: float) -> void: rules["unitMineSpeedMultiplier"] = v,
		func() -> float: return float(rules.get("unitMineSpeedMultiplier", 1.0))
	)
	_number_in(
		content,
		"@rules.unitbuildspeedmultiplier",
		false,
		func(v: float) -> void: rules["unitBuildSpeedMultiplier"] = v,
		func() -> float: return float(rules.get("unitBuildSpeedMultiplier", 1.0)),
		Callable(),
		0.001,
		50.0
	)
	_number_in(
		content,
		"@rules.unitcostmultiplier",
		false,
		func(v: float) -> void: rules["unitCostMultiplier"] = v,
		func() -> float: return float(rules.get("unitCostMultiplier", 1.0))
	)
	_number_in(
		content,
		"@rules.unithealthmultiplier",
		false,
		func(v: float) -> void: rules["unitHealthMultiplier"] = v,
		func() -> float: return float(rules.get("unitHealthMultiplier", 1.0))
	)
	if content.get_child_count() == 0:
		# `if(!current.hasChildren()) teamRules.clear()`: an empty (or fully
		# filtered-out) team row never joins the category.
		return
	# `wasCurrent.add(teamRules).row()`: the collapser rows live in the "teams"
	# category table, not directly in `_main`.
	_row_into(_current, row)

func _register_row(row: Control, cond: Callable, apply: Callable) -> void:
	_rows.append({"row": row, "cond": cond, "apply": apply})
	var enabled := true
	if cond.is_valid():
		enabled = bool(cond.call())
	apply.call(enabled)


## Re-evaluates every row condition (Java does this each frame; the port runs it
## on every edit and rebuild, which is enough for the row states).
func _refresh_conditions() -> void:
	for entry in _rows:
		var cond: Callable = entry["cond"]
		var enabled := true
		if cond.is_valid():
			enabled = bool(cond.call())
		entry["apply"].call(enabled)


## `state.isGame()` (`GameState.isGame`: playing/paused, not menu).
func _in_game() -> bool:
	var host := get_node_or_null("/root/Spine/SimHost")
	if host == null or not host.has_method("get_state"):
		return false
	return str(host.call("get_state")) != "menu"


## `Strings.canParsePositiveFloat` (`number`) / `Strings.parseInt` (`numberi`);
## the min/max bounds are checked separately by the row.
func _parse_number(text: String, integer: bool = false) -> bool:
	if text.is_empty():
		return false
	if integer:
		return text.is_valid_int()
	if not text.is_valid_float():
		return false
	return text.to_float() >= 0.0


func _number_text(value: Variant, integer: bool) -> String:
	if integer:
		return str(int(value))
	var number := float(value)
	var text := String.num(number, 6)
	if not text.contains("."):
		text += ".0"
	return text


func _invalid_field(field: LineEdit, invalid: bool) -> void:
	field.add_theme_color_override(
		"font_color", Color(1.0, 0.35, 0.35) if invalid else Color(1, 1, 1)
	)


## `MusicContainer` list text (`Seq<MusicContainer>.toString(", ")`); the port's
## JSON shape is `{"tracks": ["name", ...]}`.
func _music_text(value: Variant) -> String:
	if value is Dictionary:
		var tracks: Variant = (value as Dictionary).get("tracks", [])
		if tracks is Array:
			var names: PackedStringArray = []
			for track in tracks:
				names.append(str(track))
			return ", ".join(names)
	return ""


func _music_value(text: String) -> Variant:
	var trimmed := text.strip_edges()
	if trimmed.is_empty():
		return null
	var tracks: Array = []
	for part in trimmed.split(","):
		var name := part.strip_edges()
		if not name.is_empty():
			tracks.append(name)
	return {"tracks": tracks}


## `MusicContainer::new` accepts any comma-separated list (`parser.get(str)`).
func _music_valid(text: String) -> bool:
	var trimmed := text.strip_edges()
	if trimmed.is_empty():
		return true
	for part in trimmed.split(","):
		var name := part.strip_edges()
		if name.is_empty() or not name.is_valid_identifier():
			return false
	return true


## `@configure`: the loadout editor (`LoadoutDialog.show(999999, rules.loadout,
## i -> true, reset -> copper x100)`; `CustomRulesDialog.java:198-205`).
func _open_loadout() -> void:
	var stacks: Array = _rules.get("loadout", [])
	var panel := _overlay_open("@configure", 420.0)
	for stack_variant in stacks:
		var stack: Dictionary = stack_variant
		# code-instantiated: one amount row per loadout stack.
		var row := MindWidgets.table()
		row.add(MindWidgets.label(str(stack.get("item", "")))).left().set_pad_right(6)
		var field := MindWidgets.field(str(int(stack.get("amount", 0))))
		field.custom_minimum_size.x = 120.0
		field.text_changed.connect(func(text: String) -> void:
			var trimmed := text.strip_edges()
			if not _parse_number(trimmed) or trimmed.to_float() > 999999.0:
				_invalid_field(field, true)
				return
			_invalid_field(field, false)
			stack["amount"] = int(trimmed.to_float())
		)
		row.add(field).left()
		panel.add(row).left().grow_x_axis().pad(2)
		panel.row()
	var reset_button := _wide_button("@settings.reset", "refresh", Callable())
	reset_button.pressed.connect(func() -> void:
		_rules["loadout"] = [{"item": "copper", "amount": 100}]
		_close_overlay()
		_setup_main()
	)
	panel.add(reset_button).left()
	panel.row()
	_finish_overlay(panel)


## `current.button("@bannedblocks", ..., () -> bannedBlocks.show(rules.bannedBlocks))`
## (`CustomRulesDialog.java:189`): the two-pane `BannedContentDialog` edits the
## working `_rules` copy; edits come back through `banned_edited`.
func _open_banned_blocks() -> void:
	_open_banned("block")


## `CustomRulesDialog.java:210`: the unit ban picker.
func _open_banned_units() -> void:
	_open_banned("unit")


func _open_banned(kind: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("open_dialog"):
		return
	ui.call("open_dialog", "banned_content", JSON.stringify({"kind": kind, "rules": _rules}))
	var ui_root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui_root == null or not ui_root.has_method("dialog"):
		return
	var dialog: Node = ui_root.call("dialog", "banned_content")
	if dialog != null and dialog.has_signal("banned_edited") \
			and not dialog.is_connected("banned_edited", _on_banned_edited):
		dialog.connect("banned_edited", _on_banned_edited)


## `rules.bannedBlocks`/`bannedUnits` edited live by the ban dialog.
func _on_banned_edited(json: String) -> void:
	var parsed: Variant = JSON.parse_string(json)
	if parsed is Dictionary:
		_rules = parsed


## `@rules.weather`: weather-entry editor (`CustomRulesDialog.weatherDialog`).
func _open_weather() -> void:
	var panel := _overlay_open("@rules.weather", 460.0)
	var entries: Array = _rules.get("weather", [])
	# Drop `null` weather names (`rules.weather.removeAll(w -> w.weather == null)`).
	entries = entries.filter(func(entry: Variant) -> bool:
		return entry is Dictionary and not str((entry as Dictionary).get("weather", "")).is_empty()
	)
	_rules["weather"] = entries
	for entry_variant in entries:
		var entry: Dictionary = entry_variant
		# code-instantiated: one weather entry block (duration/frequency minutes).
		var block := MindWidgets.table()
		var header := MindWidgets.table()
		header.add(MindWidgets.label(str(entry.get("weather", "")))).left().set_pad_right(6)
		var remove_entry := _wide_button("@cancel", "cancel", Callable())
		remove_entry.pressed.connect(func() -> void:
			entries.erase(entry)
			_rules["weather"] = entries
			_close_overlay()
			_open_weather()
		)
		header.add(remove_entry).left()
		block.add(header).left().grow_x_axis()
		block.row()
		var always := bool(entry.get("always", false))
		var duration := MindWidgets.table()
		duration.add(MindWidgets.label(_t("@rules.weather.duration"))).left().set_pad_right(4)
		duration.add(_minutes_field(entry, "minDuration", not always)).left().set_pad_right(4)
		duration.add(MindWidgets.label(_t("@waves.to"))).left().set_pad_right(4)
		duration.add(_minutes_field(entry, "maxDuration", not always)).left().set_pad_right(4)
		duration.add(MindWidgets.label(_t("@unit.minutes"))).left()
		block.add(duration).left().grow_x_axis()
		block.row()
		var frequency := MindWidgets.table()
		frequency.add(MindWidgets.label(_t("@rules.weather.frequency"))).left().set_pad_right(4)
		frequency.add(_minutes_field(entry, "minFrequency", not always)).left().set_pad_right(4)
		frequency.add(MindWidgets.label(_t("@waves.to"))).left().set_pad_right(4)
		frequency.add(_minutes_field(entry, "maxFrequency", not always)).left().set_pad_right(4)
		frequency.add(MindWidgets.label(_t("@unit.minutes"))).left()
		block.add(frequency).left().grow_x_axis()
		block.row()
		var check := MindWidgets.check(_t("@rules.weather.always"), always, func(v: bool) -> void:
			entry["always"] = v
			_close_overlay()
			_open_weather()
		)
		block.add(check).left()
		panel.add(block).left().grow_x_axis().pad(3)
		panel.row()
	panel.add(_wide_button("@add", "add", _open_weather_add)).left()
	panel.row()
	_finish_overlay(panel)


func _open_weather_add() -> void:
	var panel := _overlay_open("@add", 420.0)
	var host := get_node_or_null("/root/Spine/SimHost")
	var names: Array = []
	if host != null and host.has_method("content_list"):
		var listed: Variant = host.call("content_list", "weather")
		if listed is Array:
			names = listed
	var entries: Array = _rules.get("weather", [])
	var index := 0
	for name_variant in names:
		var name := str(name_variant)
		var button := MindWidgets.button(_t("@weather.%s.name" % name))
		button.custom_minimum_size = Vector2(140.0, 50.0)
		button.pressed.connect(func() -> void:
			entries.append(
				{
					"weather": name,
					"minFrequency": 97200.0,
					"maxFrequency": 194400.0,
					"minDuration": 16200.0,
					"maxDuration": 48600.0,
					"cooldown": 0.0,
					"intensity": 1.0,
					"always": false,
				}
			)
			_rules["weather"] = entries
			_close_overlay()
			_open_weather()
		)
		panel.add(button).pad(1)
		index += 1
		if index % 2 == 0:
			panel.row()
	_finish_overlay(panel)


## `@rules.ambientlight` colour picker (`ui.picker.show`).
func _ambient_light_button() -> Control:
	var row := MindWidgets.table()
	var swatch := ColorRect.new()
	swatch.custom_minimum_size = Vector2(50.0, 50.0)
	swatch.color = _ambient_color()
	row.add(swatch).pad(4)
	row.add(MindWidgets.label(_t("@rules.ambientlight"))).left().set_pad_right(10)
	var button := Button.new()
	button.text = ""
	button.custom_minimum_size = Vector2(250.0, 50.0)
	button.pressed.connect(func() -> void:
		_show_color_picker(func(color: Color) -> void:
			_rules["ambientLight"] = "%02x%02x%02x%02x" % [
				int(color.r * 255.0), int(color.g * 255.0), int(color.b * 255.0), int(color.a * 255.0)
			]
			swatch.color = color
		)
	)
	row.add(button).left()
	return row


func _ambient_color() -> Color:
	var hex := str(_rules.get("ambientLight", "00000000"))
	if hex.length() == 8:
		return Color8(
			hex.substr(0, 2).hex_to_int(),
			hex.substr(2, 2).hex_to_int(),
			hex.substr(4, 2).hex_to_int(),
			hex.substr(6, 2).hex_to_int()
		)
	return Color(0, 0, 0, 0)


## `ui.picker.show(col, col::set)` (`ColorPicker` signal `color_selected`).
func _show_color_picker(listener: Callable) -> void:
	var picker := get_node_or_null("/root/Spine/Ui/UiRoot")
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("open_dialog"):
		return
	if picker == null or not picker.has_method("dialog"):
		return
	ui.call("open_dialog", "picker", "")
	var node: Node = picker.call("dialog", "picker")
	if node == null or not node.has_signal("color_selected"):
		return
	if node.has_method("set_color"):
		node.call("set_color", _ambient_color())
	if node.is_connected("color_selected", listener):
		node.disconnect("color_selected", listener)
	node.connect("color_selected", listener)


## `category("planet")`: planet toggle buttons (`planet.applyRules(rules, true)`)
## plus `@rules.anyenv`.
func _planet_table() -> Control:
	var table := MindWidgets.table()
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	var names: Array = []
	if campaign != null and campaign.has_method("rules_planets"):
		var listed: Variant = campaign.call("rules_planets")
		if listed is Array:
			names = listed
	var index := 0
	var buttons: Array[Button] = []
	var planet_names: Array[String] = []
	for row_variant in names:
		var row: Dictionary = row_variant
		var name := str(row.get("name", ""))
		# code-instantiated: one planet toggle per landable planet.
		var button := Button.new()
		button.text = _t("@planet.%s.name" % name)
		button.theme_type_variation = "flatTogglet"
		button.toggle_mode = true
		button.button_pressed = str(_rules.get("planet", "serpulo")) == name
		button.custom_minimum_size = Vector2(140.0, 50.0)
		button.pressed.connect(func() -> void:
			_apply_planet(name)
			for i in buttons.size():
				buttons[i].button_pressed = planet_names[i] == name
			_refresh_conditions()
		)
		table.add(button).pad(1)
		buttons.append(button)
		planet_names.append(name)
		index += 1
		if index % 3 == 0:
			table.row()
	var any := Button.new()
	any.text = _t("@rules.anyenv")
	any.theme_type_variation = "flatTogglet"
	any.toggle_mode = true
	any.button_pressed = str(_rules.get("planet", "serpulo")) == "sun"
	any.custom_minimum_size = Vector2(140.0, 50.0)
	any.pressed.connect(func() -> void:
		_rules["attributes"] = {}
		_rules["env"] = _default_env()
		_rules["planet"] = "sun"
		for i in buttons.size():
			buttons[i].button_pressed = false
		any.button_pressed = true
		_refresh_conditions()
	)
	table.add(any).pad(1)
	return table


func _apply_planet(name: String) -> void:
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign == null or not campaign.has_method("apply_planet_rules"):
		return
	var updated: Variant = campaign.call("apply_planet_rules", name, JSON.stringify(_rules))
	if updated is String and not str(updated).is_empty():
		var parsed: Variant = JSON.parse_string(str(updated))
		if parsed is Dictionary:
			_rules.clear()
			for key in (parsed as Dictionary):
				_rules[key] = (parsed as Dictionary)[key]


func _planet_env(name: String) -> int:
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign != null and campaign.has_method("planet_default_env"):
		return int(campaign.call("planet_default_env", name))
	return 0


func _default_env() -> int:
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	if campaign != null and campaign.has_method("default_env"):
		return int(campaign.call("default_env"))
	return 233


## A 300/250-wide category action button (`current.button(...).width(...)`).
func _wide_button(key: String, icon: String, listener: Callable, width: float = 300.0) -> Button:
	var button := MindWidgets.icon_button(icon, _plain(key))
	button.custom_minimum_size.x = width
	button.alignment = HORIZONTAL_ALIGNMENT_LEFT
	button.pressed.connect(listener)
	return button


## One minutes field of a weather entry (`field(f, entry.minDuration / toMinutes,
## v -> entry.minDuration = v * toMinutes)`).
func _minutes_field(entry: Dictionary, key: String, enabled: bool) -> LineEdit:
	var field := MindWidgets.field(_number_text(float(entry.get(key, 0.0)) / 60.0, false))
	field.custom_minimum_size = Vector2(90.0, 40.0)
	field.editable = enabled
	field.text_changed.connect(func(text: String) -> void:
		var trimmed := text.strip_edges()
		if not _parse_number(trimmed):
			_invalid_field(field, true)
			return
		_invalid_field(field, false)
		entry[key] = trimmed.to_float() * 60.0
	)
	return field


## `@edit`: the clipboard menu (`CustomRulesDialog.java:60-108`).
func _open_edit_menu() -> void:
	var panel := _overlay_open("@edit.menu", 320.0)
	var copy_button := _wide_button("@copy.clipboard", "copy", Callable())
	copy_button.pressed.connect(func() -> void:
		var copy := _rules.duplicate(true)
		copy.erase("spawns")
		DisplayServer.clipboard_set(JSON.stringify(copy))
		var ui := get_node_or_null("/root/MindUi")
		if ui != null and ui.has_method("show_info"):
			ui.call("show_info", _t("@copied"))
		_close_overlay()
	)
	panel.add(copy_button).left()
	panel.row()
	var clipboard := DisplayServer.clipboard_get()
	var loadable := not clipboard.is_empty() and clipboard.strip_edges().begins_with("{")
	var load_button := _wide_button("@load.clipboard", "download", func() -> void:
		var parsed: Variant = JSON.parse_string(DisplayServer.clipboard_get())
		if not (parsed is Dictionary):
			_show_error("@rules.invaliddata")
			_close_overlay()
			return
		var loaded: Dictionary = parsed
		# `newRules.spawns/objectives = rules.spawns/objectives` (map-specific).
		loaded["spawns"] = _rules.get("spawns", [])
		loaded["objectives"] = _rules.get("objectives", [])
		_rules.clear()
		for key in loaded:
			_rules[key] = loaded[key]
		_close_overlay()
		_setup_main()
	)
	load_button.disabled = not loadable
	panel.add(load_button).left()
	panel.row()
	var reset_menu := _wide_button("@settings.reset", "refresh", Callable())
	reset_menu.pressed.connect(func() -> void:
		var reset := _reset if not _reset.is_empty() else _rules
		var copy := {}
		for key in reset:
			copy[key] = reset[key]
		_rules.clear()
		for key in copy:
			_rules[key] = copy[key]
		_close_overlay()
		_setup_main()
	)
	panel.add(reset_menu).left()
	panel.row()
	_finish_overlay(panel)


func _show_error(key: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("show_text"):
		ui.call("show_text", _t("@error.title"), _t(key))


## Places a freshly built overlay's cells immediately (a new table is not sorted
## until the next layout pass).
func _finish_overlay(panel: MindTable) -> void:
	panel.sort_now()
	var parent := panel.get_parent()
	if parent != null:
		parent.queue_sort()


## Opens a code-instantiated overlay panel (`BaseDialog` sub-dialogs upstream).
func _overlay_open(title: String, width: float) -> MindTable:
	_close_overlay()
	# code-instantiated: per-action popup panels have no static scene.
	var root := ColorRect.new()
	root.color = Color(0, 0, 0, 0.8)
	root.set_anchors_preset(Control.PRESET_FULL_RECT)
	root.mouse_filter = Control.MOUSE_FILTER_STOP
	add_child(root)
	var center := CenterContainer.new()
	center.set_anchors_preset(Control.PRESET_FULL_RECT)
	center.mouse_filter = Control.MOUSE_FILTER_IGNORE
	root.add_child(center)
	var panel_root := PanelContainer.new()
	panel_root.theme_type_variation = "defaultDialog"
	panel_root.custom_minimum_size.x = width
	center.add_child(panel_root)
	var layout := VBoxContainer.new()
	layout.add_theme_constant_override("separation", 6)
	panel_root.add_child(layout)
	layout.add_child(MindWidgets.styled_label(MindWidgets.markup(title), "techLabel"))
	var body := MindTable.new()
	body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	layout.add_child(body)
	var close := MindWidgets.button(_t("@back"))
	close.pressed.connect(_close_overlay)
	layout.add_child(close)
	_overlay = root
	return body


func _close_overlay() -> void:
	if _overlay != null and is_instance_valid(_overlay):
		_overlay.queue_free()
	_overlay = null
