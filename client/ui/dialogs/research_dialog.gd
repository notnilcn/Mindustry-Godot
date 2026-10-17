## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/ResearchDialog.java.
##
## The Java graph view over the T1a `research_tree_json`: a `MindResearchView`
## canvas (60 px node buttons at the `treeLayout` positions, connectors,
## pan/zoom), a root-icon title button opening the tree selector when more than
## one root is selectable, click-to-research on locked spendable nodes, and the
## `@database` action. The title table is removed (`super("")`).

extends MindDialog

var _view: MindResearchView = null
var _title_button: Button = null
var _title_icon_holder: Control = null
var _title_label: MindLabel = null
var _items_display: MindResearchItemsDisplay = null
var _popup: PanelContainer = null
var _hovered := -1
var _tree: Dictionary = {}
var _last_root := ""

## `iconMed` (`Scl.scl(32f)`) — title icon, root rows and the balancing spacer.
const ICON_SIZE := 32.0


func _ready() -> void:
	# Java: `super("")` + `titleTable.remove()`.
	title_text = ""
	should_pause = true
	# Java `BaseDialog`: `setFillParent(true)` + `margin(0f).marginBottom(8)`.
	# ResearchDialog is a plain BaseDialog (black9 Dim + `defaultDialog`), so it
	# fills the viewport without switching to the `full_dialog` style.
	super._ready()
	fill_panel()
	# `BaseDialog` sets no background (`BaseDialog.java`): Java's ResearchDialog
	# renders straight over the (dimmed) planet page. The shared panel's
	# `defaultDialog` style would paint an opaque slate over the globe, so clear
	# the panel background and keep the dim (java/40-tech.png).
	var panel := get_node_or_null("Center/Panel") as Control
	if panel != null:
		# `margin(0f).marginBottom(8)` plus the buttons cell pad(3) and its
		# buttons' pad(3) (Dialog.java:211-214): the button row's bottom edge is
		# at 648 - 15 in java/research.png (PAR-0428; the port's touched 648).
		var empty := StyleBoxEmpty.new()
		empty.content_margin_bottom = 15.0
		panel.add_theme_stylebox_override("panel", empty)
	_build()
	add_close_button(210.0)
	add_button(_t("@database"), _open_database, "book", 210.0)


func _build() -> void:
	var layout := get_node_or_null("Center/Panel/Layout")
	if layout == null:
		return
	# code-instantiated: the tree canvas replaces the generic scroll body
	# (Java `cont.stack(titleTable, view, itemDisplay)`).
	_view = MindResearchView.new()
	_view.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_view.size_flags_vertical = Control.SIZE_EXPAND_FILL
	layout.add_child(_view)
	_view.node_pressed.connect(_on_node_pressed)
	_view.node_hovered.connect(_on_node_hovered)
	_view.node_exited.connect(_on_node_exited)
	# code-instantiated: root-selector title button (Java `titleTable`, a stack
	# layer over the canvas; `titleTable.toFront()` keeps it on top).
	_title_button = Button.new()
	_title_button.theme_type_variation = "defaultt"
	_title_button.visible = false
	_title_button.pressed.connect(_open_root_selector)
	_view.add_child(_title_button)
	_title_button.set_anchors_preset(Control.PRESET_CENTER_TOP)
	_title_button.offset_left = -150.0
	_title_button.offset_right = 150.0
	_title_button.offset_top = 4.0
	_title_button.offset_bottom = 64.0
	_build_title_row()
	# code-instantiated: the global item pool overlays the canvas top-left
	# (Java `cont.stack(titleTable, view, itemDisplay)`).
	_items_display = MindResearchItemsDisplay.new()
	# The stack sits in `cont` under the Dialog's and cont's `pad(3)` cells
	# (Dialog.java:209-213): the items box is at (6, 6) in java/research.png.
	_items_display.position = Vector2(6.0, 6.0)
	_view.add_child(_items_display)
	if body != null:
		# The canvas is not a scroll body; keep it out of the ScrollContainer
		# (it must receive drags) and above the hidden Body in the VBox.
		var body_slot := body.get_index()
		layout.move_child(_view, body_slot)
		body.visible = false


func shown() -> void:
	# Java `shown`: the planet page's planet, else the played sector's planet.
	_tree = campaign_json("research_tree_json", [str(_context.get("planet", ""))])
	if _tree.is_empty():
		return
	_view.set_tree(_tree)
	# The pool overlay draws above the node buttons; `titleTable.toFront()` draws
	# the title above everything.
	_view.move_child(_items_display, -1)
	_view.move_child(_title_button, -1)
	_items_display.max_height = maxf(60.0, _view.size.y - 120.0)
	_items_display.rebuild(_tree.get("items", []))
	var root_name := str(_tree.get("root", ""))
	if root_name != _last_root:
		# Java keeps pan between opens of the same tree, resets on a switch.
		_view.reset_view()
		_last_root = root_name
	_update_title_button()
	# `ResearchDialog` ctor: the `titleTable` root button is always present
	# (its listener opens the root selector; Java filters the rows by unlock).
	_title_button.visible = not str(_tree.get("root", "")).is_empty()


func hidden() -> void:
	# Java `hidden(ui.planet::setup)`: refresh the planet page underneath.
	var ui := get_node_or_null("/root/MindUi")
	if ui == null or not ui.has_method("is_dialog_shown"):
		return
	if not bool(ui.call("is_dialog_shown", "planet")):
		return
	var root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if root == null or not root.has_method("dialog"):
		return
	var planet: Variant = root.call("dialog", "planet")
	if planet is Node and (planet as Node).has_method("refresh_page"):
		(planet as Node).call_deferred("refresh_page")


## `titleTable.button` content (`ResearchDialog.java` ctor): 32 px root icon,
## `padRight(8)`, a grow spacer, the accent label, another grow spacer and a
## 32 px spacer so the label sits centred; built once and retargeted per tree.
func _build_title_row() -> void:
	# code-instantiated: title row content is tree-driven.
	var row := HBoxContainer.new()
	row.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	row.alignment = BoxContainer.ALIGNMENT_CENTER
	row.add_theme_constant_override("separation", 0)
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_title_button.add_child(row)
	# code-instantiated: root icon follows the tree data (atlas region or the
	# `terrain` glyph for a sector root).
	_title_icon_holder = Control.new()
	_title_icon_holder.custom_minimum_size = Vector2(ICON_SIZE, ICON_SIZE)
	_title_icon_holder.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(_title_icon_holder)
	row.add_child(_title_spacer(8.0))
	row.add_child(_title_spacer(-1.0))
	_title_label = MindWidgets.label("")
	_title_label.add_theme_color_override("default_color", MindStyles.ACCENT)
	_title_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(_title_label)
	row.add_child(_title_spacer(-1.0))
	row.add_child(_title_spacer(ICON_SIZE))


## Retargets the title button on the displayed tree.
func _update_title_button() -> void:
	if _title_icon_holder == null or _title_label == null:
		return
	for child in _title_icon_holder.get_children():
		child.queue_free()
	var icon := MindWidgets.content_icon(
		str(_tree.get("root_icon", "")), str(_tree.get("root_type", "")), ICON_SIZE
	)
	if icon != null:
		icon.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
		_title_icon_holder.add_child(icon)
	_title_label.text = str(_tree.get("root_title", ""))


## Spacer cell for the title button row (`b.add().growX()` = -1).
func _title_spacer(size: float) -> Control:
	# code-instantiated: title row is data-driven (Java `b.add()` cells).
	var spacer := Control.new()
	spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
	if size < 0.0:
		spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	else:
		spacer.custom_minimum_size = Vector2(size, 0.0)
	return spacer


## `titleTable.button` click: the `@techtree.select` BaseDialog (`ResearchDialog.java`
## ctor) — one 300x60 `flatTogglet` icon+name row per selectable root.
func _open_root_selector() -> void:
	var roots: Array = _tree.get("roots", [])
	if roots.size() <= 1:
		return
	var ui := get_node_or_null("/root/MindUi")
	var root := get_node_or_null("/root/Spine/Ui/UiRoot")
	if ui == null or root == null:
		return
	var selector: Variant = root.call("dialog", "research_root_select")
	if selector is Node and (selector as Node).has_signal("root_selected"):
		if not (selector as Node).is_connected("root_selected", _select_root):
			(selector as Node).connect("root_selected", _select_root)
	if not bool(ui.call("open_dialog", "research_root_select", JSON.stringify({
		"roots": roots, "selected": str(_tree.get("root", "")),
	}))):
		return


## `rebuildTree(node)`: switch tree, reset the view.
func _select_root(root_name: String) -> void:
	if root_name.is_empty():
		return
	if root_name == _last_root:
		_update_title_button()
		return
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("close_dialog"):
		ui.call("close_dialog", "research_root_select")
	_tree = campaign_json("research_tree_json", [root_name])
	if _tree.is_empty():
		return
	_view.set_tree(_tree)
	_view.reset_view()
	_last_root = str(_tree.get("root", root_name))
	_update_title_button()


## Desktop click (`ResearchDialog.java:463-487`): `if(canSpend && locked) spend`.
func _on_node_pressed(index: int) -> void:
	var node := _view.node_at(index)
	if node.is_empty():
		return
	var content := str(node.get("content", ""))
	if content.is_empty():
		return
	if not bool(node.get("locked", false)) or not bool(node.get("can_spend", false)):
		return
	var campaign := campaign_node()
	if campaign == null or not campaign.has_method("research"):
		return
	if not bool(campaign.call("research", content)):
		return
	# Requirement amounts that changed flash (`ResearchDialog.spend` shine).
	var previous_have := {}
	for req_variant in (node.get("reqs", []) as Array):
		var req: Dictionary = req_variant
		previous_have[str(req.get("item", ""))] = str(req.get("have", ""))
	var was_locked := bool(node.get("locked", true))
	# Lock state and layout positions change after a spend (Java `treeLayout`
	# runs from `unlock`); re-fetch the same tree.
	_tree = campaign_json("research_tree_json", [_last_root])
	if _tree.is_empty():
		return
	var shine := {}
	var unlocked_now := false
	for node_variant in (_tree.get("nodes", []) as Array):
		var refreshed: Dictionary = node_variant
		if str(refreshed.get("content", "")) != content:
			continue
		unlocked_now = not bool(refreshed.get("locked", true))
		for req_variant in (refreshed.get("reqs", []) as Array):
			var req: Dictionary = req_variant
			var item_name := str(req.get("item", ""))
			if previous_have.has(item_name) and previous_have[item_name] != str(req.get("have", "")):
				shine[item_name] = true
		break
	_view.set_tree(_tree)
	_view.move_child(_items_display, -1)
	_view.move_child(_title_button, -1)
	_items_display.rebuild(_tree.get("items", []), shine)
	_update_title_button()
	if _hovered >= 0:
		_rebuild_popup(shine)
	if was_locked and unlocked_now:
		# Java `unlock`: `Sounds.uiUnlock.play()` (ResearchDialog.java:623).
		_play_ui_sound("uiUnlock")


## `ResearchDialog.java:490-501`: hover shows the info table for visible nodes.
func _on_node_hovered(index: int) -> void:
	var node := _view.node_at(index)
	if node.is_empty() or not bool(node.get("visible", true)):
		return
	_hovered = index
	_rebuild_popup({})


func _on_node_exited(index: int) -> void:
	if _hovered == index:
		call_deferred("_maybe_clear_popup")


## Keep the popup while the mouse is over it or the node button; else remove.
func _maybe_clear_popup() -> void:
	if _hovered < 0:
		return
	var mouse := get_viewport().get_mouse_position()
	if _popup != null and _popup.get_global_rect().has_point(mouse):
		return
	if _view.button_rect(_hovered).has_point(mouse):
		return
	_clear_popup()


func _clear_popup() -> void:
	_hovered = -1
	_free_popup()


func _free_popup() -> void:
	if _popup != null:
		_popup.queue_free()
		_popup = null


## `View.rebuild` (ResearchDialog.java:632-784): the hover info table.
func _rebuild_popup(shine: Dictionary) -> void:
	_free_popup()
	if _hovered < 0:
		return
	var node := _view.node_at(_hovered)
	if node.is_empty():
		return
	var selectable := bool(node.get("selectable", false))
	var locked := bool(node.get("locked", true))
	# code-instantiated: hover info table (Java `infoTable`).
	var popup := PanelContainer.new()
	var box := StyleBoxTexture.new()
	box.texture = MindWidgets.icon_texture("button")
	box.texture_margin_left = 8.0
	box.texture_margin_top = 8.0
	box.texture_margin_right = 8.0
	box.texture_margin_bottom = 8.0
	popup.add_theme_stylebox_override("panel", box)
	popup.mouse_filter = Control.MOUSE_FILTER_STOP
	popup.mouse_exited.connect(func() -> void: call_deferred("_maybe_clear_popup"))
	_view.add_child(popup)
	_popup = popup

	# code-instantiated: info row + description column (Java `infoTable.table`).
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 4)
	popup.add_child(column)
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 6)
	column.add_child(row)
	if selectable:
		# code-instantiated: `Icon.info` button (`ui.content.show`).
		var info := MindWidgets.glyph_button("info", "i", "flati")
		info.custom_minimum_size = Vector2(50.0, 0.0)
		info.pressed.connect(open_content_info.bind(str(node.get("content", ""))))
		row.add_child(info)
	# code-instantiated: description column (`rebuild(shine)` desc table).
	var desc := VBoxContainer.new()
	desc.add_theme_constant_override("separation", 2)
	row.add_child(desc)
	var name_label := MindWidgets.label(
		str(node.get("name", "")) if selectable else "???"
	)
	if not selectable:
		name_label.add_theme_color_override("default_color", MindStyles.ACCENT)
	desc.add_child(name_label)
	if locked:
		if selectable:
			_build_requirement_rows(desc, node, shine)
		elif not (node.get("objectives", []) as Array).is_empty():
			# code-instantiated: incomplete objective rows (`@complete` block).
			desc.add_child(MindWidgets.label(MindWidgets.markup("@complete")))
			for objective_variant in (node.get("objectives", []) as Array):
				var objective_row := HBoxContainer.new()
				objective_row.add_theme_constant_override("separation", 3)
				var objective_label := MindWidgets.label("> " + str(objective_variant))
				objective_label.add_theme_color_override("default_color", Color(0.75, 0.75, 0.75))
				objective_row.add_child(objective_label)
				objective_row.add_child(
					MindWidgets.glyph("cancel", 18, Color(1.0, 0.204, 0.11))
				)
				desc.add_child(objective_row)
	elif not locked:
		desc.add_child(MindWidgets.label(MindWidgets.markup("@completed")))
	# `rebuild`: description wrap 270 px when longer than 20 chars.
	var description := str(node.get("description", ""))
	if selectable and bool(node.get("inline_description", false)) and not description.is_empty():
		var description_label := MindWidgets.label(description)
		description_label.add_theme_color_override("default_color", Color(0.75, 0.75, 0.75))
		if description.length() > 20:
			description_label.custom_minimum_size.x = 270.0
			description_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		column.add_child(description_label)
	_update_popup_position()


## `ResearchDialog.java:682-736`: progress line + one row per unfinished stack.
func _build_requirement_rows(desc: VBoxContainer, node: Dictionary, shine: Dictionary) -> void:
	var progress := int(node.get("progress", -1))
	if progress >= 0:
		var progress_label := MindWidgets.label(
			MindWidgets.markup_format("@research.progress", [progress])
		)
		progress_label.add_theme_color_override("default_color", Color(0.75, 0.75, 0.75))
		desc.add_child(progress_label)
	for req_variant in (node.get("reqs", []) as Array):
		var req: Dictionary = req_variant
		var item_name := str(req.get("item", ""))
		var has := bool(req.get("has", false))
		var target := Color(0.75, 0.75, 0.75) if has else Color(1.0, 0.204, 0.11)
		# code-instantiated: one requirement row (`list.image` + labels).
		var req_row := HBoxContainer.new()
		req_row.add_theme_constant_override("separation", 3)
		var icon := TextureRect.new()
		icon.texture = MindWidgets.icon_texture("item-%s-ui" % item_name)
		icon.custom_minimum_size = Vector2(24.0, 24.0)
		icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
		icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		req_row.add_child(icon)
		var item_label := MindWidgets.label(str(req.get("name", item_name)))
		item_label.add_theme_color_override("default_color", Color(0.75, 0.75, 0.75))
		req_row.add_child(item_label)
		var amount_label := MindWidgets.label(
			" %s / %s" % [str(req.get("have", "0")), str(req.get("need", "0"))]
		)
		amount_label.add_theme_color_override("default_color", target)
		if shine.has(item_name):
			# `Actions.color(Pal.accent -> targetColor, 0.75s)`.
			amount_label.modulate = MindStyles.ACCENT
			var tween := amount_label.create_tween()
			tween.tween_property(amount_label, "modulate", Color.WHITE, 0.75)
		req_row.add_child(amount_label)
		desc.add_child(req_row)


func _update_popup_position() -> void:
	if _popup == null or _hovered < 0:
		return
	# Java places it at the button's top-right (`Align.topLeft`, y-up): Godot
	# top-right corner = (right, top). Controls are unrotated/unscaled here, so
	# view-local = global - view.global_position.
	var rect := _view.button_rect(_hovered)
	var corner := Vector2(rect.position.x + rect.size.x, rect.position.y)
	_popup.position = corner - _view.global_position


func _process(_delta: float) -> void:
	_update_popup_position()


## Emit-only UI sound (`G07` owns playback wiring).
func _play_ui_sound(sound_name: String) -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null and ui.has_method("play_ui_sound"):
		ui.call("play_ui_sound", sound_name)


## Java: hide research, show the database.
func _open_database() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	ui.call("close_dialog", "research")
	ui.call("open_dialog", "database", "")
