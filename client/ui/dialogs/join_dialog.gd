## SPDX-License-Identifier: GPL-3.0-only
## Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
## Source: core/src/mindustry/ui/dialogs/JoinDialog.java (plan 14 §3.4, M3).
##
## Direct-connect + server list. Local rows come from the dialog context; the
## global section lists the public `all_matches` rows through `MindNet`, whose
## `connect_to`/`join_match` calls `UiRoot` wires from the two intents below.

extends MindDialog

signal connect_requested(address: String)
signal join_requested(match_id: int)

## Player color shown in the header swatch (upstream `player.color()`).
var _color := Color.WHITE
var _table: MindTable = null
var _name_field: LineEdit = null
var _swatch_tint: ColorRect = null
var _search_field: LineEdit = null
var _add_panel: Control = null
var _ip_field: LineEdit = null
var _local_list: MindTable = null
var _global_list: MindTable = null


func _ready() -> void:
	set_title_key("@joingame")
	should_pause = false
	full_dialog = true
	super._ready()
	_build()


func _build() -> void:
	_table = content_table()
	_table.add_theme_constant_override("separation", 8)
	_build_header()
	_build_add_panel()
	_build_sections()
	add_button(_t("@back"), _close, "left", 210.0)
	add_button(_t("@server.add"), _toggle_add, "add")
	add_button("?", _show_info, "", 60.0)


## Header row: `@name` label + name field + square color swatch (`JoinDialog.setup`).
func _build_header() -> void:
	# code-instantiated: the name/color header row is data-driven and has no
	# static scene counterpart.
	var header := HBoxContainer.new()
	header.add_theme_constant_override("separation", 6)
	header.add_child(MindWidgets.label(_t("@name")))
	_name_field = MindWidgets.field(_t("@name"))
	_name_field.max_length = 40
	_name_field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	header.add_child(_name_field)
	# code-instantiated: the square color swatch is tinted at runtime by the
	# color chosen in the `picker` dialog.
	var swatch := Button.new()
	swatch.theme_type_variation = "squarei"
	swatch.custom_minimum_size = Vector2(54, 54)
	_swatch_tint = ColorRect.new()
	_swatch_tint.color = _color
	_swatch_tint.mouse_filter = Control.MOUSE_FILTER_IGNORE
	swatch.add_child(_swatch_tint)
	_swatch_tint.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	swatch.pressed.connect(_open_picker)
	header.add_child(swatch)
	_table.add(header).grow_x_axis().pad(4)
	_table.row()


## Inline add-server form, revealed by the `@server.add` button (`JoinDialog.add`).
func _build_add_panel() -> void:
	# code-instantiated: the inline add-server form is revealed by `@server.add`
	# and toggled at runtime.
	_add_panel = VBoxContainer.new()
	_add_panel.visible = false
	_add_panel.add_theme_constant_override("separation", 4)
	var ip_row := HBoxContainer.new()
	ip_row.add_theme_constant_override("separation", 6)
	ip_row.add_child(MindWidgets.label(_t("@joingame.ip")))
	_ip_field = MindWidgets.field(_t("@joingame.ip"))
	_ip_field.max_length = 100
	_ip_field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	ip_row.add_child(_ip_field)
	_add_panel.add_child(ip_row)
	var actions := HBoxContainer.new()
	actions.add_theme_constant_override("separation", 6)
	var ok := MindWidgets.button(_t("@ok"))
	ok.pressed.connect(_submit_add)
	actions.add_child(ok)
	var cancel := MindWidgets.button(_t("@cancel"))
	cancel.pressed.connect(_hide_add)
	actions.add_child(cancel)
	_add_panel.add_child(actions)
	_table.add(_add_panel).grow_x_axis().pad(4)
	_table.row()


## The three server groups (`JoinDialog.section`): local, remote, global.
func _build_sections() -> void:
	var local := _add_section("@servers.local", false)
	# code-instantiated: local discovery rows are data-driven from the plan-21
	# host list.
	_local_list = MindTable.new()
	_local_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	local.add_child(_local_list)
	_rebuild_local()

	var remote := _add_section("@servers.remote", false)
	# code-instantiated: placeholder for the persisted-server list once loaded.
	remote.add_child(MindWidgets.space(0, 4))

	var global := _add_section("@servers.global", true)
	# code-instantiated: public match rows come from the `all_matches` view via
	# `MindNet.get_public_matches_json`; rebuilt on every `shown()`.
	_global_list = MindTable.new()
	_global_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	global.add_child(_global_list)
	_rebuild_public()


## Builds one section header (accent title + collapse glyph, optional search row)
## and returns its collapsible body container.
func _add_section(label_key: String, with_search: bool) -> Control:
	# code-instantiated: section header + collapsible body repeat for the three
	# server groups and are toggled at runtime.
	var body := VBoxContainer.new()
	body.add_theme_constant_override("separation", 4)
	var header := HBoxContainer.new()
	header.add_theme_constant_override("separation", 4)
	var title := MindWidgets.styled_label(_tm(label_key), "outlineLabel")
	title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	title.horizontal_alignment = HORIZONTAL_ALIGNMENT_LEFT
	title.add_theme_color_override("default_color", MindStyles.ACCENT)
	header.add_child(title)
	var toggle := Button.new()
	toggle.theme_type_variation = "emptyi"
	toggle.custom_minimum_size = Vector2(40, 40)
	# code-instantiated: the collapse glyph swaps between up-open/down-open.
	var toggle_glyph := MindWidgets.glyph("up-open", 22, MindStyles.ACCENT)
	toggle.add_child(toggle_glyph)
	toggle.pressed.connect(_toggle_section.bind(body, toggle_glyph))
	header.add_child(toggle)
	_table.add(header).grow_x_axis().pad(4)
	_table.row()

	if with_search:
		# code-instantiated: the community search/refresh row is data-driven.
		var search_row := HBoxContainer.new()
		search_row.add_theme_constant_override("separation", 6)
		search_row.add_child(MindWidgets.label(_t("@search")))
		_search_field = MindWidgets.field(_t("@search"))
		_search_field.max_length = 50
		_search_field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		search_row.add_child(_search_field)
		var zoom := MindWidgets.icon_button("zoom", "", "emptyi")
		zoom.custom_minimum_size = Vector2(54, 54)
		search_row.add_child(zoom)
		_table.add(search_row).grow_x_axis().pad(4)
		_table.row()

	_table.add(body).grow_x_axis().pad(4)
	_table.row()
	return body


func _toggle_section(body: Control, glyph: Label) -> void:
	body.visible = not body.visible
	glyph.text = MindIcons.glyph("up-open" if body.visible else "down-open")


func shown() -> void:
	_rebuild_local()
	_rebuild_public()


## Local group body: the discovered hosts, or the `@hosts.none` empty state with
## a refresh control (`JoinDialog.finishLocalHosts`).
func _rebuild_local() -> void:
	if _local_list == null:
		return
	_local_list.clear_children()
	var servers: Array = _context.get("servers", [])
	if servers.is_empty():
		# code-instantiated: empty-state row carries the refresh control.
		var empty := HBoxContainer.new()
		empty.add_theme_constant_override("separation", 6)
		var empty_label := MindWidgets.styled_label(_tm("@hosts.none"), "outlineLabel")
		# Without autowrap the rich label reports its text width, so it is not
		# squeezed to zero when the parent cell is momentarily narrow.
		empty_label.autowrap_mode = TextServer.AUTOWRAP_OFF
		empty_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		empty.add_child(empty_label)
		var refresh := MindWidgets.icon_button("refresh", "", "emptyi")
		refresh.custom_minimum_size = Vector2(70, 70)
		refresh.pressed.connect(_rebuild_local)
		empty.add_child(refresh)
		_local_list.add(empty).grow_x_axis().pad(4)
		_local_list.row()
		_local_list.sort_now()
		_local_list.call_deferred("sort_now")
		return
	for entry in servers:
		# code-instantiated: remembered server rows come from the plan-21 list.
		var button := MindWidgets.button(str(entry))
		button.pressed.connect(_connect_to.bind(str(entry)))
		_local_list.add(button).grow_x_axis().pad(3)
		_local_list.row()
	_local_list.sort_now()
	_local_list.call_deferred("sort_now")


## Global group body: the public `all_matches` rows (`MindNet` browser), or the
## `@hosts.none` empty state with a refresh control (`JoinDialog.finishLocalHosts`).
func _rebuild_public() -> void:
	if _global_list == null:
		return
	_global_list.clear_children()
	var matches := _public_matches()
	if matches.is_empty():
		# code-instantiated: empty-state row carries the refresh control.
		var empty := HBoxContainer.new()
		empty.add_theme_constant_override("separation", 6)
		var empty_label := MindWidgets.styled_label(_tm("@hosts.none"), "outlineLabel")
		empty_label.autowrap_mode = TextServer.AUTOWRAP_OFF
		empty_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		empty.add_child(empty_label)
		var refresh := MindWidgets.icon_button("refresh", "", "emptyi")
		refresh.custom_minimum_size = Vector2(70, 70)
		refresh.pressed.connect(_rebuild_public)
		empty.add_child(refresh)
		_global_list.add(empty).grow_x_axis().pad(4)
		_global_list.row()
		_global_list.sort_now()
		_global_list.call_deferred("sort_now")
		return
	for row_variant in matches:
		# code-instantiated: public match rows come from the server view.
		var row: Dictionary = row_variant
		var match_id := int(row.get("match_id", 0))
		if match_id <= 0:
			continue
		var label := "%s  %s  %d/%d" % [
			str(row.get("map_id", "")),
			str(row.get("mode_name", "")),
			int(row.get("player_count", 0)),
			int(row.get("max_players", 0)),
		]
		var button := MindWidgets.button(label)
		button.pressed.connect(_join.bind(match_id))
		_global_list.add(button).grow_x_axis().pad(3)
		_global_list.row()
	_global_list.sort_now()
	_global_list.call_deferred("sort_now")


## Live `all_matches` rows from `MindNet` (empty when absent or offline).
func _public_matches() -> Array:
	var net := get_node_or_null("/root/Spine/MindNet")
	if net == null or not net.has_method("get_public_matches_json"):
		return []
	var parsed: Variant = JSON.parse_string(str(net.call("get_public_matches_json")))
	return parsed if parsed is Array else []


## `JoinDialog.safeConnect` intent: join the picked public match.
func _join(match_id: int) -> void:
	join_requested.emit(match_id)
	_close()


func _open_picker() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui == null:
		return
	ui.call("open_dialog", "picker", "{}")
	var picker := get_tree().root.find_child("picker", true, false)
	if picker != null and picker.has_signal("color_selected") and not picker.is_connected("color_selected", _on_color_selected):
		picker.connect("color_selected", _on_color_selected)


func _on_color_selected(color: Color) -> void:
	_color = color
	if _swatch_tint != null:
		_swatch_tint.color = _color


func _toggle_add() -> void:
	if _add_panel != null:
		_add_panel.visible = not _add_panel.visible
		if _add_panel.visible and _ip_field != null:
			_ip_field.grab_focus()


func _hide_add() -> void:
	if _add_panel != null:
		_add_panel.visible = false


func _submit_add() -> void:
	var address := ""
	if _ip_field != null:
		address = _ip_field.text.strip_edges()
	if not address.is_empty():
		connect_requested.emit(address)
	_hide_add()


func _connect_to(address: String) -> void:
	connect_requested.emit(address)


func _show_info() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("show_info", _t("@join.info"))


## Closes through `MindUi` so the dialog stack and pause governor stay in sync.
func _close() -> void:
	var ui := get_node_or_null("/root/MindUi")
	if ui != null:
		ui.call("close_dialog", "join")
	else:
		hide_dialog()
