## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/editor/SectorGenerateDialog.java
##
## UI-only: the planet picker (generator + non-empty sectors + accessible), the
## sector/seed fields with Java's validity, the `[ NxN ]` size label and Apply
## (`MindEditor.sector_generate` replaces the editor grid with the generated
## terrain + rules).

extends MindDialog

@export var editor_path: NodePath = ^"/root/Spine/MindEditor"

var _editor: Node = null
var _planet := "erekir"
var _sector := 0
var _seed := 0
var _planet_button: Button = null
var _sector_field: LineEdit = null
var _seed_field: LineEdit = null
var _size_label: MindLabel = null


func _ready() -> void:
	set_title_key("@editor.sectorgenerate")
	should_pause = false
	super._ready()
	_editor = get_node_or_null(editor_path)
	clear_buttons()
	# `addCloseButton()` + `buttons.button("@editor.apply", Icon.ok, ...)`.
	add_close_button()
	add_button(_t("@editor.apply"), _apply, "ok", 210.0)


func shown() -> void:
	_build()


## `setup()` (`:23-85`).
func _build() -> void:
	if cont == null:
		return
	for child in cont.get_children():
		cont.remove_child(child)
		child.queue_free()
	var root := content_table()

	# `cont.add("@editor.planet"); cont.button(planet.localizedName, ...)`.
	root.add(_row(_t("@editor.planet"), _planet_picker())).pad(4)
	root.row()
	# `cont.field(sector + "", ...).valid(...)`.
	_sector_field = MindWidgets.field("")
	_sector_field.text = str(_sector)
	_sector_field.custom_minimum_size.x = 200.0
	_sector_field.text_changed.connect(_on_sector_changed)
	root.add(_row(_t("@editor.sector"), _sector_field)).pad(4)
	root.row()
	# `cont.field(seed + "", ...).valid(Strings::canParseInt)`.
	_seed_field = MindWidgets.field("")
	_seed_field.text = str(_seed)
	_seed_field.custom_minimum_size.x = 200.0
	_seed_field.text_changed.connect(func(text: String) -> void:
		if not text.is_empty() and text != "-" and text.is_valid_int():
			_seed = int(text))
	root.add(_row(_t("@editor.seed"), _seed_field)).pad(4)
	root.row()
	# `cont.label(() -> "[ " + size + "x" + size + " ]").color(Pal.accent)`.
	_size_label = MindWidgets.label("")
	_size_label.add_theme_color_override("font_color", MindStyles.ACCENT)
	_size_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	root.add(_size_label).pad(5)
	_refresh_size()


func _row(label_text: String, control: Control) -> Control:
	# code-instantiated: one dialog row (planet/sector/seed), data-driven.
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 10)
	row.add_child(MindWidgets.label(label_text))
	row.add_child(control)
	return row


## `cont.button(planet.localizedName, ...)`: the `BaseDialog` planet list
## (`:33-57`), built as a dialog-local overlay with 4 toggle columns.
func _planet_picker() -> Button:
	_planet_button = MindWidgets.button(_planet_name())
	_planet_button.custom_minimum_size = Vector2(200.0, 40.0)
	_planet_button.pressed.connect(_open_planet_picker)
	return _planet_button


func _open_planet_picker() -> void:
	var parsed: Variant = JSON.parse_string(str(_editor.call("sector_planets_json")))
	if not (parsed is Array):
		return
	# code-instantiated: per-action picker panels have no static scene.
	var overlay := ColorRect.new()
	overlay.name = "PlanetPicker"
	overlay.color = Color(0, 0, 0, 0.8)
	overlay.set_anchors_preset(Control.PRESET_FULL_RECT)
	overlay.mouse_filter = Control.MOUSE_FILTER_STOP
	add_child(overlay)
	var center := CenterContainer.new()
	center.set_anchors_preset(Control.PRESET_FULL_RECT)
	center.mouse_filter = Control.MOUSE_FILTER_IGNORE
	overlay.add_child(center)
	var panel := PanelContainer.new()
	panel.theme_type_variation = "defaultDialog"
	center.add_child(panel)
	var box := VBoxContainer.new()
	panel.add_child(box)
	var grid := GridContainer.new()
	grid.columns = 4
	grid.add_theme_constant_override("h_separation", 4)
	grid.add_theme_constant_override("v_separation", 4)
	box.add_child(grid)
	for value in parsed:
		var row: Dictionary = value
		var planet_name := str(row.get("name", ""))
		var button := MindWidgets.button(_planet_label(planet_name))
		button.theme_type_variation = "flatTogglet"
		button.custom_minimum_size = Vector2(110.0, 45.0)
		button.toggle_mode = true
		button.button_pressed = planet_name == _planet
		button.pressed.connect(_select_planet.bind(planet_name, overlay))
		grid.add_child(button)
	var close := MindWidgets.button(_t("@back"))
	close.pressed.connect(overlay.queue_free)
	box.add_child(close)


## `planet = plan; sector = min(sector, sectors.size - 1); seed = 0`.
func _select_planet(planet_name: String, overlay: Control) -> void:
	_planet = planet_name
	var sectors := _planet_sector_count()
	if sectors > 0:
		_sector = mini(_sector, sectors - 1)
	_seed = 0
	overlay.queue_free()
	_build()


## `valid(text -> planet.sectors.size > parseInt(text, 99999) && parseInt(text, 9999) >= 0)`.
func _on_sector_changed(text: String) -> void:
	if not text.is_valid_int():
		return
	var value := int(text)
	var sectors := _planet_sector_count()
	if value < 0 or (sectors > 0 and value >= sectors):
		return
	_sector = value
	_refresh_size()


func _refresh_size() -> void:
	if _size_label == null or _editor == null:
		return
	var sector_size := int(_editor.call("sector_size", _planet, _sector))
	_size_label.text = "[ %dx%d ]" % [sector_size, sector_size]


## `ui.loadAnd(() -> { apply(); hide(); })` + `editor.tags.put("genfilters", "{}")`
## (`SectorGenerateDialog.java:79-110`): the loading overlay is up while the
## sector generates.
func _apply() -> void:
	var ui := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui != null and ui.has_method("load_and"):
		ui.call("load_and", _apply_now)
	else:
		_apply_now()


func _apply_now() -> void:
	if _editor == null or not bool(_editor.call("sector_generate", _planet, _sector, _seed)):
		_notify("@editor.sectorgenerate")
		return
	_close_pressed()


func _planet_sector_count() -> int:
	var parsed: Variant = JSON.parse_string(str(_editor.call("sector_planets_json")))
	if parsed is Array:
		for value in parsed:
			var row: Dictionary = value
			if str(row.get("name", "")) == _planet:
				return int(row.get("sectors", 0))
	return 0


func _planet_name() -> String:
	return _planet_label(_planet)


func _planet_label(planet: String) -> String:
	var ui := get_node_or_null("/root/MindAssets")
	if ui != null and ui.has_method("bundle_get"):
		return str(ui.call("bundle_get", "planet.%s.name" % planet))
	return planet


func _notify(key: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("toast"):
		ui.call("toast", _t(key), "error")
