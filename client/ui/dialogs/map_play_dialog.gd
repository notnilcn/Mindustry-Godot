## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/MapPlayDialog.java (plan 14 M5).
##
## Gamemode selection, the mode help list, Customize Rules, the map preview and
## the high score, then Back/Play. `Gamemode.editor` is hidden upstream. The
## dialog is a non-fill `BaseDialog` (`setFillParent(false)`): the column floats
## centred over the dim without a boxed panel or scroll body.
## The selected mode + rules JSON are handed to `MindCampaign.start_map_mode`
## (`control.playMap(map, rules, playtesting)`).

extends MindDialog

## Visible `Gamemode` names in declaration order (`Gamemode.all` minus editor).
const MODES := ["survival", "sandbox", "attack", "pvp"]
## `MapPlayDialog` mode-button size (`size(140f, mobile ? 44f : 54f)`).
const MODE_BUTTON := Vector2(140.0, 54.0)
## `size(mobile && !portrait ? 150f : 250f)`.
const PREVIEW_SIZE := Vector2(250.0, 250.0)
## `buttons.button(...).size(210f, 64f)` (`addCloseButton` + `@play`).
const ACTION_BUTTON := Vector2(210.0, 64.0)

var _map_name := ""
var _mode := "survival"
## Working rules JSON (`MapPlayDialog.rules`); empty = the map's own rules.
var _rules_json := ""
## `MapPlayDialog.show(map, playtesting)`: the editor's Playtest route returns
## to the editor after play (`MapEditorDialog.playtest`).
var _playtest := false
var _mode_buttons: Array[Button] = []
var _button_modes: Array[String] = []


func _ready() -> void:
	should_pause = false
	super._ready()
	_apply_floating_layout()


## `super(""); setFillParent(false)` with `Styles.defaultDialog`
## (`background = windowEmpty`, `stageBackground = black9`): the content floats
## centred over the dim with no boxed panel, the dialog sizes to its content
## (no scrolling body), and the title uses `titleFontColor = Pal.accent`.
func _apply_floating_layout() -> void:
	var panel := get_node_or_null("Center/Panel") as Control
	if panel != null:
		panel.theme_type_variation = "none"
		panel.custom_minimum_size = Vector2.ZERO
		# The floating table still carries `windowEmpty`'s 4 px inset above
		# the (unpadded) title row, and under the buttons the button's own
		# `pad(3)` plus the `buttons` cell's `pad(3)` (`Dialog.java:208-214`)
		# before that inset: java/05-sbplay.png puts the title rule at y=56
		# and the mode frame at y=90 for the 1152x648 sandbox dialog.
		var inset := StyleBoxEmpty.new()
		inset.content_margin_top = 4.0
		inset.content_margin_bottom = 4.0 + 3.0 + 3.0
		panel.add_theme_stylebox_override("panel", inset)
	if body != null:
		# The body reports its content height so nothing is clipped/scrolled.
		body.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
		body.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	if buttons != null:
		buttons.add_theme_constant_override("separation", 8)
	title_color = MindStyles.ACCENT


func set_context_json(json_text: String) -> void:
	super.set_context_json(json_text)
	var ctx := context()
	_map_name = str(ctx.get("map", ""))
	var requested := str(ctx.get("mode", ""))
	if MODES.has(requested):
		_mode = requested
	_rules_json = str(ctx.get("rules", ""))
	_playtest = bool(ctx.get("playtest", false))


## `MapPlayDialog.show(map)`: rebuild the body for the current map (the map may
## have changed since the context was set).
func shown() -> void:
	if _map_name.is_empty():
		_map_name = str(context().get("map", ""))
	_build()


func _build() -> void:
	set_title_text(_map_name if not _map_name.is_empty() else _t("@map.play"))
	# `cont.clearChildren()` (`MapPlayDialog.java:45`): `content_table()` appends,
	# so a rebuild (a new map) must drop the previous body first.
	if cont != null:
		for child in cont.get_children():
			cont.remove_child(child)
			child.queue_free()
	var modes := _mode_validity()
	# `if(!selectedGamemode.valid(map)) selectedGamemode = first valid || survival`
	if not bool(modes.get(_mode, true)):
		var picked := ""
		for mode in MODES:
			if bool(modes.get(mode, false)):
				picked = mode
				break
		_mode = picked if not picked.is_empty() else "survival"
	if _rules_json.is_empty():
		_rules_json = _mode_rules_json(_mode)

	var root := content_table()
	# `Table selmode`: `add("@level.mode").colspan(2)`, then the `Tex.button`
	# mode table and the `?` button on one row (`MapPlayDialog.java:56-71`).
	var selmode := MindWidgets.table()
	selmode.add(MindWidgets.label(_t("@level.mode"))).set_colspan(2)
	selmode.row()
	# code-instantiated: `selmode.table(Tex.button, modes -> ...)` frame.
	var mode_frame := PanelContainer.new()
	var frame_box := _pane_box(Color.BLACK, PANE_OUTLINE_GRAY)
	# `Tex.button` pads its content 12 px (the 9-patch splits): the Java mode
	# box is 304x132 around the 2x2 140x54 buttons (java/mapplay-caldera.png).
	frame_box.content_margin_left = 12.0
	frame_box.content_margin_right = 12.0
	frame_box.content_margin_top = 12.0
	frame_box.content_margin_bottom = 12.0
	mode_frame.add_theme_stylebox_override("panel", frame_box)
	var mode_box := MindWidgets.table()
	mode_frame.add_child(mode_box)
	_mode_buttons.clear()
	_button_modes.clear()
	var index := 0
	for mode in MODES:
		# code-instantiated: one toggle per visible Gamemode (`Gamemode.all`).
		var button := MindWidgets.button(_t("@mode.%s.name" % mode))
		button.theme_type_variation = "flatToggleMenut"
		_apply_flat_toggle_menu(button)
		button.toggle_mode = true
		button.button_pressed = mode == _mode
		button.disabled = not bool(modes.get(mode, true))
		button.custom_minimum_size = MODE_BUTTON
		button.pressed.connect(_select_mode.bind(mode))
		mode_box.add(button).width(MODE_BUTTON.x).height(MODE_BUTTON.y)
		if index % 2 == 1:
			mode_box.row()
		index += 1
		_mode_buttons.append(button)
		_button_modes.append(mode)
	selmode.add(mode_frame)
	# `selmode.button("?", this::displayGameModeHelp).width(50f).fillY().padLeft(18f)`
	var help := MindWidgets.button("?")
	apply_pane_button(help, PANE_OUTLINE_GRAY)
	help.custom_minimum_size = Vector2(50.0, 0.0)
	help.pressed.connect(_show_mode_help)
	selmode.add(help).width(50.0).fill_y_axis().set_pad_left(18.0)
	root.add(selmode).expand_x_axis()
	root.row()
	# `cont.button("@customize", Icon.settings, ...).height(50f).width(230)`
	var customize := MindWidgets.button("")
	apply_pane_button(customize, PANE_OUTLINE_GRAY)
	build_button_content(customize, MindWidgets.glyph("settings", 24), _t("@customize"))
	customize.custom_minimum_size = Vector2(230.0, 50.0)
	customize.pressed.connect(_open_rules)
	root.add(customize).width(230.0).height(50.0)
	root.row()
	# `cont.add(new BorderImage(map.safeTexture(), 3f)).size(250f).get()
	# .setScaling(Scaling.fit)`: the whole preview, fitted, never clipped.
	var preview := get_node_or_null("/root/Spine/MindPreview")
	var texture: Texture2D = null
	if preview != null and preview.has_method("texture_for_name"):
		texture = preview.call("texture_for_name", _map_name)
	if texture != null:
		# code-instantiated: the preview texture comes from the map registry.
		var image := MindBorderImage.new()
		image.texture = texture
		image.thickness = 3.0
		# `map.safeTexture()` is an Arc `Texture` with the default
		# `TextureFilter.nearest` (Arc `GLTexture.java:18-19`): crisp pixels.
		# `MindBorderImage._ready` resets `texture_filter` from `force_nearest`.
		image.force_nearest = true
		image.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		image.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		image.custom_minimum_size = PREVIEW_SIZE
		root.add(image).width(PREVIEW_SIZE.x).height(PREVIEW_SIZE.y)
	# `if(Gamemode.survival.valid(map)) cont.label(level.highscore).pad(3f)`
	if bool(modes.get("survival", false)):
		root.row()
		root.add(MindWidgets.label(_high_score_text())).pad(3)

	clear_buttons()
	# `addCloseButton()` (210x64) then `buttons.button("@play", Icon.play, ...)
	# .size(210f, 64f)`.
	_add_action_button(_t("@back"), "left", _close_pressed)
	_add_action_button(_t("@play"), "play", func() -> void: _play(_mode, _playtest))


## A 210x64 `Tex.button` action button with an icon (`BaseDialog` buttons row).
func _add_action_button(text_value: String, icon_name: String, listener: Callable) -> Button:
	# code-instantiated: the button row is rebuilt per shown map.
	var button := MindWidgets.button("")
	button.custom_minimum_size = ACTION_BUTTON
	apply_pane_button(button, PANE_OUTLINE_GRAY)
	build_button_content(button, MindWidgets.glyph(icon_name, 24), text_value)
	button.pressed.connect(listener)
	if buttons != null:
		buttons.add_child(button)
	return button


## `Styles.flatToggleMenut` (`Styles.java:210-219`): `up = clear`,
## `over = flatOver` (#454545), `down`/`checked = flatDown` (the accent
## `flat-down-base` frame), `disabled = black` with `disabledFontColor = gray`.
static func _apply_flat_toggle_menu(button: Button) -> void:
	var over := StyleBoxFlat.new()
	over.bg_color = Color(0.271, 0.271, 0.271)
	var checked := StyleBoxFlat.new()
	checked.bg_color = Color.BLACK
	checked.border_color = MindStyles.ACCENT
	checked.set_border_width_all(4)
	checked.set_corner_radius_all(2)
	var disabled := StyleBoxFlat.new()
	disabled.bg_color = Color.BLACK
	button.add_theme_stylebox_override("normal", StyleBoxEmpty.new())
	button.add_theme_stylebox_override("hover", over)
	button.add_theme_stylebox_override("pressed", checked)
	button.add_theme_stylebox_override("hover_pressed", checked)
	button.add_theme_stylebox_override("disabled", disabled)
	button.add_theme_stylebox_override("focus", StyleBoxEmpty.new())
	button.add_theme_color_override("font_color", Color.WHITE)
	button.add_theme_color_override("font_hover_color", Color.WHITE)
	button.add_theme_color_override("font_pressed_color", Color.WHITE)
	button.add_theme_color_override("font_hover_pressed_color", Color.WHITE)
	button.add_theme_color_override("font_disabled_color", MindStyles.ARC_GRAY)


## `Gamemode.valid(map)` per visible mode (`Gamemode.all`, `MindPreview.map_modes`).
func _mode_validity() -> Dictionary:
	var preview := get_node_or_null("/root/Spine/MindPreview")
	if preview != null and preview.has_method("map_modes"):
		var modes: Variant = preview.call("map_modes", _map_name)
		if modes is Dictionary:
			return modes
	return {}


## `map.applyRules(mode)` serialized (`MindPreview.map_rules_json`).
func _mode_rules_json(mode: String) -> String:
	var preview := get_node_or_null("/root/Spine/MindPreview")
	if preview != null and preview.has_method("map_rules_json"):
		return str(preview.call("map_rules_json", _map_name, mode))
	return ""


## `mode.toString()` + the selected index, refreshing the rules (`MapPlayDialog`
## mode button listener).
func _select_mode(mode: String) -> void:
	_mode = mode
	for index in _mode_buttons.size():
		_mode_buttons[index].button_pressed = _button_modes[index] == mode
	# `rules = map.applyRules(mode)` upstream.
	_rules_json = _mode_rules_json(mode)


## `displayGameModeHelp` (`MapPlayDialog.java:91-112`): every visible mode as
## `[accent]<name>:[] [lightgray]<description>`, width 400, `@ok` 110x50.
func _show_mode_help() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("show_text"):
		return
	var lines: PackedStringArray = []
	for mode in MODES:
		lines.append(
			"[accent]%s:[] [lightgray]%s" % [_t("@mode.%s.name" % mode), _t("@mode.%s.description" % mode)]
		)
	var body := "\n".join(lines)
	ui.call("show_text", _t("@mode.help.title"), MindWidgets.markup_text(body))


## `CustomRulesDialog(true).show(rules, () -> rules = map.applyRules(mode))`:
## the dialog edits the working rules; `rules_edited` replaces them.
func _open_rules() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("open_dialog"):
		return
	var current: Variant = JSON.parse_string(_rules_json)
	if not (current is Dictionary):
		current = {}
	var reset: Variant = JSON.parse_string(_mode_rules_json(_mode))
	if not (reset is Dictionary):
		reset = {}
	ui.call(
		"open_dialog",
		"custom_rules",
		JSON.stringify({"rules": current, "reset": reset, "show_rule_edit": false})
	)
	var ui_root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui_root == null or not ui_root.has_method("dialog"):
		return
	var rules_dialog: Node = ui_root.call("dialog", "custom_rules")
	if rules_dialog == null or not rules_dialog.has_signal("rules_edited"):
		return
	if not rules_dialog.is_connected("rules_edited", Callable(self, "_on_rules_edited")):
		rules_dialog.connect("rules_edited", Callable(self, "_on_rules_edited"))


## The edited rules JSON from the Custom Rules dialog.
func _on_rules_edited(json: String) -> void:
	_rules_json = json


## `Core.bundle.format("level.highscore", map.getHighScore())`.
func _high_score_text() -> String:
	var preview := get_node_or_null("/root/Spine/MindPreview")
	var ui := get_node_or_null("/root/MindUi")
	if preview == null or ui == null:
		return _t("@level.highscore")
	if not preview.has_method("map_high_score_key") or not ui.has_method("settings_get_int"):
		return _t("@level.highscore")
	var key := str(preview.call("map_high_score_key", _map_name))
	if key.is_empty():
		return _t("@level.highscore")
	var score := int(ui.call("settings_get_int", key, 0))
	return MindWidgets.markup_text(_t("@level.highscore").format([score]))


## `control.playMap(map, rules, playtesting)` + `ui.custom.hide()`: starts the
## selected map with the chosen gamemode/rules and shows the game view.
func _play(mode: String, playtest: bool) -> void:
	var campaign := get_node_or_null("/root/Spine/MindCampaign")
	var started := false
	if campaign != null and not _map_name.is_empty():
		# `start_map_mode` loads the world, applies the gamemode + custom rules
		# and starts the session in one call (`Control.playMap`).
		started = bool(campaign.call("start_map_mode", _map_name, mode, playtest, _rules_json))
	_close_stack()
	if not started and playtest:
		# `Control.playMap`: "booted out of map, resume editing"
		# (`Control.java:422-428`, `ui.editor.resumeAfterPlaytest(map)`): the
		# editor still holds the map the playtest just saved, so re-show it
		# over the error.
		var shell := get_node_or_null("/root/Spine/Ui/EditorDialog")
		if shell != null and shell.has_method("show_dialog"):
			shell.call("show_dialog")
		return
	if started:
		# `MapEditorDialog.playtest`'s `playListener = this::hide`: a playtest
		# returns to the editor, so hide the shell while the map plays.
		if playtest:
			var editor_dialog := get_node_or_null("/root/Spine/Ui/EditorDialog")
			if editor_dialog != null and editor_dialog.has_method("hide_dialog"):
				editor_dialog.call("hide_dialog")
		var ui_root := get_node_or_null("/root/Spine/Ui/UiRoot")
		if ui_root != null and ui_root.has_method("set_menu_visible"):
			ui_root.call("set_menu_visible", false)


func _close_stack() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		hide_dialog()
		return
	ui.call("close_dialog", "map_play")
	ui.call("close_dialog", "custom")
