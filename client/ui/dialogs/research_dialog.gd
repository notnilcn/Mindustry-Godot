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
var _root_selector: PopupPanel = null
var _tree: Dictionary = {}
var _last_root := ""


func _ready() -> void:
	# Java: `super("")` + `titleTable.remove()`.
	title_text = ""
	should_pause = true
	full_dialog = true
	super._ready()
	_build()
	add_close_button(210.0)
	add_button(_t("@database"), _open_database, "book", 210.0)


func _build() -> void:
	var layout := get_node_or_null("Center/Panel/Layout")
	if layout == null:
		return
	# code-instantiated: root-selector title button (Java `titleTable`).
	_title_button = Button.new()
	_title_button.theme_type_variation = "flatTogglet"
	_title_button.custom_minimum_size = Vector2(300.0, 60.0)
	_title_button.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	_title_button.visible = false
	_title_button.pressed.connect(_open_root_selector)
	layout.add_child(_title_button)
	# code-instantiated: the tree canvas replaces the generic scroll body
	# (Java `cont.stack(titleTable, view, itemDisplay)`).
	_view = MindResearchView.new()
	_view.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_view.size_flags_vertical = Control.SIZE_EXPAND_FILL
	layout.add_child(_view)
	_view.node_pressed.connect(_on_node_pressed)
	# code-instantiated: transient root selector (Java BaseDialog; a popup
	# keeps the manifest dialog inventory unchanged).
	_root_selector = PopupPanel.new()
	_root_selector.hide()
	add_child(_root_selector)
	if body != null:
		# The canvas is not a scroll body; keep it out of the ScrollContainer
		# (it must receive drags): title first, then the view, above the hidden
		# Body in the VBox (Java `cont.stack(titleTable, view, itemDisplay)`).
		var body_slot := body.get_index()
		layout.move_child(_title_button, body_slot)
		layout.move_child(_view, body_slot + 1)
		body.visible = false


func shown() -> void:
	# Java `shown`: the planet page's planet, else the played sector's planet.
	_tree = campaign_json("research_tree_json", [str(_context.get("planet", ""))])
	if _tree.is_empty():
		return
	_view.set_tree(_tree)
	var root_name := str(_tree.get("root", ""))
	if root_name != _last_root:
		# Java keeps pan between opens of the same tree, resets on a switch.
		_view.reset_view()
		_last_root = root_name
	_update_title_button()
	_title_button.visible = (_tree.get("roots", []) as Array).size() > 1


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


## Title content: root icon + accent `techtree.<root>` label.
func _update_title_button() -> void:
	for child in _title_button.get_children():
		child.queue_free()
	# code-instantiated: title row content is tree-driven.
	var row := HBoxContainer.new()
	row.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	row.alignment = BoxContainer.ALIGNMENT_CENTER
	row.add_theme_constant_override("separation", 8)
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_title_button.add_child(row)
	# code-instantiated: root icon follows the tree data.
	var icon := TextureRect.new()
	icon.texture = MindWidgets.icon_texture(str(_tree.get("root_icon", "")))
	icon.custom_minimum_size = Vector2(32.0, 32.0)
	icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(icon)
	var label := MindWidgets.label(str(_tree.get("root_title", "")))
	label.add_theme_color_override("default_color", MindStyles.ACCENT)
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	row.add_child(label)


## `titleTable.button` click: 300x60 `flatTogglet` rows per selectable root.
func _open_root_selector() -> void:
	var roots: Array = _tree.get("roots", [])
	if roots.size() <= 1:
		return
	for child in _root_selector.get_children():
		child.queue_free()
	# code-instantiated: root rows come from the tree JSON roots list.
	var table := VBoxContainer.new()
	table.add_theme_constant_override("separation", 0)
	_root_selector.add_child(table)
	for root_variant in roots:
		var entry: Dictionary = root_variant
		# code-instantiated: one toggle button per selectable root.
		var row := Button.new()
		row.text = str(entry.get("title", entry.get("name", "")))
		row.theme_type_variation = "flatTogglet"
		row.toggle_mode = true
		row.button_pressed = bool(entry.get("selected", false))
		row.custom_minimum_size = Vector2(300.0, 60.0)
		row.pressed.connect(_select_root.bind(str(entry.get("name", ""))))
		table.add_child(row)
	var rect := _title_button.get_global_rect()
	_root_selector.popup(Rect2i(Vector2i(rect.position), Vector2i(rect.size)))


## `rebuildTree(node)`: switch tree, reset the view.
func _select_root(root_name: String) -> void:
	if root_name.is_empty():
		return
	_root_selector.hide()
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
	# Lock state and layout positions change after a spend (Java `treeLayout`
	# runs from `unlock`); re-fetch the same tree.
	_tree = campaign_json("research_tree_json", [_last_root])
	if _tree.is_empty():
		return
	_view.set_tree(_tree)
	_update_title_button()


## Java: hide research, show the database.
func _open_database() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	ui.call("close_dialog", "research")
	ui.call("open_dialog", "database", "")
